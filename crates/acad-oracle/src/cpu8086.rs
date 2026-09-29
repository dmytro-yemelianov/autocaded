//! A small, dependency-free 8086 real-mode execution core.
//!
//! This is the CPU foundation for the in-tree oracle, not yet a DOS machine:
//! BIOS/DOS interrupts and the remaining 8086 instruction set are deliberately
//! reported as unsupported rather than silently approximated.

use std::fmt;

pub const MEMORY_SIZE: usize = 1 << 20;

const CF: u16 = 1 << 0;
const PF: u16 = 1 << 2;
const AF: u16 = 1 << 4;
const ZF: u16 = 1 << 6;
const SF: u16 = 1 << 7;
const TF: u16 = 1 << 8;
const IF: u16 = 1 << 9;
const DF: u16 = 1 << 10;
const OF: u16 = 1 << 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Registers {
    pub ax: u16,
    pub bx: u16,
    pub cx: u16,
    pub dx: u16,
    pub sp: u16,
    pub bp: u16,
    pub si: u16,
    pub di: u16,
    pub cs: u16,
    pub ds: u16,
    pub ss: u16,
    pub es: u16,
    pub ip: u16,
    flags: u16,
}

impl Registers {
    pub fn carry(self) -> bool {
        self.flags & CF != 0
    }

    pub fn parity(self) -> bool {
        self.flags & PF != 0
    }

    pub fn auxiliary_carry(self) -> bool {
        self.flags & AF != 0
    }

    pub fn zero(self) -> bool {
        self.flags & ZF != 0
    }

    pub fn sign(self) -> bool {
        self.flags & SF != 0
    }

    pub fn overflow(self) -> bool {
        self.flags & OF != 0
    }

    pub fn raw_flags(self) -> u16 {
        self.flags | 2
    }

    fn set_flag(&mut self, flag: u16, value: bool) {
        if value {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuError {
    UnsupportedOpcode { cs: u16, ip: u16, opcode: u8 },
    StepLimit { limit: usize },
}

impl fmt::Display for CpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOpcode { cs, ip, opcode } => {
                write!(
                    f,
                    "unsupported 8086 opcode {opcode:#04x} at {cs:04x}:{ip:04x}"
                )
            }
            Self::StepLimit { limit } => write!(f, "8086 execution exceeded {limit} steps"),
        }
    }
}

impl std::error::Error for CpuError {}

#[derive(Debug, Clone)]
pub struct Cpu8086 {
    pub registers: Registers,
    /// Minimal 8087 state. The instruction decoder grows this as the DOS
    /// program exercises more of the coprocessor instruction set.
    pub fpu: Fpu8087,
    memory: Vec<u8>,
    io_ports: Vec<u8>,
    halted: bool,
    instructions: u64,
    pending_interrupt: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fpu8087 {
    pub control_word: u16,
    pub status_word: u16,
    pub tag_word: u16,
    pub registers: [f64; 8],
    /// Low words retained while values remain on the 8087 register stack.
    /// This avoids rounding every arithmetic result to a memory f64.
    extra: [f64; 8],
    pub top: u8,
}

#[derive(Debug, Clone, Copy)]
struct FpuValue {
    hi: f64,
    lo: f64,
}

impl FpuValue {
    fn from_f64(value: f64) -> Self {
        Self { hi: value, lo: 0.0 }
    }

    fn round_to_f64(self) -> f64 {
        self.hi + self.lo
    }

    fn normalize(hi: f64, lo: f64) -> Self {
        let sum = hi + lo;
        let remainder = lo - (sum - hi);
        Self {
            hi: sum,
            lo: remainder,
        }
    }

    fn round_to_8087_precision(self, control_word: u16) -> Self {
        match (control_word >> 8) & 3 {
            0 => Self::from_f64((self.round_to_f64() as f32) as f64),
            2 => Self::from_f64(self.round_to_f64()),
            _ => {
                // The 8087 default has a 64-bit significand. A normal f64
                // high word is already an exact multiple of 2^11 such units;
                // round its low residual to the nearest unit, ties to even.
                let exponent = ((self.hi.to_bits() >> 52) & 0x7ff) as i32 - 1023;
                let unit_exponent = exponent - 63;
                if !self.hi.is_normal() || unit_exponent < -1022 {
                    return self;
                }
                let unit = f64::from_bits(((unit_exponent + 1023) as u64) << 52);
                Self::normalize(self.hi, (self.lo / unit).round_ties_even() * unit)
            }
        }
    }

    fn add(self, other: Self) -> Self {
        if !self.hi.is_finite() || !other.hi.is_finite() {
            return Self::from_f64(self.hi + other.hi);
        }
        let sum = self.hi + other.hi;
        let shifted = sum - self.hi;
        let error = (self.hi - (sum - shifted)) + (other.hi - shifted);
        Self::normalize(sum, error + self.lo + other.lo)
    }

    fn sub(self, other: Self) -> Self {
        self.add(Self {
            hi: -other.hi,
            lo: -other.lo,
        })
    }

    fn mul(self, other: Self) -> Self {
        let product = self.hi * other.hi;
        if !product.is_finite() {
            return Self::from_f64(product);
        }
        let error = self.hi.mul_add(other.hi, -product)
            + self.hi * other.lo
            + self.lo * other.hi
            + self.lo * other.lo;
        Self::normalize(product, error)
    }

    fn div(self, other: Self) -> Self {
        let quotient = self.hi / other.hi;
        if !quotient.is_finite() {
            return Self::from_f64(quotient);
        }
        let first = Self::from_f64(quotient);
        let remainder = self.sub(other.mul(first));
        let correction = remainder.hi / other.hi;
        first.add(Self::from_f64(correction))
    }
}

impl Fpu8087 {
    pub fn st(&self, index: usize) -> Option<f64> {
        self.st_wide(index).map(FpuValue::round_to_f64)
    }

    fn st_wide(&self, index: usize) -> Option<FpuValue> {
        if index >= 8 {
            return None;
        }
        let physical = (usize::from(self.top) + index) & 7;
        let tag = (self.tag_word >> (physical * 2)) & 3;
        (tag != 3).then_some(FpuValue {
            hi: self.registers[physical],
            lo: self.extra[physical],
        })
    }

    fn push(&mut self, value: f64) {
        self.top = self.top.wrapping_sub(1) & 7;
        self.registers[usize::from(self.top)] = value;
        self.extra[usize::from(self.top)] = 0.0;
        // 8087 tag value 00b denotes a valid finite register.
        self.tag_word &= !(3 << (usize::from(self.top) * 2));
        self.status_word = (self.status_word & !(7 << 11)) | (u16::from(self.top) << 11);
    }

    fn set_st_wide(&mut self, index: usize, value: FpuValue) {
        let physical = (usize::from(self.top) + index) & 7;
        let value = value.round_to_8087_precision(self.control_word);
        self.registers[physical] = value.hi;
        self.extra[physical] = value.lo;
        self.tag_word &= !(3 << (physical * 2));
    }

    fn pop(&mut self) -> Option<f64> {
        let physical = usize::from(self.top);
        if (self.tag_word >> (physical * 2)) & 3 == 3 {
            return None;
        }
        let value = FpuValue {
            hi: self.registers[physical],
            lo: self.extra[physical],
        }
        .round_to_f64();
        self.tag_word |= 3 << (physical * 2);
        self.top = self.top.wrapping_add(1) & 7;
        self.status_word = (self.status_word & !(7 << 11)) | (u16::from(self.top) << 11);
        Some(value)
    }

    fn rounded_integer(&self, value: f64) -> Option<i64> {
        if !value.is_finite() {
            return None;
        }
        let rounded = match (self.control_word >> 10) & 3 {
            0 => value.round_ties_even(),
            1 => value.floor(),
            2 => value.ceil(),
            _ => value.trunc(),
        };
        (rounded >= i64::MIN as f64 && rounded < -(i64::MIN as f64)).then_some(rounded as i64)
    }
}

impl Default for Fpu8087 {
    fn default() -> Self {
        Self {
            control_word: 0x037F,
            status_word: 0,
            tag_word: 0xFFFF,
            registers: [0.0; 8],
            extra: [0.0; 8],
            top: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Operand {
    Register(u8),
    Memory(u16, u16),
}

impl Default for Cpu8086 {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu8086 {
    pub fn new() -> Self {
        Self {
            registers: Registers::default(),
            fpu: Fpu8087::default(),
            memory: vec![0; MEMORY_SIZE],
            io_ports: vec![0; 1 << 16],
            halted: false,
            instructions: 0,
            pending_interrupt: None,
        }
    }

    pub fn load(&mut self, segment: u16, offset: u16, bytes: &[u8]) {
        let start = physical(segment, offset);
        for (i, byte) in bytes.iter().copied().enumerate() {
            self.memory[(start + i) & (MEMORY_SIZE - 1)] = byte;
        }
        self.registers.cs = segment;
        self.registers.ip = offset;
        self.halted = false;
        self.instructions = 0;
        self.pending_interrupt = None;
    }

    pub fn read_byte(&self, segment: u16, offset: u16) -> u8 {
        self.memory[physical(segment, offset)]
    }

    pub fn write_byte(&mut self, segment: u16, offset: u16, value: u8) {
        let at = physical(segment, offset);
        self.memory[at] = value;
    }

    pub fn read_word(&self, segment: u16, offset: u16) -> u16 {
        u16::from_le_bytes([
            self.read_byte(segment, offset),
            self.read_byte(segment, offset.wrapping_add(1)),
        ])
    }

    pub fn write_word(&mut self, segment: u16, offset: u16, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.write_byte(segment, offset, low);
        self.write_byte(segment, offset.wrapping_add(1), high);
    }

    pub fn read_port_byte(&self, port: u16) -> u8 {
        self.io_ports[usize::from(port)]
    }

    pub fn write_port_byte(&mut self, port: u16, value: u8) {
        self.io_ports[usize::from(port)] = value;
    }

    fn read_port_word(&self, port: u16) -> u16 {
        u16::from_le_bytes([
            self.read_port_byte(port),
            self.read_port_byte(port.wrapping_add(1)),
        ])
    }

    fn write_port_word(&mut self, port: u16, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.write_port_byte(port, low);
        self.write_port_byte(port.wrapping_add(1), high);
    }

    pub fn is_halted(&self) -> bool {
        self.halted
    }

    pub fn instructions(&self) -> u64 {
        self.instructions
    }

    /// Return the most recently executed software-interrupt number once.
    /// The interrupt vector has already been followed, so a machine layer can
    /// observe or service the interrupt before the handler's first opcode.
    pub fn take_interrupt(&mut self) -> Option<u8> {
        self.pending_interrupt.take()
    }

    pub fn set_zero_flag(&mut self, value: bool) {
        self.registers.set_flag(ZF, value);
    }

    /// Complete a software-interrupt return after a host service routine has
    /// updated registers and memory.
    pub fn interrupt_return(&mut self) {
        self.registers.ip = self.pop16();
        self.registers.cs = self.pop16();
        self.registers.flags = self.pop16() | 2;
    }

    /// Execute one instruction. The initial core covers common bootstrap and
    /// scalar code operations; unsupported opcodes fail with their address.
    pub fn step(&mut self) -> Result<(), CpuError> {
        if self.halted {
            return Ok(());
        }
        let ip = self.registers.ip;
        let mut opcode = self.fetch8();
        let mut segment_override = None;
        let mut repeat = None;
        loop {
            let segment = match opcode {
                0x26 => Some(self.registers.es),
                0x2E => Some(self.registers.cs),
                0x36 => Some(self.registers.ss),
                0x3E => Some(self.registers.ds),
                _ => None,
            };
            if let Some(segment) = segment {
                segment_override = Some(segment);
                opcode = self.fetch8();
            } else if matches!(opcode, 0xF2 | 0xF3) {
                repeat = Some(opcode);
                opcode = self.fetch8();
            } else if opcode == 0xF0 {
                opcode = self.fetch8();
            } else {
                break;
            }
        }
        match opcode {
            0x90 => {} // NOP
            0x9B => {} // FWAIT: the in-tree 8087 has no asynchronous operations
            0x9E => {
                let ah = self.register8(4).expect("AH register");
                let mask = SF | ZF | AF | PF | CF;
                self.registers.flags = (self.registers.flags & !mask) | (u16::from(ah) & mask);
            }
            0x9F => {
                self.set_reg8(4, (self.registers.raw_flags() & 0x00D7) as u8);
            }
            0x91..=0x97 => {
                let register = opcode - 0x90;
                let other = self.reg16(register);
                self.set_reg16(register, self.registers.ax);
                self.registers.ax = other;
            }
            0x98 => self.registers.ax = (self.registers.ax as u8 as i8 as i16) as u16, // CBW
            0x27 => {
                let original = self.register8(0).expect("AL register");
                let carry = self.registers.carry();
                let low_adjust = original & 0x0F > 9 || self.registers.flags & AF != 0;
                let high_adjust = original > 0x99 || carry;
                let mut adjusted = original;
                if low_adjust {
                    adjusted = adjusted.wrapping_add(0x06);
                }
                if high_adjust {
                    adjusted = adjusted.wrapping_add(0x60);
                }
                self.set_reg8(0, adjusted);
                self.registers.set_flag(AF, low_adjust);
                self.registers.set_flag(CF, high_adjust);
                self.set_szp8(adjusted);
            }
            0x99 => {
                self.registers.dx = if self.registers.ax & 0x8000 != 0 {
                    0xFFFF
                } else {
                    0
                }
            } // CWD
            0xD4 => {
                let base = self.fetch8();
                if base == 0 {
                    self.enter_interrupt(0, ip);
                } else {
                    let value = self.register8(0).expect("AL register");
                    self.set_reg8(4, value / base);
                    let result = value % base;
                    self.set_reg8(0, result);
                    self.set_szp8(result);
                }
            }
            0xD5 => {
                let base = self.fetch8();
                let value = self
                    .register8(4)
                    .expect("AH register")
                    .wrapping_mul(base)
                    .wrapping_add(self.register8(0).expect("AL register"));
                self.set_reg8(0, value);
                self.set_reg8(4, 0);
                self.set_szp8(value);
            }
            0xD8 => {
                let modrm = self.fetch8();
                let operation = (modrm >> 3) & 7;
                if modrm >= 0xC0 && matches!(operation, 0 | 1 | 4..=7) {
                    let right = self
                        .fpu
                        .st_wide(usize::from(modrm & 7))
                        .unwrap_or(FpuValue::from_f64(f64::NAN));
                    let left = self.fpu.st_wide(0).unwrap_or(FpuValue::from_f64(f64::NAN));
                    let result = match operation {
                        0 => left.add(right),
                        1 => left.mul(right),
                        4 => left.sub(right),
                        5 => right.sub(left),
                        6 => left.div(right),
                        7 => right.div(left),
                        _ => unreachable!(),
                    };
                    self.fpu.set_st_wide(0, result);
                } else {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                }
            }
            0xDB => {
                let modrm = self.fetch8();
                if modrm == 0xE3 {
                    // FNINIT: reset the 8087 control/status/tag state.
                    self.fpu = Fpu8087::default();
                } else {
                    let operation = (modrm >> 3) & 7;
                    let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                    let Operand::Memory(segment, offset) = operand else {
                        return Err(CpuError::UnsupportedOpcode {
                            cs: self.registers.cs,
                            ip,
                            opcode,
                        });
                    };
                    match operation {
                        0 => {
                            let low = u32::from(self.read_word(segment, offset));
                            let high = u32::from(self.read_word(segment, offset.wrapping_add(2)));
                            self.fpu.push(f64::from(((high << 16) | low) as i32));
                        }
                        3 => {
                            let value = self.fpu.st(0).unwrap_or(f64::NAN);
                            let rounded = self.fpu.rounded_integer(value);
                            let integer = rounded
                                .filter(|value| i32::try_from(*value).is_ok())
                                .unwrap_or(i64::from(i32::MIN))
                                as i32;
                            let bytes = integer.to_le_bytes();
                            for (index, byte) in bytes.into_iter().enumerate() {
                                self.write_byte(segment, offset.wrapping_add(index as u16), byte);
                            }
                            self.fpu.pop();
                        }
                        _ => {
                            return Err(CpuError::UnsupportedOpcode {
                                cs: self.registers.cs,
                                ip,
                                opcode,
                            });
                        }
                    }
                }
            }
            0xD9 => {
                let modrm = self.fetch8();
                match modrm {
                    0xE8 => self.fpu.push(1.0), // FLD1
                    _ => {
                        let operation = (modrm >> 3) & 7;
                        let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                        match (operation, operand) {
                            (5, Operand::Memory(segment, offset)) => {
                                self.fpu.control_word = self.read_word(segment, offset);
                            }
                            (7, Operand::Memory(segment, offset)) => {
                                self.write_word(segment, offset, self.fpu.control_word);
                            }
                            _ => {
                                return Err(CpuError::UnsupportedOpcode {
                                    cs: self.registers.cs,
                                    ip,
                                    opcode,
                                });
                            }
                        }
                    }
                }
            }
            0xFE => {
                let modrm = self.fetch8();
                let operation = (modrm >> 3) & 7;
                if operation > 1 {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                }
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let value = self.read_operand8(operand);
                let carry = self.registers.carry();
                let result = if operation == 0 {
                    self.alu8(0, value, 1)
                } else {
                    self.alu8(5, value, 1)
                };
                self.registers.set_flag(CF, carry);
                self.write_operand8(operand, result);
            }
            0xFF => {
                let modrm = self.fetch8();
                let operation = (modrm >> 3) & 7;
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                match operation {
                    0 | 1 => {
                        let value = self.read_operand16(operand);
                        let carry = self.registers.carry();
                        let result = if operation == 0 {
                            self.alu16(0, value, 1)
                        } else {
                            self.alu16(5, value, 1)
                        };
                        self.registers.set_flag(CF, carry);
                        self.write_operand16(operand, result);
                    }
                    2 => {
                        let target = self.read_operand16(operand);
                        self.push16(self.registers.ip);
                        self.registers.ip = target;
                    }
                    3 | 5 => {
                        let Operand::Memory(segment, offset) = operand else {
                            return Err(CpuError::UnsupportedOpcode {
                                cs: self.registers.cs,
                                ip,
                                opcode,
                            });
                        };
                        let target_ip = self.read_word(segment, offset);
                        let target_cs = self.read_word(segment, offset.wrapping_add(2));
                        if operation == 3 {
                            self.push16(self.registers.cs);
                            self.push16(self.registers.ip);
                        }
                        self.registers.ip = target_ip;
                        self.registers.cs = target_cs;
                    }
                    4 => self.registers.ip = self.read_operand16(operand),
                    6 => self.push16(self.read_operand16(operand)),
                    _ => {
                        return Err(CpuError::UnsupportedOpcode {
                            cs: self.registers.cs,
                            ip,
                            opcode,
                        });
                    }
                }
            }
            0xDD => {
                let modrm = self.fetch8();
                let operation = (modrm >> 3) & 7;
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let Operand::Memory(segment, offset) = operand else {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                };
                match operation {
                    0 => {
                        let bytes = std::array::from_fn(|index| {
                            self.read_byte(segment, offset.wrapping_add(index as u16))
                        });
                        self.fpu.push(f64::from_le_bytes(bytes));
                    }
                    3 => {
                        let bytes = self.fpu.pop().unwrap_or(f64::NAN).to_le_bytes();
                        for (index, byte) in bytes.into_iter().enumerate() {
                            self.write_byte(segment, offset.wrapping_add(index as u16), byte);
                        }
                    }
                    7 => self.write_word(segment, offset, self.fpu.status_word),
                    _ => {
                        return Err(CpuError::UnsupportedOpcode {
                            cs: self.registers.cs,
                            ip,
                            opcode,
                        });
                    }
                }
            }
            0xDE => {
                let modrm = self.fetch8();
                if modrm != 0xD9 {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                }
                let left = self.fpu.st(0).unwrap_or(f64::NAN);
                let right = self.fpu.st(1).unwrap_or(f64::NAN);
                const C0: u16 = 1 << 8;
                const C2: u16 = 1 << 10;
                const C3: u16 = 1 << 14;
                self.fpu.status_word &= !(C0 | C2 | C3);
                let condition = if left.is_nan() || right.is_nan() {
                    C0 | C2 | C3
                } else if left < right {
                    C0
                } else if left == right {
                    C3
                } else {
                    0
                };
                self.fpu.status_word |= condition;
                self.fpu.pop();
                self.fpu.pop();
            }
            0xDF => {
                let modrm = self.fetch8();
                let operation = (modrm >> 3) & 7;
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let Operand::Memory(segment, offset) = operand else {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                };
                match operation {
                    0 => self
                        .fpu
                        .push(f64::from(self.read_word(segment, offset) as i16)),
                    3 => {
                        let value = self.fpu.st(0).unwrap_or(f64::NAN);
                        let integer =
                            self.fpu.rounded_integer(value).unwrap_or(i16::MIN as i64) as i16;
                        self.write_word(segment, offset, integer as u16);
                        self.fpu.pop();
                    }
                    _ => {
                        return Err(CpuError::UnsupportedOpcode {
                            cs: self.registers.cs,
                            ip,
                            opcode,
                        });
                    }
                }
            }
            0xF4 => self.halted = true,
            0xF5 => self.registers.set_flag(CF, !self.registers.carry()),
            0xF8 => self.registers.set_flag(CF, false),
            0xF9 => self.registers.set_flag(CF, true),
            0xFA => self.registers.set_flag(IF, false),
            0xFB => self.registers.set_flag(IF, true),
            0xFC => self.registers.set_flag(DF, false),
            0xFD => self.registers.set_flag(DF, true),
            0x00..=0x3B if opcode & 7 <= 3 => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let reg = (modrm >> 3) & 7;
                let operation = (opcode >> 3) & 7;
                let is_word = opcode & 1 != 0;
                let to_register = opcode & 2 != 0;
                if is_word {
                    let (left, right) = if to_register {
                        (self.reg16(reg), self.read_operand16(operand))
                    } else {
                        (self.read_operand16(operand), self.reg16(reg))
                    };
                    let result = self.alu16(operation, left, right);
                    if operation != 7 {
                        if to_register {
                            self.set_reg16(reg, result);
                        } else {
                            self.write_operand16(operand, result);
                        }
                    }
                } else {
                    let (left, right) = if to_register {
                        (
                            self.register8(reg).expect("three-bit register index"),
                            self.read_operand8(operand),
                        )
                    } else {
                        (
                            self.read_operand8(operand),
                            self.register8(reg).expect("three-bit register index"),
                        )
                    };
                    let result = self.alu8(operation, left, right);
                    if operation != 7 {
                        if to_register {
                            self.set_reg8(reg, result);
                        } else {
                            self.write_operand8(operand, result);
                        }
                    }
                }
            }
            0x80..=0x83 => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let operation = (modrm >> 3) & 7;
                if opcode == 0x81 {
                    let left = self.read_operand16(operand);
                    let right = self.fetch16();
                    let result = self.alu16(operation, left, right);
                    if operation != 7 {
                        self.write_operand16(operand, result);
                    }
                } else if opcode == 0x83 {
                    let left = self.read_operand16(operand);
                    let right = (self.fetch8() as i8 as i16) as u16;
                    let result = self.alu16(operation, left, right);
                    if operation != 7 {
                        self.write_operand16(operand, result);
                    }
                } else {
                    let left = self.read_operand8(operand);
                    let right = self.fetch8();
                    let result = self.alu8(operation, left, right);
                    if operation != 7 {
                        self.write_operand8(operand, result);
                    }
                }
            }
            0x84 | 0x85 => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let reg = (modrm >> 3) & 7;
                if opcode == 0x85 {
                    self.logic16(self.read_operand16(operand) & self.reg16(reg));
                } else {
                    self.logic8(
                        self.read_operand8(operand)
                            & self.register8(reg).expect("three-bit register index"),
                    );
                }
            }
            0xA8 => {
                let immediate = self.fetch8();
                self.logic8(self.registers.ax as u8 & immediate);
            }
            0xA9 => {
                let immediate = self.fetch16();
                self.logic16(self.registers.ax & immediate);
            }
            0x8D => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let Operand::Memory(_, offset) = operand else {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                };
                self.set_reg16((modrm >> 3) & 7, offset);
            }
            0xC4 | 0xC5 => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let Operand::Memory(segment, offset) = operand else {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                };
                let value = self.read_word(segment, offset);
                let target_segment = self.read_word(segment, offset.wrapping_add(2));
                self.set_reg16((modrm >> 3) & 7, value);
                if opcode == 0xC4 {
                    self.registers.es = target_segment;
                } else {
                    self.registers.ds = target_segment;
                }
            }
            0x8F => {
                let modrm = self.fetch8();
                let operation = (modrm >> 3) & 7;
                if operation != 0 {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                }
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let value = self.pop16();
                self.write_operand16(operand, value);
            }
            0xF6 | 0xF7 => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let operation = (modrm >> 3) & 7;
                let word = opcode == 0xF7;
                if operation == 1 {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                }
                if operation == 0 {
                    if word {
                        let left = self.read_operand16(operand);
                        let right = self.fetch16();
                        self.logic16(left & right);
                    } else {
                        let left = self.read_operand8(operand);
                        let right = self.fetch8();
                        self.logic8(left & right);
                    }
                } else if word {
                    let value = self.read_operand16(operand);
                    match operation {
                        2 => self.write_operand16(operand, !value),
                        3 => {
                            let result = 0u16.wrapping_sub(value);
                            self.set_sub_flags(0, value, result, value != 0);
                            self.write_operand16(operand, result);
                        }
                        4 => {
                            let product = u32::from(self.registers.ax) * u32::from(value);
                            self.registers.ax = product as u16;
                            self.registers.dx = (product >> 16) as u16;
                            let overflow = self.registers.dx != 0;
                            self.registers.set_flag(CF, overflow);
                            self.registers.set_flag(OF, overflow);
                        }
                        5 => {
                            let product =
                                i32::from(self.registers.ax as i16) * i32::from(value as i16);
                            self.registers.ax = product as u16;
                            self.registers.dx = (product >> 16) as u16;
                            let overflow = !(-32768..=32767).contains(&product);
                            self.registers.set_flag(CF, overflow);
                            self.registers.set_flag(OF, overflow);
                        }
                        6 => {
                            let divisor = u32::from(value);
                            let dividend =
                                (u32::from(self.registers.dx) << 16) | u32::from(self.registers.ax);
                            if divisor == 0 || dividend / divisor > u32::from(u16::MAX) {
                                self.enter_interrupt(0, ip);
                            } else {
                                self.registers.ax = (dividend / divisor) as u16;
                                self.registers.dx = (dividend % divisor) as u16;
                            }
                        }
                        7 => {
                            let divisor = i64::from(value as i16);
                            let dividend = (i64::from(self.registers.dx as i16) << 16)
                                | i64::from(self.registers.ax);
                            let quotient = if divisor == 0 { 0 } else { dividend / divisor };
                            if divisor == 0 || !(-32768..=32767).contains(&quotient) {
                                self.enter_interrupt(0, ip);
                            } else {
                                let remainder = dividend % divisor;
                                self.registers.ax = quotient as i16 as u16;
                                self.registers.dx = remainder as i16 as u16;
                            }
                        }
                        _ => unreachable!("three-bit group-3 operation"),
                    }
                } else {
                    let value = self.read_operand8(operand);
                    match operation {
                        2 => self.write_operand8(operand, !value),
                        3 => {
                            let result = 0u8.wrapping_sub(value);
                            self.set_sub_flags8(0, value, result, value != 0);
                            self.write_operand8(operand, result);
                        }
                        4 => {
                            let product = u16::from(self.register8(0).unwrap()) * u16::from(value);
                            self.registers.ax = product;
                            let overflow = product >> 8 != 0;
                            self.registers.set_flag(CF, overflow);
                            self.registers.set_flag(OF, overflow);
                        }
                        5 => {
                            let product = i16::from(self.register8(0).unwrap() as i8)
                                * i16::from(value as i8);
                            self.registers.ax = product as u16;
                            let overflow = !(-128..=127).contains(&product);
                            self.registers.set_flag(CF, overflow);
                            self.registers.set_flag(OF, overflow);
                        }
                        6 => {
                            let divisor = u16::from(value);
                            let dividend = self.registers.ax;
                            if divisor == 0 || dividend / divisor > u16::from(u8::MAX) {
                                self.enter_interrupt(0, ip);
                            } else {
                                let quotient = (dividend / divisor) as u8;
                                let remainder = (dividend % divisor) as u8;
                                self.registers.ax = u16::from_le_bytes([quotient, remainder]);
                            }
                        }
                        7 => {
                            let divisor = i16::from(value as i8);
                            let dividend = self.registers.ax as i16;
                            let quotient = if divisor == 0 { 0 } else { dividend / divisor };
                            if divisor == 0 || !(-128..=127).contains(&quotient) {
                                self.enter_interrupt(0, ip);
                            } else {
                                let remainder = dividend % divisor;
                                self.registers.ax = u16::from_le_bytes([
                                    quotient as i8 as u8,
                                    remainder as i8 as u8,
                                ]);
                            }
                        }
                        _ => unreachable!("three-bit group-3 operation"),
                    }
                }
            }
            0xD0..=0xD3 => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let operation = (modrm >> 3) & 7;
                let count = if opcode & 2 == 0 {
                    1
                } else {
                    self.register8(1).expect("CL register index")
                };
                if opcode & 1 == 0 {
                    let value = self.read_operand8(operand);
                    let result = self.shift8(operation, value, count);
                    self.write_operand8(operand, result);
                } else {
                    let value = self.read_operand16(operand);
                    let result = self.shift16(operation, value, count);
                    self.write_operand16(operand, result);
                }
            }
            0xC6 | 0xC7 => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                if (modrm >> 3) & 7 != 0 {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                }
                if opcode == 0xC6 {
                    let value = self.fetch8();
                    self.write_operand8(operand, value);
                } else {
                    let value = self.fetch16();
                    self.write_operand16(operand, value);
                }
            }
            0x50..=0x57 => self.push16(self.reg16(opcode - 0x50)),
            0x58..=0x5F => {
                let value = self.pop16();
                self.set_reg16(opcode - 0x58, value);
            }
            0x06 => self.push16(self.registers.es),
            0x0E => self.push16(self.registers.cs),
            0x16 => self.push16(self.registers.ss),
            0x1E => self.push16(self.registers.ds),
            0x07 => self.registers.es = self.pop16(),
            0x17 => self.registers.ss = self.pop16(),
            0x1F => self.registers.ds = self.pop16(),
            0x9C => self.push16(self.registers.raw_flags()),
            0x9D => self.registers.flags = self.pop16() | 2,
            0xCD => {
                let interrupt = self.fetch8();
                self.enter_interrupt(interrupt, self.registers.ip);
            }
            0xCF => self.interrupt_return(),
            0xB0..=0xB7 => {
                let value = self.fetch8();
                self.set_reg8(opcode - 0xB0, value);
            }
            0xB8..=0xBF => {
                let value = self.fetch16();
                self.set_reg16(opcode - 0xB8, value);
            }
            0x86 | 0x87 => {
                let modrm = self.fetch8();
                let register = (modrm >> 3) & 7;
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                if opcode == 0x86 {
                    let left = self.read_operand8(operand);
                    let right = self.register8(register).expect("8-bit register");
                    self.write_operand8(operand, right);
                    self.set_reg8(register, left);
                } else {
                    let left = self.read_operand16(operand);
                    let right = self.reg16(register);
                    self.write_operand16(operand, right);
                    self.set_reg16(register, left);
                }
            }
            0x88..=0x8B => {
                let modrm = self.fetch8();
                let mode = modrm >> 6;
                let reg = (modrm >> 3) & 7;
                let rm = modrm & 7;
                let operand = self.decode_rm(mode, rm, segment_override);
                match opcode {
                    0x88 => {
                        let value = self.register8(reg).expect("three-bit register index");
                        self.write_operand8(operand, value);
                    }
                    0x8A => {
                        let value = self.read_operand8(operand);
                        self.set_reg8(reg, value);
                    }
                    0x89 => {
                        let value = self.reg16(reg);
                        self.write_operand16(operand, value);
                    }
                    0x8B => {
                        let value = self.read_operand16(operand);
                        self.set_reg16(reg, value);
                    }
                    _ => unreachable!(),
                }
            }
            0x8C | 0x8E => {
                let modrm = self.fetch8();
                let operand = self.decode_rm(modrm >> 6, modrm & 7, segment_override);
                let segment_index = (modrm >> 3) & 7;
                let Some(segment_value) = self.segment_register(segment_index) else {
                    return Err(CpuError::UnsupportedOpcode {
                        cs: self.registers.cs,
                        ip,
                        opcode,
                    });
                };
                if opcode == 0x8C {
                    self.write_operand16(operand, segment_value);
                } else {
                    if segment_index == 1 {
                        return Err(CpuError::UnsupportedOpcode {
                            cs: self.registers.cs,
                            ip,
                            opcode,
                        });
                    }
                    self.set_segment_register(segment_index, self.read_operand16(operand));
                }
            }
            0xA0..=0xA3 => {
                let offset = self.fetch16();
                let segment = segment_override.unwrap_or(self.registers.ds);
                match opcode {
                    0xA0 => self.set_reg8(0, self.read_byte(segment, offset)),
                    0xA1 => self.registers.ax = self.read_word(segment, offset),
                    0xA2 => self.write_byte(segment, offset, self.register8(0).unwrap()),
                    0xA3 => self.write_word(segment, offset, self.registers.ax),
                    _ => unreachable!(),
                }
            }
            0xA4..=0xA7 | 0xAA..=0xAF => {
                self.execute_string(opcode, repeat, segment_override);
            }
            0x40..=0x47 => {
                let reg = opcode - 0x40;
                let value = self.reg16(reg).wrapping_add(1);
                self.set_reg16(reg, value);
                self.set_szp16(value);
                self.registers.set_flag(AF, (value & 0x000F) == 0);
                self.registers.set_flag(OF, value == 0x8000);
            }
            0x48..=0x4F => {
                let reg = opcode - 0x48;
                let old = self.reg16(reg);
                let value = old.wrapping_sub(1);
                self.set_reg16(reg, value);
                self.set_szp16(value);
                self.registers.set_flag(AF, old & 0x000F == 0);
                self.registers.set_flag(OF, old == 0x8000);
            }
            0x04..=0x3D if opcode & 6 == 4 => {
                let operation = (opcode >> 3) & 7;
                if opcode & 1 == 0 {
                    let right = self.fetch8();
                    let result = self.alu8(operation, self.register8(0).unwrap(), right);
                    if operation != 7 {
                        self.set_reg8(0, result);
                    }
                } else {
                    let right = self.fetch16();
                    let result = self.alu16(operation, self.registers.ax, right);
                    if operation != 7 {
                        self.registers.ax = result;
                    }
                }
            }
            0xEB => {
                let displacement = self.fetch8() as i8;
                self.registers.ip = self
                    .registers
                    .ip
                    .wrapping_add_signed(i16::from(displacement));
            }
            0xE4 => {
                let port = u16::from(self.fetch8());
                self.set_reg8(0, self.read_port_byte(port));
            }
            0xE5 => {
                let port = u16::from(self.fetch8());
                self.registers.ax = self.read_port_word(port);
            }
            0xE6 => {
                let port = u16::from(self.fetch8());
                self.write_port_byte(port, self.registers.ax as u8);
            }
            0xE7 => {
                let port = u16::from(self.fetch8());
                self.write_port_word(port, self.registers.ax);
            }
            0xEC => self.set_reg8(0, self.read_port_byte(self.registers.dx)),
            0xED => self.registers.ax = self.read_port_word(self.registers.dx),
            0xEE => self.write_port_byte(self.registers.dx, self.registers.ax as u8),
            0xEF => self.write_port_word(self.registers.dx, self.registers.ax),
            0xE9 => {
                let displacement = self.fetch16() as i16;
                self.registers.ip = self.registers.ip.wrapping_add_signed(displacement);
            }
            0xE8 => {
                let displacement = self.fetch16() as i16;
                self.push16(self.registers.ip);
                self.registers.ip = self.registers.ip.wrapping_add_signed(displacement);
            }
            0xC3 => self.registers.ip = self.pop16(),
            0xC2 => {
                let adjustment = self.fetch16();
                self.registers.ip = self.pop16();
                self.registers.sp = self.registers.sp.wrapping_add(adjustment);
            }
            0xCB => {
                self.registers.ip = self.pop16();
                self.registers.cs = self.pop16();
            }
            0xCA => {
                let adjustment = self.fetch16();
                self.registers.ip = self.pop16();
                self.registers.cs = self.pop16();
                self.registers.sp = self.registers.sp.wrapping_add(adjustment);
            }
            0x70..=0x7F => {
                let displacement = self.fetch8() as i8;
                if self.condition(opcode & 0x0F) {
                    self.registers.ip = self
                        .registers
                        .ip
                        .wrapping_add_signed(i16::from(displacement));
                }
            }
            0xE0..=0xE3 => {
                let displacement = self.fetch8() as i8;
                let take = match opcode {
                    0xE0 => {
                        self.registers.cx = self.registers.cx.wrapping_sub(1);
                        self.registers.cx != 0 && !self.registers.zero()
                    }
                    0xE1 => {
                        self.registers.cx = self.registers.cx.wrapping_sub(1);
                        self.registers.cx != 0 && self.registers.zero()
                    }
                    0xE2 => {
                        self.registers.cx = self.registers.cx.wrapping_sub(1);
                        self.registers.cx != 0
                    }
                    0xE3 => self.registers.cx == 0,
                    _ => unreachable!(),
                };
                if take {
                    self.registers.ip = self
                        .registers
                        .ip
                        .wrapping_add_signed(i16::from(displacement));
                }
            }
            _ => {
                return Err(CpuError::UnsupportedOpcode {
                    cs: self.registers.cs,
                    ip,
                    opcode,
                });
            }
        }
        self.instructions += 1;
        Ok(())
    }

    pub fn run(&mut self, limit: usize) -> Result<(), CpuError> {
        for _ in 0..limit {
            if self.halted {
                return Ok(());
            }
            self.step()?;
        }
        if self.halted {
            Ok(())
        } else {
            Err(CpuError::StepLimit { limit })
        }
    }

    fn fetch8(&mut self) -> u8 {
        let value = self.read_byte(self.registers.cs, self.registers.ip);
        self.registers.ip = self.registers.ip.wrapping_add(1);
        value
    }

    fn fetch16(&mut self) -> u16 {
        let low = self.fetch8();
        let high = self.fetch8();
        u16::from_le_bytes([low, high])
    }

    fn push16(&mut self, value: u16) {
        self.registers.sp = self.registers.sp.wrapping_sub(2);
        self.write_word(self.registers.ss, self.registers.sp, value);
    }

    fn pop16(&mut self) -> u16 {
        let value = self.read_word(self.registers.ss, self.registers.sp);
        self.registers.sp = self.registers.sp.wrapping_add(2);
        value
    }

    fn enter_interrupt(&mut self, interrupt: u8, return_ip: u16) {
        self.push16(self.registers.raw_flags());
        self.push16(self.registers.cs);
        self.push16(return_ip);
        self.registers.flags &= !(IF | TF);
        let vector = u16::from(interrupt) * 4;
        self.registers.ip = self.read_word(0, vector);
        self.registers.cs = self.read_word(0, vector.wrapping_add(2));
        self.pending_interrupt = Some(interrupt);
    }

    fn condition(&self, code: u8) -> bool {
        let (carry, parity, zero, sign, overflow) = (
            self.registers.carry(),
            self.registers.parity(),
            self.registers.zero(),
            self.registers.sign(),
            self.registers.overflow(),
        );
        match code {
            0 => overflow,
            1 => !overflow,
            2 => carry,
            3 => !carry,
            4 => zero,
            5 => !zero,
            6 => carry || zero,
            7 => !carry && !zero,
            8 => sign,
            9 => !sign,
            10 => parity,
            11 => !parity,
            12 => sign != overflow,
            13 => sign == overflow,
            14 => zero || sign != overflow,
            15 => !zero && sign == overflow,
            _ => unreachable!("8086 condition field is four bits"),
        }
    }

    fn execute_string(&mut self, opcode: u8, repeat: Option<u8>, source_override: Option<u16>) {
        let word = opcode & 1 != 0;
        let width = if word { 2 } else { 1 };
        let step = if self.registers.flags & DF != 0 {
            -width
        } else {
            width
        };
        let source_segment = source_override.unwrap_or(self.registers.ds);
        let repeated = repeat.is_some();
        if repeated && self.registers.cx == 0 {
            return;
        }
        loop {
            match opcode {
                0xA4 | 0xA5 => {
                    if word {
                        let value = self.read_word(source_segment, self.registers.si);
                        self.write_word(self.registers.es, self.registers.di, value);
                    } else {
                        let value = self.read_byte(source_segment, self.registers.si);
                        self.write_byte(self.registers.es, self.registers.di, value);
                    }
                    self.registers.si = self.registers.si.wrapping_add_signed(step);
                    self.registers.di = self.registers.di.wrapping_add_signed(step);
                }
                0xA6 | 0xA7 => {
                    if word {
                        let left = self.read_word(source_segment, self.registers.si);
                        let right = self.read_word(self.registers.es, self.registers.di);
                        self.set_sub_flags(left, right, left.wrapping_sub(right), left < right);
                    } else {
                        let left = self.read_byte(source_segment, self.registers.si);
                        let right = self.read_byte(self.registers.es, self.registers.di);
                        self.set_sub_flags8(left, right, left.wrapping_sub(right), left < right);
                    }
                    self.registers.si = self.registers.si.wrapping_add_signed(step);
                    self.registers.di = self.registers.di.wrapping_add_signed(step);
                }
                0xAA | 0xAB => {
                    if word {
                        self.write_word(self.registers.es, self.registers.di, self.registers.ax);
                    } else {
                        self.write_byte(
                            self.registers.es,
                            self.registers.di,
                            self.register8(0).expect("AL register index"),
                        );
                    }
                    self.registers.di = self.registers.di.wrapping_add_signed(step);
                }
                0xAC | 0xAD => {
                    if word {
                        self.registers.ax = self.read_word(source_segment, self.registers.si);
                    } else {
                        let value = self.read_byte(source_segment, self.registers.si);
                        self.set_reg8(0, value);
                    }
                    self.registers.si = self.registers.si.wrapping_add_signed(step);
                }
                0xAE | 0xAF => {
                    if word {
                        let left = self.registers.ax;
                        let right = self.read_word(self.registers.es, self.registers.di);
                        self.set_sub_flags(left, right, left.wrapping_sub(right), left < right);
                    } else {
                        let left = self.register8(0).expect("AL register index");
                        let right = self.read_byte(self.registers.es, self.registers.di);
                        self.set_sub_flags8(left, right, left.wrapping_sub(right), left < right);
                    }
                    self.registers.di = self.registers.di.wrapping_add_signed(step);
                }
                _ => unreachable!("execute_string only receives string opcodes"),
            }
            if !repeated {
                break;
            }
            self.registers.cx = self.registers.cx.wrapping_sub(1);
            if self.registers.cx == 0 {
                break;
            }
            let compares = matches!(opcode, 0xA6 | 0xA7 | 0xAE | 0xAF);
            if compares
                && ((repeat == Some(0xF3) && !self.registers.zero())
                    || (repeat == Some(0xF2) && self.registers.zero()))
            {
                break;
            }
        }
    }

    fn segment_register(&self, index: u8) -> Option<u16> {
        match index {
            0 => Some(self.registers.es),
            1 => Some(self.registers.cs),
            2 => Some(self.registers.ss),
            3 => Some(self.registers.ds),
            _ => None,
        }
    }

    fn set_segment_register(&mut self, index: u8, value: u16) {
        match index {
            0 => self.registers.es = value,
            1 => self.registers.cs = value,
            2 => self.registers.ss = value,
            3 => self.registers.ds = value,
            _ => unreachable!("validated segment-register field"),
        }
    }

    fn decode_rm(&mut self, mode: u8, rm: u8, segment_override: Option<u16>) -> Operand {
        if mode == 3 {
            return Operand::Register(rm);
        }

        let (base, uses_bp) = match rm {
            0 => (self.registers.bx.wrapping_add(self.registers.si), false),
            1 => (self.registers.bx.wrapping_add(self.registers.di), false),
            2 => (self.registers.bp.wrapping_add(self.registers.si), true),
            3 => (self.registers.bp.wrapping_add(self.registers.di), true),
            4 => (self.registers.si, false),
            5 => (self.registers.di, false),
            6 if mode == 0 => (self.fetch16(), false),
            6 => (self.registers.bp, true),
            7 => (self.registers.bx, false),
            _ => unreachable!("8086 r/m field is three bits"),
        };
        let displacement = match mode {
            0 => 0,
            1 => i16::from(self.fetch8() as i8),
            2 => self.fetch16() as i16,
            _ => unreachable!("register mode returned above"),
        };
        let offset = base.wrapping_add_signed(displacement);
        let default_segment = if uses_bp {
            self.registers.ss
        } else {
            self.registers.ds
        };
        Operand::Memory(segment_override.unwrap_or(default_segment), offset)
    }

    fn read_operand8(&self, operand: Operand) -> u8 {
        match operand {
            Operand::Register(index) => self.register8(index).expect("three-bit register index"),
            Operand::Memory(segment, offset) => self.read_byte(segment, offset),
        }
    }

    fn write_operand8(&mut self, operand: Operand, value: u8) {
        match operand {
            Operand::Register(index) => self.set_reg8(index, value),
            Operand::Memory(segment, offset) => self.write_byte(segment, offset, value),
        }
    }

    fn read_operand16(&self, operand: Operand) -> u16 {
        match operand {
            Operand::Register(index) => self.reg16(index),
            Operand::Memory(segment, offset) => self.read_word(segment, offset),
        }
    }

    fn write_operand16(&mut self, operand: Operand, value: u16) {
        match operand {
            Operand::Register(index) => self.set_reg16(index, value),
            Operand::Memory(segment, offset) => self.write_word(segment, offset, value),
        }
    }

    fn reg16(&self, index: u8) -> u16 {
        match index {
            0 => self.registers.ax,
            1 => self.registers.cx,
            2 => self.registers.dx,
            3 => self.registers.bx,
            4 => self.registers.sp,
            5 => self.registers.bp,
            6 => self.registers.si,
            7 => self.registers.di,
            _ => unreachable!("8086 register field is three bits"),
        }
    }

    fn set_reg16(&mut self, index: u8, value: u16) {
        match index {
            0 => self.registers.ax = value,
            1 => self.registers.cx = value,
            2 => self.registers.dx = value,
            3 => self.registers.bx = value,
            4 => self.registers.sp = value,
            5 => self.registers.bp = value,
            6 => self.registers.si = value,
            7 => self.registers.di = value,
            _ => unreachable!("8086 register field is three bits"),
        }
    }

    pub fn register8(&self, index: u8) -> Option<u8> {
        match index {
            0 => Some(self.registers.ax as u8),
            1 => Some(self.registers.cx as u8),
            2 => Some(self.registers.dx as u8),
            3 => Some(self.registers.bx as u8),
            4 => Some((self.registers.ax >> 8) as u8),
            5 => Some((self.registers.cx >> 8) as u8),
            6 => Some((self.registers.dx >> 8) as u8),
            7 => Some((self.registers.bx >> 8) as u8),
            _ => None,
        }
    }

    pub fn set_reg8(&mut self, index: u8, value: u8) {
        let word = match index {
            0 => &mut self.registers.ax,
            1 => &mut self.registers.cx,
            2 => &mut self.registers.dx,
            3 => &mut self.registers.bx,
            4 => &mut self.registers.ax,
            5 => &mut self.registers.cx,
            6 => &mut self.registers.dx,
            7 => &mut self.registers.bx,
            _ => unreachable!("8086 register field is three bits"),
        };
        if index < 4 {
            *word = (*word & 0xFF00) | u16::from(value);
        } else {
            *word = (*word & 0x00FF) | (u16::from(value) << 8);
        }
    }

    fn set_szp16(&mut self, value: u16) {
        self.registers.set_flag(ZF, value == 0);
        self.registers.set_flag(SF, value & 0x8000 != 0);
        self.registers.set_flag(PF, even_parity(value as u8));
    }

    fn set_add_flags(&mut self, left: u16, right: u16, result: u16, carry: bool) {
        self.registers.set_flag(CF, carry);
        self.registers
            .set_flag(AF, (left ^ right ^ result) & 0x10 != 0);
        self.registers
            .set_flag(OF, (!(left ^ right) & (left ^ result)) & 0x8000 != 0);
        self.set_szp16(result);
    }

    fn set_sub_flags(&mut self, left: u16, right: u16, result: u16, borrow: bool) {
        self.registers.set_flag(CF, borrow);
        self.registers
            .set_flag(AF, (left ^ right ^ result) & 0x10 != 0);
        self.registers
            .set_flag(OF, ((left ^ right) & (left ^ result)) & 0x8000 != 0);
        self.set_szp16(result);
    }

    fn alu8(&mut self, operation: u8, left: u8, right: u8) -> u8 {
        let carry_in = u8::from(self.registers.carry());
        match operation {
            0 => {
                let wide = u16::from(left) + u16::from(right);
                let result = wide as u8;
                self.set_add_flags8(left, right, result, wide > u8::MAX as u16);
                result
            }
            1 => self.logic8(left | right),
            2 => {
                let wide = u16::from(left) + u16::from(right) + u16::from(carry_in);
                let result = wide as u8;
                self.set_add_flags8(left, right, result, wide > u8::MAX as u16);
                result
            }
            3 => {
                let subtrahend = u16::from(right) + u16::from(carry_in);
                let result = left.wrapping_sub(right).wrapping_sub(carry_in);
                self.set_sub_flags8(left, right, result, u16::from(left) < subtrahend);
                result
            }
            4 => self.logic8(left & right),
            5 | 7 => {
                let result = left.wrapping_sub(right);
                self.set_sub_flags8(left, right, result, left < right);
                result
            }
            6 => self.logic8(left ^ right),
            _ => unreachable!("three-bit ALU operation"),
        }
    }

    fn alu16(&mut self, operation: u8, left: u16, right: u16) -> u16 {
        let carry_in = u16::from(self.registers.carry());
        match operation {
            0 => {
                let wide = u32::from(left) + u32::from(right);
                let result = wide as u16;
                self.set_add_flags(left, right, result, wide > u16::MAX as u32);
                result
            }
            1 => self.logic16(left | right),
            2 => {
                let wide = u32::from(left) + u32::from(right) + u32::from(carry_in);
                let result = wide as u16;
                self.set_add_flags(left, right, result, wide > u16::MAX as u32);
                result
            }
            3 => {
                let subtrahend = u32::from(right) + u32::from(carry_in);
                let result = left.wrapping_sub(right).wrapping_sub(carry_in);
                self.set_sub_flags(left, right, result, u32::from(left) < subtrahend);
                result
            }
            4 => self.logic16(left & right),
            5 | 7 => {
                let result = left.wrapping_sub(right);
                self.set_sub_flags(left, right, result, left < right);
                result
            }
            6 => self.logic16(left ^ right),
            _ => unreachable!("three-bit ALU operation"),
        }
    }

    fn logic8(&mut self, value: u8) -> u8 {
        self.registers.set_flag(CF, false);
        self.registers.set_flag(OF, false);
        self.registers.set_flag(AF, false);
        self.set_szp8(value);
        value
    }

    fn logic16(&mut self, value: u16) -> u16 {
        self.registers.set_flag(CF, false);
        self.registers.set_flag(OF, false);
        self.registers.set_flag(AF, false);
        self.set_szp16(value);
        value
    }

    fn set_szp8(&mut self, value: u8) {
        self.registers.set_flag(ZF, value == 0);
        self.registers.set_flag(SF, value & 0x80 != 0);
        self.registers.set_flag(PF, even_parity(value));
    }

    fn set_add_flags8(&mut self, left: u8, right: u8, result: u8, carry: bool) {
        self.registers.set_flag(CF, carry);
        self.registers
            .set_flag(AF, (left ^ right ^ result) & 0x10 != 0);
        self.registers
            .set_flag(OF, (!(left ^ right) & (left ^ result)) & 0x80 != 0);
        self.set_szp8(result);
    }

    fn set_sub_flags8(&mut self, left: u8, right: u8, result: u8, borrow: bool) {
        self.registers.set_flag(CF, borrow);
        self.registers
            .set_flag(AF, (left ^ right ^ result) & 0x10 != 0);
        self.registers
            .set_flag(OF, ((left ^ right) & (left ^ result)) & 0x80 != 0);
        self.set_szp8(result);
    }

    fn shift8(&mut self, operation: u8, mut value: u8, count: u8) -> u8 {
        if count == 0 {
            return value;
        }
        let original = value;
        for _ in 0..count {
            let old_carry = self.registers.carry();
            let carry = match operation {
                0 | 2 | 4 | 6 => value & 0x80 != 0,
                1 | 3 | 5 | 7 => value & 1 != 0,
                _ => unreachable!("three-bit shift operation"),
            };
            value = match operation {
                0 => value.rotate_left(1),
                1 => value.rotate_right(1),
                2 => (value << 1) | u8::from(old_carry),
                3 => (value >> 1) | (u8::from(old_carry) << 7),
                4 | 6 => value << 1,
                5 => value >> 1,
                7 => ((value as i8) >> 1) as u8,
                _ => unreachable!("three-bit shift operation"),
            };
            self.registers.set_flag(CF, carry);
        }
        if operation >= 4 {
            self.set_szp8(value);
        }
        if count == 1 {
            let msb = value & 0x80 != 0;
            let next_msb = value & 0x40 != 0;
            let overflow = match operation {
                0 | 2 | 4 | 6 | 1 | 3 => msb ^ self.registers.carry(),
                5 => original & 0x80 != 0,
                7 => false,
                _ => unreachable!("three-bit shift operation"),
            };
            self.registers.set_flag(
                OF,
                if matches!(operation, 1 | 3) {
                    msb ^ next_msb
                } else {
                    overflow
                },
            );
        }
        value
    }

    fn shift16(&mut self, operation: u8, mut value: u16, count: u8) -> u16 {
        if count == 0 {
            return value;
        }
        let original = value;
        for _ in 0..count {
            let old_carry = self.registers.carry();
            let carry = match operation {
                0 | 2 | 4 | 6 => value & 0x8000 != 0,
                1 | 3 | 5 | 7 => value & 1 != 0,
                _ => unreachable!("three-bit shift operation"),
            };
            value = match operation {
                0 => value.rotate_left(1),
                1 => value.rotate_right(1),
                2 => (value << 1) | u16::from(old_carry),
                3 => (value >> 1) | (u16::from(old_carry) << 15),
                4 | 6 => value << 1,
                5 => value >> 1,
                7 => ((value as i16) >> 1) as u16,
                _ => unreachable!("three-bit shift operation"),
            };
            self.registers.set_flag(CF, carry);
        }
        if operation >= 4 {
            self.set_szp16(value);
        }
        if count == 1 {
            let msb = value & 0x8000 != 0;
            let next_msb = value & 0x4000 != 0;
            let overflow = match operation {
                0 | 2 | 4 | 6 | 1 | 3 => msb ^ self.registers.carry(),
                5 => original & 0x8000 != 0,
                7 => false,
                _ => unreachable!("three-bit shift operation"),
            };
            self.registers.set_flag(
                OF,
                if matches!(operation, 1 | 3) {
                    msb ^ next_msb
                } else {
                    overflow
                },
            );
        }
        value
    }
}

fn physical(segment: u16, offset: u16) -> usize {
    ((usize::from(segment) << 4) + usize::from(offset)) & (MEMORY_SIZE - 1)
}

fn even_parity(value: u8) -> bool {
    value.count_ones().is_multiple_of(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediate_moves_and_halt_execute_in_real_mode() {
        let mut cpu = Cpu8086::new();
        cpu.load(0x1000, 0, &[0xB8, 0x34, 0x12, 0xB7, 0xA5, 0xF4]);
        cpu.run(3).unwrap();
        assert_eq!(cpu.registers.ax, 0x1234);
        assert_eq!(cpu.registers.bx, 0xA500);
        assert_eq!(cpu.register8(7), Some(0xA5));
        assert_eq!(cpu.register8(8), None);
        assert!(cpu.is_halted());
        assert_eq!(cpu.instructions(), 3);
    }

    #[test]
    fn fninit_resets_8087_control_status_and_register_tags() {
        let mut cpu = Cpu8086::new();
        cpu.fpu = Fpu8087 {
            control_word: 0,
            status_word: 0xFFFF,
            tag_word: 0,
            ..Fpu8087::default()
        };
        cpu.load(0, 0, &[0xDB, 0xE3, 0xF4]);
        cpu.step().unwrap();
        assert_eq!(cpu.fpu, Fpu8087::default());
        assert_eq!(cpu.instructions(), 1);
    }

    #[test]
    fn fld1_pushes_one_onto_the_8087_register_stack() {
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xD9, 0xE8, 0xF4]);
        cpu.step().unwrap();
        assert_eq!(cpu.fpu.st(0), Some(1.0));
        assert_eq!(cpu.fpu.st(1), None);
        assert_eq!(cpu.fpu.top, 7);
        assert_eq!((cpu.fpu.status_word >> 11) & 7, 7);
        assert_eq!((cpu.fpu.tag_word >> 14) & 3, 0);
    }

    #[test]
    fn fwait_advances_past_idle_8087_without_mutating_state() {
        let mut cpu = Cpu8086::new();
        cpu.registers.flags = CF | ZF;
        let before = cpu.fpu;
        cpu.load(0, 0, &[0x9B, 0xF4]);
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ip, 1);
        assert_eq!(cpu.registers.raw_flags(), CF | ZF | 2);
        assert_eq!(cpu.fpu, before);
    }

    #[test]
    fn fld_m64real_pushes_the_exact_little_endian_double() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ds = 0x1000;
        cpu.registers.si = 0x2000;
        let value = 12.375_f64;
        for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
            cpu.write_byte(0x1000, 0x2000 + index as u16, byte);
        }
        cpu.load(0, 0, &[0xDD, 0x04]); // FLD qword [SI]
        cpu.step().unwrap();
        assert_eq!(cpu.fpu.st(0), Some(value));
        assert_eq!(cpu.registers.ip, 2);
    }

    #[test]
    fn fstp_m64real_stores_to_bp_default_stack_segment_and_pops() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x2000;
        cpu.registers.ds = 0x3000;
        cpu.registers.bp = 0x0100;
        cpu.load(0, 0, &[0xD9, 0xE8, 0xDD, 0x5E, 0x04]); // FLD1; FSTP qword [BP+4]
        cpu.step().unwrap();
        cpu.step().unwrap();
        let bytes = std::array::from_fn(|index| cpu.read_byte(0x2000, 0x0104 + index as u16));
        assert_eq!(bytes, 1.0_f64.to_le_bytes());
        assert_eq!(cpu.fpu.st(0), None);
        assert_eq!(cpu.read_word(0x3000, 0x0104), 0);
    }

    #[test]
    fn fistp_m16int_stores_the_rounded_value_and_pops_st0() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ds = 0x1000;
        cpu.load(0, 0, &[0xD9, 0xE8, 0xDF, 0x1E, 0x00, 0x20, 0xF4]);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.read_word(0x1000, 0x2000), 1);
        assert_eq!(cpu.fpu.st(0), None);
        assert_eq!(cpu.fpu.top, 0);
    }

    #[test]
    fn indirect_near_jump_uses_the_selected_register_target() {
        let mut cpu = Cpu8086::new();
        cpu.registers.si = 5;
        cpu.load(0, 0, &[0xFF, 0xE6, 0xF4, 0xF4, 0xF4, 0xF4, 0xF4, 0xF4]);
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ip, 5);
        cpu.step().unwrap();
        assert!(cpu.is_halted());
    }

    #[test]
    fn lea_computes_the_effective_offset_without_reading_memory() {
        let mut cpu = Cpu8086::new();
        cpu.registers.bx = 0x1000;
        cpu.registers.si = 0x0020;
        cpu.load(0, 0, &[0x8D, 0x50, 0x05]); // LEA DX,[BX+SI+5]
        cpu.step().unwrap();
        assert_eq!(cpu.registers.dx, 0x1025);
    }

    #[test]
    fn cbw_and_cwd_sign_extend_accumulator_widths() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ax = 0x0080;
        cpu.load(0, 0, &[0x98, 0x99]);
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ax, 0xFF80);
        cpu.step().unwrap();
        assert_eq!(cpu.registers.dx, 0xFFFF);
    }

    #[test]
    fn accumulator_exchange_opcodes_swap_words_without_changing_flags() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ax = 0x1234;
        cpu.registers.bx = 0xABCD;
        cpu.registers.di = 0x9876;
        cpu.registers.flags = CF | ZF;
        cpu.load(0, 0, &[0x93, 0x97, 0x90]); // XCHG AX,BX; XCHG AX,DI; NOP
        cpu.step().unwrap();
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ax, 0x9876);
        assert_eq!(cpu.registers.bx, 0x1234);
        assert_eq!(cpu.registers.di, 0xABCD);
        assert_eq!(cpu.registers.raw_flags(), CF | ZF | 2);
    }

    #[test]
    fn byte_increment_and_decrement_preserve_carry() {
        let mut cpu = Cpu8086::new();
        cpu.registers.flags = CF;
        cpu.write_byte(0, 0x20, 0xFF);
        cpu.write_byte(0, 0x21, 0);
        cpu.load(0, 0, &[0xFE, 0x06, 0x20, 0, 0xFE, 0x0E, 0x21, 0]);
        cpu.step().unwrap();
        assert_eq!(cpu.read_byte(0, 0x20), 0);
        assert!(cpu.registers.carry());
        assert!(cpu.registers.zero());
        cpu.step().unwrap();
        assert_eq!(cpu.read_byte(0, 0x21), 0xFF);
        assert!(cpu.registers.carry());
        assert!(cpu.registers.sign());
    }

    #[test]
    fn register_memory_exchange_swaps_both_widths_without_touching_flags() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ax = 0x12AB;
        cpu.registers.cx = 0x3456;
        cpu.registers.flags = CF | ZF;
        cpu.write_byte(0, 0x20, 0xCD);
        cpu.write_word(0, 0x22, 0x789A);
        cpu.load(0, 0, &[0x86, 0x06, 0x20, 0, 0x87, 0x0E, 0x22, 0]);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ax, 0x12CD);
        assert_eq!(cpu.read_byte(0, 0x20), 0xAB);
        assert_eq!(cpu.registers.cx, 0x789A);
        assert_eq!(cpu.read_word(0, 0x22), 0x3456);
        assert_eq!(cpu.registers.raw_flags(), CF | ZF | 2);
    }

    #[test]
    fn jcxz_and_loop_family_apply_count_and_zero_conditions() {
        let mut cpu = Cpu8086::new();
        cpu.registers.cx = 2;
        cpu.load(0, 0, &[0xE2, 0xFE, 0xE3, 0x02, 0xB8, 1, 0, 0xB8, 2, 0]);
        cpu.step().unwrap();
        assert_eq!(cpu.registers.cx, 1);
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ip, 2);
        cpu.registers.cx = 0;
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ip, 6);
    }

    #[test]
    fn aam_and_aad_use_the_encoded_radix_and_update_szp() {
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xB0, 37, 0xD4, 10, 0xD5, 10, 0xF4]);
        cpu.run(4).unwrap();
        assert_eq!(cpu.register8(4), Some(0));
        assert_eq!(cpu.register8(0), Some(37));
        assert!(!cpu.registers.zero());
        assert_eq!(cpu.instructions(), 4);
    }

    #[test]
    fn arithmetic_sets_wrapping_result_and_8086_status_flags() {
        // MOV AX, FFFFh; ADD AX, 1; HLT
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xB8, 0xFF, 0xFF, 0x05, 1, 0, 0xF4]);
        cpu.run(3).unwrap();
        assert_eq!(cpu.registers.ax, 0);
        assert!(cpu.registers.carry());
        assert!(cpu.registers.zero());
        assert!(cpu.registers.parity());
        assert!(cpu.registers.auxiliary_carry());
        assert!(!cpu.registers.sign());
        assert!(!cpu.registers.overflow());
    }

    #[test]
    fn compare_and_conditional_branch_skip_the_taken_path() {
        // MOV AX, 7; CMP AX, 7; JZ +3; MOV BX, 1; HLT
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xB8, 7, 0, 0x3D, 7, 0, 0x74, 3, 0xBB, 1, 0, 0xF4]);
        cpu.run(5).unwrap();
        assert_eq!(cpu.registers.ax, 7);
        assert_eq!(cpu.registers.bx, 0);
        assert!(cpu.registers.zero());
        assert!(cpu.is_halted());
    }

    #[test]
    fn segment_addressing_wraps_at_the_one_megabyte_bus() {
        let mut cpu = Cpu8086::new();
        cpu.write_byte(0xFFFF, 0x0010, 0xA5);
        assert_eq!(cpu.read_byte(0, 0), 0xA5);
    }

    #[test]
    fn word_access_wraps_the_segment_offset_little_endian() {
        let mut cpu = Cpu8086::new();
        cpu.write_word(0x1000, 0xFFFF, 0xBEEF);
        assert_eq!(cpu.read_byte(0x1000, 0xFFFF), 0xEF);
        assert_eq!(cpu.read_byte(0x1000, 0), 0xBE);
        assert_eq!(cpu.read_word(0x1000, 0xFFFF), 0xBEEF);
    }

    #[test]
    fn unsupported_instruction_reports_its_original_address() {
        let mut cpu = Cpu8086::new();
        cpu.load(0x1234, 0x5678, &[0xCC]);
        assert_eq!(
            cpu.step(),
            Err(CpuError::UnsupportedOpcode {
                cs: 0x1234,
                ip: 0x5678,
                opcode: 0xCC,
            })
        );
    }

    #[test]
    fn relative_jump_wraps_the_instruction_pointer() {
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xEB, 0xFC]);
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ip, 0xFFFE);
    }

    #[test]
    fn modrm_moves_use_8086_base_index_addressing_and_default_ds() {
        // MOV [BX+SI], AX; MOV CX, [BX+SI]; HLT
        let mut cpu = Cpu8086::new();
        cpu.registers.ds = 0x2000;
        cpu.registers.bx = 0x0100;
        cpu.registers.si = 0x0004;
        cpu.registers.ax = 0xBEEF;
        cpu.load(0, 0, &[0x89, 0x00, 0x8B, 0x08, 0xF4]);
        cpu.run(3).unwrap();
        assert_eq!(cpu.read_word(0x2000, 0x0104), 0xBEEF);
        assert_eq!(cpu.registers.cx, 0xBEEF);
    }

    #[test]
    fn bp_addressing_defaults_to_ss_and_segment_override_selects_es() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ds = 0x1000;
        cpu.registers.es = 0x2000;
        cpu.registers.ss = 0x3000;
        cpu.registers.bp = 0x0010;
        cpu.registers.si = 0x0002;
        cpu.registers.bx = 0x0020;
        cpu.registers.ax = 0x1234;
        // MOV [BP+SI+2], AX; MOV DX, ES:[BX]; HLT
        cpu.load(0, 0, &[0x89, 0x42, 2, 0x26, 0x8B, 0x17, 0xF4]);
        cpu.write_word(0x2000, 0x0020, 0xCAFE);
        cpu.run(3).unwrap();
        assert_eq!(cpu.read_word(0x3000, 0x0014), 0x1234);
        assert_eq!(cpu.registers.dx, 0xCAFE);
        assert_eq!(cpu.read_word(0x1000, 0x0014), 0);
    }

    #[test]
    fn modrm_alu_uses_signed_overflow_and_writes_to_the_selected_operand() {
        // MOV AL, 7Fh; MOV BL, 1; ADD AL, BL; HLT
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xB0, 0x7F, 0xB3, 1, 0x00, 0xD8, 0xF4]);
        cpu.run(4).unwrap();
        assert_eq!(cpu.register8(0), Some(0x80));
        assert!(cpu.registers.sign());
        assert!(cpu.registers.overflow());
        assert!(!cpu.registers.carry());
    }

    #[test]
    fn immediate_group_sign_extends_and_cmp_does_not_store() {
        // MOV AX, 1000h; SUB AX, -1 (83 /5); CMP AX, 1001h; HLT
        let mut cpu = Cpu8086::new();
        cpu.load(
            0,
            0,
            &[0xB8, 0, 0x10, 0x83, 0xE8, 0xFF, 0x3D, 1, 0x10, 0xF4],
        );
        cpu.run(4).unwrap();
        assert_eq!(cpu.registers.ax, 0x1001);
        assert!(cpu.registers.zero());
        assert!(!cpu.registers.carry());
    }

    #[test]
    fn daa_adjusts_both_bcd_digits_and_updates_carry_and_auxiliary_carry() {
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0x27, 0xF4]); // DAA; HLT
        cpu.registers.ax = 0x009A;
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ax as u8, 0x00);
        assert!(cpu.registers.carry());
        assert!(cpu.registers.auxiliary_carry());
        assert!(cpu.registers.zero());

        cpu.registers.ip = 0;
        cpu.registers.ax = 0x0015;
        cpu.registers.flags = 2;
        cpu.step().unwrap();
        assert_eq!(cpu.registers.ax as u8, 0x15);
        assert!(!cpu.registers.carry());
        assert!(!cpu.registers.auxiliary_carry());
    }

    #[test]
    fn register_stack_operations_use_ss_and_restore_sp() {
        // MOV AX, 1234h; PUSH AX; MOV AX, 0; POP BX; HLT
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x1000;
        cpu.registers.sp = 0x0100;
        cpu.load(0, 0, &[0xB8, 0x34, 0x12, 0x50, 0xB8, 0, 0, 0x5B, 0xF4]);
        cpu.run(5).unwrap();
        assert_eq!(cpu.registers.ax, 0);
        assert_eq!(cpu.registers.bx, 0x1234);
        assert_eq!(cpu.registers.sp, 0x0100);
        assert_eq!(cpu.read_word(0x1000, 0x00FE), 0x1234);
    }

    #[test]
    fn pop_rm16_loads_register_and_memory_operands() {
        let mut register = Cpu8086::new();
        register.registers.ss = 0x1000;
        register.registers.sp = 0x00FE;
        register.write_word(0x1000, 0x00FE, 0x1234);
        register.load(0, 0, &[0x8F, 0xC3, 0xF4]); // POP BX
        register.run(2).unwrap();
        assert_eq!(register.registers.bx, 0x1234);
        assert_eq!(register.registers.sp, 0x0100);

        let mut memory = Cpu8086::new();
        memory.registers.ss = 0x1000;
        memory.registers.sp = 0x00FE;
        memory.write_word(0x1000, 0x00FE, 0x5678);
        memory.load(0, 0, &[0x8F, 0x46, 0x00, 0xF4]); // POP [BP]
        memory.registers.bp = 0x0200;
        memory.run(2).unwrap();
        assert_eq!(memory.read_word(0x1000, 0x0200), 0x5678);
        assert_eq!(memory.registers.sp, 0x0100);
    }

    #[test]
    fn near_call_pushes_the_return_ip_and_ret_pops_it() {
        // CALL 0006h; HLT; NOP; NOP; MOV AX, 42h; RET
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x1000;
        cpu.registers.sp = 0x0100;
        cpu.load(0, 0, &[0xE8, 3, 0, 0xF4, 0x90, 0x90, 0xB8, 0x42, 0, 0xC3]);
        cpu.run(4).unwrap();
        assert_eq!(cpu.registers.ax, 0x42);
        assert_eq!(cpu.registers.ip, 4);
        assert_eq!(cpu.registers.sp, 0x0100);
        assert!(cpu.is_halted());
    }

    #[test]
    fn far_return_restores_ip_cs_and_optional_stack_cleanup() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x1000;
        cpu.registers.sp = 0x00FC;
        cpu.write_word(0x1000, 0x00FC, 0x1234);
        cpu.write_word(0x1000, 0x00FE, 0x2000);
        cpu.load(0, 0, &[0xCA, 0x04, 0x00]); // RETF 4
        cpu.step().unwrap();
        assert_eq!((cpu.registers.cs, cpu.registers.ip), (0x2000, 0x1234));
        assert_eq!(cpu.registers.sp, 0x0104);
    }

    #[test]
    fn signed_condition_branch_uses_sign_and_overflow_flags() {
        // MOV AX, 8000h; CMP AX, 1; JL +3; MOV BX, 1; HLT
        let mut cpu = Cpu8086::new();
        cpu.load(
            0,
            0,
            &[0xB8, 0, 0x80, 0x3D, 1, 0, 0x7C, 3, 0xBB, 1, 0, 0xF4],
        );
        cpu.run(5).unwrap();
        assert_eq!(cpu.registers.bx, 0);
        assert!(cpu.is_halted());
    }

    #[test]
    fn segment_register_and_moffs_moves_address_absolute_memory() {
        // MOV DS, AX; MOV AX, [1234h]; MOV [1236h], AX; HLT
        let mut cpu = Cpu8086::new();
        cpu.registers.ax = 0x1000;
        cpu.load(
            0,
            0,
            &[0x8E, 0xD8, 0xA1, 0x34, 0x12, 0xA3, 0x36, 0x12, 0xF4],
        );
        cpu.write_word(0x1000, 0x1234, 0xABCD);
        cpu.run(4).unwrap();
        assert_eq!(cpu.registers.ds, 0x1000);
        assert_eq!(cpu.registers.ax, 0xABCD);
        assert_eq!(cpu.read_word(0x1000, 0x1236), 0xABCD);
    }

    #[test]
    fn immediate_mov_stores_bytes_and_words_through_modrm() {
        // MOV word [0020h], BEEFh; MOV byte [0030h], 5Ah; HLT
        let mut cpu = Cpu8086::new();
        cpu.registers.ds = 0x1000;
        cpu.load(
            0,
            0,
            &[
                0xC7, 0x06, 0x20, 0, 0xEF, 0xBE, 0xC6, 0x06, 0x30, 0, 0x5A, 0xF4,
            ],
        );
        cpu.run(3).unwrap();
        assert_eq!(cpu.read_word(0x1000, 0x0020), 0xBEEF);
        assert_eq!(cpu.read_byte(0x1000, 0x0030), 0x5A);
    }

    #[test]
    fn accumulator_immediates_cover_adc_and_logic_flags() {
        // MOV AL, FFh; ADD AL, 1; ADC AL, 0; XOR AL, 1; HLT
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xB0, 0xFF, 0x04, 1, 0x14, 0, 0x34, 1, 0xF4]);
        cpu.run(5).unwrap();
        assert_eq!(cpu.register8(0), Some(0));
        assert!(cpu.registers.zero());
        assert!(cpu.registers.parity());
        assert!(!cpu.registers.carry());
        assert!(!cpu.registers.overflow());
    }

    #[test]
    fn shift_group_updates_carry_overflow_and_preserves_flags_for_zero_count() {
        // MOV AL, 81h; SHL AL, 1; MOV CL, 0; CMP AL, 2; SHR AL, CL; HLT
        let mut cpu = Cpu8086::new();
        cpu.load(
            0,
            0,
            &[0xB0, 0x81, 0xD0, 0xE0, 0xB1, 0, 0x3C, 2, 0xD2, 0xE8, 0xF4],
        );
        cpu.step().unwrap(); // MOV
        cpu.step().unwrap(); // SHL
        assert_eq!(cpu.register8(0), Some(2));
        assert!(cpu.registers.carry());
        assert!(cpu.registers.overflow());
        cpu.run(4).unwrap();
        assert_eq!(cpu.register8(0), Some(2));
        assert!(cpu.registers.zero());
        assert!(!cpu.registers.carry());
        assert!(!cpu.registers.overflow());
    }

    #[test]
    fn rep_movsb_obeys_direction_flag_and_consumes_cx() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ds = 0x1000;
        cpu.registers.es = 0x2000;
        cpu.registers.si = 0x10;
        cpu.registers.di = 0x20;
        cpu.registers.cx = 3;
        cpu.write_byte(0x1000, 0x10, b'A');
        cpu.write_byte(0x1000, 0x0F, b'B');
        cpu.write_byte(0x1000, 0x0E, b'C');
        cpu.load(0, 0, &[0xFD, 0xF3, 0xA4, 0xF4]); // STD; REP MOVSB; HLT
        cpu.run(3).unwrap();
        assert_eq!(cpu.read_byte(0x2000, 0x20), b'A');
        assert_eq!(cpu.read_byte(0x2000, 0x1F), b'B');
        assert_eq!(cpu.read_byte(0x2000, 0x1E), b'C');
        assert_eq!(
            (cpu.registers.si, cpu.registers.di, cpu.registers.cx),
            (0x0D, 0x1D, 0)
        );
    }

    #[test]
    fn repe_cmpsb_stops_at_the_first_mismatch() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ds = 0x1000;
        cpu.registers.es = 0x2000;
        cpu.registers.si = 0x10;
        cpu.registers.di = 0x20;
        cpu.registers.cx = 3;
        for (offset, value) in [(0x10, 1), (0x11, 2), (0x12, 3)] {
            cpu.write_byte(0x1000, offset, value);
        }
        for (offset, value) in [(0x20, 1), (0x21, 9), (0x22, 3)] {
            cpu.write_byte(0x2000, offset, value);
        }
        cpu.load(0, 0, &[0xF3, 0xA6, 0xF4]); // REPE CMPSB; HLT
        cpu.step().unwrap();
        assert_eq!(
            (cpu.registers.si, cpu.registers.di, cpu.registers.cx),
            (0x12, 0x22, 1)
        );
        assert!(!cpu.registers.zero());
        assert!(cpu.registers.carry());
    }

    #[test]
    fn group3_neg_mul_and_div_have_8086_register_results() {
        let mut neg = Cpu8086::new();
        neg.load(0, 0, &[0xB8, 0, 0x80, 0xF7, 0xD8, 0xF4]); // NEG AX
        neg.run(3).unwrap();
        assert_eq!(neg.registers.ax, 0x8000);
        assert!(neg.registers.carry());
        assert!(neg.registers.overflow());

        let mut mul = Cpu8086::new();
        mul.load(0, 0, &[0xB0, 0x10, 0xB3, 0x10, 0xF6, 0xE3, 0xF4]); // MUL BL
        mul.run(4).unwrap();
        assert_eq!(mul.registers.ax, 0x0100);
        assert!(mul.registers.carry() && mul.registers.overflow());

        let mut div = Cpu8086::new();
        div.load(0, 0, &[0xB8, 100, 0, 0xB3, 10, 0xF6, 0xF3, 0xF4]); // DIV BL
        div.run(4).unwrap();
        assert_eq!(div.registers.ax, 10);
    }

    #[test]
    fn divide_overflow_enters_interrupt_zero_with_faulting_ip() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x3000;
        cpu.registers.sp = 0x0100;
        cpu.write_word(0, 0, 0x0100);
        cpu.write_word(0, 2, 0x2000);
        cpu.write_byte(0x2000, 0x0100, 0xCF);
        cpu.load(0x1000, 0, &[0xB8, 0, 0x10, 0xB3, 1, 0xF6, 0xF3]); // DIV BL overflows AL
        cpu.step().unwrap();
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.take_interrupt(), Some(0));
        assert_eq!((cpu.registers.cs, cpu.registers.ip), (0x2000, 0x0100));
        assert_eq!(cpu.read_word(0x3000, 0x00FA), 5);
    }

    #[test]
    fn in_out_instructions_transfer_bytes_and_words_without_changing_flags() {
        let mut cpu = Cpu8086::new();
        cpu.load(
            0,
            0,
            &[
                0xB8, 0x34, 0x12, // MOV AX,1234h
                0xBA, 0xFE, 0xFF, // MOV DX,FFFEh
                0xEF, // OUT DX,AX
                0xB8, 0, 0,    // MOV AX,0
                0xED, // IN AX,DX
                0xE6, 0x20, // OUT 20h,AL
                0xB0, 0, // MOV AL,0
                0xE4, 0x20, // IN AL,20h
                0xBA, 0xD8, 0x03, // MOV DX,03D8h
                0xEE, // OUT DX,AL
                0xB0, 0,    // MOV AL,0
                0xEC, // IN AL,DX
                0xE7, 0x30, // OUT 30h,AX
                0xB8, 0, 0, // MOV AX,0
                0xE5, 0x30, // IN AX,30h
                0xF4,
            ],
        );
        cpu.run(20).unwrap();
        assert_eq!(cpu.read_port_byte(0xFFFE), 0x34);
        assert_eq!(cpu.read_port_byte(0xFFFF), 0x12);
        assert_eq!(cpu.read_port_byte(0x20), 0x34);
        assert_eq!(cpu.read_port_byte(0x03D8), 0x34);
        assert_eq!(cpu.registers.ax, 0x1234);
        assert_eq!(cpu.read_port_byte(0x30), 0x34);
        assert_eq!(cpu.read_port_byte(0x31), 0x12);
        assert_eq!(cpu.registers.raw_flags(), 2);
    }

    #[test]
    fn test_instructions_set_logic_flags_without_writing_operands() {
        let mut cpu = Cpu8086::new();
        cpu.load(
            0,
            0,
            &[
                0xB0, 0xF0, // MOV AL,F0h
                0xB3, 0x0F, // MOV BL,0Fh
                0xF9, // STC
                0x84, 0xD8, // TEST AL,BL
                0xA8, 0xF0, // TEST AL,F0h
                0xB8, 0xF0, 0, // MOV AX,00F0h
                0xBB, 0x0F, 0x0F, // MOV BX,0F0Fh
                0x85, 0xD8, // TEST AX,BX
                0xA9, 0xFF, 0, // TEST AX,00FFh
                0xF4,
            ],
        );
        for _ in 0..4 {
            cpu.step().unwrap();
        }
        assert_eq!(cpu.register8(0), Some(0xF0));
        assert_eq!(cpu.register8(3), Some(0x0F));
        assert!(cpu.registers.zero());
        assert!(!cpu.registers.carry());
        cpu.step().unwrap();
        assert!(!cpu.registers.zero());
        assert!(cpu.registers.sign());
        for _ in 0..3 {
            cpu.step().unwrap();
        }
        assert!(cpu.registers.zero());
        assert_eq!(cpu.registers.ax, 0x00F0);
        assert_eq!(cpu.registers.bx, 0x0F0F);
        cpu.step().unwrap();
        assert!(!cpu.registers.zero());
        assert_eq!(cpu.registers.ax, 0x00F0);
    }

    #[test]
    fn fild_m16int_sign_extends_before_pushing_to_8087_stack() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x3000;
        cpu.registers.bp = 0x0100;
        cpu.write_word(0x3000, 0x010A, 0xFFF6); // -10
        cpu.load(0, 0, &[0xDF, 0x46, 0x0A]); // FILD word [BP+0Ah]
        cpu.step().unwrap();
        assert_eq!(cpu.fpu.st(0), Some(-10.0));
        assert_eq!(cpu.registers.ip, 3);
    }

    #[test]
    fn fmul_register_multiplies_st0_by_selected_stack_register() {
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xD8, 0xC9]); // FMUL ST(0), ST(1)
        cpu.fpu.push(2.5);
        cpu.fpu.push(-4.0);
        cpu.step().unwrap();
        assert_eq!(cpu.fpu.st(0), Some(-10.0));
        assert_eq!(cpu.fpu.st(1), Some(2.5));
        assert_eq!(cpu.registers.ip, 2);
    }

    #[test]
    fn fpu_retains_bits_beyond_f64_until_a_memory_store() {
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xD8, 0xC1, 0xD8, 0xC1, 0xDD, 0x1E, 0x00, 0x20]);
        cpu.fpu.push(10_000_000_000_000_000.0);
        cpu.fpu.push(1.0);
        cpu.step().unwrap(); // FADD ST(0),ST(1): 10^16 + 1
        cpu.fpu.push(-10_000_000_000_000_000.0);
        cpu.step().unwrap(); // FADD ST(0),ST(1): the retained unit survives
        cpu.step().unwrap(); // FSTP qword [2000h]
        let bytes = std::array::from_fn(|index| cpu.read_byte(0, 0x2000 + index as u16));
        assert_eq!(f64::from_le_bytes(bytes), 1.0);
    }

    #[test]
    fn default_8087_precision_rounds_a_64_bit_significand() {
        let base = FpuValue::from_f64(2f64.powi(64));
        let one = FpuValue::from_f64(1.0);
        let three = FpuValue::from_f64(3.0);
        let tied = base.add(one).round_to_8087_precision(0x037f);
        assert_eq!(tied.sub(base).round_to_f64(), 0.0);
        let above = base.add(three).round_to_8087_precision(0x037f);
        assert_eq!(above.sub(base).round_to_f64(), 4.0);
        assert_eq!(
            base.add(one)
                .round_to_8087_precision(0x027f)
                .sub(base)
                .round_to_f64(),
            0.0
        );
    }

    #[test]
    fn fdiv_register_uses_st0_as_dividend() {
        let mut cpu = Cpu8086::new();
        cpu.load(0, 0, &[0xD8, 0xF1]); // FDIV ST(0), ST(1)
        cpu.fpu.push(2.0);
        cpu.fpu.push(10.0);
        cpu.step().unwrap();
        assert_eq!(cpu.fpu.st(0), Some(5.0));
        assert_eq!(cpu.fpu.st(1), Some(2.0));
    }

    #[test]
    fn les_and_lds_load_far_pointers_from_memory() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x3000;
        cpu.registers.bp = 0x0100;
        cpu.write_word(0x3000, 0x0104, 0x1234);
        cpu.write_word(0x3000, 0x0106, 0x5678);
        cpu.write_word(0x3000, 0x0108, 0xABCD);
        cpu.write_word(0x3000, 0x010A, 0x2468);
        cpu.load(0, 0, &[0xC4, 0x76, 0x04, 0xC5, 0x5E, 0x08]);
        cpu.step().unwrap(); // LES SI,[BP+4]
        assert_eq!(cpu.registers.si, 0x1234);
        assert_eq!(cpu.registers.es, 0x5678);
        cpu.step().unwrap(); // LDS BX,[BP+8]
        assert_eq!(cpu.registers.bx, 0xABCD);
        assert_eq!(cpu.registers.ds, 0x2468);
        assert_eq!(cpu.registers.ip, 6);
    }

    #[test]
    fn fldcw_controls_fistp_m32int_rounding() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x3000;
        cpu.registers.bp = 0x0100;
        cpu.write_word(0x3000, 0x00FC, 0x0F7F); // truncate toward zero
        cpu.load(0, 0, &[0xD9, 0x6E, 0xFC, 0xDB, 0x5E, 0xFE]);
        cpu.fpu.push(-12.75);
        cpu.step().unwrap();
        assert_eq!(cpu.fpu.control_word, 0x0F7F);
        cpu.step().unwrap();
        assert_eq!(cpu.read_word(0x3000, 0x00FE), (-12i32) as u16);
        assert_eq!(cpu.read_word(0x3000, 0x0100), 0xFFFF);
        assert_eq!(cpu.fpu.st(0), None);
    }

    #[test]
    fn fcompp_status_word_can_drive_carry_via_sahf() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x3000;
        cpu.registers.bp = 0x0100;
        cpu.load(
            0,
            0,
            &[
                0xDE, 0xD9, // FCOMPP
                0xDD, 0x7E, 0x04, // FNSTSW [BP+4]
                0x8A, 0x66, 0x05, // MOV AH,[BP+5]
                0x9E, // SAHF
                0x9F, // LAHF
            ],
        );
        cpu.fpu.push(3.0);
        cpu.fpu.push(2.0);
        for _ in 0..5 {
            cpu.step().unwrap();
        }
        assert_eq!(cpu.fpu.st(0), None);
        assert_eq!(cpu.read_word(0x3000, 0x0104) & 0x4500, 0x0100);
        assert!(cpu.registers.carry());
        assert_eq!(cpu.register8(4), Some(0x03)); // C0 maps to CF; bit 1 is fixed
    }

    #[test]
    fn software_interrupt_vectors_and_iret_restore_execution_state() {
        let mut cpu = Cpu8086::new();
        cpu.registers.ss = 0x3000;
        cpu.registers.sp = 0x0100;
        cpu.write_word(0, 0x21 * 4, 0x0100);
        cpu.write_word(0, 0x21 * 4 + 2, 0x2000);
        cpu.write_byte(0x2000, 0x0100, 0xCF); // IRET
        cpu.load(0x1000, 0, &[0xCD, 0x21, 0xF4]);

        cpu.step().unwrap();
        assert_eq!(cpu.take_interrupt(), Some(0x21));
        assert_eq!((cpu.registers.cs, cpu.registers.ip), (0x2000, 0x0100));
        assert_eq!(cpu.read_word(0x3000, 0x00FA), 2); // return IP is on top
        assert_eq!(cpu.take_interrupt(), None);
        cpu.step().unwrap(); // IRET
        assert_eq!((cpu.registers.cs, cpu.registers.ip), (0x1000, 2));
        assert_eq!(cpu.registers.sp, 0x0100);
        cpu.step().unwrap(); // HLT after INT
        assert!(cpu.is_halted());
    }
}
