// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::project::Project;

/// Represents a node in the struct dependency graph
#[derive(Debug, Clone)]
pub struct StructNode {
    pub name: String,
    pub module: String,
    pub address: String,
}

/// Represents an edge in the struct dependency graph
#[derive(Debug, Clone)]
pub struct StructEdge {
    pub from: String,  // From struct name
    pub to: String,    // To struct name
    pub field_name: String, // Field name that creates the dependency
}

/// Struct dependency graph representation
#[derive(Debug, Default)]
pub struct StructDepGraph {
    pub nodes: Vec<StructNode>,
    pub edges: Vec<StructEdge>,
}

impl StructDepGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
    
    /// Generate struct dependency graph for a project
    pub fn generate_for_project(_project: &Project) -> Self {
        eprintln!("Generating struct dependency graph for project...");
        
        // For now, we'll return an example graph with hardcoded data
        // to demonstrate the frontend visualization until we properly integrate with Move AST
        let mut graph = StructDepGraph::new();
        
        // Add example nodes
        graph.nodes.push(StructNode {
            name: "Coin".to_string(),
            module: "coin".to_string(),
            address: "0x2".to_string(),
        });
        
        graph.nodes.push(StructNode {
            name: "TreasuryCap".to_string(),
            module: "coin".to_string(),
            address: "0x2".to_string(),
        });
        
        graph.nodes.push(StructNode {
            name: "Balance".to_string(),
            module: "balance".to_string(),
            address: "0x2".to_string(),
        });
        
        graph.nodes.push(StructNode {
            name: "UID".to_string(),
            module: "object".to_string(),
            address: "0x2".to_string(),
        });
        
        graph.nodes.push(StructNode {
            name: "ID".to_string(),
            module: "object".to_string(),
            address: "0x2".to_string(),
        });
        
        // Add example edges
        graph.edges.push(StructEdge {
            from: "coin.Coin".to_string(),
            to: "balance.Balance".to_string(),
            field_name: "value".to_string(),
        });
        
        graph.edges.push(StructEdge {
            from: "coin.TreasuryCap".to_string(),
            to: "coin.Coin".to_string(),
            field_name: "dummy_field".to_string(),
        });
        
        graph.edges.push(StructEdge {
            from: "object.UID".to_string(),
            to: "object.ID".to_string(),
            field_name: "id".to_string(),
        });
        
        graph.edges.push(StructEdge {
            from: "coin.Coin".to_string(),
            to: "object.UID".to_string(),
            field_name: "id".to_string(),
        });
        
        eprintln!("Generated example struct dependency graph with {} nodes and {} edges", graph.nodes.len(), graph.edges.len());
        graph
    }

    /// Export the graph in a format suitable for visualization (e.g., JSON)
    pub fn to_json(&self) -> String {
        use serde_json::{json, Value};
        
        let nodes_json: Vec<Value> = self.nodes.iter().map(|node| {
            json!({
                "id": format!("{}.{}", node.module, node.name),
                "label": node.name,
                "module": node.module,
                "address": node.address
            })
        }).collect();

        let edges_json: Vec<Value> = self.edges.iter().map(|edge| {
            json!({
                "from": edge.from.clone(),
                "to": edge.to.clone(),
                "label": edge.field_name,
                "arrows": "to"
            })
        }).collect();

        json!({
            "nodes": nodes_json,
            "edges": edges_json
        }).to_string()
    }

}