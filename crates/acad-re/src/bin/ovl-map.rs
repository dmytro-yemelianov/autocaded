//! Emits the Ghidra loader's input: where every overlay region lives in the
//! file and where it belongs in memory. The PyGhidra scripts read this and
//! never decode the 1983 container format themselves.
use acad_re::ovl;
use serde::Serialize;
use std::{fs, path::PathBuf, process::ExitCode};

/// `ACAD.EXE`'s image loads at this paragraph in Ghidra (its MZ loader default).
const IMAGE_SEG: u32 = 0x1000;
/// `DS = SS = image + 0x0E15` paragraphs (spec §4.1).
const DATA_SEG: u32 = IMAGE_SEG + 0x0e15;
/// Byte offset in the image where the data segment starts.
const DATA_OFF: u32 = 0x0e15 * 16;
/// The real image, excluding 508 bytes of cluster slack.
const IMAGE_LEN: u32 = 153 * 512 + 4;
/// The code window is allocated above the image; the first free paragraph.
const WINDOW_SEG: u32 = IMAGE_SEG + 0x1321;

#[derive(Serialize)]
struct Block {
    name: String,
    seg: u32,
    off: u32,
    len: u32,
    /// `None` for blocks filled from `ACAD.EXE`, `Some` for overlay regions.
    src_off: Option<u32>,
    overlay: bool,
}

#[derive(Serialize)]
struct Map {
    exe: PathBuf,
    ovl: PathBuf,
    image_len: u32,
    window_bytes: u16,
    blocks: Vec<Block>,
    entry_points: Vec<EntryPoint>,
}

#[derive(Serialize)]
struct EntryPoint {
    entry: usize,
    seg: u32,
    off: u32,
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(exe), Some(ovl_path), Some(out)) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: ovl-map <ACAD.EXE> <ACAD.OVL> <out.json>");
        return ExitCode::FAILURE;
    };
    let (exe, ovl_path, out) = (
        PathBuf::from(exe),
        PathBuf::from(ovl_path),
        PathBuf::from(out),
    );

    let bytes = match fs::read(&ovl_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {}: {e}", ovl_path.display());
            return ExitCode::FAILURE;
        }
    };
    let dir = match ovl::parse(&bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}: {e}", ovl_path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut blocks = vec![
        Block {
            name: "EXE_CODE".into(),
            seg: IMAGE_SEG,
            off: 0,
            len: DATA_OFF,
            src_off: None,
            overlay: false,
        },
        Block {
            name: "EXE_DATA".into(),
            seg: DATA_SEG,
            off: 0,
            len: IMAGE_LEN - DATA_OFF,
            src_off: None,
            overlay: false,
        },
    ];
    let mut entry_points = Vec::new();
    for e in &dir.entries {
        if !e.code.is_empty() {
            blocks.push(Block {
                name: format!("OVL{:02}_CODE", e.index),
                seg: WINDOW_SEG,
                off: e.code.dest as u32,
                len: e.code.len as u32,
                src_off: Some(e.code.file_off),
                overlay: true,
            });
        }
        if !e.data.is_empty() {
            blocks.push(Block {
                name: format!("OVL{:02}_DATA", e.index),
                seg: DATA_SEG,
                off: e.data.dest as u32,
                len: e.data.len as u32,
                src_off: Some(e.data.file_off),
                overlay: true,
            });
        }
        entry_points.push(EntryPoint {
            entry: e.index,
            seg: WINDOW_SEG,
            off: e.entry_point as u32,
        });
    }

    let map = Map {
        exe,
        ovl: ovl_path,
        image_len: IMAGE_LEN,
        window_bytes: dir.window_bytes,
        blocks,
        entry_points,
    };
    match fs::write(&out, serde_json::to_string_pretty(&map).unwrap()) {
        Ok(()) => {
            println!("wrote {} ({} blocks)", out.display(), map.blocks.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cannot write {}: {e}", out.display());
            ExitCode::FAILURE
        }
    }
}
