// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::files::{Files, SimpleFiles};
use lsp_types::{Command, Location, Position};
use move_command_line_common::files::FileHash;
use move_ir_types::location::*;
use move_package::source_package::layout::SourcePackageLayout;
use move_symbol_pool::Symbol;
use std::{collections::HashMap, path::*, vec};

/// Converts a location from the byte index format to the line/character (Position) format, where
/// line/character are 0-based.
pub fn get_loc(
    fhash: &FileHash,
    pos: ByteIndex,
    files: &SimpleFiles<Symbol, String>,
    file_id_mapping: &HashMap<FileHash, usize>,
) -> Option<Position> {
    let id = match file_id_mapping.get(fhash) {
        Some(v) => v,
        None => return None,
    };
    match files.location(*id, pos as usize) {
        Ok(v) => Some(Position {
            // we need 0-based column location
            line: v.line_number as u32 - 1,
            character: v.column_number as u32 - 1,
        }),
        Err(_) => None,
    }
}

/// Double way mapping between FileHash and FilePath.
#[derive(Debug, Default)]
pub struct PathBufHashMap {
    path_2_hash: HashMap<PathBuf, FileHash>,
    hash_2_path: HashMap<FileHash, PathBuf>,
}

impl PathBufHashMap {
    pub fn update(&mut self, path: PathBuf, hash: FileHash) {
        if let Some(hash) = self.path_2_hash.get(&path) {
            self.hash_2_path.remove(hash);
        }
        self.path_2_hash.insert(path.clone(), hash);
        self.hash_2_path.insert(hash, path);
    }
    pub(crate) fn get_path(&self, hash: &FileHash) -> Option<&'_ PathBuf> {
        self.hash_2_path.get(hash)
    }
}

#[derive(Debug, Default)]
pub struct FileLineMapping {
    m: HashMap<PathBuf /* filepath */, Vec<ByteIndex>>,
}

impl FileLineMapping {
    pub fn update(&mut self, filepath: PathBuf, content: &str) {
        let mut v = vec![0];
        for (index, s) in content.as_bytes().iter().enumerate() {
            // TODO how to support windows \r\n
            if *s == 10 {
                // \n
                v.push((index + 1) as ByteIndex);
            }
        }
        if let Some(last) = content.as_bytes().last() {
            if *last != 10 {
                v.push((content.as_bytes().len()) as ByteIndex);
            }
        }
        self.m.insert(filepath, v);
    }

    pub(crate) fn translate(
        &self,
        filepath: &PathBuf,
        start_index: ByteIndex,
        mut end_index: ByteIndex,
    ) -> Option<FileRange> {
        if end_index < start_index {
            // maybe something goes wrong with syntax.rs
            // sometimes end_index < start_index.
            // this is a dummy fix.
            end_index = start_index;
        }
        let vec = self.m.get(filepath)?;
        let too_big = vec.last().map(|x| *x < end_index).unwrap_or(false);
        if too_big {
            log::error!(
                "end index too big. vec.last() = {:?}, end index = {:?}",
                vec.last(),
                end_index
            );
            return None;
        }
        fn search(vec: &[ByteIndex], byte_index: ByteIndex) -> (u32, u32) {
            let mut index = bisection::bisect_left(vec, &byte_index);
            if vec[index] != byte_index {
                index -= 1;
            }
            (index as u32, byte_index - vec[index as usize])
        }

        let (line_start, col_start) = search(&vec[..], start_index);
        let end = if let Some(t) = vec.get(line_start as usize + 1) {
            if *t > end_index {
                // Most case O(1) so we can have the same result but more fast.
                Some((line_start, end_index - vec[line_start as usize]))
            } else {
                None
            }
        } else {
            None
        };
        let (line_end, col_end) = end.unwrap_or_else(|| search(&vec[..], end_index));
        Some(FileRange {
            path: filepath.clone(),
            line_start,
            col_start,
            line_end,
            col_end,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRange {
    pub path: PathBuf,
    /// Start.
    pub line_start: u32,
    pub col_start: u32,

    /// End.
    pub line_end: u32,
    pub col_end: u32,
}

impl FileRange {
    pub fn mk_location(&self) -> lsp_types::Location {
        let range = lsp_types::Range {
            start: lsp_types::Position {
                line: self.line_start,
                character: self.col_start,
            },
            end: Position {
                line: self.line_end,
                character: self.col_end,
            },
        };
        let uri = url::Url::from_file_path(self.path.as_path()).unwrap();
        lsp_types::Location::new(uri, range)
    }
}

impl std::fmt::Display for FileRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}:({},{}):({},{})",
            self.path.as_path(),
            self.line_start,
            self.col_start,
            self.line_end,
            self.col_end
        )
    }
}
impl FileRange {
    pub(crate) fn unknown() -> Self {
        Self {
            path: PathBuf::from("<unknown>"),
            line_start: 0,
            col_start: 0,
            col_end: 0,
            line_end: 0,
        }
    }
}

/// Path concat from
pub fn path_concat(p1: &Path, p2: &Path) -> PathBuf {
    let p2: Vec<_> = p2.components().collect();
    let is_abs = matches!(
        p2.get(0).unwrap(),
        Component::RootDir | Component::Prefix(_)
    );
    let mut p1: Vec<_> = p1.components().collect();
    normal_path_components(if is_abs {
        &p2
    } else {
        {
            p1.extend(p2);
            &p1
        }
    })
}

/// concat Move.toml file.
pub fn path_concat_move_toml(p1: &Path, p2: &Path) -> PathBuf {
    let p1_is_move_toml = match p1.to_str() {
        Some(x) => x.ends_with(PROJECT_FILE_NAME),
        None => false,
    };
    if p1_is_move_toml {
        let mut p1 = p1.to_path_buf();
        p1.pop();
        path_concat(p1.as_path(), p2)
    } else {
        path_concat(p1, p2)
    }
}

pub fn normal_path_components(x: &Vec<Component<'_>>) -> PathBuf {
    let mut ret = PathBuf::new();
    for v in x {
        match v {
            Component::Prefix(x) => ret.push(x.as_os_str()),
            Component::RootDir => ret.push("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = ret.pop();
            }
            Component::Normal(x) => ret.push(*x),
        }
    }
    if ret.to_str().unwrap() == "" {
        ret.push(".")
    }
    ret
}

pub(crate) fn normal_path(p: &Path) -> PathBuf {
    let x: Vec<_> = p.components().collect();
    normal_path_components(&x)
}

pub trait GetPosition {
    fn get_position(&self) -> (PathBuf, u32 /* line */, u32 /* col */);
    fn in_range(x: &impl GetPosition, range: &FileRange) -> bool {
        let (filepath, line, col) = x.get_position();
        if filepath != range.path.clone() {
            return false;
        }
        if line < range.line_start {
            return false;
        }
        if line == range.line_start && col < range.col_start {
            return false;
        }
        if line > range.line_end {
            return false;
        }
        if line == range.line_end && col > range.col_end {
            return false;
        }
        true
    }
}

pub struct GetPositionStruct {
    pub fpath: PathBuf,
    pub line: u32,
    pub col: u32,
}

impl GetPosition for GetPositionStruct {
    fn get_position(&self) -> (PathBuf, u32 /* line */, u32 /* col */) {
        (self.fpath.clone(), self.line, self.col)
    }
}

pub fn discover_manifest_and_kind(x: &Path) -> Option<(PathBuf, SourcePackageLayout)> {
    let mut x: Vec<_> = x.components().collect();
    // We should be able at least pop one.
    x.pop()?;
    let mut layout: Option<&SourcePackageLayout> = None;
    while !x.is_empty() {
        while !x.is_empty() {
            layout = x
                .last()
                .and_then(|x| match x.as_os_str().to_str().unwrap() {
                    "tests" => Some(&SourcePackageLayout::Tests),
                    "sources" => Some(&SourcePackageLayout::Sources),
                    "scripts" => Some(&SourcePackageLayout::Scripts),
                    _ => None,
                });
            if layout.is_some() {
                break;
            }
            x.pop();
        }
        let layout = layout?;
        // Pop tests or sources ...
        x.pop()?;
        let mut manifest_dir = PathBuf::new();
        for x in x.iter() {
            manifest_dir.push(x);
        }
        // check if manifest exists.
        let mut manifest_file = manifest_dir.clone();
        manifest_file.push(PROJECT_FILE_NAME);
        if manifest_file.exists() {
            return Some((manifest_dir, layout.clone()));
        }
    }
    None
}

pub fn is_sub_dir(p: PathBuf, mut sub: PathBuf) -> bool {
    while sub.pop() {
        if p == sub {
            return true;
        }
    }
    false
}

/// There command should implemented in `LSP` client.
pub enum MoveAnalyzerClientCommands {
    GotoDefinition(Location),
}

impl MoveAnalyzerClientCommands {
    pub(crate) fn to_lsp_command(&self) -> Command {
        match self {
            MoveAnalyzerClientCommands::GotoDefinition(x) => Command::new(
                "Goto Definition".to_string(),
                "sui-move-analyzer.goto_definition".to_string(),
                Some(vec![serde_json::to_value(PathAndRange::from(x)).unwrap()]),
            ),
        }
    }
}
use lsp_types::Range;

#[derive(Clone, serde::Serialize)]
pub struct PathAndRange {
    range: Range,
    fpath: String,
}

impl From<&Location> for PathAndRange {
    fn from(value: &Location) -> Self {
        Self {
            range: value.range,
            fpath: value
                .uri
                .to_file_path()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        }
    }
}

pub const PROJECT_FILE_NAME: &str = "Move.toml";

use move_compiler::expansion::name_validation::{
    IMPLICIT_STD_MEMBERS, IMPLICIT_STD_MODULES, IMPLICIT_SUI_MEMBERS, IMPLICIT_SUI_MODULES,
};

use crate::types::BuildInType;

fn generate_module_ident_by_name(
    ident_address_name: &str,
    module_name: Symbol,
    default_loc: Loc,
) -> Spanned<move_compiler::parser::ast::ModuleIdent_> {
    let std_name = Spanned::new(default_loc, Symbol::from(ident_address_name));
    let module_symbol = Spanned::new(default_loc, module_name);

    let std_lead_access = move_compiler::parser::ast::LeadingNameAccess_::Name { 0: std_name };
    let module_name = move_compiler::parser::ast::ModuleName { 0: module_symbol };

    Spanned::new(
        default_loc,
        move_compiler::parser::ast::ModuleIdent_ {
            address: Spanned::new(default_loc, std_lead_access),
            module: module_name,
        },
    )
}

fn generate_buildin_type_use_decl_for_std_module(
    default_use_decl: &mut Vec<move_compiler::parser::ast::ModuleMember>,
    default_loc: Loc,
) {
    default_use_decl.extend(BuildInType::num_types().iter().map(|ty| {
        let module_symbol = Symbol::from(ty.to_static_str());
        let module_use = move_compiler::parser::ast::Use::ModuleUse {
            0: generate_module_ident_by_name("std", module_symbol, default_loc),
            1: move_compiler::parser::ast::ModuleUse::Module { 0: None },
        };
        move_compiler::parser::ast::ModuleMember::Use(move_compiler::parser::ast::UseDecl {
            doc: move_compiler::parser::ast::DocComment::empty(),
            loc: default_loc,
            attributes: vec![],
            use_: module_use,
        })
    }));
}

fn generate_default_use_decl_for_std_module(
    default_use_decl: &mut Vec<move_compiler::parser::ast::ModuleMember>,
    default_loc: Loc,
) {
    default_use_decl.extend(IMPLICIT_STD_MODULES.iter().copied().map(|module_symbol| {
        let module_use = move_compiler::parser::ast::Use::ModuleUse {
            0: generate_module_ident_by_name("std", module_symbol, default_loc),
            1: move_compiler::parser::ast::ModuleUse::Module { 0: None },
        };
        move_compiler::parser::ast::ModuleMember::Use(move_compiler::parser::ast::UseDecl {
            doc: move_compiler::parser::ast::DocComment::empty(),
            loc: default_loc,
            attributes: vec![],
            use_: module_use,
        })
    }));
}

fn generate_default_use_decl_for_sui_module(
    default_use_decl: &mut Vec<move_compiler::parser::ast::ModuleMember>,
    default_loc: Loc,
) {
    default_use_decl.extend(IMPLICIT_SUI_MODULES.iter().copied().map(|module_symbol| {
        let module_use = move_compiler::parser::ast::Use::ModuleUse {
            0: generate_module_ident_by_name("sui", module_symbol, default_loc),
            1: move_compiler::parser::ast::ModuleUse::Module { 0: None },
        };
        move_compiler::parser::ast::ModuleMember::Use(move_compiler::parser::ast::UseDecl {
            doc: move_compiler::parser::ast::DocComment::empty(),
            loc: default_loc,
            attributes: vec![],
            use_: module_use,
        })
    }));
}

fn generate_default_use_decl_for_sui_module_member(
    default_use_decl: &mut Vec<move_compiler::parser::ast::ModuleMember>,
    default_loc: Loc,
) {
    default_use_decl.extend(
        IMPLICIT_SUI_MEMBERS
            .iter()
            .copied()
            .map(|(m_sym, mem_sym, _)| {
                let module_use = move_compiler::parser::ast::Use::ModuleUse {
                    0: generate_module_ident_by_name("sui", m_sym, default_loc),
                    1: move_compiler::parser::ast::ModuleUse::Members {
                        0: vec![(Spanned::new(default_loc, mem_sym), None)],
                    },
                };
                move_compiler::parser::ast::ModuleMember::Use(move_compiler::parser::ast::UseDecl {
                    doc: move_compiler::parser::ast::DocComment::empty(),
                    loc: default_loc,
                    attributes: vec![],
                    use_: module_use,
                })
            }),
    );
}

fn generate_default_use_decl_for_std_module_member(
    default_use_decl: &mut Vec<move_compiler::parser::ast::ModuleMember>,
    default_loc: Loc,
) {
    default_use_decl.extend(
        IMPLICIT_STD_MEMBERS
            .iter()
            .copied()
            .map(|(m_sym, mem_sym, _)| {
                let module_use = move_compiler::parser::ast::Use::ModuleUse {
                    0: generate_module_ident_by_name("std", m_sym, default_loc),
                    1: move_compiler::parser::ast::ModuleUse::Members {
                        0: vec![(Spanned::new(default_loc, mem_sym), None)],
                    },
                };
                move_compiler::parser::ast::ModuleMember::Use(move_compiler::parser::ast::UseDecl {
                    doc: move_compiler::parser::ast::DocComment::empty(),
                    loc: default_loc,
                    attributes: vec![],
                    use_: module_use,
                })
            }),
    );
}

pub fn get_default_usedecl(file_hash: FileHash) -> Vec<move_compiler::parser::ast::ModuleMember> {
    let default_loc = Loc::new(file_hash, 0, 0);
    let mut default_use_decl = vec![];
    generate_default_use_decl_for_std_module(&mut default_use_decl, default_loc);
    generate_default_use_decl_for_sui_module(&mut default_use_decl, default_loc);
    generate_default_use_decl_for_sui_module_member(&mut default_use_decl, default_loc);
    generate_default_use_decl_for_std_module_member(&mut default_use_decl, default_loc);
    generate_buildin_type_use_decl_for_std_module(&mut default_use_decl, default_loc);
    return default_use_decl;
}
