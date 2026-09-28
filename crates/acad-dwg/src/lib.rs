//! The 1983 DWG binary codec. `AC1.2` read support; `AC1.40` and the write
//! direction come after milestone ③ (spec §8).
pub mod discover;
pub mod entity;
pub mod error;
pub mod header;
pub mod text;

pub use error::DwgError;

/// Read an `AC1.2` drawing.
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
