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


use std::{fmt::format, path::{Path, PathBuf}, str::FromStr};

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
use std::env;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use crate::goto_definition::{on_go_to_type_def_request, on_go_to_def_request};

#[derive(Debug)]
pub struct WasmConnection;

impl WasmConnection {
    pub fn new() -> Self {
        WasmConnection {}
    }

    fn send_response(&mut self, response: WasmResponse) {
        println!("send_response 11111111111");
        if let Ok(bytes) = serde_json::to_vec(&response) {
            println!("send_response 2222222222222");
            println!("send_response 333333333333");
            let ptr = bytes.as_ptr();
            let len = bytes.len();
            std::mem::forget(bytes); // 防止数据被提前释放
            // unsafe { callback(ptr, len); }
            unsafe { 
                js_message_callback(ptr, len);
            }
            println!("send_response 44444444444");
            
        }
    }
}

thread_local! {
    pub static GLOBAL_CONTEXT: OnceCell<RefCell<Context>> = OnceCell::new();
    pub static GLOBAL_CONNECTION: OnceCell<RefCell<WasmConnection>> = OnceCell::new();
}


// fn ensure_initialized() {
//     static INIT: std::sync::Once = std::sync::Once::new();
//     INIT.call_once(|| {
//         init_context();
//     });
// }

pub fn init_context() {
    let context = Context {
        projects: MultiProject::default(),
        ref_caches: Default::default(),
        diag_version: FileDiags::new(),
    };

    GLOBAL_CONTEXT.with(|cell| {
        if let Err(e) =  cell.set(RefCell::new(context)) {
            println!("GLOBAL_CONTEXT init failed: {:?}", e)
        }
    });

    let conn = WasmConnection::new();
    GLOBAL_CONNECTION.with(|cell|{
        cell.set(RefCell::new(conn)).unwrap();
    });
}

#[link(wasm_import_module = "env")]
extern "C" {
    fn js_message_callback(ptr: *const u8, len: usize);
}

fn with_context<F, R>(f: F, val: lsp_server::Request) -> R
where
    F: FnOnce(&mut Context, lsp_server::Request) -> R,
{
    GLOBAL_CONTEXT.with(|cell| {
        let ctx_cell = cell.get().expect("Context not initialized");
        let mut ctx = ctx_cell.borrow_mut();
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


fn update_defs(context: &mut Context, fpath: PathBuf, content: &str) {
    println!("update_defs: fpath {:?}. content {:?}", fpath, content);
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
    println!("update defs 22222222");
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
    println!("update defs 333333333")
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
    #[derive(Deserialize, Debug)]
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
                crate::context::send_show_message(conn.get().unwrap(), lsp_types::MessageType::ERROR, "from send_show_message".to_string());
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
        let p = match context.projects.load_project(conn.get().unwrap(), &mani) {
            anyhow::Result::Ok(x) => x,
            anyhow::Result::Err(e) => {
                log::error!("load project failed,err:{:?}", e);
                return;
            }
        };
        context.projects.insert_project(p);
        println!("context.projects.projects len: {:?}", context.projects.projects.len());
    })
    
    // make_diag(context, diag_sender, fpath);

}

fn handle_did_change(context: &mut Context, request: lsp_server::Request) {
    #[derive(Deserialize)]
    struct ChangeDocumentParams { pub url: String, pub content: String };
    let params = match serde_json::from_value::<ChangeDocumentParams>(request.params) {
        Ok(p) => p,
        Err(e) => {
            println!("OpenDocumentParams from value failed: {:?}", e);
            return;
        }
    };
    
    let fpath = PathBuf::from_str(params.url.as_str()).unwrap();
    println!("context.projects.projects len: {:?}", context.projects.projects.len());
    update_defs(
        context,
        fpath,
        params.content.as_str(),
    );
}

fn handle_goto_definition<'a>(context: &'a mut Context, request: lsp_server::Request) -> serde_json::Value {
    println!("handle_goto_definition");
    #[derive(Deserialize, Debug)]
    struct GotoDefinitionParams { pub url: String, pub pos: lsp_types::Position };
    let params = match serde_json::from_value::<GotoDefinitionParams>(request.params) {
        Ok(p) => p,
        Err(e) => {
            println!("GotoDefinitionParams from value failed: {:?}", e);
            return serde_json::Value::Null;
        }
    };
    let fpath = PathBuf::from_str(params.url.as_str()).unwrap();
    on_go_to_def_request(&context, fpath, params.pos)
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
            return serialize_empty();
        }
    };
    // println!("process_message input  data{:?}", data);
    
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
            let result = with_context(
                |ctx, request| handle_goto_definition(ctx, request),
                request
            );
            return serialize_with_length_prefix(result);
        }
        _ => {
            println!("Method is not a string");
        }
    }
    return serialize_empty();
    // // 序列化输出
    // let output_str = serde_json::to_string(&output).unwrap();
    // let output_bytes = output_str.into_bytes();
    
    // // 返回
    // Box::into_raw(output_bytes.into_boxed_slice()) as *mut u8
}

pub fn serialize_with_length_prefix(value: Value) -> *mut u8 {
    if let serde_json::Value::Null = value {
        return serialize_empty();
    }
    
    // 序列化为 JSON 字符串
    let output_str = serde_json::to_string(&value).unwrap();
    let output_bytes = output_str.as_bytes();

    // 计算长度（u32，小端）
    let len = output_bytes.len() as u32;
    let mut buf = Vec::with_capacity(4 + output_bytes.len());

    // 写入长度
    buf.extend_from_slice(&len.to_le_bytes());

    // 写入 JSON 字节
    buf.extend_from_slice(output_bytes);
    // 转成 Box<[u8]> 再 into_raw
    let boxed_slice = buf.into_boxed_slice();
    Box::into_raw(boxed_slice) as *mut u8
}

/// 返回一个只包含长度前缀=0 的 buffer
pub fn serialize_empty() -> *mut u8 {
    let mut buf = Vec::with_capacity(4);
    buf.extend_from_slice(&0u32.to_le_bytes());
    let boxed_slice = buf.into_boxed_slice();
    Box::into_raw(boxed_slice) as *mut u8
}