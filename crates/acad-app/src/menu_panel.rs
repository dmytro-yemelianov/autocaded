use acad_cmd::menu::{MenuEntry, MenuEntryKind, MenuFile};

use crate::bitmap::{draw_rect, draw_text, GLYPH_HEIGHT, GLYPH_WIDTH};

/// Label used for the synthetic `< GO >` header row. It is only ever drawn
/// on the page that contains the `MenuFile`'s single `Header`-kind entry
/// (`[< GO >];`, `ACAD.MNU` line 1) — that entry is not itself a numbered
/// page item, per Task 1's recovery.
const GO_LABEL: &str = "< GO >";

/// Label for the synthetic final row every page gets. `NEXT` is not stored
/// in `MenuFile` at all (there is no 58th `.MNU` line for it) — Task 1
/// confirmed it is the native renderer's own always-present control, so
/// `draw_panel` appends it itself.
const NEXT_LABEL: &str = "NEXT";

/// The fixed-width right-hand panel rectangle and row height for one frame.
/// Always recomputed fresh by `layout_for` — never cached across frames,
/// so a window resize is reflected immediately.
#[allow(dead_code)]
pub(crate) struct PanelLayout {
    pub rect: (usize, usize, usize, usize),
    pub row_height: usize,
}

impl PanelLayout {
    /// Blank slots and the divider are panel chrome too.
    pub(crate) fn contains(&self, x: f64, y: f64) -> bool {
        let (left, top, width, height) = self.rect;
        x >= left as f64
            && y >= top as f64
            && x < (left + width) as f64
            && y < (top + height) as f64
    }
}

/// Returns `menu`'s entries with the single `Header`-kind entry (if any)
/// stripped off the front. The header (`[< GO >];`) is a fixed title-row
/// slot, not a numbered page item, so it is never part of any page's item
/// list.
fn item_entries(menu: &MenuFile) -> &[MenuEntry] {
    if menu.entries.first().map(|entry| entry.kind) == Some(MenuEntryKind::Header) {
        &menu.entries[1..]
    } else {
        &menu.entries[..]
    }
}

/// Computes page-start indices (into `items`) per Task 1's recovered rule:
/// pagination is driven by `MenuEntry.repeat == true` (the `.MNU` source's
/// leading `*`, e.g. `*POINT`/`*LIMITS` in `corpus/System/ACAD.MNU`), not
/// by any fixed per-page entry count. Page 0 always starts at index 0;
/// every subsequent `repeat == true` entry starts a new page.
fn page_starts(items: &[MenuEntry]) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, entry) in items.iter().enumerate().skip(1) {
        if entry.repeat {
            starts.push(index);
        }
    }
    starts
}

/// Returns the slice of `items` belonging to `page`, wrapping around (per
/// Task 1's recovered `NEXT`-wraps-to-page-0 behavior) if `page` is past
/// the last page.
fn page_slice<'a>(items: &'a [MenuEntry], starts: &[usize], page: usize) -> &'a [MenuEntry] {
    if starts.is_empty() {
        return &[];
    }
    let page = page % starts.len();
    let start = starts[page];
    let end = starts.get(page + 1).copied().unwrap_or(items.len());
    &items[start..end]
}

/// True only for the one page that owns the `MenuFile`'s single `Header`
/// entry (always page 0, when a header entry exists at all).
fn has_header(menu: &MenuFile, page: usize) -> bool {
    let starts = page_starts(item_entries(menu));
    if starts.is_empty() {
        return false;
    }
    page % starts.len() == 0
        && menu.entries.first().map(|entry| entry.kind) == Some(MenuEntryKind::Header)
}

/// The largest number of items any page holds. Used so every page's panel
/// occupies the same fixed row count (Task 1's recovered bottom-anchored
/// layout: a page with fewer items leaves blank rows at the *top*, not a
/// shorter panel).
fn max_items_per_page(items: &[MenuEntry], starts: &[usize]) -> usize {
    (0..starts.len())
        .map(|page| page_slice(items, starts, page).len())
        .max()
        .unwrap_or(0)
}

/// Complete panel dimensions in physical client pixels, shared by layout
/// and the loaded-menu window minimum. No row compression or new pagination.
pub(crate) fn required_panel_size(menu: &MenuFile) -> (u32, u32) {
    let scale = 2; // matches draw_text's internal 2x glyph scale
    let row_height = GLYPH_HEIGHT * scale;

    let items = item_entries(menu);
    let starts = page_starts(items);
    let max_items = max_items_per_page(items, &starts);

    // Fixed-height region, matching Task 1's recovered 21-row panel
    // (1 header/blank row + up to 19 item rows + 1 synthetic NEXT row for
    // `ACAD.MNU`, generalized here to `max_items` instead of the literal
    // 19 so a differently-shaped `.MNU` still lays out consistently):
    // every page occupies the same total height regardless of its own
    // item count.
    let total_rows = 1 + max_items + 1;
    let panel_height = total_rows * row_height;

    // Fixed-width column: wide enough for the longest label across every
    // page (so the panel doesn't change width when `NEXT` is clicked),
    // plus one glyph-cell of padding on each side.
    let longest = items
        .iter()
        .map(|entry| entry.label.chars().count())
        .chain([GO_LABEL.chars().count(), NEXT_LABEL.chars().count()])
        .max()
        .unwrap_or(0);
    let padding = GLYPH_WIDTH * scale;
    let panel_width = longest * GLYPH_WIDTH * scale + 2 * padding;

    (panel_width as u32, panel_height as u32)
}

/// Computes the current page's panel rectangle and row height. Pure and
/// cheap enough to call fresh every frame — callers must not cache the
/// result across frames, since the window can resize at any time.
///
/// Row height is pinned to Task 1's recovered native geometry: AutoCAD
/// 1.4's screen-menu renderer draws each row 16 display pixels tall
/// (`docs/superpowers/plans/2026-10-01-screen-menu-rendering.md`, Task 1
/// Step 3's "Clickable-slot geometry" finding) — exactly `GLYPH_HEIGHT * 2`
/// here, since `draw_text` already scales the embedded 8x8 font 2x.
pub(crate) fn layout_for(
    menu: &MenuFile,
    // Geometry is identical on every page (Task 1's recovered fixed 21-row,
    // bottom-anchored panel), so `page` does not affect the computed rect —
    // it stays in the signature to match `draw_panel`'s and the plan's
    // per-page interface contract.
    _page: usize,
    window_width: u32,
    window_height: u32,
) -> PanelLayout {
    let (required_width, required_height) = required_panel_size(menu);
    let panel_width = required_width.min(window_width) as usize;
    let panel_height = required_height.min(window_height) as usize;
    let row_height = GLYPH_HEIGHT * 2;

    let x = (window_width as usize).saturating_sub(panel_width);
    PanelLayout {
        rect: (x, 0, panel_width, panel_height),
        row_height,
    }
}

/// Draws the current page of `menu`'s screen menu as a right-hand panel,
/// following Task 1's recovered layout: row 0 is the header/blank slot
/// (only page 0 draws `< GO >` there), item rows are bottom-anchored
/// within the header+item region (a page with fewer than the maximum
/// item count leaves blank rows at the top, immediately below the header
/// slot, never next to `NEXT`), and the final row is always the
/// synthetic `NEXT` control, appended here rather than stored in
/// `MenuFile`.
#[allow(dead_code, clippy::too_many_arguments)]
pub(crate) fn draw_panel(
    buffer: &mut [u32],
    buffer_width: u32,
    buffer_height: u32,
    menu: &MenuFile,
    page: usize,
    layout: &PanelLayout,
) {
    const BACKGROUND: u32 = 0x0000_0000;
    const BORDER: u32 = 0x0080_8080;
    const TEXT: u32 = 0x00FF_FFFF;

    let (x, y, w, h) = layout.rect;
    let buffer_width_px = buffer_width as usize;
    let buffer_height_px = buffer_height as usize;
    if w == 0 || h == 0 {
        return;
    }

    draw_rect(
        buffer,
        buffer_width_px,
        buffer_height_px,
        x,
        y,
        w,
        h,
        BACKGROUND,
    );
    // Left-edge divider line, echoing Task 1's recovered 4px-wide vertical
    // divider that separates the panel from the drawing area.
    draw_rect(
        buffer,
        buffer_width_px,
        buffer_height_px,
        x,
        y,
        2,
        h,
        BORDER,
    );

    let text_x = x + GLYPH_WIDTH; // half a scaled glyph-cell of left margin

    let items = item_entries(menu);
    let starts = page_starts(items);
    if starts.is_empty() {
        return;
    }
    let page_items = page_slice(items, &starts, page);
    let max_items = max_items_per_page(items, &starts);

    if has_header(menu, page) {
        draw_text(buffer, buffer_width_px, text_x, y, GO_LABEL, TEXT);
    }

    // Bottom-anchor this page's items within the header+item region: the
    // leading gap is however many fewer items this page has than the
    // busiest page, so short pages leave blank rows at the top instead of
    // shrinking the row that `NEXT` sits on.
    let leading_gap = max_items - page_items.len();
    for (offset, entry) in page_items.iter().enumerate() {
        let row = 1 + leading_gap + offset;
        draw_text(
            buffer,
            buffer_width_px,
            text_x,
            y + row * layout.row_height,
            &entry.label,
            TEXT,
        );
    }

    let next_row = 1 + max_items;
    draw_text(
        buffer,
        buffer_width_px,
        text_x,
        y + next_row * layout.row_height,
        NEXT_LABEL,
        TEXT,
    );
}

/// What a click inside the panel rect resolved to, in `draw_panel`'s own
/// row-numbering (row 0 = header/blank slot, rows `1..=max_items` = item
/// rows, final row = synthetic `NEXT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelHit {
    /// A click on one of this page's item rows. The `usize` is a row index
    /// *relative to this page's displayed item slice* (i.e. an index into
    /// `page_slice(item_entries(menu), page_starts(...), page)`), already
    /// adjusted for the leading-gap of blank rows that `draw_panel` leaves
    /// at the top of a short page — NOT a raw index into `menu.entries`,
    /// and NOT a row number. Resolve it the rest of the way to a
    /// `MenuEntry` with `resolve_entry`.
    Entry(usize),
    /// A click on the synthetic final `NEXT` row.
    Next,
    /// A click on row 0 of the one page that owns the header entry.
    Go,
}

/// Hit-tests a click at buffer-space `(x, y)` against `layout`'s rect,
/// returning which row of the *current page* it landed on, in `draw_panel`'s
/// own row-numbering scheme (re-read above `draw_panel`'s loop: row 0 is the
/// header/blank slot, item rows are `1 + leading_gap + offset`, and the
/// final row is always `1 + max_items`).
///
/// Needs `menu` and `page` (not just `layout`) because which row is the
/// header/blank slot vs. an item row vs. `NEXT` depends on `has_header` and
/// `max_items_per_page`, exactly like `draw_panel` itself.
pub(crate) fn entry_at(
    menu: &MenuFile,
    page: usize,
    layout: &PanelLayout,
    x: f64,
    y: f64,
) -> Option<PanelHit> {
    let (_, rect_y, _, _) = layout.rect;
    if !layout.contains(x, y) {
        return None;
    }
    if layout.row_height == 0 {
        return None;
    }
    let row = ((y - rect_y as f64) / layout.row_height as f64) as usize;

    let items = item_entries(menu);
    let starts = page_starts(items);
    if starts.is_empty() {
        return None;
    }
    let page_items = page_slice(items, &starts, page);
    let max_items = max_items_per_page(items, &starts);
    let next_row = 1 + max_items;

    if row == 0 {
        return if has_header(menu, page) {
            Some(PanelHit::Go)
        } else {
            None
        };
    }
    if row == next_row {
        return Some(PanelHit::Next);
    }
    if row > next_row {
        return None;
    }
    // row is in 1..next_row: an item row. Undo draw_panel's leading_gap
    // offset (`row = 1 + leading_gap + offset`) to recover `offset`, the
    // index into `page_items` — this is the fix for the brief's bug: a
    // page-relative row is NOT an index into `menu.entries`.
    let leading_gap = max_items - page_items.len();
    let offset = row - 1;
    if offset < leading_gap {
        // Clicked a blank row above this page's first item.
        return None;
    }
    let item_index = offset - leading_gap;
    if item_index >= page_items.len() {
        return None;
    }
    Some(PanelHit::Entry(item_index))
}

/// Resolves a `PanelHit::Entry(index)` (a page-relative row index, per
/// `entry_at`'s contract) to the real `MenuEntry` it refers to. This is the
/// index-space fix the brief's sample code got wrong: `index` is an offset
/// into `page_slice(item_entries(menu), page_starts(item_entries(menu)),
/// page)`, not into `menu.entries` directly, because `item_entries` strips
/// the leading `Header` entry and `page_slice` sub-slices what's left.
pub(crate) fn resolve_entry(menu: &MenuFile, page: usize, index: usize) -> Option<&MenuEntry> {
    let items = item_entries(menu);
    let starts = page_starts(items);
    if starts.is_empty() {
        return None;
    }
    let page_items = page_slice(items, &starts, page);
    page_items.get(index)
}

/// The number of pages `menu` lays out into, i.e. `page_starts(...).len()`.
/// Callers (e.g. advancing past `NEXT`) wrap modulo this value, matching
/// `page_slice`'s own wrapping.
pub(crate) fn page_count(menu: &MenuFile) -> usize {
    page_starts(item_entries(menu)).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_nonspace_character_in_shipped_labels_renders_on_every_page() {
        let Some(menu) = acad_mnu() else {
            return;
        };
        let (width, height) = required_panel_size(&menu);
        let items = item_entries(&menu);
        let starts = page_starts(items);
        let max_items = max_items_per_page(items, &starts);
        for page in 0..starts.len() {
            let layout = layout_for(&menu, page, width, height);
            let mut buffer = vec![0; width as usize * height as usize];
            draw_panel(&mut buffer, width, height, &menu, page, &layout);
            let page_items = page_slice(items, &starts, page);
            let labels = page_items.iter().enumerate().map(|(offset, entry)| {
                (
                    1 + max_items - page_items.len() + offset,
                    entry.label.as_str(),
                )
            });
            let header = has_header(&menu, page).then_some((0, GO_LABEL));
            for (row, label) in labels.chain(header).chain([(1 + max_items, NEXT_LABEL)]) {
                for (column, ch) in label.chars().enumerate().filter(|(_, ch)| *ch != ' ') {
                    let left = layout.rect.0 + GLYPH_WIDTH + column * GLYPH_WIDTH * 2;
                    let top = row * layout.row_height;
                    assert!(
                        (top..top + layout.row_height).any(|y| {
                            (left..left + GLYPH_WIDTH * 2)
                                .any(|x| buffer[y * width as usize + x] == 0x00ff_ffff)
                        }),
                        "page {page} label {label:?}: missing {ch:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_full_lowercase_label_renders_each_letter_without_uppercasing() {
        let label = "abcdefghijklmnopqrstuvwxyz";
        let width = label.len() * GLYPH_WIDTH * 2;
        let height = GLYPH_HEIGHT * 2;
        let mut buffer = vec![0; width * height];
        draw_text(&mut buffer, width, 0, 0, label, 1);
        for (column, ch) in label.chars().enumerate() {
            let left = column * GLYPH_WIDTH * 2;
            assert!(
                (0..height).any(|y| {
                    (left..left + GLYPH_WIDTH * 2).any(|x| buffer[y * width + x] == 1)
                }),
                "missing lowercase {ch}"
            );
        }
        // Lowercase 'a' starts below the cap height and has a curved bowl;
        // this checks the encoded shape, rather than a fallback to uppercase.
        assert!(
            buffer[..2 * width].contains(&1),
            "ascenders elsewhere in the alphabet must remain visible"
        );
        assert!((0..4).all(|y| buffer[y * width..y * width + 16].iter().all(|&p| p == 0)));
        assert_eq!(
            &buffer[4 * width..4 * width + 16],
            &[0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0]
        );
    }

    #[test]
    fn draw_rect_fills_only_the_requested_area_and_clips_at_buffer_edges() {
        let (width, height) = (10usize, 10usize);
        let mut buffer = vec![0u32; width * height];
        draw_rect(&mut buffer, width, height, 2, 2, 100, 100, 0xFF0000);
        // Clipped to the buffer: every pixel from (2,2) onward is filled,
        // nothing before row/column 2 is touched.
        assert_eq!(buffer[2 * width + 2], 0xFF0000);
        assert_eq!(buffer[9 * width + 9], 0xFF0000);
        assert_eq!(buffer[width + 1], 0);
        assert_eq!(buffer[0], 0);
    }

    #[test]
    fn draw_text_renders_a_known_glyph_and_stops_at_buffer_width() {
        let (width, height) = (GLYPH_WIDTH * 2, GLYPH_HEIGHT);
        let mut buffer = vec![0u32; width * height];
        draw_text(&mut buffer, width, 0, 0, "AB", 0xFFFFFF);
        // "A" must differ from an untouched buffer (some pixel in its cell is lit).
        let a_cell: Vec<u32> = (0..GLYPH_HEIGHT)
            .flat_map(|row| (0..GLYPH_WIDTH).map(move |col| (row, col)))
            .map(|(row, col)| buffer[row * width + col])
            .collect();
        assert!(
            a_cell.contains(&0xFFFFFF),
            "expected 'A' to draw at least one lit pixel"
        );
        // A third character past the 2-glyph-wide buffer must not panic or
        // write out of bounds — re-running with "ABC" on the same buffer
        // size must still return normally.
        draw_text(&mut buffer, width, 0, 0, "ABC", 0xFFFFFF);
    }

    fn acad_mnu() -> Option<MenuFile> {
        let bytes = crate::corpus_file("System/ACAD.MNU")?;
        Some(acad_cmd::menu::parse_menu(&bytes).unwrap())
    }

    #[test]
    fn page_boundaries_follow_repeat_marked_entries_not_a_fixed_count() {
        // Task 1's recovery (docs/superpowers/plans/2026-10-01-screen-menu-rendering.md,
        // Task 1 Step 3) found AutoCAD 1.4's native screen-menu pagination is
        // driven entirely by MenuEntry.repeat == true (the `*POINT`/`*LIMITS`
        // lines), landing at uneven page sizes of 19/18/19 — not a fixed N
        // per page. This pins that scan, not a hardcoded page size.
        let Some(menu) = acad_mnu() else {
            return;
        };
        let items = item_entries(&menu);
        let starts = page_starts(items);
        let lengths: Vec<usize> = (0..starts.len())
            .map(|page| page_slice(items, &starts, page).len())
            .collect();
        assert_eq!(
            lengths,
            vec![19, 18, 19],
            "page sizes must come from the repeat-marked entries' real positions, not a constant"
        );
        // Task 1 also found a third NEXT click wraps back to page 0 byte-for-byte.
        assert_eq!(page_slice(items, &starts, 3), page_slice(items, &starts, 0));
        assert_eq!(page_slice(items, &starts, 4), page_slice(items, &starts, 1));
    }

    #[test]
    fn layout_for_uses_the_recovered_row_height_and_a_fixed_panel_height_across_pages() {
        let Some(menu) = acad_mnu() else {
            return;
        };
        let layout = layout_for(&menu, 0, 640, 480);
        assert!(
            layout.rect.2 > 0 && layout.rect.3 > 0,
            "panel rect must have positive size"
        );
        // Task 1's recovered native row height: 16 display pixels per row,
        // which is exactly GLYPH_HEIGHT * 2 given draw_text's 2x glyph scale.
        assert_eq!(layout.row_height, GLYPH_HEIGHT * 2);
        // Task 1's recovered panel is always 21 rows tall regardless of page
        // (1 header/blank row + up to 19 item rows + 1 synthetic NEXT row).
        assert_eq!(layout.rect.3, 21 * layout.row_height);
        let layout_page1 = layout_for(&menu, 1, 640, 480);
        let layout_page2 = layout_for(&menu, 2, 640, 480);
        assert_eq!(
            layout_page1.rect.3, layout.rect.3,
            "page 1 must keep the same fixed panel height as page 0"
        );
        assert_eq!(
            layout_page2.rect.3, layout.rect.3,
            "page 2 must keep the same fixed panel height as page 0"
        );
    }

    #[test]
    fn draw_panel_leaves_blank_rows_at_the_top_for_shorter_pages_and_keeps_next_fixed() {
        let Some(menu) = acad_mnu() else {
            return;
        };
        let (width, height) = (640u32, 480u32);
        let layout = layout_for(&menu, 1, width, height);
        let mut buffer = vec![0u32; width as usize * height as usize];
        draw_panel(&mut buffer, width, height, &menu, 1, &layout);

        let (x, y, _, _) = layout.rect;
        let text_x = x + GLYPH_WIDTH;
        let is_row_lit = |buffer: &[u32], row: usize| {
            let row_y = y + row * layout.row_height;
            (row_y..row_y + layout.row_height).any(|py| {
                (text_x..text_x + GLYPH_WIDTH * 2).any(|px| buffer[py * width as usize + px] != 0)
            })
        };

        // Page 1 (18 items, no header) leaves rows 0 and 1 blank per Task 1's
        // recovery, with its first item (`POINT`) starting at row 2.
        assert!(
            !is_row_lit(&buffer, 0),
            "row 0 must be blank on a headerless page"
        );
        assert!(
            !is_row_lit(&buffer, 1),
            "row 1 must be blank on page 1 (18 items, max is 19)"
        );
        assert!(
            is_row_lit(&buffer, 2),
            "row 2 must hold page 1's first item"
        );
        // NEXT is always the fixed final row, 1 + max_items_per_page.
        assert!(
            is_row_lit(&buffer, 20),
            "row 20 must always hold the synthetic NEXT row"
        );
    }

    #[test]
    fn entry_at_resolves_a_click_inside_a_row_and_none_outside_the_panel() {
        let Some(menu) = acad_mnu() else {
            return;
        };
        let layout = layout_for(&menu, 0, 640, 480);
        let (rect_x, rect_y, _, _) = layout.rect;
        let row_height = layout.row_height;
        // Page 0 has a header, so row 1 (not row 0, which is the `< GO >`
        // header slot) is its first item row.
        let inside = entry_at(
            &menu,
            0,
            &layout,
            (rect_x + 2) as f64,
            (rect_y + row_height + 2) as f64,
        );
        assert!(matches!(inside, Some(PanelHit::Entry(_))));
        let outside = entry_at(&menu, 0, &layout, 0.0, 0.0);
        assert_eq!(outside, None);
    }

    #[test]
    fn entry_at_resolves_row_0_on_page_0_to_go_and_on_page_1_to_none() {
        let Some(menu) = acad_mnu() else {
            return;
        };
        let layout = layout_for(&menu, 0, 640, 480);
        let (rect_x, rect_y, _, _) = layout.rect;
        assert_eq!(
            entry_at(&menu, 0, &layout, (rect_x + 2) as f64, (rect_y + 2) as f64),
            Some(PanelHit::Go)
        );
        // Page 1 has no header entry, so row 0 is just a blank slot, not Go.
        assert_eq!(
            entry_at(&menu, 1, &layout, (rect_x + 2) as f64, (rect_y + 2) as f64),
            None
        );
    }

    #[test]
    fn entry_at_resolves_the_final_row_to_next_on_every_page() {
        let Some(menu) = acad_mnu() else {
            return;
        };
        let layout = layout_for(&menu, 0, 640, 480);
        let (rect_x, rect_y, _, _) = layout.rect;
        let row_height = layout.row_height;
        let next_row_y = rect_y + 20 * row_height + 2;
        assert_eq!(
            entry_at(&menu, 0, &layout, (rect_x + 2) as f64, next_row_y as f64),
            Some(PanelHit::Next)
        );
        assert_eq!(
            entry_at(&menu, 1, &layout, (rect_x + 2) as f64, next_row_y as f64),
            Some(PanelHit::Next)
        );
    }

    #[test]
    fn entry_at_on_page_1_row_2_resolves_through_resolve_entry_to_the_point_entry() {
        // Worked example from the task brief: page 1 (ACAD.MNU's second
        // page, stripped-index 19 onward) is headerless with 18 items (max
        // is 19), so draw_panel leaves a 1-row leading gap and the first
        // item (`*POINT`, stripped index 19) is drawn at row 2. Clicking
        // that row must resolve — through entry_at's page-relative index and
        // resolve_entry's un-offsetting — to the entry labeled "POINT" with
        // repeat == true, not to whatever sits at menu.entries[19] or [20].
        let Some(menu) = acad_mnu() else {
            return;
        };
        let page = 1;
        let layout = layout_for(&menu, page, 640, 480);
        let (rect_x, rect_y, _, _) = layout.rect;
        let row_height = layout.row_height;
        let row_2_y = rect_y + 2 * row_height + 2;
        let hit = entry_at(&menu, page, &layout, (rect_x + 2) as f64, row_2_y as f64);
        let Some(PanelHit::Entry(index)) = hit else {
            panic!("expected an Entry hit at page 1 row 2, got {hit:?}");
        };
        let entry = resolve_entry(&menu, page, index).expect("resolved entry must exist");
        assert_eq!(entry.label, "POINT");
        assert!(entry.repeat);
        assert_eq!(entry.action, b"POINT");

        // Confirm this is NOT the same as naively indexing menu.entries by
        // the page-relative index (the brief's bug): menu.entries[0] is the
        // `< GO >` header, so a raw index would be off by the header plus
        // this page's own start offset.
        assert_ne!(menu.entries[index].label, "POINT");
    }

    #[test]
    fn page_count_reports_the_number_of_pages_acad_mnu_lays_out_into() {
        let Some(menu) = acad_mnu() else {
            return;
        };
        assert_eq!(page_count(&menu), 3);
    }
}
