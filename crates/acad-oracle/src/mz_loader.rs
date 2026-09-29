//! Minimal DOS MZ executable parsing and relocation/loading support.

use crate::cpu8086::{Cpu8086, MEMORY_SIZE};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MzError {
    TruncatedHeader,
    InvalidSignature,
    InvalidDeclaredSize,
    HeaderPastEnd,
    RelocationTablePastHeader,
    RelocationOutsideImage { segment: u16, offset: u16 },
    ImageTooLarge,
}

impl fmt::Display for MzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader => write!(f, "MZ executable header is shorter than 28 bytes"),
            Self::InvalidSignature => write!(f, "file does not begin with the MZ signature"),
            Self::InvalidDeclaredSize => write!(f, "MZ header declares an invalid file size"),
            Self::HeaderPastEnd => write!(f, "MZ header extends beyond the declared file size"),
            Self::RelocationTablePastHeader => {
                write!(f, "MZ relocation table extends beyond the header")
            }
            Self::RelocationOutsideImage { segment, offset } => write!(
                f,
                "MZ relocation target {segment:04x}:{offset:04x} lies outside the image"
            ),
            Self::ImageTooLarge => write!(f, "MZ load module exceeds the 8086 address space"),
        }
    }
}

impl std::error::Error for MzError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Relocation {
    offset: u16,
    segment: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MzExecutable {
    pub initial_ip: u16,
    pub initial_cs: u16,
    pub initial_sp: u16,
    pub initial_ss: u16,
    pub min_extra_paragraphs: u16,
    pub max_extra_paragraphs: u16,
    /// Load-module length declared by the MZ header, excluding the header.
    pub image_bytes: usize,
    /// Total file length declared by the MZ header.
    pub file_bytes: usize,
    relocations: Vec<Relocation>,
    image: Vec<u8>,
}

impl MzExecutable {
    /// Parse a DOS MZ executable. DOS reads the final 512-byte page in full,
    /// so bytes beyond the declared end but within that page remain resident.
    /// Bytes after the final declared page are not part of the load module.
    pub fn parse(bytes: &[u8]) -> Result<Self, MzError> {
        if bytes.len() < 28 {
            return Err(MzError::TruncatedHeader);
        }
        if &bytes[..2] != b"MZ" {
            return Err(MzError::InvalidSignature);
        }
        let word = |at| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let last_page_bytes = usize::from(word(2));
        let pages = usize::from(word(4));
        let file_bytes = match (pages, last_page_bytes) {
            (0, _) => return Err(MzError::InvalidDeclaredSize),
            (_, 0) => pages.checked_mul(512).ok_or(MzError::InvalidDeclaredSize)?,
            (_, last) if last <= 511 => (pages - 1)
                .checked_mul(512)
                .and_then(|n| n.checked_add(last))
                .ok_or(MzError::InvalidDeclaredSize)?,
            _ => return Err(MzError::InvalidDeclaredSize),
        };
        let relocation_count = usize::from(word(6));
        let header_bytes = usize::from(word(8))
            .checked_mul(16)
            .ok_or(MzError::HeaderPastEnd)?;
        if file_bytes > bytes.len() || file_bytes < 28 {
            return Err(MzError::InvalidDeclaredSize);
        }
        if header_bytes < 28 || header_bytes > file_bytes {
            return Err(MzError::HeaderPastEnd);
        }
        let relocation_at = usize::from(word(24));
        let relocation_end = relocation_at
            .checked_add(
                relocation_count
                    .checked_mul(4)
                    .ok_or(MzError::RelocationTablePastHeader)?,
            )
            .ok_or(MzError::RelocationTablePastHeader)?;
        if relocation_count != 0 && (relocation_at < 28 || relocation_end > header_bytes) {
            return Err(MzError::RelocationTablePastHeader);
        }
        let image_bytes = file_bytes - header_bytes;
        if image_bytes > MEMORY_SIZE {
            return Err(MzError::ImageTooLarge);
        }
        let mut relocations = Vec::with_capacity(relocation_count);
        for i in 0..relocation_count {
            let at = relocation_at + i * 4;
            relocations.push(Relocation {
                offset: u16::from_le_bytes([bytes[at], bytes[at + 1]]),
                segment: u16::from_le_bytes([bytes[at + 2], bytes[at + 3]]),
            });
        }
        let loaded_end = file_bytes
            .div_ceil(512)
            .saturating_mul(512)
            .min(bytes.len());
        let image = bytes[header_bytes..loaded_end].to_vec();
        for relocation in &relocations {
            let target = usize::from(relocation.segment) * 16 + usize::from(relocation.offset);
            if target + 2 > image.len() {
                return Err(MzError::RelocationOutsideImage {
                    segment: relocation.segment,
                    offset: relocation.offset,
                });
            }
        }
        Ok(Self {
            initial_ip: word(20),
            initial_cs: word(22),
            initial_sp: word(16),
            initial_ss: word(14),
            min_extra_paragraphs: word(10),
            max_extra_paragraphs: word(12),
            image_bytes,
            file_bytes,
            relocations,
            image,
        })
    }

    pub fn relocation_count(&self) -> usize {
        self.relocations.len()
    }

    /// Load the image at `load_segment`, apply its relocation table, and set
    /// CS:IP and SS:SP as DOS does. The caller sets DS/ES to the PSP segment.
    pub fn load_at(&self, cpu: &mut Cpu8086, load_segment: u16) -> Result<(), MzError> {
        if self.image.len() > MEMORY_SIZE {
            return Err(MzError::ImageTooLarge);
        }
        cpu.load(load_segment, 0, &self.image);
        for relocation in &self.relocations {
            let segment = load_segment.wrapping_add(relocation.segment);
            let value = cpu.read_word(segment, relocation.offset);
            cpu.write_word(segment, relocation.offset, value.wrapping_add(load_segment));
        }
        cpu.registers.cs = load_segment.wrapping_add(self.initial_cs);
        cpu.registers.ip = self.initial_ip;
        cpu.registers.ss = load_segment.wrapping_add(self.initial_ss);
        cpu.registers.sp = self.initial_sp;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_mz() -> Vec<u8> {
        let mut bytes = vec![0u8; 516];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[2..4].copy_from_slice(&4u16.to_le_bytes()); // 512 + 4 bytes
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&1u16.to_le_bytes()); // one relocation
        bytes[8..10].copy_from_slice(&32u16.to_le_bytes()); // 512-byte header
        bytes[10..12].copy_from_slice(&2u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
        bytes[14..16].copy_from_slice(&0x20u16.to_le_bytes());
        bytes[16..18].copy_from_slice(&0x100u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&2u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&0x10u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&28u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&2u16.to_le_bytes());
        bytes[30..32].copy_from_slice(&0u16.to_le_bytes());
        bytes[512..516].copy_from_slice(&[0xF4, 0, 0x34, 0x12]);
        bytes
    }

    #[test]
    fn parses_declared_size_entrypoint_stack_and_relocations() {
        let mut bytes = tiny_mz();
        bytes.extend_from_slice(&[0xAA; 20]); // bytes in the last page are resident
        let mz = MzExecutable::parse(&bytes).unwrap();
        assert_eq!(mz.file_bytes, 516);
        assert_eq!(mz.image_bytes, 4);
        let mut cpu = Cpu8086::new();
        mz.load_at(&mut cpu, 0x1000).unwrap();
        assert_eq!(cpu.read_byte(0x1000, 4), 0xAA);
        assert_eq!(cpu.read_byte(0x1000, 23), 0xAA);
        assert_eq!((mz.initial_cs, mz.initial_ip), (0x10, 2));
        assert_eq!((mz.initial_ss, mz.initial_sp), (0x20, 0x100));
        assert_eq!(mz.min_extra_paragraphs, 2);
        assert_eq!(mz.max_extra_paragraphs, 0xFFFF);
    }

    #[test]
    fn load_applies_relocations_and_sets_the_dos_entry_registers() {
        let mz = MzExecutable::parse(&tiny_mz()).unwrap();
        let mut cpu = Cpu8086::new();
        mz.load_at(&mut cpu, 0x1000).unwrap();
        assert_eq!((cpu.registers.cs, cpu.registers.ip), (0x1010, 2));
        assert_eq!((cpu.registers.ss, cpu.registers.sp), (0x1020, 0x100));
        assert_eq!(cpu.read_word(0x1000, 2), 0x2234);
        assert_eq!(cpu.read_byte(0x1000, 0), 0xF4);
    }

    #[test]
    fn only_bytes_from_the_final_declared_page_can_remain_resident() {
        let mut bytes = tiny_mz();
        bytes.extend_from_slice(&[0xAA; 508]);
        bytes.extend_from_slice(&[0xBB; 10]);
        let mz = MzExecutable::parse(&bytes).unwrap();
        let mut cpu = Cpu8086::new();
        mz.load_at(&mut cpu, 0x1000).unwrap();
        assert_eq!(cpu.read_byte(0x1000, 4), 0xAA);
        assert_eq!(cpu.read_byte(0x1000, 511), 0xAA);
        assert_eq!(cpu.read_byte(0x1000, 512), 0);
    }

    #[test]
    fn malformed_mz_sizes_and_relocations_are_rejected() {
        let mut bytes = tiny_mz();
        bytes[2..4].copy_from_slice(&512u16.to_le_bytes());
        assert_eq!(
            MzExecutable::parse(&bytes),
            Err(MzError::InvalidDeclaredSize)
        );

        let mut bytes = tiny_mz();
        bytes[28..30].copy_from_slice(&0xFFFFu16.to_le_bytes());
        assert!(matches!(
            MzExecutable::parse(&bytes),
            Err(MzError::RelocationOutsideImage { .. })
        ));
    }
}
