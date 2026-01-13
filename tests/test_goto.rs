// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use lsp_server::{Connection, Request, Response};
    use sui_move_analyzer::{Context, FileDiags, MultiProject, goto_definition, symbols, utils::*, test_update_defs, discover_manifest_and_kind};
    
    use serde_json::json;
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };
    // pub use url::Url;

    #[test]
    fn test_on_go_to_def_request_001() {
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
        let p = match mock_ctx.projects.load_project(&mock_ctx.connection, &mani, Default::default()) {
            anyhow::Result::Ok(x) => x,
            anyhow::Result::Err(e) => {
                log::error!("load project failed,err:{:?}", e);
                return;
            }
        };
        mock_ctx.projects.insert_project(p);

        let params_json = json!({
            "position": {
                "line": 53,  // 在main函数中s[i]处
                "character": 22   // s[i]中的s位置
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
}
