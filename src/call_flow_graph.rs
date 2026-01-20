// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::project::Project;

/// Empty implementation of CallFlowGraph to satisfy module imports
#[derive(Debug, Default)]
pub struct CallFlowGraph {
    _dummy: (),
}

impl CallFlowGraph {
    pub fn new() -> Self {
        Self { _dummy: () }
    }

    pub fn generate_for_project(_project: &Project) -> Self {
        Self::new()
    }

    pub fn to_json(&self) -> String {
        use serde_json::json;
        json!({
            "nodes": [],
            "edges": [],
            "message": "Function call graph feature is not yet implemented"
        }).to_string()
    }
}