use acad_re::{
    ir::{lower, RecoveredExport, RecoveredFunction},
    whole_program::{
        block_to_mod, emit_function, emit_translated_crate, function_symbol, SymbolTable,
    },
};
use std::{
    collections::BTreeMap,
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn module_naming_and_function_symbols() {
    assert_eq!(block_to_mod("EXE_CODE"), "exe_code");
    assert_eq!(block_to_mod("OVL00_CODE"), "ovl00_code");
    assert_eq!(block_to_mod("OVL01_CODE"), "ovl01_code");
    assert_eq!(block_to_mod("OVL02_CODE"), "ovl02_code");
    assert_eq!(block_to_mod("OVL03_CODE"), "ovl03_code");
    assert_eq!(block_to_mod("OVL04_CODE"), "ovl04_code");
    assert_eq!(block_to_mod("OVL05_CODE"), "ovl05_code");
    assert_eq!(block_to_mod("OVL06_CODE"), "ovl06_code");
    assert_eq!(block_to_mod("OVL07_CODE"), "ovl07_code");
    assert_eq!(block_to_mod("OVL08_CODE"), "ovl08_code");
    assert_eq!(block_to_mod("OVL09_CODE"), "ovl09_code");
    assert_eq!(block_to_mod("OVL10_CODE"), "ovl10_code");
    assert_eq!(block_to_mod("OVL11_CODE"), "ovl11_code");
    assert_eq!(block_to_mod("CUSTOM"), "unknown");

    assert_eq!(function_symbol("EXE_CODE", 0x0100), "fn_exe_0100");
    assert_eq!(function_symbol("OVL00_CODE", 0x1234), "fn_ovl00_1234");
    assert_eq!(function_symbol("OVL01_CODE", 0x4a6e), "fn_ovl01_4a6e");
    assert_eq!(function_symbol("OVL02_CODE", 0xf809), "fn_ovl02_f809");
    assert_eq!(function_symbol("OVL07_CODE", 0x0360), "fn_ovl07_0360");
    assert_eq!(function_symbol("OTHER", 0x0042), "fn_unk_0042");
}

fn make_mock_function(block: &str, entry: u16) -> RecoveredFunction {
    RecoveredFunction {
        block: block.into(),
        entry,
        instructions: vec![],
        inline_data: vec![],
        dependencies: vec![],
        edges: vec![],
        errors: vec![],
    }
}

#[test]
fn symbol_table_resolution_hierarchy() {
    let funcs = vec![
        make_mock_function("EXE_CODE", 0x0100),
        make_mock_function("EXE_CODE", 0x0200),
        make_mock_function("OVL01_CODE", 0x4a6e),
        make_mock_function("OVL02_CODE", 0xf809),
    ];
    let symbols = SymbolTable::new(&funcs);

    assert_eq!(symbols.len(), 4);
    assert!(!symbols.is_empty());
    assert!(symbols.contains("EXE_CODE", 0x0100));
    assert!(!symbols.contains("EXE_CODE", 0x9999));
    assert!(symbols.contains("OVL01_CODE", 0x4a6e));
    assert!(symbols.contains("OVL02_CODE", 0xf809));

    // 1. Intra-block call resolution
    assert_eq!(
        symbols.resolve_call("OVL01_CODE", 0x24a6e),
        Some(("OVL01_CODE".into(), 0x4a6e))
    );

    // 2. Resident kernel call resolution
    assert_eq!(
        symbols.resolve_call("OVL01_CODE", 0x10100),
        Some(("EXE_CODE".into(), 0x0100))
    );

    // 3. Shared overlay helper call resolution (OVL02)
    assert_eq!(
        symbols.resolve_call("OVL01_CODE", 0x2f809),
        Some(("OVL02_CODE".into(), 0xf809))
    );

    // 4. Unresolved call target
    assert_eq!(symbols.resolve_call("OVL01_CODE", 0x19999), None);

    // 5. Tailcall resolution hierarchy
    assert_eq!(
        symbols.resolve_tailcall("OVL01_CODE", "OVL02_CODE", 0x2f809),
        Some(("OVL02_CODE".into(), 0xf809))
    );
    assert_eq!(
        symbols.resolve_tailcall("OVL01_CODE", "NONEXISTENT", 0x24a6e),
        Some(("OVL01_CODE".into(), 0x4a6e))
    );
    assert_eq!(
        symbols.resolve_tailcall("OVL01_CODE", "NONEXISTENT", 0x10100),
        Some(("EXE_CODE".into(), 0x0100))
    );
    assert_eq!(
        symbols.resolve_tailcall("OVL01_CODE", "NONEXISTENT", 0x2f809),
        Some(("OVL02_CODE".into(), 0xf809))
    );
    assert_eq!(
        symbols.resolve_tailcall("OVL01_CODE", "NONEXISTENT", 0x19999),
        None
    );
}

#[test]
fn single_function_emit_linked_mode() {
    let export: RecoveredExport =
        serde_json::from_str(include_str!("fixtures/dim-shift-cfg.json")).unwrap();
    let symbols = SymbolTable::new(&export.functions);
    let ir = lower(&export.architecture, &export.functions[0]).unwrap();
    let emitted = emit_function(&ir, &symbols);

    assert!(emitted.contains("pub fn fn_ovl04_1778(registers: &mut [u8; 8192], memory: &mut [u8], budget: &mut usize) -> Result<u64, crate::runtime::Trap>"));
    assert!(emitted.contains("while *budget > 0 {"));
    assert!(emitted.contains("*budget -= 1;"));
    assert!(emitted.contains("Err(crate::runtime::Trap::Budget)"));
}

#[test]
fn whole_program_emit_translated_crate_and_compiles() {
    let export: RecoveredExport =
        serde_json::from_str(include_str!("fixtures/dim-shift-cfg.json")).unwrap();

    let bytes = [
        0xe3, 0x0e, 0x83, 0xf9, 0x20, 0x76, 0x03, 0xb9, 0x20, 0x00, 0xd1, 0xe0, 0xd1, 0xd2, 0xe2,
        0xfa, 0xc3,
    ];
    let mut image = vec![0; 82313];
    image[57784..57784 + bytes.len()].copy_from_slice(&bytes);
    image[82296..82296 + bytes.len()].copy_from_slice(&bytes);

    let images = BTreeMap::from([("ACAD.OVL".into(), image)]);

    let root = std::env::temp_dir().join(format!(
        "acad-translated-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());

    let count = emit_translated_crate(&export, &images, &root).expect("emit failed");
    assert_eq!(count, 2);

    assert!(root.join("Cargo.toml").exists());
    assert!(root.join("src/runtime.rs").exists());
    assert!(root.join("src/lib.rs").exists());
    assert!(root.join("src/ovl01_code.rs").exists());
    assert!(root.join("src/ovl04_code.rs").exists());

    let output = Command::new("cargo")
        .args(["check", "--offline"])
        .current_dir(&root)
        .output()
        .expect("failed to run cargo check");

    assert!(
        output.status.success(),
        "generated crate failed to compile:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
