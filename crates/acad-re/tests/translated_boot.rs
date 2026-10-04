//! Optional replay using local recovery/corpus artifacts; no Ghidra or native VM.
use acad_re::{ir::RecoveredExport, whole_program::emit_translated_crate};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires recovered CFG and original ACAD images; run explicitly with --ignored"]
fn translated_boot_reaches_the_menu_and_accepts_exit() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = |key, default: &str| {
        std::env::var_os(key)
            .map(PathBuf::from)
            .unwrap_or_else(|| repo.join(default))
    };
    let cfg = path(
        "ACAD_RE_CFG",
        "target/acad-ir-20261002-01/export/recovered-cfg.json",
    );
    let images_dir = path("ACAD_RE_IMAGES", "target/acad-ir-20261002-01/input");
    let system_dir = path("ACAD_RE_SYSTEM", "corpus/System");
    let mut export: RecoveredExport = serde_json::from_slice(&fs::read(cfg).unwrap()).unwrap();
    let callbacks: RecoveredExport =
        serde_json::from_str(include_str!("fixtures/sprintf-callback-cfg.json")).unwrap();
    for function in callbacks.functions {
        if !export
            .functions
            .iter()
            .any(|f| f.block == function.block && f.entry == function.entry)
        {
            export.functions.push(function);
        }
    }
    let images = ["ACAD.EXE", "ACAD.OVL"]
        .into_iter()
        .map(|name| (name.to_string(), fs::read(images_dir.join(name)).unwrap()))
        .collect::<BTreeMap<_, _>>();
    let output_dir = repo.join("target/acad-boot-check");
    emit_translated_crate(&export, &images, &output_dir).unwrap();
    fs::create_dir_all(output_dir.join("tests")).unwrap();
    fs::write(
        output_dir.join("tests/boot.rs"),
        include_str!("fixtures/boot-reference.rs"),
    )
    .unwrap();
    let output = Command::new("cargo")
        .args(["test", "--offline", "--test", "boot", "--", "--nocapture"])
        .env("ACAD_RE_IMAGES", images_dir)
        .env("ACAD_RE_SYSTEM", system_dir)
        .current_dir(output_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "boot replay failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .take(50)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
