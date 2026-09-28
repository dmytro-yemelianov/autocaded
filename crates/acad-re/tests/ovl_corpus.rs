use acad_re::ovl::{self, Directory};

/// The corpus is extracted from archives that are deliberately not in git, so
/// a fresh checkout has none. Tests that need it skip rather than fail.
fn corpus(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(format!("../../corpus/{rel}")) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: corpus/{rel} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

fn directory() -> Option<(Vec<u8>, Directory)> {
    let bytes = corpus("System/ACAD.OVL")?;
    let dir = ovl::parse(&bytes).unwrap();
    Some((bytes, dir))
}

#[test]
fn the_real_overlay_declares_a_64768_byte_window() {
    let Some((_, dir)) = directory() else { return };
    assert_eq!(dir.window_bytes, 0xfd00);
    assert_eq!(dir.entries.len(), 11);
}

#[test]
fn the_largest_code_region_ends_exactly_at_the_window_size() {
    // Entry 2 is the big overlay: dest 0x3770 + len 0xc590 == 0xfd00. The window
    // is sized to its largest consumer, so this is the load-bearing coincidence
    // that confirms `dest` is a byte offset into the window.
    let Some((_, dir)) = directory() else { return };
    let e = dir.entries[2];
    assert_eq!(
        e.code.dest as u32 + e.code.len as u32,
        dir.window_bytes as u32
    );
}

#[test]
fn resident_entries_abut_in_both_windows() {
    // Entries 8 and 9 are loaded together and must not overlap: 8's code ends
    // where 9's begins, and likewise for data.
    let Some((_, dir)) = directory() else { return };
    let (a, b) = (dir.entries[8], dir.entries[9]);
    assert_eq!(a.code.dest + a.code.len, b.code.dest, "code windows abut");
    assert_eq!(a.data.dest + a.data.len, b.data.dest, "data windows abut");
}

#[test]
fn every_data_region_fits_below_the_exe_string_pool() {
    // Overlay data pages into the EXE's own data segment at DS:0x0002-0x3552;
    // DS:0x3553 is the first byte of `ACAD.EXE`'s string pool. A data region
    // reaching 0x3553 would overwrite "Not enough core for overlays".
    const STRING_POOL: u32 = 0x3553;
    let Some((_, dir)) = directory() else { return };
    for e in &dir.entries {
        assert!(
            e.data.dest as u32 + e.data.len as u32 <= STRING_POOL,
            "entry {} data region reaches {:#x}, into the string pool",
            e.index,
            e.data.dest as u32 + e.data.len as u32
        );
    }
}

#[test]
fn the_regions_cover_almost_the_whole_file_with_only_alignment_gaps() {
    let Some((bytes, dir)) = directory() else {
        return;
    };
    let mut spans: Vec<(u64, u64)> = dir
        .entries
        .iter()
        .flat_map(|e| [e.code, e.data])
        .filter(|r| !r.is_empty())
        .map(|r| (r.file_off as u64, r.end()))
        .collect();
    assert_eq!(spans.len(), 22, "11 entries x 2 regions, all non-empty");
    spans.sort_unstable();

    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let covered: u64 = merged.iter().map(|(s, e)| e - s).sum();
    let total = bytes.len() as u64;
    // 178,221 of 179,480 is 99.2986%, which the spec rounds to 99.3%. Assert on
    // ten-thousandths so integer truncation does not force the bound down to a
    // slacker 99.2%: 9929 passes on today's corpus and 9930 does not.
    assert!(
        covered * 10_000 / total >= 9_929,
        "regions cover {covered} of {total} bytes ({}.{:02}%), expected >= 99.29%",
        covered * 100 / total,
        covered * 10_000 / total % 100
    );

    // The payload starts at 0x100: the 211-byte header, padded to 256. That
    // leading gap is the header, not lost content, so the scan starts after it.
    assert_eq!(
        merged[0].0, 0x100,
        "payload should start just past the padded header"
    );

    // Every gap between regions is a pad to the next 0x80 boundary.
    let mut prev = 0x100u64;
    for (s, e) in &merged {
        if *s > prev {
            assert!(
                s - prev < 0x80,
                "gap {prev:#x}-{s:#x} is too large to be alignment padding"
            );
        }
        prev = *e;
    }
}
