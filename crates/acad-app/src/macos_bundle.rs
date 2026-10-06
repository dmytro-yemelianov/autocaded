//! Finder startup keeps immutable resources in the bundle and drawings outside it.
use std::{fs, io::Write, path::Path};

pub fn resources() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    if macos.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
        return None;
    }
    let resources = contents.join("Resources");
    resources
        .join("System/TXT.SHP")
        .is_file()
        .then_some(resources)
}

pub fn prepare_drawings(resources: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| format!("{}: {e}", destination.display()))?;
    for name in ["WELCOME.DWG", "BRACKET.DWG", "FLOORPLAN.DWG", "PALETTE.DWG"] {
        let source = resources.join("demo").join(name);
        let bytes = fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
        let target = destination.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => file
                .write_all(&bytes)
                .map_err(|e| format!("{}: {e}", target.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("{}: {e}", target.display())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_startup_preserves_edited_demos_and_bundle() {
        let root = std::env::temp_dir().join(format!("acad-bundle-{}", std::process::id()));
        let resources = root.join("Resources");
        let drawings = root.join("User drawings");
        fs::create_dir_all(resources.join("demo")).unwrap();
        for name in ["WELCOME.DWG", "BRACKET.DWG", "FLOORPLAN.DWG", "PALETTE.DWG"] {
            fs::write(resources.join("demo").join(name), b"original").unwrap();
        }
        prepare_drawings(&resources, &drawings).unwrap();
        fs::write(drawings.join("WELCOME.DWG"), b"user edits").unwrap();
        prepare_drawings(&resources, &drawings).unwrap();
        assert_eq!(
            fs::read(drawings.join("WELCOME.DWG")).unwrap(),
            b"user edits"
        );
        assert_eq!(fs::read(drawings.join("BRACKET.DWG")).unwrap(), b"original");
        assert_eq!(
            fs::read(resources.join("demo/WELCOME.DWG")).unwrap(),
            b"original"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
