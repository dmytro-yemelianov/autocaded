use crate::shp::{Library, ShapeError};
use std::{collections::BTreeMap, path::PathBuf};

/// Libraries are supplied by the caller, not opened from names in a drawing.
/// Explicit aliases take precedence; other DOS paths are relocated by basename.
#[derive(Debug, Default)]
pub struct Libraries {
    entries: BTreeMap<String, Library>,
}

fn key(name: &str) -> String {
    let name = name.to_ascii_uppercase();
    name.strip_suffix(".SHP").unwrap_or(&name).to_owned()
}

impl Libraries {
    /// Explicit directories first. In the extracted corpus, System/TXT is
    /// AutoCAD 1.4's startup font; drawing-local libraries follow it.
    pub fn for_drawing(drawing: &std::path::Path, overrides: &[PathBuf]) -> (Self, Vec<String>) {
        let mut directories = overrides.to_vec();
        let local = drawing
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        if let Some(parent) = local.parent() {
            let system = parent.join("System");
            if system.is_dir() && system != local {
                directories.push(system);
            }
        }
        directories.push(local.to_path_buf());
        Self::load(&directories)
    }
    pub fn insert(&mut self, name: &str, bytes: &[u8]) -> Result<(), ShapeError> {
        self.entries.insert(key(name), Library::parse(bytes)?);
        Ok(())
    }
    pub fn get(&self, name: &str) -> Option<&Library> {
        self.entries.get(&key(name)).or_else(|| {
            self.entries
                .get(&key(name.rsplit([':', '/', '\\']).next().unwrap_or(name)))
        })
    }

    /// The available SHP libraries and their canonical lookup names. Drive
    /// aliases such as `B:ES` and their basename `ES` may refer to the same
    /// library.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Library)> {
        self.entries
            .iter()
            .map(|(name, library)| (name.as_str(), library))
    }

    /// Search in priority order. First definition of a basename wins, so
    /// an explicit directory takes precedence over a fallback directory.
    /// Extracted System/Samples directories also supply A:/B: aliases.
    pub fn load(directories: &[PathBuf]) -> (Self, Vec<String>) {
        let mut libraries = Self::default();
        let mut diagnostics = Vec::new();
        for directory in directories {
            let entries = match std::fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(e) => {
                    diagnostics.push(format!("{}: {e}", directory.display()));
                    continue;
                }
            };
            let mut paths = Vec::new();
            for entry in entries {
                match entry {
                    Ok(entry) => paths.push(entry.path()),
                    Err(e) => diagnostics.push(format!("{}: {e}", directory.display())),
                }
            }
            paths.sort();
            for path in paths {
                if !path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("shp"))
                {
                    continue;
                }
                let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let drive = directory
                    .file_name()
                    .and_then(|s| s.to_str())
                    .and_then(|s| match s {
                        "System" => Some("A"),
                        "Samples" => Some("B"),
                        _ => None,
                    });
                let alias = drive.map(|drive| key(&format!("{drive}:{name}")));
                if libraries.entries.contains_key(&key(name))
                    && alias
                        .as_ref()
                        .is_none_or(|alias| libraries.entries.contains_key(alias))
                {
                    continue;
                }
                match std::fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| Library::parse(&b).map_err(|e| e.to_string()))
                {
                    Ok(library) => {
                        if let Some(alias) = alias {
                            libraries
                                .entries
                                .entry(alias)
                                .or_insert_with(|| library.clone());
                        }
                        libraries.entries.entry(key(name)).or_insert(library);
                    }
                    Err(e) => diagnostics.push(format!("{}: {e}", path.display())),
                }
            }
        }
        (libraries, diagnostics)
    }
}
