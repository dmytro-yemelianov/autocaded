//! Derive printable-ASCII metrics and verify composition against complete text.
use acad_model::Point;
use acad_render::shp::{Glyph, Library};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: txt-metrics FONT.SHP");
    let font = Library::parse(&std::fs::read(path).unwrap()).unwrap();
    let glyphs: Vec<_> = (32_u8..=126)
        .map(|code| {
            font.contains(u16::from(code))
                .then(|| font.text(&char::from(code).to_string()).unwrap())
        })
        .collect();
    // Check actual stroke sequences, not only their bounds. This catches scale,
    // stack and subshape state that would invalidate independent glyph metrics.
    for a in 32_u8..=126 {
        for b in 32_u8..=126 {
            let Some(first) = &glyphs[usize::from(a - 32)] else {
                continue;
            };
            let Some(second) = &glyphs[usize::from(b - 32)] else {
                continue;
            };
            let whole = font
                .text(&format!("{}{}", char::from(a), char::from(b)))
                .unwrap();
            let mut expected = first.strokes.clone();
            expected.extend(second.strokes.iter().map(|stroke| {
                stroke
                    .iter()
                    .map(|p| Point {
                        x: p.x + first.advance.x,
                        y: p.y + first.advance.y,
                    })
                    .collect::<Vec<_>>()
            }));
            assert_eq!(whole.strokes, expected, "pair {a},{b}");
            assert_eq!(
                whole.advance,
                Point {
                    x: first.advance.x + second.advance.x,
                    y: first.advance.y + second.advance.y
                },
                "advance {a},{b}"
            );
        }
    }
    let count = glyphs.iter().flatten().count();
    println!(
        "// Derived from TXT.SHP; all {} ordered defined ASCII pairs verified.",
        count * count
    );
    println!(
        "pub(crate) const TXT_CAP_HEIGHT: f64 = {:?};",
        font.cap_height.unwrap()
    );
    println!("// (advance, optional ink x bounds), ASCII 32..=126.");
    println!("pub(crate) const TXT_METRICS: [Option<(f64, Option<(f64,f64)>)>; 95] = [");
    for (code, glyph) in (32_u8..=126).zip(&glyphs) {
        let Some(glyph) = glyph else {
            println!("    None, // ASCII {code}: not defined by this font");
            continue;
        };
        println!(
            "    Some(({:?}, {:?})), // ASCII {}",
            glyph.advance.x,
            bounds(glyph),
            code
        );
    }
    println!("];");
}

fn bounds(glyph: &Glyph) -> Option<(f64, f64)> {
    let mut points = glyph.strokes.iter().flatten();
    let first = points.next()?;
    let mut lo = first.x;
    let mut hi = first.x;
    for p in points {
        lo = lo.min(p.x);
        hi = hi.max(p.x);
    }
    Some((lo, hi))
}
