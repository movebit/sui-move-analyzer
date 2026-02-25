// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::item::{Access, Item, ItemOrAccess};
use crate::project::{ItemOrAccessHandler, Project};
use crate::project_context::ProjectContext;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Represents a node in the call flow graph
#[derive(Debug, Clone, serde::Serialize)]
pub struct CallNode {
    pub id: String, // address.module.name
    pub name: String,
    pub module: String,
    pub address: String,
    pub filepath: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

impl std::hash::Hash for CallNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for CallNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for CallNode {}

/// Represents an edge in the call flow graph
#[derive(Debug, Clone, Hash, Eq, PartialEq, serde::Serialize)]
pub struct CallEdge {
    pub from: String, // from function id
    pub to: String,   // to function id
    pub filepath: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

/// Call flow graph representation
#[derive(Debug, Default, serde::Serialize)]
pub struct CallFlowGraph {
    pub nodes: Vec<CallNode>,
    pub edges: Vec<CallEdge>,
}

struct CallGraphVisitor<'a> {
    project: &'a Project,
    nodes: HashMap<String, CallNode>,
    edges: HashSet<CallEdge>,
    current_function_id: Option<String>,
    root_manifest: Option<PathBuf>,
}

impl<'a> CallGraphVisitor<'a> {
    fn new(project: &'a Project) -> Self {
        let root_manifest = project.manifest_paths.first().cloned();
        Self {
            project,
            nodes: HashMap::new(),
            edges: HashSet::new(),
            current_function_id: None,
            root_manifest,
        }
    }

    fn get_id(address: &str, module: &str, name: &str) -> String {
        format!("{}.{}.{}", address, module, name)
    }

    fn is_in_root_project(&self, item_fun: &crate::item::ItemFun) -> bool {
        let loc = item_fun.name.0.loc;
        use crate::project::ConvertLoc;
        if let Some(range) = self.project.convert_loc_range(&loc) {
            if let Some((manifest_path, _)) = crate::utils::discover_manifest_and_kind(&range.path)
            {
                if let Some(root) = &self.root_manifest {
                    return &manifest_path == root;
                }
            }
        }
        false
    }

    fn add_node(&mut self, item_fun: &crate::item::ItemFun) -> Option<String> {
        // Filter out functions not in the root project
        if !self.is_in_root_project(item_fun) {
            return None;
        }

        let address = item_fun.addr_and_name.addr.to_hex_literal();
        let module = item_fun.addr_and_name.name.0.value.as_str().to_string();
        let name = item_fun.name.0.value.as_str().to_string();
        let id = Self::get_id(&address, &module, &name);

        if !self.nodes.contains_key(&id) {
            use crate::project::ConvertLoc;
            let loc = item_fun.name.0.loc;
            let file_range = self.project.convert_loc_range(&loc);

            let node = CallNode {
                id: id.clone(),
                name,
                module,
                address,
                filepath: file_range
                    .as_ref()
                    .map(|r| r.path.to_string_lossy().to_string()),
                line: file_range.as_ref().map(|r| r.line_start),
                col: file_range.as_ref().map(|r| r.col_start),
            };
            self.nodes.insert(id.clone(), node);
        }
        Some(id)
    }
}

impl ItemOrAccessHandler for CallGraphVisitor<'_> {
    fn handle_item_or_access(
        &mut self,
        _services: &dyn crate::project::HandleItemService,
        _project_context: &ProjectContext,
        item: &ItemOrAccess,
    ) {
        match item {
            ItemOrAccess::Item(Item::ModuleName(_)) => {
                self.current_function_id = None;
            }
            ItemOrAccess::Item(Item::Fun(item_fun)) => {
                // We found a function definition or we are entering a function body
                self.current_function_id = self.add_node(item_fun);
            }
            ItemOrAccess::Access(Access::ExprAccessChain(chain, _, item)) => {
                if let Item::Fun(target_fun) = item.as_ref() {
                    let to_id_opt = self.add_node(target_fun);
                    if let (Some(from_id), Some(to_id)) = (&self.current_function_id, to_id_opt) {
                        use crate::project::ConvertLoc;
                        let file_range = self.project.convert_loc_range(&chain.loc);

                        self.edges.insert(CallEdge {
                            from: from_id.clone(),
                            to: to_id,
                            filepath: file_range
                                .as_ref()
                                .map(|r| r.path.to_string_lossy().to_string()),
                            line: file_range.as_ref().map(|r| r.line_start),
                            col: file_range.as_ref().map(|r| r.col_start),
                        });
                    }
                }
            }
            ItemOrAccess::Access(Access::MacroCall(_, chain, item)) => {
                if let Item::Fun(target_fun) = item.as_ref() {
                    let to_id_opt = self.add_node(target_fun);
                    if let (Some(from_id), Some(to_id)) = (&self.current_function_id, to_id_opt) {
                        use crate::project::ConvertLoc;
                        let file_range = self.project.convert_loc_range(&chain.loc);

                        self.edges.insert(CallEdge {
                            from: from_id.clone(),
                            to: to_id,
                            filepath: file_range
                                .as_ref()
                                .map(|r| r.path.to_string_lossy().to_string()),
                            line: file_range.as_ref().map(|r| r.line_start),
                            col: file_range.as_ref().map(|r| r.col_start),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn function_or_spec_body_should_visit(&self, _range: &crate::utils::FileRange) -> bool {
        true
    }

    fn visit_fun_or_spec_body(&self) -> bool {
        true
    }

    fn finished(&self) -> bool {
        false
    }
}

impl std::fmt::Display for CallGraphVisitor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CallGraphVisitor")
    }
}

impl CallFlowGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn generate_for_project(project: &Project) -> Self {
        use crate::project::{DummyHandler, ModulesAstProvider};
        use move_package::source_package::layout::SourcePackageLayout;

        let mut visitor = CallGraphVisitor::new(project);
        project.project_context.clear_scopes_and_addresses();

        let manifests: Vec<_> = project.manifest_paths.iter().rev().cloned().collect();
        let root_manifest = project.manifest_paths.first();

        for m in manifests.iter() {
            let is_root = Some(m) == root_manifest;
            if is_root {
                project.visit(
                    &project.project_context,
                    &mut visitor,
                    ModulesAstProvider::new(project, m.clone(), SourcePackageLayout::Sources),
                    true,
                );
                project.visit(
                    &project.project_context,
                    &mut visitor,
                    ModulesAstProvider::new(project, m.clone(), SourcePackageLayout::Tests),
                    true,
                );
            } else {
                let mut dummy = DummyHandler;
                project.visit(
                    &project.project_context,
                    &mut dummy,
                    ModulesAstProvider::new(project, m.clone(), SourcePackageLayout::Sources),
                    true,
                );
                project.visit(
                    &project.project_context,
                    &mut dummy,
                    ModulesAstProvider::new(project, m.clone(), SourcePackageLayout::Tests),
                    true,
                );
            }
        }

        CallFlowGraph {
            nodes: visitor.nodes.into_values().collect(),
            edges: visitor.edges.into_iter().collect(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize graph: {}\"}}", e))
    }
}
