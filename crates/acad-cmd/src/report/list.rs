//! Selected LIST details in AutoCAD 1.4's layout; the bounds are native
//! Rust policy.
use crate::selection::selectable_items;
use acad_model::{Drawing, Item};

pub(super) const MAX_LIST_OBJECTS: usize = 1_000;
const MAX_LIST_BYTES: usize = 256_000;

/// LIST: the selected records in AutoCAD 1.4's layout (`entity_format`),
/// bounded by object count and size.
pub(crate) fn list_report(drawing: &Drawing, ids: &[usize]) -> String {
    let format = super::entity_format::EntityFormat::new(drawing.header.units);
    let mut report = String::new();
    let mut listed = 0;
    let selected: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    let total = selected.len();
    for object in selectable_items(drawing)
        .filter(|object| selected.contains(&object.id))
        .take(MAX_LIST_OBJECTS)
    {
        let mut lines = Vec::new();
        match object.item {
            Item::Entity(entity) => format.entity(entity, &mut lines),
            Item::Repeat(repeat) => format.repeat(repeat, &mut lines),
            _ => unreachable!(),
        }
        let text = lines.join("\n");
        if report.len() + text.len() + 1 > MAX_LIST_BYTES {
            break;
        }
        if !report.is_empty() {
            report.push('\n');
        }
        report.push_str(&text);
        listed += 1;
    }
    if listed < total {
        report.push_str(&format!(
            "\nLIST limit: {listed} of {total} objects detailed; {} omitted",
            total - listed
        ));
    }
    report
}
