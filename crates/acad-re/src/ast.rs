//! The Ghidra export, as typed Rust. Mirrors `tools/ghidra/export_ast.py`;
//! the committed fixtures in `tests/fixtures/` are the contract between them.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Varnode {
    pub space: String,
    pub offset: i64,
    pub size: u32,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcodeOp {
    pub seq: String,
    pub op: String,
    pub out: Option<Varnode>,
    #[serde(rename = "in")]
    pub inputs: Vec<Option<Varnode>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClangNode {
    pub depth: u32,
    pub class: String,
    pub text: String,
    /// The P-Code op this C AST node links back to, when it has one.
    #[serde(default)]
    pub pcode: Option<String>,
}

/// A function Ghidra could not decompile. Recorded, never dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub address: String,
    pub name: String,
    pub block: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function<Body> {
    pub address: String,
    pub name: String,
    pub block: Option<String>,
    /// `halt_baddata` / `UNRECOVERED_JUMPTABLE` found in the C text.
    #[serde(default)]
    pub markers: Vec<String>,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcodeBody {
    #[serde(default)]
    pub ops: Vec<PcodeOp>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClangBody {
    #[serde(default)]
    pub nodes: Vec<ClangNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export<Body> {
    #[serde(default)]
    pub functions: Vec<Function<Body>>,
    #[serde(default)]
    pub failures: Vec<Failure>,
}

pub type PcodeExport = Export<PcodeBody>;
pub type ClangExport = Export<ClangBody>;

impl<Body: serde::de::DeserializeOwned + Default> Export<Body> {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    /// Functions with no `halt_baddata` and no `UNRECOVERED_JUMPTABLE` — the
    /// numerator of the §8 gate's 70% criterion.
    pub fn decompiled_cleanly(&self) -> impl Iterator<Item = &Function<Body>> {
        self.functions.iter().filter(|f| f.markers.is_empty())
    }
}
