//! Drawing extents and mode report described by retained ACAD.HLP.
//! Layout is a native Rust policy; no original STATUS output is retained.
use crate::parse::format_measurement;
use acad_model::{Drawing, Mode, Point, Units};

pub(crate) fn status_report(drawing: &Drawing) -> String {
    let h = &drawing.header;
    let value = |v| format_measurement(v, h.units);
    let point = |p: Point| format!("{},{}", value(p.x), value(p.y));
    let toggle = |on| if on { "ON" } else { "OFF" };
    fn mode(mode: Mode, units: Units) -> String {
        format!(
            "{}  spacing={}",
            if mode.on { "ON" } else { "OFF" },
            format_measurement(mode.spacing, units)
        )
    }
    let bounds = |e: acad_model::Extents| {
        format!(
            "{},{} to {},{}",
            value(e.xmin),
            value(e.ymin),
            value(e.xmax),
            value(e.ymax)
        )
    };
    let mut lines = vec![
        "Drawing status".to_owned(),
        format!("EXTENTS: {}", bounds(h.extents)),
        format!("LIMITS: {}", bounds(h.limits)),
        format!("BASE: {}", point(h.base)),
        format!(
            "VIEW: center={}  height={}",
            point(h.view.center),
            value(h.view.height)
        ),
        format!("LAYER: {}  defined={}", h.current_layer, h.layers.len()),
        format!("LAYERS OFF: {:?}", h.off_layers),
        format!("SNAP: {}", mode(h.snap, h.units)),
        format!("GRID: {}", mode(h.grid, h.units)),
        format!("AXIS: {}", mode(h.axis, h.units)),
        format!("ORTHO: {}", toggle(h.ortho)),
        format!("FILL: {}", toggle(h.fill)),
        format!(
            "UNITS: {:?}  precision={}",
            h.units.format, h.units.precision
        ),
        format!("TXTSIZE: {}", value(h.text_size)),
        format!("TRACEWID: {}", value(h.trace_width)),
    ];
    if let Some(arrow) = h.dim_arrow {
        lines.push(format!("DIMARROW: {}", value(arrow)));
    }
    lines.join("\n")
}
