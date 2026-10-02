//! Offline: recovered CFG + original image directory -> typed IR + standalone Rust.
use acad_re::{
    ir::{lower, verify_source, RecoveredExport},
    rust_emit,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err(
            "usage: re-lower CFG.json BLOCK ENTRY_HEX ORIGINAL_IMAGE_DIR NEW_OUTPUT_DIR".into(),
        );
    }
    let export: RecoveredExport = serde_json::from_slice(&fs::read(&args[0])?)?;
    let entry = u16::from_str_radix(args[2].trim_start_matches("0x"), 16)?;
    let candidates: Vec<_> = export
        .functions
        .iter()
        .filter(|f| f.block == args[1] && f.entry == entry)
        .collect();
    if candidates.len() != 1 {
        return Err("expected exactly one matching function".into());
    }
    let function = candidates[0];
    let original = PathBuf::from(&args[3]);
    let mut images = BTreeMap::new();
    for row in &function.instructions {
        // Export metadata must not escape the supplied image directory.
        if !matches!(row.file.as_str(), "ACAD.EXE" | "ACAD.OVL") {
            return Err("unknown original image".into());
        }
        if !images.contains_key(&row.file) {
            images.insert(row.file.clone(), fs::read(original.join(&row.file))?);
        }
    }
    verify_source(function, &images)?;
    let ir = lower(&export.architecture, function)?;
    let output = PathBuf::from(&args[4]);
    fs::create_dir(&output)?;
    fs::write(
        output.join("function.ir.json"),
        serde_json::to_vec_pretty(&ir)?,
    )?;
    fs::write(output.join("function.rs"), rust_emit::emit(&ir))?;
    println!(
        "lowered {}:{entry:04x}: {} instructions -> IR + Rust",
        function.block,
        ir.instructions.len()
    );
    Ok(())
}
