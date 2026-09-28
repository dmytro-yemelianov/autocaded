//! Prints where each DXF entity's coordinates live inside the DWG sibling.
//!
//!     cargo run -p acad-dwg --example dwg-discover -- \
//!         corpus/Samples/SUBDIV.DWG corpus/Samples/SUBDIV.DXF
//!
//! Reading its output is how a record layout gets recovered; nothing here is
//! part of the shipped read path.
//!
//! This lives under `examples/` rather than `src/bin/` because it needs
//! `acad_dxf` to parse the DXF oracle, and only examples may use
//! dev-dependencies — a shipped binary in `src/bin/` cannot.
use acad_dwg::discover::find_f64;
use acad_model::Entity;
use std::{fs, process::ExitCode};

/// The DXF prints six decimals, so a hit can be off by half of the last place.
const TOLERANCE: f64 = 5e-7;

/// `Entity` is an enum of struct variants — `Entity::Line { start, end }`, not
/// `Entity::Line(line)` — with eight kinds since Task 8 added `Point`,
/// `Trace` and `Solid`. Spelling is American: `center`, `start_deg`,
/// `end_deg`.
fn coords(e: &Entity) -> Vec<(&'static str, f64)> {
    match e {
        Entity::Line { start, end } => vec![
            ("x1", start.x),
            ("y1", start.y),
            ("x2", end.x),
            ("y2", end.y),
        ],
        Entity::Circle { center, radius } => {
            vec![("cx", center.x), ("cy", center.y), ("r", *radius)]
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => vec![
            ("cx", center.x),
            ("cy", center.y),
            ("r", *radius),
            ("start", *start_deg),
            ("end", *end_deg),
        ],
        Entity::Text {
            origin,
            height,
            rotation_deg,
            ..
        } => vec![
            ("x", origin.x),
            ("y", origin.y),
            ("h", *height),
            ("rot", *rotation_deg),
        ],
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            ..
        } => vec![
            ("x", origin.x),
            ("y", origin.y),
            ("xs", *x_scale),
            ("ys", *y_scale),
            ("rot", *rotation_deg),
        ],
        // acad_dxf::parse never produces these (no corpus DXF exercises
        // POINT/TRACE/SOLID — see acad-dwg's entity.rs module doc), so this
        // arm never actually runs against real discovery input; it exists
        // to keep this match exhaustive now that acad_dwg::parse can
        // produce them.
        Entity::Point { origin } => vec![("x", origin.x), ("y", origin.y)],
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => vec![
            ("x1", p1.x),
            ("y1", p1.y),
            ("x2", p2.x),
            ("y2", p2.y),
            ("x3", p3.x),
            ("y3", p3.y),
            ("x4", p4.x),
            ("y4", p4.y),
        ],
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(dwg_path), Some(dxf_path)) = (args.next(), args.next()) else {
        eprintln!("usage: dwg-discover <file.DWG> <file.DXF>");
        return ExitCode::FAILURE;
    };
    let dwg = fs::read(&dwg_path).expect("read DWG");
    let dxf = acad_dxf::parse(&fs::read(&dxf_path).expect("read DXF")).expect("parse DXF");

    for (i, entity) in dxf.entities().enumerate() {
        let fields = coords(entity);
        if fields.is_empty() {
            continue;
        }
        println!("DXF entity {i}: {}", entity_name(entity));
        for (label, value) in fields {
            let hits = find_f64(&dwg, value, TOLERANCE);
            let where_ = hits
                .iter()
                .map(|h| format!("{:#06x}", h.offset))
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "    {label:<6} {value:<14} -> {}",
                if where_.is_empty() {
                    "—".into()
                } else {
                    where_
                }
            );
        }
        if i >= 4 {
            println!("... (stopping after 5; enough to see the stride)");
            break;
        }
    }
    ExitCode::SUCCESS
}

fn entity_name(e: &Entity) -> &'static str {
    match e {
        Entity::Line { .. } => "LINE",
        Entity::Circle { .. } => "CIRCLE",
        Entity::Arc { .. } => "ARC",
        Entity::Text { .. } => "TEXT",
        Entity::Insert { .. } => "INSERT",
        Entity::Point { .. } => "POINT",
        Entity::Trace { .. } => "TRACE",
        Entity::Solid { .. } => "SOLID",
    }
}
