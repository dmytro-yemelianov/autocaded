use acad_re::{
    ast::Varnode,
    ir::{
        lower, verify_source, Architecture, CompilerFrame, InlineData, Operation, RawOp,
        RecoveredDependency, RecoveredEdge, RecoveredExport, RecoveredFunction,
        RecoveredInstruction,
    },
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
        |e| {
            e.functions[0].instructions[0].ops[0].op = "FLOAT_NAN".into();
            e.functions[0].instructions[0].ops[0].inputs.truncate(1);
        },
        |e| {
            e.functions[0].instructions[0].ops[0].op = "FLOAT2FLOAT".into();
            e.functions[0].instructions[0].ops[0].inputs.truncate(1);
        },
        |e| {
            e.functions[0].instructions[0].ops[0].op = "INT2FLOAT".into();
            e.functions[0].instructions[0].ops[0].inputs.truncate(1);
        },
        |e| {
            e.functions[0].instructions[0].ops[0].op = "ROUND".into();
            e.functions[0].instructions[0].ops[0].inputs.truncate(1);
        },
        |e| {
            e.functions[0].instructions[0].ops[0].op = "TRUNC".into();
            e.functions[0].instructions[0].ops[0].inputs.truncate(1);
        },
        |e| {
            e.functions[0].instructions[0].ops[0].op = "FLOAT_EQUAL".into();
        },
        |e| {
            if let Some(out) = e.functions[0].instructions[0].ops[0].out.as_mut() {
                out.space = "register".into();
                out.offset = 8192;
                out.size = 2;
            }
        },
        |e| {
            e.functions[0].instructions[0].ops.clear();
        },
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
    compile_source_and_check(&rust_emit::emit(ir), reference);
}

fn compile_source_and_check(source: &str, reference: &str) {
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
    fs::write(&generated, format!("{source}\n{reference}")).unwrap();
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

#[test]
fn x87_80bit_float_lowering_and_execution() {
    let arch = Architecture {
        language: "x86:LE:16:Real Mode".into(),
        ram_space_id: 497,
        userops: vec!["segment".into()],
    };
    let v_reg = |offset: i64, size: u32| Varnode {
        space: "register".into(),
        offset,
        size,
        unique: false,
    };
    let v_uniq = |offset: i64, size: u32| Varnode {
        space: "unique".into(),
        offset,
        size,
        unique: true,
    };
    let v_const = |offset: i64, size: u32| Varnode {
        space: "const".into(),
        offset,
        size,
        unique: false,
    };
    let raw_fn = RecoveredFunction {
        block: "EXE_CODE".into(),
        entry: 0,
        instructions: vec![RecoveredInstruction {
            offset: 0,
            address: "0100:0000".into(),
            bytes: "90".into(),
            file: "ACAD.EXE".into(),
            file_offset: 0,
            instruction: "NOP".into(),
            ops: vec![
                // 1. INT2FLOAT: in: reg 0 (i16=42), out: unique 100 (f80)
                RawOp {
                    op: "INT2FLOAT".into(),
                    out: Some(v_uniq(100, 10)),
                    inputs: vec![Some(v_reg(0, 2))],
                },
                // 2. FLOAT_ADD: in: [unique 100, unique 100], out: unique 200 (f80=84)
                RawOp {
                    op: "FLOAT_ADD".into(),
                    out: Some(v_uniq(200, 10)),
                    inputs: vec![Some(v_uniq(100, 10)), Some(v_uniq(100, 10))],
                },
                // 3. FLOAT_SUB: in: [unique 200, unique 100], out: unique 300 (f80=42)
                RawOp {
                    op: "FLOAT_SUB".into(),
                    out: Some(v_uniq(300, 10)),
                    inputs: vec![Some(v_uniq(200, 10)), Some(v_uniq(100, 10))],
                },
                // 4. FLOAT_MULT: in: [unique 300, unique 100], out: unique 400 (f80=1764)
                RawOp {
                    op: "FLOAT_MULT".into(),
                    out: Some(v_uniq(400, 10)),
                    inputs: vec![Some(v_uniq(300, 10)), Some(v_uniq(100, 10))],
                },
                // 5. FLOAT_DIV: in: [unique 400, unique 100], out: unique 500 (f80=42)
                RawOp {
                    op: "FLOAT_DIV".into(),
                    out: Some(v_uniq(500, 10)),
                    inputs: vec![Some(v_uniq(400, 10)), Some(v_uniq(100, 10))],
                },
                // 6. FLOAT_EQUAL: in: [unique 500, unique 100], out: unique 600 (bool=1)
                RawOp {
                    op: "FLOAT_EQUAL".into(),
                    out: Some(v_uniq(600, 1)),
                    inputs: vec![Some(v_uniq(500, 10)), Some(v_uniq(100, 10))],
                },
                // 7. FLOAT_LESS: in: [unique 100, unique 200], out: unique 700 (bool=1)
                RawOp {
                    op: "FLOAT_LESS".into(),
                    out: Some(v_uniq(700, 1)),
                    inputs: vec![Some(v_uniq(100, 10)), Some(v_uniq(200, 10))],
                },
                // 8. FLOAT_NAN: in: [unique 100], out: unique 800 (bool=0)
                RawOp {
                    op: "FLOAT_NAN".into(),
                    out: Some(v_uniq(800, 1)),
                    inputs: vec![Some(v_uniq(100, 10))],
                },
                // 9. FLOAT2FLOAT: in: unique 100 (f80), out: unique 900 (f64)
                RawOp {
                    op: "FLOAT2FLOAT".into(),
                    out: Some(v_uniq(900, 8)),
                    inputs: vec![Some(v_uniq(100, 10))],
                },
                // 10. ROUND: in: unique 100 (f80=42), out: unique 1000 (i16)
                RawOp {
                    op: "ROUND".into(),
                    out: Some(v_uniq(1000, 2)),
                    inputs: vec![Some(v_uniq(100, 10))],
                },
                // 10b. ROUND to float (FRNDINT): in: unique 100 (f80=42), out: unique 1050 (f80)
                RawOp {
                    op: "ROUND".into(),
                    out: Some(v_uniq(1050, 10)),
                    inputs: vec![Some(v_uniq(100, 10))],
                },
                // 11. TRUNC: in: unique 100 (f80=42), out: unique 1100 (i16)
                RawOp {
                    op: "TRUNC".into(),
                    out: Some(v_uniq(1100, 2)),
                    inputs: vec![Some(v_uniq(100, 10))],
                },
                // Store results into high registers (>= 1024 to also test expanded bounds):
                // reg 2048 (size 10) = unique 500 (f80)
                RawOp {
                    op: "COPY".into(),
                    out: Some(v_reg(2048, 10)),
                    inputs: vec![Some(v_uniq(500, 10))],
                },
                // reg 2060 (size 1) = unique 600 (eq)
                RawOp {
                    op: "COPY".into(),
                    out: Some(v_reg(2060, 1)),
                    inputs: vec![Some(v_uniq(600, 1))],
                },
                // reg 2061 (size 1) = unique 700 (lt)
                RawOp {
                    op: "COPY".into(),
                    out: Some(v_reg(2061, 1)),
                    inputs: vec![Some(v_uniq(700, 1))],
                },
                // reg 2062 (size 1) = unique 800 (nan)
                RawOp {
                    op: "COPY".into(),
                    out: Some(v_reg(2062, 1)),
                    inputs: vec![Some(v_uniq(800, 1))],
                },
                // reg 2064 (size 2) = unique 1000 (round)
                RawOp {
                    op: "COPY".into(),
                    out: Some(v_reg(2064, 2)),
                    inputs: vec![Some(v_uniq(1000, 2))],
                },
                // reg 2066 (size 2) = unique 1100 (trunc)
                RawOp {
                    op: "COPY".into(),
                    out: Some(v_reg(2066, 2)),
                    inputs: vec![Some(v_uniq(1100, 2))],
                },
                // reg 2070 (size 10) = unique 1050 (frndint f80)
                RawOp {
                    op: "COPY".into(),
                    out: Some(v_reg(2070, 10)),
                    inputs: vec![Some(v_uniq(1050, 10))],
                },
                // RETURN const 0x1234
                RawOp {
                    op: "RETURN".into(),
                    out: None,
                    inputs: vec![Some(v_const(0x1234, 2))],
                },
            ],
            compiler_frame: None,
            kernel_tailcall: None,
        }],
        inline_data: vec![],
        dependencies: vec![],
        edges: vec![],
        errors: vec![],
    };

    let ir = lower(&arch, &raw_fn).expect("lowering failed");
    let test_ref = r#"
#[test]
fn run_float_test() {
    let mut registers = [0u8; 8192];
    registers[0..2].copy_from_slice(&42u16.to_le_bytes());
    let mut memory = [0u8; 16];
    let ret = run(&mut registers, &mut memory, 10).unwrap();
    assert_eq!(ret, 0x1234);
    assert_eq!(registers[2060], 1, "FLOAT_EQUAL should be true");
    assert_eq!(registers[2061], 1, "FLOAT_LESS should be true");
    assert_eq!(registers[2062], 0, "FLOAT_NAN should be false");
    let round_val = u16::from_le_bytes([registers[2064], registers[2065]]);
    assert_eq!(round_val, 42, "ROUND should be 42");
    let trunc_val = u16::from_le_bytes([registers[2066], registers[2067]]);
    assert_eq!(trunc_val, 42, "TRUNC should be 42");
    let mant = u64::from_le_bytes(registers[2048..2056].try_into().unwrap());
    let exp = u16::from_le_bytes(registers[2056..2058].try_into().unwrap());
    assert_eq!(mant, 0xa800_0000_0000_0000);
    assert_eq!(exp, 0x4004);
    let frnd_mant = u64::from_le_bytes(registers[2070..2078].try_into().unwrap());
    let frnd_exp = u16::from_le_bytes(registers[2078..2080].try_into().unwrap());
    assert_eq!(frnd_mant, 0xa800_0000_0000_0000);
    assert_eq!(frnd_exp, 0x4004);
}
"#;
    compile_and_check(&ir, test_ref);
}

#[test]
fn wait_instruction_lowers_empty_ops_with_fallthrough() {
    let arch = Architecture {
        language: "x86:LE:16:Real Mode".into(),
        ram_space_id: 497,
        userops: vec!["segment".into()],
    };
    let v_const = |offset: i64, size: u32| Varnode {
        space: "const".into(),
        offset,
        size,
        unique: false,
    };
    let raw_fn = RecoveredFunction {
        block: "EXE_CODE".into(),
        entry: 0,
        instructions: vec![
            RecoveredInstruction {
                offset: 0,
                address: "0100:0000".into(),
                bytes: "9b".into(),
                file: "ACAD.EXE".into(),
                file_offset: 0,
                instruction: "WAIT".into(),
                ops: vec![],
                compiler_frame: None,
                kernel_tailcall: None,
            },
            RecoveredInstruction {
                offset: 1,
                address: "0100:0001".into(),
                bytes: "c3".into(),
                file: "ACAD.EXE".into(),
                file_offset: 1,
                instruction: "RET".into(),
                ops: vec![RawOp {
                    op: "RETURN".into(),
                    out: None,
                    inputs: vec![Some(v_const(0, 2))],
                }],
                compiler_frame: None,
                kernel_tailcall: None,
            },
        ],
        inline_data: vec![],
        dependencies: vec![],
        edges: vec![],
        errors: vec![],
    };

    let ir = lower(&arch, &raw_fn).expect("lowering failed");
    assert_eq!(ir.instructions.len(), 2);
    assert_eq!(ir.instructions[0].assembly, "WAIT");
    assert!(ir.instructions[0].operations.is_empty());
    assert_eq!(ir.instructions[0].fallthrough, Some(0x1000 + 1));
}

#[test]
fn non_wait_instruction_with_empty_ops_is_rejected() {
    let arch = Architecture {
        language: "x86:LE:16:Real Mode".into(),
        ram_space_id: 497,
        userops: vec!["segment".into()],
    };
    let raw_fn = RecoveredFunction {
        block: "EXE_CODE".into(),
        entry: 0,
        instructions: vec![RecoveredInstruction {
            offset: 0,
            address: "0100:0000".into(),
            bytes: "90".into(),
            file: "ACAD.EXE".into(),
            file_offset: 0,
            instruction: "NOP".into(),
            ops: vec![],
            compiler_frame: None,
            kernel_tailcall: None,
        }],
        inline_data: vec![],
        dependencies: vec![],
        edges: vec![],
        errors: vec![],
    };

    let err = lower(&arch, &raw_fn).unwrap_err();
    assert_eq!(err, "instruction has no Pcode");
}

#[test]
fn inter_function_branch_lowers_to_tail_call() {
    let arch = Architecture {
        language: "x86:LE:16:Real Mode".into(),
        ram_space_id: 497,
        userops: vec!["segment".into()],
    };
    let v_ram = |offset: i64, size: u32| Varnode {
        space: "ram".into(),
        offset,
        size,
        unique: false,
    };
    let raw_fn = RecoveredFunction {
        block: "EXE_CODE".into(),
        entry: 0,
        instructions: vec![RecoveredInstruction {
            offset: 0,
            address: "0100:0000".into(),
            bytes: "e90010".into(),
            file: "ACAD.EXE".into(),
            file_offset: 0,
            instruction: "JMP 0x2003".into(),
            ops: vec![RawOp {
                op: "BRANCH".into(),
                out: None,
                inputs: vec![Some(v_ram(0x2003, 4))],
            }],
            compiler_frame: None,
            kernel_tailcall: None,
        }],
        inline_data: vec![],
        dependencies: vec![],
        edges: vec![],
        errors: vec![],
    };

    let ir = lower(&arch, &raw_fn).expect("lowering failed");
    assert_eq!(ir.instructions.len(), 1);
    assert!(matches!(
        &ir.instructions[0].operations[0],
        Operation::TailCall {
            target_block,
            target: 0x2003,
        } if target_block == "EXE_CODE"
    ));
    assert_eq!(ir.instructions[0].fallthrough, None);
}

#[test]
fn aztec_compiler_frame_helper_with_signature_lowers_limit() {
    let arch = Architecture {
        language: "x86:LE:16:Real Mode".into(),
        ram_space_id: 497,
        userops: vec!["segment".into()],
    };
    let raw_fn = RecoveredFunction {
        block: "OVL07_CODE".into(),
        entry: 0,
        instructions: vec![
            RecoveredInstruction {
                offset: 0,
                address: "OVL07_CODE::001000".into(),
                bytes: "e80500".into(),
                file: "ACAD.OVL".into(),
                file_offset: 0,
                instruction: "CALL 0x0008".into(),
                ops: vec![],
                compiler_frame: Some(CompilerFrame {
                    local_bytes: 4,
                    continuation: 3,
                    status: "ok".into(),
                }),
                kernel_tailcall: None,
            },
            RecoveredInstruction {
                offset: 3,
                address: "OVL07_CODE::001003".into(),
                bytes: "c3".into(),
                file: "ACAD.OVL".into(),
                file_offset: 3,
                instruction: "RET".into(),
                ops: vec![RawOp {
                    op: "RETURN".into(),
                    out: None,
                    inputs: vec![Some(Varnode {
                        space: "const".into(),
                        offset: 0,
                        size: 2,
                        unique: false,
                    })],
                }],
                compiler_frame: None,
                kernel_tailcall: None,
            },
        ],
        inline_data: vec![],
        dependencies: vec![RecoveredDependency {
            block: "OVL07_CODE".into(),
            offset: 8,
            kind: ("frame".into(), None),
            status: "ok".into(),
            signature_bytes: "5efc2eadebed".into(),
        }],
        edges: vec![],
        errors: vec![],
    };

    let ir = lower(&arch, &raw_fn).expect("lowering failed");
    assert_eq!(ir.instructions.len(), 2);
    assert!(ir.instructions[0].operations.iter().any(|op| {
        if let Operation::Assign { expression, .. } = op {
            format!("{expression:?}").contains("0x505d")
                || format!("{expression:?}").contains("20573")
        } else {
            false
        }
    }));
}

#[test]
fn aztec_switch_consumes_value_from_stack_segment() {
    let arch = Architecture {
        language: "x86:LE:16:Real Mode".into(),
        ram_space_id: 497,
        userops: vec!["segment".into()],
    };
    let instruction = |offset, result| RecoveredInstruction {
        offset,
        address: format!("1000:{offset:04x}"),
        bytes: if offset == 0 { "e872b7" } else { "c3" }.into(),
        file: "ACAD.EXE".into(),
        file_offset: offset as usize,
        instruction: if offset == 0 { "CALL 0xb775" } else { "RET" }.into(),
        ops: if offset == 0 {
            vec![]
        } else {
            vec![RawOp {
                op: "RETURN".into(),
                out: None,
                inputs: vec![Some(Varnode {
                    space: "const".into(),
                    offset: result,
                    size: 2,
                    unique: false,
                })],
            }]
        },
        compiler_frame: None,
        kernel_tailcall: None,
    };
    let raw = RecoveredFunction {
        block: "EXE_CODE".into(),
        entry: 0,
        instructions: vec![
            instruction(0, 0),
            instruction(3, 1),
            instruction(4, 2),
            instruction(5, 3),
        ],
        inline_data: vec![InlineData {
            offset: 6,
            bytes: "".into(),
            kind: "switch-table".into(),
            call: 0,
            status: "ok".into(),
        }],
        dependencies: vec![],
        edges: vec![
            RecoveredEdge {
                from: 0,
                kind: "switch".into(),
                case: Some(114),
                target: Some(3),
            },
            RecoveredEdge {
                from: 0,
                kind: "switch".into(),
                case: Some(-1),
                target: Some(4),
            },
            RecoveredEdge {
                from: 0,
                kind: "switch".into(),
                case: None,
                target: Some(5),
            },
        ],
        errors: vec![],
    };
    let ir = lower(&arch, &raw).unwrap();
    let reference = include_str!("fixtures/switch-stack-reference.rs");
    compile_and_check(&ir, reference);

    // Exercise the linked emitter used by the boot experiment as well.
    let symbols = acad_re::whole_program::SymbolTable::new(&[raw]);
    let source = format!(
        "mod runtime {{ {} }}\nmod exe_code {{ use crate::runtime::*; {} }}\n\
         use runtime::*;\n\
         fn dispatch_address(_: &str, _: u64, _: &mut [u8; 8192], _: &mut [u8], _: &mut usize) -> Result<u64, Trap> {{ Err(Trap::InvalidPc) }}\n\
         fn run(registers: &mut [u8; 8192], memory: &mut [u8], mut budget: usize) -> Result<u64, Trap> {{\n\
         exe_code::fn_exe_0000(registers, memory, &mut budget)\n}}",
        acad_re::whole_program::RUNTIME_RS,
        acad_re::whole_program::emit_function(&ir, &symbols),
    );
    compile_source_and_check(&source, reference);
}

#[test]
fn hosted_swi_is_not_dispatched_as_an_indirect_guest_call() {
    let mut export = fixture();
    let raw = &mut export.functions[0];
    raw.instructions[0].ops = serde_json::from_value(serde_json::json!([
        {"op":"CALLOTHER", "out":{"space":"unique","offset":100,"size":4,"unique":true},
         "in":[{"space":"const","offset":16,"size":4,"unique":false},
               {"space":"const","offset":33,"size":1,"unique":false}]},
        {"op":"CALLIND", "out":null,
         "in":[{"space":"unique","offset":100,"size":4,"unique":true}]}
    ]))
    .unwrap();
    let ir = lower(&export.architecture, raw).unwrap();
    assert!(ir.instructions[0].operations.iter().any(|op| matches!(
        op,
        Operation::Assign {
            expression: acad_re::ir::Expression::Swi(_),
            ..
        }
    )));
    assert!(!ir.instructions[0]
        .operations
        .iter()
        .any(|op| matches!(op, Operation::CallIndirect { .. })));
    // A different target is a real indirect call and must still be checked.
    raw.instructions[0].ops[1].inputs[0] = Some(Varnode {
        space: "const".into(),
        offset: 0,
        size: 4,
        unique: false,
    });
    let ir = lower(&export.architecture, raw).unwrap();
    assert!(ir.instructions[0]
        .operations
        .iter()
        .any(|op| matches!(op, Operation::CallIndirect { .. })));
}

#[test]
fn far_memory_jump_loads_the_segment_and_preserves_the_stack() {
    let raw: RecoveredFunction = serde_json::from_value(serde_json::json!({
        "block":"EXE_CODE", "entry":256,
        "instructions":[{"offset":256,"address":"1000:0100","bytes":"ff2e0040",
            "file":"ACAD.EXE","file_offset":0,"instruction":"JMPF [0x4000]","ops":[]}],
        "inline_data":[],"dependencies":[],"edges":[],"errors":[]
    }))
    .unwrap();
    let ir = lower(&fixture().architecture, &raw).unwrap();
    let symbols = acad_re::whole_program::SymbolTable::new(&[raw]);
    let source = format!(
        "mod runtime {{ {} }}\nmod exe_code {{ use crate::runtime::*; {} }}\n\
         use runtime::*;\n\
         fn dispatch_address(_: &str, target: u64, _: &mut [u8; 8192], _: &mut [u8], budget: &mut usize) -> Result<u64, Trap> {{ assert_eq!(*budget, 0); Ok(target) }}\n",
        acad_re::whole_program::RUNTIME_RS,
        acad_re::whole_program::emit_function(&ir, &symbols),
    );
    compile_source_and_check(
        &source,
        r#"
        #[test]
        fn reads_the_far_pointer_in_ds_without_touching_ss_or_sp() {
            let mut registers = [0xa5; 8192];
            write(&mut registers, 262, 2, 0x120);
            let mut expected = registers;
            write(&mut expected, 258, 2, 0x2abb);
            let mut memory = vec![0; 0x10000];
            store(&mut memory, 0x5200, 2, 0x100).unwrap();
            store(&mut memory, 0x5202, 2, 0x2abb).unwrap();
            let expected_memory = memory.clone();
            let mut budget = 1;
            assert_eq!(exe_code::fn_exe_0100(&mut registers, &mut memory, &mut budget), Ok(0x2acb0));
            assert_eq!(registers, expected);
            assert_eq!(memory, expected_memory);
        }
    "#,
    );
}

#[test]
fn near_indirect_call_preserves_relocated_cs_and_pushes_native_ip() {
    let export: RecoveredExport =
        serde_json::from_str(include_str!("fixtures/overlay-near-call-cfg.json")).unwrap();
    let ir = lower(&export.architecture, &export.functions[0]).unwrap();
    let symbols = acad_re::whole_program::SymbolTable::new(&export.functions);
    let source = format!(
        "mod runtime {{ {} }}\nmod ovl00_code {{ use crate::runtime::*; {} }}\n\
         use runtime::*;\n\
         fn dispatch_address(block: &str, target: u64, registers: &mut [u8; 8192], memory: &mut [u8], _: &mut usize) -> Result<u64, Trap> {{\n\
           assert_eq!(block, \"OVL00_CODE\"); assert_eq!(target, 0x2b15e);\n\
           assert_eq!(read(registers,258,2), 0x2abb); assert_eq!(read(registers,16,2), 0x1e);\n\
           assert_eq!(load(memory,0x11e,2), Ok(0x11e));\n\
           write(registers,16,2,0x20); write(registers,0,2,42); Ok(0)\n}}\n",
        acad_re::whole_program::RUNTIME_RS,
        acad_re::whole_program::emit_function(&ir, &symbols),
    );
    compile_source_and_check(
        &source,
        r#"
        #[test]
        fn calls_loaded_overlay() {
            let mut registers = [0;8192]; let mut memory = vec![0;0x200]; let mut budget = 2;
            write(&mut registers,258,2,0x2abb); write(&mut registers,260,2,0x10);
            write(&mut registers,16,2,0x20); write(&mut registers,24,2,0x5ae);
            assert_eq!(ovl00_code::fn_ovl00_011c(&mut registers,&mut memory,&mut budget),Ok(42));
            assert_eq!(read(&registers,258,2),0x2abb); assert_eq!(read(&registers,16,2),0x20);
        }
    "#,
    );
}

#[test]
fn cs_override_reads_from_the_loaded_segment() {
    let export: RecoveredExport =
        serde_json::from_str(include_str!("fixtures/overlay-cs-read-cfg.json")).unwrap();
    let ir = lower(&export.architecture, &export.functions[0]).unwrap();
    compile_and_check(
        &ir,
        r#"
        #[test]
        fn reads_relocated_cs() {
            let mut registers=[0;8192]; let mut memory=vec![0;0x30000];
            write(&mut registers,258,2,0x2abb); write(&mut registers,24,2,0x100);
            store(&mut memory,0x2acb0,2,0xcafe).unwrap(); store(&mut memory,0x20100,2,0xbeef).unwrap();
            assert_eq!(run(&mut registers,&mut memory,2),Ok(0xcafe));
            assert_eq!(read(&registers,258,2),0x2abb);
        }
    "#,
    );
}

#[test]
fn far_memory_call_pushes_cs_ip_and_enters_the_target_segment() {
    let raw: RecoveredFunction=serde_json::from_value(serde_json::json!({
        "block":"EXE_CODE","entry":256,"instructions":[
          {"offset":256,"address":"1000:0100","bytes":"ff1e0040","file":"ACAD.EXE","file_offset":0,"instruction":"CALLF [0x4000]","ops":[]},
          {"offset":260,"address":"1000:0104","bytes":"c3","file":"ACAD.EXE","file_offset":4,"instruction":"RET",
           "ops":[{"op":"RETURN","out":null,"in":[{"space":"const","offset":99,"size":2,"unique":false}]}]}],
        "inline_data":[],"dependencies":[],"edges":[],"errors":[]
    })).unwrap();
    let ir = lower(&fixture().architecture, &raw).unwrap();
    let symbols = acad_re::whole_program::SymbolTable::new(&[raw]);
    let source=format!("mod runtime {{ {} }}\nmod exe_code {{ use crate::runtime::*; {} }}\nuse runtime::*;\n\
       fn dispatch_address(_: &str, target:u64, registers:&mut [u8;8192], memory:&mut [u8], budget:&mut usize)->Result<u64,Trap> {{\n\
        assert_eq!(target,0x2acb0); assert_eq!(read(registers,258,2),0x2abb);\n\
        assert_eq!(read(registers,16,2),0x1c); assert_eq!(load(memory,0x11c,2),Ok(0x104)); assert_eq!(load(memory,0x11e,2),Ok(0x1000));\n\
        assert_eq!(*budget,1); assert_eq!(dispatch_indirect_jump(registers,memory,budget,\"OVL00_CODE\",0x10104),Ok(0x10104));\n\
        write(registers,16,2,0x20); write(registers,258,2,0x1000); Ok(0x10104)\n}}",
        acad_re::whole_program::RUNTIME_RS,acad_re::whole_program::emit_function(&ir,&symbols));
    compile_source_and_check(
        &source,
        r#"
        #[test]
        fn calls_far_pointer() {
            let mut registers=[0;8192]; let mut memory=vec![0;0x10000]; let mut budget=2;
            write(&mut registers,258,2,0x1000); write(&mut registers,262,2,0x120);
            write(&mut registers,260,2,0x10); write(&mut registers,16,2,0x20);
            store(&mut memory,0x5200,2,0x100).unwrap(); store(&mut memory,0x5202,2,0x2abb).unwrap();
            assert_eq!(exe_code::fn_exe_0100(&mut registers,&mut memory,&mut budget),Ok(99));
            assert_eq!(budget,0); assert_eq!(read(&registers,258,2),0x1000); assert_eq!(read(&registers,16,2),0x20);
        }
    "#,
    );
}
