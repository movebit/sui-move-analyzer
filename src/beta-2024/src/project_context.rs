// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{item::*, project::*, scope::*, types::*, utils::*};
use move_command_line_common::files::FileHash;
use move_compiler::{parser::ast::*, shared::Identifier};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    borrow::BorrowMut,
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

#[derive(Clone)]
pub struct ProjectContext {
    scopes: Rc<RefCell<Vec<Scope>>>,
    pub(crate) addresses: RefCell<Addresses>,
    pub(crate) addr_and_name: RefCell<AddrAndModuleName>,
    pub(crate) access_env: Cell<AccessEnv>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AccessEnv {
    Move,
    Test,
    Spec,
}
impl ProjectContext {
    pub(crate) fn clear_scopes_and_addresses(&self) {
        let d = Self::default();
        *self.scopes.as_ref().borrow_mut() = d.scopes.as_ref().borrow().clone();
        *self.addresses.borrow_mut() = Default::default();
    }
}

impl Default for ProjectContext {
    fn default() -> Self {
        let x = Self {
            scopes: Default::default(),
            addresses: Default::default(),
            addr_and_name: RefCell::new(AddrAndModuleName {
                addr: *ERR_ADDRESS,
                name: ModuleName(Spanned {
                    loc: Loc::new(FileHash::empty(), 0, 0),
                    value: Symbol::from("_"),
                }),
            }),
            access_env: Cell::new(Default::default()),
        };
        let s = Scope::default();
        x.scopes.as_ref().borrow_mut().push(s);
        x.enter_build_in();
        x
    }
}

impl Default for AccessEnv {
    fn default() -> Self {
        Self::Move
    }
}

impl AccessEnv {
    pub(crate) fn is_test(self) -> bool {
        self == Self::Test
    }
    pub(crate) fn is_spec(self) -> bool {
        self == Self::Spec
    }
}

impl ProjectContext {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    /// try fix a local var type
    /// for syntax like
    /// ```move
    /// let x ;
    /// ...
    /// x = 1  // fix can happen here.
    /// ...
    /// ```
    /// This function also return a option lambda expr associate with
    /// like syntax
    /// ```move
    /// let add : |u8 , u8| u8
    /// add = |x , y | x + y
    /// ```
    #[must_use]
    pub(crate) fn try_fix_local_var_ty(
        &self,
        name: Symbol,
        tye: &ResolvedType,
    ) -> Option<LambdaExp> {
        let mut b = self.scopes.as_ref().borrow_mut();
        let mut ret = None;
        {
            let mut fixed = false;
            b.iter_mut().rev().for_each(|x| {
                if !fixed {
                    if let Some(item) = x.items.get_mut(&name) {
                        match item {
                            Item::Var { ty, .. } | Item::Parameter(_, ty) => {
                                if ty.is_err() {
                                    *ty = tye.clone();
                                    fixed = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            });
        }
        {
            let mut fixed = false;
            b.iter_mut().rev().for_each(|x| {
                if !fixed {
                    if let Some(Item::Var { ty, lambda, .. }) = x.items.get_mut(&name) {
                        if ty.is_err() {
                            *ty = tye.clone();
                            fixed = true;
                            // try visit lambda expr
                            ret = lambda.clone();
                        }
                    }
                }
            });
        }
        ret
    }

    pub(crate) fn set_current_addr_and_module_name(&self, addr: AccountAddress, name: Symbol) {
        self.addr_and_name.borrow_mut().addr = addr;
        self.addr_and_name.borrow_mut().name = ModuleName(Spanned {
            loc: Loc::new(FileHash::empty(), 0, 0),
            value: name,
        });
    }

    pub(crate) fn module_is_test(&self, addr: AccountAddress, name: Symbol) -> Option<bool> {
        Some(
            self.addresses
                .borrow()
                .address
                .get(&addr)?
                .modules
                .get(&name)?
                .as_ref()
                .borrow()
                .is_test,
        )
    }

    pub(crate) fn get_current_addr_and_module_name(&self) -> AddrAndModuleName {
        self.addr_and_name.borrow().clone()
    }

    pub(crate) fn set_up_module(
        &self,
        addr: AccountAddress,
        module_name: ModuleName,
        is_test: bool,
    ) {
        log::info!(
            "set up module,addr:0x{:?} module_name:{:?}",
            addr.short_str_lossless(),
            module_name
        );
        if self.addresses.borrow().address.get(&addr).is_none() {
            self.addresses
                .borrow_mut()
                .address
                .insert(addr, Default::default());
        }
        let name_and_addr = AddrAndModuleName {
            name: module_name,
            addr,
        };

        if let Some(scope) = self
            .addresses
            .borrow_mut()
            .address
            .get_mut(&addr)
            .unwrap()
            .modules
            .get_mut(&module_name.0.value)
        {
            scope.as_ref().borrow_mut().name_and_addr = name_and_addr;
            scope.as_ref().borrow_mut().friends = Default::default();
            return;
        }

        self.addresses
            .borrow_mut()
            .address
            .get_mut(&addr)
            .unwrap()
            .modules
            .insert(
                module_name.0.value,
                Rc::new(RefCell::new(ModuleScope::new(name_and_addr, is_test))),
            );
    }

    pub(crate) fn insert_friend(
        &self,
        addr: AccountAddress,
        module_name: Symbol,
        friend: (AccountAddress, Symbol),
    ) {
        self.visit_address(|x| {
            x.address
                .get(&addr)
                .unwrap()
                .modules
                .get(&module_name)
                .unwrap()
                .as_ref()
                .borrow_mut()
                .friends
                .insert(friend)
        });
    }

    pub(crate) fn query_item<R>(
        &self,
        addr: AccountAddress,
        module_name: Symbol,
        item_name: Symbol,
        x: impl FnOnce(&Item) -> R,
    ) -> Option<R> {
        Some(x(self
            .addresses
            .borrow()
            .address
            .get(&addr)?
            .modules
            .get(&module_name)?
            .as_ref()
            .borrow()
            .module
            .items
            .get(&item_name)?))
    }

    pub(crate) fn enter_build_in(&self) {
        self.scopes
            .as_ref()
            .borrow_mut()
            .first_mut()
            .unwrap()
            .enter_build_in();
    }
    pub(crate) fn enter_scope<R>(&self, call_back: impl FnOnce(&ProjectContext) -> R) -> R {
        let s = Scope::default();
        self.scopes.as_ref().borrow_mut().push(s);
        let _guard = ScopesGuarder::new(self.clone());
        call_back(self)
    }

    // Enter
    pub(crate) fn enter_item(
        &self,
        convert_loc: &dyn ConvertLoc,
        name: Symbol,
        item: impl Into<Item>,
    ) {
        let item = item.into();
        let loc = item.def_loc();
        let loc = convert_loc
            .convert_loc_range(&loc)
            .unwrap_or_else(FileRange::unknown);
        log::trace!("{}", loc);
        log::trace!("enter scope name:{:?} item:{}", name, item);
        self.scopes
            .as_ref()
            .borrow_mut()
            .last_mut()
            .unwrap()
            .enter_item(name, item);
    }
    pub(crate) fn enter_use_item(
        &self,
        convert_loc: &dyn ConvertLoc,
        name: Symbol,
        item: impl Into<Item>,
    ) {
        let item = item.into();
        let loc = item.def_loc();
        let loc = convert_loc
            .convert_loc_range(&loc)
            .unwrap_or_else(FileRange::unknown);
        log::trace!("{}", loc);
        log::trace!("enter scope name:{:?} item:{}", name, item);
        self.scopes
            .as_ref()
            .borrow_mut()
            .last_mut()
            .unwrap()
            .enter_use_item(name, item);
    }

    pub(crate) fn enter_types(
        &self,
        convert_loc: &dyn ConvertLoc,
        name: Symbol,
        item: impl Into<Item>,
    ) {
        let item = item.into();
        let loc = item.def_loc();
        let loc = convert_loc
            .convert_loc_range(&loc)
            .unwrap_or_else(FileRange::unknown);
        log::trace!("{}", loc);
        log::trace!("enter scope name:{:?} item:{}", name, item);
        self.scopes
            .as_ref()
            .borrow_mut()
            .last_mut()
            .unwrap()
            .enter_types(name, item);
    }

    pub(crate) fn enter_top_item(
        &self,
        convert_loc: &dyn ConvertLoc,
        address: AccountAddress,
        module: Symbol,
        item_name: Symbol,
        item: impl Into<Item>,
        is_spec_module: bool,
    ) {
        let item: Item = item.into();
        let loc = item.def_loc();
        let loc = convert_loc
            .convert_loc_range(&loc)
            .unwrap_or_else(FileRange::unknown);
        log::trace!("{}", loc);
        log::trace!(
            "enter top scope address:0x{:?} module:{:?} name:{:?} item:{}",
            address.short_str_lossless(),
            module,
            item_name,
            item,
        );
        if is_spec_module {
            self.addresses
                .borrow_mut()
                .address
                .get_mut(&address)
                .unwrap()
                .modules
                .get_mut(&module)
                .unwrap()
                .as_ref()
                .borrow_mut()
                .borrow_mut()
                .spec
                .enter_item(item_name, item);
        } else {
            self.addresses
                .borrow_mut()
                .address
                .get_mut(&address)
                .unwrap()
                .modules
                .get_mut(&module)
                .unwrap()
                .as_ref()
                .borrow_mut()
                .borrow_mut()
                .module
                .enter_item(item_name, item);
        }
    }

    pub(crate) fn enter_top_use_item(
        &self,
        convert_loc: &dyn ConvertLoc,
        address: AccountAddress,
        module: Symbol,
        item_name: Symbol,
        item: impl Into<Item>,
        is_spec_module: bool,
    ) {
        let item: Item = item.into();
        let loc = item.def_loc();
        let loc = convert_loc
            .convert_loc_range(&loc)
            .unwrap_or_else(FileRange::unknown);
        log::trace!("{}", loc);
        log::trace!(
            "enter top scope address:0x{:?} module:{:?} name:{:?} item:{}",
            address.short_str_lossless(),
            module,
            item_name,
            item,
        );
        if is_spec_module {
            self.addresses
                .borrow_mut()
                .address
                .get_mut(&address)
                .unwrap()
                .modules
                .get_mut(&module)
                .unwrap()
                .as_ref()
                .borrow_mut()
                .borrow_mut()
                .spec
                .enter_use_item(item_name, item);
        } else {
            self.addresses
                .borrow_mut()
                .address
                .get_mut(&address)
                .unwrap()
                .modules
                .get_mut(&module)
                .unwrap()
                .as_ref()
                .borrow_mut()
                .borrow_mut()
                .module
                .enter_use_item(item_name, item);
        }
    }

    /// Visit all scope from inner to outer.
    pub(crate) fn inner_first_visit(
        &self,
        mut visitor: impl FnMut(&Scope) -> bool, /*  stop??? */
    ) {
        for s in self.scopes.as_ref().borrow().iter().rev() {
            if visitor(s) {
                return;
            }
        }
    }

    /// If none of item enter could be None.
    fn clone_scope(
        &self,
        addr: AccountAddress,
        module_name: Symbol,
        is_spec: bool,
    ) -> Option<Scope> {
        Some({
            let x = self
                .addresses
                .borrow()
                .address
                .get(&addr)?
                .modules
                .get(&module_name)?
                .as_ref()
                .borrow()
                .clone();
            if is_spec {
                x.clone_spec_scope()
            } else {
                x.clone_module_scope()
            }
        })
    }

    pub(crate) fn clone_scope_and_enter(
        &self,
        addr: AccountAddress,
        module_name: Symbol,
        is_spec: bool,
    ) -> ScopesGuarder {
        self.enter_scope_guard(
            self.clone_scope(addr, module_name, is_spec)
                .unwrap_or_default(),
        )
    }

    pub(crate) fn get_access_env(&self) -> AccessEnv {
        self.access_env.get()
    }

    pub(crate) fn set_access_env(&self, x: AccessEnv) -> AccessEnv {
        self.access_env.replace(x)
    }

    pub(crate) fn find_var_type(&self, name: Symbol) -> ResolvedType {
        let mut ret = None;
        self.inner_first_visit(|s| {
            if let Some(v) = s.items.get(&name) {
                match v {
                    Item::Parameter(_, ty) | Item::Var { ty, .. } => {
                        ret = Some(ty.clone());
                        return true;
                    }
                    _ => {}
                }
            };
            false
        });
        ResolvedType::UnKnown
    }

    pub(crate) fn with_friends<R>(
        &self,
        addr: AccountAddress,
        module_name: Symbol,
        call_back: impl FnOnce(&HashSet<(AccountAddress, Symbol)>) -> R,
    ) -> R
    where
        R: Default,
    {
        let get_friend_result = || {
            return Some(call_back(
                &self
                    .addresses
                    .borrow()
                    .address
                    .get(&addr)?
                    .modules
                    .get(&module_name)?
                    .as_ref()
                    .borrow()
                    .friends,
            ));
        };
        get_friend_result().unwrap_or_default()
    }

    pub(crate) fn find_name_chain_item(
        &self,
        chain: &NameAccessChain,
        name_to_addr: &impl Name2Addr,
    ) -> (
        Option<Item>,
        Option<AddrAndModuleName>, /* with a possible module loc returned  */
    ) {
        let mut item_ret = None;
        let mut module_scope = None;
        match &chain.value {
            NameAccessChain_::Single(path_entry) => {
                let name = path_entry.name;
                self.inner_first_visit(|s| {
                    if let Some(v) = if let Some(x) = s.items.get(&name.value) {
                        Some(x)
                    } else {
                        s.uses.get(&name.value)
                    } {
                        match v {
                            Item::Use(x) => {
                                for x in x.iter() {
                                    match x {
                                        ItemUse::Module(_) => {}
                                        ItemUse::Item(_) => {
                                            item_ret = Some(v.clone());
                                            return true;
                                        }
                                    }
                                }
                            }
                            _ => {
                                item_ret = Some(v.clone());
                                return true;
                            }
                        }
                    }
                    false
                });
            }
            NameAccessChain_::Path(name_path) => {
                let name = name_path.root.name;
                match name.value {
                    LeadingNameAccess_::Name(name) | LeadingNameAccess_::GlobalAddress(name)=> {
                        self.inner_first_visit(|s| {
                            if let Some(Item::Use(x)) = s.uses.get(&name.value) {
                                for x in x.iter() {
                                    match x {
                                        ItemUse::Module(ItemUseModule { members, .. }) => {
                                            
                                            for entry in name_path.entries.iter() {
                                                let member = entry.name.value;
                                                if let Some(item) = members
                                                    .as_ref()
                                                    .borrow()
                                                    .module
                                                    .items
                                                    .get(&member)
                                                {
                                                    module_scope = Some(
                                                        members.as_ref().borrow().name_and_addr.clone(),
                                                    );
                                                    item_ret = Some(item.clone());
                                                    // make inner_first_visit stop.
                                                    return true;
                                                }
                                                if let Some(item) = members
                                                    .as_ref()
                                                    .borrow()
                                                    .spec
                                                    .items
                                                    .get(&member)
                                                {
                                                    module_scope = Some(
                                                        members.as_ref().borrow().name_and_addr.clone(),
                                                    );
                                                    item_ret = Some(item.clone());
                                                    // make inner_first_visit stop.
                                                    return true;
                                                }
                                            }
                                        }
                                        ItemUse::Item(_) => {}
                                    }
                                }
                            }
                            false
                        });
                    }
                    LeadingNameAccess_::AnonymousAddress(addr) => {
                        let x = self.visit_address(|x| -> Option<AddrAndModuleName> {
                            Some(
                                x.address
                                    .get(&addr.into_inner())?
                                    .modules
                                    .get(&name_path.entries.last().unwrap().name.value)?
                                    .as_ref()
                                    .borrow()
                                    .name_and_addr
                                    .clone(),
                            )
                        });
                        module_scope = x;
                    }
                }
            }
        }
        (item_ret, module_scope)
    }

    pub(crate) fn find_name_chain_ty(
        &self,
        chain: &NameAccessChain,
        name_to_addr: &impl Name2Addr,
    ) -> (
        Option<ResolvedType>,
        Option<AddrAndModuleName>, /* with a possible module loc returned  */
    ) {
        let mut item_ret = None;
        let mut module_scope = None;
        match &chain.value {
            NameAccessChain_::Single(path_entry) => {
                let name = path_entry.name;
                self.inner_first_visit(|s| {
                    if let Some(v) = s.types.get(&name.value) {
                        if let Some(t) = v.to_type() {
                            item_ret = Some(t);
                            return true;
                        }
                    }

                    if let Some(v) = s.items.get(&name.value) {
                        if let Some(t) = v.to_type() {
                            item_ret = Some(t);
                            return true;
                        }
                    }
                    if let Some(v) = s.uses.get(&name.value) {
                        if let Some(t) = v.to_type() {
                            item_ret = Some(t);
                            return true;
                        }
                    }
                    false
                });
            }
            NameAccessChain_::Path(name_path) => match name_path.root.name.value {
                LeadingNameAccess_::Name(name) | LeadingNameAccess_::GlobalAddress(name) => {
                    self.inner_first_visit(|s| {
                        if let Some(Item::Use(x)) = s.uses.get(&name.value) {
                            for x in x.iter() {
                                match x {
                                    ItemUse::Module(ItemUseModule { members, .. }) => {
                                        for entry in name_path.entries.iter() {
                                            if let Some(item) = members
                                            .as_ref()
                                            .borrow()
                                            .module
                                            .items
                                            .get(&entry.name.value)
                                            {
                                                module_scope = Some(
                                                    members.as_ref().borrow().name_and_addr.clone(),
                                                );
                                                if let Some(t) = item.to_type() {
                                                    item_ret = Some(t);
                                                    return true;
                                                }
                                            }
                                        }
                                        
                                    }
                                    ItemUse::Item(_) => {}
                                }
                            }
                        }
                        false
                    });
                }
                LeadingNameAccess_::AnonymousAddress(addr) => {
                    let x = self.visit_address(|x| -> Option<AddrAndModuleName> {
                        Some(
                            x.address
                                .get(&addr.into_inner())?
                                .modules
                                .get(&name_path.entries.last().unwrap().name.value)?
                                .as_ref()
                                .borrow()
                                .name_and_addr
                                .clone(),
                        )
                    });
                    module_scope = x;
                }
            },
            // NameAccessChain_::Three(chain_two, member) => self.visit_address(|top| {
            //     let modules = top.address.get(&match &chain_two.value.0.value {
            //         LeadingNameAccess_::AnonymousAddress(x) => x.into_inner(),
            //         LeadingNameAccess_::Name(name) => name_to_addr.name_2_addr(name.value),
            //     });
            //     if modules.is_none() {
            //         return;
            //     }
            //     let modules = modules.unwrap();
            //     let module = modules.modules.get(&chain_two.value.1.value);
            //     if module.is_none() {
            //         return;
            //     }
            //     let module = module.unwrap();
            //     module_scope = Some(module.as_ref().borrow().name_and_addr.clone());
            //     if let Some(item) = module.as_ref().borrow().module.items.get(&member.value) {
            //         if let Some(t) = item.to_type() {
            //             item_ret = Some(t);
            //         }
            //     } else if let Some(item) = module.as_ref().borrow().spec.items.get(&member.value) {
            //         if let Some(t) = item.to_type() {
            //             item_ret = Some(t);
            //         }
            //     }
            // }),
        }
        (item_ret, module_scope)
    }

    pub(crate) fn find_var(&self, name: Symbol) -> Option<Item> {
        let mut r = None;
        self.inner_first_visit(|scope| {
            if let Some(item) = scope.items.get(&name) {
                match item {
                    Item::Var { .. } | Item::Parameter(_, _) => {
                        r = Some(item.clone());
                        return true;
                    }
                    _ => {}
                }
            }
            false
        });
        r
    }

    pub(crate) fn visit_address<R>(&self, x: impl FnOnce(&Addresses) -> R) -> R {
        x(&self.addresses.borrow())
    }

    pub(crate) fn enter_scope_guard(&self, s: Scope) -> ScopesGuarder {
        self.scopes.as_ref().borrow_mut().push(s);
        ScopesGuarder::new(self.clone())
    }

    pub(crate) fn resolve_type(&self, ty: &Type, name_to_addr: &impl Name2Addr) -> ResolvedType {
        let r = match &ty.value {
            Type_::Apply(ref chain) => {
                let types = Default::default();
                if let NameAccessChain_::Single(path_entry) = &chain.value {
                    // Special handle for vector.
                    let types = match path_entry.clone().tyargs {
                        Some(x) => {
                            x.value
                            .iter()
                            .map(|ty| self.resolve_type(ty, name_to_addr))
                            .collect()
                        },
                        None => vec![],
                    };
                    
                    let e_ty =  types.get(0).unwrap_or(&UNKNOWN_TYPE).clone();
                    if path_entry.name.value.as_str() == "vector" {
                        // let e_ty = types.get(0).unwrap_or(&UNKNOWN_TYPE).clone();
                        return ResolvedType::new_vector(e_ty);
                    }
                }

                let (chain_ty, _) = self.find_name_chain_ty(chain, name_to_addr);
                let mut chain_ty = chain_ty.unwrap_or_default();
                let chain_ty = match &mut chain_ty {
                    ResolvedType::Struct(
                        ItemStructNameRef {
                            type_parameters: _type_parameters,
                            ..
                        },
                        m,
                    ) => {
                        let _ = std::mem::replace(m, types);
                        chain_ty
                    }
                    _ => chain_ty,
                };
                return chain_ty;
            }
            Type_::Ref(m, ref b) => {
                ResolvedType::Ref(*m, Box::new(self.resolve_type(b.as_ref(), name_to_addr)))
            }
            Type_::Fun(args, ret_ty) => {
                let args: Vec<_> = args
                    .iter()
                    .map(|x| self.resolve_type(x, name_to_addr))
                    .collect();
                let ret_ty = self.resolve_type(ret_ty.as_ref(), name_to_addr);
                ResolvedType::Lambda {
                    args,
                    ret_ty: Box::new(ret_ty),
                }
            }

            Type_::Unit => ResolvedType::Unit,
            Type_::Multiple(ref types) => {
                let types: Vec<_> = types
                    .iter()
                    .map(|v| self.resolve_type(v, name_to_addr))
                    .collect();
                ResolvedType::Multiple(types)
            }
        };
        r
    }

    pub(crate) fn delete_module_items(
        &self,
        addr: AccountAddress,
        module_name: Symbol,
        is_spec_module: bool,
    ) {
        let delete_module_items = || -> Option<()> {
            if is_spec_module {
                self.addresses
                    .borrow_mut()
                    .address
                    .get_mut(&addr)?
                    .modules
                    .get_mut(&module_name)?
                    .as_ref()
                    .borrow_mut()
                    .spec
                    .items
                    .clear();
                self.addresses
                    .borrow_mut()
                    .address
                    .get_mut(&addr)?
                    .modules
                    .get_mut(&module_name)?
                    .as_ref()
                    .borrow_mut()
                    .spec
                    .uses
                    .clear();
                None
            } else {
                self.addresses
                    .borrow_mut()
                    .address
                    .get_mut(&addr)?
                    .modules
                    .get_mut(&module_name)?
                    .as_ref()
                    .borrow_mut()
                    .module
                    .items
                    .clear();
                self.addresses
                    .borrow_mut()
                    .address
                    .get_mut(&addr)?
                    .modules
                    .get_mut(&module_name)?
                    .as_ref()
                    .borrow_mut()
                    .module
                    .uses
                    .clear();
                self.addresses
                    .borrow_mut()
                    .address
                    .get_mut(&addr)?
                    .modules
                    .get_mut(&module_name)?
                    .as_ref()
                    .borrow_mut()
                    .friends
                    .clear();
                None
            }
        };
        delete_module_items();
    }

    pub(crate) fn resolve_friend(&self, addr: AccountAddress, name: Symbol) -> Option<ModuleName> {
        self.visit_address(|x| {
            Some(
                x.address
                    .get(&addr)?
                    .modules
                    .get(&name)?
                    .as_ref()
                    .borrow()
                    .name_and_addr
                    .name,
            )
        })
    }

    /// Collect all spec schema.
    pub(crate) fn collect_all_spec_schema(&self) -> Vec<Item> {
        let mut ret = Vec::new();
        self.inner_first_visit(|scope| {
            for (_, item) in scope.items.iter() {
                if let Item::SpecSchema(_, _) = item {
                    ret.push(item.clone());
                }
            }
            false
        });
        ret
    }

    pub(crate) fn collect_all_spec_target(&self) -> Vec<Item> {
        let mut ret = Vec::new();
        self.inner_first_visit(|scope| {
            for (_, item) in scope.items.iter() {
                match item {
                    Item::Struct(_) | Item::StructNameRef(_) | Item::Fun(_) => {
                        ret.push(item.clone());
                    }
                    _ => {}
                }
            }
            false
        });
        ret
    }

    /// Collect type item in all nest scopes.
    pub(crate) fn collect_all_type_items(&self) -> Vec<Item> {
        let under_test = self.get_access_env();
        let mut ret = Vec::new();
        let mut ret_names = HashSet::new();
        let item_ok = |item: &Item| -> bool {
            match item {
                Item::TParam(_, _) => true,
                Item::Struct(_) | Item::StructNameRef(_) if item.struct_accessible(under_test) => {
                    true
                }
                Item::BuildInType(_) => true,
                _ => false,
            }
        };

        self.inner_first_visit(|scope| {
            for (kname, item) in scope
                .types
                .iter()
                .chain(scope.items.iter())
                .chain(scope.uses.iter())
            {
                match item {
                    Item::Use(x) => {
                        for x in x.iter() {
                            match x {
                                ItemUse::Module(_) => {
                                    if !ret_names.contains(kname) {
                                        ret.push(item.clone());
                                        ret_names.insert(*kname);
                                    }
                                }
                                ItemUse::Item(ItemUseItem { members, name, .. }) => {
                                    // TODO this could be a type like struct.
                                    // do a query to if if this is a type.
                                    let x = members
                                        .as_ref()
                                        .borrow()
                                        .module
                                        .items
                                        .get(&name.value)
                                        .map(|item| item_ok(item));
                                    if x.unwrap_or(true) && !ret_names.contains(kname) {
                                        ret.push(item.clone());
                                        ret_names.insert(*kname);
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        if item_ok(item) {
                            ret.push(item.clone());
                            ret_names.insert(*kname);
                        }
                    }
                };
            }
            false
        });
        ret
    }

    /// Collect all item in nest scopes.
    pub(crate) fn collect_items(&self, filter: impl Fn(&Item) -> bool) -> Vec<Item> {
        let mut ret = Vec::new();
        self.inner_first_visit(|scope| {
            for (_, item) in scope
                .types
                .iter()
                .chain(scope.items.iter())
                .chain(scope.uses.iter())
            {
                if !self.item_access_able(item) {
                    continue;
                }
                if filter(item) {
                    ret.push(item.clone());
                }
            }
            false
        });
        ret
    }

    fn item_access_able(&self, item: &Item) -> bool {
        let env = self.get_access_env();
        match item {
            Item::Const(ItemConst { is_test, .. }) | Item::Struct(ItemStruct { is_test, .. }) => {
                env.is_test() || !(*is_test)
            }
            Item::Fun(x) => x.accessible(self, env),
            Item::SpecBuildInFun(_) | Item::SpecConst(_) => env.is_spec(),
            _ => true,
        }
    }

    /// Collect all import modules.
    /// like use 0x1::vector.
    pub(crate) fn collect_imported_modules(&self) -> Vec<Item> {
        let mut ret = Vec::new();
        self.inner_first_visit(|scope| {
            for (_, item) in scope
                .types
                .iter()
                .chain(scope.items.iter())
                .chain(scope.uses.iter())
            {
                if let Item::Use(_) = item {
                    ret.push(item.clone());
                };
            }
            false
        });
        ret
    }

    /// Collect all items in a module like 0x1::vector.
    pub(crate) fn collect_use_module_items(
        &self,
        name: &LeadingNameAccess,
        select_item: impl Fn(&Item) -> bool,
    ) -> Vec<Item> {
        let mut ret = Vec::new();
        let name = match &name.value {
            LeadingNameAccess_::AnonymousAddress(addr) => {
                log::error!("addr:{:?} should not be here.", addr);
                return ret;
            }
            LeadingNameAccess_::Name(name) | LeadingNameAccess_::GlobalAddress(name)=> name.value,
        };
        self.inner_first_visit(|scope| {
            for (name2, item) in scope
                .types
                .iter()
                .chain(scope.items.iter())
                .chain(scope.uses.iter())
            {
                if let Item::Use(x) = item {
                    for x in x.iter() {
                        match x {
                            ItemUse::Module(ItemUseModule { members, .. }) => {
                                if name == *name2 {
                                    members.borrow().module.items.iter().for_each(|(_, item)| {
                                        if !self.item_access_able(item) {
                                            return;
                                        }

                                        if select_item(item) {
                                            ret.push(item.clone());
                                        }
                                    });
                                    members.borrow().spec.items.iter().for_each(|(_, item)| {
                                        if !self.item_access_able(item) {
                                            return;
                                        }
                                        if select_item(item) {
                                            ret.push(item.clone());
                                        }
                                    });
                                    return true;
                                };
                            }
                            ItemUse::Item(_) => {}
                        }
                    }
                };
            }
            false
        });
        ret
    }

    /// Collect all module names in a addr.
    /// like module 0x1::vector{ ... }
    pub(crate) fn collect_modules(&self, addr: &AccountAddress) -> Vec<ModuleName> {
        let addr_and_name = self.get_current_addr_and_module_name();
        let empty = Default::default();
        let mut ret = Vec::new();
        let env = self.get_access_env();
        self.visit_address(|x| {
            x.address
                .get(addr)
                .unwrap_or(&empty)
                .modules
                .iter()
                .for_each(|(_, x)| {
                    let name = x.as_ref().borrow().name_and_addr.name;
                    if (*addr != addr_and_name.addr || name.value() != addr_and_name.name.value())
                        && (env.is_test() || !x.as_ref().borrow().is_test)
                    {
                        ret.push(name);
                    }
                })
        });
        ret
    }

    /// Collect all module names in a addr.
    /// like module 0x1::vector{ ... }
    pub(crate) fn collect_modules_items(
        &self,
        addr: &AccountAddress,
        module_name: Symbol,
        filter: impl Fn(&Item) -> bool,
    ) -> Vec<Item> {
        let env = self.get_access_env();
        let empty = Default::default();
        let empty2 = Default::default();
        let mut ret = Vec::new();
        self.visit_address(|x| {
            x.address
                .get(addr)
                .unwrap_or(&empty)
                .modules
                .get(&module_name)
                .unwrap_or(&empty2)
                .borrow()
                .module
                .items
                .iter()
                .for_each(|(_, x)| {
                    if !self.item_access_able(x) {
                        return;
                    }
                    if filter(x) {
                        ret.push(x.clone())
                    }
                });

            if env.is_spec() {
                x.address
                    .get(addr)
                    .unwrap_or(&empty)
                    .modules
                    .get(&module_name)
                    .unwrap_or(&empty2)
                    .borrow()
                    .spec
                    .items
                    .iter()
                    .for_each(|(_, x)| {
                        if !self.item_access_able(x) {
                            return;
                        }
                        if filter(x) {
                            ret.push(x.clone())
                        }
                    });
            }
        });
        ret
    }
}

/// RAII type pop on when enter a scope.
#[must_use]
pub(crate) struct ScopesGuarder(Rc<RefCell<Vec<Scope>>>);

impl ScopesGuarder {
    pub(crate) fn new(s: ProjectContext) -> Self {
        Self(s.scopes)
    }
}

impl Drop for ScopesGuarder {
    fn drop(&mut self) {
        self.0.as_ref().borrow_mut().pop().unwrap();
    }
}
