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

pub mod code_lens;
pub mod completion;
pub mod context;
pub mod diagnostics;
pub mod goto_definition;
pub mod hover;
pub mod inlay_hints;
pub mod item;
pub mod lastest_implicit_deps;
pub mod linter;
pub mod message_for_js;
pub mod move_generate_spec;
pub mod move_generate_spec_chen;
pub mod move_generate_spec_file;
pub mod move_generate_spec_sel;
pub mod project;
pub mod project_context;
pub mod project_visitor;
pub mod references;
pub mod scope;
pub mod sui_move_analyzer_beta_2024;
pub mod symbols;
pub mod types;
pub mod utils;

// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, str::FromStr, time};

use move_command_line_common::files::FileHash;
use move_compiler::{
    diagnostics::warning_filters::WarningFiltersBuilder,
    editions::{Edition, Flavor},
    shared::{CompilationEnv, PackageConfig},
    Flags,
};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    context::{Context, FileDiags, MultiProject},
    hover::on_hover_request,
    references::on_references_request,
    sui_move_analyzer_beta_2024::try_reload_projects,
    utils::{discover_manifest_and_kind, get_default_usedecl},
};

use crate::goto_definition::{on_go_to_def_request, on_go_to_type_def_request};
// use crate::sui_move_analyzer_beta_2024::make_diag;
use once_cell::unsync::OnceCell;
use std::cell::RefCell;
use vfs::MemoryFS;

#[derive(Debug)]
pub struct WasmConnection;

impl WasmConnection {
    pub fn new() -> Self {
        WasmConnection {}
    }

    fn send_response(&mut self, response: message_for_js::response_type::Response4JSType) {
        if let Ok(bytes) = serde_json::to_vec(&response) {
            let ptr = bytes.as_ptr();
            let len = bytes.len();
            std::mem::forget(bytes); // 防止数据被提前释放
                                     // unsafe { callback(ptr, len); }
            unsafe {
                js_message_callback(ptr, len);
            }
        }
    }
}

thread_local! {
    pub static GLOBAL_CONTEXT: OnceCell<RefCell<Context>> = OnceCell::new();
    pub static GLOBAL_CONNECTION: OnceCell<RefCell<WasmConnection>> = OnceCell::new();
}

pub fn init_context() {
    let context = Context {
        projects: MultiProject::default(),
        ref_caches: Default::default(),
        diag_version: FileDiags::new(),
        ide_files_root: MemoryFS::new().into(),
    };

    GLOBAL_CONTEXT.with(|cell| {
        if let Err(e) = cell.set(RefCell::new(context)) {
            println!("GLOBAL_CONTEXT init failed: {:?}", e)
        }
    });

    let conn = WasmConnection::new();
    GLOBAL_CONNECTION.with(|cell| {
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
        GLOBAL_CONNECTION.with(|conn| {
            try_reload_projects(&mut ctx, conn.get().unwrap());
        });

        f(&mut ctx, val)
    })
}

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
            println!("update file failed,err:{:?}", d);
            return;
        }
    };
    // let (defs, _) = defs;
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
    #[derive(Deserialize, Debug)]
    struct OpenDocumentParams {
        pub url: String,
    };

    let params: OpenDocumentParams =
        match serde_json::from_value::<OpenDocumentParams>(request.params) {
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
                crate::context::send_popup_message(
                    conn.get().unwrap(),
                    lsp_types::MessageType::WARNING,
                    format!("{} not in move project", fpath.as_path().to_string_lossy()),
                );
            });
            return;
        }
    };
    match context.projects.get_project(&fpath) {
        Some(_) => {
            if let Ok(x) = std::fs::read_to_string(fpath.as_path()) {
                update_defs(context, fpath.clone(), x.as_str());
            };
            // make_diag(context, fpath);
            return;
        }
        None => {
            println!("project '{:?}' not found try load.", fpath.as_path());
        }
    };
    GLOBAL_CONNECTION.with(|conn| {
        let res = context.projects.load_project(conn.get().unwrap(), &mani);
        let p = match res {
            anyhow::Result::Ok(x) => x,
            anyhow::Result::Err(e) => {
                println!("load project failed,err:{:?}", e);
                return;
            }
        };
        context.projects.insert_project(p);
        println!(
            "context.projects.projects len: {:?}",
            context.projects.projects.len()
        );
    })
}

fn handle_did_change(context: &mut Context, request: lsp_server::Request) {
    #[derive(Deserialize)]
    struct ChangeDocumentParams {
        pub url: String,
        pub content: String,
    };
    let params = match serde_json::from_value::<ChangeDocumentParams>(request.params) {
        Ok(p) => p,
        Err(e) => {
            println!("OpenDocumentParams from value failed: {:?}", e);
            return;
        }
    };

    let fpath = PathBuf::from_str(params.url.as_str()).unwrap();
    println!(
        "context.projects.projects len: {:?}",
        context.projects.projects.len()
    );
    update_defs(context, fpath.clone(), params.content.as_str());
    // make_diag(context, fpath);
}

fn handle_goto_definition<'a>(
    context: &'a mut Context,
    request: lsp_server::Request,
) -> serde_json::Value {
    #[derive(Deserialize, Debug)]
    struct GotoDefinitionParams {
        pub url: String,
        pub pos: lsp_types::Position,
    };

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

fn handle_reference<'a>(
    context: &'a mut Context,
    request: lsp_server::Request,
) -> serde_json::Value {
    #[derive(Deserialize, Debug)]
    struct ReferenceParams {
        pub url: String,
        pub pos: lsp_types::Position,
        pub include_declaration: bool,
    };

    let params = match serde_json::from_value::<ReferenceParams>(request.params) {
        Ok(p) => p,
        Err(e) => {
            println!("ReferenceParams from value failed: {:?}", e);
            return serde_json::Value::Null;
        }
    };
    let fpath = PathBuf::from_str(params.url.as_str()).unwrap();
    on_references_request(context, fpath, params.pos, params.include_declaration)
}

fn handle_hover<'a>(context: &'a mut Context, request: lsp_server::Request) -> serde_json::Value {
    #[derive(Deserialize, Debug)]
    struct HoverParams {
        pub url: String,
        pub pos: lsp_types::Position,
    };

    let params = match serde_json::from_value::<HoverParams>(request.params) {
        Ok(p) => p,
        Err(e) => {
            println!("HoverParams from value failed: {:?}", e);
            return serde_json::Value::Null;
        }
    };
    let fpath = PathBuf::from_str(params.url.as_str()).unwrap();
    on_hover_request(context, fpath, params.pos)
}

fn handle_projects_clear(context: &mut Context, _request: lsp_server::Request) {
    context.projects.clear();
}

#[no_mangle]
pub extern "C" fn process_message(ptr: *const u8, len: usize) -> *mut u8 {
    // 读取输入
    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
    let start_time = time::Instant::now();
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
            println!("DidOpenTextDocument");
            with_context(|ctx, request| handle_open_document(ctx, request), request);
            println!("{}ms", start_time.elapsed().as_millis());
        }
        "DidChangeTextDocument" => {
            println!("DidChangeTextDocument");
            with_context(|ctx, request| handle_did_change(ctx, request), request);
            println!("{}ms", start_time.elapsed().as_millis());
        }
        "GotoDefinition" => {
            println!("GotoDefinition");
            let result = with_context(|ctx, request| handle_goto_definition(ctx, request), request);
            println!("{}ms", start_time.elapsed().as_millis());
            return serialize_with_length_prefix(result);
        }
        "FetchDependencies" => {
            println!("FetchDependencies");
            with_context(|ctx, request| handle_projects_clear(ctx, request), request);
            println!("{}ms", start_time.elapsed().as_millis());
        }
        "Reference" => {
            println!("Reference");
            let result = with_context(|ctx, request| handle_reference(ctx, request), request);
            println!("{}ms", start_time.elapsed().as_millis());
            return serialize_with_length_prefix(result);
        }
        "Hover" => {
            println!("Hover");
            let result = with_context(|ctx, request| handle_hover(ctx, request), request);
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
