//! Selected LIST details. Field layout and bounds are native Rust policy.
use crate::{
    entity_ops::bare, geometry::entity_layer, parse::format_measurement,
    selection::selectable_items,
};
use acad_model::{Drawing, Entity, Item, Point, Repeat};

pub(super) const MAX_LIST_OBJECTS: usize = 1_000;
const MAX_LIST_BYTES: usize = 256_000;

fn label(text: &str) -> String {
    let mut chars = text.chars().flat_map(char::escape_default);
    let mut out: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn repeat_details(repeat: &Repeat, drawing: &Drawing) -> String {
    let mut detail = format!(
        "members={} columns={} rows={} column spacing={} row spacing={} (whole group)",
        repeat.entities.len(),
        repeat.columns,
        repeat.rows,
        format_measurement(repeat.column_spacing, drawing.header.units),
        format_measurement(repeat.row_spacing, drawing.header.units)
    );
    // Erased stored leaves anywhere beneath this group, including nested groups.
    // Iterative: stored nesting depth is bounded by codecs, not by the call stack.
    let mut erased = 0usize;
    let mut stack = vec![repeat.entities.iter()];
    while let Some(entities) = stack.last_mut() {
        match entities.next() {
            None => {
                stack.pop();
            }
            Some(entity) if entity.is_erased() => erased += 1,
            Some(entity) => {
                if let Entity::Repeat(nested) = bare(entity) {
                    stack.push(nested.entities.iter());
                }
            }
        }
    }
    if erased > 0 {
        detail.push_str(&format!(" erased members={erased}"));
    }
    detail
}

fn details(entity: &Entity, drawing: &Drawing) -> String {
    let value = |number| format_measurement(number, drawing.header.units);
    let point = |p: Point| format!("({},{})", value(p.x), value(p.y));
    let geometry = match bare(entity) {
        Entity::Line { start, end } => format!("start={} end={}", point(*start), point(*end)),
        Entity::Circle { center, radius } => {
            format!("center={} radius={}", point(*center), value(*radius))
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => format!(
            "center={} radius={} start angle={start_deg} end angle={end_deg}",
            point(*center),
            value(*radius)
        ),
        Entity::Point { origin } => format!("origin={}", point(*origin)),
        Entity::Text {
            origin,
            height,
            rotation_deg,
            value: text,
        } => format!(
            "origin={} height={} angle={} text=\"{}\"",
            point(*origin),
            value(*height),
            rotation_deg,
            label(text)
        ),
        Entity::Shape {
            origin,
            height,
            rotation_deg,
            number,
        } => format!(
            "origin={} height={} angle={} shape={}",
            point(*origin),
            value(*height),
            rotation_deg,
            number
        ),
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            name,
        } => format!(
            "origin={} X scale={} Y scale={} angle={} block=\"{}\"",
            point(*origin),
            x_scale,
            y_scale,
            rotation_deg,
            label(name)
        ),
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => format!(
            "p1={} p2={} p3={} p4={}",
            point(*p1),
            point(*p2),
            point(*p3),
            point(*p4)
        ),
        Entity::Repeat(repeat) => repeat_details(repeat, drawing),
        Entity::Load { .. } | Entity::OnLayer { .. } | Entity::Erased(_) => unreachable!(),
    };
    format!("layer={} {geometry}", entity_layer(entity))
}

pub(crate) fn list_report(drawing: &Drawing, ids: &[usize]) -> String {
    let mut report = super::list_entities(drawing, ids);
    let mut listed = 0;
    let selected: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    let total = selected.len();
    for object in selectable_items(drawing)
        .filter(|object| selected.contains(&object.id))
        .take(MAX_LIST_OBJECTS)
    {
        let detail = match object.item {
            Item::Entity(entity) => details(entity, drawing),
            Item::Repeat(repeat) => repeat_details(repeat, drawing),
            _ => unreachable!(),
        };
        let line = format!("\n{}: {}", object.id, detail);
        if report.len() + line.len() > MAX_LIST_BYTES {
            break;
        }
        report.push_str(&line);
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
