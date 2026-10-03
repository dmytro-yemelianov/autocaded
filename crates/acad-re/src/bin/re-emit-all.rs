//! Whole-program code generator: recovered CFG + original images -> standalone compilable Rust crate.
use acad_re::{ir::RecoveredExport, whole_program::emit_translated_crate};
use std::{collections::BTreeMap, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!("usage: re-emit-all CFG.json ORIGINAL_IMAGE_DIR OUTPUT_DIR");
        return ExitCode::FAILURE;
    }

    let cfg_path = PathBuf::from(&args[0]);
    let img_dir = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);

    let export: RecoveredExport = match fs::read(&cfg_path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cannot read or parse {}: {e}", cfg_path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut images = BTreeMap::new();
    for name in ["ACAD.EXE", "ACAD.OVL"] {
        let p = img_dir.join(name);
        match fs::read(&p) {
            Ok(bytes) => {
                images.insert(name.to_string(), bytes);
            }
            Err(e) => {
                eprintln!("cannot read original image {}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        }
    }

    match emit_translated_crate(&export, &images, &out_dir) {
        Ok(count) => {
            println!(
                "successfully emitted whole-program crate with {count} functions to {}",
                out_dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("failed to emit translated crate: {e}");
            ExitCode::FAILURE
        }
    }
}
