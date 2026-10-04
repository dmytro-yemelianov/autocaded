//! Runtime environment for whole-program translated AutoCAD 2.18.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trap {
    Memory,
    DivisionByZero,
    InvalidPc,
    Budget,
    StackOverflow,
    Halt,
    InputRequired,
    UnsupportedInterrupt,
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Trap {}

#[allow(dead_code)]
pub fn read(bank: &[u8], offset: usize, size: usize) -> u64 {
    bank[offset..offset + size]
        .iter()
        .enumerate()
        .fold(0, |value, (i, byte)| value | ((*byte as u64) << (i * 8)))
}

#[allow(dead_code)]
pub fn write(bank: &mut [u8], offset: usize, size: usize, value: u64) {
    for (i, byte) in bank[offset..offset + size].iter_mut().enumerate() {
        *byte = (value >> (i * 8)) as u8;
    }
}

#[allow(dead_code)]
pub fn read128(bank: &[u8], offset: usize, size: usize) -> u128 {
    bank[offset..offset + size]
        .iter()
        .enumerate()
        .fold(0u128, |value, (i, byte)| {
            value | ((*byte as u128) << (i * 8))
        })
}

#[allow(dead_code)]
pub fn write128(bank: &mut [u8], offset: usize, size: usize, value: u128) {
    for (i, byte) in bank[offset..offset + size].iter_mut().enumerate() {
        *byte = (value >> (i * 8)) as u8;
    }
}

#[allow(dead_code)]
pub fn load(memory: &[u8], address: u64, size: usize) -> Result<u64, Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get(offset..end).ok_or(Trap::Memory)?;
    Ok(read(bytes, 0, size))
}

#[allow(dead_code)]
pub fn store(memory: &mut [u8], address: u64, size: usize, value: u64) -> Result<(), Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get_mut(offset..end).ok_or(Trap::Memory)?;
    write(bytes, 0, size, value);
    Ok(())
}

#[allow(dead_code)]
pub fn load128(memory: &[u8], address: u64, size: usize) -> Result<u128, Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get(offset..end).ok_or(Trap::Memory)?;
    Ok(read128(bytes, 0, size))
}

#[allow(dead_code)]
pub fn store128(memory: &mut [u8], address: u64, size: usize, value: u128) -> Result<(), Trap> {
    let offset = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let end = offset.checked_add(size).ok_or(Trap::Memory)?;
    let bytes = memory.get_mut(offset..end).ok_or(Trap::Memory)?;
    write128(bytes, 0, size, value);
    Ok(())
}

#[allow(dead_code)]
pub fn f80_to_f64(raw: u128) -> f64 {
    let sign = ((raw >> 79) & 1) != 0;
    let exp = ((raw >> 64) & 0x7fff) as u32;
    let mantissa = (raw & 0xffff_ffff_ffff_ffff) as u64;
    if exp == 0 && mantissa == 0 {
        return if sign { -0.0 } else { 0.0 };
    }
    if exp == 0x7fff {
        if mantissa == 0x8000_0000_0000_0000 {
            return if sign {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        return f64::NAN;
    }
    let frac = (mantissa & 0x7fff_ffff_ffff_ffff) as f64 / (1u64 << 63) as f64;
    let val = if mantissa & (1u64 << 63) != 0 {
        1.0 + frac
    } else {
        frac
    };
    let shift = (exp as i32) - 16383;
    let res = val * (2.0f64).powi(shift.clamp(-1022, 1023));
    if sign {
        -res
    } else {
        res
    }
}

#[allow(dead_code)]
pub fn f64_to_f80(val: f64) -> u128 {
    if val == 0.0 {
        return if val.is_sign_negative() {
            1u128 << 79
        } else {
            0
        };
    }
    if val.is_nan() {
        return (0x7fffu128 << 64) | (0xc000_0000_0000_0000u128);
    }
    if val.is_infinite() {
        let sign_bit = if val.is_sign_negative() {
            1u128 << 79
        } else {
            0
        };
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
pub fn signed(value: u64, bits: u32) -> i64 {
    ((value << (64 - bits)) as i64) >> (64 - bits)
}

#[allow(dead_code)]
pub fn shift_left(value: u64, count: u64) -> u64 {
    if count >= 64 {
        0
    } else {
        value << count
    }
}

#[allow(dead_code)]
pub fn shift_right(value: u64, count: u64) -> u64 {
    if count >= 64 {
        0
    } else {
        value >> count
    }
}

#[allow(dead_code)]
pub fn shift_signed_right(value: u64, count: u64, bits: u32) -> u64 {
    let signed_val = signed(value, bits);
    let shift = if count >= bits as u64 {
        (bits - 1) as u64
    } else {
        count
    };
    (signed_val >> shift) as u64
}

#[allow(dead_code)]
pub fn segment(seg: u64, off: u64) -> u64 {
    (seg << 4).wrapping_add(off)
}

#[allow(dead_code)]
pub fn read_port(_port: u64) -> u64 {
    0
}

#[allow(dead_code)]
pub fn write_port(_port: u64, _value: u64) {}

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

pub struct DosEnvironment {
    pub files: BTreeMap<[u8; 11], Vec<u8>>,
    pub dta: usize,
    pub console_output: Vec<u8>,
    pub input_lines: VecDeque<Vec<u8>>,
    pub input_bytes: VecDeque<u8>,
    /// Replay policy: stop at an empty nonblocking input poll.
    pub yield_on_empty_input: bool,
    pub exit_code: Option<u8>,
    loaded_overlay: Vec<LoadedOverlayRange>,
}

struct LoadedOverlayRange {
    address: usize,
    length: usize,
    file_offset: usize,
}

/// Resolve a guest address using the bytes actually transferred from ACAD.OVL.
pub fn loaded_overlay_offset(target: u64) -> Option<usize> {
    let target = usize::try_from(target).ok()?;
    DOS_ENV.with(|env| {
        env.borrow().loaded_overlay.iter().find_map(|range| {
            let displacement = target.checked_sub(range.address)?;
            (displacement < range.length).then_some(range.file_offset + displacement)
        })
    })
}

fn record_file_transfer(address: usize, length: usize, overlay: Option<(usize, usize)>) {
    let end = address + length;
    DOS_ENV.with(|env| {
        let mut env = env.borrow_mut();
        let mut ranges = Vec::new();
        for range in env.loaded_overlay.drain(..) {
            let old_end = range.address + range.length;
            if old_end <= address || range.address >= end {
                ranges.push(range);
                continue;
            }
            if range.address < address {
                ranges.push(LoadedOverlayRange {
                    address: range.address,
                    length: address - range.address,
                    file_offset: range.file_offset,
                });
            }
            if old_end > end {
                ranges.push(LoadedOverlayRange {
                    address: end,
                    length: old_end - end,
                    file_offset: range.file_offset + end - range.address,
                });
            }
        }
        if let Some((file_offset, length)) = overlay {
            if length != 0 {
                ranges.push(LoadedOverlayRange {
                    address,
                    length,
                    file_offset,
                });
            }
        }
        env.loaded_overlay = ranges;
    });
}

impl Default for DosEnvironment {
    fn default() -> Self {
        Self {
            files: BTreeMap::new(),
            dta: 0x10080,
            console_output: Vec::new(),
            input_lines: VecDeque::new(),
            input_bytes: VecDeque::new(),
            yield_on_empty_input: false,
            exit_code: None,
            loaded_overlay: Vec::new(),
        }
    }
}

thread_local! {
    pub static DOS_ENV: RefCell<DosEnvironment> = RefCell::new(DosEnvironment::default());
    static CALL_RETURNS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// Keep native far returns bounded to the current translated caller.
pub fn call_with_return(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
    callee: impl FnOnce(&mut [u8; 8192], &mut [u8], &mut usize) -> Result<u64, Trap>,
) -> Result<u64, Trap> {
    let ip = load(
        memory,
        segment(read(registers, 260, 2), read(registers, 16, 2)),
        2,
    )?;
    let target = segment(read(registers, 258, 2), ip);
    with_return_target(registers, memory, budget, target, callee)
}

pub fn call_far_with_return(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
    callee: impl FnOnce(&mut [u8; 8192], &mut [u8], &mut usize) -> Result<u64, Trap>,
) -> Result<u64, Trap> {
    let ss = read(registers, 260, 2);
    let sp = read(registers, 16, 2) as u16;
    let ip = load(memory, segment(ss, sp as u64), 2)?;
    let cs = load(memory, segment(ss, sp.wrapping_add(2) as u64), 2)?;
    with_return_target(registers, memory, budget, segment(cs, ip), callee)
}

fn with_return_target(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
    target: u64,
    callee: impl FnOnce(&mut [u8; 8192], &mut [u8], &mut usize) -> Result<u64, Trap>,
) -> Result<u64, Trap> {
    CALL_RETURNS.with(|returns| returns.borrow_mut().push(target));
    struct ReturnScope;
    impl Drop for ReturnScope {
        fn drop(&mut self) {
            CALL_RETURNS.with(|returns| {
                returns.borrow_mut().pop();
            });
        }
    }
    let _scope = ReturnScope;
    callee(registers, memory, budget)
}

pub fn mount_file(name: &str, data: Vec<u8>) {
    let key = to_fcb_name(name);
    DOS_ENV.with(|env| {
        env.borrow_mut().files.insert(key, data);
    });
}

pub fn get_console_output() -> Vec<u8> {
    DOS_ENV.with(|env| env.borrow().console_output.clone())
}

pub fn reset_dos() {
    DOS_ENV.with(|env| {
        *env.borrow_mut() = DosEnvironment::default();
    });
}

/// Queue a complete line for a deterministic, noninteractive DOS input call.
pub fn queue_input_line(line: &[u8]) {
    let end = line
        .iter()
        .position(|b| matches!(b, b'\r' | b'\n'))
        .unwrap_or(line.len());
    DOS_ENV.with(|env| env.borrow_mut().input_lines.push_back(line[..end].to_vec()));
}

pub fn queue_input_bytes(bytes: &[u8]) {
    DOS_ENV.with(|env| env.borrow_mut().input_bytes.extend(bytes.iter().copied()));
}

fn service_buffered_input(registers: &[u8; 8192], memory: &mut [u8]) -> Result<(), Trap> {
    let address = segment(read(registers, 262, 2), read(registers, 8, 2));
    let capacity = load(memory, address, 1)? as usize;
    if capacity == 0 {
        return Ok(());
    }
    // Validate the entire caller-provided buffer before consuming input.
    let start = usize::try_from(address).map_err(|_| Trap::Memory)?;
    let buffer = memory
        .get_mut(start..start + 2 + capacity)
        .ok_or(Trap::Memory)?;
    DOS_ENV.with(|env| {
        let mut env = env.borrow_mut();
        let line = env.input_lines.pop_front().ok_or(Trap::InputRequired)?;
        let count = line.len().min(capacity - 1);
        buffer[1] = count as u8;
        buffer[2..2 + count].copy_from_slice(&line[..count]);
        buffer[2 + count] = b'\r';
        env.console_output.extend_from_slice(&line[..count]);
        env.console_output.push(b'\r');
        Ok(())
    })
}

pub fn to_fcb_name(name: &str) -> [u8; 11] {
    let mut key = [b' '; 11];
    let (base, ext) = match name.split_once('.') {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    for (i, b) in base.bytes().take(8).enumerate() {
        key[i] = b.to_ascii_uppercase();
    }
    for (i, b) in ext.bytes().take(3).enumerate() {
        key[8 + i] = b.to_ascii_uppercase();
    }
    key
}

fn service_fcb_read(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    requested: usize,
    block_mode: bool,
) -> Result<(), Trap> {
    let ds = read(registers, 262, 2) as usize;
    let dx = read(registers, 8, 2) as usize;
    let fcb_addr = (ds << 4) + dx;
    memory.get(fcb_addr..fcb_addr + 37).ok_or(Trap::Memory)?;

    let mut key = [b' '; 11];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = memory
            .get(fcb_addr + 1 + i)
            .copied()
            .unwrap_or(b' ')
            .to_ascii_uppercase();
    }

    let rec_size = u16::from_le_bytes([memory[fcb_addr + 14], memory[fcb_addr + 15]]) as usize;
    let rec_num = u32::from_le_bytes([
        memory[fcb_addr + 33],
        memory[fcb_addr + 34],
        memory[fcb_addr + 35],
        memory[fcb_addr + 36],
    ]);

    let (file_data, dta) = DOS_ENV.with(|e| {
        let env = e.borrow();
        (env.files.get(&key).cloned(), env.dta)
    });

    let Some(file) = file_data else {
        write(registers, 0, 1, 1);
        if block_mode {
            write(registers, 4, 2, 0);
        }
        return Ok(());
    };

    if rec_size == 0 || requested == 0 {
        write(registers, 0, 1, 0);
        return Ok(());
    }

    let mut completed = 0;
    let mut current_rec = rec_num;
    let mut eof = false;

    let file_offset = (rec_num as usize)
        .checked_mul(rec_size)
        .ok_or(Trap::Memory)?;
    let available = file.len().saturating_sub(file_offset);
    let records = requested.min(available.div_ceil(rec_size));
    let transfer_length = records.checked_mul(rec_size).ok_or(Trap::Memory)?;
    let transfer_end = dta.checked_add(transfer_length).ok_or(Trap::Memory)?;
    memory.get(dta..transfer_end).ok_or(Trap::Memory)?;

    for i in 0..requested {
        let start = (current_rec as usize) * rec_size;
        if start >= file.len() {
            eof = true;
            break;
        }
        let avail = (file.len() - start).min(rec_size);
        let dest = dta + i * rec_size;
        let buffer = memory.get_mut(dest..dest + rec_size).ok_or(Trap::Memory)?;
        buffer[..avail].copy_from_slice(&file[start..start + avail]);
        if avail < rec_size {
            buffer[avail..].fill(0);
            eof = true;
        }
        completed += 1;
        current_rec = current_rec.wrapping_add(1);
        if eof {
            break;
        }
    }

    if transfer_length != 0 {
        let overlay = (key == to_fcb_name("ACAD.OVL"))
            .then_some((file_offset, transfer_length.min(available)));
        record_file_transfer(dta, transfer_length, overlay);
    }

    if block_mode {
        memory[fcb_addr + 33..fcb_addr + 37].copy_from_slice(&current_rec.to_le_bytes());
        write(registers, 4, 2, completed as u64);
    }

    let status = if eof {
        if completed == 0 {
            1
        } else {
            3
        }
    } else {
        0
    };
    write(registers, 0, 1, status);
    Ok(())
}

fn service_fcb_write(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    requested: usize,
    block_mode: bool,
) -> Result<(), Trap> {
    let ds = read(registers, 262, 2) as usize;
    let dx = read(registers, 8, 2) as usize;
    let fcb_addr = (ds << 4) + dx;
    memory.get(fcb_addr..fcb_addr + 37).ok_or(Trap::Memory)?;

    let mut key = [b' '; 11];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = memory
            .get(fcb_addr + 1 + i)
            .copied()
            .unwrap_or(b' ')
            .to_ascii_uppercase();
    }

    let rec_size = u16::from_le_bytes([memory[fcb_addr + 14], memory[fcb_addr + 15]]) as usize;
    let rec_num = u32::from_le_bytes([
        memory[fcb_addr + 33],
        memory[fcb_addr + 34],
        memory[fcb_addr + 35],
        memory[fcb_addr + 36],
    ]);

    let dta = DOS_ENV.with(|e| e.borrow().dta);
    let transfer_size = requested.checked_mul(rec_size).ok_or(Trap::Memory)?;
    memory
        .get(dta..dta.checked_add(transfer_size).ok_or(Trap::Memory)?)
        .ok_or(Trap::Memory)?;

    let mut completed = 0;
    let mut current_rec = rec_num;

    DOS_ENV.with(|e| {
        let mut env = e.borrow_mut();
        if let Some(file) = env.files.get_mut(&key) {
            for i in 0..requested {
                let start = (current_rec as usize) * rec_size;
                let src = dta + i * rec_size;
                if start + rec_size > file.len() {
                    file.resize(start + rec_size, 0);
                }
                if src + rec_size <= memory.len() {
                    file[start..start + rec_size].copy_from_slice(&memory[src..src + rec_size]);
                }
                completed += 1;
                current_rec = current_rec.wrapping_add(1);
            }
            let new_len = file.len() as u32;
            memory[fcb_addr + 16..fcb_addr + 20].copy_from_slice(&new_len.to_le_bytes());
        }
    });

    if block_mode {
        memory[fcb_addr + 33..fcb_addr + 37].copy_from_slice(&current_rec.to_le_bytes());
        write(registers, 4, 2, completed as u64);
    }
    write(registers, 0, 1, 0);
    Ok(())
}

/// Initialize the hosted machine's 80x25 color text display and BIOS data.
pub fn initialize_text_video(memory: &mut [u8]) -> Result<(), Trap> {
    memory.get(0xb8000..0xb8000 + 4000).ok_or(Trap::Memory)?;
    store(memory, 0x449, 1, 3)?;
    store(memory, 0x44a, 2, 80)?;
    store(memory, 0x44c, 2, 0x1000)?;
    store(memory, 0x44e, 2, 0)?;
    memory[0x450..0x460].fill(0);
    store(memory, 0x460, 2, 0x0607)?;
    store(memory, 0x462, 1, 0)?;
    store(memory, 0x463, 2, 0x3d4)?;
    for cell in memory[0xb8000..0xb8000 + 4000].chunks_exact_mut(2) {
        cell.copy_from_slice(b" \x07");
    }
    Ok(())
}

pub fn host_interrupt(
    int_num: u64,
    registers: &mut [u8; 8192],
    memory: &mut [u8],
) -> Result<u64, Trap> {
    let ax = read(registers, 0, 2);
    let ah = (ax >> 8) as u8;
    let al = (ax & 0xff) as u8;
    let cx = read(registers, 4, 2);
    let dx = read(registers, 8, 2);
    let ds = read(registers, 262, 2);

    match int_num {
        0x10 => {
            if std::env::var_os("ACAD_RE_TRACE_DOS").is_some() {
                eprintln!(
                    "[BIOS] INT 10h: AX={ax:04x} BX={:04x} CX={cx:04x} DX={dx:04x}",
                    read(registers, 12, 2)
                );
            }
            match ah {
                0x00 if al == 3 => initialize_text_video(memory)?,
                0x01 => store(memory, 0x460, 2, cx)?,
                0x02 => {
                    if read(registers, 13, 1) != 0 || dx >> 8 >= 25 || dx & 0xff >= 80 {
                        return Err(Trap::UnsupportedInterrupt);
                    }
                    store(memory, 0x450, 2, dx)?;
                }
                0x03 => {
                    if read(registers, 13, 1) != 0 {
                        return Err(Trap::UnsupportedInterrupt);
                    }
                    write(registers, 4, 2, load(memory, 0x460, 2)?);
                    write(registers, 8, 2, load(memory, 0x450, 2)?);
                }
                0x05 if al == 0 => {}
                0x08..=0x0a => {
                    if read(registers, 13, 1) != 0 {
                        return Err(Trap::UnsupportedInterrupt);
                    }
                    let cursor = load(memory, 0x450, 2)?;
                    let row = (cursor >> 8) as usize;
                    let column = (cursor & 0xff) as usize;
                    if row >= 25 || column >= 80 {
                        return Err(Trap::UnsupportedInterrupt);
                    }
                    let address = 0xb8000 + (row * 80 + column) * 2;
                    if ah == 0x08 {
                        write(registers, 0, 2, load(memory, address as u64, 2)?);
                    } else {
                        let count = cx as usize;
                        if count > 2000 - (row * 80 + column) {
                            return Err(Trap::UnsupportedInterrupt);
                        }
                        let end = address + count * 2;
                        let buffer = memory.get_mut(address..end).ok_or(Trap::Memory)?;
                        for cell in buffer.chunks_exact_mut(2) {
                            cell[0] = al;
                            if ah == 0x09 {
                                cell[1] = read(registers, 12, 1) as u8;
                            }
                        }
                    }
                }
                0x0e => {
                    let cursor = load(memory, 0x450, 2)?;
                    let mut row = (cursor >> 8) as usize;
                    let mut column = (cursor & 0xff) as usize;
                    if row >= 25 || column >= 80 {
                        return Err(Trap::UnsupportedInterrupt);
                    }
                    match al {
                        7 => {}
                        8 => column = column.saturating_sub(1),
                        13 => column = 0,
                        10 => row += 1,
                        _ => {
                            store(
                                memory,
                                (0xb8000 + (row * 80 + column) * 2) as u64,
                                1,
                                al as u64,
                            )?;
                            column += 1;
                            if column == 80 {
                                column = 0;
                                row += 1;
                            }
                        }
                    }
                    if row == 25 {
                        let mut scroll_registers = *registers;
                        write(&mut scroll_registers, 0, 2, 0x0601);
                        let attribute = load(memory, 0xb8000 + 3999, 1)?;
                        write(&mut scroll_registers, 12, 2, attribute << 8);
                        write(&mut scroll_registers, 4, 2, 0);
                        write(&mut scroll_registers, 8, 2, 0x184f);
                        host_interrupt(0x10, &mut scroll_registers, memory)?;
                        row = 24;
                    }
                    store(memory, 0x450, 2, ((row << 8) | column) as u64)?;
                    DOS_ENV.with(|env| env.borrow_mut().console_output.push(al));
                }
                0x0f => {
                    write(registers, 0, 2, 0x5003);
                    write(registers, 13, 1, 0);
                }
                0x06 => {
                    // Initial hosted display: 80x25 color text, page zero.
                    let top = (cx >> 8) as usize;
                    let left = (cx & 0xff) as usize;
                    let bottom = (dx >> 8) as usize;
                    let right = (dx & 0xff) as usize;
                    if top > bottom || left > right || bottom >= 25 || right >= 80 {
                        return Err(Trap::UnsupportedInterrupt);
                    }
                    memory.get(0xb8000..0xb8000 + 4000).ok_or(Trap::Memory)?;
                    let height = bottom - top + 1;
                    let lines = if al == 0 {
                        height
                    } else {
                        (al as usize).min(height)
                    };
                    let attribute = read(registers, 13, 1) as u8;
                    for row in top..=bottom {
                        for column in left..=right {
                            let dest = 0xb8000 + (row * 80 + column) * 2;
                            if row + lines <= bottom {
                                let src = 0xb8000 + ((row + lines) * 80 + column) * 2;
                                memory[dest] = memory[src];
                                memory[dest + 1] = memory[src + 1];
                            } else {
                                memory[dest] = b' ';
                                memory[dest + 1] = attribute;
                            }
                        }
                    }
                }
                _ => return Err(Trap::UnsupportedInterrupt),
            }
        }
        0x20 => {
            eprintln!("[DOS] INT 20h: Terminate Process");
            DOS_ENV.with(|e| e.borrow_mut().exit_code = Some(0));
            return Err(Trap::Halt);
        }
        0x21 => {
            if std::env::var_os("ACAD_RE_TRACE_DOS").is_some() {
                eprintln!(
                    "[DOS] INT 21h: AH={ah:02x} AL={al:02x} CX={cx:04x} DX={dx:04x} DS={ds:04x}"
                );
            }
            match ah {
                0x0A => service_buffered_input(registers, memory)?,
                0x00 => {
                    DOS_ENV.with(|e| e.borrow_mut().exit_code = Some(0));
                    return Err(Trap::Halt);
                }
                0x4C => {
                    DOS_ENV.with(|e| e.borrow_mut().exit_code = Some(al));
                    return Err(Trap::Halt);
                }
                0x02 => {
                    let byte = dx as u8;
                    print!("{}", byte as char);
                    DOS_ENV.with(|e| e.borrow_mut().console_output.push(byte));
                    write(registers, 0, 1, byte as u64);
                }
                0x06 => {
                    let byte = dx as u8;
                    if byte == 0xFF {
                        let (input, yield_on_empty) = DOS_ENV.with(|env| {
                            let mut env = env.borrow_mut();
                            (env.input_bytes.pop_front(), env.yield_on_empty_input)
                        });
                        write(registers, 0, 1, input.unwrap_or(0) as u64);
                        write(registers, 518, 1, u64::from(input.is_none()));
                        if input.is_none() && yield_on_empty {
                            return Err(Trap::InputRequired);
                        }
                    } else {
                        print!("{}", byte as char);
                        DOS_ENV.with(|e| e.borrow_mut().console_output.push(byte));
                        write(registers, 0, 1, byte as u64);
                        write(registers, 518, 1, 0);
                    }
                }
                0x09 => {
                    let seg_base = (ds as usize) << 4;
                    let mut off = dx as usize;
                    let mut string = Vec::new();
                    while off < 0x10000 {
                        let b = load(memory, (seg_base + off) as u64, 1)? as u8;
                        if b == b'$' {
                            break;
                        }
                        string.push(b);
                        off += 1;
                    }
                    if let Ok(s) = std::str::from_utf8(&string) {
                        print!("{s}");
                    }
                    DOS_ENV.with(|e| e.borrow_mut().console_output.extend(&string));
                    write(registers, 0, 1, b'$' as u64);
                }
                0x0E => {
                    write(registers, 0, 1, 1);
                }
                0x19 => {
                    write(registers, 0, 1, 0);
                }
                0x1A => {
                    let dta = ((ds as usize) << 4) + (dx as usize);
                    DOS_ENV.with(|e| e.borrow_mut().dta = dta);
                }
                0x0F => {
                    let fcb_addr = ((ds as usize) << 4) + (dx as usize);
                    memory.get(fcb_addr..fcb_addr + 37).ok_or(Trap::Memory)?;
                    let mut key = [b' '; 11];
                    for (i, byte) in key.iter_mut().enumerate() {
                        *byte = memory
                            .get(fcb_addr + 1 + i)
                            .copied()
                            .unwrap_or(b' ')
                            .to_ascii_uppercase();
                    }
                    let is_console = key.starts_with(b"CON:") || key.starts_with(b"CON ");
                    let found = is_console || DOS_ENV.with(|e| e.borrow().files.contains_key(&key));
                    let name_str = String::from_utf8_lossy(&key);
                    eprintln!(
                        "[DOS] Open FCB at {fcb_addr:05x}: '{name_str}', drive: {}, found: {found}",
                        memory[fcb_addr]
                    );
                    if found {
                        memory[fcb_addr + 12..fcb_addr + 14].copy_from_slice(&0u16.to_le_bytes());
                        let curr_rec_size =
                            u16::from_le_bytes([memory[fcb_addr + 14], memory[fcb_addr + 15]]);
                        if curr_rec_size == 0 {
                            memory[fcb_addr + 14..fcb_addr + 16]
                                .copy_from_slice(&128u16.to_le_bytes());
                        }
                        let file_len = DOS_ENV.with(|e| {
                            e.borrow()
                                .files
                                .get(&key)
                                .map(|v| v.len() as u32)
                                .unwrap_or(0)
                        });
                        memory[fcb_addr + 16..fcb_addr + 20]
                            .copy_from_slice(&file_len.to_le_bytes());
                        write(registers, 0, 1, 0);
                    } else {
                        write(registers, 0, 1, 0xFF);
                    }
                }
                0x10 => {
                    write(registers, 0, 1, 0);
                }
                0x16 => {
                    let fcb_addr = ((ds as usize) << 4) + (dx as usize);
                    memory.get(fcb_addr..fcb_addr + 37).ok_or(Trap::Memory)?;
                    let mut key = [b' '; 11];
                    for (i, byte) in key.iter_mut().enumerate() {
                        *byte = memory
                            .get(fcb_addr + 1 + i)
                            .copied()
                            .unwrap_or(b' ')
                            .to_ascii_uppercase();
                    }
                    DOS_ENV.with(|e| {
                        e.borrow_mut().files.insert(key, Vec::new());
                    });
                    for field in 12..=36 {
                        memory[fcb_addr + field] = 0;
                    }
                    memory[fcb_addr + 14..fcb_addr + 16].copy_from_slice(&128u16.to_le_bytes());
                    write(registers, 0, 1, 0);
                }
                0x21 => {
                    service_fcb_read(registers, memory, 1, false)?;
                }
                0x27 => {
                    service_fcb_read(registers, memory, cx as usize, true)?;
                }
                0x22 => {
                    service_fcb_write(registers, memory, 1, false)?;
                }
                0x28 => {
                    service_fcb_write(registers, memory, cx as usize, true)?;
                }
                0x25 => {
                    let addr = (al as usize) * 4;
                    if addr + 4 <= memory.len() {
                        memory[addr..addr + 2].copy_from_slice(&(dx as u16).to_le_bytes());
                        memory[addr + 2..addr + 4].copy_from_slice(&(ds as u16).to_le_bytes());
                    }
                }
                0x35 => {
                    let addr = (al as usize) * 4;
                    if addr + 4 <= memory.len() {
                        let off = u16::from_le_bytes([memory[addr], memory[addr + 1]]);
                        let seg = u16::from_le_bytes([memory[addr + 2], memory[addr + 3]]);
                        write(registers, 12, 2, off as u64);
                        write(registers, 256, 2, seg as u64);
                    }
                }
                0x30 => {
                    write(registers, 0, 2, 0x0002);
                }
                0x36 => {
                    write(registers, 0, 2, 1);
                    write(registers, 12, 2, 300);
                    write(registers, 4, 2, 512);
                    write(registers, 8, 2, 354);
                }
                _ => return Err(Trap::UnsupportedInterrupt),
            }
        }
        _ => return Err(Trap::UnsupportedInterrupt),
    }
    Ok(0)
}

pub fn dispatch_call(
    _registers: &mut [u8; 8192],
    _memory: &mut [u8],
    _budget: &mut usize,
    _target: u64,
) -> Result<u64, Trap> {
    Err(Trap::InvalidPc)
}

pub fn dispatch_tailcall(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
    target_block: &str,
    target: u64,
) -> Result<u64, Trap> {
    crate::dispatch_address(target_block, target, registers, memory, budget)
}

fn bridge_step(registers: &mut [u8; 8192], offset: usize, increment: bool) -> u16 {
    let previous = read(registers, offset, 2) as u16;
    let result = if increment {
        previous.wrapping_add(1)
    } else {
        previous.wrapping_sub(1)
    };
    write(registers, offset, 2, result as u64);
    write(
        registers,
        523,
        1,
        u64::from(previous == if increment { 0x7fff } else { 0x8000 }),
    );
    write(registers, 519, 1, u64::from(result & 0x8000 != 0));
    write(registers, 518, 1, u64::from(result == 0));
    write(
        registers,
        514,
        1,
        u64::from((result as u8).count_ones() % 2 == 0),
    );
    result
}

/// Execute the verified POP SI / CS:[SI] / far-jump kernel thunk and
/// resident 8f80 bridge, returning to the translated overlay caller.
pub fn dispatch_kernel_tailcall(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
    native_ip: u16,
) -> Result<u64, Trap> {
    let ds = read(registers, 262, 2);
    let pointer = segment(ds, 0x4d76);
    if load(memory, pointer, 2)? != 0x8f80 {
        return Err(Trap::InvalidPc);
    }
    let resident_cs = load(memory, pointer + 2, 2)?;
    // The helper consumes its own inline operand, leaving the caller's IP
    // on the stack for the resident bridge to save.
    write(registers, 24, 2, native_ip as u64);
    write(registers, 258, 2, resident_cs);
    let counter = segment(ds, 0x4d86);
    write(registers, 12, 2, load(memory, counter, 2)?);
    bridge_step(registers, 12, false);
    let bx = bridge_step(registers, 12, false);
    let ss = read(registers, 260, 2);
    let sp = read(registers, 16, 2) as u16;
    let return_ip = load(memory, segment(ss, sp as u64), 2)?;
    store(
        memory,
        segment(ds, 0x4d7eu16.wrapping_add(bx) as u64),
        2,
        return_ip,
    )?;
    write(registers, 16, 2, sp.wrapping_add(2) as u64);
    if bx & 0x8000 != 0 {
        return Err(Trap::StackOverflow);
    }
    store(memory, counter, 2, bx as u64)?;
    // CALL SI pushes the resident continuation; RET consumes it normally.
    let sp = (read(registers, 16, 2) as u16).wrapping_sub(2);
    write(registers, 16, 2, sp as u64);
    store(memory, segment(ss, sp as u64), 2, 0x8f92)?;
    call_with_return(registers, memory, budget, |registers, memory, budget| {
        crate::dispatch_address(
            "EXE_CODE",
            segment(resident_cs, native_ip as u64),
            registers,
            memory,
            budget,
        )
    })?;
    let ds = read(registers, 262, 2);
    let counter = segment(ds, 0x4d86);
    let si = load(memory, counter, 2)? as u16;
    let di = load(memory, segment(ds, 0x4d7eu16.wrapping_add(si) as u64), 2)?;
    write(registers, 24, 2, si as u64);
    write(registers, 28, 2, di);
    bridge_step(registers, 24, true);
    let si = bridge_step(registers, 24, true);
    store(memory, counter, 2, si as u64)?;
    store(memory, segment(ds, 0x4d72), 2, di)?;
    let overlay_cs = load(memory, segment(ds, 0x4d74), 2)?;
    if std::env::var_os("ACAD_RE_TRACE_DOS").is_some() {
        eprintln!("[bridge] resident {native_ip:04x} returns to {overlay_cs:04x}:{di:04x}");
    }
    write(registers, 258, 2, overlay_cs);
    Ok(segment(overlay_cs, di))
}

pub fn dispatch_indirect(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
    caller_block: &str,
    target: u64,
) -> Result<u64, Trap> {
    if std::env::var_os("ACAD_RE_TRACE_DOS").is_some() {
        eprintln!("[indirect] {caller_block} -> {target:#x}");
    }
    // The resident BIOS wrapper builds exactly INT imm8; RETF in DS.
    // Validate its location and bytes rather than interpreting arbitrary RAM.
    let ds = read(registers, 262, 2);
    if caller_block == "EXE_CODE" && target == segment(ds, 0x4e1a) && read(registers, 258, 2) == ds
    {
        if load(memory, target, 1)? != 0xcd || load(memory, target + 2, 1)? != 0xcb {
            return Err(Trap::InvalidPc);
        }
        if *budget == 0 {
            return Err(Trap::Budget);
        }
        *budget -= 1;
        let int_num = load(memory, target + 1, 1)?;
        host_interrupt(int_num, registers, memory)?;
        if *budget == 0 {
            return Err(Trap::Budget);
        }
        *budget -= 1;
        let ss = read(registers, 260, 2);
        let sp = read(registers, 16, 2) as u16;
        let ip = load(memory, segment(ss, sp as u64), 2)?;
        let cs = load(memory, segment(ss, sp.wrapping_add(2) as u64), 2)?;
        write(registers, 16, 2, sp.wrapping_add(4) as u64);
        write(registers, 258, 2, cs);
        return Ok(segment(cs, ip));
    }
    crate::dispatch_address(caller_block, target, registers, memory, budget)
}

pub fn dispatch_indirect_jump(
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
    caller_block: &str,
    target: u64,
) -> Result<u64, Trap> {
    if CALL_RETURNS.with(|returns| returns.borrow().last().copied() == Some(target)) {
        return Ok(target);
    }
    crate::dispatch_address(caller_block, target, registers, memory, budget)
}
