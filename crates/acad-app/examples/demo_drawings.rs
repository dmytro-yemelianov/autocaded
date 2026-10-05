//! Build AutoCADED's own demo drawings (demo/*.DWG) by driving the editor,
//! so the public browser build ships no Autodesk sample files.
//!
//!     python3 tools/demo/gen_font.py
//!     cargo run -p acad-app --example demo_drawings
//!
//! Text uses demo/AUTOCADED.SHP registered as TXT, which also replaces any
//! local corpus font, so the output is the same on every machine.
use acad_app::Session;
use std::path::Path;

/// Draws one demo drawing into a fresh session.
type Builder = fn(&mut Session);

fn run(session: &mut Session, inputs: &[&str]) {
    for input in inputs {
        session
            .command(input)
            .unwrap_or_else(|e| panic!("{input:?}: {e}"));
    }
}

fn layer(session: &mut Session, layer: u8, color: u8) {
    run(
        session,
        &[&format!("LAYER {layer}"), &format!("LAYER COLOR {color}")],
    );
}

fn text(session: &mut Session, at: &str, height: &str, value: &str) {
    run(session, &["TEXT", at, height, "0", value]);
}

fn welcome(s: &mut Session) {
    layer(s, 1, 7);
    run(s, &["LINE", "0,0", "48,0", "48,30", "0,30", "C"]);
    run(s, &["LINE", "1,1", "47,1", "47,29", "1,29", "C"]);
    layer(s, 2, 11);
    text(s, "4,19", "3.5", "AutoCADED");
    layer(s, 3, 14);
    text(s, "4,14", "1", "AutoCAD 1.4 (1983) rebuilt in Rust");
    layer(s, 4, 10);
    text(
        s,
        "4,10",
        "0.7",
        "Type a command: LINE, CIRCLE, ARC, TEXT, HATCH, DIM",
    );
    text(
        s,
        "4,8",
        "0.7",
        "Pick from the screen menu on the right, or HELP",
    );
    text(
        s,
        "4,6",
        "0.7",
        "Open... loads DWG/DXF; Save as... writes DWG, DXF, SVG, PNG",
    );
    layer(s, 5, 12);
    run(s, &["CIRCLE", "41,20", "4"]);
    run(s, &["CIRCLE", "41,20", "2.5"]);
    layer(s, 6, 9);
    run(s, &["LINE", "36,20", "46,20", ""]);
    run(s, &["LINE", "41,15", "41,25", ""]);
}

fn bracket(s: &mut Session) {
    layer(s, 1, 7);
    run(
        s,
        &[
            "LINE", "0,0", "30,0", "30,4", "12,4", "12,18", "8,22", "0,22", "C",
        ],
    );
    layer(s, 2, 10);
    run(s, &["CIRCLE", "4,18", "1.5"]);
    run(s, &["CIRCLE", "6,2", "1"]);
    run(s, &["CIRCLE", "24,2", "1"]);
    layer(s, 3, 12);
    run(s, &["LINE", "4,15", "4,21", ""]);
    run(s, &["LINE", "1,18", "7,18", ""]);
    // Section A-A: its own closed boundary, hatched by window.
    layer(s, 4, 9);
    run(s, &["LINE", "36,0", "40,0", "40,22", "36,22", "C"]);
    run(
        s,
        &[
            "HATCH",
            "U",
            "45",
            "0.75",
            "N",
            "W",
            "35.5,-0.5",
            "40.5,22.5",
        ],
    );
    layer(s, 5, 11);
    run(s, &["DIM", "0,0", "0,-4", "30,0", ""]);
    run(s, &["DIM", "30,0", "34,0", "30,4", ""]);
    run(s, &["DIM", "0,0", "-4,0", "0,22", ""]);
    layer(s, 6, 14);
    text(s, "16,14", "1", "BRACKET  steel  1:1");
    text(s, "16,11.5", "0.7", "holes 3 x through");
    text(s, "34.5,23.5", "0.5", "SECTION A-A");
}

fn floor_plan(s: &mut Session) {
    layer(s, 1, 7);
    run(
        s,
        &["TRACE", "0.4", "0,0", "40,0", "40,26", "0,26", "0,0.2", ""],
    );
    run(s, &["TRACE", "0.25", "18,0", "18,11", ""]);
    run(s, &["TRACE", "0.25", "18,15", "18,26", ""]);
    run(s, &["TRACE", "0.25", "18,13", "40,13", ""]);
    layer(s, 2, 14);
    run(s, &["ARC", "18,11", "20.8,12.2", "22,15"]);
    run(s, &["LINE", "18,11", "22,11", ""]);
    layer(s, 3, 10);
    run(s, &["SOLID", "3,3", "13,3", "3,8", "13,8", ""]);
    run(s, &["CIRCLE", "30,4.5", "2.5"]);
    run(s, &["LINE", "24,18", "36,18", "36,23", "24,23", "C"]);
    layer(s, 4, 11);
    text(s, "4,20", "1.5", "LIVING");
    text(s, "26,8", "1.5", "KITCHEN");
    text(s, "26,15", "1.5", "STUDIO");
    text(
        s,
        "0,-3",
        "0.7",
        "Plan: walls TRACE, door ARC, furniture SOLID and LINE",
    );
}

fn palette(s: &mut Session) {
    for color in 1u8..=32 {
        let (row, column) = ((color - 1) / 8, (color - 1) % 8);
        let (x, y) = (f64::from(column) * 5.0, f64::from(3 - row) * 5.0);
        layer(s, color, color);
        let p = |dx: f64, dy: f64| format!("{},{}", x + dx, y + dy);
        run(
            s,
            &[
                "SOLID",
                &p(0.0, 0.0),
                &p(4.0, 0.0),
                &p(0.0, 4.0),
                &p(4.0, 4.0),
                "",
            ],
        );
        layer(s, 100, 7);
        text(s, &p(0.0, -0.9), "0.7", &color.to_string());
    }
    layer(s, 101, 7);
    text(s, "0,21", "1", "Colours 1-32 (PC 16-colour palette)");
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo");
    let font = std::fs::read(root.join("AUTOCADED.SHP")).expect("run tools/demo/gen_font.py first");
    let drawings: [(&str, Builder); 4] = [
        ("WELCOME", welcome),
        ("BRACKET", bracket),
        ("FLOORPLAN", floor_plan),
        ("PALETTE", palette),
    ];
    for (name, build) in drawings {
        let mut session = Session::default();
        session.add_shape_library("TXT", &font).expect("font");
        build(&mut session);
        run(&mut session, &["ZOOM", "E", "ZOOM", "0.85"]);
        let bytes = acad_dwg::write_version(session.drawing(), acad_dwg::header::Version::Ac140)
            .expect("DWG");
        let path = root.join(format!("{name}.DWG"));
        std::fs::write(&path, bytes).unwrap();
        println!(
            "{}: {} entities",
            path.display(),
            session.drawing().entities().count()
        );
    }
}
