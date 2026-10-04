//! Ordered library state, visibility and resource-aware entity expansion.
use crate::flatten::{aci_rgb, flatten_entity, shift_prim, style, Prim};
use crate::Viewport;
use acad_model::{Drawing, Entity, Point, Repeat};

#[derive(Clone)]
pub(crate) struct LibraryState {
    pub(crate) font: String,
    pub(crate) shapes: Vec<String>,
}

pub(crate) struct Walker<'a> {
    pub(crate) drawing: &'a Drawing,
    pub(crate) viewport: &'a Viewport,
    pub(crate) libraries: &'a crate::Libraries,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) context_failed: bool,
}

impl Walker<'_> {
    pub(crate) fn repeat(
        &mut self,
        repeat: &Repeat,
        state: &mut LibraryState,
        depth: u32,
    ) -> Vec<Prim> {
        let base: Vec<Prim> = repeat
            .entities
            .iter()
            .flat_map(|e| self.entity(e, state, depth))
            .collect();
        let origin = self.viewport.to_screen(Point { x: 0.0, y: 0.0 });
        let mut out = Vec::new();
        for row in 0..repeat.rows {
            for column in 0..repeat.columns {
                let offset = self.viewport.to_screen(Point {
                    x: f64::from(column) * repeat.column_spacing,
                    y: f64::from(row) * repeat.row_spacing,
                });
                let delta = Point {
                    x: offset.x - origin.x,
                    y: offset.y - origin.y,
                };
                out.extend(base.iter().cloned().map(|p| shift_prim(p, delta)));
            }
        }
        out
    }
    pub(crate) fn report(&mut self, message: String) {
        if !self.diagnostics.contains(&message) {
            self.diagnostics.push(message);
        }
    }
    pub(crate) fn entity(
        &mut self,
        entity: &Entity,
        state: &mut LibraryState,
        depth: u32,
    ) -> Vec<Prim> {
        if !self.drawing.header.entity_is_visible(entity) {
            self.hidden_loads(entity, state, depth);
            return Vec::new();
        }
        self.visible_entity(entity, state, depth)
    }
    // Library records keep their ordered effect even inside hidden groups.
    pub(crate) fn hidden_loads(&mut self, entity: &Entity, state: &mut LibraryState, depth: u32) {
        self.hidden_loads_bounded(entity, state, depth, 0, &mut 100_000);
    }
    pub(crate) fn hidden_repeat_loads(
        &mut self,
        repeat: &Repeat,
        state: &mut LibraryState,
        depth: u32,
    ) {
        let mut remaining = 100_000;
        for child in &repeat.entities {
            self.hidden_loads_bounded(child, state, depth, 0, &mut remaining);
            if self.context_failed {
                break;
            }
        }
    }
    fn hidden_loads_bounded(
        &mut self,
        entity: &Entity,
        state: &mut LibraryState,
        depth: u32,
        stored_depth: usize,
        remaining: &mut usize,
    ) {
        if stored_depth > 256 || *remaining == 0 {
            self.context_failed = true;
            self.report("LOAD context exceeds stored traversal budget".into());
            return;
        }
        *remaining -= 1;
        match entity {
            // Hidden-owner LOAD replay stops at erased members, without unwrapping.
            Entity::Erased(_) => {}
            Entity::OnLayer { entity, .. } => {
                self.hidden_loads_bounded(entity, state, depth, stored_depth + 1, remaining)
            }
            Entity::Load { .. } => {
                self.visible_entity(entity, state, depth);
            }
            Entity::Repeat(repeat) => {
                for child in &repeat.entities {
                    self.hidden_loads_bounded(child, state, depth, stored_depth + 1, remaining);
                    if self.context_failed {
                        break;
                    }
                }
            }
            Entity::Insert { .. } if depth == 0 => {
                self.context_failed = true;
                self.report("LOAD context: block recursion limit reached".into());
            }
            Entity::Insert { name, .. } if depth > 0 => {
                if let Some(block) = self.drawing.block(name) {
                    for child in &block.entities {
                        self.hidden_loads_bounded(
                            child,
                            state,
                            depth - 1,
                            stored_depth + 1,
                            remaining,
                        );
                        if self.context_failed {
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn visible_entity(
        &mut self,
        entity: &Entity,
        state: &mut LibraryState,
        depth: u32,
    ) -> Vec<Prim> {
        match entity {
            Entity::Erased(_) => Vec::new(),
            Entity::Repeat(repeat) => self.repeat(repeat, state, depth),
            Entity::OnLayer { layer, entity } => {
                let color_index = self.drawing.header.layers.get(layer).copied().unwrap_or(7);
                style(
                    self.visible_entity(entity, state, depth),
                    aci_rgb(color_index),
                )
            }
            Entity::Load { name } => {
                match self.libraries.get(name) {
                    Some(library) if library.cap_height.is_some() => state.font = name.clone(),
                    Some(_) => state.shapes.push(name.clone()),
                    None => {
                        self.report(format!("LOAD: missing SHP library {name}"));
                        // Do not render following text with a stale font.
                        state.font = name.clone();
                    }
                }
                Vec::new()
            }
            Entity::Text {
                origin,
                height,
                rotation_deg,
                value,
            } => {
                let name = &state.font;
                let Some(library) = self.libraries.get(name) else {
                    self.report(format!("TEXT: missing SHP font {name}"));
                    return Vec::new();
                };
                match library.text(value) {
                    Ok(glyph) => self.place(
                        glyph,
                        *origin,
                        *height / library.cap_height.unwrap(),
                        *rotation_deg,
                    ),
                    Err(e) => {
                        self.report(format!("TEXT {value:?}, font {name}: {e}"));
                        Vec::new()
                    }
                }
            }
            Entity::Shape {
                origin,
                height,
                rotation_deg,
                number,
            } => {
                let library = state.shapes.iter().rev().find_map(|name| {
                    self.libraries
                        .get(name)
                        .filter(|l| l.contains(*number))
                        .map(|l| (name, l))
                });
                let Some((name, library)) = library else {
                    self.report(format!(
                        "SHAPE {number}: no loaded library defines this shape"
                    ));
                    return Vec::new();
                };
                match library.shape(*number) {
                    Ok(glyph) => self.place(glyph, *origin, *height, *rotation_deg),
                    Err(e) => {
                        self.report(format!("SHAPE {number}, library {name}: {e}"));
                        Vec::new()
                    }
                }
            }
            Entity::Insert {
                origin,
                x_scale,
                y_scale,
                rotation_deg,
                name,
            } => {
                if depth == 0 {
                    self.report(format!("INSERT {name}: block recursion limit reached"));
                    return Vec::new();
                }
                let Some(block) = self.drawing.block(name) else {
                    self.report(format!("INSERT: missing block {name}"));
                    return Vec::new();
                };
                let (sin, cos) = rotation_deg.to_radians().sin_cos();
                let mut out = Vec::new();
                for inner in &block.entities {
                    for prim in self.entity(inner, state, depth - 1) {
                        let (points, filled, color) = match prim {
                            Prim::Polyline(points) => (points, false, None),
                            Prim::FilledPolygon(points) => (points, true, None),
                            Prim::ColoredPolyline { points, rgb } => (points, false, Some(rgb)),
                            Prim::ColoredFilledPolygon { points, rgb } => (points, true, Some(rgb)),
                        };
                        let points = points
                            .into_iter()
                            .map(|p| {
                                let p = self.viewport.to_world(p);
                                let x = (p.x - block.base.x) * x_scale;
                                let y = (p.y - block.base.y) * y_scale;
                                self.viewport.to_screen(Point {
                                    x: origin.x + x * cos - y * sin,
                                    y: origin.y + x * sin + y * cos,
                                })
                            })
                            .collect();
                        out.push(match (filled, color) {
                            (true, Some(rgb)) => Prim::ColoredFilledPolygon { points, rgb },
                            (false, Some(rgb)) => Prim::ColoredPolyline { points, rgb },
                            (true, None) => Prim::FilledPolygon(points),
                            (false, None) => Prim::Polyline(points),
                        });
                    }
                }
                out
            }
            Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
                let points = vec![
                    self.viewport.to_screen(*p1),
                    self.viewport.to_screen(*p2),
                    self.viewport.to_screen(*p4),
                    self.viewport.to_screen(*p3),
                    self.viewport.to_screen(*p1),
                ];
                let mut out = Vec::with_capacity(2);
                if self.drawing.header.fill {
                    out.push(Prim::FilledPolygon(points.clone()));
                }
                out.push(Prim::Polyline(points));
                out
            }
            other => flatten_entity(other, self.viewport),
        }
    }
    fn place(
        &self,
        glyph: crate::shp::Glyph,
        origin: Point,
        scale: f64,
        rotation: f64,
    ) -> Vec<Prim> {
        let (sin, cos) = rotation.to_radians().sin_cos();
        glyph
            .strokes
            .into_iter()
            .map(|points| {
                Prim::Polyline(
                    points
                        .into_iter()
                        .map(|p| {
                            self.viewport.to_screen(Point {
                                x: origin.x + scale * (p.x * cos - p.y * sin),
                                y: origin.y + scale * (p.x * sin + p.y * cos),
                            })
                        })
                        .collect(),
                )
            })
            .collect()
    }
}

/// Resolve the renderer's ordered font immediately before a top-level item or
/// append position. LOAD context is walked without expanding generated cells;
/// hidden geometry still contributes LOAD, erased/definition records do not.
pub fn text_font_at(
    drawing: &Drawing,
    before_item: usize,
    libraries: &crate::Libraries,
) -> Result<String, String> {
    if before_item > drawing.items.len() {
        return Err("TEXT font position is outside the drawing".into());
    }
    let viewport = Viewport::from_view(Point { x: 0.0, y: 0.0 }, 1.0, 1, 1);
    let mut walker = Walker {
        drawing,
        viewport: &viewport,
        libraries,
        diagnostics: Vec::new(),
        context_failed: false,
    };
    let mut state = LibraryState {
        font: "TXT".into(),
        shapes: Vec::new(),
    };
    for item in drawing.items.iter().take(before_item) {
        match item {
            acad_model::Item::Entity(e) => walker.hidden_loads(e, &mut state, 16),
            acad_model::Item::Repeat(r) => walker.hidden_repeat_loads(r, &mut state, 16),
            acad_model::Item::Block(_) | acad_model::Item::Erased(_) => {}
        }
        if walker.context_failed {
            return Err(format!(
                "TEXT font context failed: {}",
                walker.diagnostics.join("; ")
            ));
        }
    }
    Ok(state.font)
}
