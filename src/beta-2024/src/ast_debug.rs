// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Simple trait used for pretty printing the various AST
///
/// Unfortunately, the trait implementation cannot be derived. The actual implementation should
/// closely resemble the source syntax. As suchfield does not get printed in a direct manner, and
/// most of the logic is ad hoc
///
/// To avoid missing fields in the printing, be sure to fully pattern match against the struct
/// (without the use of `..`) when implementing `MyMyAstDebug`. For example,
///
/// ```rust,ignore
/// impl MyMyAstDebug for StructDefinition {
///     fn my_my_ast_debug(&self, w: &mut AstWriter) {
///         let StructDefinition {
///             resource_opt,
///             name,
///             type_parameters,
///             fields,
///         } = self;
///         ...
///     }
/// }
/// ```
//**************************************************************************************************
// Macros
//**************************************************************************************************
use std::fmt::Display;

#[macro_export]
macro_rules! debug_print {
    ($e:expr) => {
        $crate::shared::my_my_ast_debug::print(&$e)
    };
}

#[macro_export]
macro_rules! debug_print_verbose {
    ($e:expr) => {
        $crate::shared::my_my_ast_debug::print_verbose(&$e)
    };
}

#[macro_export]
macro_rules! debug_display {
    ($e:expr) => {
        $crate::shared::my_my_ast_debug::DisplayWrapper(&$e, false)
    };
}

#[macro_export]
macro_rules! debug_display_verbose {
    ($e:expr) => {
        $crate::shared::my_my_ast_debug::DisplayWrapper(&$e, true)
    };
}

//**************************************************************************************************
// Printer
//**************************************************************************************************


pub trait MyAstDebug {
    fn my_ast_debug(&self, w: &mut AstWriter);
    fn print(&self) {
        let mut writer = AstWriter::normal();
        self.my_ast_debug(&mut writer);
        print!("{}", writer);
    }
    fn print_verbose(&self) {
        let mut writer = AstWriter::verbose();
        self.my_ast_debug(&mut writer);
        print!("{}", writer);
    }
}

impl<T: MyAstDebug> MyAstDebug for Box<T> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        self.as_ref().my_ast_debug(w)
    }
}

impl<T: MyAstDebug> MyAstDebug for &T {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        MyAstDebug::my_ast_debug(*self, w)
    }
}

impl<T: MyAstDebug> MyAstDebug for &mut T {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        MyAstDebug::my_ast_debug(*self, w)
    }
}

pub struct AstWriter {
    verbose: bool,
    margin: usize,
    lines: Vec<String>,
}

impl AstWriter {
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            margin: 0,
            lines: vec![String::new()],
        }
    }

    fn normal() -> Self {
        Self::new(false)
    }

    fn verbose() -> Self {
        Self::new(true)
    }

    fn cur(&mut self) -> &mut String {
        self.lines.last_mut().unwrap()
    }

    pub fn new_line(&mut self) {
        self.lines.push(String::new());
    }

    pub fn write(&mut self, s: impl AsRef<str>) {
        let margin = self.margin;
        let cur = self.cur();
        if cur.is_empty() {
            (0..margin).for_each(|_| cur.push(' '));
        }
        cur.push_str(s.as_ref());
    }

    pub fn writeln(&mut self, s: impl AsRef<str>) {
        self.write(s);
        self.new_line();
    }

    pub fn indent<F: FnOnce(&mut AstWriter)>(&mut self, inc: usize, f: F) {
        self.new_line();
        self.margin += inc;
        f(self);
        self.margin -= inc;
        self.new_line();
    }

    pub fn block<F: FnOnce(&mut AstWriter)>(&mut self, f: F) {
        self.write(" {");
        self.indent(4, f);
        self.write("}");
    }

    pub fn annotate<F: FnOnce(&mut AstWriter), Annot: MyAstDebug>(&mut self, f: F, annot: &Annot) {
        self.annotate_gen(f, annot, |w, annot| annot.my_ast_debug(w))
    }

    pub fn annotate_gen<
        F: FnOnce(&mut AstWriter),
        Annot,
        FAnnot: FnOnce(&mut AstWriter, &Annot),
    >(
        &mut self,
        f: F,
        annot: &Annot,
        annot_writer: FAnnot,
    ) {
        if self.verbose {
            self.write("(");
        }
        f(self);
        if self.verbose {
            self.write(": ");
            annot_writer(self, annot);
            self.write(")");
        }
    }

    pub fn list<T, F: FnMut(&mut AstWriter, T) -> bool>(
        &mut self,
        items: impl std::iter::IntoIterator<Item = T>,
        sep: &str,
        mut f: F,
    ) {
        let iter = items.into_iter();
        let len = match iter.size_hint() {
            (lower, None) => {
                assert!(lower == 0);
                return;
            }
            (_, Some(len)) => len,
        };
        for (idx, item) in iter.enumerate() {
            let needs_newline = f(self, item);
            if idx + 1 != len {
                self.write(sep);
                if needs_newline {
                    self.new_line()
                }
            }
        }
    }

    pub fn comma<T, F: FnMut(&mut AstWriter, T)>(
        &mut self,
        items: impl std::iter::IntoIterator<Item = T>,
        mut f: F,
    ) {
        self.list(items, ", ", |w, item| {
            f(w, item);
            false
        })
    }

    pub fn semicolon<T, F: FnMut(&mut AstWriter, T)>(
        &mut self,
        items: impl std::iter::IntoIterator<Item = T>,
        mut f: F,
    ) {
        self.list(items, ";", |w, item| {
            f(w, item);
            true
        })
    }
}

impl Display for AstWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for line in &self.lines {
            writeln!(f, "{}", line)?;
        }
        Ok(())
    }
}

impl<T: MyAstDebug> MyAstDebug for move_ir_types::location::Spanned<T> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        self.value.my_ast_debug(w)
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

pub struct DisplayWrapper<'a, T: MyAstDebug>(pub &'a T, /* verbose */ pub bool);

impl<T: MyAstDebug> Display for DisplayWrapper<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut writer = if self.1 {
            AstWriter::verbose()
        } else {
            AstWriter::normal()
        };
        self.0.my_ast_debug(&mut writer);
        writer.fmt(f)
    }
}



use move_compiler::parser::ast::*;
use move_compiler::shared::{NamedAddressMap, NamedAddressMaps, Name};
use move_ir_types::location::Loc;

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl MyAstDebug for Program {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let Self {
            named_address_maps,
            source_definitions,
            lib_definitions,
        } = self;
        w.writeln("------ Lib Defs: ------");
        for def in lib_definitions {
            my_ast_debug_package_definition(w, named_address_maps, def)
        }
        w.new_line();
        w.writeln("------ Source Defs: ------");
        for def in source_definitions {
            my_ast_debug_package_definition(w, named_address_maps, def)
        }
    }
}

fn my_ast_debug_package_definition(
    w: &mut AstWriter,
    named_address_maps: &NamedAddressMaps,
    pkg: &PackageDefinition,
) {
    let PackageDefinition {
        package,
        named_address_map,
        def,
    } = pkg;
    match package {
        Some(n) => w.writeln(&format!("package: {}", n)),
        None => w.writeln("no package"),
    }
    named_address_maps.get(*named_address_map).my_ast_debug(w);
    def.my_ast_debug(w);
}

impl MyAstDebug for NamedAddressMap {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        for (sym, addr) in self {
            w.write(&format!("{} => {}", sym, addr));
            w.new_line()
        }
    }
}

impl MyAstDebug for Definition {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            Definition::Address(a) => a.my_ast_debug(w),
            Definition::Module(m) => m.my_ast_debug(w),
        }
    }
}

impl MyAstDebug for AddressDefinition {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let AddressDefinition {
            attributes,
            loc: _loc,
            addr,
            modules,
        } = self;
        attributes.my_ast_debug(w);
        w.write(&format!("address {}", addr));
        w.writeln(" {{");
        for m in modules {
            m.my_ast_debug(w)
        }
        w.writeln("}");
    }
}

impl MyAstDebug for AttributeValue_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            AttributeValue_::Value(v) => v.my_ast_debug(w),
            AttributeValue_::ModuleAccess(n) => n.my_ast_debug(w),
        }
    }
}

impl MyAstDebug for Attribute_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            Attribute_::Name(n) => w.write(&format!("{}", n)),
            Attribute_::Assigned(n, v) => {
                w.write(&format!("{}", n));
                w.write(" = ");
                v.my_ast_debug(w);
            }
            Attribute_::Parameterized(n, inners) => {
                w.write(&format!("{}", n));
                w.write("(");
                w.list(&inners.value, ", ", |w, inner| {
                    inner.my_ast_debug(w);
                    false
                });
                w.write(")");
            }
        }
    }
}

impl MyAstDebug for Vec<Attribute> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.write("#[");
        w.list(self, ", ", |w, attr| {
            attr.my_ast_debug(w);
            false
        });
        w.write("]");
    }
}

impl MyAstDebug for Vec<Attributes> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.list(self, "", |w, attrs| {
            attrs.my_ast_debug(w);
            true
        });
    }
}

impl MyAstDebug for ModuleDefinition {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let ModuleDefinition {
            attributes,
            loc: _loc,
            address,
            name,
            is_spec_module,
            members,
        } = self;
        attributes.my_ast_debug(w);
        match address {
            None => w.write(&format!(
                "module {}{}",
                if *is_spec_module { "spec " } else { "" },
                name
            )),
            Some(addr) => w.write(&format!("module {}::{}", addr, name)),
        };
        w.block(|w| {
            for mem in members {
                mem.my_ast_debug(w)
            }
        });
    }
}

impl MyAstDebug for ModuleMember {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            ModuleMember::Function(f) => f.my_ast_debug(w),
            ModuleMember::Struct(s) => s.my_ast_debug(w),
            ModuleMember::Enum(e) => e.my_ast_debug(w),
            ModuleMember::Use(u) => u.my_ast_debug(w),
            ModuleMember::Friend(f) => f.my_ast_debug(w),
            ModuleMember::Constant(c) => c.my_ast_debug(w),
            ModuleMember::Spec(s) => w.write(&s.value),
        }
    }
}

impl MyAstDebug for UseDecl {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let UseDecl {
            attributes,
            loc: _,
            use_,
        } = self;
        attributes.my_ast_debug(w);
        use_.my_ast_debug(w);
    }
}

impl MyAstDebug for ModuleUse {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            ModuleUse::Module(alias) => {
                alias.map(|alias| w.write(&format!("as {}", alias)));
            }
            ModuleUse::Members(members) => w.block(|w| {
                w.comma(members, |w, (name, alias)| {
                    w.write(&format!("{}", name));
                    alias.map(|alias| w.write(&format!("as {}", alias.value)));
                })
            }),
        }
    }
}

impl MyAstDebug for Use {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.write("use ");
        match self {
            Use::ModuleUse(mident, use_) => {
                w.write(&format!("{}", mident));
                use_.my_ast_debug(w);
            }
            Use::NestedModuleUses(addr, entries) => {
                w.write(&format!("{}::", addr));
                w.block(|w| {
                    w.comma(entries, |w, (name, use_)| {
                        w.write(&format!("{}::", name));
                        use_.my_ast_debug(w);
                    })
                })
            }
            Use::Fun {
                visibility,
                function,
                ty,
                method,
            } => {
                visibility.my_ast_debug(w);
                w.write(" use fun ");
                function.my_ast_debug(w);
                w.write(" as ");
                ty.my_ast_debug(w);
                w.write(format!(".{method}"));
            }
        }
        w.write(";")
    }
}

impl MyAstDebug for FriendDecl {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let FriendDecl {
            attributes,
            loc: _,
            friend,
        } = self;
        attributes.my_ast_debug(w);
        w.write(&format!("friend {}", friend));
    }
}

impl MyAstDebug for EnumDefinition {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let EnumDefinition {
            attributes,
            loc: _loc,
            abilities,
            name,
            type_parameters,
            variants,
        } = self;
        attributes.my_ast_debug(w);

        if !abilities.is_empty() {
            w.write("[");
            w.list(abilities, " ", |w, ab_mod| {
                ab_mod.my_ast_debug(w);
                false
            });
            w.write("]");
        }

        w.write(&format!(" enum {}", name));
        type_parameters.my_ast_debug(w);
        w.block(|w| {
            w.list(variants, ",", |w, variant| {
                variant.my_ast_debug(w);
                true
            })
        });
    }
}

impl MyAstDebug for VariantDefinition {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let VariantDefinition {
            loc: _,
            name,
            fields,
        } = self;
        w.write(&format!("{}", name));
        match fields {
            VariantFields::Named(fields) => w.block(|w| {
                w.semicolon(fields, |w, (f, st)| {
                    w.write(&format!("{}: ", f));
                    st.my_ast_debug(w);
                });
            }),
            VariantFields::Positional(types) => w.block(|w| {
                w.semicolon(types.iter().enumerate(), |w, (i, st)| {
                    w.write(&format!("pos{}: ", i));
                    st.my_ast_debug(w);
                });
            }),
            VariantFields::Empty => (),
        }
    }
}

impl MyAstDebug for StructDefinition {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let StructDefinition {
            attributes,
            loc: _loc,
            abilities,
            name,
            type_parameters,
            fields,
        } = self;
        attributes.my_ast_debug(w);

        w.list(abilities, " ", |w, ab_mod| {
            ab_mod.my_ast_debug(w);
            false
        });

        if let StructFields::Native(_) = fields {
            w.write("native ");
        }

        w.write(&format!("struct {}", name));
        type_parameters.my_ast_debug(w);
        match fields {
            StructFields::Named(fields) => w.block(|w| {
                w.semicolon(fields, |w, (f, st)| {
                    w.write(&format!("{}: ", f));
                    st.my_ast_debug(w);
                });
            }),
            StructFields::Positional(types) => w.block(|w| {
                w.semicolon(types.iter().enumerate(), |w, (i, st)| {
                    w.write(&format!("pos{}: ", i));
                    st.my_ast_debug(w);
                });
            }),
            StructFields::Native(_) => (),
        }
    }
}

impl MyAstDebug for Function {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let Function {
            attributes,
            loc: _loc,
            visibility,
            entry,
            macro_,
            signature,
            name,
            body,
        } = self;
        attributes.my_ast_debug(w);
        visibility.my_ast_debug(w);
        if entry.is_some() {
            w.write(&format!("{} ", ENTRY_MODIFIER));
        }
        if macro_.is_some() {
            w.write(&format!("{} ", MACRO_MODIFIER));
        }
        if let FunctionBody_::Native = &body.value {
            w.write(&format!("{} ", NATIVE_MODIFIER));
        }
        w.write(&format!("fun {}", name));
        signature.my_ast_debug(w);
        match &body.value {
            FunctionBody_::Defined(body) => w.block(|w| body.my_ast_debug(w)),
            FunctionBody_::Native => w.writeln(";"),
        }
    }
}

impl MyAstDebug for Visibility {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.write(&format!("{} ", self))
    }
}

impl MyAstDebug for FunctionSignature {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let FunctionSignature {
            type_parameters,
            parameters,
            return_type,
        } = self;
        type_parameters.my_ast_debug(w);
        w.write("(");
        w.comma(parameters, |w, (mut_, v, st)| {
            if mut_.is_some() {
                w.write("mut ");
            }
            w.write(&format!("{}: ", v));
            st.my_ast_debug(w);
        });
        w.write(")");
        w.write(": ");
        return_type.my_ast_debug(w)
    }
}

impl MyAstDebug for Constant {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let Constant {
            attributes,
            loc: _loc,
            name,
            signature,
            value,
        } = self;
        attributes.my_ast_debug(w);
        w.write(&format!("const {}:", name));
        signature.my_ast_debug(w);
        w.write(" = ");
        value.my_ast_debug(w);
        w.write(";");
    }
}

impl MyAstDebug for Vec<(Name, Vec<Ability>)> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        if !self.is_empty() {
            w.write("<");
            w.comma(self, |w, tp| tp.my_ast_debug(w));
            w.write(">")
        }
    }
}

impl MyAstDebug for (Name, Vec<Ability>) {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let (n, abilities) = self;
        w.write(n.value);
        ability_constraints_my_ast_debug(w, abilities);
    }
}

impl MyAstDebug for Vec<DatatypeTypeParameter> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        if !self.is_empty() {
            w.write("<");
            w.comma(self, |w, tp| tp.my_ast_debug(w));
            w.write(">");
        }
    }
}

impl MyAstDebug for DatatypeTypeParameter {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let Self {
            is_phantom,
            name,
            constraints,
        } = self;
        if *is_phantom {
            w.write("phantom ");
        }
        w.write(name.value);
        ability_constraints_my_ast_debug(w, constraints);
    }
}

fn ability_constraints_my_ast_debug(w: &mut AstWriter, abilities: &[Ability]) {
    if !abilities.is_empty() {
        w.write(": ");
        w.list(abilities, "+", |w, ab| {
            ab.my_ast_debug(w);
            false
        })
    }
}

impl MyAstDebug for Ability_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.write(&format!("{}", self))
    }
}

impl MyAstDebug for Type_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            Type_::Unit => w.write("()"),
            Type_::Multiple(ss) => {
                w.write("(");
                ss.my_ast_debug(w);
                w.write(")")
            }
            Type_::Apply(m) => {
                m.my_ast_debug(w);
            }
            Type_::Ref(mut_, s) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                s.my_ast_debug(w)
            }
            Type_::Fun(args, result) => {
                w.write("(");
                w.comma(args, |w, ty| ty.my_ast_debug(w));
                w.write("):");
                result.my_ast_debug(w);
            }
        }
    }
}

impl MyAstDebug for Vec<Type> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, s| s.my_ast_debug(w))
    }
}

impl MyAstDebug for RootPathEntry {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let RootPathEntry {
            name,
            tyargs,
            is_macro,
        } = self;
        w.write(format!("{}", name));
        if is_macro.is_some() {
            w.write("!");
        }
        if let Some(ts) = tyargs {
            w.write("<");
            ts.my_ast_debug(w);
            w.write(">");
        }
    }
}

impl MyAstDebug for PathEntry {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let PathEntry {
            name,
            tyargs,
            is_macro,
        } = self;
        w.write(format!("{}", name));
        if is_macro.is_some() {
            w.write("!");
        }
        if let Some(ts) = tyargs {
            w.write("<");
            ts.my_ast_debug(w);
            w.write(">");
        }
    }
}

impl MyAstDebug for NamePath {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let NamePath { root, entries } = self;
        w.write(format!("{}::", root));
        w.list(entries, "::", |w, e| {
            e.my_ast_debug(w);
            false
        });
    }
}

impl MyAstDebug for NameAccessChain_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            NameAccessChain_::Single(entry) => entry.my_ast_debug(w),
            NameAccessChain_::Path(entry) => entry.my_ast_debug(w),
        }
    }
}

impl MyAstDebug
    for (
        Vec<UseDecl>,
        Vec<SequenceItem>,
        Option<Loc>,
        Box<Option<Exp>>,
    )
{
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let (uses, seq, _, last_e) = self;
        for u in uses {
            u.my_ast_debug(w);
            w.new_line();
        }
        w.semicolon(seq, |w, item| item.my_ast_debug(w));
        if !seq.is_empty() {
            w.writeln(";")
        }
        if let Some(e) = &**last_e {
            e.my_ast_debug(w)
        }
    }
}

impl MyAstDebug for SequenceItem_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        use SequenceItem_ as I;
        match self {
            I::Seq(e) => e.my_ast_debug(w),
            I::Declare(sp!(_, bs), ty_opt) => {
                w.write("let ");
                bs.my_ast_debug(w);
                if let Some(ty) = ty_opt {
                    ty.my_ast_debug(w)
                }
            }
            I::Bind(sp!(_, bs), ty_opt, e) => {
                w.write("let ");
                bs.my_ast_debug(w);
                if let Some(ty) = ty_opt {
                    ty.my_ast_debug(w)
                }
                w.write(" = ");
                e.my_ast_debug(w);
            }
        }
    }
}

impl MyAstDebug for Exp_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        use Exp_ as E;
        match self {
            E::Unit => w.write("()"),
            E::Parens(e) => {
                w.write("(");
                e.my_ast_debug(w);
                w.write(")");
            }
            E::Value(v) => v.my_ast_debug(w),
            E::Move(_, e) => {
                w.write("move ");
                e.my_ast_debug(w);
            }
            E::Copy(_, e) => {
                w.write("copy ");
                e.my_ast_debug(w);
            }
            E::Name(ma) => {
                ma.my_ast_debug(w);
            }
            E::Call(ma, sp!(_, rhs)) => {
                ma.my_ast_debug(w);
                w.write("(");
                w.comma(rhs, |w, e| e.my_ast_debug(w));
                w.write(")");
            }
            E::Pack(ma, fields) => {
                ma.my_ast_debug(w);
                w.write("{");
                w.comma(fields, |w, (f, e)| {
                    w.write(&format!("{}: ", f));
                    e.my_ast_debug(w);
                });
                w.write("}");
            }
            E::Vector(_loc, tys_opt, sp!(_, elems)) => {
                w.write("vector");
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.my_ast_debug(w);
                    w.write(">");
                }
                w.write("[");
                w.comma(elems, |w, e| e.my_ast_debug(w));
                w.write("]");
            }
            E::IfElse(b, t, f_opt) => {
                w.write("if (");
                b.my_ast_debug(w);
                w.write(") ");
                t.my_ast_debug(w);
                if let Some(f) = f_opt {
                    w.write(" else ");
                    f.my_ast_debug(w);
                }
            }
            E::Match(subject, arms) => {
                w.write("match (");
                subject.my_ast_debug(w);
                w.write(") ");
                w.block(|w| {
                    w.list(&arms.value, ", ", |w, arm| {
                        arm.my_ast_debug(w);
                        true
                    })
                });
            }
            E::While(b, e) => {
                w.write("while (");
                b.my_ast_debug(w);
                w.write(")");
                e.my_ast_debug(w);
            }
            E::Loop(e) => {
                w.write("loop ");
                e.my_ast_debug(w);
            }
            E::Labeled(name, e) => {
                w.write(format!("'{name}: "));
                e.my_ast_debug(w)
            }
            E::Block(seq) => w.block(|w| seq.my_ast_debug(w)),
            E::Lambda(sp!(_, bs), ty_opt, e) => {
                bs.my_ast_debug(w);
                if let Some(ty) = ty_opt {
                    w.write(" -> ");
                    ty.my_ast_debug(w);
                }
                e.my_ast_debug(w);
            }
            E::Quant(kind, sp!(_, rs), trs, c_opt, e) => {
                kind.my_ast_debug(w);
                w.write(" ");
                rs.my_ast_debug(w);
                trs.my_ast_debug(w);
                if let Some(c) = c_opt {
                    w.write(" where ");
                    c.my_ast_debug(w);
                }
                w.write(" : ");
                e.my_ast_debug(w);
            }
            E::ExpList(es) => {
                w.write("(");
                w.comma(es, |w, e| e.my_ast_debug(w));
                w.write(")");
            }
            E::Assign(lvalue, rhs) => {
                lvalue.my_ast_debug(w);
                w.write(" = ");
                rhs.my_ast_debug(w);
            }
            E::Abort(e) => {
                w.write("abort ");
                e.my_ast_debug(w);
            }
            E::Return(name, e) => {
                w.write("return");
                name.map(|name| w.write(format!(" '{} ", name)));
                if let Some(v) = e {
                    w.write(" ");
                    v.my_ast_debug(w);
                }
            }
            E::Break(name, e) => {
                w.write("break");
                name.map(|name| w.write(format!(" '{} ", name)));
                if let Some(v) = e {
                    w.write(" ");
                    v.my_ast_debug(w);
                }
            }
            E::Continue(name) => {
                w.write("continue");
                name.map(|name| w.write(format!(" '{}", name)));
            }
            E::Dereference(e) => {
                w.write("*");
                e.my_ast_debug(w)
            }
            E::UnaryExp(op, e) => {
                op.my_ast_debug(w);
                w.write(" ");
                e.my_ast_debug(w);
            }
            E::BinopExp(l, op, r) => {
                l.my_ast_debug(w);
                w.write(" ");
                op.my_ast_debug(w);
                w.write(" ");
                r.my_ast_debug(w)
            }
            E::Borrow(mut_, e) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                e.my_ast_debug(w);
            }
            E::Dot(e, n) => {
                e.my_ast_debug(w);
                w.write(&format!(".{}", n));
            }
            E::DotCall(e, n, is_macro, tyargs, sp!(_, rhs)) => {
                e.my_ast_debug(w);
                w.write(&format!(".{}", n));
                if is_macro.is_some() {
                    w.write("!");
                }
                if let Some(ts) = tyargs {
                    w.write("<");
                    ts.my_ast_debug(w);
                    w.write("<");
                }
                w.write("(");
                w.comma(rhs, |w, e| e.my_ast_debug(w));
                w.write(")");
            }
            E::Cast(e, ty) => {
                w.write("(");
                e.my_ast_debug(w);
                w.write(" as ");
                ty.my_ast_debug(w);
                w.write(")");
            }
            E::Index(e, rhs) => {
                e.my_ast_debug(w);
                w.write("[");
                w.comma(&rhs.value, |w, e| e.my_ast_debug(w));
                w.write("]");
            }
            E::Annotate(e, ty) => {
                w.write("(");
                e.my_ast_debug(w);
                w.write(": ");
                ty.my_ast_debug(w);
                w.write(")");
            }
            E::Spec(s) => {
                w.write(&s.value);
            }
            E::UnresolvedError => w.write("_|_"),
        }
    }
}

impl MyAstDebug for MatchArm_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let MatchArm_ {
            pattern,
            guard,
            rhs,
        } = self;
        pattern.my_ast_debug(w);
        if let Some(exp) = guard.as_ref() {
            w.write(" if ");
            exp.my_ast_debug(w);
        }
        w.write(" => ");
        rhs.my_ast_debug(w);
    }
}

impl<T: MyAstDebug> MyAstDebug for Ellipsis<T> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            Ellipsis::Ellipsis(_) => {
                w.write("..");
            }
            Ellipsis::Binder(p) => p.my_ast_debug(w),
        }
    }
}

impl MyAstDebug for Ellipsis<(Field, MatchPattern)> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            Ellipsis::Ellipsis(_) => {
                w.write("..");
            }
            Ellipsis::Binder((n, p)) => {
                w.write(&format!("{}: ", n));
                p.my_ast_debug(w);
            }
        }
    }
}

impl MyAstDebug for MatchPattern_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        use MatchPattern_::*;
        match self {
            PositionalConstructor(name, fields) => {
                name.my_ast_debug(w);
                w.write("(");
                w.comma(fields.value.iter(), |w, pat| {
                    pat.my_ast_debug(w);
                });
                w.write(") ");
            }
            FieldConstructor(name, fields) => {
                name.my_ast_debug(w);
                w.write(" {");
                w.comma(fields.value.iter(), |w, field_pat| field_pat.my_ast_debug(w));
                w.write("} ");
            }
            Name(mut_, name) => {
                if mut_.is_some() {
                    w.write("mut ");
                }
                name.my_ast_debug(w)
            }
            Literal(v) => v.my_ast_debug(w),
            Or(lhs, rhs) => {
                lhs.my_ast_debug(w);
                w.write(" | ");
                rhs.my_ast_debug(w);
            }
            At(x, pat) => {
                w.write(format!("{} @ ", x));
                pat.my_ast_debug(w);
            }
        }
    }
}

impl MyAstDebug for BinOp_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.write(&format!("{}", self));
    }
}

impl MyAstDebug for UnaryOp_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.write(&format!("{}", self));
    }
}

impl MyAstDebug for QuantKind_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            QuantKind_::Forall => w.write("forall"),
            QuantKind_::Exists => w.write("exists"),
            QuantKind_::Choose => w.write("choose"),
            QuantKind_::ChooseMin => w.write("min"),
        }
    }
}

impl MyAstDebug for Vec<BindWithRange> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let parens = self.len() != 1;
        if parens {
            w.write("(");
        }
        w.comma(self, |w, b| b.my_ast_debug(w));
        if parens {
            w.write(")");
        }
    }
}

impl MyAstDebug for (Bind, Exp) {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        self.0.my_ast_debug(w);
        w.write(" in ");
        self.1.my_ast_debug(w);
    }
}

impl MyAstDebug for Value_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        use Value_ as V;
        w.write(&match self {
            V::Address(addr) => format!("@{}", addr),
            V::Num(u) => u.to_string(),
            V::Bool(b) => format!("{}", b),
            V::HexString(s) => format!("x\"{}\"", s),
            V::ByteString(s) => format!("b\"{}\"", s),
        })
    }
}

impl MyAstDebug for Vec<Bind> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        let parens = self.len() != 1;
        if parens {
            w.write("(");
        }
        w.comma(self, |w, b| b.my_ast_debug(w));
        if parens {
            w.write(")");
        }
    }
}

impl MyAstDebug for Vec<Vec<Exp>> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        for trigger in self {
            w.write("{");
            w.comma(trigger, |w, b| b.my_ast_debug(w));
            w.write("}");
        }
    }
}

impl MyAstDebug for Bind_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        use Bind_ as B;
        match self {
            B::Var(mut_, v) => {
                if mut_.is_some() {
                    w.write("mut ");
                }
                w.write(&format!("{}", v))
            }
            B::Unpack(ma, fields) => {
                ma.my_ast_debug(w);
                fields.my_ast_debug(w);
            }
        }
    }
}

impl MyAstDebug for LambdaBindings_ {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        w.write("|");
        w.comma(self, |w, (b, ty_opt)| {
            b.my_ast_debug(w);
            if let Some(ty) = ty_opt {
                w.write(": ");
                ty.my_ast_debug(w);
            }
        });
        w.write("| ");
    }
}

impl MyAstDebug for Ellipsis<(Field, Bind)> {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            Ellipsis::Ellipsis(_) => {
                w.write("..");
            }
            Ellipsis::Binder((n, b)) => {
                w.write(&format!("{}: ", n));
                b.my_ast_debug(w);
            }
        }
    }
}

impl MyAstDebug for FieldBindings {
    fn my_ast_debug(&self, w: &mut AstWriter) {
        match self {
            FieldBindings::Named(bs) => {
                w.write("{");
                w.comma(bs, |w, e| {
                    e.my_ast_debug(w);
                });
                w.write("}");
            }
            FieldBindings::Positional(bs) => {
                w.write("(");
                w.comma(bs, |w, b| {
                    b.my_ast_debug(w);
                });
                w.write(")");
            }
        }
    }
}
