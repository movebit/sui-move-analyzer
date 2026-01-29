// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use lsp_server::{Connection, Request, Response};
    use sui_move_analyzer::{
        Context, FileDiags, MultiProject, discover_manifest_and_kind, goto_definition, symbols,
        test_update_defs, utils::*,
    };

    use serde_json::json;
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };

    #[test]
    fn test_index_syntax() {
        let (connection, _) = Connection::stdio();
        let symbols = Arc::new(Mutex::new(symbols::Symbolicator::empty_symbols()));

        let mut mock_ctx = Context {
            projects: MultiProject::new(),
            connection: &connection,
            symbols,
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/beta_2024/project1/sources/index_syntax.move").as_path(),
        );

        eprintln!("fpath = {:?}", fpath.to_str());
        let (mani, _) = match discover_manifest_and_kind(&fpath) {
            Some(x) => x,
            None => {
                log::error!("not move project.");
                return;
            }
        };
        match mock_ctx.projects.get_project(&fpath) {
            Some(_) => {
                if let Ok(x) = std::fs::read_to_string(fpath.as_path()) {
                    test_update_defs(&mut mock_ctx, fpath.clone(), x.as_str());
                };
                return;
            }
            None => {
                eprintln!("project '{:?}' not found try load.", fpath.as_path());
            }
        };
        let p =
            match mock_ctx
                .projects
                .load_project(&mock_ctx.connection, &mani, Default::default())
            {
                anyhow::Result::Ok(x) => x,
                anyhow::Result::Err(e) => {
                    log::error!("load project failed,err:{:?}", e);
                    return;
                }
            };
        mock_ctx.projects.insert_project(p);

        let params_json = json!({
            "position": {
                "line": 53,  // In the main function at s[i]
                "character": 22   // The position of i in s[i]
            },
            "textDocument": {
                "uri": "file:///".to_string() + fpath.to_str().unwrap()
            },
        });
        let request = Request {
            id: "go_to_def_request_001".to_string().into(),
            method: String::from("textDocument/definition"),
            params: params_json,
        };

        let actual_r = goto_definition::on_go_to_def_request(&mock_ctx, &request);
        let expect_r = Response::new_ok(
            "go_to_def_request_001".to_string().into(),
            json!([{
                "range":{
                    "end":{
                        "character":17,
                        "line":50
                    },
                    "start":{
                        "character":16,
                        "line":50
                    }
                },
                "uri": ("file://".to_string() + fpath.to_str().unwrap()).replace('\\', "/")
            }]),
        );
        std::thread::sleep(Duration::new(1, 0));
        eprintln!("\n------------------------------\n");
        eprintln!("actual_r = {:?}", actual_r);
        eprintln!("\n");
        eprintln!("expect_r = {:?}", expect_r);
        eprintln!("\n------------------------------\n");
        assert_eq!(actual_r.result, expect_r.result);
    }

    #[test]
    fn test_duplicate_fun_name_goto() {
        let (connection, _) = Connection::stdio();
        let symbols = Arc::new(Mutex::new(symbols::Symbolicator::empty_symbols()));

        let mut mock_ctx = Context {
            projects: MultiProject::new(),
            connection: &connection,
            symbols,
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/bugfix/sources/test_duplicate_fun_name.move").as_path(),
        );

        eprintln!("fpath = {:?}", fpath.to_str());
        let (mani, _) = match discover_manifest_and_kind(&fpath) {
            Some(x) => x,
            None => {
                log::error!("not move project.");
                return;
            }
        };
        match mock_ctx.projects.get_project(&fpath) {
            Some(_) => {
                if let Ok(x) = std::fs::read_to_string(fpath.as_path()) {
                    test_update_defs(&mut mock_ctx, fpath.clone(), x.as_str());
                };
                return;
            }
            None => {
                eprintln!("project '{:?}' not found try load.", fpath.as_path());
            }
        };
        let p =
            match mock_ctx
                .projects
                .load_project(&mock_ctx.connection, &mani, Default::default())
            {
                anyhow::Result::Ok(x) => x,
                anyhow::Result::Err(e) => {
                    log::error!("load project failed,err:{:?}", e);
                    return;
                }
            };
        mock_ctx.projects.insert_project(p);

        let params_json = json!({
            "position": {
                "line": 17,
                "character": 38
            },
            "textDocument": {
                "uri": "file:///".to_string() + fpath.to_str().unwrap()
            },
        });
        let request = Request {
            id: "go_to_def_request_002".to_string().into(),
            method: String::from("textDocument/definition"),
            params: params_json,
        };

        let actual_r = goto_definition::on_go_to_def_request(&mock_ctx, &request);
        let expect_r = Response::new_ok(
            "go_to_def_request_002".to_string().into(),
            json!([{
                "range":{
                    "end":{
                        "character":19,
                        "line":7
                    },
                    "start":{
                        "character":15,
                        "line":7
                    }
                },
                "uri": ("file://".to_string() + fpath.to_str().unwrap()).replace('\\', "/")
            }]),
        );
        std::thread::sleep(Duration::new(1, 0));
        eprintln!("\n------------------------------\n");
        eprintln!("actual_r = {:?}", actual_r);
        eprintln!("\n");
        eprintln!("expect_r = {:?}", expect_r);
        eprintln!("\n------------------------------\n");
        assert_eq!(actual_r.result, expect_r.result);
    }

    #[test]
    fn test_deref_vector_goto() {
        let (connection, _) = Connection::stdio();
        let symbols = Arc::new(Mutex::new(symbols::Symbolicator::empty_symbols()));

        let mut mock_ctx = Context {
            projects: MultiProject::new(),
            connection: &connection,
            symbols,
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/bugfix/sources/test_deref_vector.move").as_path(),
        );

        eprintln!("fpath = {:?}", fpath.to_str());
        let (mani, _) = match discover_manifest_and_kind(&fpath) {
            Some(x) => x,
            None => {
                log::error!("not move project.");
                return;
            }
        };
        match mock_ctx.projects.get_project(&fpath) {
            Some(_) => {
                if let Ok(x) = std::fs::read_to_string(fpath.as_path()) {
                    test_update_defs(&mut mock_ctx, fpath.clone(), x.as_str());
                };
                return;
            }
            None => {
                eprintln!("project '{:?}' not found try load.", fpath.as_path());
            }
        };
        let deps = sui_move_analyzer::implicit_deps();
        eprintln!("DEBUG: implicit_deps len: {}", deps.len());
        for (k, v) in deps.iter() {
            eprintln!("DEBUG: dep: {:?} -> {:?}", k, v);
        }
        let p = match mock_ctx
            .projects
            .load_project(&mock_ctx.connection, &mani, deps)
        {
            anyhow::Result::Ok(x) => x,
            anyhow::Result::Err(e) => {
                log::error!("load project failed,err:{:?}", e);
                return;
            }
        };
        mock_ctx.projects.insert_project(p);

        let params_json = json!({
            "position": {
                "line": 14,
                "character": 42
            },
            "textDocument": {
                "uri": "file:///".to_string() + fpath.to_str().unwrap()
            },
        });
        let request = Request {
            id: "go_to_def_request_003".to_string().into(),
            method: String::from("textDocument/definition"),
            params: params_json,
        };

        let actual_r = goto_definition::on_go_to_def_request(&mock_ctx, &request);
        let target_fpath = PathBuf::from(
            "/Users/edy/.move/https___github_com_MystenLabs_sui_git_mainnet/crates/sui-framework/packages/move-stdlib/sources/vector.move",
        );
        let expect_r = Response::new_ok(
            "go_to_def_request_003".to_string().into(),
            json!([{
                "range":{
                    "end":{
                        "character":24,
                        "line":37
                    },
                    "start":{
                        "character":18,
                        "line":37
                    }
                },
                "uri": ("file://".to_string() + target_fpath.to_str().unwrap()).replace('\\', "/")
            }]),
        );
        std::thread::sleep(Duration::new(1, 0));
        eprintln!("\n------------------------------\n");
        eprintln!("actual_r = {:?}", actual_r);
        eprintln!("\n");
        eprintln!("expect_r = {:?}", expect_r);
        eprintln!("\n------------------------------\n");
        assert_eq!(actual_r.result, expect_r.result);
    }

    #[test]
    fn test_method_syntax1() {
        let (connection, _) = Connection::stdio();
        let symbols = Arc::new(Mutex::new(symbols::Symbolicator::empty_symbols()));

        let mut mock_ctx = Context {
            projects: MultiProject::new(),
            connection: &connection,
            symbols,
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/beta_2024/project2/sources/method_syntax.move").as_path(),
        );

        eprintln!("fpath = {:?}", fpath.to_str());
        let (mani, _) = match discover_manifest_and_kind(&fpath) {
            Some(x) => x,
            None => {
                log::error!("not move project.");
                return;
            }
        };
        match mock_ctx.projects.get_project(&fpath) {
            Some(_) => {
                if let Ok(x) = std::fs::read_to_string(fpath.as_path()) {
                    test_update_defs(&mut mock_ctx, fpath.clone(), x.as_str());
                };
                return;
            }
            None => {
                eprintln!("project '{:?}' not found try load.", fpath.as_path());
            }
        };
        let deps = sui_move_analyzer::implicit_deps();
        let p = match mock_ctx
            .projects
            .load_project(&mock_ctx.connection, &mani, deps)
        {
            anyhow::Result::Ok(x) => x,
            anyhow::Result::Err(e) => {
                log::error!("load project failed,err:{:?}", e);
                return;
            }
        };
        mock_ctx.projects.insert_project(p);

        let params_json = json!({
            "position": {
                "line": 50,
                "character": 22
            },
            "textDocument": {
                "uri": "file:///".to_string() + fpath.to_str().unwrap()
            },
        });
        let request = Request {
            id: "go_to_def_request_004".to_string().into(),
            method: String::from("textDocument/definition"),
            params: params_json,
        };

        let actual_r = goto_definition::on_go_to_def_request(&mock_ctx, &request);
        let expect_r = Response::new_ok(
            "go_to_def_request_004".to_string().into(),
            json!([{
                "range":{
                    "end":{
                        "character":29,
                        "line":15
                    },
                    "start":{
                        "character":15,
                        "line":15
                    }
                },
                "uri": ("file://".to_string() + fpath.to_str().unwrap()).replace('\\', "/")
            }]),
        );
        std::thread::sleep(Duration::new(1, 0));
        eprintln!("\n------------------------------\n");
        eprintln!("actual_r = {:?}", actual_r);
        eprintln!("\n");
        eprintln!("expect_r = {:?}", expect_r);
        eprintln!("\n------------------------------\n");
        assert_eq!(actual_r.result, expect_r.result);
    }

    #[test]
    fn test_issue14_macro_goto() {
        let (connection, _) = Connection::stdio();
        let symbols = Arc::new(Mutex::new(symbols::Symbolicator::empty_symbols()));

        let mut mock_ctx = Context {
            projects: MultiProject::new(),
            connection: &connection,
            symbols,
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/bugfix/sources/issue14.move").as_path(),
        );

        let (mani, _) = discover_manifest_and_kind(&fpath).expect("not move project.");
        let implicit_deps = sui_move_analyzer::implicit_deps();
        let p = mock_ctx
            .projects
            .load_project(&mock_ctx.connection, &mani, implicit_deps)
            .expect("load project failed");
        mock_ctx.projects.insert_project(p);

        // Position of "do_mut!" in issue14.move:201
        let params_json = json!({
            "position": {
                "line": 200,
                "character": 25
            },
            "textDocument": {
                "uri": "file:///".to_string() + fpath.to_str().unwrap()
            },
        });
        let request = Request {
            id: "issue14_goto_def".to_string().into(),
            method: String::from("textDocument/definition"),
            params: params_json,
        };

        let actual_r = goto_definition::on_go_to_def_request(&mock_ctx, &request);
        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/bugfix/sources/keyed_big_vector.move").as_path(),
        );
        let expect_r = Response::new_ok(
            "issue14_goto_def".to_string().into(),
            json!([{
                "range":{
                    "end":{
                        "character":27,
                        "line":356
                    },
                    "start":{
                        "character":21,
                        "line":356
                    }
                },
                "uri": ("file://".to_string() + fpath.to_str().unwrap()).replace('\\', "/")
            }]),
        );
        eprintln!("\n------------------------------\n");
        eprintln!("actual_r = {:?}", actual_r);
        eprintln!("\n");
        eprintln!("expect_r = {:?}", expect_r);
        eprintln!("\n------------------------------\n");
        assert_eq!(actual_r.result, expect_r.result);
    }

    #[test]
    fn test_issue14_macro_body_hover() {
        let (connection, _) = Connection::stdio();
        let symbols = Arc::new(Mutex::new(symbols::Symbolicator::empty_symbols()));

        let mut mock_ctx = Context {
            projects: MultiProject::new(),
            connection: &connection,
            symbols,
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/bugfix/sources/issue14.move").as_path(),
        );

        let (mani, _) = discover_manifest_and_kind(&fpath).expect("not move project.");
        let p = mock_ctx
            .projects
            .load_project(&mock_ctx.connection, &mani, Default::default())
            .expect("load project failed");
        mock_ctx.projects.insert_project(p);

        // Position of "lp_user_share" inside the closure at line 203
        let params_json = json!({
            "position": {
                "line": 202,
                "character": 15
            },
            "textDocument": {
                "uri": "file:///".to_string() + fpath.to_str().unwrap()
            },
        });
        let request = Request {
            id: "issue14_hover".to_string().into(),
            method: String::from("textDocument/hover"),
            params: params_json,
        };

        use sui_move_analyzer::hover;
        let actual_r = hover::on_hover_request(&mut mock_ctx, &request);
        eprint!("\n------------------------------\n");
        eprintln!("actual_r = {:?}", actual_r);
        eprint!("\n------------------------------\n");
        assert!(actual_r.result.is_some());
    }
}
