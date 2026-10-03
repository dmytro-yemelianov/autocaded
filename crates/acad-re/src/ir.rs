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
pub struct RecoveredEdge {
    pub from: u16,
    pub kind: String,
    #[serde(default)]
    pub case: Option<i64>,
    #[serde(default)]
    pub target: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecoveredFunction {
    pub block: String,
    pub entry: u16,
    pub instructions: Vec<RecoveredInstruction>,
    pub inline_data: Vec<InlineData>,
    pub dependencies: Vec<RecoveredDependency>,
    #[serde(default)]
    pub edges: Vec<RecoveredEdge>,
    pub errors: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InlineData {
    pub offset: u16,
    pub bytes: String,
    pub kind: String,
    pub call: u16,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecoveredDependency {
    pub block: String,
    pub offset: u16,
    pub kind: (String, Option<u16>),
    pub status: String,
    pub signature_bytes: String,
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
    pub compiler_frame: Option<CompilerFrame>,
    #[serde(default)]
    pub kernel_tailcall: Option<KernelTailcall>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompilerFrame {
    pub local_bytes: u16,
    pub continuation: u64,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KernelTailcall {
    pub native_ip: u16,
    pub target: u64,
    pub block: String,
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
    SignExtend,
    BoolNegate,
    Popcount,
    TwoComplement,
    BitwiseNegate,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub enum Binary {
    Add,
    Sub,
    Multiply,
    Divide,
    SignedDivide,
    And,
    Or,
    Xor,
    Left,
    Right,
    SignedRight,
    Remainder,
    SignedRemainder,
    Equal,
    NotEqual,
    Less,
    SignedLess,
    SignedBorrow,
    Carry,
    SignedCarry,
    BoolOr,
    BoolAnd,
    BoolXor,
    Subpiece,
}
#[derive(Debug, Clone, Serialize)]
pub enum Expression {
    Unary(Unary, Value),
    Binary(Binary, Value, Value),
    Segment(Value, Value),
    Load(Value),
    In(Value),
    Swi(Value),
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
    Trap {
        condition: Value,
    },
    Call {
        target: u64,
    },
    TailCall {
        target_block: String,
        target: u64,
    },
    Jump {
        target: u64,
    },
    CallIndirect {
        target: Value,
    },
    JumpIndirect {
        target: Value,
    },
    Out {
        port: Value,
        value: Value,
    },
    Lock,
    Unlock,
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
        1 | 2 | 3 | 4 | 8 => Ok(Width(node.size as u8)),
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
    for inline in &raw.inline_data {
        if inline.kind != "frame-size"
            && inline.kind != "kernel-function"
            && inline.kind != "switch-table"
        {
            return Err("inline compiler helper lowering required".into());
        }
    }
    for dep in &raw.dependencies {
        if dep.kind.0 != "frame" && dep.kind.0 != "kernel-tailcall" && dep.kind.0 != "switch" {
            return Err("call/helper dependency lowering required".into());
        }
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
    let mut next_temp = temps.len();
    let entry = base + raw.entry as u64;
    if !addresses.contains(&entry) {
        return Err("missing entry instruction".into());
    }
    let mut instructions = Vec::new();
    for row in &raw.instructions {
        let mut defined = BTreeSet::new();
        let mut operations = Vec::new();
        let fallthrough;
        if let Some(frame) = &row.compiler_frame {
            let row_bytes = decode_bytes(&row.bytes)?;
            if row_bytes.len() != 3 || row_bytes[0] != 0xe8 {
                return Err("compiler frame instruction must be a near call".into());
            }
            let target = (row.offset as u32)
                .wrapping_add(3)
                .wrapping_add((u16::from_le_bytes([row_bytes[1], row_bytes[2]]) as i16) as u32)
                & 0xFFFF;
            let target = target as u16;
            let dep = raw
                .dependencies
                .iter()
                .find(|d| d.offset == target && d.kind.0 == "frame")
                .ok_or("missing dependency for compiler frame")?;
            let sig = decode_bytes(&dep.signature_bytes)?;
            let limit_addr = if sig.starts_with(&[0x5e, 0x33, 0xc0, 0xeb, 0x06]) {
                if sig.len() < 20 || sig[5 + 11] != 0x3b || sig[5 + 12] != 0x26 {
                    return Err("unrecognized compiler helper signature (width 0)".into());
                }
                u16::from_le_bytes([sig[18], sig[19]]) as u64
            } else if sig.starts_with(&[0x5e, 0xfc, 0x2e]) {
                if sig.len() < 15 || sig[11] != 0x3b || sig[12] != 0x26 {
                    return Err("unrecognized compiler helper signature (width 1/2)".into());
                }
                u16::from_le_bytes([sig[13], sig[14]]) as u64
            } else {
                return Err("unrecognized compiler helper signature".into());
            };

            let sp_place = Place::Register {
                offset: 16,
                width: Width(2),
            };
            operations.push(Operation::Assign {
                destination: sp_place.clone(),
                expression: Expression::Binary(
                    Binary::Sub,
                    Value::Read(sp_place.clone()),
                    Value::Constant {
                        value: 2,
                        width: Width(2),
                    },
                ),
            });
            let bp_read = Value::Read(Place::Register {
                offset: 20,
                width: Width(2),
            });
            operations.push(Operation::Store {
                pointer: Value::Read(sp_place.clone()),
                value: bp_read,
            });
            let bp_place = Place::Register {
                offset: 20,
                width: Width(2),
            };
            operations.push(Operation::Assign {
                destination: bp_place,
                expression: Expression::Unary(Unary::Copy, Value::Read(sp_place.clone())),
            });
            let ax_place = Place::Register {
                offset: 0,
                width: Width(2),
            };
            operations.push(Operation::Assign {
                destination: ax_place,
                expression: Expression::Unary(
                    Unary::Copy,
                    Value::Constant {
                        value: frame.local_bytes as u64,
                        width: Width(2),
                    },
                ),
            });
            if frame.local_bytes > 0 {
                operations.push(Operation::Assign {
                    destination: sp_place.clone(),
                    expression: Expression::Binary(
                        Binary::Sub,
                        Value::Read(sp_place.clone()),
                        Value::Constant {
                            value: frame.local_bytes as u64,
                            width: Width(2),
                        },
                    ),
                });
            }

            let t_limit = Place::Temporary {
                offset: next_temp,
                width: Width(2),
            };
            next_temp += 2;
            operations.push(Operation::Assign {
                destination: t_limit.clone(),
                expression: Expression::Load(Value::Constant {
                    value: limit_addr,
                    width: Width(2),
                }),
            });

            let t_less = Place::Temporary {
                offset: next_temp,
                width: Width(1),
            };
            next_temp += 1;
            operations.push(Operation::Assign {
                destination: t_less.clone(),
                expression: Expression::Binary(
                    Binary::Less,
                    Value::Read(t_limit),
                    Value::Read(sp_place),
                ),
            });

            let t_cond = Place::Temporary {
                offset: next_temp,
                width: Width(1),
            };
            next_temp += 1;
            operations.push(Operation::Assign {
                destination: t_cond.clone(),
                expression: Expression::Unary(Unary::BoolNegate, Value::Read(t_less)),
            });

            operations.push(Operation::Trap {
                condition: Value::Read(t_cond),
            });

            fallthrough = Some(base + frame.continuation);
        } else if let Some(tailcall) = &row.kernel_tailcall {
            operations.push(Operation::TailCall {
                target_block: tailcall.block.clone(),
                target: tailcall.target,
            });
            fallthrough = None;
        } else if raw
            .inline_data
            .iter()
            .any(|i| i.call == row.offset && i.kind == "switch-table")
        {
            let sp_place = Place::Register {
                offset: 16,
                width: Width(2),
            };
            let ax_place = Place::Register {
                offset: 0,
                width: Width(2),
            };

            // AX = [SP]
            operations.push(Operation::Assign {
                destination: ax_place.clone(),
                expression: Expression::Load(Value::Read(sp_place.clone())),
            });
            // SP = SP + 2
            operations.push(Operation::Assign {
                destination: sp_place.clone(),
                expression: Expression::Binary(
                    Binary::Add,
                    Value::Read(sp_place),
                    Value::Constant {
                        value: 2,
                        width: Width(2),
                    },
                ),
            });

            let switch_edges: Vec<_> = raw
                .edges
                .iter()
                .filter(|e| e.from == row.offset && e.kind == "switch")
                .collect();

            for edge in &switch_edges {
                if let Some(case) = edge.case {
                    let target_offset = edge.target.ok_or("missing switch case target")?;
                    let target = base + target_offset;
                    if !addresses.contains(&target) {
                        return Err("unresolved switch case target".into());
                    }

                    let cond_temp = Place::Temporary {
                        offset: next_temp,
                        width: Width(1),
                    };
                    next_temp += 1;

                    operations.push(Operation::Assign {
                        destination: cond_temp.clone(),
                        expression: Expression::Binary(
                            Binary::Equal,
                            Value::Read(ax_place.clone()),
                            Value::Constant {
                                value: case as u64 & 0xFFFF,
                                width: Width(2),
                            },
                        ),
                    });
                    operations.push(Operation::Branch {
                        condition: Value::Read(cond_temp),
                        target,
                    });
                }
            }

            let default_edge = switch_edges
                .iter()
                .find(|e| e.case.is_none())
                .ok_or("missing switch default")?;
            let default_target =
                base + default_edge.target.ok_or("missing switch default target")?;
            if !addresses.contains(&default_target) {
                return Err("unresolved switch default target".into());
            }

            operations.push(Operation::Jump {
                target: default_target,
            });
            fallthrough = None;
        } else {
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
                    "COPY" | "INT_ZEXT" | "INT_SEXT" | "BOOL_NEGATE" | "POPCOUNT" | "RETURN"
                    | "CALL" | "BRANCH" | "INT_2COMP" | "INT_NEGATE" | "CALLIND" | "BRANCHIND" => 1,
                    "STORE" => 3,
                    "CALLOTHER" => {
                        if inputs.is_empty() {
                            return Err("CALLOTHER requires at least 1 input".into());
                        }
                        match inputs[0].offset {
                            17 | 18 => 1,
                            1 | 16 => 2,
                            0 | 2 => 3,
                            _ => {
                                return Err(format!(
                                    "unsupported CALLOTHER userop {}",
                                    inputs[0].offset
                                ))
                            }
                        }
                    }
                    "CBRANCH" | "LOAD" | "INT_ADD" | "INT_SUB" | "INT_MULT" | "INT_AND"
                    | "INT_OR" | "INT_XOR" | "INT_LEFT" | "INT_RIGHT" | "INT_REM" | "INT_EQUAL"
                    | "INT_NOTEQUAL" | "INT_LESS" | "INT_SLESS" | "INT_SBORROW" | "BOOL_OR"
                    | "BOOL_AND" | "BOOL_XOR" | "SUBPIECE" | "INT_CARRY" | "INT_SCARRY"
                    | "INT_DIV" | "INT_SDIV" | "INT_SRIGHT" | "INT_SREM" => 2,
                    _ => return Err(format!("unsupported Pcode opcode {}", op.op)),
                };
                if inputs.len() != expected {
                    return Err(format!("{} arity mismatch", op.op));
                }
                let a = || value(inputs[0], &temps, &defined);
                let b = || value(inputs[1], &temps, &defined);
                let control = matches!(
                    op.op.as_str(),
                    "CBRANCH" | "RETURN" | "CALL" | "BRANCH" | "CALLIND" | "BRANCHIND"
                );
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
                    if op.out.is_some() || (op.op != "CBRANCH" && index + 1 != row.ops.len()) {
                        return Err("control transfer must terminate instruction Pcode".into());
                    }
                    if op.op == "RETURN" {
                        operations.push(Operation::Return { target: a()? });
                    } else if op.op == "CALL" {
                        if inputs[0].space != "ram"
                            || inputs[0].offset < 0
                            || inputs[0].offset as u64 > 0x10ffef
                        {
                            return Err("unresolved Pcode call".into());
                        }
                        operations.push(Operation::Call {
                            target: inputs[0].offset as u64,
                        });
                    } else if op.op == "BRANCH" {
                        if (inputs[0].space != "ram" && inputs[0].space != raw.block)
                            || inputs[0].offset < 0
                            || !addresses.contains(&(inputs[0].offset as u64))
                        {
                            return Err("unresolved or relative Pcode branch".into());
                        }
                        operations.push(Operation::Jump {
                            target: inputs[0].offset as u64,
                        });
                    } else if op.op == "CBRANCH" {
                        if (inputs[0].space != "ram" && inputs[0].space != raw.block)
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
                    } else if op.op == "CALLIND" {
                        if inputs[0].size != 4 {
                            return Err("CALLIND target width must be 4".into());
                        }
                        operations.push(Operation::CallIndirect { target: a()? });
                    } else if op.op == "BRANCHIND" {
                        if inputs[0].size != 4 {
                            return Err("BRANCHIND target width must be 4".into());
                        }
                        operations.push(Operation::JumpIndirect { target: a()? });
                    }
                    continue;
                }
                if op.op == "CALLOTHER" && op.out.is_none() {
                    match inputs[0].offset {
                        2 => {
                            let port = b()?;
                            let val = value(inputs[2], &temps, &defined)?;
                            operations.push(Operation::Out { port, value: val });
                        }
                        17 => operations.push(Operation::Lock),
                        18 => operations.push(Operation::Unlock),
                        _ => {
                            return Err(format!(
                                "unsupported outputless CALLOTHER {}",
                                inputs[0].offset
                            ))
                        }
                    }
                    continue;
                }
                let output = op.out.as_ref().ok_or("missing Pcode output")?;
                let destination = place(output, &temps)?;
                let expression = match op.op.as_str() {
                    "CALLOTHER" => match inputs[0].offset {
                        0 => {
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
                        1 => Expression::In(value(inputs[1], &temps, &defined)?),
                        16 => {
                            if output.size != 4 {
                                return Err("swi userop contract".into());
                            }
                            Expression::Swi(value(inputs[1], &temps, &defined)?)
                        }
                        _ => {
                            return Err(format!(
                                "unsupported output CALLOTHER {}",
                                inputs[0].offset
                            ))
                        }
                    },
                    "LOAD" => {
                        constant(inputs[0], architecture.ram_space_id)?;
                        Expression::Load(b()?)
                    }
                    "COPY" | "INT_2COMP" | "INT_NEGATE" => {
                        if output.size != inputs[0].size {
                            return Err(format!("{} width mismatch", op.op));
                        }
                        let unary = match op.op.as_str() {
                            "COPY" => Unary::Copy,
                            "INT_2COMP" => Unary::TwoComplement,
                            _ => Unary::BitwiseNegate,
                        };
                        Expression::Unary(unary, a()?)
                    }
                    "INT_ZEXT" | "INT_SEXT" | "BOOL_NEGATE" | "POPCOUNT" => {
                        let unary = match op.op.as_str() {
                            "INT_ZEXT" => {
                                if output.size <= inputs[0].size {
                                    return Err("invalid extension width".into());
                                }
                                Unary::ZeroExtend
                            }
                            "INT_SEXT" => {
                                if output.size <= inputs[0].size {
                                    return Err("invalid extension width".into());
                                }
                                Unary::SignExtend
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
                            "INT_DIV" => Binary::Divide,
                            "INT_SDIV" => Binary::SignedDivide,
                            "INT_AND" => Binary::And,
                            "INT_OR" => Binary::Or,
                            "INT_XOR" => Binary::Xor,
                            "INT_LEFT" => Binary::Left,
                            "INT_RIGHT" => Binary::Right,
                            "INT_SRIGHT" => Binary::SignedRight,
                            "INT_REM" => Binary::Remainder,
                            "INT_SREM" => Binary::SignedRemainder,
                            "INT_EQUAL" => Binary::Equal,
                            "INT_NOTEQUAL" => Binary::NotEqual,
                            "INT_LESS" => Binary::Less,
                            "INT_SLESS" => Binary::SignedLess,
                            "INT_SBORROW" => Binary::SignedBorrow,
                            "BOOL_OR" => Binary::BoolOr,
                            "BOOL_AND" => Binary::BoolAnd,
                            "BOOL_XOR" => Binary::BoolXor,
                            "SUBPIECE" => Binary::Subpiece,
                            "INT_CARRY" => Binary::Carry,
                            "INT_SCARRY" => Binary::SignedCarry,
                            _ => return Err(format!("unsupported Pcode opcode {other}")),
                        };
                        match binary {
                            Binary::Equal
                            | Binary::NotEqual
                            | Binary::Less
                            | Binary::SignedLess
                            | Binary::SignedBorrow
                            | Binary::Carry
                            | Binary::SignedCarry => {
                                if inputs[0].size != inputs[1].size || output.size != 1 {
                                    return Err("comparison width mismatch".into());
                                }
                            }
                            Binary::BoolOr | Binary::BoolAnd | Binary::BoolXor => {
                                if inputs[0].size != 1 || inputs[1].size != 1 || output.size != 1 {
                                    return Err("boolean width mismatch".into());
                                }
                            }
                            Binary::Left | Binary::Right | Binary::SignedRight => {
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
                                if inputs[0].size != inputs[1].size || output.size != inputs[0].size
                                {
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
            let next = base + row.offset as u64 + decode_bytes(&row.bytes)?.len() as u64;
            fallthrough = if matches!(
                operations.last(),
                Some(
                    Operation::Return { .. }
                        | Operation::TailCall { .. }
                        | Operation::Jump { .. }
                        | Operation::JumpIndirect { .. }
                )
            ) {
                None
            } else {
                if !addresses.contains(&next) {
                    return Err(format!(
                        "unresolved fallthrough at {:x}",
                        base + row.offset as u64
                    ));
                }
                Some(next)
            };
        }
        let address = base + row.offset as u64;
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
        temporary_bytes: next_temp,
        instructions,
    })
}
