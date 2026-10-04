use acad_cmd::{FilesFilter, FilesRequest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn execute(request: FilesRequest) -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    let drives = drive_roots();
    execute_in(request, &cwd, &drives)
}

pub(crate) fn drive_roots() -> BTreeMap<char, PathBuf> {
    ('A'..='Z')
        .filter_map(|letter| {
            std::env::var_os(format!("AUTOCAD_DRIVE_{letter}"))
                .map(PathBuf::from)
                .map(|path| (letter, path))
        })
        .collect()
}

pub(crate) fn drive_path(
    letter: char,
    cwd: &Path,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<PathBuf, String> {
    drives
        .get(&letter)
        .cloned()
        .or_else(|| (letter == 'A').then(|| cwd.to_owned()))
        .ok_or_else(|| {
            format!("FILES drive {letter}: has no directory mapping; set AUTOCAD_DRIVE_{letter}")
        })
}

fn filtered_files(
    directory: &Path,
    pattern: &str,
    filter: Option<FilesFilter>,
) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("cannot list {}: {error}", directory.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read directory entry: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !wildcard_match(pattern, &name) {
            continue;
        }
        if filter.is_some_and(|filter| !matches_filter(&name, filter)) {
            continue;
        }
        files.push(entry.path());
    }
    files.sort_by_key(|path| path.file_name().unwrap_or_default().to_ascii_lowercase());
    Ok(files)
}

fn matches_filter(name: &str, filter: FilesFilter) -> bool {
    let ext = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    match filter {
        FilesFilter::Drawings => ext.eq_ignore_ascii_case("DWG"),
        FilesFilter::Menus => ext.eq_ignore_ascii_case("MNU"),
        FilesFilter::Shapes => ext.eq_ignore_ascii_case("SHP"),
        FilesFilter::Patterns => ext.eq_ignore_ascii_case("PAT"),
    }
}

fn listing(files: &[PathBuf]) -> String {
    let mut report = format!(
        "{} file{}\n",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    for path in files {
        if let Some(name) = path.file_name() {
            report.push_str(&name.to_string_lossy());
            report.push('\n');
        }
    }
    report
}

fn split_specification<'a>(
    specification: &'a str,
    cwd: &Path,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<(PathBuf, &'a str), String> {
    let bytes = specification.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let directory = drive_path(bytes[0].to_ascii_uppercase() as char, cwd, drives)?;
        let name = specification[2..].trim_start_matches(['/', '\\']);
        if name.is_empty() {
            return Err("file specification needs a filename".into());
        }
        return Ok((directory, name));
    }
    let path = Path::new(specification);
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(cwd);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid file specification: {specification}"))?;
    Ok((directory.to_owned(), name))
}

fn exact_path(
    specification: &str,
    cwd: &Path,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<PathBuf, String> {
    let (directory, name) = split_specification(specification, cwd, drives)?;
    if name.contains(['*', '?']) {
        return Err("wildcards are only valid for list and delete".into());
    }
    Ok(directory.join(name))
}

fn execute_in(
    request: FilesRequest,
    cwd: &Path,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<String, String> {
    match request {
        FilesRequest::ListDrive { filter, drive } => {
            let directory = drive_path(drive, cwd, drives)?;
            let files = filtered_files(&directory, "*", Some(filter))?;
            Ok(listing(&files))
        }
        FilesRequest::ListSpecification(specification) => {
            let (directory, pattern) = split_specification(&specification, cwd, drives)?;
            let files = filtered_files(&directory, pattern, None)?;
            Ok(listing(&files))
        }
        FilesRequest::Delete(specification) => {
            let (directory, pattern) = split_specification(&specification, cwd, drives)?;
            let files = filtered_files(&directory, pattern, None)?;
            for path in &files {
                std::fs::remove_file(path)
                    .map_err(|error| format!("cannot delete {}: {error}", path.display()))?;
            }
            Ok(format!(
                "Deleted {} file{}",
                files.len(),
                if files.len() == 1 { "" } else { "s" }
            ))
        }
        FilesRequest::Rename {
            source,
            destination,
        } => {
            let source = exact_path(&source, cwd, drives)?;
            let destination = exact_path(&destination, cwd, drives)?;
            if !source.is_file() {
                return Err(format!("file not found: {}", source.display()));
            }
            // Any existing entry, including a dangling symlink, is kept.
            if std::fs::symlink_metadata(&destination).is_ok() {
                return Err(format!(
                    "destination already exists: {}",
                    destination.display()
                ));
            }
            std::fs::rename(&source, &destination).map_err(|error| {
                format!(
                    "cannot rename {} to {}: {error}",
                    source.display(),
                    destination.display()
                )
            })?;
            Ok(format!(
                "Renamed {} to {}",
                source.display(),
                destination.display()
            ))
        }
    }
}

fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let name = name.as_bytes();
    let (mut p, mut n) = (0, 0);
    let (mut star, mut retry) = (None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p].eq_ignore_ascii_case(&name[n])) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = n;
        } else if let Some(star_index) = star {
            retry += 1;
            n = retry;
            p = star_index + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            // Concurrent tests can observe the same macOS wall-clock tick.
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("acad-files-{}-{nonce}-{id}", std::process::id()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn listing_filters_by_extension_and_dos_wildcard() {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("A.DWG"), b"dwg").unwrap();
        std::fs::write(dir.0.join("B.DXF"), b"dxf").unwrap();
        std::fs::write(dir.0.join("C.dwg"), b"dwg").unwrap();
        let files = filtered_files(&dir.0, "*.DWG", Some(FilesFilter::Drawings)).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(listing(&files).lines().next(), Some("2 files"));
    }

    #[test]
    fn delete_and_rename_change_only_files_matching_the_request() {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("ONE.DWG"), b"one").unwrap();
        std::fs::write(dir.0.join("TWO.DWG"), b"two").unwrap();
        let drives = BTreeMap::from([('B', dir.0.clone())]);
        assert!(execute_in(
            FilesRequest::Rename {
                source: "B:ONE.DWG".into(),
                destination: "B:RENAMED.DWG".into(),
            },
            &dir.0,
            &drives
        )
        .is_ok());
        assert_eq!(std::fs::read(dir.0.join("RENAMED.DWG")).unwrap(), b"one");
        assert_eq!(
            execute_in(FilesRequest::Delete("B:TWO.*".into()), &dir.0, &drives).unwrap(),
            "Deleted 1 file"
        );
        assert!(!dir.0.join("TWO.DWG").exists());
        assert!(dir.0.join("RENAMED.DWG").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rename_never_replaces_an_existing_entry_even_a_dangling_symlink() {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("ONE.DWG"), b"one").unwrap();
        std::os::unix::fs::symlink("MISSING.DWG", dir.0.join("LINK.DWG")).unwrap();
        let drives = BTreeMap::from([('B', dir.0.clone())]);
        let error = execute_in(
            FilesRequest::Rename {
                source: "B:ONE.DWG".into(),
                destination: "B:LINK.DWG".into(),
            },
            &dir.0,
            &drives,
        )
        .unwrap_err();
        assert!(error.contains("destination already exists"), "{error}");
        assert_eq!(std::fs::read(dir.0.join("ONE.DWG")).unwrap(), b"one");
        assert!(std::fs::symlink_metadata(dir.0.join("LINK.DWG"))
            .unwrap()
            .is_symlink());
    }
}
