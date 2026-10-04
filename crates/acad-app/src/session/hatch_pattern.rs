//! HATCH pattern files: the Session finds and reads the file, the editor
//! parses it and keeps the prompt flow (docs/native-hatch-user.md).
use super::external_insert::{candidate_files, is_regular_file, read_bounded_kind, FileKind};
use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PATTERN_FILE: FileKind = FileKind {
    command: "HATCH",
    noun: "pattern file",
    extensions: &["PAT", "pat"],
};

impl Session {
    /// Answer the pattern prompt from a file. With no file found the reply
    /// goes to the editor, which reports an unknown pattern; lookup and read
    /// errors end HATCH with nothing changed.
    pub(crate) fn submit_hatch_pattern_file(
        &mut self,
        input: &str,
        spec: &str,
    ) -> Result<acad_cmd::Effect, String> {
        let directory = self.document.path.as_deref().and_then(Path::parent);
        let read = std::env::current_dir()
            .map_err(|e| format!("HATCH: current directory: {e}"))
            .and_then(|cwd| {
                resolve_pattern_file(spec, directory, &cwd, &crate::files::drive_roots())
            })
            .and_then(|path| match path {
                None => Ok(None),
                Some(path) => read_bounded_kind(
                    &path,
                    acad_cmd::MAX_HATCH_PATTERN_FILE_BYTES as u64,
                    PATTERN_FILE,
                )
                .map(|bytes| Some((path, bytes))),
            });
        match read {
            Ok(None) => self.editor.submit(input),
            Ok(Some((path, bytes))) => {
                let source = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                self.editor
                    .submit_hatch_pattern_file(input, &source, &bytes)
            }
            Err(error) => {
                self.editor.cancel_command()?;
                Err(error)
            }
        }
    }
}

/// `NAME.PAT`/`NAME.pat` (and, for a bare name, its upper- and lower-case
/// spellings) through the INSERT lookup, then for a bare name `ACAD.PAT`
/// and `acad.pat` in the document and process directories. The first
/// regular file wins; `None` when there is none.
fn resolve_pattern_file(
    spec: &str,
    document_dir: Option<&Path>,
    cwd: &Path,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<Option<PathBuf>, String> {
    let bare = !spec.contains(['/', '\\', ':', '.']);
    let mut candidates = Vec::new();
    let mut spellings = vec![spec.to_owned()];
    if bare {
        spellings.push(spec.to_ascii_uppercase());
        spellings.push(spec.to_ascii_lowercase());
    }
    for spelling in spellings {
        candidates.extend(candidate_files(
            &spelling,
            document_dir,
            cwd,
            drives,
            PATTERN_FILE,
        )?);
    }
    if bare {
        for name in ["ACAD.PAT", "acad.pat"] {
            candidates.extend(candidate_files(
                name,
                document_dir,
                cwd,
                drives,
                PATTERN_FILE,
            )?);
        }
    }
    Ok(candidates.into_iter().find(|path| is_regular_file(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Dir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("acad-hatch-resolve-{}-{label}", std::process::id()));
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
    fn name_pat_in_either_directory_precedes_acad_pat() {
        let document = Dir::new("doc");
        let cwd = Dir::new("cwd");
        let drives = BTreeMap::new();
        let resolve = |spec: &str| resolve_pattern_file(spec, Some(&document.0), &cwd.0, &drives);
        assert_eq!(resolve("RAILS").unwrap(), None);
        std::fs::write(document.0.join("ACAD.PAT"), "*RAILS\n0,0,0,0,1\n").unwrap();
        assert_eq!(resolve("RAILS").unwrap(), Some(document.0.join("ACAD.PAT")));
        std::fs::write(cwd.0.join("RAILS.PAT"), "*RAILS\n0,0,0,0,1\n").unwrap();
        assert_eq!(resolve("RAILS").unwrap(), Some(cwd.0.join("RAILS.PAT")));
        // Lower-case spelling, upper-case file; a directory never matches.
        let found = resolve("rails").unwrap().unwrap();
        assert!(found.starts_with(&cwd.0), "{found:?}");
        std::fs::create_dir(document.0.join("RAILS.PAT")).unwrap();
        assert!(resolve("RAILS").unwrap().unwrap().starts_with(&cwd.0));
        // Explicit paths never fall back to ACAD.PAT.
        assert_eq!(resolve("sub/RAILS").unwrap(), None);
        assert_eq!(resolve("RAILS.TXT").unwrap(), None);
        assert!(resolve("Q:RAILS")
            .unwrap_err()
            .starts_with("HATCH: FILES drive Q:"));
    }

    #[test]
    fn pattern_reads_are_capped() {
        let dir = Dir::new("cap");
        let path = dir.0.join("BIG.PAT");
        std::fs::write(&path, vec![b'\n'; 11]).unwrap();
        assert!(read_bounded_kind(&path, 10, PATTERN_FILE)
            .unwrap_err()
            .contains("10-byte pattern file limit"));
    }
}
