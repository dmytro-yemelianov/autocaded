use acad_model::{Drawing, Entity, Item, Point};
use std::fmt::Write as _;

fn f(v: f64) -> String { format!("{v:.6}") }
fn pt(p: &Point) -> String { format!("{},{}", f(p.x), f(p.y)) }

fn entity(out: &mut String, e: &Entity) {
    match e {
        Entity::Line { start, end } =>
            { let _ = write!(out, "LINE,1\r\n{},{}\r\n", pt(start), pt(end)); }
        Entity::Circle { center, radius } =>
            { let _ = write!(out, "CIRCLE,1\r\n{},{}\r\n", pt(center), f(*radius)); }
        Entity::Arc { center, radius, start_deg, end_deg } =>
            { let _ = write!(out, "ARC,1\r\n{},{},{},{}\r\n",
                pt(center), f(*radius), f(*start_deg), f(*end_deg)); }
        Entity::Text { origin, height, rotation_deg, value } =>
            { let _ = write!(out, "TEXT,1\r\n{},{},{}\r\n{}\r\n",
                pt(origin), f(*height), f(*rotation_deg), value); }
        Entity::Insert { origin, x_scale, y_scale, rotation_deg, name } =>
            { let _ = write!(out, "INSERT,1\r\n{},{},{},{}\r\n{}\r\n",
                pt(origin), f(*x_scale), f(*y_scale), f(*rotation_deg), name); }
    }
}

pub fn write(d: &Drawing) -> Vec<u8> {
    let mut s = String::new();
    let h = &d.header;
    let _ = write!(s, "EXTENTS,1\r\n{},{},{},{}\r\n",
        f(h.extents.xmin), f(h.extents.xmax), f(h.extents.ymin), f(h.extents.ymax));
    let _ = write!(s, "LIMITS,1\r\n{},{},{},{}\r\n",
        f(h.limits.xmin), f(h.limits.xmax), f(h.limits.ymin), f(h.limits.ymax));
    let _ = write!(s, "BASE,1\r\n{}\r\n", pt(&h.base));
    let _ = write!(s, "DWGVIEW,1\r\n{},{}\r\n", pt(&h.view.center), f(h.view.height));
    let _ = write!(s, "MODERES,1\r\n{},{}\r\n", h.snap.on as u8, f(h.snap.spacing));
    let _ = write!(s, "MODEGRID,1\r\n{},{}\r\n", h.grid.on as u8, f(h.grid.spacing));
    let _ = write!(s, "MODEORTHO,1\r\n{}\r\n", h.ortho as u8);
    let _ = write!(s, "MODEFILL,1\r\n{}\r\n", h.fill as u8);
    let _ = write!(s, "TXTSIZE,1\r\n{}\r\n", f(h.text_size));
    let _ = write!(s, "TRACEWID,1\r\n{}\r\n", f(h.trace_width));
    let _ = write!(s, "LAYER,1\r\n{}\r\n", h.current_layer);
    s.push_str("LAYERC,1\r\n");
    for row in h.layer_colors.chunks(16) {
        let cells: Vec<String> = row.iter().map(|c| c.to_string()).collect();
        let _ = write!(s, "{}\r\n", cells.join(","));
    }
    // Document order, not bucketed by kind: SUBDIV.DXF interleaves the two.
    for item in &d.items {
        match item {
            Item::Block(b) => {
                let _ = write!(s, "BLOCK,1\r\n{}\r\n{}\r\n", pt(&b.base), b.name);
                for e in &b.entities { entity(&mut s, e); }
                s.push_str("ENDBLK,1\r\n");
            }
            Item::Entity(e) => entity(&mut s, e),
        }
    }
    let mut bytes = s.into_bytes();
    bytes.push(0x1a);
    bytes
}
