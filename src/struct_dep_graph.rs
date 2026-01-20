use crate::item::{Item, ItemOrAccess};
use crate::project::ItemOrAccessHandler;
use crate::project::Project;
use crate::project_context::ProjectContext;
use crate::types::ResolvedType;
use move_compiler::shared::Identifier;
use std::collections::{HashMap, HashSet};

/// Represents a node in the struct dependency graph
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct StructNode {
    pub name: String,
    pub module: String,
    pub address: String,
}

/// Represents an edge in the struct dependency graph
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct StructEdge {
    pub from: String,             // From struct name
    pub to: String,               // To struct name
    pub field_names: Vec<String>, // Field names that create the dependency
}

/// Struct dependency graph representation
#[derive(Debug, Default)]
pub struct StructDepGraph {
    pub nodes: Vec<StructNode>,
    pub edges: Vec<StructEdge>,
}

struct StructDependencyVisitor {
    nodes: HashSet<StructNode>,
    // key: (from, to), value: Set of field names to ensure uniqueness per edge
    edges: HashMap<(String, String), HashSet<String>>,
}

impl StructDependencyVisitor {
    fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            edges: HashMap::new(),
        }
    }

    fn add_node(&mut self, node: StructNode) {
        self.nodes.insert(node);
    }

    fn add_edge(&mut self, from: String, to: String, field_name: String) {
        self.edges.entry((from, to)).or_default().insert(field_name);
    }

    /// Recursively extract dependencies from a type
    fn extract_dependencies(&mut self, from_node_id: &str, field_name: &str, ty: &ResolvedType) {
        match ty {
            ResolvedType::Struct(struct_ref, type_args) => {
                let to_node = StructNode {
                    name: struct_ref.name.value().to_string(),
                    module: struct_ref.module_name.to_string(),
                    address: struct_ref.addr.to_hex_literal(),
                };
                let to_node_id = format!("{}.{}", to_node.module, to_node.name);

                // Avoid self-loops if desired, or keep them. keeping them for now.
                // Also avoid adding edge if we don't know the struct (e.g. error/unknown) - though StructRef usually implies it exists or is at least named.

                self.add_node(to_node.clone());
                self.add_edge(from_node_id.to_string(), to_node_id, field_name.to_string());

                // Recursively handle type arguments (e.g. vector<Coin<SUI>>)
                for arg in type_args {
                    self.extract_dependencies(from_node_id, field_name, arg);
                }
            }
            ResolvedType::Vec(inner_ty) => {
                self.extract_dependencies(from_node_id, field_name, inner_ty);
            }
            ResolvedType::Ref(_, inner_ty) => {
                self.extract_dependencies(from_node_id, field_name, inner_ty);
            }
            ResolvedType::Multiple(tys) => {
                for t in tys {
                    self.extract_dependencies(from_node_id, field_name, t);
                }
            }
            // Handle other types as needed (e.g. TParam might be relevant if we track generic constraints, but usually we care about concrete struct dependencies or instantiated ones)
            _ => {}
        }
    }
}

impl std::fmt::Display for StructDependencyVisitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StructDependencyVisitor")
    }
}

impl ItemOrAccessHandler for StructDependencyVisitor {
    fn handle_item_or_access(
        &mut self,
        _services: &dyn crate::project::HandleItemService,
        _project_context: &ProjectContext,
        item: &ItemOrAccess,
    ) {
        if let ItemOrAccess::Item(Item::Struct(struct_item)) = item {
            if struct_item.is_test {
                return;
            }

            let node = StructNode {
                name: struct_item.name.value().to_string(),
                module: struct_item.module_name.to_string(),
                address: struct_item.addr.to_hex_literal(),
            };
            self.add_node(node.clone());

            let from_node_id = format!("{}.{}", node.module, node.name);

            // Access fields potentially directly or resolve them if they are in the definition
            // The visitor hits the definition, so fields should be available if parsed.
            // However, `struct_item.fields` is `Vec<(Field, ResolvedType)>`.

            for (field, ty) in &struct_item.fields {
                let fs = field.0.value.as_str();
                self.extract_dependencies(&from_node_id, fs, ty);
            }
        }
    }

    fn function_or_spec_body_should_visit(&self, _range: &crate::utils::FileRange) -> bool {
        false
    }

    fn visit_fun_or_spec_body(&self) -> bool {
        false
    }

    fn finished(&self) -> bool {
        false
    }
}

impl StructDepGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Generate struct dependency graph for a project
    pub fn generate_for_project(project: &Project) -> Self {
        use crate::project::DummyHandler;
        use crate::project::ModulesAstProvider;
        use move_package::source_package::layout::SourcePackageLayout;

        eprintln!("Generating struct dependency graph for project...");

        let mut visitor = StructDependencyVisitor::new();
        project.project_context.clear_scopes_and_addresses();

        // Iterate dependencies first (rev)
        let manifests: Vec<_> = project.manifest_paths.iter().rev().cloned().collect();
        let root_manifest = project.manifest_paths.first();

        for m in manifests.iter() {
            let is_root = Some(m) == root_manifest;

            if is_root {
                // Visit Sources
                project.visit(
                    &project.project_context,
                    &mut visitor,
                    ModulesAstProvider::new(project, m.clone(), SourcePackageLayout::Sources),
                    true,
                );
                // Visit Tests
                project.visit(
                    &project.project_context,
                    &mut visitor,
                    ModulesAstProvider::new(project, m.clone(), SourcePackageLayout::Tests),
                    true,
                );
            } else {
                // Visit dependencies just to populate context
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

        // Convert HashSet to Vec
        let nodes = visitor.nodes.into_iter().collect();

        // Convert HashMap to Vec for Edges
        let edges = visitor
            .edges
            .into_iter()
            .map(|((from, to), fields)| {
                let mut field_names: Vec<String> = fields.into_iter().collect();
                field_names.sort(); // Consistent order
                StructEdge {
                    from,
                    to,
                    field_names,
                }
            })
            .collect();

        let graph = StructDepGraph { nodes, edges };

        eprintln!(
            "Generated struct dependency graph with {} nodes and {} edges",
            graph.nodes.len(),
            graph.edges.len()
        );
        graph
    }

    /// Export the graph in a format suitable for visualization (e.g., JSON)
    pub fn to_json(&self) -> String {
        use serde_json::{Value, json};

        let nodes_json: Vec<Value> = self
            .nodes
            .iter()
            .map(|node| {
                json!({
                    "id": format!("{}.{}", node.module, node.name),
                    "label": node.name,
                    "module": node.module,
                    "address": node.address
                })
            })
            .collect();

        let edges_json: Vec<Value> = self
            .edges
            .iter()
            .map(|edge| {
                json!({
                    "from": edge.from.clone(),
                    "to": edge.to.clone(),
                    "label": edge.field_names.join(", "),
                    "arrows": "to"
                })
            })
            .collect();

        json!({
            "nodes": nodes_json,
            "edges": edges_json
        })
        .to_string()
    }
}
