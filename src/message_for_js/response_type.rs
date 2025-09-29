use std::collections::HashMap;
use url::Url;

#[derive(serde::Serialize)]
pub enum Response4JSType {
    Request(Response4Request),       // Response to a request
    Popup(Response4Popup),           // Popup message
    Diagnostic(Response4Diagnostic), // Diagnostic message
}

#[derive(serde::Serialize)]
pub struct Response4Request {
    pub id: String,
    pub method: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

#[derive(serde::Serialize)]
pub struct Response4Popup {
    pub message: String,
    pub mty: lsp_types::MessageType,
}

#[derive(serde::Serialize)]
pub struct Response4Diagnostic {
    pub diags: HashMap<Url, Vec<lsp_types::Diagnostic>>,
}
