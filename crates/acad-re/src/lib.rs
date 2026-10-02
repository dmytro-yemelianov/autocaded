//! Dev-only reverse-engineering support (spec §5): the `ACAD.OVL` container
//! codec, a typed Ghidra AST model, and the analyses built on them. No shipped
//! crate depends on this one.
pub mod analysis;
pub mod ast;
pub mod error;
pub mod ir;
pub mod ovl;
pub mod rust_emit;

pub use error::ReError;
