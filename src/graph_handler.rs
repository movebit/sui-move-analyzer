// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    struct_dep_graph::StructDepGraph,
    // call_flow_graph::CallFlowGraph,  // Temporarily commented out
};
use lsp_server::{Request, Response};
use lsp_types::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};

// Define custom request types for graph generation
#[derive(Debug, Serialize, Deserialize)]
pub struct GraphParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    #[serde(rename = "graphType")]
    pub graph_type: String, // "struct_dependency" or "call_flow"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphResponse {
    pub graph_data: String, // JSON string representation of the graph
    pub error: Option<String>,
}

pub fn on_struct_dependency_request(context: &Context, request: &Request) {
    eprintln!("Handling struct dependency request");
    handle_graph_request(context, request, "struct_dependency");
}

pub fn on_call_flow_request(context: &Context, request: &Request) {
    // Temporarily disabled until implementation is complete
    let _params: GraphParams = match serde_json::from_value(request.params.clone()) {
        Ok(params) => params,
        Err(e) => {
            let response = Response::new_err(
                request.id.clone(),
                lsp_server::ErrorCode::InvalidParams as i32,
                format!("Failed to parse parameters: {}", e),
            );
            if let Err(e) = context.connection.sender.send(lsp_server::Message::Response(response)) {
                eprintln!("Error sending call flow request error response: {}", e);
            }
            return;
        }
    };

    let response = GraphResponse {
        graph_data: serde_json::json!({
            "nodes": [],
            "edges": [],
            "message": "Function call graph feature is not yet implemented"
        }).to_string(),
        error: Some("Function call graph feature is not yet implemented".to_string()),
    };

    let response = Response::new_ok(request.id.clone(), serde_json::to_value(response).unwrap());
    if let Err(e) = context.connection.sender.send(lsp_server::Message::Response(response)) {
        eprintln!("Error sending call flow response: {}", e);
    }
}

fn handle_graph_request(
    context: &Context, 
    request: &Request, 
    graph_type: &str
) {
    eprintln!("Entering handle_graph_request with graph_type: {}", graph_type);
    
    let params: GraphParams = match serde_json::from_value::<GraphParams>(request.params.clone()) {
        Ok(params) => {
            eprintln!("Successfully parsed parameters: graph_type={}", params.graph_type);
            params
        },
        Err(e) => {
            eprintln!("Failed to parse parameters: {}", e);
            let response = Response::new_err(
                request.id.clone(),
                lsp_server::ErrorCode::InvalidParams as i32,
                format!("Failed to parse parameters: {}", e),
            );
            if let Err(e) = context.connection.sender.send(lsp_server::Message::Response(response)) {
                eprintln!("Error sending error response: {}", e);
            }
            return;
        }
    };

    let file_path = match params.text_document.uri.to_file_path() {
        Ok(path) => {
            eprintln!("Successfully converted URI to file path: {:?}", path);
            path
        },
        Err(_) => {
            eprintln!("Failed to convert URI to file path: {}", params.text_document.uri);
            let response = Response::new_err(
                request.id.clone(),
                lsp_server::ErrorCode::InvalidParams as i32,
                "Could not convert URI to file path".to_string(),
            );
            if let Err(e) = context.connection.sender.send(lsp_server::Message::Response(response)) {
                eprintln!("Error sending file path conversion error response: {}", e);
            }
            return;
        }
    };

    // Get the project for the file
    let project = match context.projects.get_project(&file_path) {
        Some(proj) => {
            eprintln!("Successfully found project for file: {:?}", file_path);
            proj
        },
        None => {
            eprintln!("Could not find project for file: {:?}", file_path);
            let response = Response::new_err(
                request.id.clone(),
                lsp_server::ErrorCode::InvalidRequest as i32,
                "Could not find project for file".to_string(),
            );
            if let Err(e) = context.connection.sender.send(lsp_server::Message::Response(response)) {
                eprintln!("Error sending project not found response: {}", e);
            }
            return;
        }
    };

    // Generate the appropriate graph
    let graph_data = match graph_type {
        "struct_dependency" => {
            eprintln!("Generating struct dependency graph for project...");
            let graph = StructDepGraph::generate_for_project(project);
            eprintln!("Generated struct dependency graph with {} nodes and {} edges", graph.nodes.len(), graph.edges.len());
            graph.to_json()
        },
        "call_flow" => {
            // Return empty graph for now since implementation is disabled
            eprintln!("Call flow graph is temporarily disabled");
            return on_call_flow_request(context, request);
        },
        unsupported => {
            eprintln!("Unsupported graph type requested: {}", unsupported);
            let response = Response::new_err(
                request.id.clone(),
                lsp_server::ErrorCode::InvalidParams as i32,
                format!("Unsupported graph type: {}", graph_type),
            );
            if let Err(e) = context.connection.sender.send(lsp_server::Message::Response(response)) {
                eprintln!("Error sending unsupported graph type response: {}", e);
            }
            return;
        }
    };

    let response = GraphResponse {
        graph_data,
        error: None,
    };

    eprintln!("Returning successful response for {} graph", graph_type);
    eprintln!("Response data: {:?}", response);
    
    let lsp_response = Response::new_ok(request.id.clone(), serde_json::to_value(response).unwrap());
    eprintln!("Created LSP response: {:?}", lsp_response);
    
    if let Err(e) = context.connection.sender.send(lsp_server::Message::Response(lsp_response)) {
        eprintln!("Error sending struct dependency response: {}", e);
    }
}