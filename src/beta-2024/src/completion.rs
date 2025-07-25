// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{item::*, project::*, project_context::*, types::ResolvedType, utils::*};
use crate::context::Context;
use lsp_server::*;
use lsp_types::*;
use move_compiler::{
    parser::{
        ast::{LeadingNameAccess_, ModuleName},
        keywords::{CONTEXTUAL_KEYWORDS, KEYWORDS, PRIMITIVE_TYPES},
    },
    shared::{Identifier, Name},
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;
use std::{
    collections::{HashMap, HashSet},
    path::*,
    vec,
};

/// Constructs an `lsp_types::CompletionItem` with the given `label` and `kind`.
fn completion_item(label: &str, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(kind),
        ..Default::default()
    }
}

/// Return a list of completion items corresponding to each one of Move's keywords.
///
/// Currently, this does not filter keywords out based on whether they are valid at the completion
/// request's cursor position, but in the future it ought to. For example, this function returns
/// all specification language keywords, but in the future it should be modified to only do so
/// within a spec block.
fn keywords() -> Vec<CompletionItem> {
    KEYWORDS
        .iter()
        .chain(CONTEXTUAL_KEYWORDS.iter())
        .chain(PRIMITIVE_TYPES.iter())
        .map(|label| {
            let kind = if label == &"copy" || label == &"move" {
                CompletionItemKind::OPERATOR
            } else {
                CompletionItemKind::KEYWORD
            };
            completion_item(label, kind)
        })
        .collect()
}

/// Return a list of completion items of Move's primitive types
fn primitive_types() -> Vec<CompletionItem> {
    PRIMITIVE_TYPES
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::KEYWORD))
        .collect()
}

/// Return a list of completion items corresponding to each one of Move's builtin functions.
fn move_builtin_funs() -> Vec<CompletionItem> {
    enum_iterator::all::<MoveBuildInFun>()
        .collect::<Vec<_>>()
        .iter()
        .map(|label| completion_item(label.to_static_str(), CompletionItemKind::FUNCTION))
        .collect()
}

fn spec_builtin_funs() -> Vec<CompletionItem> {
    enum_iterator::all::<SpecBuildInFun>()
        .collect::<Vec<_>>()
        .iter()
        .map(|label| completion_item(label.to_static_str(), CompletionItemKind::FUNCTION))
        .collect()
}

fn all_intrinsic() -> Vec<CompletionItem> {
    let mut all = move_builtin_funs();
    all.extend(spec_builtin_funs().into_iter());
    all.extend(primitive_types().into_iter());
    all.extend(keywords().into_iter());
    all.extend(sui_framework_completion().into_iter());
    all
}

fn sui_framework_completion() -> Vec<CompletionItem> {
    let mut ret = Vec::new();
    let x = String::from(
        r#"
fun init(ctx: &mut sui::tx_context::TxContext) {

}"#,
    );
    ret.push(CompletionItem {
        label: String::from("init"),
        kind: Some(CompletionItemKind::FUNCTION),
        insert_text: Some(x.clone()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });
    ret.push(CompletionItem {
        label: String::from("fun init"),
        kind: Some(CompletionItemKind::FUNCTION),
        insert_text: Some(x),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });
    ret
}

/// Sends the given connection a response to a completion request.
///
/// The completions returned depend upon where the user's cursor is positioned.
pub fn on_completion_request(context: &Context, request: &Request) {
    let parameters = serde_json::from_value::<CompletionParams>(request.params.clone())
        .expect("could not deserialize references request");
    let fpath = parameters
        .text_document_position
        .text_document
        .uri
        .to_file_path()
        .unwrap();

    let loc = parameters.text_document_position.position;
    let line = loc.line;
    let col = loc.character;
    eprintln!(
        "\n========== completion_request ===========\nfile path = {:?}, line, col = {}, {}\n",
        fpath.as_path(),
        line,
        col
    );
    let fpath = path_concat(std::env::current_dir().unwrap().as_path(), fpath.as_path());

    let mut handler = Handler::new(fpath.clone(), line, col);
    let _ = match context.projects.get_project(&fpath) {
        Some(x) => x,
        None => {
            log::error!("completion_request Could not find project");
            return;
        }
    }
    .run_visitor_for_file(&mut handler, &fpath, false);
    let mut result = handler.result.unwrap_or_default();
    if result.is_empty() && !handler.completion_on_def {
        result = all_intrinsic();
    }
    let ret = Some(CompletionResponse::Array(result));
    let r = Response::new_ok(request.id.clone(), serde_json::to_value(ret).unwrap());
    context
        .connection
        .sender
        .send(Message::Response(r))
        .unwrap();
    eprintln!("completion_request Success.\n=================\n");
}

pub(crate) struct Handler {
    /// The file we are looking for.
    pub(crate) filepath: PathBuf,
    pub(crate) line: u32,
    pub(crate) col: u32,
    pub(crate) result: Option<Vec<CompletionItem>>,
    completion_on_def: bool,
}

impl Handler {
    pub(crate) fn new(filepath: impl Into<PathBuf>, line: u32, col: u32) -> Self {
        Self {
            filepath: filepath.into(),
            line,
            col,
            result: None,
            completion_on_def: false,
        }
    }
    ///  match loc
    fn match_loc(&self, loc: &Loc, services: &dyn HandleItemService) -> bool {
        let r = services.convert_loc_range(loc);
        match &r {
            Some(r) => GetPositionStruct::in_range(
                &GetPositionStruct {
                    fpath: self.filepath.clone(),
                    line: self.line,
                    col: self.col,
                },
                r,
            ),
            None => false,
        }
    }
}

impl ItemOrAccessHandler for Handler {
    fn visit_fun_or_spec_body(&self) -> bool {
        true
    }
    fn handle_item_or_access(
        &mut self,
        services: &dyn HandleItemService,
        project_context: &ProjectContext,
        item_or_access: &ItemOrAccess,
    ) {
        let push_items = |visitor: &mut Handler, items: &Vec<Item>| {
            if visitor.result.is_none() {
                visitor.result = Some(vec![]);
            }
            let x: Vec<_> = items.iter().map(item_to_completion_item).collect();
            x.into_iter().for_each(|x| {
                if let Some(x) = x {
                    visitor.result.as_mut().unwrap().push(x);
                }
            });
        };
        let push_addr_spaces = |visitor: &mut Handler,
                                items: &HashSet<AddressSpace>,
                                project_context: &ProjectContext| {
            if visitor.result.is_none() {
                visitor.result = Some(vec![]);
            }

            let items: HashSet<_> = items
                .clone()
                .into_iter()
                .filter(|x| {
                    let addr = match *x {
                        AddressSpace::Addr(addr) => addr,
                        AddressSpace::Name(name) => services.name_2_addr(name),
                    };
                    !project_context.collect_modules(&addr).is_empty()
                })
                .collect();

            let x = name_spaces_to_completion_items(&items, true);
            x.into_iter()
                .for_each(|x| visitor.result.as_mut().unwrap().push(x));
        };

        // just like push_addr_spaces
        // bu only push names.
        let push_addr_spaces_names = |visitor: &mut Handler, items: &HashSet<AddressSpace>| {
            if visitor.result.is_none() {
                visitor.result = Some(vec![]);
            }
            let x = name_spaces_to_completion_items(items, false);
            x.into_iter()
                .for_each(|x| visitor.result.as_mut().unwrap().push(x));
        };

        let push_completion_items = |visitor: &mut Handler, items: Vec<CompletionItem>| {
            if visitor.result.is_none() {
                visitor.result = Some(vec![]);
            }

            items
                .into_iter()
                .for_each(|x| visitor.result.as_mut().unwrap().push(x));
        };
        let push_fields = |visitor: &mut Handler, items: &HashMap<Symbol, (Name, ResolvedType)>| {
            if visitor.result.is_none() {
                visitor.result = Some(vec![]);
            }
            let x: Vec<_> = fields_2_completion_items(items);
            x.into_iter()
                .for_each(|x| visitor.result.as_mut().unwrap().push(x));
        };
        let push_module_names = |visitor: &mut Handler, items: &Vec<ModuleName>| {
            if visitor.result.is_none() {
                visitor.result = Some(vec![]);
            }
            let x: Vec<_> = module_names_2_completion_items(items);
            x.into_iter()
                .for_each(|x| visitor.result.as_mut().unwrap().push(x));
        };
        log::trace!("completion access:{}", item_or_access);
        match item_or_access {
            ItemOrAccess::Item(item) => {
                match item {
                    Item::Use(x) => {
                        for x in x.iter().rev() {
                            match x {
                                ItemUse::Module(ItemUseModule {
                                    module_ident,
                                    alias,
                                    ..
                                }) => {
                                    let addr = match &module_ident.value.address.value {
                                        LeadingNameAccess_::AnonymousAddress(addr) => {
                                            addr.into_inner()
                                        }
                                        LeadingNameAccess_::Name(name)
                                        | LeadingNameAccess_::GlobalAddress(name) => {
                                            services.name_2_addr(name.value)
                                        }
                                    };
                                    let whole_loc = Loc::new(
                                        module_ident.loc.file_hash(),
                                        module_ident.loc.start(),
                                        if let Some(alias) = alias {
                                            alias.loc().end()
                                        } else {
                                            module_ident.loc.end()
                                        },
                                    );
                                    if self.match_loc(&module_ident.value.address.loc, services) {
                                        let items = services.get_all_addrs(project_context);
                                        push_addr_spaces(self, &items, project_context);
                                    } else if self
                                        .match_loc(&module_ident.value.module.loc(), services)
                                    {
                                        let items = project_context.collect_modules(&addr);
                                        push_module_names(self, &items);
                                    } else if self.match_loc(&whole_loc, services) {
                                        let items = project_context.collect_modules_items(
                                            &addr,
                                            module_ident.value.module.0.value,
                                            |x| match x {
                                                // top level can only have const as expr.
                                                Item::Fun(_) => true,
                                                Item::Struct(_) | Item::StructNameRef(_) => true,
                                                Item::SpecSchema(_, _) => true,
                                                _ => false,
                                            },
                                        );
                                        push_items(self, &items);
                                    }
                                }
                                ItemUse::Item(ItemUseItem {
                                    module_ident, name, ..
                                }) => {
                                    let addr = match &module_ident.value.address.value {
                                        LeadingNameAccess_::AnonymousAddress(addr) => {
                                            addr.into_inner()
                                        }
                                        LeadingNameAccess_::Name(name)
                                        | LeadingNameAccess_::GlobalAddress(name) => {
                                            services.name_2_addr(name.value)
                                        }
                                    };
                                    if self.match_loc(&module_ident.value.address.loc, services) {
                                        let items = services.get_all_addrs(project_context);
                                        push_addr_spaces(self, &items, project_context);
                                    } else if self
                                        .match_loc(&module_ident.value.module.loc(), services)
                                    {
                                        let items = project_context.collect_modules(&addr);
                                        push_module_names(self, &items);
                                    } else if self.match_loc(&name.loc, services) {
                                        let items = project_context.collect_modules_items(
                                            &addr,
                                            module_ident.value.module.0.value,
                                            |x| match x {
                                                // top level can only have const as expr.
                                                Item::Fun(_) => true,
                                                Item::Struct(_) | Item::StructNameRef(_) => true,
                                                Item::SpecSchema(_, _) => true,
                                                _ => false,
                                            },
                                        );
                                        push_items(self, &items);
                                        push_completion_items(
                                            self,
                                            vec![CompletionItem {
                                                label: "Self".to_string(),
                                                kind: Some(CompletionItemKind::KEYWORD),
                                                ..Default::default()
                                            }],
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        if self.match_loc(&item.def_loc(), services) {
                            self.completion_on_def = true;
                        }
                    }
                }
            }
            ItemOrAccess::Access(access) => {
                match access {
                    Access::ApplyType(chain, _, _) => match &chain.value {
                        move_compiler::parser::ast::NameAccessChain_::Single(path_entry) => {
                            let name = path_entry.name;
                            if self.match_loc(&name.loc, services) {
                                push_items(self, &project_context.collect_all_type_items());
                                // Possible all namespaces.
                                push_addr_spaces(
                                    self,
                                    &services.get_all_addrs(project_context),
                                    project_context,
                                );
                            }
                        }
                        move_compiler::parser::ast::NameAccessChain_::Path(name_path) => {
                            let space = &name_path.root.name;
                            if self.match_loc(&space.loc, services) {
                                let items = project_context.collect_imported_modules();
                                push_items(self, &items);
                                push_addr_spaces(
                                    self,
                                    &services.get_all_addrs(project_context),
                                    project_context,
                                );
                            } else {
                                for entry in name_path.entries.iter() {
                                    if self.match_loc(&entry.name.loc, services) {
                                        let addr = match &space.value {
                                            LeadingNameAccess_::Name(name)
                                            | LeadingNameAccess_::GlobalAddress(name) => {
                                                services.name_2_addr(name.value)
                                            }
                                            LeadingNameAccess_::AnonymousAddress(addr) => {
                                                addr.into_inner()
                                            }
                                        };
                                        let items = project_context.collect_modules(&addr);
                                        if !items.is_empty() {
                                            // This is a reasonable guess.
                                            // In situation like global<std::>
                                            // even this is access NameAccessChain_::Two
                                            // this is still can be unfinished NameAccessChain_::Three.
                                            push_module_names(self, &items);
                                        } else {
                                            let items = project_context.collect_use_module_items(
                                                space,
                                                |x| {
                                                    matches!(
                                                        x,
                                                        Item::Struct(_) | Item::StructNameRef(_)
                                                    )
                                                },
                                            );
                                            push_items(self, &items);
                                            let addr = match space.value {
                                                LeadingNameAccess_::AnonymousAddress(addr) => {
                                                    addr.into_inner()
                                                }
                                                LeadingNameAccess_::Name(name)
                                                | LeadingNameAccess_::GlobalAddress(name) => {
                                                    services.name_2_addr(name.value)
                                                }
                                            };
                                            push_module_names(
                                                self,
                                                &project_context.collect_modules(&addr),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Access::ExprVar(var, _) => {
                        if self.match_loc(&var.loc(), services) {
                            let items = project_context.collect_items(|x| {
                                matches!(x, Item::Var { .. } | Item::Parameter(_, _))
                            });
                            push_items(self, &items);
                        }
                    }
                    Access::ExprAccessChain(chain, _, _) | Access::MacroCall(_, chain) => {
                        match &chain.value {
                            move_compiler::parser::ast::NameAccessChain_::Single(path_entry) => {
                                let x = path_entry.name;
                                if self.match_loc(&x.loc, services) {
                                    push_items(
                                        self,
                                        &project_context.collect_items(|x| {
                                            matches!(
                                                x,
                                                Item::Var { .. }
                                                    | Item::Parameter(_, _)
                                                    | Item::Use(_)
                                                    | Item::SpecSchema(_, _)
                                                    | Item::Fun(_)
                                                    | Item::Struct(_)
                                                    | Item::Const(_)
                                                    | Item::MoveBuildInFun(_)
                                                    | Item::SpecBuildInFun(_)
                                                    | Item::SpecConst(_)
                                            )
                                        }),
                                    );
                                    let items = services.get_all_addrs(project_context);
                                    push_addr_spaces(self, &items, project_context);
                                    push_completion_items(self, keywords());
                                }
                            }

                            move_compiler::parser::ast::NameAccessChain_::Path(name_path) => {
                                let leading_name_access = name_path.root.name;
                                if self.match_loc(&leading_name_access.loc, services) {
                                    let items = project_context.collect_imported_modules();
                                    push_items(self, &items);
                                    let items = services.get_all_addrs(project_context);
                                    push_addr_spaces(self, &items, project_context);
                                } else {
                                    for entry in name_path.entries.iter() {
                                        if self.match_loc(&entry.name.loc, services) {
                                            let addr = match &leading_name_access.value {
                                                LeadingNameAccess_::Name(name)
                                                | LeadingNameAccess_::GlobalAddress(name) => {
                                                    services.name_2_addr(name.value)
                                                }
                                                LeadingNameAccess_::AnonymousAddress(addr) => {
                                                    addr.into_inner()
                                                }
                                            };
                                            let items = project_context.collect_modules(&addr);
                                            if !items.is_empty() {
                                                // This is a reasonable guess.
                                                // In situation like global<std::>
                                                // even this is access NameAccessChain_::Two
                                                // this is still can be unfinished NameAccessChain_::Three.
                                                push_module_names(self, &items);
                                            } else {
                                                let items = project_context
                                                    .collect_use_module_items(
                                                        &leading_name_access,
                                                        |x| {
                                                            matches!(
                                                                x,
                                                                Item::Fun(_)
                                                                    | Item::SpecSchema(_, _)
                                                            )
                                                        },
                                                    );
                                                push_items(self, &items);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Access::AccessFiled(AccessFiled {
                        from, all_fields, ..
                    }) => {
                        if self.match_loc(&from.loc(), services) {
                            push_fields(self, all_fields);
                        }
                    }
                    Access::KeyWords(_) => {}
                    Access::Friend(chain, _) => match &chain.value {
                        move_compiler::parser::ast::NameAccessChain_::Single(path_entry) => {
                            let name = path_entry.name;
                            if self.match_loc(&name.loc, services) {
                                let items = services.get_all_addrs(project_context);
                                push_addr_spaces(self, &items, project_context);
                            }
                        }
                        move_compiler::parser::ast::NameAccessChain_::Path(name_path) => {
                            // addr, name
                            let addr = &name_path.root.name;
                            if self.match_loc(&addr.loc, services) {
                                let items = services.get_all_addrs(project_context);
                                push_addr_spaces(self, &items, project_context);
                            } else {
                                for entry in name_path.entries.iter() {
                                    if self.match_loc(&entry.name.loc, services) {
                                        let addr = match &addr.value {
                                            LeadingNameAccess_::AnonymousAddress(addr) => {
                                                addr.into_inner()
                                            }
                                            LeadingNameAccess_::Name(name)
                                            | LeadingNameAccess_::GlobalAddress(name) => {
                                                services.name_2_addr(name.value)
                                            }
                                        };
                                        let items = project_context.collect_modules(&addr);
                                        push_module_names(self, &items);
                                    }
                                }
                            }
                        }
                    },
                    Access::IncludeSchema(x, _) => {
                        if self.match_loc(&x.loc, services) {
                            let items = project_context.collect_all_spec_schema();
                            push_items(self, &items);
                        }
                    }
                    Access::ApplySchemaTo(x, _) => {
                        if self.match_loc(&x.loc, services) {
                            let items = project_context.collect_all_spec_target();
                            push_items(self, &items);
                        }
                    }
                    Access::ExprAddressName(var) => {
                        if self.match_loc(&var.loc, services) {
                            push_addr_spaces_names(self, &services.get_all_addrs(project_context));
                        }
                    }
                    Access::SpecFor(name, _) => {
                        if self.match_loc(&name.loc, services) {
                            let items = project_context.collect_all_spec_target();
                            push_items(self, &items);
                        }
                    }
                };
            }
        }
    }

    fn function_or_spec_body_should_visit(&self, range: &FileRange) -> bool {
        Self::in_range(self, range)
    }
    fn finished(&self) -> bool {
        self.result.is_some() || self.completion_on_def
    }
}

impl std::fmt::Display for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "completion,file:{:?} line:{} col:{}",
            self.filepath, self.line, self.col
        )
    }
}

impl GetPosition for Handler {
    fn get_position(&self) -> (PathBuf, u32 /* line */, u32 /* col */) {
        (self.filepath.clone(), self.line, self.col)
    }
}

#[allow(dead_code)]
fn pragma_property_completion_items() -> Vec<CompletionItem> {
    let ret: Vec<CompletionItem> = vec![
        CompletionItem {
            label: String::from("verify = true"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: String::from("intrinsic"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: String::from("timeout=1000"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: String::from("verify_duration_estimate=1000"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: String::from("seed"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: String::from("aborts_if_is_strict"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: String::from("opaque"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
        CompletionItem {
            label: String::from("aborts_if_is_partial"),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        },
    ];
    ret
}

fn fields_2_completion_items(x: &HashMap<Symbol, (Name, ResolvedType)>) -> Vec<CompletionItem> {
    let mut ret = Vec::new();
    x.values()
        .for_each(|(name, ty)| ret.push(field_2_completion_item(name, ty)));
    ret
}

fn field_2_completion_item(field: &Name, ty: &ResolvedType) -> CompletionItem {
    CompletionItem {
        label: String::from(field.value.as_str()),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(format!("field {}:{}", field.value.as_str(), ty)),
        ..Default::default()
    }
}

fn module_names_2_completion_items(x: &Vec<ModuleName>) -> Vec<CompletionItem> {
    let mut ret = Vec::with_capacity(x.len());
    for xx in x.iter() {
        ret.push(CompletionItem {
            label: String::from(xx.0.value.as_str()),
            kind: Some(CompletionItemKind::MODULE),
            ..Default::default()
        })
    }
    ret
}

fn name_spaces_to_completion_items(
    x: &HashSet<AddressSpace>,
    accept_addr: bool,
) -> Vec<CompletionItem> {
    let mut ret = Vec::with_capacity(x.len());
    for space in x.iter() {
        match space {
            AddressSpace::Addr(addr) => {
                if accept_addr {
                    ret.push(CompletionItem {
                        label: format!("0x{}", addr.short_str_lossless()),
                        kind: Some(ADDR_COMPLETION_KIND),
                        ..Default::default()
                    });
                }
            }
            AddressSpace::Name(name) => {
                ret.push(CompletionItem {
                    label: String::from(name.as_str()),
                    kind: Some(ADDR_COMPLETION_KIND), // TODO this should be a module,should be a namespace.
                    ..Default::default()
                });
            }
        }
    }

    ret
}

const ADDR_COMPLETION_KIND: CompletionItemKind = CompletionItemKind::FOLDER;

fn item_to_completion_item(item: &Item) -> Option<CompletionItem> {
    let x = match item {
        Item::Parameter(var, _) => CompletionItem {
            label: String::from(var.0.value.as_str()),
            kind: Some(CompletionItemKind::VARIABLE),
            ..Default::default()
        },
        Item::Use(x) => {
            if let Some(x) = x.iter().rev().next() {
                match x {
                    ItemUse::Module(ItemUseModule {
                        module_ident,
                        alias,
                        ..
                    }) => {
                        return Some(CompletionItem {
                            label: if let Some(alias) = alias {
                                String::from(alias.value().as_str())
                            } else {
                                String::from(module_ident.value.module.value().as_str())
                            },
                            kind: Some(CompletionItemKind::MODULE),
                            ..Default::default()
                        });
                    }
                    ItemUse::Item(ItemUseItem {
                        alias,
                        name,
                        members,
                        ..
                    }) => {
                        return Some(CompletionItem {
                            label: String::from(if let Some(alias) = alias {
                                alias.value.as_str()
                            } else {
                                name.value.as_str()
                            }),
                            kind: {
                                let name = if let Some(alias) = alias {
                                    alias.value
                                } else {
                                    name.value
                                };
                                let item_kind = |item: &Item| -> CompletionItemKind {
                                    match item {
                                        Item::Struct(_) => CompletionItemKind::STRUCT,
                                        Item::Fun(_) => CompletionItemKind::FUNCTION,
                                        _ => CompletionItemKind::TEXT,
                                    }
                                };
                                Some(|| -> CompletionItemKind {
                                    if let Some(item) =
                                        members.as_ref().borrow().module.items.get(&name)
                                    {
                                        item_kind(item)
                                    } else if let Some(item) =
                                        members.as_ref().borrow().spec.items.get(&name)
                                    {
                                        item_kind(item)
                                    } else {
                                        CompletionItemKind::TEXT
                                    }
                                }())
                            },
                            ..Default::default()
                        })
                    }
                }
            }
            return None;
        }

        Item::Const(ItemConst { name, .. }) | Item::SpecConst(ItemConst { name, .. }) => {
            CompletionItem {
                label: String::from(name.0.value.as_str()),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some(format!("{}", item)),
                ..Default::default()
            }
        }

        Item::Var { var: name, .. } => CompletionItem {
            label: String::from(name.0.value.as_str()),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("{}", item)),
            ..Default::default()
        },
        Item::Field(field, _) => CompletionItem {
            label: String::from(field.0.value.as_str()),
            detail: Some(format!("{}", item)),
            kind: Some(CompletionItemKind::FIELD),
            ..Default::default()
        },
        Item::Struct(x) => CompletionItem {
            label: String::from(x.name.0.value.as_str()),
            detail: Some(format!("{}", item)),
            kind: Some(CompletionItemKind::STRUCT),
            ..Default::default()
        },
        Item::StructNameRef(ItemStructNameRef { name, .. }) => CompletionItem {
            label: String::from(name.0.value.as_str()),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(format!("{}", item)),
            ..Default::default()
        },
        Item::Fun(x) => CompletionItem {
            label: String::from(x.name.0.value.as_str()),
            detail: Some(format!("{}", item)),
            kind: Some(CompletionItemKind::FUNCTION),
            ..Default::default()
        },
        Item::BuildInType(x) => CompletionItem {
            label: String::from(x.to_static_str()),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(format!("{}", item)),
            ..Default::default()
        },
        Item::TParam(name, _) => CompletionItem {
            label: String::from(name.value.as_str()),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some(format!("{}", item)),
            ..Default::default()
        },
        Item::SpecSchema(name, _) => CompletionItem {
            label: String::from(name.value.as_str()),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(format!("{}", item)),
            ..Default::default()
        },
        Item::ModuleName(_) => {
            // TODO.
            return None;
        }
        Item::Dummy => {
            return None;
        }
        Item::MoveBuildInFun(name) => CompletionItem {
            label: String::from(name.to_static_str()),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(name.to_notice().to_string()),
            ..Default::default()
        },
        Item::SpecBuildInFun(name) => CompletionItem {
            label: String::from(name.to_static_str()),
            detail: Some(name.to_notice().to_string()),
            kind: Some(CompletionItemKind::FUNCTION),
            ..Default::default()
        },
    };
    Some(x)
}
