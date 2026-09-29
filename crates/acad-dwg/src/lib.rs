//! The 1983 DWG binary reader for `AC1.2` and `AC1.40`.
//! Includes ordered LOAD records and SHAPE references, plus AC1.2/AC1.40 writing.
pub mod discover;
pub mod entity;
pub mod error;
pub mod header;
pub mod text;
pub mod write;

pub use error::DwgError;

/// Read an `AC1.2` or `AC1.40` drawing.
///
/// This is the crate's one public entry point, mirroring `acad_dxf::parse`:
/// decode the fixed header, then walk the entity region into a flat,
/// document-order `Vec<Item>` (block definitions and loose entities
/// interleaved exactly as they sit in the file — spec §4.4).
pub fn parse(bytes: &[u8]) -> Result<acad_model::Drawing, DwgError> {
    let (header, meta) = header::parse_header(bytes)?;
    let items = entity::read_items(bytes, &meta)?;
    Ok(acad_model::Drawing { header, items })
}

/// Encode a drawing as an AC1.40 file.
pub fn write(drawing: &acad_model::Drawing) -> Result<Vec<u8>, DwgError> {
    write::encode(drawing)
}

/// Encode a drawing in the requested revision of the DWG format.
pub fn write_version(
    drawing: &acad_model::Drawing,
    version: header::Version,
) -> Result<Vec<u8>, DwgError> {
    write::encode_version(drawing, version)
}
