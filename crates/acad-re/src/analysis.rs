//! Analyses over the exported AST: who calls whom, and which overlay owns
//! which command.
use crate::ast::{Function, PcodeBody, PcodeExport};
use crate::ovl::Directory;
use std::collections::{BTreeMap, BTreeSet};

/// A directed call graph over exported function addresses (`"seg:off"`).
#[derive(Debug, Default, Clone)]
pub struct CallGraph {
    edges: BTreeSet<(String, String)>,
    blocks: BTreeMap<String, String>,
}

/// `"1000:01a3"` -> `0x101a3`, the linear address Ghidra reports in varnodes.
fn linear(addr: &str) -> Option<i64> {
    let (seg, off) = addr.split_once(':')?;
    let seg = i64::from_str_radix(seg, 16).ok()?;
    let off = i64::from_str_radix(off, 16).ok()?;
    Some(seg * 16 + off)
}

impl CallGraph {
    pub fn from_pcode(export: &PcodeExport) -> Self {
        let by_linear: BTreeMap<i64, &str> = export
            .functions
            .iter()
            .filter_map(|f| linear(&f.address).map(|l| (l, f.address.as_str())))
            .collect();

        let mut graph = Self::default();
        for f in &export.functions {
            if let Some(b) = &f.block {
                graph.blocks.insert(f.address.clone(), b.clone());
            }
        }
        for f in &export.functions {
            for target in call_targets(f) {
                // Only edges to functions we actually exported. A CALL into
                // unanalysed bytes is a fact about Ghidra, not about the program.
                if let Some(callee) = by_linear.get(&target) {
                    graph
                        .edges
                        .insert((f.address.clone(), (*callee).to_string()));
                }
            }
        }
        graph
    }

    pub fn callees_of<'a>(&'a self, addr: &str) -> Vec<&'a str> {
        self.edges
            .iter()
            .filter(|(from, _)| from == addr)
            .map(|(_, to)| to.as_str())
            .collect()
    }

    pub fn callers_of<'a>(&'a self, addr: &str) -> Vec<&'a str> {
        self.edges
            .iter()
            .filter(|(_, to)| to == addr)
            .map(|(from, _)| from.as_str())
            .collect()
    }

    /// Edges whose endpoints live in different overlay blocks. The §8 gate asks
    /// whether these resolve to named targets; this is how they are counted.
    pub fn cross_overlay_edges(&self) -> Vec<(&str, &str)> {
        self.edges
            .iter()
            .filter(
                |(from, to)| match (self.blocks.get(from), self.blocks.get(to)) {
                    (Some(a), Some(b)) => a != b && a.starts_with("OVL") && b.starts_with("OVL"),
                    _ => false,
                },
            )
            .map(|(from, to)| (from.as_str(), to.as_str()))
            .collect()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

fn call_targets(f: &Function<PcodeBody>) -> Vec<i64> {
    f.body
        .ops
        .iter()
        .filter(|op| op.op == "CALL" || op.op == "CALLIND")
        .filter_map(|op| op.inputs.first().and_then(|v| v.as_ref()).map(|v| v.offset))
        .collect()
}

/// The fewest words a run must hold before it is taken for the command table
/// rather than an ordinary uppercase message. The real table holds 57.
const MIN_TABLE_WORDS: usize = 8;

/// A byte that may appear inside the command table: the command names
/// themselves are uppercase, `?` is a command, and single spaces separate them.
fn table_byte(b: u8) -> bool {
    b.is_ascii_uppercase() || b == b' ' || b == b'?'
}

/// Command names are not stored one per NUL-terminated literal. They live in a
/// single NUL-terminated, space-separated table that the dispatcher scans by
/// index: `"LINE POINT CIRCLE SHAPE ... BREAK SKETCH FILES "`. Anything shorter
/// than `MIN_TABLE_WORDS` words is an ordinary uppercase message, not the table.
fn table_words(bytes: &[u8], from: usize, to: usize) -> Vec<String> {
    let mut out = Vec::new();
    let end = to.min(bytes.len());
    let mut i = from;
    while i < end {
        if !table_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < end && table_byte(bytes[i]) {
            i += 1;
        }
        // The table is NUL-terminated, like every other C string here.
        if i >= end || bytes[i] != 0 {
            continue;
        }
        let words: Vec<String> = String::from_utf8_lossy(&bytes[start..i])
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if words.len() >= MIN_TABLE_WORDS {
            out.extend(words);
        }
    }
    out
}

/// Which overlay's regions hold the command table, and what is in it.
///
/// This attributes commands to the overlay whose bytes carry the table, which
/// is where the names live — not to the overlay that implements each command.
/// The latter needs the dispatcher that indexes this table, and is not
/// recovered here.
pub fn commands(ovl_bytes: &[u8], dir: &Directory) -> BTreeMap<usize, Vec<String>> {
    let mut map: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for entry in &dir.entries {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for region in [entry.code, entry.data] {
            if region.is_empty() {
                continue;
            }
            let from = region.file_off as usize;
            let to = region.end() as usize;
            names.extend(table_words(ovl_bytes, from, to));
        }
        if !names.is_empty() {
            map.insert(entry.index, names.into_iter().collect());
        }
    }
    map
}
