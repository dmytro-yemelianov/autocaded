use super::*;

impl Session {
    pub(crate) fn load_menu(&mut self, requested: &str) {
        if let Err(error) = self.try_load_menu(requested) {
            self.status = error;
        }
    }
    pub(crate) fn try_load_menu(&mut self, requested: &str) -> Result<(), String> {
        let (path, menu) = load_menu_file(requested)?;
        let items = menu
            .entries
            .iter()
            .filter(|entry| entry.kind == acad_cmd::menu::MenuEntryKind::Item)
            .count();
        self.status = format!("Loaded {} menu entries from {}", items, path.display());
        self.menu = Some(menu);
        self.menu_page = 0;
        Ok(())
    }

    pub(crate) fn unload_menu(&mut self) {
        self.menu = None;
        self.menu_page = 0;
        self.status.clear();
    }

    pub(crate) fn resolve_shape_library(&mut self, requested: &str) -> Result<String, String> {
        if requested.trim().is_empty() {
            return Err("shape library name cannot be empty".into());
        }
        if let Some(path) = shape_library_path(requested) {
            let bytes = std::fs::read(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let library = acad_render::shp::Library::parse(&bytes)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| format!("invalid SHP filename: {}", path.display()))?
                .to_owned();
            if let Some(existing) = self.libraries.get(&name) {
                if existing != &library {
                    return Err(format!(
                        "another SHP library named {name} is already available; rename one file"
                    ));
                }
            } else {
                self.libraries
                    .insert(&name, &bytes)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
            }
            self.editor.register_shape_library(
                &name,
                library
                    .named_shapes()
                    .map(|(shape, number)| (shape.to_owned(), number)),
            );
            self.register_libraries();
            return Ok(name);
        }
        if let Some(library) = self.libraries.get(requested) {
            self.editor.register_shape_library(
                requested,
                library
                    .named_shapes()
                    .map(|(shape, number)| (shape.to_owned(), number)),
            );
            self.register_libraries();
            return Ok(requested.to_owned());
        }
        Err(format!("shape library is not available: {requested}"))
    }
}

/// NEXT advances independently of command submission and wraps the last page.
pub(crate) fn advance_menu_page(current_page: usize, menu: &acad_cmd::menu::MenuFile) -> usize {
    let pages = menu_panel::page_count(menu);
    if pages > 0 {
        (current_page + 1) % pages
    } else {
        current_page
    }
}

/// Largest screen-menu source MENU reads; retained menus are under 2 KiB.
pub(crate) const MAX_MENU_FILE_BYTES: u64 = 64 * 1024;
const MENU_FILE: super::external_insert::FileKind = super::external_insert::FileKind {
    command: "MENU",
    noun: "menu file",
    extensions: &["MNU", "mnu"],
};

/// Regular files only, read through the shared byte-capped reader.
pub(crate) fn load_menu_file(
    requested: &str,
) -> Result<(std::path::PathBuf, acad_cmd::menu::MenuFile), String> {
    let requested = std::path::PathBuf::from(requested.trim());
    let mut candidates = vec![requested.clone()];
    if requested.extension().is_none() {
        candidates.push(requested.with_extension("MNU"));
    }
    if let Some(name) = candidates.last().cloned() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        candidates.push(corpus.join("System").join(&name));
        candidates.push(corpus.join("Samples").join(name));
    }
    let path = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("menu file not found: {}", requested.display()))?;
    let bytes = super::external_insert::read_bounded_kind(&path, MAX_MENU_FILE_BYTES, MENU_FILE)?;
    let menu = acad_cmd::menu::parse_menu(&bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((path, menu))
}

pub(crate) fn shape_library_path(requested: &str) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(requested.trim());
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("shp"))
        && path.is_file()
    {
        return Some(path);
    }
    if path.extension().is_none() {
        let with_extension = path.with_extension("SHP");
        if with_extension.is_file() {
            return Some(with_extension);
        }
    }
    None
}
