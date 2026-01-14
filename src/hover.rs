// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::context::Context;
use super::goto_definition;
use super::item::*;
use super::utils::*;
use lsp_server::*;
use lsp_types::*;
use std::path::PathBuf;

/// Handles hover request of the language server.
pub fn on_hover_request(
    context: &Context,
    fpath: PathBuf,
    pos: lsp_types::Position,
) -> serde_json::Value {
    let line = pos.line;
    let col = pos.character;
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
            println!("project not found:{:?}", fpath.as_path());
            return serde_json::Value::Null;
        }
    }
    .run_visitor_for_file(&mut handler, &fpath, false);
    let item = handler.result_item_or_access.clone();
    let hover = item.map(|x| hover_on_item_or_access(&x));
    let hover = hover.map(|x| Hover {
        contents: HoverContents::Scalar(MarkedString::String(x)),
        range: None,
    });
    serde_json::to_value(hover).unwrap()
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
