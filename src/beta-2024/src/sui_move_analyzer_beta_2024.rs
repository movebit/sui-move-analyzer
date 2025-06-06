// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use crossbeam::channel::Sender;
use lsp_server::{Notification, Request, Response};
use lsp_types::{
    notification::Notification as _, request::Request as _,
};
use move_command_line_common::files::FileHash;
use move_compiler::{diagnostics::WarningFilters, editions::{Edition, Flavor}, shared::*};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use crate::utils::path_concat;

use crate::{
    code_lens,
    completion::on_completion_request,

    context::Context,
    goto_definition, hover, inlay_hints, inlay_hints::*,
    move_generate_spec_file::on_generate_spec_file,
    move_generate_spec_sel::on_generate_spec_sel,
    project::ConvertLoc,
    references, symbols,
    utils::*,
    linter,
};
// use move_symbol_pool::Symbol;
use url::Url;
pub type DiagnosticsBeta2024 = move_compiler::diagnostics::Diagnostics;

pub fn try_reload_projects(context: &mut Context) {
    context.projects.try_reload_projects(&context.connection);
}

pub fn on_request(context: &mut Context, request: &Request, inlay_hints_config: &mut InlayHintsConfig) {
    log::info!("receive method:{}", request.method.as_str());
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
            linter::on_run_linter(context, request);
        }
        _ => eprintln!("handle request '{}' from client", request.method),
    }
}

pub fn on_response(_context: &Context, _response: &Response) {
    eprintln!("handle response from client");
}

type DiagSender = Arc<Mutex<Sender<(PathBuf, DiagnosticsBeta2024)>>>;

pub fn on_notification(context: &mut Context, diag_sender: DiagSender, notification: &Notification) {
    // let (diag_sender, _) 
    //     = bounded::<(PathBuf, move_compiler::diagnostics::Diagnostics)>(1);
    // let diag_sender = Arc::new(Mutex::new(diag_sender));
    fn update_defs(context: &mut Context, fpath: PathBuf, content: &str) {
        use move_compiler::parser::syntax::parse_file_string;
        let file_hash = FileHash::new(content);
        let mut env 
            = CompilationEnv::new(
                Flags::testing(),
                Default::default(), 
                Default::default(),
                Default::default(),
                Some(
                    PackageConfig {
                        is_dependency: false,
                        warning_filter: WarningFilters::new_for_source(),
                        flavor: Flavor::default(),
                        edition: Edition::E2024_BETA,
                    },
                ),
        );
        let defs = parse_file_string(&mut env, file_hash, content, None);
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
            let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();
            let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
            let content = std::fs::read_to_string(fpath.as_path());
            let content = match content {
                Ok(x) => x,
                Err(err) => {
                    log::error!("read file failed,err:{:?}", err);
                    return;
                }
            };
            log::trace!("update_defs(beta) >>");
            update_defs(context, fpath.clone(), content.as_str());
            make_diag(context, diag_sender, fpath);
        }
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            use lsp_types::DidChangeTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidChangeTextDocumentParams request");
            let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();
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
            let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();
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
            let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();

            let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
            let (_, _) = match crate::utils::discover_manifest_and_kind(&fpath) {
                Some(x) => x,
                None => {
                    log::error!("not move project.");
                    send_not_project_file_error(context, fpath, false);
                    return;
                }
            };
        }

        _ => {},
    }
}

fn get_package_compile_diagnostics(
    pkg_path: &Path,
) -> Result<move_compiler::diagnostics::Diagnostics> {
    let file_content = std::fs::read_to_string(pkg_path).unwrap_or_else(|_| panic!("'{:?}' can't read_to_string", pkg_path));
    let file_hash = FileHash::new(file_content.as_str());
    let mut env = CompilationEnv::new(
        Flags::testing(), 
        Default::default(), 
        Default::default(), 
        Default::default(),
        Some(
            PackageConfig {
                is_dependency: false,
                warning_filter: WarningFilters::new_for_source(),
                flavor: Flavor::default(),
                edition: Edition::E2024_BETA
            }
            
        ),
    );

    if let Err(diags) = move_compiler::parser::syntax::parse_file_string(&mut env, file_hash, file_content.as_str(), None) {
        return Ok(diags);
    } else {
        eprintln!("parse_file_string not has diag");
    }
    return Ok(Default::default());

    // use anyhow::*;
    // use move_package::compilation::build_plan::BuildPlan;
    // use tempfile::tempdir;
    // let build_config = move_package::BuildConfig {
    //     test_mode: true,
    //     install_dir: Some(tempdir().unwrap().path().to_path_buf()),
    //     skip_fetch_latest_git_deps: true,
    //     ..Default::default()
    // };
    // // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
    // // vector as the writer
    // let resolution_graph = build_config.resolution_graph_for_package(pkg_path, &mut Vec::new())?;
    // let build_plan = BuildPlan::create(resolution_graph)?;
    // let mut diagnostics = None;
    // build_plan.compile_with_driver(&mut std::io::sink(), |compiler| {
    //     let (_, compilation_result) = compiler.run::<PASS_NAMING>()?;
    //     match compilation_result {
    //         std::result::Result::Ok(_) => {
    //             eprintln!("get_package_compile_diagnostics compilate success");
    //         }
    //         std::result::Result::Err(diags) => {
    //             eprintln!("get_package_compile_diagnostics compilate failed");
    //             diagnostics = Some(diags);
    //         }
    //     };
    //     Ok(Default::default())
    // })?;
    
    // let mut filterd_diagnostics = Diagnostics::new();
    // if let Some(x) = diagnostics.clone() {
    //     for diag in x.1.into_vec() {
    //         eprintln!("diag primary_msg: {}", diag.primary_msg());
    //         let diag_info = diag.info();
    //         eprintln!("     diag info msg: {}", diag_info.message());
    //         if !diag_info.message().contains("feature is not supported in specified edition")
    //             && !diag_info.message().contains("unbound type") {
    //             filterd_diagnostics.add(diag);
    //         }
    //     }
    // }
    // Ok(filterd_diagnostics)


    // match diagnostics {
    //     Some(x) => Ok(x.1),
    //     None => Ok(Default::default()),
    // }
}

fn make_diag(context: &Context, diag_sender: DiagSender, fpath: PathBuf) {
    log::trace!("make_diag(beta) >>");
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
                log::trace!("load_ok(beta) false");
                return;
            }
        }
        None => return,
    };
    std::thread::spawn(move || {
        log::trace!("in new thread, about get_package_compile_diagnostics(beta)");
        let x = match get_package_compile_diagnostics(&fpath) {
            Ok(x) => {
                log::trace!("in new thread, get(beta) diags success");
                x
            },
            Err(err) => {
                log::error!("get_package_compile_diagnostics failed,err:{:?}", err);
                return;
            }
        };
        log::trace!("in new thread, send(beta) diags");
        diag_sender.lock().unwrap().send((mani, x)).unwrap();
    });
}

fn send_not_project_file_error(context: &mut Context, fpath: PathBuf, is_open: bool) {
    // let url = url::Url::from_file_path(fpath.as_path()).unwrap();
    let url = get_url_from_path(fpath.as_path()).unwrap();
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

pub fn send_diag(context: &mut Context, mani: PathBuf, x: DiagnosticsBeta2024) {
    log::trace!("bin send_diag(beta) >>");
    let mut result: HashMap<Url, Vec<lsp_types::Diagnostic>> = HashMap::new();
    log::trace!("bin send_diag(beta) x = {:?} <<", x.clone().into_codespan_format());
    for x in x.into_codespan_format() {
        let (s, msg, (loc, m), _, notes) = x;
        if let Some(r) = context.projects.convert_loc_range(&loc) {
            let url = get_url_from_path(r.path.as_path()).unwrap();
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
        log::trace!("bin send_diag(beta) serde_json::to_value(ds) = {:?} <<", serde_json::to_value(ds.clone()));
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


pub fn read_move_toml(path: &Path) -> Option<PathBuf> {
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

pub fn test_update_defs(context: &mut Context, fpath: PathBuf, content: &str) {
    use move_compiler::parser::syntax::parse_file_string;
    let file_hash = FileHash::new(content);
    let mut env 
        = CompilationEnv::new(
            Flags::testing(),
            Default::default(), 
            Default::default(),
            Default::default(),
            Some(
                PackageConfig {
                    is_dependency: false,
                    warning_filter: WarningFilters::new_for_source(),
                    flavor: Flavor::default(),
                    edition: Edition::E2024_BETA,
                },
            ),
    );
    let defs = parse_file_string(&mut env, file_hash, content, None);
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
