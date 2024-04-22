// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::context::*;
use super::item::*;
use super::utils::*;
use lsp_server::*;
use lsp_types::CodeLens;
use lsp_types::CodeLensParams;
use lsp_types::Command;
use move_compiler::shared::Identifier;

pub fn move_get_test_code_lens(context: &Context, request: &lsp_server::Request) {
    let parameters = serde_json::from_value::<CodeLensParams>(request.params.clone())
        .expect("could not deserialize  CodeLensParams request");
    let fpath = parameters.text_document.uri.to_file_path().unwrap();
    let fpath = path_concat(
        std::env::current_dir().unwrap().as_path(),
        fpath.as_path(),
    );
    let _send_err = |msg: String| {
        let r = Response::new_err(request.id.clone(), ErrorCode::UnknownErrorCode as i32, msg);
        context
            .connection
            .sender
            .send(Message::Response(r))
            .unwrap();
    };
    let mut v = TestHandler::new();
    let _ = match context.projects.get_project(&fpath) {
        Some(p) => p,
        None => return,
    }
    .run_visitor_for_file(&mut v, &fpath, false);
    let r = Response::new_ok(request.id.clone(), serde_json::to_value(v.result).unwrap());
    context
        .connection
        .sender
        .send(Message::Response(r))
        .unwrap();
}

#[derive(Default)]
pub struct TestHandler {
    result: Vec<CodeLens>,
}
impl TestHandler {
    fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Display for TestHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "move test code lens")
    }
}

impl super::project::ItemOrAccessHandler for TestHandler {
    fn handle_item_or_access(
        &mut self,
        services: &dyn crate::project::HandleItemService,
        _project_context: &crate::project_context::ProjectContext,
        item: &crate::item::ItemOrAccess,
    ) {
        let push = |v: &mut TestHandler, name: &str, range: FileRange| {
            let (manifest_dir, _) = discover_manifest_and_kind(range.path.as_path()).unwrap();
            v.result.push(CodeLens {
                range: range.mk_location().range,
                command: Some(Command::new(
                    "▶︎ Run Test".to_string(),
                    "sui-move-analyzer.sui.test_ui".to_string(),
                    Some({
                        let mut x = vec![serde_json::Value::String(
                            manifest_dir.to_str().unwrap().to_string(),
                        )];
                        x.push(serde_json::Value::String(name.to_string()));
                        x
                    }),
                )),
                data: None,
            });
        };
        match item {
            ItemOrAccess::Item(x) => match x {
                Item::ModuleName(ItemModuleName { name, is_test }) => {
                    if *is_test {
                        if let Some(range) = services.convert_loc_range(&name.loc()) {
                            push(self, name.0.value.as_str(), range);
                        }
                    }
                }
                Item::Fun(f) => {
                    if f.is_test == AttrTest::Test {
                        if let Some(range) = services.convert_loc_range(&f.name.loc()) {
                            push(self, f.name.0.value.as_str(), range);
                        }
                    }
                }
                _ => {}
            },
            ItemOrAccess::Access(_) => {}
        }
    }

    fn function_or_spec_body_should_visit(&self, _range: &FileRange) -> bool {
        false
    }

    fn visit_fun_or_spec_body(&self) -> bool {
        false
    }

    fn finished(&self) -> bool {
        false
    }
}
