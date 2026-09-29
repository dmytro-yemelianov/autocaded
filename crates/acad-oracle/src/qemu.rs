use crate::session::{Session, BOOT_TIMEOUT, EDIT_TIMEOUT};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn available() -> bool {
    Command::new("qemu-system-i386")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Ask the original to export sample drawings as DXF, using the
/// System floppy in drive A and a disposable Samples floppy in drive B.
pub fn export_samples(
    system_disk: &Path,
    samples_disk: &Path,
    names: &[&str],
) -> Result<Vec<Vec<u8>>, String> {
    export(system_disk, samples_disk, names, false)
}

/// Export backups by renaming .BAK to .DWG on the disposable Samples copy
/// before boot. Reject a name if a .DWG already exists; never overwrite it.
pub fn export_sample_backups(
    system_disk: &Path,
    samples_disk: &Path,
    names: &[&str],
) -> Result<Vec<Vec<u8>>, String> {
    export(system_disk, samples_disk, names, true)
}

fn export(
    system_disk: &Path,
    samples_disk: &Path,
    names: &[&str],
    backups: bool,
) -> Result<Vec<Vec<u8>>, String> {
    if names.iter().any(|name| {
        name.is_empty()
            || name.len() > 8
            || !name
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }) {
        return Err("sample names must be 1–8 uppercase ASCII letters or digits".into());
    }
    let mut vm = Session::boot_disposable(
        system_disk,
        Some(samples_disk),
        if backups { names } else { &[] },
    )?;
    vm.wait_for_text("Enter selection:", BOOT_TIMEOUT)?;
    for (i, name) in names.iter().enumerate() {
        vm.type_line("5")?;
        vm.wait_for_text("Enter NAME of drawing", BOOT_TIMEOUT)?;
        vm.type_line(&format!("B:{name}"))?;
        vm.wait_for_text("Drawing interchange file complete.", EDIT_TIMEOUT)?;
        if i + 1 < names.len() {
            vm.type_line("")?;
            vm.wait_until_text_gone("Drawing interchange file complete.", BOOT_TIMEOUT)?;
            vm.wait_for_text("Enter selection:", BOOT_TIMEOUT)?;
        }
    }
    vm.shutdown()?;
    let image = fs::read(vm.dir().join("samples.img"))
        .map_err(|e| format!("read copied Samples floppy: {e}"))?;
    names
        .iter()
        .map(|name| fat12_file(&image, &format!("{name}.DXF")))
        .collect()
}

/// Script editor commands on the original, save with END, and return its DWG.
/// The original floppy is never mounted for writing.
pub fn generate_dwg(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<Vec<u8>, String> {
    run(system_disk, None, name, editor_lines, false, None).map(|(dwg, _)| dwg)
}

/// Also ask the original to export the generated drawing as 1983 DXF.
pub fn generate_pair(
    system_disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (dwg, dxf) = run(system_disk, None, name, editor_lines, true, None)?;
    Ok((dwg, dxf.expect("export requested")))
}

/// Generate a drawing with the copied Samples floppy available in drive B,
/// for commands such as `LOAD B:ES` that need external shape definitions.
pub fn generate_pair_with_samples(
    system_disk: &Path,
    samples_disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (dwg, dxf) = run(
        system_disk,
        Some(samples_disk),
        name,
        editor_lines,
        true,
        None,
    )?;
    Ok((dwg, dxf.expect("export requested")))
}

/// Original drawing files plus its 16 KiB CGA memory immediately before END.
pub struct VisualProbe {
    pub dwg: Vec<u8>,
    pub dxf: Vec<u8>,
    pub cga: Vec<u8>,
}

pub fn generate_visual_pair(
    system_disk: &Path,
    samples_disk: Option<&Path>,
    name: &str,
    editor_lines: &[&str],
) -> Result<VisualProbe, String> {
    let mut cga = Vec::new();
    let (dwg, dxf) = run(
        system_disk,
        samples_disk,
        name,
        editor_lines,
        true,
        Some(&mut cga),
    )?;
    Ok(VisualProbe {
        dwg,
        dxf: dxf.expect("export requested"),
        cga,
    })
}

fn run(
    system_disk: &Path,
    samples_disk: Option<&Path>,
    name: &str,
    editor_lines: &[&str],
    export_dxf: bool,
    capture: Option<&mut Vec<u8>>,
) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    if name.is_empty()
        || name.len() > 8
        || !name
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err("drawing name must be 1–8 uppercase ASCII letters or digits".into());
    }
    let mut vm = Session::boot_disposable(system_disk, samples_disk, &[])?;
    vm.wait_for_text("Enter selection:", BOOT_TIMEOUT)?;
    vm.type_line("1")?;
    vm.wait_for_text("Enter NAME of drawing:", BOOT_TIMEOUT)
        .map_err(|e| format!("new drawing: {e}"))?;
    vm.type_line(name)?;
    // AutoCAD writes CGA bitplanes directly, so QEMU's VGA text screenshot
    // becomes garbled here. The keyboard and floppy continue to work.
    vm.wait_until_text_gone("Enter NAME of drawing:", BOOT_TIMEOUT)?;
    thread::sleep(Duration::from_millis(500));
    for line in editor_lines {
        vm.type_line(line)?;
    }
    if let Some(capture) = capture {
        *capture = vm.capture_editor()?;
    }
    vm.type_line("END")?;
    vm.wait_for_text("Current drawing:", EDIT_TIMEOUT)?;
    vm.wait_for_text("Enter selection:", EDIT_TIMEOUT)?;
    thread::sleep(Duration::from_millis(300));
    if export_dxf {
        vm.type_line("5")?;
        vm.wait_for_text("Enter NAME of drawing (default", BOOT_TIMEOUT)
            .map_err(|e| format!("DXF export: {e}"))?;
        vm.type_line(name)?;
        vm.wait_for_text("Drawing interchange file complete.", EDIT_TIMEOUT)?;
    }
    vm.shutdown()?;
    let image =
        fs::read(vm.dir().join("system.img")).map_err(|e| format!("read copied floppy: {e}"))?;
    let dwg = fat12_file(&image, &format!("{name}.DWG"))?;
    let dxf = if export_dxf {
        Some(fat12_file(&image, &format!("{name}.DXF"))?)
    } else {
        None
    };
    Ok((dwg, dxf))
}

/// Change only the directory extension; the backup's cluster chain and
/// drawing bytes are left intact. Names have been validated by `export`.
pub(crate) fn promote_backup(image: &mut [u8], name: &str) -> Result<(), String> {
    // Validate the source and its FAT chain before reading boot fields.
    fat12_file(image, &format!("{name}.BAK"))?;
    let word = |at| u16::from_le_bytes([image[at], image[at + 1]]) as usize;
    let root_start = (word(14) + image[16] as usize * word(22)) * word(11);
    let root_end = root_start + word(17) * 32;
    let mut old = [b' '; 11];
    old[..name.len()].copy_from_slice(name.as_bytes());
    old[8..].copy_from_slice(b"BAK");
    let mut new = old;
    new[8..].copy_from_slice(b"DWG");
    let root = &mut image[root_start..root_end];
    if root.chunks_exact(32).any(|row| row[..11] == new) {
        return Err(format!("refusing to replace existing {name}.DWG"));
    }
    let entry = root
        .chunks_exact_mut(32)
        .find(|row| row[..11] == old && row[11] & 0x18 == 0)
        .ok_or_else(|| format!("{name}.BAK missing from root"))?;
    entry[8..11].copy_from_slice(b"DWG");
    Ok(())
}

/// Read one 8.3 file from the original 360 KiB FAT12 floppy layout.
fn fat12_file(image: &[u8], filename: &str) -> Result<Vec<u8>, String> {
    fn le16(bytes: &[u8], at: usize) -> Result<usize, String> {
        let pair = bytes
            .get(at..at + 2)
            .ok_or_else(|| format!("short FAT12 image at {at}"))?;
        Ok(u16::from_le_bytes([pair[0], pair[1]]) as usize)
    }
    let sector = le16(image, 11)?;
    let cluster_sectors = *image.get(13).ok_or("short FAT12 boot sector")? as usize;
    let reserved = le16(image, 14)?;
    let fats = *image.get(16).ok_or("short FAT12 boot sector")? as usize;
    let root_entries = le16(image, 17)?;
    let fat_sectors = le16(image, 22)?;
    if sector != 512 || cluster_sectors == 0 || fats == 0 {
        return Err("unexpected FAT12 geometry".into());
    }
    let root_start = (reserved + fats * fat_sectors) * sector;
    let data_start = root_start + (root_entries * 32).div_ceil(sector) * sector;
    let fat = image
        .get(reserved * sector..(reserved + fat_sectors) * sector)
        .ok_or("FAT runs past image")?;
    let (stem, ext) = filename
        .split_once('.')
        .ok_or("FAT filename needs extension")?;
    if stem.len() > 8 || ext.len() > 3 {
        return Err("not an 8.3 filename".into());
    }
    let mut dos_name = [b' '; 11];
    dos_name[..stem.len()].copy_from_slice(stem.as_bytes());
    dos_name[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    let mut entry = None;
    for row in image
        .get(root_start..root_start + root_entries * 32)
        .ok_or("FAT root runs past image")?
        .chunks_exact(32)
    {
        if row[0..11] == dos_name && row[11] & 0x18 == 0 {
            entry = Some(row);
            break;
        }
    }
    let entry = entry.ok_or_else(|| format!("{filename} was not written to the floppy"))?;
    let mut remaining = u32::from_le_bytes(entry[28..32].try_into().unwrap()) as usize;
    if remaining > image.len() {
        return Err("FAT12 file size exceeds floppy image".into());
    }
    let mut cluster = le16(entry, 26)?;
    let mut out = Vec::with_capacity(remaining);
    let cluster_size = cluster_sectors * sector;
    while remaining > 0 {
        if !(2..0xff8).contains(&cluster) {
            return Err(format!("invalid FAT12 cluster {cluster:#x}"));
        }
        let offset = data_start + (cluster - 2) * cluster_size;
        let take = remaining.min(cluster_size);
        out.extend_from_slice(
            image
                .get(offset..offset + take)
                .ok_or("DWG cluster runs past floppy image")?,
        );
        remaining -= take;
        let at = cluster * 3 / 2;
        let word = le16(fat, at)?;
        cluster = if cluster & 1 == 0 {
            word & 0x0fff
        } else {
            word >> 4
        };
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_drawing_name_is_rejected_before_boot() {
        assert!(generate_dwg(Path::new("/missing"), "TOO-LONG-NAME", &[]).is_err());
    }

    #[test]
    fn backup_promotion_preserves_content_and_rejects_collisions() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/Samples.img");
        let Ok(original) = fs::read(path) else { return };
        let mut image = original.clone();
        let backup = fat12_file(&image, "DISC.BAK").unwrap();
        promote_backup(&mut image, "DISC").unwrap();
        assert_eq!(fat12_file(&image, "DISC.DWG").unwrap(), backup);
        assert!(fat12_file(&image, "DISC.BAK").is_err());
        assert_eq!(
            image.iter().zip(&original).filter(|(a, b)| a != b).count(),
            3
        );
        let saved = image.clone();
        assert!(promote_backup(&mut image, "HOUSE")
            .unwrap_err()
            .contains("existing HOUSE.DWG"));
        assert_eq!(image, saved);
    }

    #[test]
    fn fat12_reader_extracts_the_original_executable() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
        let Ok(image) = fs::read(path) else { return };
        let exe = fat12_file(&image, "ACAD.EXE").unwrap();
        assert_eq!(&exe[..2], b"MZ");
        assert_eq!(exe.len(), 78_848);
    }
}
