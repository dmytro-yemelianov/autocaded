//! Render DWG/DXF to PNG, with optional font search directories after output.
//! Usage: cargo run -p acad-render --example render-png -- <drawing> <out.png> [fonts ...]
use acad_render::{flatten_with_libraries, rasterize, Libraries, Viewport};

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .unwrap_or_else(|| "corpus/Samples/SUBDIV.DXF".into());
    let output = args.next().unwrap_or_else(|| "subdiv.png".into());
    let bytes = std::fs::read(&input).unwrap_or_else(|e| panic!("{input}: {e}"));
    let d = if acad_dwg::header::Version::detect(&bytes).is_ok() {
        acad_dwg::parse(&bytes).unwrap_or_else(|e| panic!("{input}: {e}"))
    } else {
        acad_dxf::parse(&bytes).unwrap_or_else(|e| panic!("{input}: {e}"))
    };
    let directories = args.map(std::path::PathBuf::from).collect::<Vec<_>>();
    let (libraries, diagnostics) =
        Libraries::for_drawing(std::path::Path::new(&input), &directories);
    for diagnostic in diagnostics {
        eprintln!("library: {diagnostic}");
    }
    let (w, h) = (1200u32, 900u32);
    let vp = Viewport::fit(&d.header.limits, w, h);
    let rendered = flatten_with_libraries(&d, &vp, &libraries);
    for diagnostic in &rendered.diagnostics {
        eprintln!("render: {diagnostic}");
    }
    let pm = rasterize(&rendered.primitives, w, h);
    pm.save_png(&output).unwrap();
    println!(
        "{input}: {} items, {} polylines -> {output}",
        d.items.len(),
        rendered.primitives.len()
    );
}
