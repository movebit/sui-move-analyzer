// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

use super::item::*;
use crate::{item::ItemFun, project::ERR_ADDRESS, project_context::ProjectContext};
use enum_iterator::Sequence;
use move_command_line_common::files::FileHash;
use move_compiler::{
    parser::ast::*,
    shared::{Identifier, *},
};
use move_ir_types::location::{Loc, Spanned};
use move_symbol_pool::Symbol;
use std::{cmp::PartialEq, collections::HashMap, fmt::Debug, vec};

#[derive(Clone)]
pub enum ResolvedType {
    UnKnown,
    Struct(ItemStructNameRef, Vec<ResolvedType>),
    BuildInType(BuildInType),
    /// T : drop
    TParam(Name, Vec<Ability>),

    /// & mut ...
    Ref(bool, Box<ResolvedType>),
    /// ()
    Unit,
    /// (t1, t2, ... , tn)
    /// Used for return values and expression blocks
    Multiple(Vec<ResolvedType>),
    Fun(ItemFun),
    Vec(Box<ResolvedType>),

    Lambda {
        args: Vec<ResolvedType>,
        ret_ty: Box<ResolvedType>,
    },

    /// Spec type
    Range,
}

impl PartialEq for ResolvedType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ResolvedType::Struct(n1, _), ResolvedType::Struct(n2, _)) => n1 == n2,

            (ResolvedType::BuildInType(b1), ResolvedType::BuildInType(b2)) => b1 == b2,
            (ResolvedType::Ref(_, t1), ResolvedType::Ref(_, t2)) => t1 == t2,
            (ResolvedType::Multiple(v1), ResolvedType::Multiple(v2)) => v1 == v2,
            (ResolvedType::Vec(_), ResolvedType::Vec(_)) => true,

            (ResolvedType::Ref(_, t1), other) => t1.as_ref() == other,
            (other, ResolvedType::Ref(_, t2)) => other == t2.as_ref(),
            _ => false,
        }
    }
}

impl Eq for ResolvedType {}

impl Default for ResolvedType {
    fn default() -> Self {
        Self::UnKnown
    }
}

impl ResolvedType {
    pub(crate) fn nth_ty(&self, index: usize) -> Option<&'_ ResolvedType> {
        match self {
            ResolvedType::Multiple(x) => x.get(index),
            _ => Some(self),
        }
    }

    pub(crate) fn is_vector(&self) -> Option<&'_ Self> {
        match self {
            ResolvedType::Vec(x) => Some(x.as_ref()),
            _ => None,
        }
    }
    pub(crate) fn is_range(&self) -> Option<()> {
        match self {
            ResolvedType::Range => Some(()),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn new_unit() -> Self {
        ResolvedType::Unit
    }
    #[inline]
    pub(crate) fn new_build_in_from_str(name: &str) -> Self {
        match name {
            "bool" => ResolvedType::BuildInType(BuildInType::Bool),
            "u8" => ResolvedType::BuildInType(BuildInType::U8),
            "u16" => ResolvedType::BuildInType(BuildInType::U16),
            "u32" => ResolvedType::BuildInType(BuildInType::U32),
            "u64" => ResolvedType::BuildInType(BuildInType::U64),
            "u128" => ResolvedType::BuildInType(BuildInType::U128),
            "u256" => ResolvedType::BuildInType(BuildInType::U256),
            "address" => ResolvedType::BuildInType(BuildInType::Address),
            _ => unreachable!(),
        }
    }
    #[inline]
    pub(crate) fn new_build_in(b: BuildInType) -> Self {
        ResolvedType::BuildInType(b)
    }
    #[inline]
    pub(crate) fn new_vector(ty: ResolvedType) -> Self {
        ResolvedType::Vec(Box::new(ty))
    }

    #[inline]
    pub(crate) fn is_unknown(&self) -> bool {
        matches!(self, ResolvedType::UnKnown)
    }

    pub(crate) fn is_unit(&self) -> bool {
        matches!(self, ResolvedType::Unit)
    }

    #[inline]
    pub(crate) fn new_ref(is_mut: bool, ty: ResolvedType) -> Self {
        ResolvedType::Ref(is_mut, Box::new(ty))
    }
    #[inline]
    pub(crate) fn is_err(&self) -> bool {
        self.is_unknown()
    }
    #[inline]
    pub(crate) fn is_ref(&self) -> bool {
        matches!(self, Self::Ref(_, _))
    }

    /// bind type parameter to concrete type
    pub(crate) fn bind_type_parameter(&mut self, types: &HashMap<Symbol, ResolvedType>) {
        match self {
            ResolvedType::UnKnown => {}
            ResolvedType::BuildInType(_) => {}
            ResolvedType::TParam(name, _) => {
                if let Some(x) = types.get(&name.value) {
                    *self = x.clone();
                }
            }
            ResolvedType::Ref(_, b) => {
                b.as_mut().bind_type_parameter(types);
            }
            ResolvedType::Unit => {}
            ResolvedType::Multiple(xs) => {
                for i in 0..xs.len() {
                    let t = xs.get_mut(i).unwrap();
                    t.bind_type_parameter(types);
                }
            }
            ResolvedType::Fun(x) => {
                let xs = &mut x.parameters;
                for i in 0..xs.len() {
                    let t = xs.get_mut(i).unwrap();
                    t.1.bind_type_parameter(types);
                }
                x.ret_type.as_mut().bind_type_parameter(types);
            }
            ResolvedType::Vec(b) => {
                b.as_mut().bind_type_parameter(types);
            }

            ResolvedType::Struct(_, ts) => {
                for index in 0..ts.len() {
                    ts.get_mut(index).unwrap().bind_type_parameter(types);
                }
            }
            ResolvedType::Range => {}
            ResolvedType::Lambda { args, ret_ty } => {
                for a in args.iter_mut() {
                    a.bind_type_parameter(types);
                }
                ret_ty.bind_type_parameter(types);
            }
        }
    }

    /// collect type parameter from concrete type
    pub(crate) fn collect_type_parameters(&self, results: &mut Vec<ResolvedType>) {
        match self {
            ResolvedType::Vec(inner) => {
                results.push(inner.as_ref().clone());
            }
            _ => {}
        }
    }

    pub(crate) fn struct_name(&self) -> Option<Symbol> {
        match self {
            ResolvedType::Struct(s, _) => Some(s.name.value()),
            ResolvedType::Ref(_, ty) => ty.struct_name(),
            _ => None,
        }
    }
}

impl ResolvedType {
    pub(crate) fn def_loc(&self) -> Loc {
        match self {
            ResolvedType::TParam(name, _) => name.loc,
            ResolvedType::BuildInType(_) => Loc::new(FileHash::empty(), 0, 0),
            ResolvedType::Struct(ItemStructNameRef { name, .. }, _) => name.loc(),
            ResolvedType::UnKnown => Loc::new(FileHash::empty(), 0, 0),
            ResolvedType::Ref(_, _) => Loc::new(FileHash::empty(), 0, 0),
            ResolvedType::Unit => Loc::new(FileHash::empty(), 0, 0),
            ResolvedType::Multiple(_) => Loc::new(FileHash::empty(), 0, 0),
            ResolvedType::Fun(f) => f.name.0.loc,
            ResolvedType::Vec(_) => Loc::new(FileHash::empty(), 0, 0),
            ResolvedType::Range => Loc::new(FileHash::empty(), 0, 0),
            ResolvedType::Lambda { .. } => Loc::new(FileHash::empty(), 0, 0),
        }
    }
}

#[derive(Clone, Debug, Copy, Sequence, Eq, PartialEq)]
pub enum BuildInType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    /// A number type from literal.
    /// Could be u8 and ... depend on How it is used.
    NumType,
    /// https://move-book.com/advanced-topics/managing-collections-with-vectors.html?highlight=STring#hex-and-bytestring-literal-for-inline-vector-definitions
    /// alias for vector<u8>
    String,
    Signer,
}

impl BuildInType {
    pub(crate) fn to_static_str(self) -> &'static str {
        match self {
            BuildInType::U8 => "u8",
            BuildInType::U16 => "u16",
            BuildInType::U32 => "u32",
            BuildInType::U64 => "u64",
            BuildInType::U128 => "u128",
            BuildInType::U256 => "u256",
            BuildInType::Bool => "bool",
            BuildInType::Address => "address",
            BuildInType::Signer => "signer",
            BuildInType::String => "vector<u8>",
            BuildInType::NumType => "u256",
        }
    }

    pub(crate) fn is_num_types_str(name: &str) -> bool {
        Self::num_types()
            .iter()
            .map(|t| t.to_static_str())
            .any(|s| s == name)
    }

    pub(crate) fn num_types() -> Vec<Self> {
        vec![
            Self::U8,
            Self::U16,
            Self::U32,
            Self::U64,
            Self::U128,
            Self::U256,
        ]
    }

    /// Not all is build in.
    /// exclude String and NumType.
    pub(crate) fn build_ins() -> Vec<Self> {
        let mut x = Self::num_types();
        x.push(Self::Address);
        x.push(Self::Signer);
        x.push(Self::Bool);
        x
    }
}

impl std::fmt::Display for ResolvedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedType::UnKnown => write!(f, "unknown"),
            ResolvedType::Struct(
                ItemStructNameRef {
                    name,
                    type_parameters,
                    ..
                },
                ty_args,
            ) => {
                let ty_str = type_parameters
                    .iter()
                    .map(|tp| tp.name.value.as_str())
                    .collect::<Vec<_>>()
                    .join(",");

                let tp_str = type_parameters
                    .iter()
                    .zip(ty_args.iter())
                    .map(|(dt, ty)| format!("{}:{}", dt.name, ty))
                    .collect::<Vec<_>>()
                    .join(", ");

                if ty_str.is_empty() {
                    write!(f, "Struct:{}<{}>;", name.value().as_str(), tp_str)
                } else {
                    write!(f, "Struct:{}<{}>;", name.value().as_str(), tp_str)
                }
            }
            ResolvedType::BuildInType(x) => write!(f, "{}", x.to_static_str()),
            ResolvedType::TParam(name, _) => {
                write!(f, "TParam:{}", name.value.as_str())
            }
            ResolvedType::Ref(is_mut, ty) => {
                write!(f, "&{}{}", if *is_mut { "mut " } else { "" }, ty.as_ref())
            }
            ResolvedType::Unit => write!(f, "()"),
            ResolvedType::Multiple(m) => {
                write!(f, "(")?;
                for i in 0..m.len() {
                    let t = m.get(i).unwrap();
                    write!(f, "{}{}", if i == m.len() - 1 { "" } else { "," }, t)?;
                }
                write!(f, ")")
            }
            ResolvedType::Fun(x) => {
                write!(f, "Fun {}", x)
            }
            ResolvedType::Vec(ty) => {
                write!(f, "vector<<{}>>", ty.as_ref())
            }
            ResolvedType::Range => {
                write!(f, "range(n..m)")
            }
            ResolvedType::Lambda { args, ret_ty } => {
                write!(f, "|")?;
                if !args.is_empty() {
                    let last_index = args.len() - 1;
                    for (index, a) in args.iter().enumerate() {
                        write!(f, "{}", a)?;
                        if index != last_index {
                            write!(f, ",")?;
                        }
                    }
                }
                write!(f, "|")?;
                if !(matches!(ret_ty.as_ref(), ResolvedType::Unit)) {
                    write!(f, ":")?;
                    write!(f, "{}", ret_ty)
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl std::fmt::Debug for ResolvedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl ResolvedType {
    pub(crate) fn struct_ref_to_struct(&self, s: &ProjectContext) -> ItemStruct {
        match self.clone() {
            Self::Struct(
                ItemStructNameRef {
                    addr,
                    module_name,
                    name,
                    type_parameters: _type_parameters,
                    is_test: _is_test,
                },
                v,
            ) => {
                log::debug!("struct_ref_to_struct => {:?}", v);
                s.query_item(addr, module_name, name.0.value, |x| match x {
                    Item::Struct(item) => {
                        let mut item = item.clone();
                        item.type_parameters_ins = v;
                        item.bind_type_parameter(Some(&item.collect_type_parameters()) );
                        item
                    }
                    _ => {
                        unimplemented!()
                    }
                }) .expect("You are looking for a struct which can't be found,It is possible But should not happen.")
            }
            _ => ItemStruct {
                name: DatatypeName(Spanned {
                    loc: Loc::new(FileHash::empty(), 0, 0),
                    value: Symbol::from(""),
                }),
                type_parameters: vec![],
                type_parameters_ins: vec![],
                fields: vec![],
                is_test: false,
                addr: *ERR_ADDRESS,
                module_name: Symbol::from(""),
            },
        }
    }

    //     match self {
    //         ResolvedType::Vec(ty) => ty.
    //     }
    // }
}
