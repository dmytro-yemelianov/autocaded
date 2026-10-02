//! Emit standalone Rust from validated integer machine-state IR.
use crate::ir::{Binary, Expression, Function, Operation, Place, Unary, Value};
use std::fmt::Write;

fn storage(place: &Place) -> (&'static str, usize, usize) {
    match place {
        Place::Register { offset, width } => ("registers", *offset, width.bytes()),
        Place::Temporary { offset, width } => ("temporary", *offset, width.bytes()),
    }
}
fn value(value: &Value) -> String {
    match value {
        Value::Constant { value, .. } => format!("{value}u64"),
        Value::Read(place) => {
            let (bank, offset, size) = storage(place);
            format!("read(&{bank}[..], {offset}, {size})")
        }
    }
}
fn expression(expr: &Expression, size: usize) -> String {
    match expr {
        Expression::Unary(op, input) => {
            let a = value(input);
            match op {
                Unary::Copy | Unary::ZeroExtend => a,
                Unary::BoolNegate => format!("u64::from({a} == 0)"),
                Unary::Popcount => format!("({a}).count_ones() as u64"),
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
                Binary::And => format!("{a} & {b}"),
                Binary::Or => format!("{a} | {b}"),
                Binary::Xor => format!("{a} ^ {b}"),
                Binary::Left => format!("shift_left({a}, {b})"),
                Binary::Right => format!("shift_right({a}, {b})"),
                Binary::Remainder => format!("({a}).checked_rem({b}).ok_or(Trap::DivisionByZero)?"),
                Binary::Equal => format!("u64::from({a} == {b})"),
                Binary::NotEqual => format!("u64::from({a} != {b})"),
                Binary::Less => format!("u64::from({a} < {b})"),
                Binary::SignedLess => format!("u64::from(signed({a}, {bits}) < signed({b}, {bits}))"),
                Binary::SignedBorrow => format!("u64::from((({a} ^ {b}) & ({a} ^ ({a}).wrapping_sub({b}))) & (1u64 << {}) != 0)", bits - 1),
                Binary::BoolOr => format!("u64::from({a} != 0 || {b} != 0)"),
                Binary::Subpiece => format!("shift_right({a}, ({b}).saturating_mul(8))"),
            }
        }
        Expression::Segment(segment, offset) => {
            format!("({} << 4) + {}", value(segment), value(offset))
        }
        Expression::Load(pointer) => format!("load(memory, {}, {size})?", value(pointer)),
    }
}

/// `registers` uses Ghidra's byte offsets, not a platform ABI. RAM is linear,
/// little-endian, bounds checked; segment arithmetic has no implicit A20 wrap.
/// The budget counts decoded instructions, including branch and return rows.
pub fn emit(function: &Function) -> String {
    let mut source = String::from(RUNTIME);
    writeln!(
        source,
        "// {} entry {:#x}",
        function.block.escape_default(),
        function.entry
    )
    .unwrap();
    writeln!(source, "pub fn run(registers: &mut [u8; 1024], memory: &mut [u8], budget: usize) -> Result<u64, Trap> {{").unwrap();
    writeln!(
        source,
        "let mut temporary = [0u8; {}];",
        function.temporary_bytes
    )
    .unwrap();
    writeln!(source, "let mut pc = {}u64;", function.entry).unwrap();
    source.push_str("for _ in 0..budget { match pc {\n");
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
                    writeln!(
                        source,
                        "write({bank}, {offset}, {size}, value);",
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
                Operation::Store {
                    pointer,
                    value: input,
                } => {
                    writeln!(source, "let address = {};", value(pointer)).unwrap();
                    writeln!(source, "let value = {};", value(input)).unwrap();
                    writeln!(
                        source,
                        "store(memory, address, {}, value)?;",
                        input.width().bytes()
                    )
                    .unwrap();
                }
            }
        }
        if let Some(next) = row.fallthrough {
            writeln!(source, "pc = {next};").unwrap();
        }
        source.push_str("}\n");
    }
    source.push_str("_ => return Err(Trap::InvalidPc),\n} } Err(Trap::Budget)\n}\n");
    source
}

const RUNTIME: &str = r#"// Generated from raw Pcode through acad-re's bounded integer IR.
#[derive(Debug, PartialEq, Eq)]
pub enum Trap { Memory, DivisionByZero, InvalidPc, Budget }
#[allow(dead_code)]
fn read(bank: &[u8], offset: usize, size: usize) -> u64 {
    bank[offset..offset + size].iter().enumerate()
        .fold(0, |value, (i, byte)| value | ((*byte as u64) << (i * 8)))
}
#[allow(dead_code)]
fn write(bank: &mut [u8], offset: usize, size: usize, value: u64) {
    for (i, byte) in bank[offset..offset + size].iter_mut().enumerate() {
        *byte = (value >> (i * 8)) as u8;
    }
}
#[allow(dead_code)]
fn load(memory: &[u8], address: u64, size: usize) -> Result<u64, Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get(offset..end).ok_or(Trap::Memory)?;
    Ok(read(bytes, 0, size))
}
#[allow(dead_code)]
fn store(memory: &mut [u8], address: u64, size: usize, value: u64) -> Result<(), Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get_mut(offset..end).ok_or(Trap::Memory)?;
    write(bytes, 0, size, value);
    Ok(())
}
#[allow(dead_code)]
fn signed(value: u64, bits: u32) -> i64 {
    ((value << (64 - bits)) as i64) >> (64 - bits)
}
#[allow(dead_code)]
fn shift_left(value: u64, count: u64) -> u64 {
    if count >= 64 { 0 } else { value << count }
}
#[allow(dead_code)]
fn shift_right(value: u64, count: u64) -> u64 {
    if count >= 64 { 0 } else { value >> count }
}
"#;
