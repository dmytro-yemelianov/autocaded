//! Cached character-cell wrapping and source-anchored resize behavior.
use std::ops::Range;

#[derive(Default)]
pub(super) struct Layout {
    columns: usize,
    pub(super) lines: Vec<Range<usize>>,
}
impl Layout {
    pub(super) fn update(&mut self, text: &str, columns: usize) {
        if self.columns == columns && !self.lines.is_empty() {
            return;
        }
        self.columns = columns;
        self.lines.clear();
        let mut start = 0;
        for line in text.lines() {
            let end = start + line.len();
            if start == end {
                self.lines.push(start..end);
            }
            while start < end {
                // Ranges and navigation anchors are UTF-8 byte offsets, while
                // each rendered Unicode scalar takes exactly one bitmap cell.
                let mut next = text[start..end]
                    .char_indices()
                    .nth(columns.max(1))
                    .map_or(end, |(offset, _)| start + offset);
                if next < end {
                    if let Some(space) = text[start..next].rfind(' ') {
                        if space > 0 && !text[start..start + space].trim().is_empty() {
                            next = start + space + 1;
                        }
                    }
                }
                self.lines.push(start..next);
                start = next;
            }
            start = end + 1;
        }
        if self.lines.is_empty() {
            self.lines.push(0..0);
        }
    }
    pub(super) fn top(&self, anchor: usize, rows: usize) -> usize {
        self.lines
            .partition_point(|line| line.start <= anchor)
            .saturating_sub(1)
            .min(self.lines.len().saturating_sub(rows.max(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapping_retains_all_characters_blank_lines_and_long_tokens() {
        let text = "alpha beta gamma\n\n0123456789012345\n    indent\n";
        let mut layout = Layout::default();
        layout.update(text, 7);
        assert!(layout.lines.iter().all(|line| line.len() <= 7));
        let actual: String = layout
            .lines
            .iter()
            .map(|line| &text[line.clone()])
            .collect();
        assert_eq!(actual, text.replace('\n', ""));
        assert!(layout.lines.iter().any(Range::is_empty));
        assert_eq!(&text[layout.lines[0].clone()], "alpha ");
    }
    #[test]
    fn source_anchor_survives_width_changes_and_clamps_to_last_page() {
        let text = "0123456789012345678901234567890123456789";
        let mut layout = Layout::default();
        layout.update(text, 5);
        assert_eq!(layout.top(20, 2), 4);
        layout.update(text, 10);
        assert_eq!(layout.top(20, 2), 2);
        assert_eq!(layout.top(usize::MAX, 3), 1);
        layout.update("", 1);
        assert_eq!(layout.top(usize::MAX, 100), 0);
    }
    #[test]
    fn ukrainian_wraps_in_scalar_cells_and_retains_byte_boundary_anchors() {
        let text = "ҐїЄабвгде жзи\n\n0123і45678\n    яюь";
        let mut layout = Layout::default();
        for columns in [0, 1, 3, 5, 7, 9] {
            layout.update(text, columns);
            assert!(layout.lines.iter().all(|line| {
                text.is_char_boundary(line.start)
                    && text.is_char_boundary(line.end)
                    && text[line.clone()].chars().count() <= columns.max(1)
            }));
            let actual: String = layout
                .lines
                .iter()
                .map(|line| &text[line.clone()])
                .collect();
            assert_eq!(actual, text.replace('\n', ""));
            assert!(layout.lines.iter().any(Range::is_empty));
        }
        layout.update(text, 5);
        let anchor = layout.lines[2].start;
        layout.update(text, 3);
        let top = layout.top(anchor, 1);
        assert!(layout.lines[top].start <= anchor && anchor < layout.lines[top].end);
    }
}
