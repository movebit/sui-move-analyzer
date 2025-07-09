// // Copyright (c) The Move Contributors
// // SPDX-License-Identifier: Apache-2.0

use super::{context::*, item::*, project::*, project_context::*, types::ResolvedType};

use crate::utils::{get_path_from_url, path_concat, FileRange, GetPosition, GetPositionStruct};
use lsp_server::*;

use lsp_types::*;
use move_compiler::shared::Identifier;
use move_ir_types::location::Loc;
use std::path::PathBuf;

/// Handles go-to-def request of the language server.
pub fn on_go_to_def_request(
    context: &Context, 
    fpath: PathBuf, 
    pos: lsp_types::Position
) -> serde_json::Value {
    eprintln!("on_go_to_def_request fpath: {:?}, pos: {:?}", fpath, pos);

    let mut handler = Handler::new(
        fpath.clone(), 
        pos.line, 
        pos.character
    );
    let _ = match context.projects.get_project(&fpath) {
        Some(x) => x,
        None => {
            println!("project not found:{:?}", fpath.as_path());
            return serde_json::Value::Null;
        }
    }
    .run_visitor_for_file(&mut handler, &fpath, false);
    let locations = handler.to_locations();
    println!("on_go_to_def_request result: {:?}", locations);
    serde_json::to_value(locations).unwrap()
}

pub(crate) struct Handler {
    /// The file we are looking for.
    pub(crate) filepath: PathBuf,
    pub(crate) line: u32,
    pub(crate) col: u32,
    pub(crate) result: Option<FileRange>,
    /// AccessFiled ... can have this field.
    pub(crate) result2: Option<FileRange>,

    /// result_loc not convert to a FileRange
    /// Current references find depend on this field.
    pub(crate) result_loc: Option<Loc>,

    pub(crate) result_item_or_access: Option<ItemOrAccess>,
}

impl Handler {
    pub(crate) fn new(filepath: impl Into<PathBuf>, line: u32, col: u32) -> Self {
        Self {
            filepath: filepath.into(),
            line,
            col,
            result: None,
            result_loc: None,
            result2: None,
            result_item_or_access: None,
        }
    }

    fn match_loc(&self, loc: &Loc, services: &dyn HandleItemService) -> bool {
        let r = services.convert_loc_range(loc);
        println!("r: {:?}", r);
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
    fn to_locations(&self) -> Vec<Location> {
        let mut ret = Vec::with_capacity(2);
        if let Some(x) = self.result.as_ref() {
            ret.push(x.mk_location());
        }
        if let Some(x) = self.result2.as_ref() {
            ret.push(x.mk_location());
        }
        ret
    }
}

impl ItemOrAccessHandler for Handler {
    fn visit_fun_or_spec_body(&self) -> bool {
        true
    }
    fn handle_item_or_access(
        &mut self,
        services: &dyn HandleItemService,
        _project_context: &ProjectContext,
        item_or_access: &ItemOrAccess,
    ) {
        println!("handle_item_or_access<goto>, item_or_access = {}", item_or_access);
        println!(">> handle_item_or_access<goto>, self.result = {:?}", self.result);
        match item_or_access {
            ItemOrAccess::Item(item) => match item {
                Item::Use(x) => {
                    for x in x.iter() {
                        match x {
                            ItemUse::Module(ItemUseModule {
                                alias,
                                module_ident,
                                s,
                                ..
                            }) => {
                                if self.match_loc(&module_ident.value.module.loc(), services)
                                    || match alias {
                                        Some(alias) => self.match_loc(&alias.0.loc, services),
                                        None => false,
                                    }
                                    || match s {
                                        Some(s) => self.match_loc(&s.loc, services),
                                        _ => false,
                                    }
                                {
                                    if let Some(t) = services.convert_loc_range(&item.def_loc()) {
                                        self.result = Some(t);
                                        self.result_loc = Some(item.def_loc());
                                        self.result_item_or_access = Some(item_or_access.clone());
                                    }
                                }
                            }
                            ItemUse::Item(ItemUseItem {
                                module_ident,
                                name,
                                alias,
                                members,
                                ..
                            }) => {
                                if self.match_loc(&module_ident.value.module.loc(), services) {
                                    let module_loc =
                                        members.as_ref().borrow().name_and_addr.name.loc();
                                    if let Some(t) = services.convert_loc_range(&module_loc) {
                                        self.result = Some(t);
                                        self.result_loc = Some(module_loc);
                                        self.result_item_or_access = Some(item_or_access.clone());
                                        return;
                                    }
                                }

                                if self.match_loc(&name.loc, services)
                                    || match alias {
                                        Some(alias) => self.match_loc(&alias.loc, services),
                                        None => false,
                                    }
                                {
                                    if let Some(t) = services.convert_loc_range(&item.def_loc()) {
                                        self.result = Some(t);
                                        self.result_loc = Some(item.def_loc());
                                        self.result_item_or_access = Some(item_or_access.clone());
                                    }
                                }
                            }
                        }
                    }
                }

                // If Some special add here.
                // Right now default is enough.
                _ => {
                    let loc = item.def_loc();
                    if self.match_loc(&loc, services) {
                        if let Some(t) = services.convert_loc_range(&loc) {
                            self.result = Some(t);
                            self.result_loc = Some(loc);
                            self.result_item_or_access = Some(item_or_access.clone());
                        }
                    }
                }
            },
            ItemOrAccess::Access(access) => match access {
                Access::AccessFiled(AccessFiled { from, to, item, .. }) => {
                    println!("-- handle_item_or_access<goto>, AccessFiled");
                    if self.match_loc(&from.loc(), services) {
                        if let Some(t) = services.convert_loc_range(&to.loc()) {
                            self.result = Some(t);
                            self.result_loc = Some(to.loc());
                            self.result_item_or_access = Some(item_or_access.clone());
                            if let Some(item) = item {
                                self.result2 = services.convert_loc_range(&item.def_loc());
                            }
                        }
                    }
                }
                Access::ExprAccessChain(chain, _, item) if item.is_build_in() => {
                    println!("-- handle_item_or_access<goto>, ExprAccessChain");
                    println!("-- handle_item_or_access<goto>, chain.name = {}", chain.value);
                    if self.match_loc(&chain.loc, services) {
                        if let Some(t) = services.convert_loc_range(&chain.loc) {
                            self.result = Some(t);
                            self.result_item_or_access = Some(item_or_access.clone());
                        }
                    }
                }
                Access::ExprAccessChain(chain,  _, item) => {
                    match chain.value {
                        move_compiler::parser::ast::NameAccessChain_::Single(..) => {
                            println!("-- handle_item_or_access<goto> Single, ExprAccessChain");
                            println!("-- handle_item_or_access<goto> Single, chain.name = {}", chain.value);
                            println!("-- handle_item_or_access<goto> Single, chain.loc = {:?}", services.convert_loc_range(&chain.loc));
                            if self.match_loc(&chain.loc, services) {
                                println!("match_loc true");
                                if let Some(t) = services.convert_loc_range(&item.def_loc()) {
                                    println!("services.convert_loc_range true");
                                    self.result = Some(t);
                                    self.result_item_or_access = Some(item_or_access.clone());
                                }
                            }
                        }
                        _ => {
                            println!("access:{}", access);
                            if let Some((access, def)) = access.access_module() {
                                if self.match_loc(&access, services) {
                                    if let Some(t) = services.convert_loc_range(&def) {
                                        self.result = Some(t);
                                        self.result_loc = Some(def);
                                        self.result_item_or_access = Some(item_or_access.clone());
                                        return;
                                    }
                                }
                            }
                            let locs = access.access_def_loc();
                            if self.match_loc(&locs.0, services) {
                                if let Some(t) = services.convert_loc_range(&locs.1) {
                                    self.result = Some(t);
                                    self.result_loc = Some(locs.1);
                                    self.result_item_or_access = Some(item_or_access.clone());
                                }
                            }
                        }
                    }
                }
                _ => {}
            },
        }
        println!("<< handle_item_or_access<goto>, self.result = {:?}", self.result);
    }

    fn function_or_spec_body_should_visit(&self, range: &FileRange) -> bool {
        
        let a = Self::in_range(self, range);
        println!("function_or_spec_body_should_visit, {}", a);
        a
    }

    fn finished(&self) -> bool {
        self.result.is_some()
    }
}

impl std::fmt::Display for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "goto_definition,file:{:?} line:{} col:{}",
            self.filepath, self.line, self.col
        )
    }
}

impl GetPosition for Handler {
    fn get_position(&self) -> (PathBuf, u32, u32) {
        (self.filepath.clone(), self.line, self.col)
    }
}

/// Handles go-to-def request of the language server
pub fn on_go_to_type_def_request(
    context: &Context, 
    fpath: PathBuf, 
    pos: lsp_types::Position
) {
    println!(
        "on_go_to_type_def_request, fpath: {:?}, pos: {:?}", 
        fpath,
        pos
    );
    // let parameters = serde_json::from_value::<GotoDefinitionParams>(request.params.clone())
    //     .expect("could not deserialize go-to-def request");
    // let fpath = get_path_from_url(&parameters
    //     .text_document_position_params
    //     .text_document
    //     .uri
    // ).unwrap();

    // let loc = parameters.text_document_position_params.position;
    // let line = loc.line;
    // let col = loc.character;
    // let fpath = path_concat(std::env::current_dir().unwrap().as_path(), fpath.as_path());
    // log::info!(
    //     "request is goto type definition,fpath:{:?}  line:{} col:{}",
    //     fpath.as_path(),
    //     line,
    //     col,
    // );

    let mut handler = Handler::new(fpath.clone(), pos.line, pos.character);
    let modules = match context.projects.get_project(&fpath) {
        Some(x) => x,
        None => {
            println!("on_go_to_type_def_request: No available project");
            return;
            // return Response {
            //     id: "".to_string().into(),
            //     result: Some(serde_json::json!({"msg": "No available project"})),
            //     error: None,
            // };
        }
    };
    let _ = modules.run_visitor_for_file(&mut handler, &fpath, false);
    println!("111111111111111");
    fn type_defs(ret: &mut Vec<Location>, ty: &ResolvedType, modules: &super::project::Project) {
        match ty {
            ResolvedType::UnKnown => {}
            ResolvedType::Struct(x, _) => {
                if let Some(r) = modules.convert_loc_range(&x.name.loc()) {
                    ret.push(r.mk_location());
                }
            }
            ResolvedType::BuildInType(_) => {}
            ResolvedType::TParam(name, _) => {
                if let Some(r) = modules.convert_loc_range(&name.loc) {
                    ret.push(r.mk_location());
                }
            }
            ResolvedType::Ref(_, t) => {
                let t = t.as_ref();
                type_defs(ret, t, modules);
            }
            ResolvedType::Unit => {}
            ResolvedType::Multiple(types) => {
                for ty in types.iter() {
                    type_defs(ret, ty, modules);
                }
            }
            ResolvedType::Fun(_) => {
                // TODO
            }
            ResolvedType::Vec(ty) => {
                type_defs(ret, ty.as_ref(), modules);
            }

            ResolvedType::Range => {}
            ResolvedType::Lambda { args, ret_ty } => {
                for a in args.iter().chain(vec![ret_ty.as_ref()]) {
                    type_defs(ret, a, modules);
                }
            }
        }
    }
    fn item_type_defs(ret: &mut Vec<Location>, x: &Item, modules: &super::project::Project) {
        println!("item_type_defs");
        match x {
            Item::Var { ty, .. } | Item::Parameter(_, ty) => {
                type_defs(ret, ty, modules);
            }
            Item::Field(_, ty) => {
                println!("Item::Field");
                type_defs(ret, ty, modules);
            }
            Item::Struct(x) => {
                for x in x.fields.iter() {
                    type_defs(ret, &x.1, modules);
                }
            }
            _ => {}
        }
    }
    let mut locations = vec![];
    println!("222222222222222");
    match &handler.result_item_or_access {
        Some(x) => match x {
            ItemOrAccess::Item(x) => item_type_defs(&mut locations, x, modules),
            ItemOrAccess::Access(x) => match x {
                Access::ExprAccessChain(_, _, item) => {
                    println!("Access::ExprAccessChain");
                    item_type_defs(&mut locations, item.as_ref(), modules);
                }
                Access::ExprVar(_, item) => {
                    println!("Access::ExprVar");
                    item_type_defs(&mut locations, item.as_ref(), modules);
                }
                Access::ApplyType(_, _, ty) => {
                    println!("Access::ApplyType");
                    type_defs(&mut locations, ty, modules);
                }
                _ => {println!("Access::None");}
            },
        },
        None => {println!("handler.result_item_or_access is None")}
    };
    println!("3333333333");
    println!("goto definition result: {:?}", locations)
    // let r = Response::new_ok(
    //     request.id.clone(),
    //     serde_json::to_value(GotoDefinitionResponse::Array(locations)).unwrap(),
    // );
    // let ret_response = r.clone();
    // context
    //     .connection
    //     .sender
    //     .send(Message::Response(r))
    //     .unwrap();
    // ret_response
}
