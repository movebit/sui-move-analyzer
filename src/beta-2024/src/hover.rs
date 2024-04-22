// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::context::Context;
use super::goto_definition;
use super::item::*;
use super::utils::*;
use lsp_server::*;
use lsp_types::*;

/// Handles hover request of the language server.
pub fn on_hover_request(context: &Context, request: &Request) -> lsp_server::Response {
    log::info!("on_hover_request request = {:?}", request);
    let parameters = serde_json::from_value::<HoverParams>(request.params.clone())
        .expect("could not deserialize hover request");
    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;
    let fpath = path_concat(
        std::env::current_dir().unwrap().as_path(),
        fpath.as_path(),
    );
    eprintln!(
        "request is hover,fpath:{:?} line:{} col:{}",
        fpath.as_path(),
        line,
        col,
    );

    let mut handler = goto_definition::Handler::new(fpath.clone(), line, col);
    let _ = match context.projects.get_project(&fpath) {
        Some(x) => x,
        None => {
            log::error!("project not found:{:?}", fpath.as_path());
            return Response {
                id: "".to_string().into(),
                result: Some(serde_json::json!({"msg": "No available project"})),
                error: None,
            };
        },
    }
    .run_visitor_for_file(&mut handler, &fpath, false);
    let item = handler.result_item_or_access.clone();
    let hover = item.map(|x| hover_on_item_or_access(&x));
    let hover = hover.map(|x| Hover {
        contents: HoverContents::Scalar(MarkedString::String(x)),
        range: None,
    });
    let r = Response::new_ok(request.id.clone(), serde_json::to_value(hover).unwrap());
    let ret_response = r.clone();
    context
        .connection
        .sender
        .send(Message::Response(r))
        .unwrap();
    ret_response
}

fn hover_on_item_or_access(ia: &ItemOrAccess) -> String {
    let item_hover = |item: &Item| -> String {
        match item {
            Item::MoveBuildInFun(x) => String::from(x.to_notice()),
            Item::SpecBuildInFun(x) => String::from(x.to_notice()),
            Item::Use(_) => "".to_string(),
            _ => {
                // nothing special .
                format!("{}", item)
            }
        }
    };
    match ia {
        ItemOrAccess::Item(item) => item_hover(item),
        ItemOrAccess::Access(access) => match access {
            Access::ApplyType(_, _, ty) => format!("{}", ty),
            Access::ExprVar(_, item) => format!("{}", item.as_ref()),
            Access::ExprAccessChain(_, _, item) => item_hover(item.as_ref()),
            Access::ExprAddressName(_) => String::from(""), // TODO handle this.
            Access::AccessFiled(AccessFiled { to, ty, .. }) => {
                format!("field {}:{}", to.0.value.as_str(), ty)
            }
            Access::KeyWords(x) => format!("keyword {}", *x),
            Access::MacroCall(macro_, _) => format!("macro {}", macro_.to_static_str()),
            Access::Friend(_, _) => String::from(""),
            Access::ApplySchemaTo(_, item) => item_hover(item.as_ref()),
            Access::SpecFor(_, item) => format!("{}", item.as_ref()),
            Access::IncludeSchema(_, _) => String::from(""),
        },
    }
}
