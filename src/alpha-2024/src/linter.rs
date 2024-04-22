// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    project::ConvertLoc,
    context::Context,
    utils::discover_manifest_and_kind,
};
use move_command_line_common::testing::EXP_EXT;
use move_compiler::{
    cfgir::visitor::AbstractInterpreterVisitor,
    command_line::compiler::move_check_for_errors,
    diagnostics::codes::{self, WarningFilter},
    editions::Flavor,
    expansion::ast as E,
    shared::{NumericalAddress, PackageConfig},
    typing::visitor::TypingVisitor,
    Compiler, PASS_PARSER,
    diagnostics::Diagnostics,
};
use sui_move_build::linters::{
    coin_field::CoinFieldVisitor, collection_equality::CollectionEqualityVisitor,
    custom_state_change::CustomStateChangeVerifier, freeze_wrapped::FreezeWrappedVisitor,
    known_filters, self_transfer::SelfTransferVerifier, share_owned::ShareOwnedVerifier,
    LINT_WARNING_PREFIX,
};

use std::{
    str::FromStr, str,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};
use url::Url;

use lsp_server::{Message, Notification, Request, Response, ErrorCode};
use lsp_types::{
    notification::Notification as _,
};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct ReqParameters {
    fpath: String,
}
#[derive(Clone, serde::Serialize)]
pub struct Resp {
    result_msg: String,
}

const SUI_FRAMEWORK_PATH: &str = "/data/lzw/rust_projects/sui/crates/sui-framework/packages/sui-framework";
const MOVE_STDLIB_PATH: &str = "/data/lzw/rust_projects/sui/crates/sui-framework/packages/move-stdlib";

pub fn on_run_linter(context: &Context, request: &Request) {
    log::info!("on_run_linter request = {:?}", request);
    let parameters = serde_json::from_value::<ReqParameters>(request.params.clone())
        .expect("could not deserialize go-to-def request");
    let fpath = PathBuf::from_str(parameters.fpath.as_str()).unwrap();
    // let send_err = |context: &Context, msg: String| {
    //     let r = Response::new_err(request.id.clone(), ErrorCode::UnknownErrorCode as i32, msg);
    //     context
    //         .connection
    //         .sender
    //         .send(Message::Response(r))
    //         .unwrap();
    // };
    match context.projects.get_project(&fpath) {
        Some(project) => {
            let mut target = vec![];
            if let Some((manifest_path, _)) = discover_manifest_and_kind(fpath.as_path()) {
                let d = Default::default();
                let b = project
                    .modules
                    .get(&manifest_path)
                    .unwrap_or(&d)
                    .as_ref()
                    .borrow();
                target = b.clone().sources.into_iter()
                            .map(|(path_buf, _)| path_buf.to_string_lossy().to_string())
                            .collect::<Vec<_>>()
                            .clone();
            };

            let mut working_dir = fpath.clone();
            if let Some((manifest_path, _)) = discover_manifest_and_kind(fpath.as_path()) {
                working_dir = manifest_path;
                log::info!("linter -- working_dir = {:?}", working_dir);
            }
            let mut dep: Vec<String> = project.dependents.clone();
            // let result_msg = run_sigle_file_linter(&working_dir, &fpath, &mut dep);
            let diags = match run_project_linter(&fpath, &working_dir, target, &mut dep) {
                Some(diags) => diags,
                None => return,
            };

            let mut result: HashMap<Url, Vec<lsp_types::Diagnostic>> = HashMap::new();
            let mut idx = 0;
            for (s, _, (loc, detail_str), loc_str_vec, suggest_str_vec ) 
                in diags.clone().into_codespan_format() {
                let diag_vec = diags.clone().into_vec();
                let (severity, diag_ty_str) = diag_vec[idx].info().clone().render();
                idx = idx + 1;
                if !severity.contains("Lint") {
                    continue;                    
                }
                log::info!("severity = {:?}, diag_ty_str = {:?}", severity, diag_ty_str);
                log::info!("loc = {:?}, detail_str = {:?}", loc, detail_str);
                for suggest_str in suggest_str_vec.clone() {
                    log::info!("suggest_str = {:?}", suggest_str);
                }
                
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
                            "{}\n{}",
                            diag_ty_str,
                            detail_str
                        ),
                        ..Default::default()
                    };
                    if let Some(a) = result.get_mut(&url) {
                        a.push(d);
                    } else {
                        result.insert(url, vec![d]);
                    };
                }

                for (loc2, detail_str2) in loc_str_vec {
                    if let Some(r) = context.projects.convert_loc_range(&loc2) {
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
                                diag_ty_str,
                                detail_str2,
                                if !suggest_str_vec.clone().is_empty() {
                                    format!(" {:?}", suggest_str_vec)
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
        },
        None => {
            let result_msg = run_tests(&fpath);
            let r = Response::new_ok(
                request.id.clone(),
                serde_json::to_value(Resp {
                    result_msg: match result_msg.clone() {
                        Some(result_msg) => result_msg,
                        None => "null".to_string(),
                    },
                })
                .unwrap(),
            );
            context
                .connection
                .sender
                .send(Message::Response(r))
                .unwrap();
            log::info!("result_msg = ----> \n{:?}", result_msg);
        }
    };
    
}

fn default_addresses() -> BTreeMap<String, NumericalAddress> {
    let mapping = [("std", "0x1"), ("sui", "0x2")];
    mapping
        .iter()
        .map(|(name, addr)| (name.to_string(), NumericalAddress::parse_str(addr).unwrap()))
        .collect()
}

pub fn known_filters_for_linter() -> (E::AttributeName_, Vec<WarningFilter>) {
    let (filter_attr_name, mut filters) = known_filters();

    let unused_function_code_filter = WarningFilter::code(
        Some(LINT_WARNING_PREFIX),
        codes::Category::UnusedItem as u8,
        codes::UnusedItem::Function as u8,
        Some("code_suppression_should_not_work"),
    );
    let unused_function_category_filter = WarningFilter::category(
        Some(LINT_WARNING_PREFIX),
        codes::Category::UnusedItem as u8,
        Some("category_suppression_should_not_work"),
    );
    filters.push(unused_function_code_filter);
    filters.push(unused_function_category_filter);
    (filter_attr_name, filters)
}

fn run_sigle_file_linter(working_dir: &Path, path: &Path, deps: &mut Vec<std::string::String>) -> Option<String> {
    let targets: Vec<String> = vec![path.to_str().unwrap().to_owned()];
    let lint_visitors = vec![
        ShareOwnedVerifier.visitor(),
        SelfTransferVerifier.visitor(),
        CustomStateChangeVerifier.visitor(),
        CoinFieldVisitor.visitor(),
        FreezeWrappedVisitor.visitor(),
        CollectionEqualityVisitor.visitor(),
    ];

    use tempfile::tempdir;
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        skip_fetch_latest_git_deps: true,
        ..Default::default()
    };
    let resolution_graph =
        build_config.resolution_graph_for_package(&working_dir, &mut Vec::new()).ok()?;
    let named_address_mapping: Vec<_> = resolution_graph
        .extract_named_address_mapping()
        .map(|(name, addr)| format!("{}={}", name.as_str(), addr))
        .collect();
    // log::info!("named_address_mapping = {:?}", named_address_mapping);
    use move_model::parse_addresses_from_options;
    let addrs = parse_addresses_from_options(named_address_mapping.clone()).ok()?;

    let tmp_deps = vec![MOVE_STDLIB_PATH.to_string(), SUI_FRAMEWORK_PATH.to_string()];
    if deps.is_empty() {
        deps.extend(tmp_deps);
    }
    let (filter_attr_name, filters) = known_filters_for_linter();
    let (files, comments_and_compiler_res) = Compiler::from_files(
        targets,
        deps.clone(),
        addrs,
    )
    .add_visitors(lint_visitors)
    .set_default_config(PackageConfig {
        flavor: Flavor::Sui,
        ..PackageConfig::default()
    })
    .add_custom_known_filters(filters, filter_attr_name)
    .run::<PASS_PARSER>().ok()?;

    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        move_compiler::diagnostics::report_diagnostics_to_buffer(&files, diags)
    } else {
        vec![]
    };

    let rendered_diags = std::str::from_utf8(&diag_buffer).ok()?;
    Some(rendered_diags.to_string())
}

/*
fn run_project_linter(targets: Vec<std::string::String>, deps: &Vec<std::string::String>) -> Option<String> {
    let lint_visitors = vec![
        ShareOwnedVerifier.visitor(),
        SelfTransferVerifier.visitor(),
        CustomStateChangeVerifier.visitor(),
        CoinFieldVisitor.visitor(),
        FreezeWrappedVisitor.visitor(),
        CollectionEqualityVisitor.visitor(),
    ];

    // deps: vec![MOVE_STDLIB_PATH.to_string(), SUI_FRAMEWORK_PATH.to_string()],
    let (filter_attr_name, filters) = known_filters_for_linter();
    let (files, comments_and_compiler_res) = Compiler::from_files(
        targets,
        deps.clone(),
        default_addresses(),
    )
    .add_visitors(lint_visitors)
    .set_default_config(PackageConfig {
        flavor: Flavor::Sui,
        ..PackageConfig::default()
    })
    .add_custom_known_filters(filters, filter_attr_name)
    .run::<PASS_PARSER>().ok()?;

    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        move_compiler::diagnostics::report_diagnostics_to_buffer(&files, diags)
    } else {
        vec![]
    };

    let rendered_diags = std::str::from_utf8(&diag_buffer).ok()?;
    Some(rendered_diags.to_string())
}
 */
fn run_project_linter(
    cur_file: &Path,
    working_dir: &Path, 
    targets: Vec<std::string::String>, 
    deps: &mut Vec<std::string::String>) -> Option<Diagnostics> {
    let lint_visitors = vec![
        ShareOwnedVerifier.visitor(),
        SelfTransferVerifier.visitor(),
        CustomStateChangeVerifier.visitor(),
        CoinFieldVisitor.visitor(),
        FreezeWrappedVisitor.visitor(),
        CollectionEqualityVisitor.visitor(),
    ];

    use tempfile::tempdir;
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        skip_fetch_latest_git_deps: true,
        ..Default::default()
    };
    let resolution_graph =
        build_config.resolution_graph_for_package(&working_dir, &mut Vec::new()).ok()?;
    let named_address_mapping: Vec<_> = resolution_graph
        .extract_named_address_mapping()
        .map(|(name, addr)| format!("{}={}", name.as_str(), addr))
        .collect();
    // log::info!("named_address_mapping = {:?}", named_address_mapping);
    use move_model::parse_addresses_from_options;
    let addrs = parse_addresses_from_options(named_address_mapping.clone()).ok()?;

    let tmp_deps = vec![MOVE_STDLIB_PATH.to_string(), SUI_FRAMEWORK_PATH.to_string()];
    if deps.is_empty() {
        deps.extend(tmp_deps);
    }
    let (filter_attr_name, filters) = known_filters_for_linter();
    // let (files, comments_and_compiler_res) = Compiler::from_files(
    let (_, comments_and_compiler_res) = Compiler::from_files(
        targets,
        deps.clone(),
        addrs,
    )
    .add_visitors(lint_visitors)
    .set_default_config(PackageConfig {
        flavor: Flavor::Sui,
        ..PackageConfig::default()
    })
    .add_custom_known_filters(filters, filter_attr_name)
    .run::<PASS_PARSER>().ok()?;

    // let mut current_move_file = files.clone();
    // current_move_file.clear();
    // let cur_file_str = match cur_file.to_str() {
    //     Some(s) => s,
    //     None => "",
    // };
    // for (fhash, (fname, source)) in &files {
    //     if fname.to_string().contains(cur_file_str) {
    //         current_move_file.insert(*fhash, (*fname, source.clone()));
    //     }
    // }
    Some(move_check_for_errors(comments_and_compiler_res))
}

fn run_tests(path: &Path) -> Option<String> {
    // let exp_path = path.with_extension(EXP_EXT);

    let targets: Vec<String> = vec![path.to_str().unwrap().to_owned()];
    let lint_visitors = vec![
        ShareOwnedVerifier.visitor(),
        SelfTransferVerifier.visitor(),
        CustomStateChangeVerifier.visitor(),
        CoinFieldVisitor.visitor(),
        FreezeWrappedVisitor.visitor(),
        CollectionEqualityVisitor.visitor(),
    ];
    let (filter_attr_name, filters) = known_filters_for_linter();
    let (files, comments_and_compiler_res) = Compiler::from_files(
        targets,
        vec![MOVE_STDLIB_PATH.to_string(), SUI_FRAMEWORK_PATH.to_string()],
        default_addresses(),
    )
    .add_visitors(lint_visitors)
    .set_default_config(PackageConfig {
        flavor: Flavor::Sui,
        ..PackageConfig::default()
    })
    .add_custom_known_filters(filters, filter_attr_name)
    .run::<PASS_PARSER>().ok()?;

    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        move_compiler::diagnostics::report_diagnostics_to_buffer(&files, diags)
    } else {
        vec![]
    };
    let rendered_diags = std::str::from_utf8(&diag_buffer).ok()?;
    Some(rendered_diags.to_string())
}
