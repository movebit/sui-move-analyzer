// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use crossbeam::channel::{bounded, select};
use log::{Level, Metadata, Record};
use lsp_server::{Connection, Message};
use lsp_types::{
    CompletionOptions, GlobPattern, HoverProviderCapability, OneOf, SaveOptions,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TypeDefinitionProviderCapability, WorkDoneProgressOptions, notification::Notification,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use sui_move_analyzer::{
    context::{Context, FileDiags, MultiProject},
    symbols,
};

use vfs::{VfsPath, impls::memory::MemoryFS};

use sui_move_analyzer::sui_move_analyzer::{
    Diagnostics, on_notification, on_request, send_diag, try_reload_projects,
};

pub(crate) struct ContextManager<'a> {
    pub context: Context<'a>,
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

fn init_context_manager(connection: &lsp_server::Connection) -> ContextManager<'_> {
    let symbols = Arc::new(Mutex::new(symbols::Symbolicator::empty_symbols()));
    let context = Context {
        projects: MultiProject::new(),
        connection: &connection,
        // files: VirtualFileSystem::default(),
        symbols: symbols.clone(),
        ref_caches: Default::default(),
        diag_version: FileDiags::new(),
    };

    let context_manager = ContextManager {
        context,
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
    registere_did_change_watched_files(&context_manager);

    let (diag_sender, diag_receiver) = bounded::<(PathBuf, Diagnostics)>(1);

    let diag_sender = Arc::new(Mutex::new(diag_sender));

    let mut inlay_hints_config = sui_move_analyzer::inlay_hints::InlayHintsConfig::default();
    let ide_files_root: VfsPath = MemoryFS::new().into();

    let implicit_deps = sui_move_analyzer::implicit_deps();
    loop {
        select! {
            recv(diag_receiver) -> message => {
                match message {
                    Ok ((mani ,x)) => {
                        send_diag(&mut context_manager.context, mani, x);
                    }
                    Err(error) => log::error!("IDE diag message error: {:?}", error),
                }
            },
            recv(context_manager.connection.receiver) -> message => {

                match message {
                    Ok(Message::Request(request)) =>{
                        try_reload_projects(&mut context_manager.context, implicit_deps.clone());
                        on_request(&mut context_manager.context, &request, &mut inlay_hints_config);
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
                                on_notification(&mut context_manager.context,ide_files_root.clone(), diag_sender.clone(), &notification, implicit_deps.clone());
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
                change_notifications: Some(OneOf::Left(true)), // Can also use string ID
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
        definition_provider: Some(OneOf::Left(symbols::DEFS_AND_REFS_SUPPORT)),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(
            symbols::DEFS_AND_REFS_SUPPORT,
        )),
        references_provider: Some(OneOf::Left(symbols::DEFS_AND_REFS_SUPPORT)),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace: Some(generate_workspace_server_capabilities()),
        ..Default::default()
    }
}

fn registere_did_change_watched_files(context_manager: &ContextManager) {
    use lsp_server::{Message, Request};
    use lsp_types::{
        DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, Registration,
        RegistrationParams,
    };
    let watchers = vec![
        FileSystemWatcher {
            glob_pattern: GlobPattern::String("**/*.move".into()),
            kind: None,
        },
        FileSystemWatcher {
            glob_pattern: GlobPattern::String("**/Move.toml".into()),
            kind: None,
        },
    ];

    let options = DidChangeWatchedFilesRegistrationOptions { watchers };
    let reg = Registration {
        id: "file-watcher-1".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(serde_json::to_value(options).unwrap()),
    };

    let params = RegistrationParams {
        registrations: vec![reg],
    };

    let request = Request::new(
        lsp_server::RequestId::from("reg-1".to_string()),
        "client/registerCapability".to_string(),
        params,
    );

    let Err(e) = context_manager
        .connection
        .sender
        .send(Message::Request(request))
    else {
        eprintln!("Registered workspace/didChangeWatchedFiles");
        return;
    };

    eprintln!(
        "Registered workspace/didChangeWatchedFiles Failed. err info: {:?}",
        e
    );
}
