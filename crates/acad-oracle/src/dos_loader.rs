//! DOS process setup for an MZ executable: PSP, allocation, and entry state.

use crate::{cpu8086::Cpu8086, mz_loader::MzExecutable};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DosLoadError {
    InvalidMemoryRange,
    InsufficientMemory { required: u32, available: u32 },
    CommandTailTooLong,
    Executable(crate::mz_loader::MzError),
}

impl fmt::Display for DosLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMemoryRange => write!(f, "invalid DOS process memory range"),
            Self::InsufficientMemory {
                required,
                available,
            } => write!(
                f,
                "DOS executable needs {required} paragraphs, only {available} are available"
            ),
            Self::CommandTailTooLong => write!(f, "DOS command tail exceeds 126 bytes"),
            Self::Executable(error) => write!(f, "cannot load DOS executable: {error}"),
        }
    }
}

impl std::error::Error for DosLoadError {}

impl From<crate::mz_loader::MzError> for DosLoadError {
    fn from(value: crate::mz_loader::MzError) -> Self {
        Self::Executable(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DosProcess {
    pub psp_segment: u16,
    pub load_segment: u16,
    pub end_segment: u16,
}

impl DosProcess {
    /// Load an MZ process into an available real-mode paragraph range and
    /// initialize the DOS fields AutoCAD's startup code can inspect.
    pub fn load(
        cpu: &mut Cpu8086,
        executable: &MzExecutable,
        psp_segment: u16,
        top_segment: u16,
        parent_psp: u16,
        environment_segment: u16,
        command_tail: &[u8],
    ) -> Result<Self, DosLoadError> {
        if command_tail.len() > 126 {
            return Err(DosLoadError::CommandTailTooLong);
        }
        let load_segment = psp_segment
            .checked_add(0x10)
            .ok_or(DosLoadError::InvalidMemoryRange)?;
        if top_segment <= load_segment {
            return Err(DosLoadError::InvalidMemoryRange);
        }
        let available = u32::from(top_segment - load_segment);
        let image_paragraphs = u32::try_from(executable.image_bytes.div_ceil(16))
            .map_err(|_| DosLoadError::InvalidMemoryRange)?;
        let required = image_paragraphs + u32::from(executable.min_extra_paragraphs);
        if available < required {
            return Err(DosLoadError::InsufficientMemory {
                required,
                available,
            });
        }
        let max_extra = if executable.max_extra_paragraphs == u16::MAX {
            available - image_paragraphs
        } else {
            (available - image_paragraphs).min(u32::from(executable.max_extra_paragraphs))
        };
        let end_segment = u32::from(load_segment) + image_paragraphs + max_extra;
        let end_segment =
            u16::try_from(end_segment).map_err(|_| DosLoadError::InvalidMemoryRange)?;

        executable.load_at(cpu, load_segment)?;
        cpu.registers.ds = psp_segment;
        cpu.registers.es = psp_segment;

        // INT 20h at PSP:0000 is the DOS termination entry.
        cpu.write_byte(psp_segment, 0x00, 0xCD);
        cpu.write_byte(psp_segment, 0x01, 0x20);
        cpu.write_word(psp_segment, 0x02, end_segment);
        cpu.write_word(psp_segment, 0x16, parent_psp);
        cpu.write_word(psp_segment, 0x2C, environment_segment);
        cpu.write_byte(psp_segment, 0x80, command_tail.len() as u8);
        for (index, byte) in command_tail.iter().copied().enumerate() {
            cpu.write_byte(psp_segment, 0x81 + index as u16, byte);
        }
        cpu.write_byte(psp_segment, 0x81 + command_tail.len() as u16, b'\r');

        Ok(Self {
            psp_segment,
            load_segment,
            end_segment,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable() -> MzExecutable {
        let mut bytes = vec![0u8; 516];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[2..4].copy_from_slice(&4u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&32u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&2u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
        bytes[16..18].copy_from_slice(&0x0100u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&0u16.to_le_bytes()); // IP
        bytes[22..24].copy_from_slice(&0u16.to_le_bytes()); // CS
        bytes[512] = 0xF4;
        MzExecutable::parse(&bytes).unwrap()
    }

    #[test]
    fn builds_psp_allocates_remaining_paragraphs_and_sets_entry_segments() {
        let mut cpu = Cpu8086::new();
        let process = DosProcess::load(
            &mut cpu,
            &executable(),
            0x1000,
            0x2000,
            0x0800,
            0x0700,
            b"TEST ARG",
        )
        .unwrap();
        assert_eq!(process.load_segment, 0x1010);
        assert_eq!(process.end_segment, 0x2000);
        assert_eq!((cpu.registers.cs, cpu.registers.ip), (0x1010, 0));
        assert_eq!((cpu.registers.ds, cpu.registers.es), (0x1000, 0x1000));
        assert_eq!((cpu.registers.ss, cpu.registers.sp), (0x1010, 0x0100));
        assert_eq!(cpu.read_byte(0x1000, 0), 0xCD);
        assert_eq!(cpu.read_byte(0x1000, 1), 0x20);
        assert_eq!(cpu.read_word(0x1000, 2), 0x2000);
        assert_eq!(cpu.read_word(0x1000, 0x16), 0x0800);
        assert_eq!(cpu.read_word(0x1000, 0x2C), 0x0700);
        assert_eq!(cpu.read_byte(0x1000, 0x80), 8);
        assert_eq!(
            &[0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88].map(|at| cpu.read_byte(0x1000, at)),
            b"TEST ARG"
        );
        assert_eq!(cpu.read_byte(0x1000, 0x89), b'\r');
    }

    #[test]
    fn refuses_memory_ranges_below_the_executable_minimum() {
        let mut cpu = Cpu8086::new();
        let result = DosProcess::load(&mut cpu, &executable(), 0x1000, 0x1011, 0, 0, b"");
        assert!(matches!(
            result,
            Err(DosLoadError::InsufficientMemory {
                required: 3,
                available: 1
            })
        ));
    }

    #[test]
    fn rejects_overlong_command_tails_before_mutating_memory() {
        let mut cpu = Cpu8086::new();
        let tail = vec![b'X'; 127];
        assert_eq!(
            DosProcess::load(&mut cpu, &executable(), 0x1000, 0x2000, 0, 0, &tail),
            Err(DosLoadError::CommandTailTooLong)
        );
        assert_eq!(cpu.read_byte(0x1000, 0), 0);
    }
}
