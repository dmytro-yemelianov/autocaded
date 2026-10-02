//! Bounded raw-Pcode lowering. Unsupported effects fail before code generation.
//! This is a machine-state IR, not a claim to have recovered C types.
use crate::ast::Varnode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Architecture {
    pub language: String,
    pub ram_space_id: i64,
    pub userops: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecoveredExport {
    pub architecture: Architecture,
    pub functions: Vec<RecoveredFunction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecoveredFunction {
    pub block: String,
    pub entry: u16,
    pub instructions: Vec<RecoveredInstruction>,
    pub inline_data: Vec<serde_json::Value>,
    pub dependencies: Vec<serde_json::Value>,
    pub errors: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecoveredInstruction {
    pub offset: u16,
    pub address: String,
    pub bytes: String,
    pub file: String,
    pub file_offset: usize,
    pub instruction: String,
    pub ops: Vec<RawOp>,
    #[serde(default)]
    pub compiler_frame: Option<serde_json::Value>,
    #[serde(default)]
    pub kernel_tailcall: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawOp {
    pub op: String,
    pub out: Option<Varnode>,
    #[serde(rename = "in")]
    pub inputs: Vec<Option<Varnode>>,
}

/// Only supported integer widths can enter the IR.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Width(u8);
impl Width {
    pub fn bytes(self) -> usize {
        self.0 as usize
    }
    pub fn mask(self) -> u64 {
        u64::MAX >> (64 - self.0 as u32 * 8)
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Place {
    Register { offset: usize, width: Width },
    Temporary { offset: usize, width: Width },
}
impl Place {
    pub fn width(&self) -> Width {
        match self {
            Self::Register { width, .. } | Self::Temporary { width, .. } => *width,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub enum Value {
    Constant { value: u64, width: Width },
    Read(Place),
}
impl Value {
    pub fn width(&self) -> Width {
        match self {
            Self::Constant { width, .. } => *width,
            Self::Read(p) => p.width(),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize)]
pub enum Unary {
    Copy,
    ZeroExtend,
    BoolNegate,
    Popcount,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub enum Binary {
    Add,
    Sub,
    Multiply,
    And,
    Or,
    Xor,
    Left,
    Right,
    Remainder,
    Equal,
    NotEqual,
    Less,
    SignedLess,
    SignedBorrow,
    BoolOr,
    Subpiece,
}
#[derive(Debug, Clone, Serialize)]
pub enum Expression {
    Unary(Unary, Value),
    Binary(Binary, Value, Value),
    Segment(Value, Value),
    Load(Value),
}
#[derive(Debug, Clone, Serialize)]
pub enum Operation {
    Assign {
        destination: Place,
        expression: Expression,
    },
    Branch {
        condition: Value,
        target: u64,
    },
    Return {
        target: Value,
    },
    Store {
        pointer: Value,
        value: Value,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct Instruction {
    pub address: u64,
    pub file: String,
    pub file_offset: usize,
    pub bytes: String,
    pub assembly: String,
    pub operations: Vec<Operation>,
    pub fallthrough: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Function {
    pub architecture: Architecture,
    pub block: String,
    pub entry: u64,
    pub temporary_bytes: usize,
    pub instructions: Vec<Instruction>,
}

pub fn decode_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.is_empty() || hex.len() % 2 != 0 || !hex.is_ascii() {
        return Err("invalid instruction bytes".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| "invalid instruction bytes".into())
        })
        .collect()
}

/// Compare every instruction to the supplied original image before lowering.
pub fn verify_source(
    function: &RecoveredFunction,
    images: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    for row in &function.instructions {
        let bytes = decode_bytes(&row.bytes)?;
        let image = images
            .get(&row.file)
            .ok_or_else(|| format!("missing image {}", row.file))?;
        let end = row
            .file_offset
            .checked_add(bytes.len())
            .ok_or("source range overflow")?;
        if image.get(row.file_offset..end) != Some(bytes.as_slice()) {
            return Err(format!(
                "original-byte mismatch at {}:{}",
                row.file, row.file_offset
            ));
        }
    }
    Ok(())
}

fn width(node: &Varnode) -> Result<Width, String> {
    match node.size {
        1 | 2 | 4 | 8 => Ok(Width(node.size as u8)),
        _ => Err(format!("unsupported width {}", node.size)),
    }
}
fn byte_range(node: &Varnode) -> Result<std::ops::Range<i64>, String> {
    width(node)?;
    if node.offset < 0 {
        return Err("negative storage offset".into());
    }
    Ok(node.offset
        ..node
            .offset
            .checked_add(node.size as i64)
            .ok_or("storage overflow")?)
}
fn place(node: &Varnode, temps: &BTreeMap<i64, usize>) -> Result<Place, String> {
    let width = width(node)?;
    let range = byte_range(node)?;
    match node.space.as_str() {
        "register" if range.end <= 1024 => Ok(Place::Register {
            offset: range.start as usize,
            width,
        }),
        "unique" => Ok(Place::Temporary {
            offset: *temps.get(&range.start).ok_or("unallocated temporary")?,
            width,
        }),
        _ => Err(format!("unsupported storage {}", node.space)),
    }
}
fn value(
    node: &Varnode,
    temps: &BTreeMap<i64, usize>,
    defined: &BTreeSet<i64>,
) -> Result<Value, String> {
    if node.space == "const" {
        let width = width(node)?;
        return Ok(Value::Constant {
            value: node.offset as u64 & width.mask(),
            width,
        });
    }
    if node.space == "unique" && !byte_range(node)?.all(|x| defined.contains(&x)) {
        return Err("temporary read before definition in instruction".into());
    }
    Ok(Value::Read(place(node, temps)?))
}
fn constant(node: &Varnode, expected: i64) -> Result<(), String> {
    if node.space != "const" || node.offset != expected {
        return Err("unexpected space/userop selector".into());
    }
    Ok(())
}
fn linear_address(row: &RecoveredInstruction, block: &str) -> Result<u64, String> {
    if let Some((name, address)) = row.address.split_once("::") {
        if name != block {
            return Err("instruction address uses another block".into());
        }
        let address = u64::from_str_radix(address, 16).map_err(|_| "invalid linear address")?;
        if address > 0x10ffef {
            return Err("address exceeds real-mode segment range".into());
        }
        return Ok(address);
    }
    if block == "EXE_CODE" {
        let (segment, offset) = row
            .address
            .split_once(':')
            .ok_or("expected resident address")?;
        let segment = u16::from_str_radix(segment, 16).map_err(|_| "invalid segment")?;
        let offset = u16::from_str_radix(offset, 16).map_err(|_| "invalid offset")?;
        return Ok((segment as u64) * 16 + offset as u64);
    }
    Err("expected overlay address".into())
}

pub fn lower(architecture: &Architecture, raw: &RecoveredFunction) -> Result<Function, String> {
    if architecture.language != "x86:LE:16:Real Mode" {
        return Err(format!("unsupported language {}", architecture.language));
    }
    if !raw.errors.is_empty() {
        return Err("function has recovery errors".into());
    }
    if !raw.inline_data.is_empty() {
        return Err("inline compiler helper lowering required".into());
    }
    if !raw.dependencies.is_empty() {
        return Err("call/helper dependency lowering required".into());
    }
    if raw.instructions.is_empty() {
        return Err("empty function".into());
    }
    let base = linear_address(&raw.instructions[0], &raw.block)?
        .checked_sub(raw.instructions[0].offset as u64)
        .ok_or("invalid address base")?;
    let mut addresses = BTreeSet::new();
    let mut occupied = BTreeSet::new();
    let mut temporary_addresses = BTreeSet::new();
    for row in &raw.instructions {
        if row.compiler_frame.is_some() || row.kernel_tailcall.is_some() {
            return Err("compiler helper lowering is not yet supported".into());
        }
        let address = linear_address(row, &raw.block)?;
        let length = decode_bytes(&row.bytes)?.len();
        if address != base + row.offset as u64
            || !addresses.insert(address)
            || row.offset as usize + length > 65536
        {
            return Err("duplicate, inconsistent, or wrapping instruction address".into());
        }
        for byte in address..address + length as u64 {
            if !occupied.insert(byte) {
                return Err("instruction overlap".into());
            }
        }
        for node in row
            .ops
            .iter()
            .flat_map(|op| op.out.iter().chain(op.inputs.iter().flatten()))
        {
            width(node)?;
            if node.space == "unique" {
                temporary_addresses.extend(byte_range(node)?);
            }
        }
    }
    let temps: BTreeMap<_, _> = temporary_addresses
        .into_iter()
        .enumerate()
        .map(|(i, offset)| (offset, i))
        .collect();
    let entry = base + raw.entry as u64;
    if !addresses.contains(&entry) {
        return Err("missing entry instruction".into());
    }
    let mut instructions = Vec::new();
    for row in &raw.instructions {
        let mut defined = BTreeSet::new();
        let mut operations = Vec::new();
        if row.ops.is_empty() {
            return Err("instruction has no Pcode".into());
        }
        for (index, op) in row.ops.iter().enumerate() {
            let inputs: Vec<_> = op
                .inputs
                .iter()
                .map(|x| x.as_ref().ok_or("null Pcode input"))
                .collect::<Result<_, _>>()?;
            let expected = match op.op.as_str() {
                "COPY" | "INT_ZEXT" | "BOOL_NEGATE" | "POPCOUNT" | "RETURN" => 1,
                "CALLOTHER" | "STORE" => 3,
                "CBRANCH" | "LOAD" | "INT_ADD" | "INT_SUB" | "INT_MULT" | "INT_AND" | "INT_OR"
                | "INT_XOR" | "INT_LEFT" | "INT_RIGHT" | "INT_REM" | "INT_EQUAL"
                | "INT_NOTEQUAL" | "INT_LESS" | "INT_SLESS" | "INT_SBORROW" | "BOOL_OR"
                | "SUBPIECE" => 2,
                _ => return Err(format!("unsupported Pcode opcode {}", op.op)),
            };
            if inputs.len() != expected {
                return Err(format!("{} arity mismatch", op.op));
            }
            let a = || value(inputs[0], &temps, &defined);
            let b = || value(inputs[1], &temps, &defined);
            let control = matches!(op.op.as_str(), "CBRANCH" | "RETURN");
            if op.op == "STORE" {
                if op.out.is_some() {
                    return Err("STORE must not have an output".into());
                }
                constant(inputs[0], architecture.ram_space_id)?;
                operations.push(Operation::Store {
                    pointer: b()?,
                    value: value(inputs[2], &temps, &defined)?,
                });
                continue;
            }
            if control {
                if op.out.is_some() || index + 1 != row.ops.len() {
                    return Err("control transfer must terminate instruction Pcode".into());
                }
                if op.op == "RETURN" {
                    operations.push(Operation::Return { target: a()? });
                } else {
                    if inputs[0].space != "ram"
                        || inputs[0].offset < 0
                        || !addresses.contains(&(inputs[0].offset as u64))
                    {
                        return Err("unresolved or relative Pcode branch".into());
                    }
                    if inputs[1].size != 1 {
                        return Err("branch condition must be one byte".into());
                    }
                    operations.push(Operation::Branch {
                        condition: b()?,
                        target: inputs[0].offset as u64,
                    });
                }
                continue;
            }
            let output = op.out.as_ref().ok_or("missing Pcode output")?;
            let destination = place(output, &temps)?;
            let expression = match op.op.as_str() {
                "CALLOTHER" => {
                    constant(inputs[0], 0)?;
                    if architecture.userops.first().map(String::as_str) != Some("segment")
                        || inputs[1].size != 2
                        || inputs[2].size != 2
                        || output.size != 4
                    {
                        return Err("unverified segment userop contract".into());
                    }
                    Expression::Segment(
                        value(inputs[1], &temps, &defined)?,
                        value(inputs[2], &temps, &defined)?,
                    )
                }
                "LOAD" => {
                    constant(inputs[0], architecture.ram_space_id)?;
                    Expression::Load(b()?)
                }
                "COPY" | "INT_ZEXT" | "BOOL_NEGATE" | "POPCOUNT" => {
                    let unary = match op.op.as_str() {
                        "COPY" => {
                            if output.size != inputs[0].size {
                                return Err("COPY width mismatch".into());
                            }
                            Unary::Copy
                        }
                        "INT_ZEXT" => {
                            if output.size <= inputs[0].size {
                                return Err("invalid extension width".into());
                            }
                            Unary::ZeroExtend
                        }
                        "BOOL_NEGATE" => {
                            if output.size != 1 || inputs[0].size != 1 {
                                return Err("invalid boolean width".into());
                            }
                            Unary::BoolNegate
                        }
                        _ => Unary::Popcount,
                    };
                    Expression::Unary(unary, a()?)
                }
                other => {
                    let binary = match other {
                        "INT_ADD" => Binary::Add,
                        "INT_SUB" => Binary::Sub,
                        "INT_MULT" => Binary::Multiply,
                        "INT_AND" => Binary::And,
                        "INT_OR" => Binary::Or,
                        "INT_XOR" => Binary::Xor,
                        "INT_LEFT" => Binary::Left,
                        "INT_RIGHT" => Binary::Right,
                        "INT_REM" => Binary::Remainder,
                        "INT_EQUAL" => Binary::Equal,
                        "INT_NOTEQUAL" => Binary::NotEqual,
                        "INT_LESS" => Binary::Less,
                        "INT_SLESS" => Binary::SignedLess,
                        "INT_SBORROW" => Binary::SignedBorrow,
                        "BOOL_OR" => Binary::BoolOr,
                        "SUBPIECE" => Binary::Subpiece,
                        _ => return Err(format!("unsupported Pcode opcode {other}")),
                    };
                    match binary {
                        Binary::Equal
                        | Binary::NotEqual
                        | Binary::Less
                        | Binary::SignedLess
                        | Binary::SignedBorrow => {
                            if inputs[0].size != inputs[1].size || output.size != 1 {
                                return Err("comparison width mismatch".into());
                            }
                        }
                        Binary::BoolOr => {
                            if inputs[0].size != 1 || inputs[1].size != 1 || output.size != 1 {
                                return Err("boolean width mismatch".into());
                            }
                        }
                        Binary::Left | Binary::Right => {
                            if output.size != inputs[0].size {
                                return Err("shift width mismatch".into());
                            }
                        }
                        Binary::Subpiece => {
                            if inputs[1].space != "const"
                                || inputs[1].offset < 0
                                || inputs[1].offset as u64 + output.size as u64
                                    > inputs[0].size as u64
                            {
                                return Err("invalid SUBPIECE range".into());
                            }
                        }
                        _ => {
                            if inputs[0].size != inputs[1].size || output.size != inputs[0].size {
                                return Err("integer width mismatch".into());
                            }
                        }
                    }
                    Expression::Binary(binary, a()?, b()?)
                }
            };
            operations.push(Operation::Assign {
                destination,
                expression,
            });
            if output.space == "unique" {
                defined.extend(byte_range(output)?);
            }
        }
        let address = base + row.offset as u64;
        let next = address + decode_bytes(&row.bytes)?.len() as u64;
        let fallthrough = if matches!(operations.last(), Some(Operation::Return { .. })) {
            None
        } else {
            if !addresses.contains(&next) {
                return Err(format!("unresolved fallthrough at {address:x}"));
            }
            Some(next)
        };
        instructions.push(Instruction {
            address,
            file: row.file.clone(),
            file_offset: row.file_offset,
            bytes: row.bytes.clone(),
            assembly: row.instruction.clone(),
            operations,
            fallthrough,
        });
    }
    Ok(Function {
        architecture: architecture.clone(),
        block: raw.block.clone(),
        entry,
        temporary_bytes: temps.len(),
        instructions,
    })
}
