use acad_re::{
    ir::{lower, verify_source, CompilerFrame, InlineData, RecoveredDependency, RecoveredExport},
    rust_emit,
};
use std::{
    collections::BTreeMap,
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture() -> RecoveredExport {
    serde_json::from_str(include_str!("fixtures/dim-shift-cfg.json")).unwrap()
}

#[test]
fn rejects_unsafe_or_incomplete_inputs() {
    let mutations: Vec<fn(&mut RecoveredExport)> = vec![
        |e| e.architecture.language = "unknown".into(),
        |e| e.architecture.userops[0] = "unknown".into(),
        |e| e.architecture.ram_space_id += 1,
        |e| {
            e.functions[0]
                .errors
                .push(serde_json::json!("decode error"))
        },
        |e| {
            e.functions[0].dependencies.push(RecoveredDependency {
                block: "A".into(),
                offset: 0,
                kind: ("test".into(), None),
                status: "test".into(),
                signature_bytes: "".into(),
            })
        },
        |e| {
            e.functions[0].inline_data.push(InlineData {
                offset: 0,
                bytes: "".into(),
                kind: "test".into(),
                call: 0,
                status: "test".into(),
            })
        },
        |e| {
            e.functions[0].instructions[0].compiler_frame = Some(CompilerFrame {
                local_bytes: 0,
                continuation: 0,
                status: "test".into(),
            })
        },
        |e| e.functions[0].instructions[0].ops[0].op = "FLOAT_ADD".into(),
        |e| {
            e.functions[0].instructions[0].ops[0]
                .out
                .as_mut()
                .unwrap()
                .size = 3
        },
        |e| e.functions[0].instructions[0].ops[0].inputs[0] = None,
        |e| {
            e.functions[0].instructions[0].ops[0].inputs[0]
                .as_mut()
                .unwrap()
                .space = "unique".into()
        },
        |e| {
            e.functions[0].instructions[0].ops[1].inputs[0]
                .as_mut()
                .unwrap()
                .space = "const".into()
        },
        |e| {
            e.functions[0].instructions[0].ops[1].inputs[0]
                .as_mut()
                .unwrap()
                .offset += 1
        },
        |e| {
            e.functions[0].instructions.remove(1);
        },
        |e| {
            let row = e.functions[0].instructions[0].clone();
            e.functions[0].instructions.push(row);
        },
        |e| e.functions[0].instructions[0].address = "OVL09_CODE::021778".into(),
        |e| e.functions[0].instructions[0].address = "OVL04_CODE::ffffffffffffffff".into(),
        |e| e.functions[0].instructions[1].bytes = "83f92000".into(),
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut e = fixture();
        mutate(&mut e);
        let res = lower(&e.architecture, &e.functions[0]);
        assert!(res.is_err(), "mutation {index} accepted: {res:?}");
    }
}

#[test]
fn verifies_original_byte_ranges() {
    let export = fixture();
    let function = &export.functions[0];
    // A tiny sparse original-byte image is sufficient for the range validator.
    let mut image = vec![0; 82313];
    image[82296..].copy_from_slice(&[
        0xe3, 0x0e, 0x83, 0xf9, 0x20, 0x76, 0x03, 0xb9, 0x20, 0x00, 0xd1, 0xe0, 0xd1, 0xd2, 0xe2,
        0xfa, 0xc3,
    ]);
    let mut images = BTreeMap::from([("ACAD.OVL".into(), image)]);
    verify_source(function, &images).unwrap();
    images.get_mut("ACAD.OVL").unwrap()[82306] ^= 1;
    assert!(verify_source(function, &images).is_err());
    images.get_mut("ACAD.OVL").unwrap().truncate(82306);
    assert!(verify_source(function, &images).is_err());
    assert!(verify_source(function, &BTreeMap::new()).is_err());
}

#[test]
fn generated_dim_shift_compiles_and_matches_integer_reference() {
    let export = fixture();
    assert_eq!(export.functions.len(), 2);
    for function in &export.functions {
        let ir = lower(&export.architecture, function).unwrap();
        assert_eq!(ir.instructions.len(), 8);
        compile_and_check(&ir, include_str!("fixtures/dim-shift-reference.rs"));
    }
}

#[test]
fn generated_overlay_and_resident_sign_flip_match_memory_reference() {
    let export: RecoveredExport =
        serde_json::from_str(include_str!("fixtures/sign-flip-cfg.json")).unwrap();
    assert_eq!(export.functions.len(), 2);
    for function in &export.functions {
        let ir = lower(&export.architecture, function).unwrap();
        assert_eq!(ir.instructions.len(), 5);
        compile_and_check(&ir, include_str!("fixtures/sign-flip-reference.rs"));
    }
}

fn compile_and_check(ir: &acad_re::ir::Function, reference: &str) {
    let root = std::env::temp_dir().join(format!(
        "acad-ir-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let generated = root.join("generated.rs");
    fs::write(
        &generated,
        format!("{}\n{}", rust_emit::emit(ir), reference),
    )
    .unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2021", "-O", "--test"])
        .arg(&generated)
        .arg("-o")
        .arg(root.join("check"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "generated Rust did not compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(root.join("check")).output().unwrap();
    assert!(
        output.status.success(),
        "generated Rust failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
