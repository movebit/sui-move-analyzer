use anyhow::Result;
use clap::Parser;
use crossbeam::channel::{bounded, select, Sender};
use log::{Level, Metadata, Record};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    notification::Notification as _, request::{GotoTypeDefinition, Request as _}, CodeLensParams, CompletionOptions, CompletionParams, Diagnostic, DocumentSymbolParams, GotoDefinitionParams, HoverParams, HoverProviderCapability, InlayHintParams, OneOf, ReferenceParams, SaveOptions, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, TypeDefinitionProviderCapability, WorkDoneProgressOptions
};

use move_compiler_beta_2024::diagnostics::Diagnostics as Diagnostics_beta_2024;
use move_compiler_alpha_2024::diagnostics::Diagnostics as Diagnostics_alpha_2024;
use std::{
    collections::{BTreeMap, HashMap}, fs::File, io::{BufRead, BufReader, Read}, path::{Path, PathBuf}, str::FromStr, sync::{Arc, Mutex}, thread
};
// use move_symbol_pool_alpha_2024::symbol::Symbol;

use beta_2024::{
    context::{
        self, Context as Context_beta_2024, FileDiags as FileDiags_beta_2024, MultiProject as MultiProject_beta_2024
    }, 
    // inlay_hints::InlayHintsConfig, 
    symbols as symbols_beta_2024, 
    vfs::VirtualFileSystem as VirtualFileSystem_beta_2024
};

use move_package::source_package::manifest_parser::parse_move_manifest_from_file;
use url::Url;


use beta_2024::sui_move_analyzer_beta_2024::{
    send_diag as send_diag_beta_2024,
    try_reload_projects as try_reload_projects_beta_2024,
    on_request as on_request_beta_2024,
    on_notification as on_notification_beta_2024,
    on_response as on_response_beta_2024
};

use alpha_2024::{
    context::{
        FileDiags as FileDiags_alpha_2024,
        Context as Context_alpha_2024, 
        MultiProject as MultiProject_alpha_2024
    },
    inlay_hints::InlayHintsConfig, 
    symbols as symbols_alpha_2024, 
    symbols::SymbolicatorRunner as SymbolicatorRunner_alpha_2024,
    vfs::VirtualFileSystem as VirtualFileSystem_alpha_2024,
    sui_move_analyzer_alpha_2024::{
        on_notification as on_notification_alpha_2024,
        on_request as on_request_alpha_2024, 
        on_response as on_response_alpha_2024, 
        send_diag as send_diag_alpha_2024, 
        try_reload_projects as try_reload_projects_alpha_2024
    }
};

struct ContextManager<'a> {
    pub context_alpha_2024: Context_alpha_2024<'a>,
    pub context_beta_2024: Context_beta_2024<'a>,
    pub connection: &'a lsp_server::Connection,
}

#[derive(Parser)]
#[clap(author, version, about)]
struct Options {}

struct SimpleLogger;
impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{} - {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}
const LOGGER: SimpleLogger = SimpleLogger;

pub fn init_log() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Warn))
        .unwrap()
}

pub fn init_context_manager(connection: &lsp_server::Connection) -> ContextManager {
    let symbols = Arc::new(Mutex::new(symbols_alpha_2024::Symbolicator::empty_symbols()));
    let mut context_alpha_2024 = Context_alpha_2024 {
        projects: MultiProject_alpha_2024::new(),
        connection: &connection,
        files: VirtualFileSystem_alpha_2024::default(),
        symbols: symbols.clone(),
        ref_caches: Default::default(),
        diag_version: FileDiags_alpha_2024::new(),
    };

    let symbols = Arc::new(Mutex::new(symbols_beta_2024::Symbolicator::empty_symbols()));
    let mut context_beta_2024 = Context_beta_2024 {
        projects: MultiProject_beta_2024::new(),
        connection: &connection,
        files: VirtualFileSystem_beta_2024::default(),
        symbols: symbols.clone(),
        ref_caches: Default::default(),
        diag_version: FileDiags_beta_2024::new(),
    };

    let context_manager = ContextManager {
        context_alpha_2024,
        context_beta_2024,
        connection
    };
    context_manager
}



fn main() {
    #[cfg(feature = "pprof")]
    cpu_pprof(20);

    // For now, sui-move-analyzer only responds to options built-in to clap,
    // such as `--help` or `--version`.
    Options::parse();
    init_log();
    // stdio is used to communicate Language Server Protocol requests and responses.
    // stderr is used for logging (and, when Visual Studio Code is used to communicate with this
    // server, it captures this output in a dedicated "output channel").
    let exe = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .to_string();
    eprintln!(
        "Starting language server '{}' communicating via stdio...",
        exe
    );

    let (connection, io_threads) = Connection::stdio();
    let mut context_manager = init_context_manager(&connection);

    let (id, _client_response) = context_manager
        .connection
        .initialize_start()
        .expect("could not start connection initialization");

    let capabilities = serde_json::to_value(lsp_types::ServerCapabilities {
        // The server receives notifications from the client as users open, close,
        // and modify documents.
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                // TODO: We request that the language server client send us the entire text of any
                // files that are modified. We ought to use the "incremental" sync kind, which would
                // have clients only send us what has changed and where, thereby requiring far less
                // data be sent "over the wire." However, to do so, our language server would need
                // to be capable of applying deltas to its view of the client's open files. See the
                // 'sui_move_analyzer::vfs' module for details.
                change: Some(TextDocumentSyncKind::FULL),
                will_save: None,
                will_save_wait_until: None,
                save: Some(
                    SaveOptions {
                        include_text: Some(true),
                    }
                    .into(),
                ),
            },
        )),
        selection_range_provider: None,
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        // The server provides completions as a user is typing.
        completion_provider: Some(CompletionOptions {
            resolve_provider: None,
            // In Move, `foo::` and `foo.` should trigger completion suggestions for after
            // the `:` or `.`
            // (Trigger characters are just that: characters, such as `:`, and not sequences of
            // characters, such as `::`. So when the language server encounters a completion
            // request, it checks whether completions are being requested for `foo:`, and returns no
            // completions in that case.)
            trigger_characters: Some(vec![":".to_string(), ".".to_string()]),
            all_commit_characters: None,
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: None,
            },
            completion_item: None,
        }),
        definition_provider: Some(OneOf::Left(symbols_beta_2024::DEFS_AND_REFS_SUPPORT)),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(
            symbols_beta_2024::DEFS_AND_REFS_SUPPORT,
        )),
        references_provider: Some(OneOf::Left(symbols_beta_2024::DEFS_AND_REFS_SUPPORT)),
        document_symbol_provider: Some(OneOf::Left(true)),
        ..Default::default()
    })
    .expect("could not serialize server capabilities");

    let symbols = Arc::new(Mutex::new(symbols_beta_2024::Symbolicator::empty_symbols()));
    let (diag_sender_symbol, diag_receiver_symbol) =
        bounded::<Result<BTreeMap<move_symbol_pool_beta_2024::symbol::Symbol, Vec<lsp_types::Diagnostic>>>>(0);
    let mut symbolicator_runner = symbols_beta_2024::SymbolicatorRunner::idle();
    if symbols_alpha_2024::DEFS_AND_REFS_SUPPORT {
        let initialize_params: lsp_types::InitializeParams =
            serde_json::from_value(_client_response)
                .expect("could not deserialize client capabilities");

        symbolicator_runner = symbols_beta_2024::SymbolicatorRunner::new(symbols.clone(), diag_sender_symbol);

        // If initialization information from the client contains a path to the directory being
        // opened, try to initialize symbols before sending response to the client. Do not bother
        // with diagnostics as they will be recomputed whenever the first source file is opened. The
        // main reason for this is to enable unit tests that rely on the symbolication information
        // to be available right after the client is initialized.
        if let Some(uri) = initialize_params.root_uri {
            if let Some(p) = symbols_alpha_2024::SymbolicatorRunner::root_dir(&uri.to_file_path().unwrap()) {
                // need to evaluate in a separate thread to allow for a larger stack size (needed on
                // Windows)
                thread::Builder::new()
                    .stack_size(symbols_alpha_2024::STACK_SIZE_BYTES)
                    .spawn(move || {
                        if let Ok((Some(new_symbols), _)) =
                        symbols_beta_2024::Symbolicator::get_symbols(p.as_path())
                        {
                            let mut old_symbols = symbols.lock().unwrap();
                            (*old_symbols).merge(new_symbols);
                        }
                    })
                    .unwrap()
                    .join()
                    .unwrap();
            }
        }
    };

    context_manager
        .connection
        .initialize_finish(
            id,
            serde_json::json!({
                "capabilities": capabilities,
            }),
        )
        .expect("could not finish connection initialization");

    let (_, diag_receiver_beta_2024) 
        = bounded::<(PathBuf, Diagnostics_beta_2024)>(1);
        
    
    let mut inlay_hints_config_beta_2024 = beta_2024::inlay_hints::InlayHintsConfig::default();
    let mut inlay_hints_config_alpha_2024 = alpha_2024::inlay_hints::InlayHintsConfig::default();


    loop {
        select! {
            // recv(diag_receiver_beta_2024) -> message => {
            //     match message {
            //         Ok ((mani ,x)) => {
            //             send_diag_beta_2024(&mut context_manager.context_beta_2024, mani, x);
            //         }
            //         Err(error) => log::error!("beta IDE diag message error: {:?}", error),
            //     }
            // },
            // recv(diag_receiver_alpha_2024) -> message => {
            //     match message {
            //         Ok ((mani ,x)) => {
            //             // send_diag_alpha_2024(&mut context_manager.context_alpha_2024,mani, x);
            //         }
            //         Err(error) => log::error!("beta IDE diag message error: {:?}", error),
            //     }
            // },
            
            recv(diag_receiver_symbol) -> message => {
                match message {
                    Ok(result) => {
                        match result {
                            Ok(diags) => {
                                for (k, v) in diags {
                                    let url = Url::from_file_path(Path::new(&k.to_string())).unwrap();
                                    let params = lsp_types::PublishDiagnosticsParams::new(url, v, None);
                                    let notification = Notification::new(lsp_types::notification::PublishDiagnostics::METHOD.to_string(), params);
                                    if let Err(err) = context_manager
                                        .connection
                                        .sender
                                        .send(lsp_server::Message::Notification(notification)) {
                                            eprintln!("could not send diagnostics response: {:?}", err);
                                        };
                                }
                            },
                            Err(err) => {
                                let typ = lsp_types::MessageType::ERROR;
                                let message = format!("{err}");
                                    // report missing manifest only once to avoid re-generating
                                    // user-visible error in cases when the developer decides to
                                    // keep editing a file that does not belong to a packages
                                let params = lsp_types::ShowMessageParams { typ, message };
                                let notification = Notification::new(lsp_types::notification::ShowMessage::METHOD.to_string(), params);
                                if let Err(err) = context_manager
                                    .connection
                                    .sender
                                    .send(lsp_server::Message::Notification(notification)) {
                                        eprintln!("could not send compiler error response: {:?}", err);
                                    };
                            },
                        }
                    },
                    Err(error) => eprintln!("symbolicator message error: {:?}", error),
                }
            },
            recv(context_manager.connection.receiver) -> message => {
                
                match message {
                    Ok(Message::Request(request)) =>{
                        let version = get_compiler_version_from_requsets(&request);
                        if version == "alpha_2024" {
                            try_reload_projects_alpha_2024(&mut context_manager.context_alpha_2024);
                            on_request_alpha_2024(&mut context_manager.context_alpha_2024, &request , &mut inlay_hints_config_alpha_2024);
                        } else if version == "beta_2024" {
                            try_reload_projects_beta_2024(&mut context_manager.context_beta_2024);
                            on_request_beta_2024(&mut context_manager.context_beta_2024, &request, &mut inlay_hints_config_beta_2024);
                        } else {
                            eprintln!("On_Request Error: could not parse compiler version from Move.toml. Error version {:?}", version);
                        }
                    }
                    Ok(Message::Response(response)) => on_response_alpha_2024(&context_manager.context_alpha_2024, &response),
                    Ok(Message::Notification(notification)) => {
                        let version = get_compiler_version_from_notification(&notification);
                        match notification.method.as_str() {
                            lsp_types::notification::Exit::METHOD => break,
                            lsp_types::notification::Cancel::METHOD => {
                                // TODO: Currently the server does not implement request cancellation.
                                // It ought to, especially once it begins processing requests that may
                                // take a long time to respond to.
                            }
                            _ => {
                                if version == "alpha_2024" {
                                    on_notification_alpha_2024(&mut context_manager.context_alpha_2024, &notification);
                                } else if version == "beta_2024" {
                                    on_notification_beta_2024(&mut context_manager.context_beta_2024, &notification);
                                } else {
                                    eprintln!("On_Notification Error: could not parse compiler version from Move.toml. Error version {:?}", version);
                                }
                            }
                        }
                    }
                    Err(error) => eprintln!("IDE message error: {:?}", error),
                }
            }
        };
    }

    io_threads.join().expect("I/O threads could not finish");
    symbolicator_runner.quit();
    eprintln!("Shut down language server '{}'.", exe);
}

fn read_move_toml(path: &Path) -> Option<PathBuf> {
    let move_toml_path = path.join("Move.toml");

    if move_toml_path.exists() {
        // 如果存在 Move.toml 文件，则尝试读取内容并返回
        Some(move_toml_path)
    } else {
        // 如果不存在 Move.toml 文件，则递归查找上一级目录
        let parent = path.parent()?;
        if parent != Path::new("") {
            read_move_toml(parent)
        } else {
            None
        }
    }
}

pub fn get_compiler_version_from_requsets( request: &Request) -> String {

    let file = match get_file_pathbuf_from_requsets(&request) {
        Some(fpath) => {
            if let Some(x) = fpath.parent() {
                match read_move_toml(x) {
                    Some(file) => {
                        file
                    }
                    None => return String::from("beta_2024")
                }
            } else {
                return String::from("beta_2024");
            }
            
        }
        None => { return String::from("beta_2024")}
    };

    let tv = parse_move_manifest_from_file(&file);
    match tv {
        Ok(x) => {
            if let Some(edition) = x.package.edition {
                if edition.edition.as_str() == "2024" {
                    if let Some(release) = edition.release {
                        if release.as_str() == "alpha" {
                            return String::from("alpha_2024");
                        }
                    } 
                } 
            }
        }
        Err(_) => return String::from("beta_2024"),
    }
    
    return String::from("beta_2024");
    
}

pub fn get_file_pathbuf_from_requsets(request: &Request) -> Option<PathBuf> {
    match request.method.as_str() {
        lsp_types::request::Completion::METHOD => {
            let parameters = serde_json::from_value::<CompletionParams>(request.params.clone())
                .expect("could not deserialize references request");
            
            Some(
                parameters
                .text_document_position
                .text_document
                .uri
                .to_file_path()
                .unwrap())
        },
        lsp_types::request::GotoDefinition::METHOD => {
            let parameters = serde_json::from_value::<GotoDefinitionParams>(request.params.clone())
                .expect("could not deserialize go-to-def request");

            Some(parameters
                .text_document_position_params
                .text_document
                .uri
                .to_file_path()
                .unwrap()
            )
        }
        lsp_types::request::GotoTypeDefinition::METHOD => {
            let parameters = serde_json::from_value::<GotoDefinitionParams>(request.params.clone())
                .expect("could not deserialize go-to-def request");
            Some(parameters
                .text_document_position_params
                .text_document
                .uri
                .to_file_path()
                .unwrap()
            )
        }
        lsp_types::request::References::METHOD => {
            let parameters = serde_json::from_value::<ReferenceParams>(request.params.clone())
                .expect("could not deserialize references request");
            Some(parameters
                .text_document_position
                .text_document
                .uri
                .to_file_path()
                .unwrap()
            )
        }
        lsp_types::request::HoverRequest::METHOD => {
            let parameters = serde_json::from_value::<HoverParams>(request.params.clone())
                .expect("could not deserialize hover request");
            Some(parameters
                .text_document_position_params
                .text_document
                .uri
                .to_file_path()
                .unwrap()
            )
        }
        lsp_types::request::DocumentSymbolRequest::METHOD => {
            let parameters = serde_json::from_value::<DocumentSymbolParams>(request.params.clone())
                .expect("could not deserialize document symbol request");
            Some(parameters.text_document.uri.to_file_path().unwrap())
        }
        lsp_types::request::CodeLensRequest::METHOD => {
            let parameters = serde_json::from_value::<CodeLensParams>(request.params.clone())
                .expect("could not deserialize  CodeLensParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            Some(beta_2024::utils::path_concat(
                std::env::current_dir().unwrap().as_path(),
                fpath.as_path(),
            ))
        }
        lsp_types::request::InlayHintRequest::METHOD => {
            let parameters = serde_json::from_value::<InlayHintParams>(request.params.clone())
                .expect("could not deserialize go-to-def request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            Some(beta_2024::utils::path_concat(
                std::env::current_dir().unwrap().as_path(),
                fpath.as_path(),
            ))
        }
        "move/generate/spec/file" => {
            use alpha_2024::move_generate_spec_file::ReqParameters;
            let parameters = serde_json::from_value::<ReqParameters>(request.params.clone())
                .expect("could not deserialize go-to-def request");
            let fpath = parameters.fpath;
            Some(
                PathBuf::from_str(
                    fpath.as_str()
                ).unwrap_or_default()
            )
        }
        "move/generate/spec/sel" => {
            use beta_2024::move_generate_spec_sel::ReqParameters;
            let parameters = serde_json::from_value::<ReqParameters>(request.params.clone())
                .expect("could not deserialize go-to-def request");
            let fpath = parameters.fpath;
            Some(
                PathBuf::from_str(
                    fpath.as_str()
                ).unwrap_or_default()
            )
        }
        "move/lsp/client/inlay_hints/config" => {None}
        "runLinter" => {
            use beta_2024::linter::ReqParameters;
            let parameters = serde_json::from_value::<ReqParameters>(request.params.clone())
                .expect("could not deserialize go-to-def request");
            let fpath = parameters.fpath;

            Some(
                PathBuf::from_str(
                    fpath.as_str()
                ).unwrap_or_default()
            )
        }
        _ => None,
    }
}

pub fn get_compiler_version_from_notification( notification: &Notification) -> String {

    let file = match get_file_pathbuf_from_notification(&notification) {
        Some(fpath) => {
            if let Some(x) = fpath.parent() {
                match read_move_toml(x) {
                    Some(file) => {
                        file
                    }
                    None => return String::from("beta_2024")
                }
            } else {
                return String::from("beta_2024");
            }
            
        }
        None => { return String::from("beta_2024")}
    };

    let tv = parse_move_manifest_from_file(&file);
    match tv {
        Ok(x) => {
            if let Some(edition) = x.package.edition {
                if edition.edition.as_str() == "2024" {
                    if let Some(release) = edition.release {
                        if release.as_str() == "alpha" {
                            return String::from("alpha_2024");
                        }
                    }
                } 
                
            }
        }
        Err(_) => return String::from("beta_2024"),
    }
    return String::from("beta_2024");
    
}

pub fn get_file_pathbuf_from_notification(notification: &Notification) -> Option<PathBuf> {
    match notification.method.as_str() {
        lsp_types::notification::DidSaveTextDocument::METHOD => {
            use lsp_types::DidSaveTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidSaveTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidSaveTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = beta_2024::utils::path_concat(&std::env::current_dir().unwrap(), &fpath);
            Some(fpath)
        }
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            use lsp_types::DidChangeTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidChangeTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = beta_2024::utils::path_concat(&std::env::current_dir().unwrap(), &fpath);
            Some(fpath)
        }
        lsp_types::notification::DidOpenTextDocument::METHOD => {
            use lsp_types::DidOpenTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidOpenTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidOpenTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = beta_2024::utils::path_concat(&std::env::current_dir().unwrap(), &fpath);
            Some(fpath)
        }
        lsp_types::notification::DidCloseTextDocument::METHOD => {
            use lsp_types::DidCloseTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidCloseTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidCloseTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = beta_2024::utils::path_concat(&std::env::current_dir().unwrap(), &fpath);
            Some(fpath)
        }

        _ => None,
    }
}