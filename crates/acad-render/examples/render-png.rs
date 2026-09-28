//! Render a DXF to a PNG so the output can be inspected without a window.
//! Usage: cargo run -p acad-render --example render-png -- <in.dxf> <out.png>
use acad_render::{flatten, rasterize, Viewport};

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().unwrap_or_else(|| "corpus/Samples/SUBDIV.DXF".into());
    let output = args.next().unwrap_or_else(|| "subdiv.png".into());
    let bytes = std::fs::read(&input).unwrap_or_else(|e| panic!("{input}: {e}"));
    let d = acad_dxf::parse(&bytes).unwrap_or_else(|e| panic!("{input}: {e}"));
    let (w, h) = (1200u32, 900u32);
    let vp = Viewport::fit(&d.header.limits, w, h);
    let prims = flatten(&d, &vp);
    let pm = rasterize(&prims, w, h);
    pm.save_png(&output).unwrap();
    println!("{input}: {} items, {} polylines -> {output}", d.items.len(), prims.len());
}
