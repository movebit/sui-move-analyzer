// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::WasmConnection;
use anyhow::Result;
use crossbeam::channel::Sender;
use lsp_server::{Request, Response};
use lsp_types::Range;
use move_command_line_common::files::FileHash;
use move_compiler::{
    diagnostics::warning_filters::WarningFiltersBuilder,
    editions::{Edition, Flavor},
    shared::{files::MappedFiles, *},
};
use move_package::resolution::resolution_graph::ResolvedGraph;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use vfs::{
    impls::{memory::MemoryFS, overlay::OverlayFS, physical::PhysicalFS},
    VfsPath,
};

use crate::{
    code_lens,
    // completion::on_completion_request,
    context::Context,
    goto_definition,
    hover,
    inlay_hints,
    inlay_hints::*,
    // move_generate_spec_file::on_generate_spec_file,
    // move_generate_spec_sel::on_generate_spec_sel,
    project::ConvertLoc,
    references,
    symbols,
    utils::*,
    // linter,
};
// use move_symbol_pool::Symbol;
use url::Url;
pub type DiagnosticsBeta2024 = move_compiler::diagnostics::Diagnostics;

pub fn try_reload_projects(context: &mut Context, conn: &RefCell<WasmConnection>) {
    context.projects.try_reload_projects(conn);
}

pub fn on_request(
    context: &mut Context,
    request: &Request,
    // inlay_hints_config: &mut InlayHintsConfig
) {
    log::info!("receive method:{}", request.method.as_str());
    match request.method.as_str() {
        // lsp_types::request::Completion::METHOD => on_completion_request(context, request),
        // lsp_types::request::GotoDefinition::METHOD => {
        //     goto_definition::on_go_to_def_request(context, request);
        // }
        // lsp_types::request::GotoTypeDefinition::METHOD => {
        //     goto_definition::on_go_to_type_def_request(context, request);
        // }
        // lsp_types::request::References::METHOD => {
        //     references::on_references_request(context, request);
        // }
        // lsp_types::request::HoverRequest::METHOD => {
        //     hover::on_hover_request(context, request);
        // }
        // lsp_types::request::DocumentSymbolRequest::METHOD => {
        //     symbols::on_document_symbol_request(context, request, &context.symbols.lock().unwrap());
        // }
        // lsp_types::request::CodeLensRequest::METHOD => {
        //     code_lens::move_get_test_code_lens(context, request);
        // }
        // lsp_types::request::InlayHintRequest::METHOD => {
        //     inlay_hints::on_inlay_hints(context, request, *inlay_hints_config);
        // }
        // "move/generate/spec/file" => {
        //     on_generate_spec_file(context, request);
        // }
        // "move/generate/spec/sel" => {
        //     on_generate_spec_sel(context, request);
        // }
        // "move/lsp/client/inlay_hints/config" => {
        //     let parameters = serde_json::from_value::<InlayHintsConfig>(request.params.clone())
        //         .expect("could not deserialize inlay hints request");
        //     eprintln!("call inlay_hints config {:?}", parameters);
        //     *inlay_hints_config = parameters;
        // }
        // "runLinter" => {
        //     linter::on_run_linter(context, request);
        // }
        _ => eprintln!("handle request '{}' from client", request.method),
    }
}

pub fn on_response(_context: &Context, _response: &Response) {
    eprintln!("handle response from client");
}

type DiagSender = Arc<Mutex<Sender<(PathBuf, DiagnosticsBeta2024)>>>;

// pub fn on_notification(context: &mut Context, conn: &WasmConnection, diag_sender: DiagSender, notification: &Notification) {
//     // let (diag_sender, _)
//     //     = bounded::<(PathBuf, move_compiler::diagnostics::Diagnostics)>(1);
//     // let diag_sender = Arc::new(Mutex::new(diag_sender));
//     fn update_defs(context: &mut Context, fpath: PathBuf, content: &str) {
//         use move_compiler::parser::syntax::parse_file_string;
//         let file_hash = FileHash::new(content);
//         let mut env
//             = CompilationEnv::new(
//                 Flags::testing(),
//                 Default::default(),
//                 Default::default(),
//                 Default::default(),
//                 Some(
//                     PackageConfig {
//                         is_dependency: false,
//                         warning_filter: WarningFilters::new_for_source(),
//                         flavor: Flavor::default(),
//                         edition: Edition::E2024_BETA,
//                     },
//                 ),
//         );
//         let defs = parse_file_string(&mut env, file_hash, content, None);
//         let defs = match defs {
//             std::result::Result::Ok(x) => x,
//             std::result::Result::Err(d) => {
//                 println!("update file failed,err:{:?}", d);
//                 return;
//             }
//         };
//         let (defs, _) = defs;
//         context.projects.update_defs(fpath.clone(), defs);
//         context.ref_caches.clear();
//         context
//             .projects
//             .hash_file
//             .as_ref()
//             .borrow_mut()
//             .update(fpath.clone(), file_hash);
//         context
//             .projects
//             .file_line_mapping
//             .as_ref()
//             .borrow_mut()
//             .update(fpath, content);
//     }

//     match notification.method.as_str() {
//         lsp_types::notification::DidSaveTextDocument::METHOD => {
//             use lsp_types::DidSaveTextDocumentParams;
//             let parameters =
//                 serde_json::from_value::<DidSaveTextDocumentParams>(notification.params.clone())
//                     .expect("could not deserialize DidSaveTextDocumentParams request");
//             let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();
//             let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
//             let content = std::fs::read_to_string(fpath.as_path());
//             let content = match content {
//                 Ok(x) => x,
//                 Err(err) => {
//                     println!("read file failed,err:{:?}", err);
//                     return;
//                 }
//             };
//             log::trace!("update_defs(beta) >>");
//             update_defs(context, fpath.clone(), content.as_str());
//             make_diag(context, diag_sender, fpath);
//         }
//         lsp_types::notification::DidChangeTextDocument::METHOD => {
//             use lsp_types::DidChangeTextDocumentParams;
//             let parameters =
//                 serde_json::from_value::<DidChangeTextDocumentParams>(notification.params.clone())
//                     .expect("could not deserialize DidChangeTextDocumentParams request");
//             let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();
//             let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
//             update_defs(
//                 context,
//                 fpath,
//                 parameters.content_changes.last().unwrap().text.as_str(),
//             );
//         }

//         lsp_types::notification::DidOpenTextDocument::METHOD => {
//             use lsp_types::DidOpenTextDocumentParams;
//             let parameters =
//                 serde_json::from_value::<DidOpenTextDocumentParams>(notification.params.clone())
//                     .expect("could not deserialize DidOpenTextDocumentParams request");
//             let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();
//             let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
//             let (mani, _) = match discover_manifest_and_kind(&fpath) {
//                 Some(x) => x,
//                 None => {
//                     println!("not move project.");
//                     // send_not_project_file_error(context, fpath, true);
//                     return;
//                 }
//             };
//             match context.projects.get_project(&fpath) {
//                 Some(_) => {
//                     if let Ok(x) = std::fs::read_to_string(fpath.as_path()) {
//                         update_defs(context, fpath.clone(), x.as_str());
//                     };
//                     return;
//                 }
//                 None => {
//                     println!("project '{:?}' not found try load.", fpath.as_path());
//                 }
//             };
//             let p = match context.projects.load_project(&conn, &mani) {
//                 anyhow::Result::Ok(x) => x,
//                 anyhow::Result::Err(e) => {
//                     println!("load project failed,err:{:?}", e);
//                     return;
//                 }
//             };
//             context.projects.insert_project(p);
//             make_diag(context, diag_sender, fpath);
//         }
//         lsp_types::notification::DidCloseTextDocument::METHOD => {
//             use lsp_types::DidCloseTextDocumentParams;
//             let parameters =
//                 serde_json::from_value::<DidCloseTextDocumentParams>(notification.params.clone())
//                     .expect("could not deserialize DidCloseTextDocumentParams request");
//             let fpath = get_path_from_url(&parameters.text_document.uri).unwrap();

//             let fpath = path_concat(&std::env::current_dir().unwrap(), &fpath);
//             let (_, _) = match crate::utils::discover_manifest_and_kind(&fpath) {
//                 Some(x) => x,
//                 None => {
//                     println!("not move project.");
//                     // send_not_project_file_error(context, fpath, false);
//                     return;
//                 }
//             };
//         }

//         _ => {},
//     }
// }

fn get_package_compile_diagnostics(
    // ide_files_root: VfsPath,
    pkg_path: &Path,
) -> Result<(MappedFiles, move_compiler::diagnostics::Diagnostics)> {
    use anyhow::*;
    use move_compiler::{
        diagnostics::Diagnostics, PASS_CFGIR, PASS_HLIR, PASS_PARSER, PASS_TYPING,
    };
    use move_package::compilation::build_plan::BuildPlan;
    // use sui_move_build::implicit_deps;
    // use sui_package_management::system_package_versions::latest_system_packages;
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(PathBuf::from("/tmp")),
        default_flavor: Some(Flavor::Sui),
        skip_fetch_latest_git_deps: true,
        // implicit_dependencies: implicit_deps(latest_system_packages()),
        ..Default::default()
    };
    // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
    // vector as the writer
    // let res = std::panic::catch_unwind(|| {
    let resolution_graph =
        build_config.resolution_graph_for_package(pkg_path, None, &mut Vec::new())?;
    // });

    // match res {
    //     std::result::Result::Ok(_) => println!("No panic"),
    //     Err(e) => println!("Caught panic: {:?}", e),
    // }

    // let root_pkg_name = resolution_graph.graph.root_package_name;

    let overlay_fs_root = VfsPath::new(OverlayFS::new(&[
        VfsPath::new(MemoryFS::new()),
        MemoryFS::new().into(),
        VfsPath::new(PhysicalFS::new("/")),
    ]));

    let manifest_file = overlay_fs_root
        .join(pkg_path.to_string_lossy())
        .and_then(|p| p.join("Move.toml"))
        .and_then(|p| p.open_file());

    // Hash dependencies so we can check if something has changed.
    // let (mapped_files, deps_hash) =
    let mapped_files = compute_mapped_files(&resolution_graph, overlay_fs_root.clone());

    let build_plan =
        BuildPlan::create(&resolution_graph)?.set_compiler_vfs_root(overlay_fs_root.clone());
    let dependencies = build_plan.compute_dependencies();
    let mut diagnostics = None;
    build_plan.compile_with_driver_and_deps(dependencies, &mut std::io::sink(), |compiler| {
        // let compiler = compiler.set_ide_mode();
        let (files, compilation_result) = compiler.set_files_to_compile(None).run::<PASS_HLIR>()?;

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

        // // extract typed AST
        // let (compiler, parsed_program) = compiler.into_ast();
        // let compilation_result = compiler.at_parser(parsed_program).run::<PASS_TYPING>();
        // let compiler = match compilation_result {
        //     std::result::Result::Ok(v) => v,
        //     Err((_pass, diags)) => {
        //         let failure = true;
        //         diagnostics = Some((diags, failure));
        //         eprintln!("typed AST compilation failed");
        //         eprintln!("diagnostics: {:#?}", diagnostics);
        //         return Ok((files, vec![]));
        //     }
        // };
        // eprintln!("compiled to typed AST");

        //     // // extract typed AST
        //     // let (compiler, parsed_program) = compiler.into_ast();
        //     // let compilation_result = compiler.at_parser(parsed_program).run::<PASS_TYPING>();
        //     // let compiler = match compilation_result {
        //     //     std::result::Result::Ok(v) => v,
        //     //     Err((_pass, diags)) => {
        //     //         let failure = true;
        //     //         diagnostics = Some((diags, failure));
        //     //         eprintln!("typed AST compilation failed");
        //     //         eprintln!("diagnostics: {:#?}", diagnostics);
        //     //         return Ok((files, vec![]));
        //     //     }
        //     // };
        //     // eprintln!("compiled to typed AST");

        //     // let (compiler, typed_program) = compiler.into_ast();
        //     // eprintln!("compiling to CFGIR");
        //     // let compilation_result = compiler.at_typing(typed_program).run::<PASS_CFGIR>();
        //     // let compiler = match compilation_result {
        //     //     std::result::Result::Ok(v) => v,
        //     //     Err((_pass, diags)) => {
        //     //         let failure = false;
        //     //         diagnostics = Some((diags, failure));
        //     //         eprintln!("compilation to CFGIR failed");
        //     //         return Ok((files, vec![]));
        //     //     }
        //     // };
        //     // let failure = false;
        //     // diagnostics = Some((compiler.compilation_env().take_final_diags(), failure));
        //     // eprintln!("compiled to CFGIR");
        Ok((files, Default::default()))
    })?;

    // println!("diagnostics: {:#?}", diagnostics);
    let mut filterd_diagnostics = Diagnostics::new();
    if let Some((diags, _is_failed)) = diagnostics.clone() {
        for diag in diags.into_vec() {
            filterd_diagnostics.add(diag);
        }
    }
    Ok((mapped_files, filterd_diagnostics))
}

pub fn make_diag(context: &Context, fpath: PathBuf) {
    println!("make_diag(beta) >>");
    let (mani, _) = match crate::utils::discover_manifest_and_kind(fpath.as_path()) {
        Some(x) => x,
        None => {
            println!("manifest not found.");
            return;
        }
    };
    println!("mani:{:?}", mani);
    match context.projects.get_project(&fpath) {
        Some(x) => {
            if !x.load_ok() {
                println!("load_ok(beta) false");
                return;
            }
        }
        None => {
            println!("project not found.");
            return;
        }
    };

    let res = std::panic::catch_unwind(|| get_package_compile_diagnostics(&fpath));

    let res = match res {
        std::result::Result::Ok(a) => {
            println!("No panic");
            a
        }
        Err(e) => {
            println!("Caught panic: {:?}", e);
            return;
        }
    };

    let (mapped_file, x) = match res {
        Ok(x) => x,
        Err(e) => {
            println!("33333333333 {:?}", e);
            return;
        }
    };
    println!("start send diag {:?}", x);
    send_diag(&mapped_file, mani, x);
}

pub fn send_diag(
    mapped_files: &MappedFiles,
    _mani: PathBuf,
    x: move_compiler::diagnostics::Diagnostics,
) {
    let mut result: HashMap<Url, Vec<lsp_types::Diagnostic>> = HashMap::new();
    for x in x.into_codespan_format() {
        let (s, msg, (loc, m), _, notes) = x;
        if let Some(pos) = mapped_files.position_opt(&loc) {
            let url = url::Url::from_file_path(mapped_files.file_path(&loc.file_hash())).unwrap();
            let d = lsp_types::Diagnostic {
                range: Range {
                    start: lsp_types::Position {
                        line: pos.start.line_offset() as u32,
                        character: pos.start.column_offset() as u32,
                    },
                    end: lsp_types::Position {
                        line: pos.end.line_offset() as u32,
                        character: pos.end.column_offset() as u32,
                    },
                },
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

    crate::context::send_diag_message(result);
}
fn compute_mapped_files(resolved_graph: &ResolvedGraph, overlay_fs: VfsPath) -> MappedFiles /* , String )*/
{
    let mut mapped_files: MappedFiles = MappedFiles::empty();
    // let mut hasher = Sha256::new();
    for rpkg in resolved_graph.package_table.values() {
        for f in rpkg.get_sources(&resolved_graph.build_options).unwrap() {
            // let is_dep = rpkg.package_path != resolved_graph.graph.root_path;
            // dunce does a better job of canonicalization on Windows
            let fname = dunce::canonicalize(f.as_str())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| f.to_string());
            let mut contents = String::new();
            // there is a fair number of unwraps here but if we can't read the files
            // that by all accounts should be in the file system, then there is not much
            // we can do so it's better to fail so that we can investigate
            let vfs_file_path = overlay_fs.join(fname.as_str()).unwrap();
            let mut vfs_file = vfs_file_path.open_file().unwrap();
            let _ = vfs_file.read_to_string(&mut contents);
            let fhash = FileHash::new(&contents);
            // if is_dep {
            //     hasher.update(fhash.0);
            // }
            // write to top layer of the overlay file system so that the content
            // is immutable for the duration of compilation and symbolication
            let _ = vfs_file_path.parent().create_dir_all();
            let mut vfs_file = vfs_file_path.create_file().unwrap();
            let _ = vfs_file.write_all(contents.as_bytes());
            mapped_files.add(fhash, fname.into(), Arc::from(contents.into_boxed_str()));
        }
    }
    mapped_files /*format!("{:X}", hasher.finalize())) */
}

// pub fn read_move_toml(path: &Path) -> Option<PathBuf> {
//     let move_toml_path = path.join("Move.toml");

//     if move_toml_path.exists() {
//         // 如果存在 Move.toml 文件，则尝试读取内容并返回
//         Some(move_toml_path)
//     } else {
//         // 如果不存在 Move.toml 文件，则递归查找上一级目录
//         let parent = path.parent()?;
//         if parent != Path::new("") {
//             read_move_toml(parent)
//         } else {
//             None
//         }
//     }
// }

// pub fn test_update_defs(context: &mut Context, fpath: PathBuf, content: &str) {
//     use move_compiler::parser::syntax::parse_file_string;
//     let file_hash = FileHash::new(content);
//     let mut env = CompilationEnv::new(
//         Flags::testing(),
//         Default::default(),
//         Default::default(),
//         None,
//         Default::default(),
//         Some(PackageConfig {
//             is_dependency: false,
//             warning_filter: WarningFiltersBuilder::new_for_source(),
//             flavor: Flavor::default(),
//             edition: Edition::E2024_BETA,
//         }),
//         None,
//     );
//     let defs = parse_file_string(&mut env, file_hash, content, None);
//     let defs = match defs {
//         std::result::Result::Ok(x) => x,
//         std::result::Result::Err(d) => {
//             println!("update file failed,err:{:?}", d);
//             return;
//         }
//     };
//     // let (defs, _) = defs;
//     context.projects.update_defs(fpath.clone(), defs);
//     context.ref_caches.clear();
//     context
//         .projects
//         .hash_file
//         .as_ref()
//         .borrow_mut()
//         .update(fpath.clone(), file_hash);
//     context
//         .projects
//         .file_line_mapping
//         .as_ref()
//         .borrow_mut()
//         .update(fpath, content);
// }
