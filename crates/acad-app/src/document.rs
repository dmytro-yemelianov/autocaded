//! Drawing codecs and document metadata, independent of the event loop.
use acad_model::Drawing;
use std::path::{Path, PathBuf};
mod storage;
#[cfg(test)]
pub(crate) use storage::FAIL_DIRECTORY_SYNC;

#[derive(Clone, Copy)]
pub(crate) enum Format {
    Dwg(acad_dwg::header::Version),
    Dxf,
}
impl Format {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match acad_dwg::header::Version::detect(bytes) {
            Ok(version) => Self::Dwg(version),
            Err(_) => Self::Dxf,
        }
    }
    pub fn for_path(path: &Path) -> Self {
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
        {
            Self::Dxf
        } else {
            Self::Dwg(acad_dwg::header::Version::Ac140)
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Dxf => "dxf",
            Self::Dwg(acad_dwg::header::Version::Ac12) => "AC1.2",
            Self::Dwg(acad_dwg::header::Version::Ac140) => "AC1.40",
        }
    }
    fn encode(self, drawing: &Drawing) -> Result<Vec<u8>, String> {
        match self {
            Self::Dxf => acad_dxf::try_write(drawing).map_err(|e| e.to_string()),
            Self::Dwg(version) => {
                acad_dwg::write_version(drawing, version).map_err(|e| e.to_string())
            }
        }
    }
    /// Document save (END, SAVE, Save As): an existing destination's
    /// previous bytes are kept as its `.BAK` (docs/native-files-menu.md).
    /// `Ok(Some(warning))` is a completed save with an unconfirmed
    /// directory sync.
    pub fn save(self, path: &Path, drawing: &Drawing) -> Result<Option<String>, String> {
        let bytes = self.encode(drawing)?;
        storage::replace_with_backup(path, &bytes).map_err(|e| e.to_string())
    }
}

pub(crate) struct Document {
    pub path: Option<PathBuf>,
    pub format: Option<Format>,
    saved: Drawing,
}
impl Document {
    pub fn unnamed(drawing: &Drawing) -> Self {
        Self {
            path: None,
            format: None,
            saved: drawing.clone(),
        }
    }
    pub fn saved(path: PathBuf, format: Format, drawing: &Drawing) -> Self {
        Self {
            path: Some(path),
            format: Some(format),
            saved: drawing.clone(),
        }
    }
    pub fn dirty(&self, drawing: &Drawing) -> bool {
        self.saved != *drawing
    }
}

/// What a WBLOCK output name refers to before the block name prompt
/// (docs/native-files-menu.md, WBLOCK overwrite contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WblockDestination {
    New,
    /// An existing regular file, or a dangling symlink whose name is taken.
    Existing,
    /// The attached document's own file, by any name for it.
    OpenDrawing,
}

pub(crate) fn wblock_destination(
    path: &Path,
    open: Option<&Path>,
) -> Result<WblockDestination, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WblockDestination::New),
        Err(e) => Err(e.to_string()),
        Ok(_) => match std::fs::metadata(path) {
            Ok(meta) if !meta.is_file() => Err("output path is not a regular file".into()),
            Ok(_) if open.is_some_and(|open| storage::same_file(path, open)) => {
                Ok(WblockDestination::OpenDrawing)
            }
            Ok(_) => Ok(WblockDestination::Existing),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WblockDestination::Existing),
            Err(e) => Err(e.to_string()),
        },
    }
}

/// Whether `path` is the attached document's own file.
pub(crate) fn is_document_file(path: &Path, document: Option<&Path>) -> bool {
    document.is_some_and(|document| storage::same_file(path, document))
}

/// WBLOCK output, never backed up (as the original). `replace` is the
/// answered replace question; without it an existing file is refused.
pub(crate) fn save_dwg(
    path: &str,
    drawing: &acad_model::Drawing,
    replace: bool,
) -> Result<Option<String>, String> {
    let bytes = Format::Dwg(acad_dwg::header::Version::Ac140).encode(drawing)?;
    if replace {
        storage::replace(Path::new(path), &bytes)
    } else {
        storage::create_new(Path::new(path), &bytes)
    }
    .map_err(|e| e.to_string())
}

/// Main Menu tasks 5 and 6 (docs/native-main-menu.md): the interchange
/// file or drawing is replaced through the staged writer without a backup,
/// as the original makes none.
pub(crate) fn write_without_backup(
    path: &Path,
    format: Format,
    drawing: &acad_model::Drawing,
) -> Result<Option<String>, String> {
    let bytes = format.encode(drawing)?;
    storage::replace(path, &bytes).map_err(|e| e.to_string())
}

/// Chooses the codec by the file's own magic bytes, not its extension: a
/// `.BAK` file (the corpus has several) is a DWG whatever its name says, and
/// an AC1.2 or AC1.40 magic is unambiguous evidence either way (spec §4.2's
/// `Version::detect`). Only when neither magic matches — i.e. this is not a
/// DWG at all — does the file get handed to the DXF parser, which reports
/// its own error if it isn't a DXF either.
pub fn decode_drawing(bytes: &[u8]) -> Result<acad_model::Drawing, String> {
    match acad_dwg::header::Version::detect(bytes) {
        Ok(_) => acad_dwg::parse(bytes).map_err(|e| e.to_string()),
        Err(_) => acad_dxf::parse(bytes).map_err(|e| e.to_string()),
    }
}
