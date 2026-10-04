//! INSERT of a drawing file: the Session owns lookup and decoding, the editor
//! owns closure, conflict policy and the atomic commit.
use super::*;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Largest drawing file INSERT reads: the codecs' 65,535 stored-record limit
/// at a generous 256 bytes per record. OPEN does not share this cap.
pub(crate) const MAX_INSERT_FILE_BYTES: u64 = 65_535 * 256;

impl Session {
    pub(crate) fn submit_insert_file(
        &mut self,
        input: &str,
        spec: &str,
    ) -> Result<acad_cmd::Effect, String> {
        let directory = self.document.path.as_deref().and_then(Path::parent);
        let cwd = std::env::current_dir().map_err(|e| format!("INSERT: current directory: {e}"))?;
        let path = resolve_drawing_file(spec, directory, &cwd, &crate::files::drive_roots())?;
        let bytes = read_bounded_kind(&path, MAX_INSERT_FILE_BYTES, DRAWING_FILE)?;
        let drawing =
            decode_drawing(&bytes).map_err(|e| format!("INSERT: {}: {e}", path.display()))?;
        let effect = self.editor.submit_insert_drawing(input, drawing)?;
        // Libraries beside this source are merged only once the import commits.
        self.pending_insert_source = Some(path);
        Ok(effect)
    }

    /// After a committed file import, make the SHP libraries its reachable
    /// LOAD records name available from beside the source (as OPEN would
    /// search); names already available win.
    pub(crate) fn merge_committed_insert_libraries(&mut self) {
        let Some(names) = self.editor.take_committed_import_loads() else {
            return;
        };
        let Some(path) = self.pending_insert_source.take() else {
            return;
        };
        if names.is_empty() {
            return;
        }
        let (libraries, _) = Libraries::for_drawing(&path, &[]);
        if self.libraries.merge_missing(libraries, &names) > 0 {
            self.register_libraries();
        }
    }
}

/// Which command and file kind a bounded lookup serves; used in messages.
#[derive(Clone, Copy)]
pub(crate) struct FileKind {
    pub(crate) command: &'static str,
    pub(crate) noun: &'static str,
    /// Extensions tried, in order, when the specification has none.
    pub(crate) extensions: &'static [&'static str],
}

pub(crate) const DRAWING_FILE: FileKind = FileKind {
    command: "INSERT",
    noun: "drawing file",
    extensions: &["DWG", "dwg"],
};

/// Read at most `cap` bytes of one regular file; larger (or growing) files
/// are refused before any of their content is used.
pub(crate) fn read_bounded_kind(path: &Path, cap: u64, kind: FileKind) -> Result<Vec<u8>, String> {
    let FileKind { command, noun, .. } = kind;
    let too_large = || {
        format!(
            "{command}: {} exceeds the {cap}-byte {noun} limit",
            path.display()
        )
    };
    let cannot = |e: std::io::Error| format!("{command}: cannot read {}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(cannot)?;
    let metadata = file.metadata().map_err(cannot)?;
    if !metadata.is_file() {
        return Err(format!(
            "{command}: {} is not a regular file",
            path.display()
        ));
    }
    if metadata.len() > cap {
        return Err(too_large());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(cap + 1).read_to_end(&mut bytes).map_err(cannot)?;
    if bytes.len() as u64 > cap {
        return Err(too_large());
    }
    Ok(bytes)
}

/// Absolute paths as given; `D:rest` through the FILES drive mapping;
/// otherwise the document directory, then the process directory. Without an
/// extension `.DWG` then `.dwg` are tried. Only regular files (or symlinks to
/// them) are candidates; directories, FIFOs and devices are skipped.
fn resolve_drawing_file(
    spec: &str,
    document_dir: Option<&Path>,
    cwd: &Path,
    drives: &std::collections::BTreeMap<char, PathBuf>,
) -> Result<PathBuf, String> {
    resolve_file(spec, document_dir, cwd, drives, DRAWING_FILE)
}

/// [`resolve_drawing_file`]'s lookup for any [`FileKind`]: the first regular
/// candidate wins.
pub(crate) fn resolve_file(
    spec: &str,
    document_dir: Option<&Path>,
    cwd: &Path,
    drives: &std::collections::BTreeMap<char, PathBuf>,
    kind: FileKind,
) -> Result<PathBuf, String> {
    candidate_files(spec, document_dir, cwd, drives, kind)?
        .into_iter()
        .find(|candidate| is_regular_file(candidate))
        .ok_or_else(|| format!("{}: {} not found: {spec}", kind.command, kind.noun))
}

/// Only regular files (or symlinks to them) are lookup candidates.
pub(crate) fn is_regular_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// Candidate paths for a file specification, in search order. Without an
/// extension each base is tried with each of the kind's extensions.
pub(crate) fn candidate_files(
    spec: &str,
    document_dir: Option<&Path>,
    cwd: &Path,
    drives: &std::collections::BTreeMap<char, PathBuf>,
    kind: FileKind,
) -> Result<Vec<PathBuf>, String> {
    let command = kind.command;
    let plain = Path::new(spec);
    let bytes = spec.as_bytes();
    let bases: Vec<PathBuf> = if plain.is_absolute() {
        vec![plain.to_path_buf()]
    } else if bytes.len() > 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let drive = bytes[0].to_ascii_uppercase() as char;
        let root =
            crate::files::drive_path(drive, cwd, drives).map_err(|e| format!("{command}: {e}"))?;
        let rest = spec[2..].trim_start_matches(['/', '\\']).replace('\\', "/");
        vec![root.join(rest)]
    } else {
        let relative = spec.replace('\\', "/");
        let mut bases: Vec<PathBuf> = document_dir
            .map(|dir| dir.join(&relative))
            .into_iter()
            .collect();
        let local = cwd.join(&relative);
        if !bases.contains(&local) {
            bases.push(local);
        }
        bases
    };
    Ok(bases
        .into_iter()
        .flat_map(|base| {
            if base.extension().is_some() {
                vec![base]
            } else {
                kind.extensions
                    .iter()
                    .map(|extension| base.with_extension(extension))
                    .collect()
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Dir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "acad-insert-resolve-{}-{label}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_directory_candidate_does_not_shadow_a_later_regular_file() {
        let document = Dir::new("doc");
        let cwd = Dir::new("cwd");
        std::fs::create_dir(document.0.join("SHADOW.DWG")).unwrap();
        std::fs::write(cwd.0.join("SHADOW.DWG"), b"x").unwrap();
        let drives = Default::default();
        assert_eq!(
            resolve_drawing_file("SHADOW", Some(&document.0), &cwd.0, &drives).unwrap(),
            cwd.0.join("SHADOW.DWG")
        );
        std::fs::remove_file(cwd.0.join("SHADOW.DWG")).unwrap();
        assert!(
            resolve_drawing_file("SHADOW.DWG", Some(&document.0), &cwd.0, &drives)
                .unwrap_err()
                .contains("not found")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_to_regular_files_are_accepted() {
        let dir = Dir::new("link");
        std::fs::write(dir.0.join("REAL.DWG"), b"x").unwrap();
        std::os::unix::fs::symlink(dir.0.join("REAL.DWG"), dir.0.join("LINK.DWG")).unwrap();
        let path = resolve_drawing_file("LINK", Some(&dir.0), &dir.0, &Default::default()).unwrap();
        assert_eq!(read_bounded_kind(&path, 10, DRAWING_FILE).unwrap(), b"x");
    }

    #[test]
    fn reads_are_bounded_by_the_cap() {
        let dir = Dir::new("cap");
        let path = dir.0.join("BIG.DWG");
        std::fs::write(&path, b"12345").unwrap();
        assert_eq!(read_bounded_kind(&path, 5, DRAWING_FILE).unwrap(), b"12345");
        assert!(read_bounded_kind(&path, 4, DRAWING_FILE)
            .unwrap_err()
            .contains("4-byte"));
    }
}
