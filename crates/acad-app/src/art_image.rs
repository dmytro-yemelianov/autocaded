//! Deterministic, bounded PNG-to-editable-region conversion (offline tooling).
use crate::art::{valid_id, Layer, Primitive, Recipe, MAX_RECORDS};
use acad_model::aci_rgb;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const VERSION: &str = "image-regions-v1";
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PIXELS: usize = 16 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub id: String,
    pub image: String,
    pub image_sha256: String,
    pub columns: u16,
    pub colors: u8,
    pub contours: bool,
    pub contour_color: u8,
    pub contrast_threshold: u16,
    pub matte: [u8; 3],
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub generator: &'static str,
    pub image_sha256: String,
    pub input_width: u32,
    pub input_height: u32,
    pub columns: usize,
    pub rows: usize,
    pub palette: Vec<u8>,
    pub regions: usize,
    pub contours: usize,
    pub render_work: usize,
}
fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (i32::from(a) - i32::from(b)).unsigned_abs().pow(2))
        .sum()
}
fn nearest(rgb: [u8; 3], palette: &[[u8; 3]]) -> usize {
    (0..palette.len())
        .min_by_key(|&i| (distance(rgb, palette[i]), i))
        .unwrap()
}
fn palette(samples: &[[u8; 3]], count: usize) -> Vec<u8> {
    // Integer Lloyd clustering: stable seed/tie/order/iteration count on all targets.
    let candidates: Vec<_> = samples
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mean = std::array::from_fn(|c| {
        (samples.iter().map(|p| u64::from(p[c])).sum::<u64>() / samples.len() as u64) as u8
    });
    let mut centers = vec![candidates[nearest(mean, &candidates)]];
    while centers.len() < count.min(candidates.len()) {
        let next = *candidates
            .iter()
            .max_by_key(|&&p| {
                (
                    distance(p, centers[nearest(p, &centers)]),
                    std::cmp::Reverse(p),
                )
            })
            .unwrap();
        if centers.contains(&next) {
            break;
        }
        centers.push(next);
    }
    for _ in 0..8 {
        let mut sums = vec![[0u64; 4]; centers.len()];
        for &p in samples {
            let s = &mut sums[nearest(p, &centers)];
            for c in 0..3 {
                s[c] += u64::from(p[c]);
            }
            s[3] += 1;
        }
        for (center, sum) in centers.iter_mut().zip(sums) {
            if sum[3] > 0 {
                *center = std::array::from_fn(|c| (sum[c] / sum[3]) as u8);
            }
        }
    }
    centers
        .into_iter()
        .map(|p| {
            (1..=254u8)
                .min_by_key(|&i| (distance(p, aci_rgb(i)), i))
                .unwrap()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn convert(config: &Config, bytes: &[u8]) -> Result<(Recipe, Report), String> {
    if config.schema_version != 1
        || !valid_id(&config.id)
        || !(4..=128).contains(&config.columns)
        || !(1..=15).contains(&config.colors)
        || !(1..=254).contains(&config.contour_color)
        || config.contrast_threshold > 442
    {
        return Err(
            "invalid image conversion settings (columns 4..128, colors 1..15, contrast 0..442)"
                .into(),
        );
    }
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("PNG exceeds 16 MiB input limit".into());
    }
    let hash = format!("{:x}", Sha256::digest(bytes));
    if hash != config.image_sha256 {
        return Err("conversion image SHA-256 mismatch".into());
    }
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder.set_limits(png::Limits {
        bytes: 64 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || width as usize * height as usize > MAX_PIXELS
        || reader.output_buffer_size() > 64 * 1024 * 1024
    {
        return Err("PNG exceeds 4096-axis / 16-megapixel limits".into());
    }
    let columns = usize::from(config.columns).min(width as usize);
    let rows = ((height as usize * columns + width as usize / 2) / width as usize)
        .max(1)
        .min(height as usize);
    if rows > 128 {
        return Err("aspect ratio produces more than 128 rows; reduce columns".into());
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    if reader.info().animation_control.is_some() {
        return Err("animated PNG is not a still source".into());
    }
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        _ => return Err("unsupported decoded PNG color type".into()),
    };
    let mut samples = Vec::with_capacity(columns * rows);
    for y in 0..rows {
        for x in 0..columns {
            let mut sum = [0u64; 3];
            let mut n = 0u64;
            for sy in y * height as usize / rows..(y + 1) * height as usize / rows {
                for sx in x * width as usize / columns..(x + 1) * width as usize / columns {
                    let p = &buffer[(sy * width as usize + sx) * channels..][..channels];
                    let rgb = if channels <= 2 {
                        [p[0]; 3]
                    } else {
                        [p[0], p[1], p[2]]
                    };
                    let a = if channels == 2 || channels == 4 {
                        u64::from(p[channels - 1])
                    } else {
                        255
                    };
                    for c in 0..3 {
                        sum[c] +=
                            (u64::from(rgb[c]) * a + u64::from(config.matte[c]) * (255 - a) + 127)
                                / 255;
                    }
                    n += 1;
                }
            }
            samples.push(std::array::from_fn(|c| (sum[c] / n) as u8));
        }
    }
    let indices = palette(&samples, usize::from(config.colors));
    let rgbs: Vec<_> = indices.iter().map(|&i| aci_rgb(i)).collect();
    let grid: Vec<_> = samples.iter().map(|&p| nearest(p, &rgbs)).collect();
    let mut layers: Vec<_> = indices
        .iter()
        .enumerate()
        .map(|(i, &color)| Layer {
            number: (i + 1) as u8,
            color,
            role: format!("region-aci-{color}"),
        })
        .collect();
    let mut primitives = Vec::new();
    let mut used = vec![false; grid.len()];
    let height_units: f64 = format!("{:.6}", 100.0 * f64::from(height) / f64::from(width))
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let point = |x: usize, y: usize| {
        [
            100.0 * x as f64 / columns as f64,
            height_units * (rows - y) as f64 / rows as f64,
        ]
    };
    for y in 0..rows {
        for x in 0..columns {
            let at = y * columns + x;
            if used[at] {
                continue;
            }
            let color = grid[at];
            let mut end = x + 1;
            while end < columns && !used[y * columns + end] && grid[y * columns + end] == color {
                end += 1;
            }
            let mut bottom = y + 1;
            while bottom < rows
                && (x..end)
                    .all(|xx| !used[bottom * columns + xx] && grid[bottom * columns + xx] == color)
            {
                bottom += 1;
            }
            for yy in y..bottom {
                for xx in x..end {
                    used[yy * columns + xx] = true;
                }
            }
            primitives.push(Primitive::Rectangle {
                layer: (color + 1) as u8,
                min: point(x, bottom),
                max: point(end, y),
                filled: true,
            });
        }
    }
    let regions = primitives.len();
    if config.contours {
        let layer = (indices.len() + 1) as u8;
        layers.push(Layer {
            number: layer,
            color: config.contour_color,
            role: "image-contours".into(),
        });
        let boundary = |a: usize, b: usize| {
            a != b && distance(rgbs[a], rgbs[b]) >= u32::from(config.contrast_threshold).pow(2)
        };
        // Merge collinear boundary segments; no diagonal guess or unbounded polygon triangulation.
        for y in 0..=rows {
            let mut x = 0;
            while x < columns {
                let edge = |x: usize| {
                    y == 0
                        || y == rows
                        || boundary(grid[(y - 1) * columns + x], grid[y * columns + x])
                };
                if !edge(x) {
                    x += 1;
                    continue;
                }
                let start = x;
                x += 1;
                while x < columns && edge(x) {
                    x += 1;
                }
                primitives.push(Primitive::Line {
                    layer,
                    start: point(start, y),
                    end: point(x, y),
                });
            }
        }
        for x in 0..=columns {
            let mut y = 0;
            while y < rows {
                let edge = |y: usize| {
                    x == 0
                        || x == columns
                        || boundary(grid[y * columns + x - 1], grid[y * columns + x])
                };
                if !edge(y) {
                    y += 1;
                    continue;
                }
                let start = y;
                y += 1;
                while y < rows && edge(y) {
                    y += 1;
                }
                primitives.push(Primitive::Line {
                    layer,
                    start: point(x, start),
                    end: point(x, y),
                });
            }
        }
    }
    if primitives.len() > MAX_RECORDS {
        return Err(
            "conversion exceeds 5000 entities; reduce columns/colors or disable contours".into(),
        );
    }
    let contours = primitives.len() - regions;
    let recipe = Recipe {
        schema_version: 1,
        id: config.id.clone(),
        bounds: [0.0, 0.0, 100.0, height_units],
        layers,
        primitives,
        font: None,
    };
    let drawing = recipe.compile()?;
    let vp = acad_render::Viewport::fit(&drawing.header.extents, 800, 600);
    let mut libraries = acad_render::Libraries::default();
    libraries.set_palette(acad_model::Palette::Aci256);
    let mut budget = acad_render::FrameBudget::default();
    let rendered = acad_render::flatten_with_budget(&drawing, &vp, &libraries, &mut budget);
    if rendered.incomplete
        || !rendered.diagnostics.is_empty()
        || rendered.primitives.len() > crate::art::MAX_PRIMITIVES
    {
        return Err("converted drawing exceeds renderer budget".into());
    }
    Ok((
        recipe,
        Report {
            generator: VERSION,
            image_sha256: hash,
            input_width: width,
            input_height: height,
            columns,
            rows,
            palette: indices,
            regions,
            contours,
            render_work: budget.used(),
        },
    ))
}

/// Canonical recipe bytes, also checked by the artwork exporter before publication.
pub fn recipe_bytes(recipe: &Recipe) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(recipe).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}
