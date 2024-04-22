// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module is responsible for building symbolication information on top of compiler's typed
//! AST, in particular identifier definitions to be used for implementing go-to-def and
//! go-to-references language server commands.
//!
//! There are two main structs that are used at different phases of the process, the Symbolicator
//! struct is used when building symbolication information and the Symbols struct is summarizes the
//! symbolication results and is used by the language server find definitions and references.
//!
//! Here is a brief description of how the symbolication information is encoded. Each identifier is
//! in the source code of a given module is represented by its location (UseLoc struct): line
//! number, starting and ending column, and hash of the source file where this identifier is
//! located). A definition for each identifier (if any - e.g., built-in type definitions are
//! excluded as there is no place in source code where they are defined) is also represented by its
//! location in the source code (DefLoc struct): line, starting column and a hash of the source
//! file where it's located. The symbolication process maps each identifier with its definition - a
//! per module map is keyed on the line number where the identifier is located, and the map entry
//! contains a list of identifier/definition pairs ordered by the column where the identifier starts.
//!
//! For example consider the following code fragment (0-based line numbers on the left and 0-based
//! column numbers at the bottom):
//!
//! 7: const SOME_CONST: u64 = 42;
//! 8:
//! 9: SOME_CONST + SOME_CONST
//!    |     |  |   | |      |
//!    0     6  9  13 15    22
//!
//! Symbolication information for this code fragment would look as follows assuming that this code
//! is stored in a file with hash FHASH (note that identifier in the definition of the constant maps
//! to itself):
//!
//! [7] -> [UseLoc(7:6-13, FHASH), DefLoc(7:6, FHASH)]
//! [9] -> [UseLoc(9:0-9 , FHASH), DefLoc((7:6, FHASH)], [UseLoc(9:13-22, FHASH), DefLoc((7:6, FHASH)]
//!
//! Including line number (and file hash) with the (use) identifier location may appear redundant,
//! but it's needed to allow accumulating uses with each definition to support
//! go-to-references. This is done in a global map from an identifier location (DefLoc) to a set of
//! use locations (UseLoc) - we find a all references of a given identifier by first finding its
//! definition and then using this definition as a key to the global map.
//!
//! Symbolication algorithm first analyzes all top-level definitions from all modules and then
//! processes function bodies and struct definitions to match uses to definitions. For local
//! definitions, the symbolicator builds a scope stack, entering encountered definitions and
//! matching uses to a definition in the innermost scope.

use crate::{
    context::Context,
    diagnostics::{lsp_diagnostics, lsp_empty_diagnostics}, project::{ConvertLoc, Project}, 
};
use crate::utils::discover_manifest_and_kind;
use anyhow::{anyhow, Result};
use codespan_reporting::files::SimpleFiles;
use crossbeam::channel::Sender;
use derivative::*;
// use im::ordmap::OrdMap;
use lsp_server::{Request, RequestId};
use lsp_types::{
    request::GotoTypeDefinitionParams, Diagnostic, DocumentSymbol, DocumentSymbolParams,
    GotoDefinitionParams, Hover, HoverContents, HoverParams, LanguageString, Location,
    MarkedString, Position, Range, ReferenceParams, SymbolKind,
};

use std::{
    cmp,
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};
use tempfile::tempdir;
use url::Url;

use move_command_line_common::files::FileHash;
use move_compiler::{
    expansion::ast::{Address, ModuleIdent_},
    naming::ast::{Type, TypeName_, Type_},
    parser::ast::{Definition, ModuleMember, StructDefinition, StructFields},
    shared::Identifier,
    PASS_TYPING,
};
// use move_ir_types::location::*;
use move_package::compilation::build_plan::BuildPlan;
use move_symbol_pool::Symbol;

/// Enabling/disabling the language server reporting readiness to support go-to-def and
/// go-to-references to the IDE.
pub const DEFS_AND_REFS_SUPPORT: bool = true;
// Building Move code requires a larger stack size on Windows (16M has been chosen somewhat
// arbitrarily)
pub const STACK_SIZE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Copy)]
/// Location of a definition's identifier
struct DefLoc {
    /// File where the definition of the identifier starts
    fhash: FileHash,
    /// Location where the definition of the identifier starts
    start: Position,
}

/// Location of a use's identifier
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Copy)]
struct UseLoc {
    /// File where this use identifier starts
    fhash: FileHash,
    /// Location where this use identifier starts
    start: Position,
    /// Column (on the same line as start)  where this use identifier ends
    col_end: u32,
}

/// Information about a type of an identifier. The reason we need an additional enum is that there
/// is not direct representation of a function type in the Type enum.
#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum IdentType {
    RegularType(Type),
    FunctionType(
        ModuleIdent_, /* defining module */
        Symbol,       /* name */
        Vec<Type>,    /* type args */
        Vec<Symbol>,  /* arg names */
        Vec<Type>,    /* arg types */
        Type,         /* ret */
        Vec<Type>,    /* acquires */
    ),
}

/// Information about both the use identifier (source file is specified wherever an instance of this
/// struct is used) and the definition identifier
#[derive(Debug, Clone, Eq)]
pub struct UseDef {
    /// Column where the (use) identifier location starts on a given line (use this field for
    /// sorting uses on the line)
    col_start: u32,
    /// Column where the (use) identifier location ends on a given line
    col_end: u32,
    /// Type of the (use) identifier
    use_type: IdentType,
    /// Location of the definition
    def_loc: DefLoc,
    /// Location of the type definition
    type_def_loc: Option<DefLoc>,
    /// Doc string for the relevant identifier/function
    doc_string: String,
}

/// Definition of a struct field
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
struct FieldDef {
    name: Symbol,
    start: Position,
}

/// Definition of a struct
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
struct StructDef {
    name_start: Position,
    field_defs: Vec<FieldDef>,
}

#[derive(Derivative, Debug, Clone, PartialEq, Eq)]
#[derivative(PartialOrd, Ord)]
pub struct FunctionDef {
    name: Symbol,
    start: Position,
    attrs: Vec<String>,
    #[derivative(PartialOrd = "ignore")]
    #[derivative(Ord = "ignore")]
    ident_type: IdentType,
}

/// Module-level definitions
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct ModuleDefs {
    /// File where this module is located
    fhash: FileHash,
    /// Location where this module is located
    start: Position,
    /// Module name
    name: ModuleIdent_,
    /// Struct definitions
    structs: BTreeMap<Symbol, StructDef>,
    /// Const definitions
    constants: BTreeMap<Symbol, Position>,
    /// Function definitions
    functions: BTreeMap<Symbol, FunctionDef>,
}

/// Data used during symbolication
pub struct Symbolicator {
    // /// Outermost definitions in a module (structs, consts, functions)
    // mod_outer_defs: BTreeMap<ModuleIdent_, ModuleDefs>,
    // /// A mapping from file names to file content (used to obtain source file locations)
    // files: SimpleFiles<Symbol, String>,
    // /// A mapping from file hashes to file IDs (used to obtain source file locations)
    // file_id_mapping: HashMap<FileHash, usize>,
    // // A mapping from file IDs to a split vector of the lines in each file (used to build docstrings)
    // file_id_to_lines: HashMap<usize, Vec<String>>,
    // /// Contains type params where relevant (e.g. when processing function definition)
    // type_params: BTreeMap<Symbol, DefLoc>,
    // /// Current processed module (always set before module processing starts)
    // current_mod: Option<ModuleIdent>,
}

/// Maps a line number to a list of use-def pairs on a given line (use-def set is sorted by
/// col_start)
#[derive(Debug, Clone, Eq, PartialEq)]
struct UseDefMap(BTreeMap<u32, BTreeSet<UseDef>>);

/// Maps a function name to its usage definition
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FunctionIdentTypeMap(BTreeMap<String, IdentType>);

/// Result of the symbolication process
pub struct Symbols {
    /// A map from def locations to all the references (uses)
    references: BTreeMap<DefLoc, BTreeSet<UseLoc>>,
    /// A mapping from uses to definitions in a file
    file_use_defs: BTreeMap<PathBuf, UseDefMap>,
    /// A mapping from file hashes to file names
    file_name_mapping: BTreeMap<FileHash, Symbol>,
    /// A mapping from filePath to ModuleDefs
    file_mods: BTreeMap<PathBuf, BTreeSet<ModuleDefs>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
enum RunnerState {
    Run(PathBuf),
    Wait,
    Quit,
}

/// Data used during symbolication running and symbolication info updating
pub struct SymbolicatorRunner {
    mtx_cvar: Arc<(Mutex<RunnerState>, Condvar)>,
}

impl ModuleDefs {
    pub fn functions(&self) -> &BTreeMap<Symbol, FunctionDef> {
        &self.functions
    }
}

impl fmt::Display for IdentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::RegularType(t) => {
                // Technically, we could use error_format function here to display the "regular"
                // type, but the original intent of this function is subtly different that we need
                // (i.e., to be used by compiler error messages) which, for example, results in
                // verbosity that is not needed here.
                //
                // It also seems like a reasonable idea to be able to tune user experience in the
                // IDE independently on how compiler error messages are generated.
                write!(f, "{}", type_to_ide_string(t))
            }
            Self::FunctionType(mod_ident, name, type_args, arg_names, arg_types, ret, acquires) => {
                let type_args_str = if !type_args.is_empty() {
                    let mut s = '<'.to_string();
                    s.push_str(&type_list_to_ide_string(type_args));
                    s.push('>');
                    s
                } else {
                    "".to_string()
                };
                let acquires_str = if !acquires.is_empty() {
                    let mut s = " acquires ".to_string();
                    s.push_str(&type_list_to_ide_string(acquires));
                    s
                } else {
                    "".to_string()
                };
                let ret_str = match ret {
                    sp!(_, Type_::Unit) => "".to_string(),
                    _ => format!(": {}", type_to_ide_string(ret)),
                };

                write!(
                    f,
                    "fun {}::{}::{}{}({}){}{}",
                    addr_to_ide_string(&mod_ident.address),
                    mod_ident.module.value(),
                    name,
                    type_args_str,
                    arg_list_to_ide_string(arg_names, arg_types),
                    ret_str,
                    acquires_str
                )
            }
        }
    }
}

fn arg_list_to_ide_string(names: &[Symbol], types: &[Type]) -> String {
    names
        .iter()
        .zip(types.iter())
        .map(|(n, t)| format!("{}: {}", n, type_to_ide_string(t)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn type_to_ide_string(sp!(_, t): &Type) -> String {
    match t {
        Type_::Unit => "()".to_string(),
        Type_::Ref(m, r) => format!("&{} {}", if *m { "mut" } else { "" }, type_to_ide_string(r)),
        Type_::Param(tp) => {
            format!("{}", tp.user_specified_name)
        }
        Type_::Apply(_, sp!(_, type_name), ss) => match type_name {
            TypeName_::Multiple(_) => {
                format!("({})", type_list_to_ide_string(ss))
            }
            TypeName_::Builtin(name) => {
                if ss.is_empty() {
                    format!("{}", name)
                } else {
                    format!("{}<{}>", name, type_list_to_ide_string(ss))
                }
            }
            TypeName_::ModuleType(sp!(_, module_ident), struct_name) => {
                let addr = addr_to_ide_string(&module_ident.address);
                format!(
                    "{}::{}::{}{}",
                    addr,
                    module_ident.module.value(),
                    struct_name,
                    if ss.is_empty() {
                        "".to_string()
                    } else {
                        format!("<{}>", type_list_to_ide_string(ss))
                    }
                )
            }
        },
        Type_::Anything => "_".to_string(),
        Type_::Var(_) => "invalid type (var)".to_string(),
        Type_::UnresolvedError => "invalid type (unresolved)".to_string(),
    }
}
// pub enum Address {
//     Numerical {
//         name: Option<Name>,
//         value: Spanned<NumericalAddress>,
//         // set to true when the same name is used across multiple packages
//         name_conflict: bool,
//     },
//     NamedUnassigned(Name),
// }
fn addr_to_ide_string(addr: &Address) -> String {
    // match addr {
    //     Address::Numerical(None, sp!(_, bytes), false) => format!("{}", bytes),
    //     Address::Numerical(Some(name), ..) => format!("{}", name),
    //     Address::NamedUnassigned(name) => format!("{}", name),
    // }
    match addr {
        Address::Numerical {
            name: None,
            value: sp!(_, bytes),
            name_conflict: true,
        } => format!("{}", bytes),
        Address::Numerical {
            name: None,
            value: sp!(_, bytes),
            name_conflict: false,
        } => format!("{}", bytes),
        Address::Numerical {
            name: Some(name),
            ..
        } => format!("{}", name),
        Address::NamedUnassigned(name) => format!("{}", name),
    }
    
}

fn type_list_to_ide_string(types: &[Type]) -> String {
    types
        .iter()
        .map(type_to_ide_string)
        .collect::<Vec<_>>()
        .join(", ")
}

impl SymbolicatorRunner {
    /// Create a new idle runner (one that does not actually symbolicate)
    pub fn idle() -> Self {
        let mtx_cvar = Arc::new((Mutex::new(RunnerState::Wait), Condvar::new()));
        SymbolicatorRunner { mtx_cvar }
    }

    /// Create a new runner
    pub fn new(
        symbols: Arc<Mutex<Symbols>>,
        sender: Sender<Result<BTreeMap<Symbol, Vec<Diagnostic>>>>,
    ) -> Self {
        let mtx_cvar = Arc::new((Mutex::new(RunnerState::Wait), Condvar::new()));
        let thread_mtx_cvar = mtx_cvar.clone();
        let runner = SymbolicatorRunner { mtx_cvar };

        thread::Builder::new()
            .stack_size(STACK_SIZE_BYTES)
            .spawn(move || {
                let (mtx, cvar) = &*thread_mtx_cvar;
                // Locations opened in the IDE (files or directories) for which manifest file is missing
                let mut missing_manifests = BTreeSet::new();
                // infinite loop to wait for symbolication requests
                eprintln!("starting symbolicator runner loop");
                loop {
                    let starting_path_opt = {
                        // hold the lock only as long as it takes to get the data, rather than through
                        // the whole symbolication process (hence a separate scope here)
                        let mut symbolicate = mtx.lock().unwrap();
                        match symbolicate.clone() {
                            RunnerState::Quit => break,
                            RunnerState::Run(root_dir) => {
                                *symbolicate = RunnerState::Wait;
                                Some(root_dir)
                            }
                            RunnerState::Wait => {
                                // wait for next request
                                symbolicate = cvar.wait(symbolicate).unwrap();
                                match symbolicate.clone() {
                                    RunnerState::Quit => break,
                                    RunnerState::Run(root_dir) => {
                                        *symbolicate = RunnerState::Wait;
                                        Some(root_dir)
                                    }
                                    RunnerState::Wait => None,
                                }
                            }
                        }
                    };
                    if let Some(starting_path) = starting_path_opt {
                        let root_dir = Self::root_dir(&starting_path);
                        if root_dir.is_none() && !missing_manifests.contains(&starting_path) {
                            eprintln!("reporting missing manifest");

                            // report missing manifest file only once to avoid cluttering IDE's UI in
                            // cases when developer indeed intended to open a standalone file that was
                            // not meant to compile
                            missing_manifests.insert(starting_path);
                            if let Err(err) = sender.send(Err(anyhow!(
                                "Unable to find package manifest. Make sure that
                            the source files are located in a sub-directory of a package containing
                            a Move.toml file. "
                            ))) {
                                eprintln!("could not pass missing manifest error: {:?}", err);
                            }
                            continue;
                        }
                        eprintln!("symbolication started");
                        match Symbolicator::get_symbols(root_dir.unwrap().as_path()) {
                            Ok((symbols_opt, lsp_diagnostics)) => {
                                eprintln!("symbolication finished");
                                if let Some(new_symbols) = symbols_opt {
                                    // merge the new symbols with the old ones to support a
                                    // (potentially) new project/package that symbolication information
                                    // was built for
                                    //
                                    // TODO: we may consider "unloading" symbolication information when
                                    // files/directories are being closed but as with other performance
                                    // optimizations (e.g. incrementalizatino of the vfs), let's wait
                                    // until we know we actually need it
                                    let mut old_symbols = symbols.lock().unwrap();
                                    (*old_symbols).merge(new_symbols);
                                }
                                // set/reset (previous) diagnostics
                                if let Err(err) = sender.send(Ok(lsp_diagnostics)) {
                                    eprintln!("could not pass diagnostics: {:?}", err);
                                }
                            }
                            Err(err) => {
                                eprintln!("symbolication failed: {:?}", err);
                                if let Err(err) = sender.send(Err(err)) {
                                    eprintln!("could not pass compiler error: {:?}", err);
                                }
                            }
                        }
                    }
                }
            })
            .unwrap();

        runner
    }

    pub fn run(&self, starting_path: PathBuf) {
        eprintln!("scheduling run for {:?}", starting_path);
        let (mtx, cvar) = &*self.mtx_cvar;
        let mut symbolicate = mtx.lock().unwrap();
        *symbolicate = RunnerState::Run(starting_path);
        cvar.notify_one();
        eprintln!("scheduled run");
    }

    pub fn quit(&self) {
        let (mtx, cvar) = &*self.mtx_cvar;
        let mut symbolicate = mtx.lock().unwrap();
        *symbolicate = RunnerState::Quit;
        cvar.notify_one();
    }

    /// Finds manifest file in a (sub)directory of the starting path passed as argument
    pub fn root_dir(starting_path: &Path) -> Option<PathBuf> {
        let mut current_path_opt = Some(starting_path);
        while current_path_opt.is_some() {
            let current_path = current_path_opt.unwrap();
            let manifest_path = current_path.join("Move.toml");
            if manifest_path.is_file() {
                return Some(current_path.to_path_buf());
            }
            current_path_opt = current_path.parent();
        }
        None
    }
}

impl UseDef {
    pub fn get_col_start(&self) -> u32 {
        self.col_start
    }

    pub fn get_col_end(&self) -> u32 {
        self.col_end
    }

    pub fn get_use_type(&self) -> &IdentType {
        &self.use_type
    }

    pub fn get_def_loc_fhash(&self) -> &FileHash {
        &self.def_loc.fhash
    }

    pub fn get_def_loc_start(&self) -> &Position {
        &self.def_loc.start
    }

    pub fn get_type_def_start(&self) -> Option<&Position> {
        self.type_def_loc.as_ref().map(|def_loc| &def_loc.start)
    }

    pub fn get_type_def_fhash(&self) -> Option<&FileHash> {
        self.type_def_loc.as_ref().map(|def_loc| &def_loc.fhash)
    }

    pub fn get_doc_string(&self) -> &str {
        &self.doc_string
    }
}

impl Ord for UseDef {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.col_start.cmp(&other.col_start)
    }
}

impl PartialOrd for UseDef {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for UseDef {
    fn eq(&self, other: &Self) -> bool {
        self.col_start == other.col_start
    }
}

impl UseDefMap {
    // fn new() -> Self {
    //     Self(BTreeMap::new())
    // }

    // fn insert(&mut self, key: u32, val: UseDef) {
    //     self.0.entry(key).or_insert_with(BTreeSet::new).insert(val);
    // }

    fn get(&self, key: u32) -> Option<BTreeSet<UseDef>> {
        self.0.get(&key).cloned()
    }

    // fn elements(self) -> BTreeMap<u32, BTreeSet<UseDef>> {
    //     self.0
    // }

    // fn extend(&mut self, use_defs: BTreeMap<u32, BTreeSet<UseDef>>) {
    //     self.0.extend(use_defs);
    // }
}

impl FunctionIdentTypeMap {
    // fn new() -> Self {
    //     Self(BTreeMap::new())
    // }

    pub fn contains_key(self, key: &String) -> bool {
        self.0.contains_key(key)
    }
}

impl Symbols {
    pub fn merge(&mut self, other: Self) {
        for (k, v) in other.references {
            self.references
                .entry(k)
                .or_insert_with(BTreeSet::new)
                .extend(v);
        }
        self.file_use_defs.extend(other.file_use_defs);
        self.file_name_mapping.extend(other.file_name_mapping);
        self.file_mods.extend(other.file_mods);
    }

    pub fn file_mods(&self) -> &BTreeMap<PathBuf, BTreeSet<ModuleDefs>> {
        &self.file_mods
    }
}

impl Symbols {
    pub fn get_file_use_defs(&self, file_path: &PathBuf) -> Option<BTreeMap<u32, BTreeSet<UseDef>>> {
        self.file_use_defs.get(file_path).map(|use_def_map| {
            let UseDefMap(map) = use_def_map;
            map.iter().map(|(&k, v)| (k, v.clone())).collect()
        })
    }

    pub fn get_file_name_mapping(&self) -> &BTreeMap<FileHash, Symbol> {
        &self.file_name_mapping
    }

    pub fn get_file_mods(&self, file_path: &PathBuf) -> Option<&BTreeSet<ModuleDefs>> {
        self.file_mods.get(file_path)
    }
}

impl Symbolicator {
    /// Main driver to get symbols for the whole package. Returned symbols is an option as only the
    /// correctly computed symbols should be a replacement for the old set - if symbols are not
    /// actually (re)computed and the diagnostics are returned, the old symbolic information should
    /// be retained even if it's getting out-of-date.
    pub fn get_symbols(
        pkg_path: &Path,
    ) -> Result<(Option<Symbols>, BTreeMap<Symbol, Vec<Diagnostic>>)> {
        let build_config = move_package::BuildConfig {
            test_mode: true,
            install_dir: Some(tempdir().unwrap().path().to_path_buf()),
            ..Default::default()
        };

        eprintln!("symbolicating {:?}", pkg_path);

        // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
        // vector as the writer
        let resolution_graph =
            build_config.resolution_graph_for_package(pkg_path, &mut Vec::new())?;

        // get source files to be able to correlate positions (in terms of byte offsets) with actual
        // file locations (in terms of line/column numbers)
        let source_files = &resolution_graph.file_sources();
        let mut files = SimpleFiles::new();
        let mut file_id_mapping = HashMap::new();
        let mut file_id_to_lines = HashMap::new();
        let mut file_name_mapping = BTreeMap::new();
        for (fhash, (fname, source)) in source_files {
            let id = files.add(*fname, source.clone());
            file_id_mapping.insert(*fhash, id);
            file_name_mapping.insert(*fhash, *fname);
            let lines: Vec<String> = source.lines().map(String::from).collect();
            file_id_to_lines.insert(id, lines);
        }

        let build_plan = BuildPlan::create(resolution_graph)?;
        let mut typed_ast = None;
        let mut diagnostics = None;
        build_plan.compile_with_driver(&mut std::io::sink(), |compiler| {
            let (files, compilation_result) = compiler.run::<PASS_TYPING>()?;
            let (_, compiler) = match compilation_result {
                Ok(v) => v,
                Err(diags) => {
                    let failure = true;
                    diagnostics = Some((diags, failure));
                    eprintln!("typed AST compilation failed");
                    return Ok((files, vec![]));
                }
            };
            eprintln!("compiled to typed AST");
            let (compiler, typed_program) = compiler.into_ast();
            typed_ast = Some(typed_program.clone());
            eprintln!("compiling to bytecode");
            let compilation_result = compiler.at_typing(typed_program).build();
            let (units, diags) = match compilation_result {
                Ok(v) => v,
                Err(diags) => {
                    let failure = false;
                    diagnostics = Some((diags, failure));
                    eprintln!("bytecode compilation failed");
                    return Ok((files, vec![]));
                }
            };
            // warning diagnostics (if any) since compilation succeeded
            if !diags.is_empty() {
                // assign only if non-empty, otherwise return None to reset previous diagnostics
                let failure = false;
                diagnostics = Some((diags, failure));
            }
            eprintln!("compiled to bytecode");
            Ok((files, units))
        })?;

        let mut ide_diagnostics = lsp_empty_diagnostics(&file_name_mapping);
        if let Some((compiler_diagnostics, failure)) = diagnostics {
            let lsp_diagnostics = lsp_diagnostics(
                &compiler_diagnostics.into_codespan_format(),
                &files,
                &file_id_mapping,
                &file_name_mapping,
            );
            // start with empty diagnostics for all files and replace them with actual diagnostics
            // only for files that have failures/warnings so that diagnostics for all other files
            // (that no longer have failures/warnings) are reset
            ide_diagnostics.extend(lsp_diagnostics);
            if failure {
                // just return diagnostics as we don't have typed AST that we can use to compute
                // symbolication information
                debug_assert!(typed_ast.is_none());
                return Ok((None, ide_diagnostics));
            }
        }

        // let modules = &typed_ast.unwrap().inner.modules;

        // let mut mod_outer_defs = BTreeMap::new();
        // let mut mod_use_defs = BTreeMap::new();
        let file_mods = BTreeMap::new();

        // for (pos, module_ident, module_def) in modules {
        //     eprintln!("pos = {:?}, module_name ={:?}", pos, module_ident.module);
        //     eprintln!(" module_name ={:?}",module_ident.module.clone().to_string());
            // let (defs, symbols) = Self::get_mod_outer_defs(
            //     &pos,
            //     &sp(pos, *module_ident),
            //     module_def,
            //     &files,
            //     &file_id_mapping,
            // );

            // let cloned_defs = defs.clone();
            // let path = file_name_mapping.get(&cloned_defs.fhash.clone()).unwrap();
            // file_mods
            //     .entry(
            //         dunce::canonicalize(path.as_str())
            //             .unwrap_or_else(|_| PathBuf::from(path.as_str())),
            //     )
            //     .or_insert_with(BTreeSet::new)
            //     .insert(cloned_defs);

            // mod_outer_defs.insert(*module_ident, defs);
            // mod_use_defs.insert(*module_ident, symbols);
        // }

        // eprintln!("get_symbols loaded file_mods length: {}", file_mods.len());

        // let mut symbolicator = Symbolicator {
        //     mod_outer_defs,
        //     files,
        //     file_id_mapping,
        //     file_id_to_lines,
        //     type_params: BTreeMap::new(),
        //     current_mod: None,
        // };

        let references = BTreeMap::new();
        let file_use_defs = BTreeMap::new();
        // let function_ident_type = FunctionIdentTypeMap::new();

        // for (pos, module_ident, module_def) in modules {
        //     let mut use_defs = mod_use_defs.remove(module_ident).unwrap();
        //     symbolicator.current_mod = Some(sp(pos, *module_ident));
        //     symbolicator.mod_symbols(
        //         module_def,
        //         &mut references,
        //         &mut use_defs,
        //         &mut function_ident_type,
        //     );

        //     let fpath = match source_files.get(&pos.file_hash()) {
        //         Some((p, _)) => p,
        //         None => continue,
        //     };

        //     let fpath_buffer = dunce::canonicalize(fpath.as_str())
        //         .unwrap_or_else(|_| PathBuf::from(fpath.as_str()));

        //     file_use_defs
        //         .entry(fpath_buffer)
        //         .or_insert_with(UseDefMap::new)
        //         .extend(use_defs.elements());
        // }

        let symbols = Symbols {
            references,
            file_use_defs,
            file_name_mapping,
            file_mods,
        };

        eprintln!("get_symbols load complete");

        Ok((Some(symbols), ide_diagnostics))
    }

    /// Get empty symbols
    pub fn empty_symbols() -> Symbols {
        Symbols {
            file_use_defs: BTreeMap::new(),
            references: BTreeMap::new(),
            file_name_mapping: BTreeMap::new(),
            file_mods: BTreeMap::new(),
        }
    }

}

/// Handles go-to-def request of the language server
pub fn on_go_to_def_request(context: &Context, request: &Request, symbols: &Symbols) {
    let parameters = serde_json::from_value::<GotoDefinitionParams>(request.params.clone())
        .expect("could not deserialize go-to-def request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| {
            // TODO: Do we need beginning and end of the definition? Does not seem to make a
            // difference from the IDE perspective as the cursor goes to the beginning anyway (at
            // least in VSCode).
            let range = Range {
                start: u.def_loc.start,
                end: u.def_loc.start,
            };
            let path = symbols.file_name_mapping.get(&u.def_loc.fhash).unwrap();
            let loc = Location {
                uri: Url::from_file_path(path.as_str()).unwrap(),
                range,
            };
            Some(serde_json::to_value(loc).unwrap())
        },
    );
}

/// Handles go-to-type-def request of the language server
pub fn on_go_to_type_def_request(context: &Context, request: &Request, symbols: &Symbols) {
    let parameters = serde_json::from_value::<GotoTypeDefinitionParams>(request.params.clone())
        .expect("could not deserialize go-to-type-def request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| match u.type_def_loc {
            Some(def_loc) => {
                let range = Range {
                    start: def_loc.start,
                    end: def_loc.start,
                };
                let path = symbols.file_name_mapping.get(&u.def_loc.fhash).unwrap();
                let loc = Location {
                    uri: Url::from_file_path(path.as_str()).unwrap(),
                    range,
                };
                Some(serde_json::to_value(loc).unwrap())
            }
            None => Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap()),
        },
    );
}

/// Handles go-to-references request of the language server
pub fn on_references_request(context: &Context, request: &Request, symbols: &Symbols) {
    let parameters = serde_json::from_value::<ReferenceParams>(request.params.clone())
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
    let include_decl = parameters.context.include_declaration;

    on_use_request(
        context,
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| match symbols.references.get(&u.def_loc) {
            Some(s) => {
                let mut locs = vec![];
                for ref_loc in s {
                    if include_decl
                        || !(u.def_loc.start == ref_loc.start && u.def_loc.fhash == ref_loc.fhash)
                    {
                        let end_pos = Position {
                            line: ref_loc.start.line,
                            character: ref_loc.col_end,
                        };
                        let range = Range {
                            start: ref_loc.start,
                            end: end_pos,
                        };
                        let path = symbols.file_name_mapping.get(&ref_loc.fhash).unwrap();
                        locs.push(Location {
                            uri: Url::from_file_path(path.as_str()).unwrap(),
                            range,
                        });
                    }
                }
                if locs.is_empty() {
                    Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap())
                } else {
                    Some(serde_json::to_value(locs).unwrap())
                }
            }
            None => Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap()),
        },
    );
}

/// Handles hover request of the language server
pub fn on_hover_request(context: &Context, request: &Request, symbols: &Symbols) {
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

    on_use_request(
        context,
        symbols,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u| {
            let lang_string = LanguageString {
                language: "".to_string(),
                value: if !u.doc_string.is_empty() {
                    format!("{}\n\n{}", u.use_type, u.doc_string)
                } else {
                    format!("{}", u.use_type)
                },
            };
            let contents = HoverContents::Scalar(MarkedString::LanguageString(lang_string));
            let range = None;
            Some(serde_json::to_value(Hover { contents, range }).unwrap())
        },
    );
}

/// Helper function to handle language server queries related to identifier uses
pub fn on_use_request(
    context: &Context,
    symbols: &Symbols,
    use_fpath: &PathBuf,
    use_line: u32,
    use_col: u32,
    id: RequestId,
    use_def_action: impl Fn(&UseDef) -> Option<serde_json::Value>,
) {
    let mut result = None;

    let mut use_def_found = false;
    if let Some(mod_symbols) = symbols.file_use_defs.get(use_fpath) {
        if let Some(uses) = mod_symbols.get(use_line) {
            for u in uses {
                if use_col >= u.col_start && use_col <= u.col_end {
                    result = use_def_action(&u);
                    use_def_found = true;
                }
            }
        }
    }
    if !use_def_found {
        result = Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap());
    }

    // unwrap will succeed based on the logic above which the compiler is unable to figure out
    // without using Option
    let response = lsp_server::Response::new_ok(id, result.unwrap());
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send use response: {:?}", err);
    }
}

/// Handles document symbol request of the language server
#[allow(deprecated)]
pub fn on_document_symbol_request(context: &Context, request: &Request, symbols: &Symbols) {
    eprintln!("on_document_symbol_request: {:?}", request);
    let parameters = serde_json::from_value::<DocumentSymbolParams>(request.params.clone())
        .expect("could not deserialize document symbol request");
    let fpath = parameters.text_document.uri.to_file_path().unwrap();
    eprintln!("symbol_request file path = {:?}", fpath.as_path());
    
    let path_project = match context.projects.get_project(&fpath) {
        Some(x) => x,
        None => {
            log::error!("project not found:{:?}", fpath.as_path());
            return ;
        }
    };
    
    let (manifest_path, _) = match discover_manifest_and_kind(fpath.as_path()) {
        Some(x) => x,
        None => {
            log::error!("project not found:{:?}", fpath.as_path());
            return ;
        }
    };

    let d2 = Default::default();
    let b = path_project
        .modules
        .get(&manifest_path)
        .unwrap_or(&d2)
        .as_ref()
        .borrow();

    let mut result_defs: Vec<DocumentSymbol> = vec![];
    let vec_defs_defaule = Default::default();
    let vec_defs =  b.sources.get(&fpath).unwrap_or(&vec_defs_defaule);
    eprintln!("get Definition, {:?}", !vec_defs.is_empty());
    for def in vec_defs.iter() {
        match def {
            Definition::Module(def_module) => {
                eprintln!("handle symbol, Module, {:?}", def_module.name);
                
                let range = match path_project.loc_to_range(&def_module.loc) {
                    Some(x) => x,
                    None => {
                        log::error!("Could not covert Definition::Module({:?}).loc to range", def_module.name);
                        log::error!("Module Loc start = {:?}, end = {:?}", def_module.loc.start(), def_module.loc.end());
                        return ;
                    }
                };

                let name = def_module.name.clone().to_string();
                let detail = Some(def_module.name.clone().to_string());
                let kind = SymbolKind::MODULE;

                let mut children = vec![];
                for def_module_member in def_module.members.iter() {
                    match def_module_member {
                        ModuleMember::Function(x) => {
                            let func_range = match path_project.loc_to_range(&x.loc) {
                                Some(x) => x,
                                None => {
                                    log::error!("Could not covert ModuleMember::Function({:?}).loc to range", x.name);
                                    return ;
                                }
                            };
                            
                            children.push(DocumentSymbol {
                                name: x.name.to_string(),
                                detail:None,
                                kind: SymbolKind::FUNCTION,
                                range: func_range,
                                selection_range: func_range,
                                children: None,
                                tags: Some(vec![]),
                                deprecated: Some(false),
                            });
                        
                        }, // match def_module_member => function
                        ModuleMember::Struct(x) => {
                            let struct_range = match path_project.loc_to_range(&x.loc) {
                                Some(x) => x,
                                None => {
                                    log::error!("Could not covert ModuleMember::Struct({:?}).loc to range", x.name);
                                    return ;
                                }
                            };
                            let mut fields: Vec<DocumentSymbol> = vec![];
                            handle_struct_fields(path_project, x.clone(), &mut fields);
                
                            children.push(DocumentSymbol {
                                name: x.name.to_string(),
                                detail: None,
                                kind: SymbolKind::STRUCT,
                                range: struct_range,
                                selection_range: struct_range,
                                children: Some(fields),
                                tags: Some(vec![]),
                                deprecated: Some(false),
                            });
                        }, // match def_module_member => function
                        ModuleMember::Constant(x) => {
                            let const_range = match path_project.loc_to_range(&x.loc) {
                                Some(x) => x,
                                None => {
                                    log::error!("Could not covert ModuleMember::Const({:?}).loc to range", x.name);
                                    return ;
                                }
                            };

                            children.push(DocumentSymbol {
                                name: x.name.clone().to_string(),
                                detail: None,
                                kind: SymbolKind::CONSTANT,
                                range: const_range,
                                selection_range: const_range,
                                children: None,
                                tags: Some(vec![]),
                                deprecated: Some(false),
                            });
                        }, // match def_module_member => const
                        _ => {},
                    } // match def_module_member
                } // for def_module_member in def.member
                
                result_defs.push(DocumentSymbol {
                    name,
                    detail,
                    kind,
                    range,
                    selection_range: range,
                    children: Some(children),
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
                eprintln!("handle symbol, Module, {:?}, Success.", def_module.name);

            }, // match def => Definition::Module
            _ => {

            }
        } // match def
    } // for def in vec_defs

    // unwrap will succeed based on the logic above which the compiler is unable to figure out
    let response = lsp_server::Response::new_ok(
        request.id.clone(), 
        result_defs
    );
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send use response: {:?}", err);
    }
    eprintln!("on_document_symbol_request Success");
}

/// Helper function to handle struct fields for VSCode outline
/// author: zx
#[allow(deprecated)]
fn handle_struct_fields(project: &Project, struct_def: StructDefinition, fields: &mut Vec<DocumentSymbol>) {
    let clonded_fileds = struct_def.fields;
    match clonded_fileds {
        StructFields::Defined(x) => {
            for (struct_field,spanned_type) in x.iter() {
                let file_range = match project.convert_loc_range(&spanned_type.loc) {
                    Some(x) => x,
                    None => {
                        log::error!("could not convert StructFields::Defined({:?}).loc to range", struct_field);
                        return ;
                    }
                };
                let struct_field_range = lsp_types::Range {
                    start: lsp_types::Position {
                        line: file_range.line_start,
                        character: file_range.col_start,
                    },
                    end: Position {
                        line: file_range.line_end,
                        character: file_range.col_end,
                    },
                };

                fields.push(DocumentSymbol {
                    name: struct_field.clone().to_string(),
                    detail: None,
                    kind: SymbolKind::FIELD,
                    range: struct_field_range,
                    selection_range: struct_field_range,
                    children: None,
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }
        }
        StructFields::Native(_) => {
            eprintln!("struct filed Native not handle");
        }
        StructFields::Positional(_) => {
            eprintln!("struct filed Positional not handle");
        }
    }
}
