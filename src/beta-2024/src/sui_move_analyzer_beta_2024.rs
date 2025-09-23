// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::path_concat;
use crate::{
    code_lens, completion::on_completion_request, context::Context, goto_definition, hover,
    inlay_hints, inlay_hints::*, linter, move_generate_spec_file::on_generate_spec_file,
    move_generate_spec_sel::on_generate_spec_sel, project::ConvertLoc, references, snap_cache,
    symbols, utils::*,
};
use anyhow::{anyhow, Result};
use crossbeam::channel::Sender;
use lsp_server::{Notification, Request, Response};
use lsp_types::{notification::Notification as _, request::Request as _};
use move_command_line_common::files::FileHash;
use move_compiler::{
    diagnostics::warning_filters::WarningFiltersBuilder,
    editions::{Edition, Flavor},
    shared::*,
};
use move_package::source_package::parsed_manifest::Dependencies;
use std::{
    collections::{BTreeSet, HashMap},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use vfs::{
    impls::{overlay::OverlayFS, physical::PhysicalFS},
    VfsPath,
};
// use move_symbol_pool::Symbol;
use url::Url;
pub type DiagnosticsBeta2024 = move_compiler::diagnostics::Diagnostics;

use once_cell::sync::Lazy;
use threadpool::ThreadPool;

// only for diag
static DIAG_THREAD_POOL: Lazy<ThreadPool> = Lazy::new(|| ThreadPool::new(2));

pub fn try_reload_projects(context: &mut Context, implicit_deps: Dependencies) {
    context
        .projects
        .try_reload_projects(&context.connection, implicit_deps);
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

pub fn on_notification(
    context: &mut Context,
    ide_files_root: VfsPath,
    diag_sender: DiagSender,
    notification: &Notification,
    implicit_deps: Dependencies,
) {
    fn update_defs(context: &mut Context, fpath: PathBuf, content: &str) {
        use move_compiler::parser::syntax::parse_file_string;
        let file_hash = FileHash::new(content);
        let mut env = CompilationEnv::new(
            Flags::testing(),
            Default::default(),
            Default::default(),
            None,
            Default::default(),
            Some(PackageConfig {
                is_dependency: false,
                warning_filter: WarningFiltersBuilder::new_for_source(),
                flavor: Flavor::default(),
                edition: Edition::E2024_BETA,
            }),
            None,
        );
        let defs = parse_file_string(&mut env, file_hash, content, None);
        let defs = match defs {
            std::result::Result::Ok(mut x) => {
                x.iter_mut().for_each(|x| match x {
                    move_compiler::parser::ast::Definition::Module(m) => {
                        m.members.extend(get_default_usedecl(file_hash));
                    }
                    _ => {}
                });
                x
            }
            std::result::Result::Err(d) => {
                log::error!("update file failed,err:{:?}", d);
                return;
            }
        };

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

    fn vfs_file_create(
        ide_files: &VfsPath,
        file_path: PathBuf,
        first_access: bool,
    ) -> Option<Box<dyn Write + Send>> {
        let Some(vfs_path) = ide_files.join(file_path.to_string_lossy()).ok() else {
            eprintln!(
                "Could not construct file path for file creation at {:?}",
                file_path
            );
            return None;
        };
        log::debug!("vfs_file_create: {:?}", vfs_path);
        if first_access {
            // create all directories on first access, otherwise file creation will fail
            if vfs_path.parent().create_dir_all().is_err() {
                eprintln!(
                    "Could not create parent directories for file at {:?}",
                    vfs_path
                );
                return None;
            };
        }
        let Some(vfs_file) = vfs_path.create_file().ok() else {
            eprintln!("Could not create file at {:?}", vfs_path);
            return None;
        };
        Some(vfs_file)
    }

    fn vfs_file_remove(ide_files: &VfsPath, file_path: PathBuf) {
        let Some(vfs_path) = ide_files.join(file_path.to_string_lossy()).ok() else {
            eprintln!(
                "Could not construct file path for file removal at {:?}",
                file_path
            );
            return;
        };
        if vfs_path.remove_file().is_err() {
            eprintln!("Could not remove file at {:?}", vfs_path);
        };
    }

    fn update_vfs_file(
        ide_files_root: &VfsPath,
        file_path: PathBuf,
        content: String,
        first_access: bool,
    ) -> Result<()> {
        let Some(mut vfs_file) = vfs_file_create(
            &ide_files_root,
            file_path.clone(),
            /* first_access */ first_access,
        ) else {
            return Err(anyhow!("Could not create vfs file {:?}", file_path));
        };

        vfs_file.write_all(content.as_bytes())?;
        vfs_file.flush()?;
        // commit writer to flush changes
        drop(vfs_file);
        Ok(())
    }

    eprintln!(
        "============== On notification: {} ==================",
        notification.method
    );
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

            match update_vfs_file(&ide_files_root, fpath.clone(), content.clone(), false) {
                Ok(_) => make_diag(
                    ide_files_root.clone(),
                    diag_sender,
                    fpath.clone(),
                    true,
                    implicit_deps.clone(),
                ),
                Err(err) => {
                    eprintln!("Could not write to vfs file for saved document, {:?}", err);
                    vfs_file_remove(&ide_files_root, fpath.clone());
                }
            }

            update_defs(context, fpath, content.as_str());
        }
        lsp_types::notification::DidChangeTextDocument::METHOD => {
            use lsp_types::DidChangeTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidChangeTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);

            let content = parameters.content_changes.last().unwrap().text.clone();
            match update_vfs_file(&ide_files_root, fpath.clone(), content, false) {
                Ok(_) => make_diag(
                    ide_files_root.clone(),
                    diag_sender,
                    fpath.clone(),
                    true,
                    implicit_deps.clone(),
                ),
                Err(err) => {
                    eprintln!(
                        "Could not update to vfs file for change document , {:?}",
                        err
                    );
                    vfs_file_remove(&ide_files_root, fpath.clone());
                }
            }

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

            let content = parameters.text_document.text.clone();
            match update_vfs_file(&ide_files_root, fpath.clone(), content.clone(), true) {
                Ok(_) => make_diag(
                    ide_files_root.clone(),
                    diag_sender,
                    fpath.clone(),
                    false,
                    implicit_deps.clone(),
                ),
                Err(err) => {
                    eprintln!("Could not update to vfs file for open document , {:?}", err);
                    vfs_file_remove(&ide_files_root, fpath.clone());
                }
            }

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

            let p = match context
                .projects
                .load_project(&context.connection, &mani, implicit_deps)
            {
                anyhow::Result::Ok(x) => x,
                anyhow::Result::Err(e) => {
                    log::error!("load project failed,err:{:?}", e);
                    return;
                }
            };
            context.projects.insert_project(p);
            update_defs(context, fpath, content.as_str());
        }
        lsp_types::notification::DidCloseTextDocument::METHOD => {
            use lsp_types::DidCloseTextDocumentParams;
            let parameters =
                serde_json::from_value::<DidCloseTextDocumentParams>(notification.params.clone())
                    .expect("could not deserialize DidCloseTextDocumentParams request");
            let fpath = parameters.text_document.uri.to_file_path().unwrap();
            vfs_file_remove(&ide_files_root, fpath.clone());
            make_diag(
                ide_files_root.clone(),
                diag_sender,
                fpath.clone(),
                false,
                implicit_deps.clone(),
            );
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

        _ => {}
    }
    eprintln!("=================\n")
}

fn get_package_compile_diagnostics(
    ide_files_root: VfsPath,
    file_path: &Path,
    file_to_diag: bool,
    implicit_deps: Dependencies,
) -> Result<move_compiler::diagnostics::Diagnostics> {
    use anyhow::*;
    use move_compiler::{diagnostics::Diagnostics, PASS_CFGIR, PASS_PARSER, PASS_TYPING};
    use move_package::compilation::build_plan::BuildPlan;
    use tempfile::tempdir;
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        default_flavor: Some(Flavor::Sui),
        skip_fetch_latest_git_deps: true,
        implicit_dependencies: implicit_deps,
        ..Default::default()
    };
    // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
    // vector as the writer
    let resolution_graph =
        build_config.resolution_graph_for_package(file_path, None, &mut Vec::new())?;

    let snapshot_top = snap_cache::ensure_snapshot_for_graph(&resolution_graph, &ide_files_root)?;
    let overlay_fs_root = VfsPath::new(OverlayFS::new(&[
        ide_files_root.clone(),
        snapshot_top,
        VfsPath::new(PhysicalFS::new("/")),
    ]));

    let build_plan =
        BuildPlan::create(&resolution_graph)?.set_compiler_vfs_root(overlay_fs_root.clone());
    let dependencies = build_plan.compute_dependencies();
    let mut diagnostics = None;
    build_plan.compile_with_driver_and_deps(dependencies, &mut std::io::sink(), |compiler| {
        let compiler = compiler.set_ide_mode();
        let (files, compilation_result) = compiler
            .set_files_to_compile(if file_to_diag {
                Some(BTreeSet::from([file_path.to_path_buf()]))
            } else {
                None
            })
            .run::<PASS_PARSER>()?;

        let compiler = match compilation_result {
            std::result::Result::Ok(v) => v,
            Err((_pass, diags)) => {
                let failure = true;
                diagnostics = Some((diags, failure));
                eprintln!("parsed AST compilation failed");
                return Ok((files, vec![]));
            }
        };
        eprintln!("compiled to parsed AST");

        // extract typed AST
        let (compiler, parsed_program) = compiler.into_ast();
        let compilation_result = compiler.at_parser(parsed_program).run::<PASS_TYPING>();
        let compiler = match compilation_result {
            std::result::Result::Ok(v) => v,
            Err((_pass, diags)) => {
                let failure = true;
                diagnostics = Some((diags, failure));
                eprintln!("typed AST compilation failed");
                eprintln!("diagnostics: {:#?}", diagnostics);
                return Ok((files, vec![]));
            }
        };
        eprintln!("compiled to typed AST");

        let (compiler, typed_program) = compiler.into_ast();
        eprintln!("compiling to CFGIR");
        let compilation_result = compiler.at_typing(typed_program).run::<PASS_CFGIR>();
        let compiler = match compilation_result {
            std::result::Result::Ok(v) => v,
            Err((_pass, diags)) => {
                let failure = false;
                diagnostics = Some((diags, failure));
                eprintln!("compilation to CFGIR failed");
                return Ok((files, vec![]));
            }
        };
        let failure = false;
        diagnostics = Some((compiler.compilation_env().take_final_diags(), failure));
        eprintln!("compiled to CFGIR");
        Ok((files, Default::default()))
    })?;

    let mut filterd_diagnostics = Diagnostics::new();
    if let Some((diags, _is_failed)) = diagnostics {
        for diag in diags.into_vec() {
            filterd_diagnostics.add(diag);
        }
    }
    Ok(filterd_diagnostics)
}

fn make_diag(
    ide_files_root: VfsPath,
    diag_sender: DiagSender,
    fpath: PathBuf,
    file_to_diag: bool,
    implicit_deps: Dependencies,
) {
    log::debug!("make_diag(beta) >>");
    let (mani, _) = match crate::utils::discover_manifest_and_kind(fpath.as_path()) {
        Some(x) => x,
        None => {
            log::info!("manifest not found.");
            return;
        }
    };

    // clone ide_files_root;
    let ide_files_root = ide_files_root.clone();
    let diag_sender = diag_sender.clone();
    let fpath = fpath.clone();

    DIAG_THREAD_POOL.execute(move || {
        let x = match get_package_compile_diagnostics(
            ide_files_root.clone(),
            &fpath,
            file_to_diag,
            implicit_deps,
        ) {
            Ok(x) => {
                log::debug!("in worker, get(beta) diags success");
                x
            }
            Err(err) => {
                log::info!("get_package_compile_diagnostics failed, err:{:?}", err);
                return;
            }
        };
        // log::info!("in worker, send(beta) diags {:?}");
        if let Err(e) = diag_sender.lock().unwrap().send((mani, x)) {
            log::info!("failed to send diag: {:?}", e);
        }
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

pub fn send_diag(context: &mut Context, mani: PathBuf, x: DiagnosticsBeta2024) {
    log::trace!("bin send_diag(beta) >>");
    let mut result: HashMap<Url, Vec<lsp_types::Diagnostic>> = HashMap::new();
    log::trace!(
        "bin send_diag(beta) x = {:?} <<",
        x.clone().into_codespan_format()
    );
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
        log::trace!(
            "bin send_diag(beta) serde_json::to_value(ds) = {:?} <<",
            serde_json::to_value(ds.clone())
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
}

pub fn read_move_toml(path: &Path) -> Option<PathBuf> {
    let move_toml_path = path.join("Move.toml");

    if move_toml_path.exists() {
        Some(move_toml_path)
    } else {
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
    let mut env = CompilationEnv::new(
        Flags::testing(),
        Default::default(),
        Default::default(),
        None,
        Default::default(),
        Some(PackageConfig {
            is_dependency: false,
            warning_filter: WarningFiltersBuilder::new_for_source(),
            flavor: Flavor::default(),
            edition: Edition::E2024_BETA,
        }),
        None,
    );
    let defs = parse_file_string(&mut env, file_hash, content, None);
    let defs = match defs {
        std::result::Result::Ok(mut x) => {
            x.iter_mut().for_each(|x| match x {
                move_compiler::parser::ast::Definition::Module(m) => {
                    m.members.extend(get_default_usedecl(file_hash));
                }
                _ => {}
            });
            x
        }
        std::result::Result::Err(d) => {
            log::error!("update file failed,err:{:?}", d);
            return;
        }
    };

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
