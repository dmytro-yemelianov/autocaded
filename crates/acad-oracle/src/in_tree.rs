//! Run the original executable with the in-tree 8086 and DOS services.

use crate::{
    dos_machine::{DosMachine, DosRunError},
    fat12::fat12_file,
    mz_loader::MzExecutable,
};
use std::fs;
use std::path::Path;

const SYSTEM_FILES: &[&str] = &[
    "IBMBIO.COM",
    "IBMDOS.COM",
    "COMMAND.COM",
    "ACAD.OVL",
    "ACAD.CFG",
    "ACAD.HLP",
    "ACAD.MNU",
    "ACAD.PAT",
    "TXT.SHP",
    "README.DOC",
    "AUTOEXEC.BAT",
    "ACAD.BAK",
];
const STARTUP_INSTRUCTIONS: usize = 10_000_000;
const SCRIPT_CHUNK_INSTRUCTIONS: usize = 1_000_000;
const MAX_SCRIPT_CHUNKS: usize = 40;

/// Original DWG bytes and the editor's CGA frame before END.
pub struct InTreeVisualProbe {
    pub dwg: Vec<u8>,
    pub cga: Vec<u8>,
}

/// Script a new AutoCAD 1.4 drawing and return the original's DWG bytes.
/// The input disk is read only. The runner currently supports the System
/// floppy's root files and the DOS/BIOS calls exercised by tested commands.
pub fn generate_dwg_in_tree(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<Vec<u8>, String> {
    run_in_tree(system_disk, name, editor_lines, false).map(|(dwg, _)| dwg)
}

/// Capture the original's 16 KiB CGA display after the scripted commands,
/// then save the drawing. The input disk is read only.
pub fn generate_visual_dwg_in_tree(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<InTreeVisualProbe, String> {
    let (dwg, cga) = run_in_tree(system_disk, name, editor_lines, true)?;
    Ok(InTreeVisualProbe {
        dwg,
        cga: cga.expect("capture requested"),
    })
}

fn run_in_tree(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
    capture: bool,
) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    if name.is_empty()
        || name.len() > 8
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err("drawing name must be 1–8 uppercase ASCII letters or digits".into());
    }
    if editor_lines
        .iter()
        .any(|line| !line.is_ascii() || line.bytes().any(|byte| byte == b'\r' || byte == b'\n'))
    {
        return Err("editor lines must be single-line ASCII".into());
    }
    let disk = fs::read(system_disk)
        .map_err(|error| format!("read {}: {error}", system_disk.display()))?;
    let drawing_name = format!("{name}.DWG");
    if fat12_file(&disk, &drawing_name).is_ok() {
        return Err(format!(
            "{drawing_name} already exists on the System floppy"
        ));
    }
    let executable = fat12_file(&disk, "ACAD.EXE")?;
    let mz = MzExecutable::parse(&executable).map_err(|error| error.to_string())?;
    let files = SYSTEM_FILES
        .iter()
        .map(|name| fat12_file(&disk, name).map(|bytes| (*name, bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut machine = DosMachine::load_with_files(
        &mz,
        0x1000,
        0xA000,
        b"",
        files.iter().map(|(name, bytes)| (*name, bytes.as_slice())),
    )
    .map_err(|error| error.to_string())?;
    match machine.run(STARTUP_INSTRUCTIONS) {
        Err(DosRunError::StepLimit(_)) if machine.console_polls() > 0 => {}
        other => return Err(format!("AutoCAD did not reach the menu: {other:?}")),
    }

    let mut script = format!("1\r{name}\r").into_bytes();
    for line in editor_lines {
        script.extend_from_slice(line.as_bytes());
        script.push(b'\r');
    }
    if !capture {
        script.extend_from_slice(b"END\r");
    }
    machine.push_input(script);
    let cga = if capture {
        let mut previous = Vec::new();
        let mut stable = 0;
        let mut captured = None;
        for _ in 0..MAX_SCRIPT_CHUNKS {
            match machine.run(SCRIPT_CHUNK_INSTRUCTIONS) {
                Err(DosRunError::StepLimit(_)) => {}
                other => return Err(format!("AutoCAD stopped before CGA capture: {other:?}")),
            }
            if machine.pending_input() != 0 {
                stable = 0;
                continue;
            }
            let frame = machine.cga_memory();
            stable = if frame == previous { stable + 1 } else { 0 };
            if stable >= 2 {
                captured = Some(frame);
                break;
            }
            previous = frame;
        }
        let frame = captured.ok_or("AutoCAD editor CGA frame did not stabilize")?;
        machine.push_input(b"END\r".iter().copied());
        Some(frame)
    } else {
        None
    };
    for _ in 0..MAX_SCRIPT_CHUNKS {
        match machine.run(SCRIPT_CHUNK_INSTRUCTIONS) {
            Err(DosRunError::StepLimit(_)) => {}
            other => {
                return Err(format!(
                    "AutoCAD stopped before saving {drawing_name}: {other:?}"
                ))
            }
        }
        if let Some(drawing) = machine.file(&drawing_name) {
            return Ok((drawing.to_vec(), cga));
        }
    }
    Err(format!(
        "AutoCAD did not save {drawing_name} within the instruction limit"
    ))
}

/// One fixed-size slice of guest execution observed by [`observe_in_tree`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InTreeSlice {
    /// Keys still queued for DOS console input after this slice.
    pub pending_input: usize,
    /// Cumulative DOS console polls (INT 21h/AH=06h) after this slice.
    pub console_polls: u64,
    /// How many of the requested watch files exist after this slice.
    pub watched_present: usize,
}

/// Observations of the original after scripted keyboard phases.
pub struct InTreeObservation {
    /// One entry per `slice_instructions` slice, for each phase in order.
    pub phases: Vec<Vec<InTreeSlice>>,
    /// Stop condition when the run ended before all slices: an exit, an
    /// unsupported DOS/BIOS service or a CPU fault.
    pub stopped: Option<String>,
    /// Guest console output (DOS character writes), lossy ASCII.
    pub console: String,
    /// Each of the requested `watch_files` present after the run (for
    /// example a drawing saved by END), by the requested DOS name.
    pub created: Vec<(String, Vec<u8>)>,
    /// The 16 KiB CGA memory (`B800:0000`) when the run ended.
    pub cga: Vec<u8>,
}

/// Boot the original with `command_tail` and `extra_files` added to the
/// System floppy's root files. Starting at the first instruction, push each
/// phase's keys and record its `slices` slices of `slice_instructions`
/// emulated instructions (a first phase with no keys observes startup). Instruction counts are the emulator's only clock;
/// no host time is consulted. The input disk is read only.
pub fn observe_in_tree(
    system_disk: &Path,
    command_tail: &[u8],
    extra_files: &[(&str, &[u8])],
    phases: &[(&[u8], usize)],
    slice_instructions: usize,
    watch_files: &[&str],
) -> Result<InTreeObservation, String> {
    let disk = fs::read(system_disk)
        .map_err(|error| format!("read {}: {error}", system_disk.display()))?;
    let executable = fat12_file(&disk, "ACAD.EXE")?;
    let mz = MzExecutable::parse(&executable).map_err(|error| error.to_string())?;
    let mut files = SYSTEM_FILES
        .iter()
        .map(|name| fat12_file(&disk, name).map(|bytes| ((*name).to_owned(), bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    files.extend(
        extra_files
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec())),
    );
    let mut machine = DosMachine::load_with_files(
        &mz,
        0x1000,
        0xA000,
        command_tail,
        files
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    )
    .map_err(|error| error.to_string())?;
    let mut observation = InTreeObservation {
        phases: Vec::new(),
        stopped: None,
        console: String::new(),
        created: Vec::new(),
        cga: Vec::new(),
    };
    'phases: for (keys, slices) in phases {
        if observation.stopped.is_some() {
            break;
        }
        machine.push_input(keys.iter().copied());
        let mut phase = Vec::new();
        for _ in 0..*slices {
            let result = machine.run(slice_instructions);
            phase.push(InTreeSlice {
                pending_input: machine.pending_input(),
                console_polls: machine.console_polls(),
                watched_present: watch_files
                    .iter()
                    .filter(|name| machine.file(name).is_some())
                    .count(),
            });
            if !matches!(result, Err(DosRunError::StepLimit(_))) {
                observation.stopped = Some(format!("{result:?}"));
                observation.phases.push(phase);
                break 'phases;
            }
        }
        observation.phases.push(phase);
    }
    observation.console = String::from_utf8_lossy(machine.output()).into_owned();
    observation.cga = machine.cga_memory();
    for name in watch_files {
        if let Some(bytes) = machine.file(name) {
            observation
                .created
                .push(((*name).to_owned(), bytes.to_vec()));
        }
    }
    Ok(observation)
}

/// The original editor's 25 rows of 80 text cells, read from a graphics
/// [`InTreeObservation::cga`] frame with the original's own 8×8 display font
/// (the ASCII-indexed table in `ACAD.OVL`). Cells that match no printable
/// glyph read as `?`; trailing spaces are trimmed.
pub fn editor_text_rows(system_disk: &Path, cga: &[u8]) -> Result<Vec<String>, String> {
    let disk = fs::read(system_disk)
        .map_err(|error| format!("read {}: {error}", system_disk.display()))?;
    let overlay = fat12_file(&disk, "ACAD.OVL")?;
    // Locate the table by its `A` glyph, then require a blank space glyph.
    const GLYPH_A: [u8; 8] = [0x30, 0x78, 0xCC, 0xCC, 0xFC, 0xCC, 0xCC, 0x00];
    let base = overlay
        .windows(8)
        .position(|window| window == GLYPH_A)
        .and_then(|at| at.checked_sub(usize::from(b'A') * 8))
        .filter(|base| overlay.get(base + 0x20 * 8..base + 0x21 * 8) == Some(&[0; 8][..]))
        .filter(|base| overlay.len() >= base + 0x7F * 8)
        .ok_or("ACAD.OVL display font not found")?;
    let frame = crate::cga::Frame::new(cga)?;
    Ok((0..25)
        .map(|row| {
            let text: String = (0..80)
                .map(|column| {
                    let cell: Vec<u8> = (0..8)
                        .map(|line| {
                            (0..8).fold(0u8, |bits, x| {
                                bits << 1 | u8::from(frame.lit(column * 8 + x, row * 8 + line))
                            })
                        })
                        .collect();
                    (0x20u8..0x7F)
                        .find(|&code| {
                            let at = base + usize::from(code) * 8;
                            overlay[at..at + 8] == cell[..]
                        })
                        .map_or('?', char::from)
                })
                .collect();
            text.trim_end().to_owned()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_input_is_rejected_before_disk_access() {
        let missing = Path::new("/missing-System.img");
        assert!(generate_dwg_in_tree(missing, "TOO-LONG-NAME", &[]).is_err());
        assert!(generate_dwg_in_tree(missing, "VALID", &["LINE\nEND"]).is_err());
    }
}
