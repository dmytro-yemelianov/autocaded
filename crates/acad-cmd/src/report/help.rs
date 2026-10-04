//! Retained ACAD.HLP pages, independent of host files and drawing state.
use std::{collections::BTreeMap, sync::OnceLock};

const SOURCE: &str = include_str!("../../resources/acad.hlp");

pub(super) fn page(command: &str) -> Option<&'static str> {
    static PAGES: OnceLock<BTreeMap<&'static str, String>> = OnceLock::new();
    let pages = PAGES.get_or_init(|| {
        let mut pages = BTreeMap::new();
        let mut aliases = Vec::new();
        let mut body = Vec::new();
        fn finish(
            pages: &mut BTreeMap<&'static str, String>,
            aliases: &mut Vec<&'static str>,
            body: &mut Vec<&'static str>,
        ) {
            let text = format!("{}\n", body.join("\n").trim_matches('\n'));
            for alias in aliases.drain(..) {
                pages.insert(alias, text.clone());
            }
            body.clear();
        }
        for line in SOURCE.lines() {
            if let Some(name) = line.strip_prefix('\\') {
                if !body.is_empty() {
                    finish(&mut pages, &mut aliases, &mut body);
                }
                aliases.push(name);
            } else if !aliases.is_empty() {
                body.push(line);
            }
        }
        finish(&mut pages, &mut aliases, &mut body);
        pages
    });
    // These two names share SNAP state in the recovered command dispatcher;
    // the retained help file only labels that page SNAP.
    let topic = match command {
        "RES" | "RESOLUTION" => "SNAP",
        topic => topic,
    };
    pages.get(topic).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_labels_share_the_following_page() {
        for (a, b) in [("DIM", "DIMENSION"), ("COLOR", "COLORS"), ("HELP", "?")] {
            assert_eq!(page(a), page(b));
            assert!(!page(a).unwrap().trim().is_empty());
        }
        assert_eq!(page("RES"), page("SNAP"));
        assert_eq!(page("RESOLUTION"), page("SNAP"));
    }

    #[test]
    fn final_zoom_page_excludes_dos_eof_and_cluster_slack() {
        let text = page("ZOOM").unwrap();
        assert!(text.starts_with("The ZOOM command"));
        assert!(text.ends_with("Reference:  Section 4.3 of User Guide.\n"));
        assert!(!text.contains('\r') && !text.contains('\u{1a}') && !text.contains("\\ZOOM"));
    }
}
