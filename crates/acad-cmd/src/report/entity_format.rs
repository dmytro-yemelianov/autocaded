//! AutoCAD 1.4's LIST/DBLIST entity layout, read from the original's text
//! page (acad-oracle tests/reports.rs): a header line naming the record and
//! its layer, then one line per field with right-aligned labels.
//!
//! ```text
//!                   LINE      LAYER: 1
//!               from point, X=   2.0000  Y=   3.0000
//!             radius    1.5000
//!              angle   30
//!               text Hi there
//! ```
//!
//! Coordinates and lengths use the drawing's decimal UNITS precision (4
//! outside decimal units, a native choice); angles print as whole degrees and
//! INSERT scale factors with two decimals, as the original does.
use crate::entity_ops::bare;
use acad_model::{Block, Entity, Point, Repeat, UnitFormat, Units};

pub(crate) struct EntityFormat {
    decimals: usize,
}

impl EntityFormat {
    pub(crate) fn new(units: Units) -> Self {
        Self {
            decimals: match units.format {
                UnitFormat::Decimal => usize::from(units.precision),
                _ => 4,
            },
        }
    }

    fn header(&self, kind: &str, layer: u8) -> String {
        let width = (kind.chars().count() + 2).max(10);
        format!("{:18}{kind:<width$}LAYER: {layer}", "")
    }

    fn point(&self, label: &str, p: Point) -> String {
        let d = self.decimals;
        format!(
            "{:>25} X={:9.d$}  Y={:9.d$}",
            format!("{label} point,"),
            p.x,
            p.y
        )
    }

    fn scalar(&self, label: &str, value: f64) -> String {
        format!("{label:>18}{value:10.d$}", d = self.decimals)
    }

    fn angle(&self, label: &str, degrees: f64) -> String {
        format!("{label:>18}{degrees:5.0}")
    }

    fn text(&self, label: &str, value: &str) -> String {
        format!("{label:>18} {}", printable(value))
    }

    /// The lines for one record, recursing into REPEAT groups. Erased
    /// records print nothing.
    pub(crate) fn entity(&self, entity: &Entity, lines: &mut Vec<String>) {
        if entity.is_erased() {
            return;
        }
        let layer = crate::geometry::entity_layer(entity);
        match bare(entity) {
            Entity::Line { start, end } => {
                lines.push(self.header("LINE", layer));
                lines.push(self.point("from", *start));
                lines.push(self.point("to", *end));
            }
            Entity::Point { origin } => {
                lines.push(self.header("POINT", layer));
                lines.push(self.point("at", *origin));
            }
            Entity::Circle { center, radius } => {
                lines.push(self.header("CIRCLE", layer));
                lines.push(self.point("center", *center));
                lines.push(self.scalar("radius", *radius));
            }
            Entity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } => {
                lines.push(self.header("ARC", layer));
                lines.push(self.point("center", *center));
                lines.push(self.scalar("radius", *radius));
                lines.push(self.angle("from", *start_deg));
                lines.push(self.angle("to", *end_deg));
            }
            Entity::Text {
                origin,
                height,
                rotation_deg,
                value,
            } => {
                lines.push(self.header("TEXT", layer));
                lines.push(self.point("origin", *origin));
                lines.push(self.scalar("height", *height));
                lines.push(self.angle("angle", *rotation_deg));
                lines.push(self.text("text", value));
            }
            Entity::Shape {
                origin,
                height,
                rotation_deg,
                number,
            } => {
                lines.push(self.header("SHAPE", layer));
                lines.push(self.point("origin", *origin));
                lines.push(self.scalar("scale", *height));
                lines.push(self.angle("angle", *rotation_deg));
                lines.push(self.text("shape", &number.to_string()));
            }
            Entity::Load { name } => {
                lines.push(self.header("LOAD", layer));
                lines.push(self.text("file", name));
            }
            Entity::Insert {
                origin,
                x_scale,
                y_scale,
                rotation_deg,
                name,
            } => {
                lines.push(self.header("BLOCK REFERENCE", layer));
                lines.push(self.text("block", name));
                lines.push(self.point("at", *origin));
                lines.push(format!("{:>31}{x_scale:6.2}", "X scale factor"));
                lines.push(format!("{:>31}{y_scale:6.2}", "Y scale factor"));
                lines.push(self.angle("rotation angle", *rotation_deg));
            }
            Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
                let kind = if matches!(bare(entity), Entity::Trace { .. }) {
                    "TRACE"
                } else {
                    "SOLID"
                };
                lines.push(self.header(kind, layer));
                lines.push(self.point("from", *p1));
                lines.push(self.point("and", *p2));
                lines.push(self.point("to", *p3));
                lines.push(self.point("and", *p4));
            }
            Entity::Repeat(repeat) => self.repeat(repeat, lines),
            Entity::Generic(generic) => lines.push(self.header(&generic.type_name, layer)),
            Entity::Extension(extension) => {
                lines.push(self.header(extension.type_name(), layer));
            }
            Entity::OnLayer { .. } | Entity::Erased(_) => unreachable!("bare"),
        }
    }

    /// `REPEAT start`, the members, then `REPEAT end` with the array.
    pub(crate) fn repeat(&self, repeat: &Repeat, lines: &mut Vec<String>) {
        lines.push(self.header("REPEAT start", repeat.start_layer));
        for member in &repeat.entities {
            self.entity(member, lines);
        }
        lines.push(self.header("REPEAT end", repeat.end_layer));
        lines.push(format!("{:>18}{:4}", "# columns", repeat.columns));
        lines.push(format!("{:>18}{:4}", "# rows", repeat.rows));
        lines.push(self.scalar("column spacing", repeat.column_spacing));
        lines.push(self.scalar("row spacing", repeat.row_spacing));
        // Native addition: erased members (nested groups included) print
        // nothing above, so say how many there are.
        let erased = erased_members(repeat);
        if erased > 0 {
            lines.push(format!("{:>18}{erased:4}", "erased members"));
        }
    }

    /// A block definition between its BLOCK/END records. The model keeps no
    /// layer for those records; layer 1 (the default) is printed.
    pub(crate) fn block(&self, block: &Block, lines: &mut Vec<String>) {
        lines.push(self.header("BLOCK DEFINITION", 1));
        lines.push(self.text("block", &block.name));
        lines.push(self.point("origin", block.base));
        for entity in &block.entities {
            self.entity(entity, lines);
        }
        lines.push(self.header("END BLOCK DEFINITION", 1));
    }
}

/// Erased records beneath a group, nested groups included. Iterative: stored
/// nesting depth is bounded by the codecs, not by the call stack.
fn erased_members(repeat: &Repeat) -> usize {
    let mut erased = 0;
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
    erased
}

/// Text values go to a terminal-like viewer: escape control characters and
/// bound very long strings.
fn printable(text: &str) -> String {
    let mut chars = text.chars().flat_map(|c| {
        if c.is_control() {
            c.escape_default().collect::<Vec<_>>()
        } else {
            vec![c]
        }
    });
    let mut out: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}
