//! Versioned, offline artwork recipes compiled into the existing CAD model.
use acad_model::{Drawing, Entity, Item, Point};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const GENERATOR_VERSION: &str = "art-v1";
pub const MAX_RECORDS: usize = 5_000;
pub const MAX_PRIMITIVES: usize = 25_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub schema_version: u32,
    pub id: String,
    pub bounds: [f64; 4],
    pub layers: Vec<Layer>,
    pub primitives: Vec<Primitive>,
    #[serde(default)]
    pub font: Option<Font>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Font {
    pub id: String,
    pub sha256: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub number: u8,
    pub color: u8,
    pub role: String,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Primitive {
    Text {
        layer: u8,
        origin: [f64; 2],
        height: f64,
        value: String,
        #[serde(default)]
        rotation_deg: f64,
    },
    Line {
        layer: u8,
        start: [f64; 2],
        end: [f64; 2],
    },
    Circle {
        layer: u8,
        center: [f64; 2],
        radius: f64,
    },
    Solid {
        layer: u8,
        points: [[f64; 2]; 4],
    },
    Rectangle {
        layer: u8,
        min: [f64; 2],
        max: [f64; 2],
        filled: bool,
    },
    Grid {
        layer: u8,
        origin: [f64; 2],
        columns: u16,
        rows: u16,
        spacing: [f64; 2],
        size: [f64; 2],
        filled: bool,
    },
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema_version: u32,
    pub entries: Vec<Entry>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub category: Category,
    pub title_key: String,
    pub description_key: String,
    pub recipe: String,
    pub source: Source,
    pub outputs: Outputs,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Building,
    ColourStudy,
    Painting,
    Meme,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub kind: SourceKind,
    pub creator: String,
    pub license_url: String,
    pub license_version: String,
    pub attribution: String,
    pub adaptation: String,
    pub snapshot: String,
    pub sha256: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Original,
    PublicDomain,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Outputs {
    pub dwg: String,
    pub dxf: String,
    pub svg: String,
    pub png: String,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
// Match the historical DXF writer's six-decimal coordinate precision.
fn coordinate(x: f64) -> f64 {
    format!("{x:.6}").parse().unwrap_or(x)
}
fn point(p: [f64; 2]) -> Point {
    Point {
        x: coordinate(p[0]),
        y: coordinate(p[1]),
    }
}
fn positive(x: f64) -> bool {
    x.is_finite() && (0.000001..=1_000_000.0).contains(&x)
}

impl Recipe {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }
    pub fn compile(&self) -> Result<Drawing, String> {
        self.compile_with_libraries(&acad_render::Libraries::default())
    }
    /// Compile captions with the same drawing SHP library used by the renderer.
    pub fn compile_with_libraries(
        &self,
        libraries: &acad_render::Libraries,
    ) -> Result<Drawing, String> {
        if self.schema_version != 1 || !valid_id(&self.id) {
            return Err("unsupported recipe version or invalid id".into());
        }
        if let Some(font) = &self.font {
            if font.id != "autocaded"
                || font.sha256.len() != 64
                || !font
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err("unsupported font id or invalid SHA-256".into());
            }
        }
        let [xmin, ymin, xmax, ymax] = self.bounds;
        if !self
            .bounds
            .iter()
            .all(|x| x.is_finite() && x.abs() <= 1_000_000.0)
            || !positive(xmax - xmin)
            || !positive(ymax - ymin)
        {
            return Err("bounds must be finite, distinct and within +/-1000000".into());
        }
        if self.layers.is_empty() || self.layers.len() > 16 {
            return Err("recipes require 1..16 semantic layers".into());
        }
        let mut layers = BTreeSet::new();
        let mut d = crate::Session::default().drawing().clone();
        d.items.clear();
        d.header.layers.clear();
        d.header.dwg_header_passthrough = None;
        d.header.fill = true;
        d.header.grid.on = false;
        d.header.snap.on = false;
        d.header.axis.on = false;
        let bounds = acad_model::Extents {
            xmin: coordinate(xmin),
            ymin: coordinate(ymin),
            xmax: coordinate(xmax),
            ymax: coordinate(ymax),
        };
        if bounds.xmin >= bounds.xmax || bounds.ymin >= bounds.ymax {
            return Err("bounds collapse at historical coordinate precision".into());
        }
        d.header.extents = bounds;
        d.header.limits = bounds;
        d.header.view.center = point([
            (bounds.xmin + bounds.xmax) / 2.0,
            (bounds.ymin + bounds.ymax) / 2.0,
        ]);
        d.header.view.height =
            ((bounds.ymax - bounds.ymin).max((bounds.xmax - bounds.xmin) * 0.75)) * 1.08;
        for l in &self.layers {
            if l.number > 127
                || l.color > 254
                || l.number == 0
                || l.color == 0
                || l.role.trim().is_empty()
                || !layers.insert(l.number)
            {
                return Err(
                    "layers require unique numbers 1..127, ACI colours 1..254 and roles".into(),
                );
            }
            d.header.layers.insert(l.number, l.color);
        }
        let inside = |p: [f64; 2]| {
            p[0].is_finite()
                && p[1].is_finite()
                && p[0] >= xmin
                && p[0] <= xmax
                && p[1] >= ymin
                && p[1] <= ymax
        };
        for p in &self.primitives {
            Self::emit(p, &inside, &layers, &mut d, libraries, self.font.is_some())?;
        }
        if d.items.is_empty() {
            return Err("recipe must contain geometry".into());
        }
        Ok(d)
    }
    fn emit(
        p: &Primitive,
        inside: &impl Fn([f64; 2]) -> bool,
        layers: &BTreeSet<u8>,
        d: &mut Drawing,
        libraries: &acad_render::Libraries,
        has_font: bool,
    ) -> Result<(), String> {
        let mut add = |l: u8, e: Entity| {
            let ext = if let Entity::Text {
                origin,
                height,
                rotation_deg,
                value,
            } = &e
            {
                let library = libraries
                    .get("TXT")
                    .ok_or("TEXT requires validated TXT SHP font")?;
                let glyph = library.text(value).map_err(|e| e.to_string())?;
                let scale = height / library.cap_height.ok_or("TEXT requires font cap height")?;
                let (sin, cos) = rotation_deg.to_radians().sin_cos();
                let points: Vec<Point> = glyph
                    .strokes
                    .iter()
                    .flatten()
                    .map(|p| Point {
                        x: origin.x + scale * (p.x * cos - p.y * sin),
                        y: origin.y + scale * (p.x * sin + p.y * cos),
                    })
                    .collect();
                acad_model::Extents::from_points(&points)
                    .ok_or("TEXT must contain visible glyphs")?
            } else {
                e.bounding_extents()
                    .ok_or("missing emitted geometry bounds")?
            };
            let bounds = d.header.extents;
            if !inside([ext.xmin, ext.ymin])
                || !inside([ext.xmax, ext.ymax])
                || ext.xmin < bounds.xmin
                || ext.xmax > bounds.xmax
                || ext.ymin < bounds.ymin
                || ext.ymax > bounds.ymax
            {
                return Err("quantized geometry exceeds authored bounds".into());
            }
            if !layers.contains(&l) {
                return Err("undefined layer".into());
            }
            if d.items.len() >= MAX_RECORDS {
                return Err("stored record budget exceeded".into());
            }
            d.items.push(Item::Entity(Entity::OnLayer {
                layer: l,
                entity: Box::new(e),
            }));
            Ok(())
        };
        match p {
            Primitive::Text {
                layer,
                origin,
                height,
                value,
                rotation_deg,
            } => {
                if !has_font
                    || !inside(*origin)
                    || !positive(*height)
                    || !rotation_deg.is_finite()
                    || rotation_deg.abs() > 360.0
                    || value.trim().is_empty()
                    || value.len() > 256
                    || !value.bytes().all(|c| (32..=126).contains(&c))
                {
                    return Err("TEXT requires font metadata, printable ASCII, bounded origin and positive height".into());
                }
                add(
                    *layer,
                    Entity::Text {
                        origin: point(*origin),
                        height: coordinate(*height),
                        rotation_deg: coordinate(*rotation_deg),
                        value: value.clone(),
                    },
                )
            }
            Primitive::Line { layer, start, end } => {
                if !inside(*start) || !inside(*end) || point(*start) == point(*end) {
                    return Err("line must be finite, bounded and nonzero".into());
                }
                add(
                    *layer,
                    Entity::Line {
                        start: point(*start),
                        end: point(*end),
                    },
                )
            }
            Primitive::Circle {
                layer,
                center,
                radius,
            } => {
                if !positive(*radius)
                    || !inside([center[0] - radius, center[1] - radius])
                    || !inside([center[0] + radius, center[1] + radius])
                {
                    return Err("circle must be positive and bounded".into());
                }
                add(
                    *layer,
                    Entity::Circle {
                        center: point(*center),
                        radius: coordinate(*radius),
                    },
                )
            }
            Primitive::Solid { layer, points } => {
                let q = [points[0], points[1], points[3], points[2]]
                    .map(|p| [coordinate(p[0]), coordinate(p[1])]);
                let crosses: Vec<f64> = (0..4)
                    .map(|i| {
                        let a = q[i];
                        let b = q[(i + 1) % 4];
                        let c = q[(i + 2) % 4];
                        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
                    })
                    .collect();
                if !points.iter().all(|p| inside(*p))
                    || !((crosses.iter().all(|c| *c >= 0.0) && crosses.iter().any(|c| *c > 0.0))
                        || (crosses.iter().all(|c| *c <= 0.0) && crosses.iter().any(|c| *c < 0.0)))
                {
                    return Err(
                        "solid must be bounded, convex and nondegenerate in CAD corner order"
                            .into(),
                    );
                }
                add(
                    *layer,
                    Entity::Solid {
                        p1: point(points[0]),
                        p2: point(points[1]),
                        p3: point(points[2]),
                        p4: point(points[3]),
                    },
                )
            }
            Primitive::Rectangle {
                layer,
                min,
                max,
                filled,
            } => {
                if !inside(*min)
                    || !inside(*max)
                    || !positive(max[0] - min[0])
                    || !positive(max[1] - min[1])
                    || point(*min).x >= point(*max).x
                    || point(*min).y >= point(*max).y
                {
                    return Err("invalid rectangle".into());
                }
                if *filled {
                    add(
                        *layer,
                        Entity::Solid {
                            p1: point(*min),
                            p2: point([max[0], min[1]]),
                            p3: point([min[0], max[1]]),
                            p4: point(*max),
                        },
                    )
                } else {
                    for (a, b) in [
                        (*min, [max[0], min[1]]),
                        ([max[0], min[1]], *max),
                        (*max, [min[0], max[1]]),
                        ([min[0], max[1]], *min),
                    ] {
                        add(
                            *layer,
                            Entity::Line {
                                start: point(a),
                                end: point(b),
                            },
                        )?;
                    }
                    Ok(())
                }
            }
            Primitive::Grid {
                layer,
                origin,
                columns,
                rows,
                spacing,
                size,
                filled,
            } => {
                let count =
                    usize::from(*columns) * usize::from(*rows) * if *filled { 1 } else { 4 };
                if *columns == 0
                    || *rows == 0
                    || count > MAX_RECORDS
                    || !spacing
                        .iter()
                        .zip([*columns, *rows])
                        .all(|(x, n)| positive(*x) || (n == 1 && *x == 0.0))
                    || !size.iter().all(|x| positive(*x))
                {
                    return Err("invalid or over-budget grid".into());
                }
                for r in 0..*rows {
                    for c in 0..*columns {
                        let min = [
                            origin[0] + f64::from(c) * spacing[0],
                            origin[1] + f64::from(r) * spacing[1],
                        ];
                        Self::emit(
                            &Primitive::Rectangle {
                                layer: *layer,
                                min,
                                max: [min[0] + size[0], min[1] + size[1]],
                                filled: *filled,
                            },
                            inside,
                            layers,
                            d,
                            libraries,
                            has_font,
                        )?;
                    }
                }
                Ok(())
            }
        }
    }
}
