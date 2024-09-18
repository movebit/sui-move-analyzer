// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use lsp_server::{Connection, Request, Response};
    use beta_2024::{
        context::{Context, FileDiags, MultiProject},
        goto_definition, symbols,
        utils::*,
        vfs::VirtualFileSystem,
        sui_move_analyzer_beta_2024::*,
    };
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
            files: VirtualFileSystem::default(),
            symbols,
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        let fpath = path_concat(
            std::env::current_dir().unwrap().as_path(),
            PathBuf::from("tests/symbols/sources/M1.move").as_path(),
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
        let p = match mock_ctx.projects.load_project(&mock_ctx.connection, &mani) {
            anyhow::Result::Ok(x) => x,
            anyhow::Result::Err(e) => {
                log::error!("load project failed,err:{:?}", e);
                return;
            }
        };
        mock_ctx.projects.insert_project(p);

        let params_json = json!({
            "position": {
                "line": 25,
                "character": 27
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
                        "character":32,
                        "line":6
                    },
                    "start":{
                        "character":15,
                        "line":6
                    }
                },
                "uri": ("file:///".to_string() + path_concat(
                            std::env::current_dir().unwrap().as_path(),
                            PathBuf::from("tests/symbols/sources/M2.move").as_path()).to_str().unwrap()
                       ).replace('\\', "/")
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
