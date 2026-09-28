//! The 1983 DWG binary codec. `AC1.2` read support; `AC1.40` and the write
//! direction come after milestone ③ (spec §8).
pub mod error;
pub mod header;

pub use error::DwgError;
