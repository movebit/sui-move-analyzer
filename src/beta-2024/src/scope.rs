// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{item::*, types::*};
use crate::project::ERR_ADDRESS;
use move_command_line_common::files::FileHash;
use move_compiler::parser::ast::*;
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::{Loc, Spanned};
use move_symbol_pool::Symbol;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Default, Clone)]
pub struct Scope {
    /// fun struct  ...
    pub(crate) items: HashMap<Symbol, Item>,
    /// All uses.
    pub(crate) uses: HashMap<Symbol, Item>,
    /// Type parameter go into this map.
    pub(crate) types: HashMap<Symbol, Item>,
}

#[derive(Clone)]
pub struct AddrAndModuleName {
    pub(crate) addr: AccountAddress,
    pub(crate) name: ModuleName,
}
impl Eq for AddrAndModuleName {}

impl PartialEq for AddrAndModuleName {
    fn eq(&self, other: &Self) -> bool {
        self.addr == other.addr && self.name.0.value == other.name.0.value
    }
}

impl Scope {
    pub(crate) fn enter_build_in(&mut self) {
        BuildInType::build_ins().into_iter().for_each(|x| {
            self.enter_item(Symbol::from(x.to_static_str()), Item::BuildInType(x));
        });
        enum_iterator::all::<MoveBuildInFun>()
            .collect::<Vec<_>>()
            .iter()
            .for_each(|x| {
                self.enter_item(Symbol::from(x.to_static_str()), Item::MoveBuildInFun(*x))
            });
        self.enter_spec_build_in();
    }
    pub(crate) fn enter_use_item(&mut self, s: Symbol, item: impl Into<Item>) {
        let item = item.into();
        match &item {
            Item::Use(items) => {
                if let Some(x) = self.uses.get_mut(&s) {
                    match x {
                        Item::Use(x2) => {
                            // TODO provent insert twice.
                            // inserted, just return.
                            x2.extend(items.clone());
                            return;
                        }
                        _ => {
                            unreachable!()
                        }
                    }
                };
            }
            _ => {
                unreachable!()
            }
        }
        self.uses.insert(s, item);
    }

    pub(crate) fn enter_item(&mut self, s: Symbol, item: impl Into<Item>) {
        let item = item.into();
        match &item {
            Item::Var { .. } | Item::Parameter(_, _) if s.as_str() == "_" => {
                return;
            }
            Item::Use(_) => {
                unreachable!()
            }
            _ => {}
        }
        self.items.insert(s, item);
    }

    pub(crate) fn enter_types(&mut self, s: Symbol, item: impl Into<Item>) {
        let item = item.into();
        self.types.insert(s, item);
    }
    fn enter_spec_build_in(&mut self) {
        BuildInType::num_types().iter().for_each(|ty| {
            let x = Symbol::from(format!("MAX_{}", ty.to_static_str().to_uppercase()));
            self.enter_item(
                x,
                Item::SpecConst(ItemConst {
                    name: ConstantName(Spanned {
                        loc: Loc::new(FileHash::empty(), 0, 0),
                        value: x,
                    }),
                    ty: ResolvedType::new_build_in(*ty),
                    is_test: false,
                }),
            );
        });
        enum_iterator::all::<SpecBuildInFun>()
            .collect::<Vec<_>>()
            .iter()
            .for_each(|x| {
                self.enter_item(Symbol::from(x.to_static_str()), Item::SpecBuildInFun(*x))
            });
    }
}

#[derive(Clone, Default)]
pub struct Addresses {
    /// address to modules
    pub(crate) address: HashMap<AccountAddress, Address>,
}

impl Addresses {
    pub fn new() -> Self {
        Self {
            address: Default::default(),
        }
    }
}

#[derive(Default, Clone)]
pub struct Address {
    /// module name to Scope.
    pub(crate) modules: HashMap<Symbol, Rc<RefCell<ModuleScope>>>,
}

#[derive(Clone)]
pub struct ModuleScope {
    pub(crate) module: Scope,
    pub(crate) spec: Scope,
    pub(crate) name_and_addr: AddrAndModuleName,
    pub(crate) friends: HashSet<(AccountAddress, Symbol)>,
    pub(crate) is_test: bool,
}

/// Used for some dummy or empty data.
impl Default for ModuleScope {
    fn default() -> Self {
        Self {
            module: Default::default(),
            spec: Default::default(),
            name_and_addr: AddrAndModuleName {
                addr: *ERR_ADDRESS,
                name: ModuleName(Spanned {
                    loc: Loc::new(FileHash::empty(), 0, 0),
                    value: Symbol::from("_"),
                }),
            },
            friends: Default::default(),
            is_test: false,
        }
    }
}

impl ModuleScope {
    pub(crate) fn new(name_and_addr: AddrAndModuleName, is_test: bool) -> Self {
        Self {
            module: Default::default(),
            spec: Default::default(),
            name_and_addr,
            friends: Default::default(),
            is_test,
        }
    }

    fn clone_module(&self) -> Scope {
        self.module.clone()
    }

    pub(crate) fn clone_spec_scope(&self) -> Scope {
        let mut s = self.clone_module();
        for x in self.spec.items.iter() {
            s.enter_item(*x.0, x.1.clone());
        }
        for x in self.spec.uses.iter() {
            s.enter_use_item(*x.0, x.1.clone());
        }
        s
    }

    pub(crate) fn clone_module_scope(&self) -> Scope {
        let mut s = self.clone_module();
        for x in self.spec.items.iter() {
            match &x.1 {
                Item::Fun(_) | Item::SpecSchema(_, _) => {
                    s.enter_item(*x.0, x.1.clone());
                }
                _ => {}
            }
        }
        s
    }

    pub(crate) fn new_module_name(
        addr: AccountAddress,
        name: ModuleName,
    ) -> Rc<RefCell<ModuleScope>> {
        Rc::new(RefCell::new(ModuleScope::new(
            AddrAndModuleName { addr, name },
            false,
        )))
    }
}
