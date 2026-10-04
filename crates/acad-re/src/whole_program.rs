//! Whole-program symbol table, call linker, and multi-module code generation pipeline.
use crate::ir::{
    Binary, Expression, Function, Operation, Place, RecoveredExport, RecoveredFunction, Unary,
    Value,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::Path,
};

/// Maps an architectural code block identifier to its Rust module name.
pub fn block_to_mod(block: &str) -> &'static str {
    match block {
        "EXE_CODE" => "exe_code",
        "OVL00_CODE" => "ovl00_code",
        "OVL01_CODE" => "ovl01_code",
        "OVL02_CODE" => "ovl02_code",
        "OVL03_CODE" => "ovl03_code",
        "OVL04_CODE" => "ovl04_code",
        "OVL05_CODE" => "ovl05_code",
        "OVL06_CODE" => "ovl06_code",
        "OVL07_CODE" => "ovl07_code",
        "OVL08_CODE" => "ovl08_code",
        "OVL09_CODE" => "ovl09_code",
        "OVL10_CODE" => "ovl10_code",
        "OVL11_CODE" => "ovl11_code",
        _ => "unknown",
    }
}

/// Generates a standardized function symbol name from its block and entry offset.
pub fn function_symbol(block: &str, entry: u16) -> String {
    let prefix = match block {
        "EXE_CODE" => "exe",
        "OVL00_CODE" => "ovl00",
        "OVL01_CODE" => "ovl01",
        "OVL02_CODE" => "ovl02",
        "OVL03_CODE" => "ovl03",
        "OVL04_CODE" => "ovl04",
        "OVL05_CODE" => "ovl05",
        "OVL06_CODE" => "ovl06",
        "OVL07_CODE" => "ovl07",
        "OVL08_CODE" => "ovl08",
        "OVL09_CODE" => "ovl09",
        "OVL10_CODE" => "ovl10",
        "OVL11_CODE" => "ovl11",
        _ => "unk",
    };
    format!("fn_{prefix}_{entry:04x}")
}

/// Global symbol table indexing all known functions by (block, entry).
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    symbols: BTreeSet<(String, u16)>,
}

impl SymbolTable {
    pub fn new(functions: &[RecoveredFunction]) -> Self {
        let mut symbols = BTreeSet::new();
        for f in functions {
            symbols.insert((f.block.clone(), f.entry));
        }
        Self { symbols }
    }

    pub fn contains(&self, block: &str, entry: u16) -> bool {
        self.symbols.contains(&(block.to_string(), entry))
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn resolve_call(&self, caller_block: &str, target_offset: u64) -> Option<(String, u16)> {
        let entry_cand = (target_offset & 0xffff) as u16;
        if self.contains(caller_block, entry_cand) {
            Some((caller_block.to_string(), entry_cand))
        } else if self.contains("EXE_CODE", entry_cand) {
            Some(("EXE_CODE".to_string(), entry_cand))
        } else if self.contains("OVL02_CODE", entry_cand) {
            Some(("OVL02_CODE".to_string(), entry_cand))
        } else {
            None
        }
    }

    pub fn resolve_tailcall(
        &self,
        caller_block: &str,
        target_block: &str,
        target_offset: u64,
    ) -> Option<(String, u16)> {
        let entry_cand = (target_offset & 0xffff) as u16;
        if self.contains(target_block, entry_cand) {
            Some((target_block.to_string(), entry_cand))
        } else if self.contains(caller_block, entry_cand) {
            Some((caller_block.to_string(), entry_cand))
        } else if self.contains("EXE_CODE", entry_cand) {
            Some(("EXE_CODE".to_string(), entry_cand))
        } else if self.contains("OVL02_CODE", entry_cand) {
            Some(("OVL02_CODE".to_string(), entry_cand))
        } else {
            None
        }
    }
}

fn storage(place: &Place) -> (&'static str, usize, usize) {
    match place {
        Place::Register { offset, width } => ("registers", *offset, width.bytes()),
        Place::Temporary { offset, width } => ("temporary", *offset, width.bytes()),
    }
}

fn value(value: &Value) -> String {
    match value {
        Value::Constant { value, width } => {
            if width.bytes() > 8 {
                format!("{value}u128")
            } else {
                format!("{value}u64")
            }
        }
        Value::Read(place) => {
            let (bank, offset, size) = storage(place);
            if size > 8 {
                format!("read128(&{bank}[..], {offset}, {size})")
            } else {
                format!("read(&{bank}[..], {offset}, {size})")
            }
        }
    }
}

fn to_f64(v: &Value) -> String {
    let size = v.width().bytes();
    match size {
        4 => format!("(f32::from_bits({} as u32) as f64)", value(v)),
        8 => format!("f64::from_bits({})", value(v)),
        10 => format!("f80_to_f64({})", value(v)),
        _ => format!("({} as f64)", value(v)),
    }
}

fn to_float_repr(expr: &str, size: usize) -> String {
    match size {
        4 => format!("(({expr}) as f32).to_bits() as u64"),
        8 => format!("({expr}).to_bits()"),
        10 => format!("f64_to_f80({expr})"),
        _ => expr.to_string(),
    }
}

fn expression(expr: &Expression, size: usize) -> String {
    match expr {
        Expression::Unary(op, input) => {
            let a = value(input);
            let bits = input.width().bytes() * 8;
            match op {
                Unary::Copy | Unary::ZeroExtend => a,
                Unary::SignExtend => format!("(({a} as i{bits}) as u64)"),
                Unary::BoolNegate => format!("u64::from({a} == 0)"),
                Unary::Popcount => format!("({a}).count_ones() as u64"),
                Unary::TwoComplement => format!("(0u64).wrapping_sub({a})"),
                Unary::BitwiseNegate => format!("!{a}"),
                Unary::FloatNan => format!("u64::from(({}).is_nan())", to_f64(input)),
                Unary::Float2Float => to_float_repr(&to_f64(input), size),
                Unary::Int2Float => to_float_repr(
                    &format!(
                        "(signed({}, {}) as f64)",
                        value(input),
                        input.width().bytes() * 8
                    ),
                    size,
                ),
                Unary::Round => {
                    if size > 8 {
                        to_float_repr(&format!("({}).round()", to_f64(input)), size)
                    } else {
                        format!("(({}).round() as i64 as u64)", to_f64(input))
                    }
                }
                Unary::Trunc => format!("(({} as i64) as u64)", to_f64(input)),
            }
        }
        Expression::Binary(op, left, right) => {
            let a = value(left);
            let b = value(right);
            let bits = left.width().bytes() * 8;
            match op {
                Binary::Add => format!("({a}).wrapping_add({b})"),
                Binary::Sub => format!("({a}).wrapping_sub({b})"),
                Binary::Multiply => format!("({a}).wrapping_mul({b})"),
                Binary::Divide => {
                    format!("({a}).checked_div({b}).ok_or(crate::runtime::Trap::DivisionByZero)?")
                }
                Binary::SignedDivide => format!(
                    "(({a} as i{bits}).checked_div({b} as i{bits}).ok_or(crate::runtime::Trap::DivisionByZero)? as u64)"
                ),
                Binary::And => format!("{a} & {b}"),
                Binary::Or => format!("{a} | {b}"),
                Binary::Xor => format!("{a} ^ {b}"),
                Binary::Left => format!("shift_left({a}, {b})"),
                Binary::Right => format!("shift_right({a}, {b})"),
                Binary::SignedRight => format!("shift_signed_right({a}, {b}, {bits})"),
                Binary::Remainder => {
                    format!("({a}).checked_rem({b}).ok_or(crate::runtime::Trap::DivisionByZero)?")
                }
                Binary::SignedRemainder => format!(
                    "(({a} as i{bits}).checked_rem({b} as i{bits}).ok_or(crate::runtime::Trap::DivisionByZero)? as u64)"
                ),
                Binary::Equal => format!("u64::from({a} == {b})"),
                Binary::NotEqual => format!("u64::from({a} != {b})"),
                Binary::Less => format!("u64::from({a} < {b})"),
                Binary::SignedLess => {
                    format!("u64::from(signed({a}, {bits}) < signed({b}, {bits}))")
                }
                Binary::SignedBorrow => format!(
                    "u64::from((({a} ^ {b}) & ({a} ^ ({a}).wrapping_sub({b}))) & (1u64 << {}) != 0)",
                    bits - 1
                ),
                Binary::Carry => {
                    format!("u64::from(({a} as u{bits}).overflowing_add({b} as u{bits}).1)")
                }
                Binary::SignedCarry => {
                    format!("u64::from(({a} as i{bits}).overflowing_add({b} as i{bits}).1)")
                }
                Binary::BoolOr => format!("u64::from({a} != 0 || {b} != 0)"),
                Binary::BoolAnd => format!("u64::from({a} != 0 && {b} != 0)"),
                Binary::BoolXor => format!("u64::from(({a} != 0) ^ ({b} != 0))"),
                Binary::Subpiece => format!("shift_right({a}, ({b}).saturating_mul(8))"),
                Binary::FloatAdd => {
                    to_float_repr(&format!("({} + {})", to_f64(left), to_f64(right)), size)
                }
                Binary::FloatSub => {
                    to_float_repr(&format!("({} - {})", to_f64(left), to_f64(right)), size)
                }
                Binary::FloatMult => {
                    to_float_repr(&format!("({} * {})", to_f64(left), to_f64(right)), size)
                }
                Binary::FloatDiv => {
                    to_float_repr(&format!("({} / {})", to_f64(left), to_f64(right)), size)
                }
                Binary::FloatEqual => format!("u64::from({} == {})", to_f64(left), to_f64(right)),
                Binary::FloatLess => format!("u64::from({} < {})", to_f64(left), to_f64(right)),
            }
        }
        Expression::Segment(segment, offset) => {
            format!("segment({}, {})", value(segment), value(offset))
        }
        Expression::Load(pointer) => {
            if size > 8 {
                format!("load128(memory, {}, {size})?", value(pointer))
            } else {
                format!("load(memory, {}, {size})?", value(pointer))
            }
        }
        Expression::In(port) => format!("crate::runtime::read_port({})", value(port)),
        Expression::Swi(int_num) => {
            format!(
                "crate::runtime::host_interrupt({}, registers, memory)?",
                value(int_num)
            )
        }
    }
}

/// Emits a single function lowered in whole-program linked mode.
pub fn emit_function(function: &Function, symbols: &SymbolTable) -> String {
    let entry_u16 = (function.entry & 0xffff) as u16;
    let fn_name = function_symbol(&function.block, entry_u16);
    let mut source = String::new();
    let local_addresses: BTreeSet<u64> = function.instructions.iter().map(|i| i.address).collect();

    writeln!(
        source,
        "// {} entry {:#x}",
        function.block.escape_default(),
        function.entry
    )
    .unwrap();
    writeln!(
        source,
        "pub fn {fn_name}(registers: &mut [u8; 8192], memory: &mut [u8], budget: &mut usize) -> Result<u64, crate::runtime::Trap> {{"
    )
    .unwrap();
    writeln!(
        source,
        "let mut temporary = [0u8; {}];",
        function.temporary_bytes
    )
    .unwrap();
    writeln!(source, "let mut pc = {}u64;", function.entry).unwrap();
    source.push_str("while *budget > 0 {\n*budget -= 1;\nmatch pc {\n");

    for row in &function.instructions {
        writeln!(
            source,
            "// {}:{} {} {}",
            row.file.escape_default(),
            row.file_offset,
            row.bytes.escape_default(),
            row.assembly.escape_default()
        )
        .unwrap();
        writeln!(source, "{} => {{", row.address).unwrap();
        for op in &row.operations {
            match op {
                Operation::Assign {
                    destination,
                    expression: expr,
                } => {
                    let (bank, offset, size) = storage(destination);
                    writeln!(source, "let value = {};", expression(expr, size)).unwrap();
                    let fn_name = if size > 8 { "write128" } else { "write" };
                    writeln!(
                        source,
                        "{fn_name}({bank}, {offset}, {size}, value);",
                        bank = if bank == "registers" {
                            "registers"
                        } else {
                            "&mut temporary"
                        }
                    )
                    .unwrap();
                }
                Operation::Branch { condition, target } => {
                    writeln!(
                        source,
                        "if {} != 0 {{ pc = {target}; continue; }}",
                        value(condition)
                    )
                    .unwrap();
                }
                Operation::Return { target } => {
                    writeln!(source, "return Ok({});", value(target)).unwrap();
                }
                Operation::Jump { target } => {
                    if local_addresses.contains(target) {
                        writeln!(source, "pc = {target}; continue;").unwrap();
                    } else if let Some((tgt_blk, tgt_entry)) =
                        symbols.resolve_call(&function.block, *target)
                    {
                        let mod_name = block_to_mod(&tgt_blk);
                        let fn_name = function_symbol(&tgt_blk, tgt_entry);
                        writeln!(
                            source,
                            "return crate::{mod_name}::{fn_name}(registers, memory, budget);"
                        )
                        .unwrap();
                    } else {
                        writeln!(
                            source,
                            "return crate::runtime::dispatch_tailcall(registers, memory, budget, {:?}, {target});",
                            function.block
                        )
                        .unwrap();
                    }
                }
                Operation::Call { target } => {
                    if let Some((tgt_blk, tgt_entry)) =
                        symbols.resolve_call(&function.block, *target)
                    {
                        let mod_name = block_to_mod(&tgt_blk);
                        let fn_name = function_symbol(&tgt_blk, tgt_entry);
                        writeln!(
                            source,
                            "crate::runtime::call_with_return(registers, memory, budget, |registers, memory, budget| crate::{mod_name}::{fn_name}(registers, memory, budget))?;"
                        )
                        .unwrap();
                    } else {
                        writeln!(
                            source,
                            "crate::runtime::call_with_return(registers, memory, budget, |registers, memory, budget| crate::runtime::dispatch_call(registers, memory, budget, {target}))?;"
                        )
                        .unwrap();
                    }
                }
                Operation::Store {
                    pointer,
                    value: input,
                } => {
                    let size = input.width().bytes();
                    writeln!(source, "let address = {};", value(pointer)).unwrap();
                    writeln!(source, "let value = {};", value(input)).unwrap();
                    let fn_name = if size > 8 { "store128" } else { "store" };
                    writeln!(source, "{fn_name}(memory, address, {size}, value)?;",).unwrap();
                }
                Operation::Trap { condition } => {
                    writeln!(
                        source,
                        "if {} != 0 {{ return Err(crate::runtime::Trap::StackOverflow); }}",
                        value(condition)
                    )
                    .unwrap();
                }
                Operation::TailCall {
                    target_block,
                    target,
                } => {
                    if let Some((tgt_blk, tgt_entry)) =
                        symbols.resolve_tailcall(&function.block, target_block, *target)
                    {
                        let mod_name = block_to_mod(&tgt_blk);
                        let fn_name = function_symbol(&tgt_blk, tgt_entry);
                        writeln!(
                            source,
                            "return crate::{mod_name}::{fn_name}(registers, memory, budget);"
                        )
                        .unwrap();
                    } else {
                        writeln!(
                            source,
                            "return crate::runtime::dispatch_tailcall(registers, memory, budget, {target_block:?}, {target});"
                        )
                        .unwrap();
                    }
                }
                Operation::CallIndirect { target } => {
                    let call_scope = if row.bytes.starts_with("ff1e") {
                        "call_far_with_return"
                    } else {
                        "call_with_return"
                    };
                    writeln!(
                        source,
                        "let target = {};\ncrate::runtime::{call_scope}(registers, memory, budget, |registers, memory, budget| crate::runtime::dispatch_indirect(registers, memory, budget, {:?}, target))?;",
                        value(target),
                        function.block,
                    )
                    .unwrap();
                }
                Operation::KernelTailCall { native_ip } => {
                    writeln!(source, "return crate::runtime::dispatch_kernel_tailcall(registers, memory, budget, {native_ip});").unwrap();
                }
                Operation::JumpIndirect { target } => {
                    writeln!(source, "let target = {};", value(target)).unwrap();
                    if function.block != "EXE_CODE" {
                        writeln!(source, "if let Some(file_offset) = crate::runtime::loaded_overlay_offset(target) {{\nmatch file_offset {{").unwrap();
                        for instruction in &function.instructions {
                            writeln!(
                                source,
                                "{} => {{ pc = {}; continue; }},",
                                instruction.file_offset, instruction.address
                            )
                            .unwrap();
                        }
                        writeln!(source, "_ => return crate::runtime::dispatch_indirect_jump(registers, memory, budget, {:?}, target),\n}}\n}}", function.block).unwrap();
                    }
                    writeln!(source, "match target {{").unwrap();
                    for instruction in &function.instructions {
                        writeln!(
                            source,
                            "{} => {{ pc = target; continue; }},",
                            instruction.address
                        )
                        .unwrap();
                    }
                    writeln!(source, "_ => return crate::runtime::dispatch_indirect_jump(registers, memory, budget, {:?}, target),\n}}", function.block).unwrap();
                }
                Operation::Out { port, value: val } => {
                    writeln!(
                        source,
                        "crate::runtime::write_port({}, {});",
                        value(port),
                        value(val)
                    )
                    .unwrap();
                }
                Operation::Lock => {
                    writeln!(source, "// lock").unwrap();
                }
                Operation::Unlock => {
                    writeln!(source, "// unlock").unwrap();
                }
            }
        }
        if let Some(next) = row.fallthrough {
            writeln!(source, "pc = {next};").unwrap();
        }
        source.push_str("}\n");
    }

    source.push_str("_ => return Err(crate::runtime::Trap::InvalidPc),\n}\n}\nErr(crate::runtime::Trap::Budget)\n}\n");
    source
}

/// Standalone runtime library file contents.
pub const RUNTIME_RS: &str = include_str!("translated_runtime.rs");

/// Emits the whole-program translated Rust crate containing all blocks and runtime.
pub fn emit_translated_crate(
    export: &RecoveredExport,
    images: &BTreeMap<String, Vec<u8>>,
    output_dir: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut lowered = Vec::new();
    for f in &export.functions {
        crate::ir::verify_source(f, images)?;
        let ir = crate::ir::lower(&export.architecture, f)?;
        lowered.push(ir);
    }

    let symbols = SymbolTable::new(&export.functions);

    let src_dir = output_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let cargo_toml = r#"[package]
name = "acad-translated"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
"#;
    fs::write(output_dir.join("Cargo.toml"), cargo_toml)?;
    fs::write(src_dir.join("runtime.rs"), RUNTIME_RS)?;

    let mut by_block: BTreeMap<String, Vec<&Function>> = BTreeMap::new();
    for f in &lowered {
        by_block.entry(f.block.clone()).or_default().push(f);
    }

    for (block, funcs) in &by_block {
        let mod_name = block_to_mod(block);
        let mut mod_code = String::new();
        writeln!(mod_code, "//! Auto-generated module for block `{block}`.").unwrap();
        writeln!(
            mod_code,
            "#[allow(unused_imports, unused_mut, unused_variables, unreachable_code)]"
        )
        .unwrap();
        writeln!(mod_code, "use crate::runtime::*;").unwrap();
        writeln!(mod_code).unwrap();
        for f in funcs {
            mod_code.push_str(&emit_function(f, &symbols));
            mod_code.push('\n');
        }
        fs::write(src_dir.join(format!("{mod_name}.rs")), mod_code)?;
    }

    let mut lib_code = String::new();
    writeln!(
        lib_code,
        "//! Whole-program translated AutoCAD 2.18 binary and overlays."
    )
    .unwrap();
    writeln!(lib_code, "pub mod runtime;").unwrap();
    writeln!(lib_code).unwrap();
    for block in by_block.keys() {
        let mod_name = block_to_mod(block);
        writeln!(lib_code, "pub mod {mod_name};").unwrap();
    }
    writeln!(lib_code).unwrap();
    writeln!(
        lib_code,
        "pub fn dispatch(block: &str, entry: u16, registers: &mut [u8; 8192], memory: &mut [u8], budget: &mut usize) -> Result<u64, runtime::Trap> {{"
    )
    .unwrap();
    writeln!(lib_code, "    match (block, entry) {{").unwrap();
    let mut seen_entries = BTreeSet::new();
    for f in &lowered {
        let entry_u16 = (f.entry & 0xffff) as u16;
        if seen_entries.insert((f.block.clone(), entry_u16)) {
            let mod_name = block_to_mod(&f.block);
            let fn_name = function_symbol(&f.block, entry_u16);
            writeln!(
                lib_code,
                "        ({:?}, {:#06x}) => {mod_name}::{fn_name}(registers, memory, budget),",
                f.block, entry_u16
            )
            .unwrap();
        }
    }
    writeln!(lib_code, "        _ => Err(runtime::Trap::InvalidPc),").unwrap();
    writeln!(lib_code, "    }}").unwrap();
    writeln!(lib_code, "}}").unwrap();
    writeln!(lib_code).unwrap();
    writeln!(
        lib_code,
        "/// Dispatch only recovered entries at their full linear addresses."
    )
    .unwrap();
    writeln!(lib_code, "pub fn dispatch_address(caller_block: &str, target: u64, registers: &mut [u8; 8192], memory: &mut [u8], budget: &mut usize) -> Result<u64, runtime::Trap> {{\n    if let Some(file_offset) = runtime::loaded_overlay_offset(target) {{\n        return match file_offset {{").unwrap();
    let mut overlay_entries = BTreeSet::new();
    for f in &lowered {
        if f.block == "EXE_CODE" {
            continue;
        }
        let row = f
            .instructions
            .iter()
            .find(|row| row.address == f.entry)
            .ok_or("missing overlay entry instruction")?;
        if row.file != "ACAD.OVL" || !overlay_entries.insert(row.file_offset) {
            return Err("ambiguous overlay entry file offset".into());
        }
        let mod_name = block_to_mod(&f.block);
        let fn_name = function_symbol(&f.block, (f.entry & 0xffff) as u16);
        writeln!(
            lib_code,
            "            {:#x} => {mod_name}::{fn_name}(registers, memory, budget),",
            row.file_offset
        )
        .unwrap();
    }
    writeln!(lib_code, "            _ => {{\n                if std::env::var_os(\"ACAD_RE_TRACE_DOS\").is_some() {{ eprintln!(\"unresolved loaded overlay target {{target:#x}} (file {{file_offset:#x}}) from {{caller_block}}\"); }}\n                Err(runtime::Trap::InvalidPc)\n            }},\n        }};\n    }}\n    match (caller_block, target) {{").unwrap();
    for f in &lowered {
        let mod_name = block_to_mod(&f.block);
        let fn_name = function_symbol(&f.block, (f.entry & 0xffff) as u16);
        let caller = if f.block == "EXE_CODE" {
            "_".into()
        } else {
            format!("{:?}", f.block)
        };
        writeln!(
            lib_code,
            "        ({caller}, {:#x}) => {mod_name}::{fn_name}(registers, memory, budget),",
            f.entry
        )
        .unwrap();
    }
    writeln!(
        lib_code,
        "        _ => {{\n            if std::env::var_os(\"ACAD_RE_TRACE_DOS\").is_some() {{ eprintln!(\"unresolved indirect target {{target:#x}} from {{caller_block}}\"); }}\n            Err(runtime::Trap::InvalidPc)\n        }},\n    }}\n}}\n"
    )
    .unwrap();
    writeln!(
        lib_code,
        "pub fn run_entry(block: &str, entry: u16, registers: &mut [u8; 8192], memory: &mut [u8], mut budget: usize) -> Result<u64, runtime::Trap> {{"
    )
    .unwrap();
    writeln!(
        lib_code,
        "    dispatch(block, entry, registers, memory, &mut budget)"
    )
    .unwrap();
    writeln!(lib_code, "}}").unwrap();

    fs::write(src_dir.join("lib.rs"), lib_code)?;

    Ok(lowered.len())
}
