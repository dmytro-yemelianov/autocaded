use acad_model::{Drawing, Entity, Item, Point};
use std::fmt::Write as _;

fn f(v: f64) -> String {
    format!("{v:.6}")
}
fn pt(p: &Point) -> String {
    format!("{},{}", f(p.x), f(p.y))
}

// Preflight bounds recursion to 64 groups/two record wrappers. Erased members
// are skipped, but a live group keeps its REPEAT/ENDREP records even with no
// live member left, as AutoCAD 1.4's task 5 does; its task 6 and the native
// reader read that empty pair back (docs/native-group-persistence.md, R6).
fn entity(out: &mut String, e: &Entity) {
    let (layer, e) = match e {
        Entity::OnLayer { layer, entity } => (*layer, entity.as_ref()),
        _ => (1, e),
    };
    match e {
        Entity::Erased(_) => {}
        Entity::Repeat(repeat) => {
            let _ = write!(out, "REPEAT,{}\r\n", repeat.start_layer);
            for inner in &repeat.entities {
                entity(out, inner);
            }
            let _ = write!(
                out,
                "ENDREP,{}\r\n{},{},{},{}\r\n",
                repeat.end_layer,
                repeat.columns,
                repeat.rows,
                f(repeat.column_spacing),
                f(repeat.row_spacing)
            );
        }
        Entity::OnLayer { .. } => unreachable!("unwrapped above"),
        Entity::Load { name } => {
            let _ = write!(out, "LOAD,{layer}\r\n{name}\r\n");
        }
        Entity::Shape {
            origin,
            height,
            rotation_deg,
            number,
        } => {
            let _ = write!(
                out,
                "SHAPE,{layer}\r\n{},{},{},{}\r\n",
                pt(origin),
                f(*height),
                f(*rotation_deg),
                number
            );
        }
        Entity::Line { start, end } => {
            let _ = write!(out, "LINE,{layer}\r\n{},{}\r\n", pt(start), pt(end));
        }
        Entity::Circle { center, radius } => {
            let _ = write!(out, "CIRCLE,{layer}\r\n{},{}\r\n", pt(center), f(*radius));
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            let _ = write!(
                out,
                "ARC,{layer}\r\n{},{},{},{}\r\n",
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
                "TEXT,{layer}\r\n{},{},{}\r\n{}\r\n",
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
                "INSERT,{layer}\r\n{},{},{},{}\r\n{}\r\n",
                pt(origin),
                f(*x_scale),
                f(*y_scale),
                f(*rotation_deg),
                name
            );
        }
        // Original AutoCAD exports from SELEXOL, BLIVET and FLOW verify
        // these entity layouts; command-generated TRACE/SOLID exports also
        // verify their two coordinate rows byte for byte.
        Entity::Point { origin } => {
            let _ = write!(out, "POINT,{layer}\r\n{}\r\n", pt(origin));
        }
        Entity::Trace { p1, p2, p3, p4 } => {
            let _ = write!(
                out,
                "TRACE,{layer}\r\n{},{}\r\n{},{}\r\n",
                pt(p1),
                pt(p2),
                pt(p3),
                pt(p4)
            );
        }
        Entity::Solid { p1, p2, p3, p4 } => {
            let _ = write!(
                out,
                "SOLID,{layer}\r\n{},{}\r\n{},{}\r\n",
                pt(p1),
                pt(p2),
                pt(p3),
                pt(p4)
            );
        }
    }
}

/// Legacy convenience writer. Panics when drawing data cannot be encoded.
/// Production save/export callers should use `try_write` for an explicit error.
/// Like `try_write`, this omits erased owners/members; live groups keep their markers.
pub fn write(d: &Drawing) -> Vec<u8> {
    try_write(d).expect("drawing cannot be encoded as historical DXF")
}

/// Checked live-only exchange export. Erased ordinary/group owners and their
/// marker metadata/history are omitted as complete subtrees. Erased members are
/// skipped; a live group keeps its REPEAT/ENDREP records even when no live
/// member remains, as AutoCAD 1.4 writes it (an empty pair that reads back).
/// The session is unchanged.
/// Use DWG for retained erased native owner content. OOPS/UNDO history is not persisted.
pub fn try_write(d: &Drawing) -> Result<Vec<u8>, crate::DxfError> {
    acad_model::group_codec::validate_live_export_records(d).map_err(|error| {
        crate::DxfError::UnsupportedGroup {
            reason: error.message(),
        }
    })?;
    let h = &d.header;
    if h.fillet_radius != 0.0 {
        return Err(crate::DxfError::UnsupportedFilletRadius);
    }
    for (&layer, &color) in &h.layers {
        if layer >= 128 || color == 255 {
            return Err(crate::DxfError::InvalidLayerTable { layer, color });
        }
    }
    for &layer in &h.off_layers {
        let color = h.layers.get(&layer).copied();
        if layer == 0 || layer >= 128 || !color.is_some_and(|n| (1..=127).contains(&n)) {
            return Err(crate::DxfError::InvalidLayerVisibility { layer, color });
        }
    }
    let mut s = String::new();
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
    if let Some(value) = h.dim_arrow {
        let _ = write!(s, "DIMARROW,1\r\n{}\r\n", f(value));
    }
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
    let slots: Vec<i16> = (0..crate::parse::LAYER_SLOTS)
        .map(|i| {
            let layer = i as u8;
            let color = *h.layers.get(&layer).unwrap_or(&crate::parse::LAYER_UNUSED) as i16;
            if h.off_layers.contains(&layer) {
                -color
            } else {
                color
            }
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
            Item::Erased(_) => {}
            Item::Repeat(r) => {
                let _ = write!(s, "REPEAT,{}\r\n", r.start_layer);
                for e in &r.entities {
                    entity(&mut s, e);
                }
                let _ = write!(
                    s,
                    "ENDREP,{}\r\n{},{},{},{}\r\n",
                    r.end_layer,
                    r.columns,
                    r.rows,
                    f(r.column_spacing),
                    f(r.row_spacing)
                );
            }
        }
    }
    let mut bytes = s.into_bytes();
    bytes.push(0x1a);
    Ok(bytes)
}
