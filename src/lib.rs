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
// use std::{collections::BTreeMap, future::Future, path::PathBuf, sync::Arc};
// use wasm_sync::Mutex;
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format!($($t)*)))
}

#[wasm_bindgen]
pub struct WasmConnection {
    post_message: Function,
}

impl WasmConnection {
    async fn handle_initialize(&self, value: &Value) {
        let response = json!({
            "jsonrpc": "2.0",
            "id": value.get("id").and_then(|id| id.as_u64()).unwrap_or(0),
            "method": "initialize",
            "result": {
                "capabilities": {
                    "textDocumentSync": 1,
                    "completionProvider": {
                        "triggerCharacters": ["."]
                    },
                    "hoverProvider": true,
                    "definitionProvider": true,
                    "referencesProvider": true,
                }
            }
        });

        self.send_response(&response);
    }

    async fn handle_did_change(&self, value: &Value) {
        let response = json!({
            "jsonrpc": "2.0",
            "id": value.get("id").and_then(|id| id.as_u64()).unwrap_or(0),
            "method": "textDocument/didChange",
            "result": {
                "changedFile": "111", 
            }
        });

        self.send_response(&response);
    }

    fn send_response(&self, response: &Value) {
        let this = JsValue::null();
        let msg = JsValue::from_str(&response.to_string());
        if let Err(e) = self.post_message.call1(&this, &msg) {
            console_log!("Error sending response: {:?}", e);
        }
    }
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
                            Some("initialize") => {
                                console_log!("initialize");
                                spawn_local(async move {
                                    conn.handle_initialize(&value).await;
                                });
                            }
                            Some("initialized") => {
                                console_log!("Server initialized successfully");
                            }
                            Some("textDocument/didChange") => {
                                console_log!("Document changed");
                                spawn_local(async move {
                                    conn.handle_did_change(&value).await;
                                });
                            }
                            Some(method) => {
                                console_log!("Unhandled method: {}", method);
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
