// use anyhow::Result;
use clap::Parser;
use crossbeam::channel::{bounded, select};
use log::{Level, Metadata, Record};
use lsp_server::{Connection, Message};
use lsp_types::{
    CompletionOptions, HoverProviderCapability, OneOf, SaveOptions, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TypeDefinitionProviderCapability,
    WorkDoneProgressOptions, notification::Notification,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use beta_2024::{
    context::{
        Context as Context_beta_2024, FileDiags as FileDiags_beta_2024,
        MultiProject as MultiProject_beta_2024,
    },
    symbols as symbols_beta_2024,
};

use vfs::{VfsPath, impls::memory::MemoryFS};
// use url::Url;

use beta_2024::sui_move_analyzer_beta_2024::{
    DiagnosticsBeta2024,
    // on_response as on_response_beta_2024
    on_notification as on_notification_beta_2024,
    on_request as on_request_beta_2024,
    send_diag as send_diag_beta_2024,
    try_reload_projects as try_reload_projects_beta_2024,
};

pub(crate) struct ContextManager<'a> {
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
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .unwrap()
}

fn init_context_manager(connection: &lsp_server::Connection) -> ContextManager {
    let symbols_beta24 = Arc::new(Mutex::new(symbols_beta_2024::Symbolicator::empty_symbols()));
    let context_beta_2024 = Context_beta_2024 {
        projects: MultiProject_beta_2024::new(),
        connection: &connection,
        // files: VirtualFileSystem_beta_2024::default(),
        symbols: symbols_beta24.clone(),
        ref_caches: Default::default(),
        diag_version: FileDiags_beta_2024::new(),
    };

    let context_manager = ContextManager {
        context_beta_2024,
        connection,
    };
    context_manager
}

fn main() {
    // #[cfg(feature = "pprof")]
    // cpu_pprof(20);

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

    context_manager
        .connection
        .initialize_finish(
            id,
            serde_json::json!({
                "capabilities": serde_json::to_value(get_lsp_capabilities()).expect("could not serialize lsp_capabilities."),
            }),
        )
        .expect("could not finish connection initialization");

    let (diag_sender_beta2024, diag_receiver_beta_2024) =
        bounded::<(PathBuf, DiagnosticsBeta2024)>(1);

    let diag_sender_beta2024 = Arc::new(Mutex::new(diag_sender_beta2024));

    let mut inlay_hints_config_beta_2024 = beta_2024::inlay_hints::InlayHintsConfig::default();
    let ide_files_root: VfsPath = MemoryFS::new().into();

    let implicit_deps = beta_2024::implicit_deps();
    loop {
        select! {
            recv(diag_receiver_beta_2024) -> message => {
                match message {
                    Ok ((mani ,x)) => {
                        send_diag_beta_2024(&mut context_manager.context_beta_2024, mani, x);
                    }
                    Err(error) => log::error!("beta IDE diag message error: {:?}", error),
                }
            },
            recv(context_manager.connection.receiver) -> message => {

                match message {
                    Ok(Message::Request(request)) =>{
                        try_reload_projects_beta_2024(&mut context_manager.context_beta_2024, implicit_deps.clone());
                        on_request_beta_2024(&mut context_manager.context_beta_2024, &request, &mut inlay_hints_config_beta_2024);
                    }
                    Ok(Message::Response(r)) => {eprintln!("Message::Response: {:?}", r);},
                    Ok(Message::Notification(notification)) => {
                        match notification.method.as_str() {
                            lsp_types::notification::Exit::METHOD => break,
                            lsp_types::notification::Cancel::METHOD => {
                                // TODO: Currently the server does not implement request cancellation.
                                // It ought to, especially once it begins processing requests that may
                                // take a long time to respond to.
                            }
                            _ => {
                                on_notification_beta_2024(&mut context_manager.context_beta_2024,ide_files_root.clone(), diag_sender_beta2024.clone(), &notification, implicit_deps.clone());
                            }
                        }
                    }
                    Err(error) => eprintln!("IDE message error: {:?}", error),
                }
            }
        };
    }

    io_threads.join().expect("I/O threads could not finish");
    eprintln!("Shut down language server '{}'.", exe);
}

fn get_lsp_capabilities() -> lsp_types::ServerCapabilities {
    fn generate_workspace_server_capabilities() -> lsp_types::WorkspaceServerCapabilities {
        use lsp_types::{
            FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
            FileOperationRegistrationOptions, WorkspaceFileOperationsServerCapabilities,
        };

        let filters = vec![
            FileOperationFilter {
                scheme: Some("file".into()),
                pattern: FileOperationPattern {
                    glob: "**/*.move".into(),
                    matches: Some(FileOperationPatternKind::File),
                    options: None,
                },
            },
            FileOperationFilter {
                scheme: Some("file".into()),
                pattern: FileOperationPattern {
                    glob: "**/Move.toml".into(),
                    matches: Some(FileOperationPatternKind::File),
                    options: None,
                },
            },
        ];

        let did_create = FileOperationRegistrationOptions {
            filters: filters.clone(),
        };
        let did_delete = FileOperationRegistrationOptions {
            filters: filters.clone(),
        };
        let did_rename = FileOperationRegistrationOptions {
            filters: filters.clone(),
        };

        lsp_types::WorkspaceServerCapabilities {
            workspace_folders: Some(lsp_types::WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(OneOf::Left(true)), // 也可以用字符串 ID
            }),
            file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                did_create: Some(did_create),
                did_delete: Some(did_delete),
                did_rename: Some(did_rename),
                will_create: None,
                will_delete: None,
                will_rename: None,
            }),
        }
    }

    lsp_types::ServerCapabilities {
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
        workspace: Some(generate_workspace_server_capabilities()),
        ..Default::default()
    }
}
