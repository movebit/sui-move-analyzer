// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::utils::*;
use crate::js_message_callback;
use crate::message_for_js::response_type::{Response4Diagnostic, Response4JSType, Response4Popup};
use crate::{project::*, references::ReferencesCache, WasmConnection};
use im::HashSet;
use lsp_types::MessageType;
use move_command_line_common::files::FileHash;
use move_compiler::parser::ast::Definition;
use move_ir_types::location::Loc;
use move_package::source_package::layout::SourcePackageLayout;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};
use url::Url;
use vfs::VfsPath;

/// The context within which the language server is running.
pub struct Context {
    /// The connection with the language server's client.
    pub projects: MultiProject,
    pub ref_caches: ReferencesCache,
    pub diag_version: FileDiags,
    pub ide_files_root: VfsPath,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl_convert_loc!(MultiProject);

#[derive(Default)]
pub struct MultiProject {
    pub projects: HashMap<HashSet<PathBuf>, Project>,
    pub hash_file: Rc<RefCell<PathBufHashMap>>,
    pub file_line_mapping: Rc<RefCell<FileLineMapping>>,
    pub asts: HashMap<PathBuf, Rc<RefCell<SourceDefs>>>,
}

impl MultiProject {
    pub fn insert_project(&mut self, p: Project) {
        self.projects.insert(p.mk_multi_project_key(), p);
    }

    pub fn load_project(
        &mut self,
        sender: &RefCell<WasmConnection>,
        mani: &PathBuf,
    ) -> anyhow::Result<Project> {
        Project::new(mani, self, |msg: String| {
            send_popup_message(sender, MessageType::ERROR, msg)
        })
    }

    pub fn new() -> MultiProject {
        MultiProject::default()
    }

    pub fn get_project(&self, x: &Path) -> Option<&Project> {
        let (manifest, _) = super::utils::discover_manifest_and_kind(x)?;
        for (k, v) in self.projects.iter() {
            if k.contains(&manifest) {
                return Some(v);
            }
        }
        None
    }

    fn get_projects_mut(&mut self, x: &Path) -> Vec<&mut Project> {
        let (manifest, _) = match super::utils::discover_manifest_and_kind(x) {
            Some(x) => x,
            None => return vec![],
        };
        let mut ret = Vec::new();
        for (k, v) in self.projects.iter_mut() {
            if k.contains(&manifest) {
                ret.push(v);
            }
        }
        ret
    }

    pub fn update_defs(&mut self, file_path: PathBuf, defs: Vec<Definition>) {
        let (manifest, layout) = match super::utils::discover_manifest_and_kind(file_path.as_path())
        {
            Some(x) => x,
            None => {
                println!("file_path {:?} not found", file_path.as_path());
                return;
            }
        };
        let mut b = self.asts.get_mut(&manifest).unwrap().borrow_mut();
        let old_defs = if layout == SourcePackageLayout::Sources {
            b.sources.insert(file_path.clone(), defs)
        } else if layout == SourcePackageLayout::Tests {
            b.tests.insert(file_path.clone(), defs)
        } else if layout == SourcePackageLayout::Scripts {
            b.scripts.insert(file_path.clone(), defs)
        } else {
            unreachable!()
        };
        drop(b);
        self.get_projects_mut(&file_path)
            .into_iter()
            .for_each(|x| x.update_defs(&file_path, old_defs.as_ref()));
    }

    pub fn clear(&mut self) {
        self.projects.clear();
        self.hash_file.borrow_mut().clear();
        self.file_line_mapping.borrow_mut().clear();
        self.asts.clear();
    }
}

pub(crate) fn send_popup_message(conn: &RefCell<WasmConnection>, mty: MessageType, msg: String) {
    conn.borrow_mut()
        .send_response(Response4JSType::Popup(Response4Popup {
            message: msg,
            mty: mty,
        }));
}

pub(crate) fn send_diag_message(diags: HashMap<Url, Vec<lsp_types::Diagnostic>>) {
    let response = Response4JSType::Diagnostic(Response4Diagnostic { diags });
    if let Ok(bytes) = serde_json::to_vec(&response) {
        let ptr = bytes.as_ptr();
        let len = bytes.len();
        std::mem::forget(bytes); // 防止数据被提前释放
        unsafe {
            js_message_callback(ptr, len);
        }
    }
}

#[derive(Default, Debug)]
pub struct FileDiags {
    diags: HashMap<PathBuf, HashMap<url::Url, usize>>,
}

impl FileDiags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, mani: &PathBuf, fpath: &url::Url, diags: usize) {
        if let Some(x) = self.diags.get_mut(mani) {
            x.insert(fpath.clone(), diags);
        } else {
            let mut x: HashMap<url::Url, usize> = HashMap::new();
            x.insert(fpath.clone(), diags);
            self.diags.insert(mani.clone(), x);
        }
    }

    pub fn with_manifest(&self, mani: &PathBuf, mut call: impl FnMut(&HashMap<url::Url, usize>)) {
        let empty = Default::default();
        call(self.diags.get(mani).unwrap_or(&empty));
    }
}

#[allow(unused)]
static LOAD_DEPS: bool = false;

impl MultiProject {
    pub fn try_reload_projects(&mut self, connection: &RefCell<WasmConnection>) {
        println!("try_reload_projects");
        let mut all = Vec::new();
        let not_founds = {
            let mut x = Vec::new();
            for (k, v) in self.projects.iter() {
                if !v.manifest_not_exists.is_empty() {
                    x.push((
                        k.clone(),
                        v.manifest_not_exists.clone(),
                        v.manifest_paths.first().cloned().unwrap(),
                    ));
                }
            }
            x
        };
        println!("try_reload_projects not_founds: {:?}", not_founds);
        let mut modifies = Vec::new();
        for (k, p) in self.projects.iter() {
            if p.manifest_beed_modified() {
                let root = p.manifest_paths.first().cloned().unwrap();
                if !not_founds.iter().any(|x| x.2 == root) {
                    modifies.push((k.clone(), root));
                }
            }
        }
        println!("try_reload_projects modifies: {:?}", modifies);
        for (k, not_founds, root_manifest) in not_founds.into_iter() {
            let mut exists_now = false;
            for v in not_founds.iter() {
                let mut v = v.clone();
                v.push(PROJECT_FILE_NAME);
                if v.exists() {
                    exists_now = true;
                    break;
                }
            }
            if !exists_now {
                continue;
            }
            println!("reload  {:?}", root_manifest.as_path());
            let x = match Project::new(root_manifest, self, |msg| {
                send_popup_message(connection, MessageType::ERROR, msg)
            }) {
                Ok(x) => x,
                Err(_) => {
                    println!("reload project failed");
                    return;
                }
            };
            all.push((k, x));
        }
        for (k, root_manifest) in modifies.into_iter() {
            send_popup_message(
                connection,
                MessageType::INFO,
                format!("trying reload {:?}.", root_manifest.as_path()),
            );
            let x = match Project::new(root_manifest, self, |msg| {
                send_popup_message(connection, MessageType::ERROR, msg);
            }) {
                Ok(x) => x,
                Err(err) => {
                    send_popup_message(
                        connection,
                        MessageType::ERROR,
                        format!("reload project failed,err:{:?}", err),
                    );
                    continue;
                }
            };
            all.push((k, x));
        }
        for (k, v) in all.into_iter() {
            self.projects.remove(&k);
            self.insert_project(v);
        }
    }
}
