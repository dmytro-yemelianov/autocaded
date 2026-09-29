//! Development-only oracle for the original AutoCAD.

pub mod cga;
pub mod cpu8086;
pub mod dos_loader;
pub mod dos_machine;
mod fat12;
pub mod in_tree;
pub mod mz_loader;
pub mod screen;
pub use in_tree::{generate_dwg_in_tree, generate_visual_dwg_in_tree, InTreeVisualProbe};

#[cfg(unix)]
mod qemu;
#[cfg(unix)]
pub mod session;
#[cfg(unix)]
pub use qemu::{
    available, export_sample_backups, export_samples, generate_dwg, generate_pair,
    generate_pair_with_samples, generate_visual_pair, open_drawing, VisualProbe,
};
