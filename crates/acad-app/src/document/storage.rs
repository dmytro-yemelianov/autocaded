//! Stage complete output next to the destination, then replace it by rename.
//! Drawing saves keep the replaced bytes as a `.BAK` (docs/native-files-menu.md).
// wasm32's `fs::File` is an unsupported stub without `Drop`; the explicit
// drops that close the file before rename are still right on real targets.
#![cfg_attr(target_arch = "wasm32", allow(clippy::drop_non_drop))]
use std::{
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Create a file that must not exist yet (WBLOCK when no replacement was
/// confirmed). Any existing entry at `path`, even a dangling symlink, is
/// refused. The complete staged file is then hard-linked into place, which
/// fails if the name appeared meanwhile; where hard links are unsupported a
/// last existence check precedes the rename.
pub(super) fn create_new(path: &Path, bytes: &[u8]) -> io::Result<Option<String>> {
    let exists = || {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "output file already exists; not replaced",
        )
    };
    if fs::symlink_metadata(path).is_ok() {
        return Err(exists());
    }
    let target = target_path(path)?;
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("output path has no parent"))?;
    let (staged, mut file) = create_staged(parent, false)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    match fs::hard_link(&staged.0, &target) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(exists()),
        Err(_) => {
            if fs::symlink_metadata(&target).is_ok() {
                return Err(exists());
            }
            fs::rename(&staged.0, &target)?;
        }
    }
    // Dropping the staging name leaves the linked file in place.
    drop(staged);
    Ok(directory_sync_warning(parent))
}

/// Whether `a` and `b` name the same existing file, through symlinks,
/// relative components and case-insensitive volumes (device and inode).
pub(super) fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let (Ok(a), Ok(b)) = (fs::metadata(a), fs::metadata(b)) else {
            return false;
        };
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
    }
}

/// Replace without a backup (WBLOCK, as in the original).
/// `Ok(Some(warning))`: the file was replaced, but the directory fsync that
/// makes the rename durable was not confirmed. The save still succeeded.
pub(super) fn replace(path: &Path, bytes: &[u8]) -> io::Result<Option<String>> {
    stage_and_replace(path, false, |file| file.write_all(bytes))
}

/// Replace and keep an existing destination's bytes as its backup.
pub(super) fn replace_with_backup(path: &Path, bytes: &[u8]) -> io::Result<Option<String>> {
    stage_and_replace(path, true, |file| file.write_all(bytes))
}

/// `HOUSE.DWG` -> `HOUSE.BAK`, `plan.dxf` -> `plan.bak`; `None` when the
/// destination already is a backup.
fn backup_path(target: &Path) -> Option<PathBuf> {
    let extension = target.extension().and_then(OsStr::to_str);
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("bak")) {
        return None;
    }
    let lower = extension.is_some_and(|extension| {
        extension.bytes().any(|byte| byte.is_ascii_lowercase())
            && !extension.bytes().any(|byte| byte.is_ascii_uppercase())
    });
    Some(target.with_extension(if lower { "bak" } else { "BAK" }))
}

/// A create-new temporary file beside the destination, removed on drop.
/// `private` creates it owner-only (unix) until the final mode is copied,
/// so a restrictive drawing's bytes are never readable under the umask.
fn create_staged(parent: &Path, private: bool) -> io::Result<(Staged, fs::File)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".acad-save-{}-{id}.tmp", std::process::id()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        match options.open(&path) {
            Ok(file) => return Ok((Staged(path), file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// macOS/BSD user or system immutable/append-only flags (Finder "Locked").
#[cfg(target_os = "macos")]
fn locked(meta: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const UF_IMMUTABLE: u32 = 0x2;
    const UF_APPEND: u32 = 0x4;
    const SF_IMMUTABLE: u32 = 0x2_0000;
    const SF_APPEND: u32 = 0x4_0000;
    meta.st_flags() & (UF_IMMUTABLE | UF_APPEND | SF_IMMUTABLE | SF_APPEND) != 0
}
#[cfg(not(target_os = "macos"))]
fn locked(_: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
thread_local! {
    /// Test seam: a directory fsync failure to report instead of syncing.
    pub(crate) static FAIL_DIRECTORY_SYNC: std::cell::Cell<Option<io::ErrorKind>> =
        const { std::cell::Cell::new(None) };
}

/// Make completed renames in `directory` durable (unix directory fsync).
fn sync_directory(directory: &Path) -> io::Result<()> {
    #[cfg(test)]
    if let Some(kind) = FAIL_DIRECTORY_SYNC.with(std::cell::Cell::get) {
        return Err(io::Error::new(kind, "injected directory sync failure"));
    }
    #[cfg(unix)]
    fs::File::open(directory)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

/// After the final rename the save has happened: a directory fsync failure
/// becomes a warning (silent where directories cannot be fsynced), never
/// an error, so a retry cannot back up the new bytes over the real backup.
fn directory_sync_warning(directory: &Path) -> Option<String> {
    match sync_directory(directory) {
        Ok(()) => None,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
            ) =>
        {
            None
        }
        Err(e) => Some(format!("directory sync not confirmed: {e}")),
    }
}

fn stage_and_replace(
    path: &Path,
    backup: bool,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<Option<String>> {
    // Follow existing symlinks like the former fs::write path. Replacing the
    // link itself would silently redirect subsequent opens to a different file.
    let target = target_path(path)?;
    let permissions = match fs::metadata(&target) {
        Ok(meta) => {
            if !meta.is_file() {
                return Err(io::Error::other("output path is not a regular file"));
            }
            if meta.permissions().readonly() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "output file is read-only",
                ));
            }
            if locked(&meta) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "output file is locked (immutable or append-only)",
                ));
            }
            Some(meta.permissions())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("output path has no parent"))?;
    // (1) The complete new drawing, durable before anything is renamed.
    let (staged, mut file) = create_staged(parent, permissions.is_some())?;
    write(&mut file)?;
    if let Some(permissions) = &permissions {
        file.set_permissions(permissions.clone())?;
    }
    file.sync_all()?;
    drop(file);
    // (2) A durable byte copy of the current destination over the backup
    // name. Only bytes and the mode are copied (never file flags), so the
    // temporary file stays removable. The destination itself is never
    // moved, so it always names a complete drawing.
    if let (true, Some(permissions), Some(backup)) = (backup, &permissions, backup_path(&target)) {
        let (copy, mut file) = create_staged(parent, true)?;
        io::copy(&mut fs::File::open(&target)?, &mut file)?;
        file.set_permissions(permissions.clone())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&copy.0, &backup)
            .map_err(|e| io::Error::new(e.kind(), format!("backup {}: {e}", backup.display())))?;
    }
    // (3) The new drawing over the destination, then the renames made durable.
    fs::rename(&staged.0, &target)?;
    Ok(directory_sync_warning(parent))
}

fn target_path(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(target) => Ok(target),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()) {
                let link = fs::read_link(path)?;
                let target = if link.is_absolute() {
                    link
                } else {
                    path.parent().unwrap_or(Path::new(".")).join(link)
                };
                target_path(&target)
            } else {
                std::path::absolute(path)
            }
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_staging_write_leaves_original_bytes_and_removes_the_temporary_file() {
        let root = std::env::temp_dir().join(format!("acad-staging-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("drawing.dwg");
        fs::write(&path, b"original drawing").unwrap();
        let error = stage_and_replace(&path, true, |file| {
            file.write_all(b"partial replacement")?;
            Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "simulated failed write",
            ))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(fs::read(&path).unwrap(), b"original drawing");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn replacing_an_existing_file_stages_owner_only_then_copies_its_mode() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("acad-staging-mode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let path = root.join("secret.dwg");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        stage_and_replace(&path, true, |file| {
            assert_eq!(file.metadata()?.permissions().mode() & 0o777, 0o600);
            file.write_all(b"new")
        })
        .unwrap();
        for name in ["secret.dwg", "secret.bak"] {
            let meta = fs::metadata(root.join(name)).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o640, "{name}");
        }
        assert_eq!(fs::read(root.join("secret.bak")).unwrap(), b"old");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_new_writes_a_missing_file_and_refuses_any_existing_entry() {
        let root = std::env::temp_dir().join(format!("acad-staging-new-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let path = root.join("part.dwg");
        assert_eq!(create_new(&path, b"first").unwrap(), None);
        assert_eq!(fs::read(&path).unwrap(), b"first");
        let error = create_new(&path, b"second").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"first");
        #[cfg(unix)]
        {
            let dangling = root.join("dangling.dwg");
            std::os::unix::fs::symlink(root.join("missing.dwg"), &dangling).unwrap();
            let error = create_new(&dangling, b"x").unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
            assert!(!root.join("missing.dwg").exists());
        }
        let names: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            names
                .iter()
                .all(|name| !name.to_string_lossy().ends_with(".tmp")),
            "{names:?}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_directory_sync_failure_after_the_rename_is_a_warning_not_an_error() {
        let root =
            std::env::temp_dir().join(format!("acad-staging-dirsync-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let path = root.join("d.dwg");
        fs::write(&path, b"old").unwrap();
        for (kind, warned) in [
            (io::ErrorKind::Other, true),
            (io::ErrorKind::Unsupported, false),
            (io::ErrorKind::InvalidInput, false),
        ] {
            FAIL_DIRECTORY_SYNC.with(|fail| fail.set(Some(kind)));
            let result = replace_with_backup(&path, b"new");
            FAIL_DIRECTORY_SYNC.with(|fail| fail.set(None));
            let warning = result.unwrap();
            assert_eq!(warning.is_some(), warned, "{kind:?}");
            if let Some(warning) = warning {
                assert!(warning.starts_with("directory sync not confirmed: "));
            }
            assert_eq!(fs::read(&path).unwrap(), b"new");
            fs::write(&path, b"old").unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
}
