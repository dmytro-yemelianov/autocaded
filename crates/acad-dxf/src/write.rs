use acad_model::{Drawing, Entity, Item, Point};
use std::fmt::Write as _;

fn f(v: f64) -> String {
    format!("{v:.6}")
}
fn pt(p: &Point) -> String {
    format!("{},{}", f(p.x), f(p.y))
}

fn entity(out: &mut String, e: &Entity) {
    match e {
        Entity::Line { start, end } => {
            let _ = write!(out, "LINE,1\r\n{},{}\r\n", pt(start), pt(end));
        }
        Entity::Circle { center, radius } => {
            let _ = write!(out, "CIRCLE,1\r\n{},{}\r\n", pt(center), f(*radius));
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            let _ = write!(
                out,
                "ARC,1\r\n{},{},{},{}\r\n",
                pt(center),
                f(*radius),
                f(*start_deg),
                f(*end_deg)
            );
        }
        Entity::Text {
            origin,
            height,
            rotation_deg,
            value,
        } => {
            let _ = write!(
                out,
                "TEXT,1\r\n{},{},{}\r\n{}\r\n",
                pt(origin),
                f(*height),
                f(*rotation_deg),
                value
            );
        }
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            name,
        } => {
            let _ = write!(
                out,
                "INSERT,1\r\n{},{},{},{}\r\n{}\r\n",
                pt(origin),
                f(*x_scale),
                f(*y_scale),
                f(*rotation_deg),
                name
            );
        }
        // POINT, TRACE and SOLID have no DXF oracle anywhere in this corpus
        // — `SUBDIV.DXF`, the only parallel-corpus file, contains none of
        // them (spec's corpus census) — so this shape is not verified
        // against any real 1983 output, unlike every other arm here. It
        // exists so this match stays exhaustive now that `acad_dwg::parse`
        // can produce these variants (Task 8); the keyword names themselves
        // are confirmed string constants in `ACAD.EXE` (spec §4.2's entity
        // table).
        Entity::Point { origin } => {
            let _ = write!(out, "POINT,1\r\n{}\r\n", pt(origin));
        }
        Entity::Trace { p1, p2, p3, p4 } => {
            let _ = write!(
                out,
                "TRACE,1\r\n{},{},{},{}\r\n",
                pt(p1),
                pt(p2),
                pt(p3),
                pt(p4)
            );
        }
        Entity::Solid { p1, p2, p3, p4 } => {
            let _ = write!(
                out,
                "SOLID,1\r\n{},{},{},{}\r\n",
                pt(p1),
                pt(p2),
                pt(p3),
                pt(p4)
            );
        }
    }
}

pub fn write(d: &Drawing) -> Vec<u8> {
    let mut s = String::new();
    let h = &d.header;
    let _ = write!(
        s,
        "EXTENTS,1\r\n{},{},{},{}\r\n",
        f(h.extents.xmin),
        f(h.extents.xmax),
        f(h.extents.ymin),
        f(h.extents.ymax)
    );
    let _ = write!(
        s,
        "LIMITS,1\r\n{},{},{},{}\r\n",
        f(h.limits.xmin),
        f(h.limits.xmax),
        f(h.limits.ymin),
        f(h.limits.ymax)
    );
    let _ = write!(s, "BASE,1\r\n{}\r\n", pt(&h.base));
    let _ = write!(
        s,
        "DWGVIEW,1\r\n{},{}\r\n",
        pt(&h.view.center),
        f(h.view.height)
    );
    let _ = write!(
        s,
        "MODERES,1\r\n{},{}\r\n",
        h.snap.on as u8,
        f(h.snap.spacing)
    );
    let _ = write!(
        s,
        "MODEGRID,1\r\n{},{}\r\n",
        h.grid.on as u8,
        f(h.grid.spacing)
    );
    let _ = write!(s, "MODEORTHO,1\r\n{}\r\n", h.ortho as u8);
    let _ = write!(s, "MODEFILL,1\r\n{}\r\n", h.fill as u8);
    let _ = write!(s, "TXTSIZE,1\r\n{}\r\n", f(h.text_size));
    let _ = write!(s, "TRACEWID,1\r\n{}\r\n", f(h.trace_width));
    let _ = write!(s, "LAYER,1\r\n{}\r\n", h.current_layer);
    // Marshal the model's sparse layer map back into the record's fixed
    // 8x16 grid, writing the unused sentinel for every absent slot.
    s.push_str("LAYERC,1\r\n");
    let slots: Vec<u8> = (0..crate::parse::LAYER_SLOTS)
        .map(|i| {
            *h.layers
                .get(&(i as u8))
                .unwrap_or(&crate::parse::LAYER_UNUSED)
        })
        .collect();
    for row in slots.chunks(16) {
        let cells: Vec<String> = row.iter().map(|c| c.to_string()).collect();
        let _ = write!(s, "{}\r\n", cells.join(","));
    }
    // Document order, not bucketed by kind: SUBDIV.DXF interleaves the two.
    for item in &d.items {
        match item {
            Item::Block(b) => {
                let _ = write!(s, "BLOCK,1\r\n{}\r\n{}\r\n", pt(&b.base), b.name);
                for e in &b.entities {
                    entity(&mut s, e);
                }
                s.push_str("ENDBLK,1\r\n");
            }
            Item::Entity(e) => entity(&mut s, e),
        }
    }
    let mut bytes = s.into_bytes();
    bytes.push(0x1a);
    bytes
}
