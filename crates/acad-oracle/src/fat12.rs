//! Read files from the original 360 KiB FAT12 oracle floppy.

/// Read one 8.3 file from the original 360 KiB FAT12 floppy layout.
pub(crate) fn fat12_file(image: &[u8], filename: &str) -> Result<Vec<u8>, String> {
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
