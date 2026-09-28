//! Dev-only reverse-engineering support (spec §5): the `ACAD.OVL` container
//! codec, a typed Ghidra AST model, and the analyses built on them. No shipped
//! crate depends on this one.
pub mod error;
pub mod ovl;

pub use error::ReError;
