//! Analyses over the exported AST: who calls whom, and which overlay owns
//! which command.
use crate::ast::{Function, PcodeBody, PcodeExport};
use crate::ovl::Directory;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// A directed call graph over exported function addresses (`"seg:off"`).
#[derive(Debug, Default, Clone)]
pub struct CallGraph {
    edges: BTreeSet<(String, String)>,
    blocks: BTreeMap<String, String>,
}

/// The linear address Ghidra reports in varnodes, from either address form it
/// prints: `"1000:01a3"` in the default space is `seg * 16 + off`, and
/// `"OVL00_CODE::023310"` in an overlay space is already linear.
fn linear(addr: &str) -> Option<i64> {
    if let Some((_, flat)) = addr.split_once("::") {
        return i64::from_str_radix(flat, 16).ok();
    }
    let (seg, off) = addr.split_once(':')?;
    let seg = i64::from_str_radix(seg, 16).ok()?;
    let off = i64::from_str_radix(off, 16).ok()?;
    Some(seg * 16 + off)
}

/// The overlay space an address lives in, or `None` for the default space
/// (everything in `ACAD.EXE`).
fn space_of(addr: &str) -> Option<&str> {
    addr.split_once("::").map(|(space, _)| space)
}

impl CallGraph {
    pub fn from_pcode(export: &PcodeExport) -> Self {
        // The overlays share one window, so distinct overlay spaces hold
        // distinct functions at the same linear address. One address can
        // therefore have several candidates.
        let mut by_linear: BTreeMap<i64, Vec<&str>> = BTreeMap::new();
        for f in &export.functions {
            if let Some(l) = linear(&f.address) {
                by_linear.entry(l).or_default().push(f.address.as_str());
            }
        }

        let mut graph = Self::default();
        for f in &export.functions {
            if let Some(b) = &f.block {
                graph.blocks.insert(f.address.clone(), b.clone());
            }
        }
        for f in &export.functions {
            let caller_space = space_of(&f.address);
            for target in call_targets(f) {
                // Only edges to functions we actually exported. A CALL into
                // unanalysed bytes is a fact about Ghidra, not about the program.
                let Some(candidates) = by_linear.get(&target) else {
                    continue;
                };
                if let Some(callee) = resolve(candidates, caller_space) {
                    graph.edges.insert((f.address.clone(), callee.to_string()));
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

/// Pick the one function a call at some linear address means.
///
/// An overlay only ever has its own bytes and the kernel's mapped at once, so a
/// call resolves in the caller's own space first and the default space — all of
/// `ACAD.EXE` — second. If neither matches and more than one candidate remains,
/// the address is genuinely ambiguous between overlays that are never resident
/// together, and guessing would invent an edge.
fn resolve<'a>(candidates: &[&'a str], caller_space: Option<&str>) -> Option<&'a str> {
    if let Some(own) = candidates.iter().find(|c| space_of(c) == caller_space) {
        return Some(own);
    }
    if let Some(kernel) = candidates.iter().find(|c| space_of(c).is_none()) {
        return Some(kernel);
    }
    match candidates {
        [only] => Some(only),
        _ => None,
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

/// The §8 decision-gate metrics. Transpilation of the AST to Rust is adopted
/// only if `clean_ratio >= 0.70`, cross-overlay calls resolve to named targets,
/// and the DWG entity record is recoverable as a coherent struct. This type
/// measures the first two; the third is a judgement recorded alongside.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gate {
    /// Functions in a code block, plus functions that failed to decompile.
    pub total: usize,
    /// Decompiled with no `halt_baddata` and no `UNRECOVERED_JUMPTABLE`.
    pub clean: usize,
    pub marked: usize,
    pub failed: usize,
    pub clean_ratio: f64,
    /// Functions Ghidra placed outside any `*_CODE` block — in practice the
    /// EXE's data segment, where the overlay data windows live. Decompiling
    /// data as code produces these; they are reported but kept out of the
    /// population above, because they are not functions of the program.
    pub non_code_functions: usize,
    pub non_code_marked: usize,
    pub cross_overlay_edges: usize,
    /// Cross-overlay edges whose callee has a name Ghidra assigned, rather
    /// than a bare `FUN_` placeholder.
    pub cross_overlay_named: usize,
}

fn in_code_block<B>(f: &Function<B>) -> bool {
    f.block.as_deref().is_some_and(|b| b.ends_with("_CODE"))
}

impl Gate {
    pub fn measure(export: &PcodeExport, graph: &CallGraph) -> Self {
        let (code, non_code): (Vec<_>, Vec<_>) =
            export.functions.iter().partition(|f| in_code_block(f));
        let clean = code.iter().filter(|f| f.markers.is_empty()).count();
        let marked = code.len() - clean;
        let failed = export.failures.len();
        let total = code.len() + failed;
        let non_code_functions = non_code.len();
        let non_code_marked = non_code.iter().filter(|f| !f.markers.is_empty()).count();
        let named: BTreeMap<&str, &str> = export
            .functions
            .iter()
            .map(|f| (f.address.as_str(), f.name.as_str()))
            .collect();
        let edges = graph.cross_overlay_edges();
        let cross_overlay_named = edges
            .iter()
            .filter(|(_, to)| named.get(to).is_some_and(|n| !n.starts_with("FUN_")))
            .count();
        Self {
            total,
            clean,
            marked,
            failed,
            clean_ratio: if total == 0 {
                0.0
            } else {
                clean as f64 / total as f64
            },
            non_code_functions,
            non_code_marked,
            cross_overlay_edges: edges.len(),
            cross_overlay_named,
        }
    }

    /// The §8 threshold on decompiler quality.
    pub fn meets_decompile_threshold(&self) -> bool {
        self.clean_ratio >= 0.70
    }
}
