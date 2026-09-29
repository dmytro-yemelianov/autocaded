//! Minimal DOS process runner and host interrupt services for the 8086 core.

use crate::{
    cpu8086::{Cpu8086, CpuError},
    dos_loader::{DosLoadError, DosProcess},
    mz_loader::MzExecutable,
};
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fmt;

// ROM addresses reserved for host-provided DOS and video services. Programs
// such as AutoCAD can enter these through the IVT without executing INT.
const HOST_VECTOR_SEGMENT: u16 = 0xF000;
const DOS21_VECTOR_OFFSET: u16 = 0x0100;
const VIDEO10_VECTOR_OFFSET: u16 = 0x0101;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DosRunError {
    Load(DosLoadError),
    Cpu(CpuError),
    UnsupportedInterrupt(u8),
    UnsupportedFunction(u8),
    UnsupportedVideoFunction(u8),
    UnterminatedDollarString,
    StepLimit(usize),
    Halted,
}

impl fmt::Display for DosRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(f, "DOS load failed: {error}"),
            Self::Cpu(error) => write!(f, "8086 error: {error}"),
            Self::UnsupportedInterrupt(vector) => {
                write!(f, "DOS machine does not implement interrupt {vector:#04x}")
            }
            Self::UnsupportedFunction(function) => {
                write!(f, "DOS INT 21h function {function:#04x} is not implemented")
            }
            Self::UnsupportedVideoFunction(function) => {
                write!(
                    f,
                    "BIOS INT 10h function {function:#04x} is not implemented"
                )
            }
            Self::UnterminatedDollarString => {
                write!(f, "DOS function 09h string has no '$' terminator")
            }
            Self::StepLimit(limit) => write!(f, "DOS program exceeded {limit} instructions"),
            Self::Halted => write!(f, "DOS program executed HLT instead of terminating"),
        }
    }
}

impl std::error::Error for DosRunError {}

impl From<DosLoadError> for DosRunError {
    fn from(value: DosLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<CpuError> for DosRunError {
    fn from(value: CpuError) -> Self {
        Self::Cpu(value)
    }
}

#[derive(Debug, Clone)]
pub struct DosMachine {
    pub cpu: Cpu8086,
    pub process: DosProcess,
    output: Vec<u8>,
    exit_code: Option<u8>,
    files: BTreeMap<[u8; 11], Vec<u8>>,
    disk_used_clusters: usize,
    dta: (u16, u16),
    video_mode: u8,
    video_page: u8,
    cursor: (u8, u8),
    keyboard: VecDeque<u8>,
    console_polls: u64,
}

impl DosMachine {
    pub fn load(
        executable: &MzExecutable,
        psp_segment: u16,
        top_segment: u16,
        command_tail: &[u8],
    ) -> Result<Self, DosRunError> {
        Self::load_with_files(executable, psp_segment, top_segment, command_tail, [])
    }

    /// Load a DOS process with files made available through FCB operations.
    /// `files` uses DOS 8.3 names, case-insensitively.
    pub fn load_with_files<'a>(
        executable: &MzExecutable,
        psp_segment: u16,
        top_segment: u16,
        command_tail: &[u8],
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<Self, DosRunError> {
        let mut cpu = Cpu8086::new();
        initialize_bios_data_area(&mut cpu);
        install_host_vector(&mut cpu, 0x21, DOS21_VECTOR_OFFSET);
        install_host_vector(&mut cpu, 0x10, VIDEO10_VECTOR_OFFSET);
        // DOS keeps the environment in a separate paragraph block. The
        // startup runtime walks this block even when it contains no variables.
        let environment_segment = psp_segment - 0x20;
        let environment = [
            0, 0, // empty, double-NUL-terminated environment
            1, 0, // one following program-name string
            b'A', b':', b'\\', b'A', b'C', b'A', b'D', b'.', b'E', b'X', b'E', 0,
        ];
        for (offset, byte) in environment.into_iter().enumerate() {
            cpu.write_byte(environment_segment, offset as u16, byte);
        }
        let process = DosProcess::load(
            &mut cpu,
            executable,
            psp_segment,
            top_segment,
            psp_segment,
            environment_segment,
            command_tail,
        )?;
        let files = files
            .into_iter()
            .map(|(name, bytes)| (dos_name_key(name), bytes.to_vec()))
            .collect::<BTreeMap<_, _>>();
        let disk_used_clusters = executable.file_bytes.div_ceil(1024)
            + files
                .values()
                .map(|bytes| bytes.len().div_ceil(1024))
                .sum::<usize>();
        Ok(Self {
            cpu,
            process,
            output: Vec::new(),
            exit_code: None,
            files,
            disk_used_clusters,
            dta: (psp_segment, 0x80),
            video_mode: 3,
            video_page: 0,
            cursor: (0, 0),
            keyboard: VecDeque::new(),
            console_polls: 0,
        })
    }

    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn file(&self, name: &str) -> Option<&[u8]> {
        self.files.get(&dos_name_key(name)).map(Vec::as_slice)
    }

    pub fn exit_code(&self) -> Option<u8> {
        self.exit_code
    }

    pub fn console_polls(&self) -> u64 {
        self.console_polls
    }

    pub fn pending_input(&self) -> usize {
        self.keyboard.len()
    }

    /// The 16 KiB CGA display buffer at physical B8000h.
    pub fn cga_memory(&self) -> Vec<u8> {
        (0..16_384)
            .map(|offset| self.cpu.read_byte(0xB800, offset))
            .collect()
    }

    /// Queue keys for DOS console polling calls such as INT 21h/AH=06h.
    pub fn push_input(&mut self, keys: impl IntoIterator<Item = u8>) {
        self.keyboard.extend(keys);
    }

    pub fn video_mode(&self) -> u8 {
        self.video_mode
    }

    pub fn video_page(&self) -> u8 {
        self.video_page
    }

    /// Execute until DOS termination, an unsupported service, HLT, or limit.
    pub fn run(&mut self, instruction_limit: usize) -> Result<u8, DosRunError> {
        for _ in 0..instruction_limit {
            if let Some(code) = self.exit_code {
                return Ok(code);
            }
            if self.cpu.is_halted() {
                return Err(DosRunError::Halted);
            }
            if self.service_host_vector()? {
                continue;
            }
            self.update_cga_status();
            self.cpu.step()?;
            if let Some(vector) = self.cpu.take_interrupt() {
                self.service_interrupt(vector)?;
            }
        }
        self.exit_code
            .ok_or(DosRunError::StepLimit(instruction_limit))
    }

    fn update_cga_status(&mut self) {
        // Port 03DAh has display-enable (bit 0) and vertical retrace (bit 3).
        // Advance both on the emulated instruction clock so polling loops see
        // transitions without depending on host wall time.
        let ticks = self.cpu.instructions();
        let display_enable = ((ticks / 16) & 1) as u8;
        let vertical_retrace = (((ticks / 512) & 1) as u8) << 3;
        self.cpu
            .write_port_byte(0x03DA, display_enable | vertical_retrace);
    }

    fn service_host_vector(&mut self) -> Result<bool, DosRunError> {
        if self.cpu.registers.cs != HOST_VECTOR_SEGMENT {
            return Ok(false);
        }
        let nonblocking_input = self.cpu.registers.ip == DOS21_VECTOR_OFFSET
            && self.cpu.registers.ax.to_be_bytes()[0] == 0x06
            && self.cpu.registers.dx as u8 == 0xFF;
        let input_available = nonblocking_input && !self.keyboard.is_empty();
        match self.cpu.registers.ip {
            DOS21_VECTOR_OFFSET => self.service_dos21()?,
            VIDEO10_VECTOR_OFFSET => self.service_video10()?,
            _ => return Ok(false),
        }
        if self.exit_code.is_none() {
            self.cpu.interrupt_return();
            if nonblocking_input {
                self.cpu.set_zero_flag(!input_available);
            }
        }
        Ok(true)
    }

    fn service_interrupt(&mut self, vector: u8) -> Result<(), DosRunError> {
        let dos_function = if vector == 0x21 {
            self.cpu.registers.ax.to_be_bytes()[0]
        } else {
            0
        };
        let nonblocking_input =
            vector == 0x21 && dos_function == 0x06 && self.cpu.registers.dx as u8 == 0xFF;
        let input_available = nonblocking_input && !self.keyboard.is_empty();
        match vector {
            0x20 => self.exit_code = Some(0),
            0x21 => self.service_dos21()?,
            0x10 => self.service_video10()?,
            _ => {
                let address = u16::from(vector) * 4;
                let offset = self.cpu.read_word(0, address);
                let segment = self.cpu.read_word(0, address.wrapping_add(2));
                if offset == 0 && segment == 0 {
                    return Err(DosRunError::UnsupportedInterrupt(vector));
                }
                // The CPU has already pushed FLAGS:CS:IP and entered the
                // installed guest handler. Let its IRET restore execution.
                return Ok(());
            }
        }
        if self.exit_code.is_none() {
            self.cpu.interrupt_return();
            if nonblocking_input {
                self.cpu.set_zero_flag(!input_available);
            }
        }
        Ok(())
    }

    fn service_dos21(&mut self) -> Result<(), DosRunError> {
        let function = self.cpu.registers.ax.to_be_bytes()[0];
        match function {
            0x19 => self.cpu.set_reg8(0, 0), // default drive A:
            0x0E => self.cpu.set_reg8(0, 1), // one available drive
            0x0F => {
                // The original program probes the DOS CON device through an
                // FCB (CON: followed by spaces). DOS treats it as an open
                // character device; disk files are looked up on the mounted
                // floppy snapshot supplied to the DOS machine.
                let segment = self.cpu.registers.ds;
                let offset = self.cpu.registers.dx;
                let name: Vec<_> = (1..=11)
                    .map(|index| self.cpu.read_byte(segment, offset.wrapping_add(index)))
                    .collect();
                let is_console = name.starts_with(b"CON:") || name.starts_with(b"CON ");
                let key: [u8; 11] = name.as_slice().try_into().expect("11 FCB name bytes");
                let key = key.map(|byte| byte.to_ascii_uppercase());
                let opened = is_console || self.files.contains_key(&key);
                if opened {
                    self.cpu.write_word(segment, offset.wrapping_add(12), 0);
                    if self.cpu.read_word(segment, offset.wrapping_add(14)) == 0 {
                        self.cpu.write_word(segment, offset.wrapping_add(14), 128);
                    }
                    if let Some(bytes) = self.files.get(&key) {
                        let size = bytes.len().min(u32::MAX as usize) as u32;
                        self.cpu
                            .write_word(segment, offset.wrapping_add(16), size as u16);
                        self.cpu
                            .write_word(segment, offset.wrapping_add(18), (size >> 16) as u16);
                    }
                }
                self.cpu.set_reg8(0, if opened { 0 } else { 0xFF });
            }
            0x10 => {
                let key = self.fcb_name_key();
                let is_console = key.starts_with(b"CON:") || key.starts_with(b"CON ");
                let closed = is_console || self.files.contains_key(&key);
                self.cpu.set_reg8(0, if closed { 0 } else { 0xFF });
            }
            0x13 => {
                let key = self.fcb_name_key();
                let deleted = self.files.remove(&key);
                if let Some(bytes) = &deleted {
                    self.disk_used_clusters -= bytes.len().div_ceil(1024);
                }
                self.cpu
                    .set_reg8(0, if deleted.is_some() { 0 } else { 0xFF });
            }
            0x16 => {
                // FCB create truncates an existing file or adds a new one.
                let segment = self.cpu.registers.ds;
                let offset = self.cpu.registers.dx;
                let key = self.fcb_name_key();
                let is_console = key.starts_with(b"CON:") || key.starts_with(b"CON ");
                let valid_drive = self.cpu.read_byte(segment, offset) <= 1;
                if valid_drive && !is_console {
                    if let Some(previous) = self.files.insert(key, Vec::new()) {
                        self.disk_used_clusters -= previous.len().div_ceil(1024);
                    }
                }
                if valid_drive {
                    for field in 12..=36 {
                        self.cpu.write_byte(segment, offset.wrapping_add(field), 0);
                    }
                    self.cpu.write_word(segment, offset.wrapping_add(14), 128);
                }
                self.cpu.set_reg8(0, if valid_drive { 0 } else { 0xFF });
            }
            0x17 => {
                let segment = self.cpu.registers.ds;
                let offset = self.cpu.registers.dx;
                let old_name = self.fcb_name_key();
                let new_name: [u8; 11] = (17..=27)
                    .map(|field| {
                        self.cpu
                            .read_byte(segment, offset.wrapping_add(field))
                            .to_ascii_uppercase()
                    })
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("11 FCB destination name bytes");
                let renamed = if self.files.contains_key(&new_name) {
                    false
                } else if let Some(file) = self.files.remove(&old_name) {
                    self.files.insert(new_name, file);
                    true
                } else {
                    false
                };
                self.cpu.set_reg8(0, if renamed { 0 } else { 0xFF });
            }
            0x1A => self.dta = (self.cpu.registers.ds, self.cpu.registers.dx),
            0x21 => self.read_random_fcb_records(1, false)?,
            0x22 => self.write_random_fcb_records(1, false),
            0x27 => self.read_random_fcb_records(usize::from(self.cpu.registers.cx), true)?,
            0x28 => self.write_random_fcb_records(usize::from(self.cpu.registers.cx), true),
            0x25 => {
                let vector = u16::from(self.cpu.register8(0).expect("AL register"));
                let address = vector * 4;
                self.cpu.write_word(0, address, self.cpu.registers.dx);
                self.cpu
                    .write_word(0, address.wrapping_add(2), self.cpu.registers.ds);
            }
            0x35 => {
                let vector = u16::from(self.cpu.register8(0).expect("AL register"));
                let address = vector * 4;
                self.cpu.registers.bx = self.cpu.read_word(0, address);
                self.cpu.registers.es = self.cpu.read_word(0, address.wrapping_add(2));
            }
            0x00 => self.exit_code = Some(0),
            0x02 => {
                let byte = self.cpu.registers.dx as u8;
                self.output.push(byte);
                self.teletype(byte);
                self.cpu.set_reg8(0, byte);
            }
            0x06 => {
                let byte = self.cpu.registers.dx as u8;
                if byte == 0xFF {
                    self.console_polls += 1;
                    if let Some(key) = self.keyboard.pop_front() {
                        self.cpu.set_reg8(0, key);
                        self.cpu.set_zero_flag(false);
                    } else {
                        self.cpu.set_reg8(0, 0);
                        self.cpu.set_zero_flag(true);
                    }
                } else {
                    self.output.push(byte);
                    self.teletype(byte);
                    self.cpu.set_reg8(0, byte);
                    self.cpu.set_zero_flag(false);
                }
            }
            0x09 => {
                let segment = self.cpu.registers.ds;
                let mut offset = self.cpu.registers.dx;
                let mut string = Vec::new();
                let mut terminated = false;
                for _ in 0..=u16::MAX {
                    let byte = self.cpu.read_byte(segment, offset);
                    if byte == b'$' {
                        terminated = true;
                        break;
                    }
                    string.push(byte);
                    offset = offset.wrapping_add(1);
                }
                if !terminated {
                    return Err(DosRunError::UnterminatedDollarString);
                }
                self.output.extend(string.iter().copied());
                for byte in string {
                    self.teletype(byte);
                }
                self.cpu.set_reg8(0, b'$');
            }
            0x30 => self.cpu.registers.ax = 0x0002, // DOS 2.0
            0x36 => {
                // The System floppy BPB specifies 720 sectors of 512 bytes,
                // two sectors per cluster, 1 boot, 4 FAT and 7 root sectors.
                // Count the executable and supplied files as occupied; other
                // files in the original image are not modeled by this mount.
                if self.cpu.registers.dx as u8 > 1 {
                    self.cpu.registers.ax = 0xFFFF;
                } else {
                    let total_clusters = 354u16;
                    self.cpu.registers.ax = 1;
                    self.cpu.registers.bx = total_clusters.saturating_sub(
                        self.disk_used_clusters.min(usize::from(total_clusters)) as u16,
                    );
                    self.cpu.registers.cx = 512;
                    self.cpu.registers.dx = total_clusters;
                }
            }
            0x4C => self.exit_code = Some(self.cpu.registers.ax as u8),
            other => return Err(DosRunError::UnsupportedFunction(other)),
        }
        Ok(())
    }

    fn service_video10(&mut self) -> Result<(), DosRunError> {
        match self.cpu.registers.ax.to_be_bytes()[0] {
            0x00 => {
                self.video_mode = self.cpu.register8(0).expect("AL register");
                self.video_page = 0;
                self.cursor = (0, 0);
                self.cpu.write_byte(0x40, 0x49, self.video_mode);
                self.cpu.write_word(0x40, 0x4E, 0);
                self.cpu.write_byte(0x40, 0x62, 0);
                let clear = 16_384;
                for offset in (0..clear).step_by(2) {
                    if self.video_mode == 2 || self.video_mode == 3 {
                        self.cpu.write_byte(0xB800, offset, b' ');
                        self.cpu.write_byte(0xB800, offset + 1, 0x07);
                    } else {
                        self.cpu.write_byte(0xB800, offset, 0);
                        self.cpu.write_byte(0xB800, offset + 1, 0);
                    }
                }
            }
            0x0F => {
                self.cpu.set_reg8(0, self.video_mode);
                self.cpu.set_reg8(4, 80);
                self.cpu.set_reg8(7, 0);
            }
            0x02 => {
                self.cursor = (
                    self.cpu.register8(6).expect("DH register").min(24),
                    self.cpu.register8(2).expect("DL register").min(79),
                );
                let page = u16::from(self.cpu.register8(7).expect("BH register").min(7));
                self.cpu.write_byte(0x40, 0x50 + page * 2, self.cursor.0);
                self.cpu.write_byte(0x40, 0x51 + page * 2, self.cursor.1);
            }
            0x03 => {
                self.cpu.set_reg8(6, self.cursor.0);
                self.cpu.set_reg8(2, self.cursor.1);
                self.cpu.set_reg8(4, 6); // CH: cursor start scan line
                self.cpu.set_reg8(5, 7); // CL: cursor end scan line
            }
            0x05 => {
                self.video_page = self.cpu.register8(0).expect("AL register").min(7);
                self.cpu
                    .write_word(0x40, 0x4E, u16::from(self.video_page) * 0x1000);
                self.cpu.write_byte(0x40, 0x62, self.video_page);
            }
            0x06 | 0x07 => self.scroll_text_window(self.cpu.registers.ax as u8),
            0x0E => self.teletype(self.cpu.register8(0).expect("AL register")),
            0x09 | 0x0A => {
                let character = self.cpu.register8(0).expect("AL register");
                let count = usize::from(self.cpu.registers.cx);
                let attribute = if self.cpu.registers.ax.to_be_bytes()[0] == 0x09 {
                    self.cpu.register8(3).expect("BL register")
                } else {
                    self.read_text_cell(usize::from(self.cursor.0), usize::from(self.cursor.1))
                        .1
                };
                for _ in 0..count {
                    self.write_text_cell(
                        usize::from(self.cursor.0),
                        usize::from(self.cursor.1),
                        character,
                        attribute,
                    );
                }
            }
            function => return Err(DosRunError::UnsupportedVideoFunction(function)),
        }
        Ok(())
    }

    fn scroll_text_window(&mut self, lines: u8) {
        let top = usize::from(self.cpu.register8(5).expect("CH register"));
        let left = usize::from(self.cpu.register8(1).expect("CL register"));
        let bottom = usize::from(self.cpu.register8(6).expect("DH register"));
        let right = usize::from(self.cpu.register8(2).expect("DL register"));
        let attribute = self.cpu.register8(7).expect("BH register");
        if top > bottom || left > right || bottom >= 25 || right >= 80 {
            return;
        }
        let height = bottom - top + 1;
        let count = if lines == 0 {
            height
        } else {
            usize::from(lines).min(height)
        };
        if self.cpu.registers.ax.to_be_bytes()[0] == 0x06 {
            for row in top..=bottom {
                for column in left..=right {
                    let source = row + count;
                    let (character, cell_attribute) = if source <= bottom {
                        self.read_text_cell(source, column)
                    } else {
                        (b' ', attribute)
                    };
                    self.write_text_cell(row, column, character, cell_attribute);
                }
            }
        } else {
            for row in (top..=bottom).rev() {
                for column in left..=right {
                    let source = row.checked_sub(count);
                    let (character, cell_attribute) = source
                        .filter(|source| *source >= top)
                        .map(|source| self.read_text_cell(source, column))
                        .unwrap_or((b' ', attribute));
                    self.write_text_cell(row, column, character, cell_attribute);
                }
            }
        }
    }

    fn read_text_cell(&self, row: usize, column: usize) -> (u8, u8) {
        let offset = u16::from(self.video_page) * 0x1000 + ((row * 80 + column) * 2) as u16;
        (
            self.cpu.read_byte(0xB800, offset),
            self.cpu.read_byte(0xB800, offset + 1),
        )
    }

    fn write_text_cell(&mut self, row: usize, column: usize, character: u8, attribute: u8) {
        let offset = u16::from(self.video_page) * 0x1000 + ((row * 80 + column) * 2) as u16;
        self.cpu.write_byte(0xB800, offset, character);
        self.cpu.write_byte(0xB800, offset + 1, attribute);
    }

    fn teletype(&mut self, character: u8) {
        match character {
            b'\r' => self.cursor.1 = 0,
            b'\n' => self.newline(),
            8 => self.cursor.1 = self.cursor.1.saturating_sub(1),
            7 => {}
            byte => {
                self.write_text_cell(
                    usize::from(self.cursor.0),
                    usize::from(self.cursor.1),
                    byte,
                    0x07,
                );
                self.advance_cursor();
            }
        }
    }

    fn advance_cursor(&mut self) {
        self.cursor.1 += 1;
        if self.cursor.1 >= 80 {
            self.newline();
        }
    }

    fn newline(&mut self) {
        self.cursor.1 = 0;
        self.cursor.0 += 1;
        if self.cursor.0 >= 25 {
            // BIOS teletype scrolls the full text window up by one row.
            for row in 0..24 {
                for column in 0..80 {
                    let cell = self.read_text_cell(row + 1, column);
                    self.write_text_cell(row, column, cell.0, cell.1);
                }
            }
            for column in 0..80 {
                self.write_text_cell(24, column, b' ', 0x07);
            }
            self.cursor.0 = 24;
        }
    }

    fn read_random_fcb_records(
        &mut self,
        requested: usize,
        return_record_count: bool,
    ) -> Result<(), DosRunError> {
        let segment = self.cpu.registers.ds;
        let offset = self.cpu.registers.dx;
        let name = self.fcb_name_key();
        let Some(file) = self.files.get(&name) else {
            self.cpu.set_reg8(0, 1);
            if return_record_count {
                self.cpu.registers.cx = 0;
            }
            return Ok(());
        };
        let record_size = usize::from(self.cpu.read_word(segment, offset.wrapping_add(14)));
        let mut record = u32::from(self.cpu.read_word(segment, offset.wrapping_add(33)))
            | (u32::from(self.cpu.read_byte(segment, offset.wrapping_add(35))) << 16)
            | (u32::from(self.cpu.read_byte(segment, offset.wrapping_add(36))) << 24);
        if requested == 0 {
            self.cpu.set_reg8(0, 0);
            return Ok(());
        }
        if record_size == 0 {
            self.cpu.set_reg8(0, 1);
            if return_record_count {
                self.cpu.registers.cx = 0;
            }
            return Ok(());
        }
        let (dta_segment, dta_offset) = self.dta;
        let mut completed = 0;
        let mut partial = false;
        for output_record in 0..requested {
            let Some(start) = usize::try_from(record)
                .ok()
                .and_then(|record| record.checked_mul(record_size))
            else {
                partial = true;
                break;
            };
            if start >= file.len() {
                partial = true;
                break;
            }
            let available = (file.len() - start).min(record_size);
            let destination = dta_offset.wrapping_add((output_record * record_size) as u16);
            for (index, byte) in file[start..start + available].iter().copied().enumerate() {
                self.cpu
                    .write_byte(dta_segment, destination.wrapping_add(index as u16), byte);
            }
            completed += 1;
            record = record.wrapping_add(1);
            if available < record_size {
                partial = true;
                break;
            }
        }
        // Single-record random reads leave the caller's random pointer in
        // place; block reads advance it by the number transferred.
        self.set_fcb_record_position(
            segment,
            offset,
            if return_record_count {
                record
            } else {
                record.wrapping_sub(completed as u32)
            },
        );
        self.cpu.set_reg8(
            0,
            if partial {
                if completed == 0 {
                    1
                } else {
                    3
                }
            } else {
                0
            },
        );
        if return_record_count {
            self.cpu.registers.cx = completed.min(usize::from(u16::MAX)) as u16;
        }
        Ok(())
    }

    fn write_random_fcb_records(&mut self, requested: usize, return_record_count: bool) {
        const TOTAL_CLUSTERS: usize = 354;
        const CLUSTER_BYTES: usize = 1024;
        let segment = self.cpu.registers.ds;
        let offset = self.cpu.registers.dx;
        let name = self.fcb_name_key();
        let record_size = usize::from(self.cpu.read_word(segment, offset.wrapping_add(14)));
        let mut record = u32::from(self.cpu.read_word(segment, offset.wrapping_add(33)))
            | (u32::from(self.cpu.read_byte(segment, offset.wrapping_add(35))) << 16)
            | (u32::from(self.cpu.read_byte(segment, offset.wrapping_add(36))) << 24);
        if record_size == 0 || !self.files.contains_key(&name) {
            self.cpu.set_reg8(0, 1);
            if return_record_count {
                self.cpu.registers.cx = 0;
            }
            return;
        }

        let mut completed = 0usize;
        let mut status = 0u8;
        let (dta_segment, dta_offset) = self.dta;
        // DOS uses a zero-count block write to set the file length to the
        // random-record position without reading from the DTA.
        let resize_only = return_record_count && requested == 0;
        let count = if resize_only { 1 } else { requested };
        for output_record in 0..count {
            let Some(end) = usize::try_from(record)
                .ok()
                .and_then(|record| record.checked_add(usize::from(!resize_only)))
                .and_then(|record| record.checked_mul(record_size))
                .filter(|end| *end <= u32::MAX as usize)
            else {
                status = 1;
                break;
            };
            let current_size = self.files.get(&name).expect("checked FCB file").len();
            let old_clusters = current_size.div_ceil(CLUSTER_BYTES);
            let new_clusters = end.div_ceil(CLUSTER_BYTES);
            if self.disk_used_clusters - old_clusters + new_clusters > TOTAL_CLUSTERS {
                status = 1;
                break;
            }

            let bytes = if resize_only {
                Vec::new()
            } else {
                let Some(start) = output_record
                    .checked_mul(record_size)
                    .and_then(|delta| usize::from(dta_offset).checked_add(delta))
                    .filter(|start| {
                        start
                            .checked_add(record_size)
                            .is_some_and(|end| end <= 0x10000)
                    })
                else {
                    status = 2;
                    break;
                };
                (0..record_size)
                    .map(|index| self.cpu.read_byte(dta_segment, (start + index) as u16))
                    .collect::<Vec<_>>()
            };
            let file = self.files.get_mut(&name).expect("checked FCB file");
            if resize_only {
                file.resize(end, 0);
            } else {
                let start = end - record_size;
                if file.len() < end {
                    file.resize(end, 0);
                }
                file[start..end].copy_from_slice(&bytes);
                completed += 1;
                // AH=22h leaves the random pointer for the program to move;
                // AH=28h advances it past the records written.
                if return_record_count {
                    record = record.wrapping_add(1);
                }
            }
            self.disk_used_clusters =
                self.disk_used_clusters - old_clusters + file.len().div_ceil(CLUSTER_BYTES);
        }
        let size = self.files.get(&name).expect("checked FCB file").len() as u32;
        self.cpu
            .write_word(segment, offset.wrapping_add(16), size as u16);
        self.cpu
            .write_word(segment, offset.wrapping_add(18), (size >> 16) as u16);
        self.set_fcb_record_position(segment, offset, record);
        self.cpu.set_reg8(0, status);
        if return_record_count {
            self.cpu.registers.cx = completed as u16;
        }
    }

    fn fcb_name_key(&self) -> [u8; 11] {
        let segment = self.cpu.registers.ds;
        let offset = self.cpu.registers.dx;
        (1..=11)
            .map(|index| {
                self.cpu
                    .read_byte(segment, offset.wrapping_add(index))
                    .to_ascii_uppercase()
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("11 FCB name bytes")
    }

    fn set_fcb_record_position(&mut self, segment: u16, offset: u16, record: u32) {
        self.cpu
            .write_word(segment, offset.wrapping_add(33), record as u16);
        self.cpu
            .write_word(segment, offset.wrapping_add(35), (record >> 16) as u16);
        self.cpu
            .write_byte(segment, offset.wrapping_add(32), (record & 0x7f) as u8);
        self.cpu
            .write_word(segment, offset.wrapping_add(12), (record >> 7) as u16);
    }
}

fn initialize_bios_data_area(cpu: &mut Cpu8086) {
    cpu.write_word(0x40, 0x10, 0x0021); // color video and diskette equipment
    cpu.write_word(0x40, 0x13, 640); // conventional memory in KiB
    cpu.write_byte(0x40, 0x49, 3); // 80x25 color text mode
    cpu.write_word(0x40, 0x4A, 80); // text columns
    cpu.write_word(0x40, 0x4C, 0x1000); // bytes per display page
    cpu.write_word(0x40, 0x4E, 0); // active page offset
    for page in 0..8 {
        cpu.write_byte(0x40, 0x50 + page * 2, 0);
        cpu.write_byte(0x40, 0x51 + page * 2, 0);
    }
    cpu.write_byte(0x40, 0x60, 6); // cursor start scan line
    cpu.write_byte(0x40, 0x61, 7); // cursor end scan line
    cpu.write_byte(0x40, 0x62, 0); // active display page
    cpu.write_word(0x40, 0x63, 0x03D4); // color CRTC base port
    for offset in (0..16_384).step_by(2) {
        cpu.write_byte(0xB800, offset, b' ');
        cpu.write_byte(0xB800, offset + 1, 0x07);
    }
}

fn install_host_vector(cpu: &mut Cpu8086, vector: u8, offset: u16) {
    let address = u16::from(vector) * 4;
    cpu.write_word(0, address, offset);
    cpu.write_word(0, address + 2, HOST_VECTOR_SEGMENT);
    cpu.write_byte(HOST_VECTOR_SEGMENT, offset, 0xCF); // IRET if stepped directly
}

fn dos_name_key(name: &str) -> [u8; 11] {
    let (base, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let mut key = [b' '; 11];
    for (slot, byte) in key[..8].iter_mut().zip(base.bytes()) {
        *slot = byte.to_ascii_uppercase();
    }
    for (slot, byte) in key[8..].iter_mut().zip(extension.bytes()) {
        *slot = byte.to_ascii_uppercase();
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mz_with_image(image: &[u8]) -> MzExecutable {
        let file_bytes = 512 + image.len();
        let pages = file_bytes.div_ceil(512);
        let remainder = file_bytes % 512;
        let mut bytes = vec![0u8; file_bytes];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[2..4].copy_from_slice(&(remainder as u16).to_le_bytes());
        bytes[4..6].copy_from_slice(&(pages as u16).to_le_bytes());
        bytes[8..10].copy_from_slice(&32u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
        bytes[16..18].copy_from_slice(&0x0100u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&0u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&0u16.to_le_bytes());
        bytes[512..].copy_from_slice(image);
        MzExecutable::parse(&bytes).unwrap()
    }

    #[test]
    fn int21_4c_terminates_with_the_supplied_exit_code() {
        let executable = mz_with_image(&[0xB4, 0x4C, 0xB0, 0x2A, 0xCD, 0x21]);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        assert_eq!(machine.run(20).unwrap(), 0x2A);
        assert_eq!(machine.exit_code(), Some(0x2A));
        assert!(machine.output().is_empty());
    }

    #[test]
    fn dos_vector_can_be_entered_indirectly_through_the_ivt() {
        let executable = mz_with_image(&[0xF4]);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        assert_eq!(machine.cpu.read_word(0, 0x21 * 4), DOS21_VECTOR_OFFSET);
        assert_eq!(machine.cpu.read_word(0, 0x21 * 4 + 2), HOST_VECTOR_SEGMENT);

        let return_cs = machine.cpu.registers.cs;
        let return_ip = machine.cpu.registers.ip;
        let ss = machine.cpu.registers.ss;
        let sp = machine.cpu.registers.sp.wrapping_sub(6);
        machine.cpu.write_word(ss, sp, return_ip);
        machine.cpu.write_word(ss, sp + 2, return_cs);
        machine.cpu.write_word(ss, sp + 4, 0x0202);
        machine.cpu.registers.sp = sp;
        machine.cpu.registers.cs = HOST_VECTOR_SEGMENT;
        machine.cpu.registers.ip = DOS21_VECTOR_OFFSET;
        machine.cpu.registers.ax = 0x0200;
        machine.cpu.registers.dx = u16::from(b'!');

        assert_eq!(machine.run(1), Err(DosRunError::StepLimit(1)));
        assert_eq!(machine.output(), b"!");
        assert_eq!(
            (machine.cpu.registers.cs, machine.cpu.registers.ip),
            (return_cs, return_ip)
        );
        assert_eq!(machine.cpu.registers.sp, sp + 6);
    }

    #[test]
    fn dos_free_space_reports_mounted_floppy_capacity() {
        let executable = mz_with_image(&[0xF4]);
        let mut machine = DosMachine::load_with_files(
            &executable,
            0x1000,
            0xA000,
            b"",
            [("ACAD.OVL", &[0u8; 513][..])],
        )
        .unwrap();
        machine.cpu.registers.ax = 0x3600;
        machine.cpu.registers.dx = 1;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.registers.ax, 1);
        assert_eq!(machine.cpu.registers.bx, 352);
        assert_eq!(machine.cpu.registers.cx, 512);
        assert_eq!(machine.cpu.registers.dx, 354);

        machine.cpu.registers.ax = 0x3600;
        machine.cpu.registers.dx = 3;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.registers.ax, 0xFFFF);
    }

    #[test]
    fn process_setup_builds_the_environment_block_and_psp_pointer() {
        let executable = mz_with_image(&[0xF4]);
        let machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        let environment = machine.cpu.read_word(0x1000, 0x2C);
        assert_eq!(environment, 0x0FE0);
        assert_eq!(machine.cpu.read_byte(environment, 0), 0);
        assert_eq!(machine.cpu.read_byte(environment, 1), 0);
        assert_eq!(machine.cpu.read_word(environment, 2), 1);
        let program = b"A:\\ACAD.EXE\0";
        let actual: Vec<_> = (4..4 + program.len())
            .map(|offset| machine.cpu.read_byte(environment, offset as u16))
            .collect();
        assert_eq!(actual, program);
    }

    #[test]
    fn fcb_open_finds_system_overlay_and_sets_its_record_metadata() {
        let executable = mz_with_image(&[0xF4]);
        let bytes = [0x5A; 257];
        let mut machine = DosMachine::load_with_files(
            &executable,
            0x1000,
            0xA000,
            b"",
            [("acad.ovl", bytes.as_slice())],
        )
        .unwrap();
        let segment = machine.cpu.registers.ds;
        let fcb = 0x0200;
        for (index, byte) in b"ACAD    OVL".iter().copied().enumerate() {
            machine
                .cpu
                .write_byte(segment, fcb + 1 + index as u16, byte);
        }
        machine.cpu.registers.dx = fcb;
        machine.cpu.registers.ax = 0x0F00;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert_eq!(machine.cpu.read_word(segment, fcb + 12), 0);
        assert_eq!(machine.cpu.read_word(segment, fcb + 14), 128);
        assert_eq!(machine.cpu.read_word(segment, fcb + 16), 257);
        assert_eq!(machine.cpu.read_word(segment, fcb + 18), 0);
    }

    #[test]
    fn fcb_create_truncates_disk_file_and_delete_removes_it() {
        let executable = mz_with_image(&[0xF4]);
        let mut machine = DosMachine::load_with_files(
            &executable,
            0x1000,
            0xA000,
            b"",
            [("ORCNEW.$$$", &[1u8, 2, 3][..])],
        )
        .unwrap();
        let segment = machine.cpu.registers.ds;
        let fcb = 0x0200;
        machine.cpu.write_byte(segment, fcb, 1);
        for (index, byte) in b"ORCNEW  $$$".iter().copied().enumerate() {
            machine
                .cpu
                .write_byte(segment, fcb + 1 + index as u16, byte);
        }
        machine.cpu.registers.dx = fcb;
        machine.cpu.registers.ax = 0x1600;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert_eq!(machine.file("ORCNEW.$$$"), Some(&[][..]));
        assert_eq!(machine.cpu.read_word(segment, fcb + 14), 128);

        machine.cpu.registers.ax = 0x1000;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        machine.cpu.registers.ax = 0x1300;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert!(machine.file("ORCNEW.$$$").is_none());
    }

    #[test]
    fn fcb_random_reads_keep_the_single_record_pointer_and_advance_block_reads() {
        let executable = mz_with_image(&[0xF4]);
        let bytes: Vec<_> = (0..257).map(|byte| byte as u8).collect();
        let mut machine = DosMachine::load_with_files(
            &executable,
            0x1000,
            0xA000,
            b"",
            [("ACAD.OVL", bytes.as_slice())],
        )
        .unwrap();
        let segment = machine.cpu.registers.ds;
        let fcb = 0x0200;
        for (index, byte) in b"ACAD    OVL".iter().copied().enumerate() {
            machine
                .cpu
                .write_byte(segment, fcb + 1 + index as u16, byte);
        }
        machine.cpu.write_word(segment, fcb + 14, 128);
        machine.cpu.write_word(segment, fcb + 33, 1);
        machine.cpu.registers.dx = 0x0300;
        machine.cpu.registers.ax = 0x1A00;
        machine.service_dos21().unwrap();
        machine.cpu.registers.dx = fcb;
        machine.cpu.registers.ax = 0x2100;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert_eq!(machine.cpu.read_word(segment, fcb + 33), 1);
        assert_eq!(machine.cpu.read_byte(segment, 0x0300), 128);
        assert_eq!(machine.cpu.read_byte(segment, 0x037F), 255);

        machine.cpu.write_word(segment, fcb + 33, 2);
        machine.cpu.registers.cx = 2;
        machine.cpu.registers.ax = 0x2700;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(3));
        assert_eq!(machine.cpu.registers.cx, 1);
        assert_eq!(machine.cpu.read_word(segment, fcb + 33), 3);
        assert_eq!(machine.cpu.read_byte(segment, 0x0300), 0);
    }

    #[test]
    fn fcb_random_writes_use_the_dta_and_keep_single_record_pointer() {
        let executable = mz_with_image(&[0xF4]);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        let segment = machine.cpu.registers.ds;
        let fcb = 0x0200;
        machine.cpu.write_byte(segment, fcb, 1);
        for (index, byte) in b"ORCNEW  DWG".iter().copied().enumerate() {
            machine
                .cpu
                .write_byte(segment, fcb + 1 + index as u16, byte);
        }
        machine.cpu.registers.dx = fcb;
        machine.cpu.registers.ax = 0x1600;
        machine.service_dos21().unwrap();
        machine.cpu.write_word(segment, fcb + 14, 4);
        machine.cpu.write_word(segment, fcb + 33, 2);
        machine.dta = (segment, 0x0300);
        for (index, byte) in b"ABCDEFGH".iter().copied().enumerate() {
            machine.cpu.write_byte(segment, 0x0300 + index as u16, byte);
        }

        machine.cpu.registers.ax = 0x2200;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert_eq!(
            machine.file("ORCNEW.DWG"),
            Some(&b"\0\0\0\0\0\0\0\0ABCD"[..])
        );
        assert_eq!(machine.cpu.read_word(segment, fcb + 33), 2);
        assert_eq!(machine.cpu.read_word(segment, fcb + 16), 12);

        machine.cpu.write_word(segment, fcb + 33, 3);
        machine.cpu.registers.cx = 2;
        machine.cpu.registers.ax = 0x2800;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert_eq!(machine.cpu.registers.cx, 2);
        assert_eq!(machine.cpu.read_word(segment, fcb + 33), 5);
        assert_eq!(
            machine.file("ORCNEW.DWG"),
            Some(&b"\0\0\0\0\0\0\0\0ABCDABCDEFGH"[..])
        );

        machine.dta = (segment, 0xFFFE);
        machine.cpu.registers.ax = 0x2200;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(2));
        assert_eq!(machine.cpu.read_word(segment, fcb + 33), 5);
        assert_eq!(machine.file("ORCNEW.DWG").unwrap().len(), 20);

        machine.dta = (segment, 0x0300);
        machine.cpu.write_word(segment, fcb + 14, 128);
        machine.cpu.write_word(segment, fcb + 33, 3000);
        machine.cpu.registers.ax = 0x2200;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(1));
        assert_eq!(machine.file("ORCNEW.DWG").unwrap().len(), 20);

        machine.cpu.write_word(segment, fcb + 14, 4);
        machine.cpu.write_word(segment, fcb + 33, 1);
        machine.cpu.registers.cx = 0;
        machine.cpu.registers.ax = 0x2800;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert_eq!(machine.cpu.registers.cx, 0);
        assert_eq!(machine.cpu.read_word(segment, fcb + 33), 1);
        assert_eq!(machine.file("ORCNEW.DWG"), Some(&b"\0\0\0\0"[..]));
    }

    #[test]
    fn fcb_rename_moves_the_saved_file_without_changing_its_bytes() {
        let executable = mz_with_image(&[0xF4]);
        let mut machine = DosMachine::load_with_files(
            &executable,
            0x1000,
            0xA000,
            b"",
            [("ORCNEW.$$$", &b"AC1.40"[..])],
        )
        .unwrap();
        let segment = machine.cpu.registers.ds;
        let fcb = 0x0200;
        for (index, byte) in b"ORCNEW  $$$".iter().copied().enumerate() {
            machine
                .cpu
                .write_byte(segment, fcb + 1 + index as u16, byte);
        }
        for (index, byte) in b"ORCNEW  DWG".iter().copied().enumerate() {
            machine
                .cpu
                .write_byte(segment, fcb + 17 + index as u16, byte);
        }
        machine.cpu.registers.dx = fcb;
        machine.cpu.registers.ax = 0x1700;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        assert!(machine.file("ORCNEW.$$$").is_none());
        assert_eq!(machine.file("ORCNEW.DWG"), Some(&b"AC1.40"[..]));
    }

    #[test]
    fn dos_console_poll_reports_empty_then_returns_queued_key() {
        let image = [
            0xB4, 0x06, // MOV AH,06h
            0xB2, 0xFF, // MOV DL,FFh (poll)
            0xCD, 0x21, // INT 21h
            0x74, 0x02, // JZ to exit 0 when no key is queued
            0xB0, 0x01, // otherwise select exit code 1
            0xB4, 0x4C, 0xCD, 0x21, // terminate with AL
        ];
        let executable = mz_with_image(&image);
        let mut empty = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        assert_eq!(empty.run(10).unwrap(), 0);
        assert_eq!(empty.console_polls(), 1);

        let mut queued = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        queued.push_input([b'Q']);
        assert_eq!(queued.run(10).unwrap(), 1);
        assert_eq!(queued.console_polls(), 1);

        // An extended key's first byte is NUL but still counts as input.
        let mut extended = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        extended.push_input([0]);
        assert_eq!(extended.run(10).unwrap(), 1);
    }

    #[test]
    fn cga_status_poll_observes_vertical_retrace() {
        let image = [
            0xBA, 0xDA, 0x03, // MOV DX,03DAh
            0xEC, // poll: IN AL,DX
            0x24, 0x08, // AND AL,08h
            0x74, 0xFB, // JZ poll
            0xB8, 0, 0x4C, // MOV AX,4C00h
            0xCD, 0x21, // INT 21h
        ];
        let executable = mz_with_image(&image);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        assert_eq!(machine.run(2_000).unwrap(), 0);
        assert_ne!(machine.cpu.read_port_byte(0x03DA) & 8, 0);
    }

    #[test]
    fn bios_video_sets_mode_page_and_scrolls_text_cells() {
        let executable = mz_with_image(&[0xF4]);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        machine.cpu.registers.ax = 0x0003;
        machine.service_video10().unwrap();
        assert_eq!(machine.video_mode(), 3);
        assert_eq!(machine.cpu.read_byte(0x40, 0x49), 3);
        assert_eq!(machine.cpu.read_byte(0xB800, 0), b' ');

        machine.cpu.registers.ax = 0x0502;
        machine.service_video10().unwrap();
        assert_eq!(machine.video_page(), 2);
        assert_eq!(machine.cpu.read_byte(0x40, 0x62), 2);
        assert_eq!(machine.cpu.read_word(0x40, 0x4E), 0x2000);

        machine.write_text_cell(1, 0, b'A', 0x1F);
        assert_eq!(machine.read_text_cell(1, 0), (b'A', 0x1F));
        machine.cpu.registers.ax = 0x0601;
        machine.cpu.set_reg8(7, 0x1F); // BH scroll fill attribute
        machine.cpu.set_reg8(5, 0); // CH top row
        machine.cpu.set_reg8(1, 0); // CL left column
        machine.cpu.set_reg8(6, 1); // DH bottom row
        machine.cpu.set_reg8(2, 0); // DL right column
        machine.service_video10().unwrap();
        assert_eq!(machine.read_text_cell(0, 0), (b'A', 0x1F));
        assert_eq!(machine.read_text_cell(1, 0), (b' ', 0x1F));
    }

    #[test]
    fn dos_function09_writes_a_dollar_terminated_string_and_returns() {
        // MOV AH,09h; MOV DX,010Ch; INT 21h; MOV AX,4C00h; INT 21h; "HELLO$"
        let image = [
            0xB4, 0x09, 0xBA, 0x0C, 0x01, 0xCD, 0x21, 0xB8, 0, 0x4C, 0xCD, 0x21, b'H', b'E', b'L',
            b'L', b'O', b'$',
        ];
        let executable = mz_with_image(&image);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        assert_eq!(machine.run(20).unwrap(), 0);
        assert_eq!(machine.output(), b"HELLO");
    }

    #[test]
    fn dos_function02_outputs_the_character_in_dl() {
        let executable =
            mz_with_image(&[0xB4, 2, 0xB2, b'!', 0xCD, 0x21, 0xB8, 0, 0x4C, 0xCD, 0x21]);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        assert_eq!(machine.run(20).unwrap(), 0);
        assert_eq!(machine.output(), b"!");
    }

    #[test]
    fn unsupported_dos_services_stop_instead_of_faking_success() {
        let executable = mz_with_image(&[0xB4, 0x3D, 0xCD, 0x21]); // DOS open-file handle
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        assert_eq!(machine.run(20), Err(DosRunError::UnsupportedFunction(0x3D)));
    }

    #[test]
    fn console_device_fcb_open_create_and_delete_return_dos_status() {
        let executable = mz_with_image(&[0xF4]);
        let mut machine = DosMachine::load(&executable, 0x1000, 0xA000, b"").unwrap();
        let segment = machine.cpu.registers.ds;
        let offset = 0x0200;
        machine.cpu.write_byte(segment, offset, 0);
        for (index, byte) in b"CON:       ".iter().copied().enumerate() {
            machine
                .cpu
                .write_byte(segment, offset + 1 + index as u16, byte);
        }
        machine.cpu.registers.dx = offset;
        machine.cpu.registers.ax = 0x0F00;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
        machine.cpu.registers.ax = 0x1300;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0xFF));
        machine.cpu.registers.ax = 0x1600;
        machine.service_dos21().unwrap();
        assert_eq!(machine.cpu.register8(0), Some(0));
    }
}
