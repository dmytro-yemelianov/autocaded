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
                Binary::Divide => format!("({a}).checked_div({b}).ok_or(Trap::DivisionByZero)?"),
                Binary::SignedDivide => format!(
                    "(({a} as i{bits}).checked_div({b} as i{bits}).ok_or(Trap::DivisionByZero)? as u64)"
                ),
                Binary::And => format!("{a} & {b}"),
                Binary::Or => format!("{a} | {b}"),
                Binary::Xor => format!("{a} ^ {b}"),
                Binary::Left => format!("shift_left({a}, {b})"),
                Binary::Right => format!("shift_right({a}, {b})"),
                Binary::SignedRight => format!("shift_signed_right({a}, {b}, {bits})"),
                Binary::Remainder => {
                    format!("({a}).checked_rem({b}).ok_or(Trap::DivisionByZero)?")
                }
                Binary::SignedRemainder => format!(
                    "(({a} as i{bits}).checked_rem({b} as i{bits}).ok_or(Trap::DivisionByZero)? as u64)"
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
            format!("({} << 4) + {}", value(segment), value(offset))
        }
        Expression::Load(pointer) => {
            if size > 8 {
                format!("load128(memory, {}, {size})?", value(pointer))
            } else {
                format!("load(memory, {}, {size})?", value(pointer))
            }
        }
        Expression::In(port) => format!("read_port({})", value(port)),
        Expression::Swi(int_num) => format!("swi({})", value(int_num)),
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
    writeln!(source, "pub fn run(registers: &mut [u8; 8192], memory: &mut [u8], budget: usize) -> Result<u64, Trap> {{").unwrap();
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
                    writeln!(source, "pc = {target}; continue;").unwrap();
                }
                Operation::Call { target } => {
                    writeln!(source, "let _ = {target}; // call").unwrap();
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
                        "if {} != 0 {{ return Err(Trap::StackOverflow); }}",
                        value(condition)
                    )
                    .unwrap();
                }
                Operation::TailCall {
                    target_block,
                    target,
                } => {
                    writeln!(source, "let _ = ({target_block:?}, {target}); // tailcall").unwrap();
                    writeln!(source, "return Ok(0);").unwrap();
                }
                Operation::CallIndirect { target } => {
                    writeln!(source, "let _ = {}; // callind", value(target)).unwrap();
                }
                Operation::KernelTailCall { .. } => {
                    // This entry needs the linked resident bridge contract.
                    writeln!(source, "return Err(Trap::InvalidPc);").unwrap();
                }
                Operation::JumpIndirect { target } => {
                    writeln!(source, "pc = {}; continue;", value(target)).unwrap();
                }
                Operation::Out { port, value: val } => {
                    writeln!(source, "write_port({}, {});", value(port), value(val)).unwrap();
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
    source.push_str("_ => return Err(Trap::InvalidPc),\n} } Err(Trap::Budget)\n}\n");
    source
}

const RUNTIME: &str = r#"// Generated from raw Pcode through acad-re's bounded integer IR.
#[derive(Debug, PartialEq, Eq)]
pub enum Trap { Memory, DivisionByZero, InvalidPc, Budget, StackOverflow }
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
fn read128(bank: &[u8], offset: usize, size: usize) -> u128 {
    bank[offset..offset + size].iter().enumerate()
        .fold(0u128, |value, (i, byte)| value | ((*byte as u128) << (i * 8)))
}
#[allow(dead_code)]
fn write128(bank: &mut [u8], offset: usize, size: usize, value: u128) {
    for (i, byte) in bank[offset..offset + size].iter_mut().enumerate() {
        *byte = (value >> (i * 8)) as u8;
    }
}
#[allow(dead_code)]
fn load128(memory: &[u8], address: u64, size: usize) -> Result<u128, Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get(offset..end).ok_or(Trap::Memory)?;
    Ok(read128(bytes, 0, size))
}
#[allow(dead_code)]
fn store128(memory: &mut [u8], address: u64, size: usize, value: u128) -> Result<(), Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get_mut(offset..end).ok_or(Trap::Memory)?;
    write128(bytes, 0, size, value);
    Ok(())
}
#[allow(dead_code)]
fn f80_to_f64(raw: u128) -> f64 {
    let sign = ((raw >> 79) & 1) != 0;
    let exp = ((raw >> 64) & 0x7fff) as u32;
    let mantissa = (raw & 0xffff_ffff_ffff_ffff) as u64;
    if exp == 0 && mantissa == 0 {
        return if sign { -0.0 } else { 0.0 };
    }
    if exp == 0x7fff {
        if mantissa == 0x8000_0000_0000_0000 {
            return if sign { f64::NEG_INFINITY } else { f64::INFINITY };
        }
        return f64::NAN;
    }
    let frac = (mantissa & 0x7fff_ffff_ffff_ffff) as f64 / (1u64 << 63) as f64;
    let val = if mantissa & (1u64 << 63) != 0 { 1.0 + frac } else { frac };
    let shift = (exp as i32) - 16383;
    let res = val * (2.0f64).powi(shift.clamp(-1022, 1023));
    if sign { -res } else { res }
}
#[allow(dead_code)]
fn f64_to_f80(val: f64) -> u128 {
    if val == 0.0 {
        return if val.is_sign_negative() { 1u128 << 79 } else { 0 };
    }
    if val.is_nan() {
        return (0x7fffu128 << 64) | (0xc000_0000_0000_0000u128);
    }
    if val.is_infinite() {
        let sign_bit = if val.is_sign_negative() { 1u128 << 79 } else { 0 };
        return sign_bit | (0x7fffu128 << 64) | (0x8000_0000_0000_0000u128);
    }
    let bits = val.to_bits();
    let sign = (bits >> 63) != 0;
    let exp = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & 0x000f_ffff_ffff_ffff;
    let (f80_exp, f80_mant) = if exp == 0 {
        (0u128, (mantissa as u128) << 12)
    } else {
        let e = (exp - 1023 + 16383) as u128;
        let m = (1u128 << 63) | ((mantissa as u128) << 11);
        (e, m)
    };
    let sign_bit = if sign { 1u128 << 79 } else { 0 };
    sign_bit | (f80_exp << 64) | f80_mant
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
#[allow(dead_code)]
fn shift_signed_right(value: u64, count: u64, bits: u32) -> u64 {
    let signed_val = signed(value, bits);
    let shift = if count >= bits as u64 { (bits - 1) as u64 } else { count };
    (signed_val >> shift) as u64
}
#[allow(dead_code)]
fn read_port(_port: u64) -> u64 { 0 }
#[allow(dead_code)]
fn write_port(_port: u64, _value: u64) {}
#[allow(dead_code)]
fn swi(_int_num: u64) -> u64 { 0 }
"#;
