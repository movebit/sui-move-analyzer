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


use std::{fmt::format, path::PathBuf, str::FromStr};

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

use once_cell::unsync::OnceCell;
use std::cell::RefCell;
use std::borrow::Borrow;
use std::borrow::BorrowMut;
pub struct WasmConnection {
    message_callback: Option<extern "C" fn(*const u8, usize)>,
}

impl WasmConnection {
    pub fn new() -> Self {
        WasmConnection {
            message_callback: None,
        }
    }

    pub fn set_callback(&mut self, callback: extern "C" fn(*const u8, usize)) {
        println!("rust set_callback");
        self.message_callback = Some(callback);
        println!("callback is set: {:?}", self.message_callback.is_some());
    }

    fn send_response(&self, response: WasmResponse) {
        println!("callback is set: {:?}", self.message_callback.is_some());
        println!("send_response 11111111111");
        if let Ok(bytes) = serde_json::to_vec(&response) {
            println!("send_response 2222222222222");
            if let Some(callback) = self.message_callback {
                println!("send_response 333333333333");
                let ptr = bytes.as_ptr();
                let len = bytes.len();
                std::mem::forget(bytes); // 防止数据被提前释放
                callback(ptr, len);
            }
        }
    }
}

thread_local! {
    pub static GLOBAL_CONTEXT: OnceCell<RefCell<Context>> = OnceCell::new();
    pub static GLOBAL_CONNECTION: RefCell<WasmConnection> = RefCell::new(WasmConnection::new());
}

pub fn init_context() {
    let context = Context {
        projects: MultiProject::default(),
        ref_caches: Default::default(),
        diag_version: FileDiags::new(),
    };

    GLOBAL_CONTEXT.with(|cell| {
        cell.set(RefCell::new(context)).unwrap();
    });
}

#[link(wasm_import_module = "env")]
extern "C" {
    fn js_message_callback(ptr: *const u8, len: usize);
}

// 注册回调的外部函数
#[no_mangle]
pub extern "C" fn register_message_callback(callback: extern "C" fn(*const u8, usize)) {
    println!("rust register_message_callback");
    GLOBAL_CONNECTION.with(|conn| {
        conn.borrow_mut().set_callback(callback);
    });
}

fn with_context<F, R>(f: F, val: lsp_server::Request) -> R
where
    F: FnOnce(&mut Context, lsp_server::Request) -> R,
{
    println!("222222222");
    GLOBAL_CONTEXT.with(|cell| {
        println!("333333333333");
        let ctx_cell = cell.get().expect("Context not initialized");
        println!("44444444444444");
        let mut ctx = ctx_cell.borrow_mut();
        println!("55555555555555");
        f(&mut ctx, val)
    })
}

#[derive(Serialize)]
pub struct WasmResponse {
    pub id: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

// #[wasm_bindgen]
// impl WasmConnection {
//     #[wasm_bindgen(constructor)]
//     pub fn new(post_message: Function, port: MessagePort) -> Result<WasmConnection, JsValue> {
//         println!("Creating new WasmConnection");
//         check_features();
//         // ---------------------------
//         let conn: WasmConnection = WasmConnection {
//             post_message: post_message,
//         };

        

//         let conn_weak = conn.clone();
//         let callback = Closure::wrap(Box::new(move |e: MessageEvent| {
//             println!("Received message in Rust");
            
//             if let Some(message) = e.data().as_string() {
//                 println!("Message content: {}", message);
                
//                 if let Ok(value) = serde_json::from_str::<Value>(&message) {
//                     if let Some(method) = value.get("method") {
//                         let conn = conn_weak.clone();
//                         let value = value.clone();
                        
//                         match method.as_str() {
//                             Some("DidOpenTextDocument") => {
//                                 println!("DidOpenTextDocument 000");
//                                 spawn_local(async move {
//                                     println!("DidOpenTextDocument 111");
//                                     with_context(
//                                         |ctx, conn, val| handle_open_document(ctx, conn, val),
//                                         &conn,
//                                         value
//                                     );
//                                     println!("DidOpenTextDocument 222");
//                                 });
//                             }
//                             Some("DidChangeTextDocument") => {
//                                 spawn_local(async move {
//                                     with_context(
//                                         |ctx, conn, val| handle_did_change(ctx, conn, val),
//                                         &conn,
//                                         value
//                                     );
//                                 });
//                             }
//                             Some("GotoDefinition") => {
//                                 spawn_local(async move {
//                                     with_context(
//                                         |ctx, conn, val| handle_goto_definition(ctx, conn, val),
//                                         &conn,
//                                         value
//                                     );
//                                 });
//                             }
//                             Some(unkonown) => {
//                                 println!("Unhandled method: {}", unkonown);
//                             }
//                             None => {
//                                 println!("Method is not a string");
//                             }
//                         }
//                     }
//                 }
//             }
//         }) as Box<dyn FnMut(MessageEvent)>);

//         port.set_onmessage(Some(callback.as_ref().unchecked_ref()));
//         callback.forget();

//         Ok(conn)
//     }
// }

// impl Clone for WasmConnection {
//     fn clone(&self) -> Self {
//         WasmConnection {
//             post_message: self.post_message.clone(),
//         }
//     }
// }

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
    println!("检查编译条件:");
    
    #[cfg(target_arch = "wasm32")]
    println!("✅ target_arch = wasm32");
    
    #[cfg(not(target_arch = "wasm32"))]
    println!("❌ target_arch != wasm32");
    
    #[cfg(target_feature = "atomics")]
    println!("✅ atomics feature 已启用");
    
    #[cfg(not(target_feature = "atomics"))]
    println!("❌ atomics feature 未启用");
    
    #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
    println!("✅ wasm.rs 将被使用");
    
    #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
    println!("❌ wasm.rs 未启用，使用 native.rs");
}


fn handle_open_document<'a>(context: &'a mut Context, request: lsp_server::Request) {
    #[derive(Deserialize)]
    struct OpenDocumentParams { pub url: String };

    let params: OpenDocumentParams = match serde_json::from_value::<OpenDocumentParams>(request.params) {
        Ok(p) => p,
        Err(e) => {
            println!("OpenDocumentParams from value failed: {:?}", e);
            return;
        }
    };
    
    let fpath = PathBuf::from_str(params.url.as_str()).unwrap();
    let (mani, _) = match discover_manifest_and_kind(&fpath) {
        Some(x) => x,
        None => {
            println!("not move project.");
            GLOBAL_CONNECTION.with(|conn| {
                crate::context::send_show_message(conn, lsp_types::MessageType::ERROR, "from send_show_message".to_string());
            });
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
            println!("project '{:?}' not found try load.", fpath.as_path());
        }
    };
    GLOBAL_CONNECTION.with(|conn| {
        let p = match context.projects.load_project(conn, &mani) {
            anyhow::Result::Ok(x) => x,
            anyhow::Result::Err(e) => {
                log::error!("load project failed,err:{:?}", e);
                return;
            }
        };
        context.projects.insert_project(p);
    })
    
    // make_diag(context, diag_sender, fpath);

}

fn handle_did_change(context: &mut Context, request: lsp_server::Request) {
    // let request: lsp_server::Request = serde_json::from_value(value).unwrap();
    // println!("lsp server: change document: {:?}", value.get("id").unwrap());

    // let val = value.get("params").expect("no params");
    // println!("lsp server: change document: {:?}", value);

    // let url = val.get("url").unwrap().as_str().unwrap();
    // let content = val.get("content").unwrap().as_str().unwrap();
    // println!("lsp server: change document: {:?}, {:?}", url, content);
}

fn handle_goto_definition<'a>(context: &'a mut Context, request: lsp_server::Request) {
    // println!("lsp server: goto definition");
    // println!("lsp server: goto definition: {:?}", value.get("id").unwrap());
    // println!("lsp server: goto definition: {:?}", value.get("params").expect("no params"));
    // let val = value.get("params").expect("no params");
    // let url = val.get("url").unwrap().as_str().unwrap();
    // let line = val.get("pos").unwrap().get("line").unwrap().as_u64().unwrap();
    // let col = val.get("pos").unwrap().get("col").unwrap().as_u64().unwrap();
    // println!("url: {:?}, line: {:?}, col: {:?}", url, line, col);
}


#[derive(Deserialize)]
struct Input {
    id: String,
}

#[derive(Serialize)]
struct Output {
    message: String,
}

#[no_mangle]
pub extern "C" fn process_message(ptr: *const u8, len: usize) -> *mut u8 {
    // 读取输入
    let data = unsafe { std::slice::from_raw_parts(ptr, len) };

    let request: lsp_server::Request = match serde_json::from_slice(data) {
        Ok(r) => r,
        Err(e) => {
            println!("lsp_server::Request from value failed: {:?}", e);
            return std::ptr::null_mut();
        }
    };
    println!("process_message input  data{:?}", data);
    
    // 构造输出
    let method = request.method.clone();
    match method.as_str() {
        "DidOpenTextDocument" => {
            println!("DidOpenTextDocument 000");
            with_context(
                |ctx, request| handle_open_document(ctx, request),
                request
            );
        }
        "DidChangeTextDocument" => {
            with_context(
                |ctx, request| handle_did_change(ctx, request),
                request
            );
        }
        "GotoDefinition" => {
            with_context(
                |ctx, request| handle_goto_definition(ctx, request),
                request
            );
        }
        _ => {
            println!("Method is not a string");
        }
    }
    return std::ptr::null_mut();
    // // 序列化输出
    // let output_str = serde_json::to_string(&output).unwrap();
    // let output_bytes = output_str.into_bytes();
    
    // // 返回
    // Box::into_raw(output_bytes.into_boxed_slice()) as *mut u8
}