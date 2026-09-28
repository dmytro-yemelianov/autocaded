//! Measures the §8 decision gate from the exported AST.
use acad_re::{
    analysis::{CallGraph, Gate},
    ast::{ClangExport, PcodeExport},
};
use std::{fs, process::ExitCode};

fn load<T, F>(path: &str, parse: F) -> Result<T, String>
where
    F: Fn(&str) -> Result<T, serde_json::Error>,
{
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&text).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(pcode_path), Some(clang_path), Some(out)) = (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: re-report <ast-pcode.json> <ast-clang.json> <out.json>");
        return ExitCode::FAILURE;
    };

    let pcode = match load(&pcode_path, PcodeExport::from_json) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{pcode_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let clang = match load(&clang_path, ClangExport::from_json) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{clang_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let graph = CallGraph::from_pcode(&pcode);
    let gate = Gate::measure(&pcode, &graph);

    println!(
        "functions        {} ({} clean, {} marked, {} failed)",
        gate.total, gate.clean, gate.marked, gate.failed
    );
    println!(
        "clean ratio      {:.1}%  (§8 threshold 70%: {})",
        gate.clean_ratio * 100.0,
        if gate.meets_decompile_threshold() {
            "MET"
        } else {
            "NOT MET"
        }
    );
    println!(
        "non-code         {} functions outside any code block, {} marked \
         (Ghidra decompiling data as code; excluded above)",
        gate.non_code_functions, gate.non_code_marked
    );
    println!("call edges       {}", graph.edge_count());
    println!(
        "cross-overlay    {} edges, {} to named targets",
        gate.cross_overlay_edges, gate.cross_overlay_named
    );
    println!(
        "clang AST        {} functions, {} failures",
        clang.functions.len(),
        clang.failures.len()
    );

    match fs::write(&out, serde_json::to_string_pretty(&gate).unwrap()) {
        Ok(()) => {
            println!("wrote {out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cannot write {out}: {e}");
            ExitCode::FAILURE
        }
    }
}
