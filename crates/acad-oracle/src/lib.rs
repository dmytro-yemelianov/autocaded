//! Development-only oracle for the original AutoCAD.

#[cfg(unix)]
mod qemu;
#[cfg(unix)]
pub use qemu::{
    available, export_sample_backups, export_samples, generate_dwg, generate_pair,
    generate_pair_with_samples,
};
