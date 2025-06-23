// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[macro_use(sp)]
extern crate move_ir_types;

#[macro_export]
macro_rules! impl_convert_loc {
    ($struct_name : ident) => {
        impl ConvertLoc for $struct_name {
            fn convert_file_hash_filepath(&self, hash: &FileHash) -> Option<PathBuf> {
                self.hash_file
                    .as_ref()
                    .borrow()
                    .get_path(hash)
                    .map(|x| x.clone())
            }
            fn convert_loc_range(&self, loc: &Loc) -> Option<FileRange> {
                self.convert_file_hash_filepath(&loc.file_hash())
                    .map(|file| {
                        self.file_line_mapping.as_ref().borrow().translate(
                            &file,
                            loc.start(),
                            loc.end(),
                        )
                    })
                    .flatten()
            }
        }
    };
}

pub mod completion;
pub mod context;
pub mod code_lens;
pub mod diagnostics;
pub mod goto_definition;
pub mod hover;
pub mod inlay_hints;
pub mod item;
pub mod project;
pub mod project_context;
pub mod project_visitor;
pub mod references;
pub mod scope;
pub mod symbols;
pub mod types;
pub mod utils;
pub mod vfs;
pub mod linter;
pub mod move_generate_spec;
pub mod move_generate_spec_chen;
pub mod move_generate_spec_file;
pub mod move_generate_spec_sel;
pub mod sui_move_analyzer_beta_2024;



// pub mod sui_move_analyzer_beta_2024;
// pub mod sui_move_analyzer_alpha_2024;

// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0


use std::{path::PathBuf, str::FromStr};

use move_command_line_common::files::FileHash;
use move_compiler::{diagnostics::WarningFilters, editions::{Edition, Flavor}, shared::{CompilationEnv, PackageConfig}, Flags};
// use anyhow::Result;
// use crossbeam::channel::bounded;
// use lsp_types::Diagnostic;
// use move_compiler::linters::LintLevel;
// use move_package::source_package::parsed_manifest::{Dependencies, Dependency, DependencyKind, GitInfo, InternalDependency};
// use symbols::Symbols;
// use utils::get_path_from_url;
// use ::vfs::{MemoryFS, VfsPath};
use wasm_bindgen::prelude::*;
use serde_json::{Value, json};
use web_sys::{MessageEvent, MessagePort};
use wasm_bindgen::JsCast;
use js_sys::Function;
use wasm_bindgen_futures::spawn_local;
use context::Context;
use serde::{Serialize, Deserialize};
use serde_wasm_bindgen;

use crate::{context::{FileDiags, MultiProject}, utils::discover_manifest_and_kind};

mod console {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn log(s: &str);
    }
}

#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => (crate::console::log(&format!($($t)*)))
}

use once_cell::unsync::OnceCell;
use std::cell::RefCell;

thread_local! {
    pub static GLOBAL_CONTEXT: OnceCell<RefCell<Context>> = OnceCell::new();
}

// pub fn with_context<F, R>(f: F, conn: &WasmConnection, val: Value) -> R
// where
//     F: FnOnce(&mut Context, &WasmConnection, Value) -> R,
// {
//     GLOBAL_CONTEXT.with(|cell| {
//         let ctx_cell = cell.get().expect("Context not initialized");
//         let mut ctx = ctx_cell.borrow_mut();
//         f(&mut ctx, conn, val)
//     })
// }

// pub fn with_context<F, Fut>(f: F, conn: &WasmConnection, val: Value) -> Fut
// where
//     F: for<'a> FnOnce(&'a mut Context, &'a WasmConnection, Value) -> Fut,
//     Fut: std::future::Future<Output = ()>,
// {
//     GLOBAL_CONTEXT.with(|cell| {
//         let ctx_cell = cell.get().expect("Context not initialized");
//         let mut ctx = ctx_cell.borrow_mut();
//         f(&mut ctx, conn, val)
//     })
// }

fn with_context<F, R>(f: F, conn: &WasmConnection, val: Value) -> R
where
    F: FnOnce(&mut Context, &WasmConnection, Value) -> R,
{
    GLOBAL_CONTEXT.with(|cell| {
        let ctx_cell = cell.get().expect("Context not initialized");
        let mut ctx = ctx_cell.borrow_mut();
        f(&mut ctx, conn, val)
    })
}


#[wasm_bindgen]
pub struct WasmConnection {
    post_message: Function,
}

impl WasmConnection {
    fn send_response(&self, response: WasmResponse) {
        let msg = serde_wasm_bindgen::to_value(&response).unwrap();
        if let Err(e) = self.post_message.call1(&JsValue::NULL, &msg) {
            console_log!("Error sending response: {:?}", e);
        }
    }
}

#[derive(Serialize)]
pub struct WasmResponse {
    pub id: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

#[wasm_bindgen]
impl WasmConnection {
    #[wasm_bindgen(constructor)]
    pub fn new(post_message: Function, port: MessagePort) -> Result<WasmConnection, JsValue> {
        console_log!("Creating new WasmConnection");
        check_features();
        // ---------------------------
        let conn: WasmConnection = WasmConnection {
            post_message: post_message,
        };

        let context = Context {
            projects: MultiProject::default(),
            ref_caches: Default::default(),
            diag_version: FileDiags::new(),
        };

        GLOBAL_CONTEXT.with(|cell| {
            cell.set(RefCell::new(context)).unwrap();
        });

        let conn_weak = conn.clone();
        let callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            console_log!("Received message in Rust");
            
            if let Some(message) = e.data().as_string() {
                console_log!("Message content: {}", message);
                
                if let Ok(value) = serde_json::from_str::<Value>(&message) {
                    if let Some(method) = value.get("method") {
                        let conn = conn_weak.clone();
                        let value = value.clone();
                        
                        match method.as_str() {
                            Some("DidOpenTextDocument") => {
                                spawn_local(async move {
                                    with_context(
                                        |ctx, conn, val| handle_open_document(ctx, conn, val.into()),
                                        &conn,
                                        value
                                    );
                                });
                            }
                            Some("DidChangeTextDocument") => {
                                spawn_local(async move {
                                    with_context(
                                        |ctx, conn, val| handle_did_change(ctx, conn, val),
                                        &conn,
                                        value
                                    );
                                });
                            }
                            Some("GotoDefinition") => {
                                spawn_local(async move {
                                    with_context(
                                        |ctx, conn, val| handle_goto_definition(ctx, conn, val),
                                        &conn,
                                        value
                                    );
                                });
                            }
                            Some(unkonown) => {
                                console_log!("Unhandled method: {}", unkonown);
                            }
                            None => {
                                console_log!("Method is not a string");
                            }
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        port.set_onmessage(Some(callback.as_ref().unchecked_ref()));
        callback.forget();

        Ok(conn)
    }
}

impl Clone for WasmConnection {
    fn clone(&self) -> Self {
        WasmConnection {
            post_message: self.post_message.clone(),
        }
    }
}

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

fn check_features() {
    console_log!("检查编译条件:");
    
    #[cfg(target_arch = "wasm32")]
    console_log!("✅ target_arch = wasm32");
    
    #[cfg(not(target_arch = "wasm32"))]
    println!("❌ target_arch != wasm32");
    
    #[cfg(target_feature = "atomics")]
    console_log!("✅ atomics feature 已启用");
    
    #[cfg(not(target_feature = "atomics"))]
    console_log!("❌ atomics feature 未启用");
    
    #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
    console_log!("✅ wasm.rs 将被使用");
    
    #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
    console_log!("❌ wasm.rs 未启用，使用 native.rs");
}


fn handle_open_document<'a>(context: &'a mut Context, conn: &'a WasmConnection, value: Value) {
    console_log!("lsp server: open document: {:?}", value.get("id").unwrap().as_str());
    #[derive(Deserialize)]
    struct OpenDocumentParams { pub uri: String };

    let request: lsp_server::Request = serde_json::from_value(value).unwrap();
    let params: OpenDocumentParams =  serde_json::from_value::<OpenDocumentParams>(request.params).unwrap();

    
    let fpath = PathBuf::from_str(params.uri.as_str()).unwrap();
    let (mani, _) = match discover_manifest_and_kind(&fpath) {
        Some(x) => x,
        None => {
            console_log!("not move project.");
            // send_not_project_file_error(context, fpath, true);
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
            console_log!("project '{:?}' not found try load.", fpath.as_path());
        }
    };
    let p = match context.projects.load_project(&conn, &mani) {
        anyhow::Result::Ok(x) => x,
        anyhow::Result::Err(e) => {
            log::error!("load project failed,err:{:?}", e);
            return;
        }
    };
    context.projects.insert_project(p);
    // make_diag(context, diag_sender, fpath);

}

fn handle_did_change(context: &mut Context, conn: &WasmConnection, value: Value) {
    // let request: lsp_server::Request = serde_json::from_value(value).unwrap();
    // console_log!("lsp server: change document: {:?}", value.get("id").unwrap());

    // let val = value.get("params").expect("no params");
    // console_log!("lsp server: change document: {:?}", value);

    // let url = val.get("url").unwrap().as_str().unwrap();
    // let content = val.get("content").unwrap().as_str().unwrap();
    // console_log!("lsp server: change document: {:?}, {:?}", url, content);
}

fn handle_goto_definition<'a>(context: &'a mut Context, conn: &'a WasmConnection, value: Value) {
    // console_log!("lsp server: goto definition");
    // console_log!("lsp server: goto definition: {:?}", value.get("id").unwrap());
    // console_log!("lsp server: goto definition: {:?}", value.get("params").expect("no params"));
    // let val = value.get("params").expect("no params");
    // let url = val.get("url").unwrap().as_str().unwrap();
    // let line = val.get("pos").unwrap().get("line").unwrap().as_u64().unwrap();
    // let col = val.get("pos").unwrap().get("col").unwrap().as_u64().unwrap();
    // console_log!("url: {:?}, line: {:?}, col: {:?}", url, line, col);
}
