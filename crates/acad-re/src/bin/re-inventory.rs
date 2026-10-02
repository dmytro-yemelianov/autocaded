//! Report which recovered functions pass the bounded IR validator.
//! Acceptance here does not imply behavioral verification.
use acad_re::ir::{lower, verify_source, RecoveredExport};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: re-inventory CFG.json ORIGINAL_IMAGE_DIR NEW_REPORT.json".into());
    }
    let export: RecoveredExport = serde_json::from_slice(&fs::read(&args[0])?)?;
    let original = PathBuf::from(&args[1]);
    let mut images = BTreeMap::new();
    for file in ["ACAD.EXE", "ACAD.OVL"] {
        images.insert(file.to_string(), fs::read(original.join(file))?);
    }
    let mut rows = Vec::new();
    let mut accepted = 0;
    let mut rejected: BTreeMap<String, usize> = BTreeMap::new();
    for function in &export.functions {
        // Bad bytes fail the whole inventory, rather than becoming a feature gap.
        verify_source(function, &images)?;
        let error = match lower(&export.architecture, function) {
            Ok(_) => {
                accepted += 1;
                None
            }
            Err(error) => {
                *rejected.entry(error.clone()).or_default() += 1;
                Some(error)
            }
        };
        rows.push(serde_json::json!({"block": function.block, "entry": function.entry,
            "instructions": function.instructions.len(), "lowerable": error.is_none(), "reason": error}));
    }
    let report = serde_json::json!({"total": rows.len(), "lowerable": accepted,
        "behavioral_verification": "Only independently tested fixtures are verified; lowerable is a structural result.",
        "rejections": rejected, "functions": rows});
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    serde_json::to_writer_pretty(&mut out, &report)?;
    println!(
        "{accepted}/{} structurally lowerable; report {}",
        rows.len(),
        args[2]
    );
    Ok(())
}
