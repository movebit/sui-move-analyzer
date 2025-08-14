// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use crossbeam::channel::Sender;
use lsp_server::{Notification, Request, Response};
use lsp_types::{notification::Notification as _, request::Request as _};
use move_command_line_common::files::FileHash;
use move_compiler::{shared::*, PASS_HLIR};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    code_lens, completion::on_completion_request, context::Context, goto_definition, hover,
    inlay_hints, inlay_hints::*, linter, move_generate_spec_file::on_generate_spec_file,
    move_generate_spec_sel::on_generate_spec_sel, project::ConvertLoc, references, symbols,
    utils::*,
};
use url::Url;

pub type DiagnosticsAlpha2024 = move_compiler::diagnostics::Diagnostics;

pub fn try_reload_projects(context: &mut Context) {
    context.projects.try_reload_projects(&context.connection);
}

pub fn on_request(
    context: &mut Context,
    request: &Request,
    inlay_hints_config: &mut InlayHintsConfig,
) {
    match request.method.as_str() {
        lsp_types::request::Completion::METHOD => on_completion_request(context, request),
        lsp_types::request::GotoDefinition::METHOD => {
            goto_definition::on_go_to_def_request(context, request);
        }
        lsp_types::request::GotoTypeDefinition::METHOD => {
            goto_definition::on_go_to_type_def_request(context, request);
        }
        lsp_types::request::References::METHOD => {
            references::on_references_request(context, request);
        }
        lsp_types::request::HoverRequest::METHOD => {
            hover::on_hover_request(context, request);
        }
        lsp_types::request::DocumentSymbolRequest::METHOD => {
            symbols::on_document_symbol_request(context, request, &context.symbols.lock().unwrap());
        }
        lsp_types::request::CodeLensRequest::METHOD => {
            code_lens::move_get_test_code_lens(context, request);
        }
        lsp_types::request::InlayHintRequest::METHOD => {
            inlay_hints::on_inlay_hints(context, request, *inlay_hints_config);
        }
        "move/generate/spec/file" => {
            on_generate_spec_file(context, request);
        }
        "move/generate/spec/sel" => {
            on_generate_spec_sel(context, request);
        }
        "move/lsp/client/inlay_hints/config" => {
            let parameters = serde_json::from_value::<InlayHintsConfig>(request.params.clone())
                .expect("could not deserialize inlay hints request");
            eprintln!("call inlay_hints config {:?}", parameters);
            *inlay_hints_config = parameters;
        }
        "runLinter" => {
            // linter::on_run_linter(context, request);
        }
        _ => eprintln!("handle request '{}' from client", request.method),
    }
}

pub fn on_response(_context: &Context, _response: &Response) {
    eprintln!("handle response from client");
}

type DiagSender = Arc<Mutex<Sender<(PathBuf, DiagnosticsAlpha2024)>>>;

pub fn on_notification(
    context: &mut Context,
    diag_sender: DiagSender,
    notification: &Notification,
) {
    // let (diag_sender, _)
    //     = bounded::<(PathBuf, move_compiler::diagnostics::Diagnostics)>(1);
    // let diag_sender = Arc::new(Mutex::new(diag_sender));

    fn update_defs(context: &mut Context, fpath: PathBuf, content: &str) {
        use crate::syntax::parse_file_string;
        let file_hash = FileHash::new(content);
        let mut env = CompilationEnv::new(
            Flags::testing(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let defs = parse_file_string(&mut env, file_hash, content);
        let defs = match defs {
            std::result::Result::Ok(x) => x,
            std::result::Result::Err(d) => {
                log::error!("update file failed,err:{:?}", d);
                return;
            }
        };
        let (defs, _) = defs;
        context.projects.update_defs(fpath.clone(), defs);
        context.ref_caches.clear();
        context
            .projects
            .hash_file
            .as_ref()
            .borrow_mut()
            .update(fpath.clone(), file_hash);
        context
            .projects
            .file_line_mapping
            .as_ref()
            .borrow_mut()
            .update(fpath, content);
    }

    match notification.method.as_str() {
        lsp_types::notification::DidSaveTextDocument::METHOD => {
            use lsp_types::DidSaveTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidSaveTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidSaveTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
            let content = std::fs::read_to_string(fpath.as_path());
            let content = match content {
                Ok(x) => x,
                Err(err) => {
                    log::error!("read file failed,err:{:?}", err);
                    return;
                }
            };
            update_defs(context, fpath.clone(), content.as_str());
            make_diag(context, diag_sender, fpath);
        }
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            use lsp_types::DidChangeTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidChangeTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
            update_defs(
                context,
                fpath,
                parameters.content_changes.last().unwrap().text.as_str(),
            );
        }

        lsp_types::notification::DidOpenTextDocument::METHOD => {
            use lsp_types::DidOpenTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidOpenTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidOpenTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
            let (mani, _) = match discover_manifest_and_kind(&fpath) {
                Some(x) => x,
                None => {
                    log::error!("not move project.");
                    send_not_project_file_error(context, fpath, true);
                    return;
                }
            };
            match context.projects.get_project(&fpath) {
                Some(_) => {
                    if let Ok(x) = std::fs::read_to_string(fpath.as_path()) {
                        update_defs(context, fpath.clone(), x.as_str());
                    };
                    return;
                }
                None => {
                    eprintln!("project '{:?}' not found try load.", fpath.as_path());
                }
            };
            let p = match context.projects.load_project(&context.connection, &mani) {
                anyhow::Result::Ok(x) => x,
                anyhow::Result::Err(e) => {
                    log::error!("load project failed,err:{:?}", e);
                    return;
                }
            };
            context.projects.insert_project(p);
            make_diag(context, diag_sender, fpath);
        }
        lsp_types::notification::DidCloseTextDocument::METHOD => {
            use lsp_types::DidCloseTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidCloseTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidCloseTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
            let (_, _) = match discover_manifest_and_kind(&fpath) {
                Some(x) => x,
                None => {
                    log::error!("not move project.");
                    send_not_project_file_error(context, fpath, false);
                    return;
                }
            };
        }

        _ => {}
    }
}

fn get_package_compile_diagnostics(
    pkg_path: &Path,
) -> Result<move_compiler::diagnostics::Diagnostics> {
    use anyhow::*;
    use move_package::compilation::build_plan::BuildPlan;
    use tempfile::tempdir;
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        skip_fetch_latest_git_deps: true,
        ..Default::default()
    };
    // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
    // vector as the writer
    let resolution_graph = build_config.resolution_graph_for_package(pkg_path, &mut Vec::new())?;
    let build_plan = BuildPlan::create(resolution_graph)?;
    let mut diagnostics = None;
    build_plan.compile_with_driver(&mut std::io::sink(), |compiler| {
        let (_, compilation_result) = compiler.run::<PASS_HLIR>()?;
        match compilation_result {
            std::result::Result::Ok(_) => {}
            std::result::Result::Err(diags) => {
                eprintln!("get_package_compile_diagnostics compilate failed");
                diagnostics = Some(diags);
            }
        };
        Ok(Default::default())
    })?;
    match diagnostics {
        Some(x) => Ok(x),
        None => Ok(Default::default()),
    }
}

fn make_diag(context: &Context, diag_sender: DiagSender, fpath: PathBuf) {
    let (mani, _) = match crate::utils::discover_manifest_and_kind(fpath.as_path()) {
        Some(x) => x,
        None => {
            log::error!("manifest not found.");
            return;
        }
    };
    match context.projects.get_project(&fpath) {
        Some(x) => {
            if !x.load_ok() {
                log::info!("load_ok(alpha) false");
                return;
            }
        }
        None => return,
    };
    std::thread::spawn(move || {
        log::info!("in new thread, about get_package_compile_diagnostics(alpha)");
        let x = match get_package_compile_diagnostics(mani.as_path()) {
            Ok(x) => x,
            Err(err) => {
                log::error!("get_package_compile_diagnostics failed,err:{:?}", err);
                return;
            }
        };
        diag_sender.lock().unwrap().send((mani, x)).unwrap();
    });
}

fn send_not_project_file_error(context: &mut Context, fpath: PathBuf, is_open: bool) {
    let url = url::Url::from_file_path(fpath.as_path()).unwrap();
    let content = std::fs::read_to_string(fpath.as_path()).unwrap_or_else(|_| "".to_string());
    let lines: Vec<_> = content.lines().collect();
    let last_line = lines.len();
    let last_col = lines.last().map(|x| (*x).len()).unwrap_or(1);
    let ds = lsp_types::PublishDiagnosticsParams::new(
        url,
        if is_open {
            vec![lsp_types::Diagnostic {
                range: lsp_types::Range {
                    start: lsp_types::Position {
                        line: 0,
                        character: 0,
                    },
                    end: lsp_types::Position {
                        line: last_line as u32,
                        character: last_col as u32,
                    },
                },
                message: "This file doesn't belong to a move project.\nMaybe a build artifact???"
                    .to_string(),
                ..Default::default()
            }]
        } else {
            vec![]
        },
        None,
    );
    context
        .connection
        .sender
        .send(lsp_server::Message::Notification(Notification {
            method: lsp_types::notification::PublishDiagnostics::METHOD.to_string(),
            params: serde_json::to_value(ds).unwrap(),
        }))
        .unwrap();
}

pub fn send_diag(context: &mut Context, mani: PathBuf, x: DiagnosticsAlpha2024) {
    let mut result: HashMap<Url, Vec<lsp_types::Diagnostic>> = HashMap::new();
    for x in x.into_codespan_format() {
        let (s, msg, (loc, m), _, notes) = x;
        if let Some(r) = context.projects.convert_loc_range(&loc) {
            let url = url::Url::from_file_path(r.path.as_path()).unwrap();
            let d = lsp_types::Diagnostic {
                range: r.mk_location().range,
                severity: Some(match s {
                    codespan_reporting::diagnostic::Severity::Bug => {
                        lsp_types::DiagnosticSeverity::ERROR
                    }
                    codespan_reporting::diagnostic::Severity::Error => {
                        lsp_types::DiagnosticSeverity::ERROR
                    }
                    codespan_reporting::diagnostic::Severity::Warning => {
                        lsp_types::DiagnosticSeverity::WARNING
                    }
                    codespan_reporting::diagnostic::Severity::Note => {
                        lsp_types::DiagnosticSeverity::INFORMATION
                    }
                    codespan_reporting::diagnostic::Severity::Help => {
                        lsp_types::DiagnosticSeverity::HINT
                    }
                }),
                message: format!(
                    "{}\n{}{:?}",
                    msg,
                    m,
                    if !notes.is_empty() {
                        format!(" {:?}", notes)
                    } else {
                        "".to_string()
                    }
                ),
                ..Default::default()
            };
            if let Some(a) = result.get_mut(&url) {
                a.push(d);
            } else {
                result.insert(url, vec![d]);
            };
        }
    }
    // update version.
    for (k, v) in result.iter() {
        context.diag_version.update(&mani, k, v.len());
    }
    context.diag_version.with_manifest(&mani, |x| {
        for (old, v) in x.iter() {
            if !result.contains_key(old) && *v > 0 {
                result.insert(old.clone(), vec![]);
            }
        }
    });
    for (k, x) in result.iter() {
        if x.is_empty() {
            context.diag_version.update(&mani, k, 0);
        }
    }
    for (k, v) in result.into_iter() {
        let ds = lsp_types::PublishDiagnosticsParams::new(k.clone(), v, None);
        context
            .connection
            .sender
            .send(lsp_server::Message::Notification(Notification {
                method: lsp_types::notification::PublishDiagnostics::METHOD.to_string(),
                params: serde_json::to_value(ds).unwrap(),
            }))
            .unwrap();
    }
}
