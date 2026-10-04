//! Ordered library state, visibility and resource-aware entity expansion.
use crate::budget::{name_units, prim_units, FrameBudget};
use crate::flatten::{aci_rgb, flatten_entity, shift_prim, style, Prim};
use crate::Viewport;
use acad_model::{BlockIndex, Drawing, Entity, Point, Repeat};

#[derive(Clone)]
pub(crate) struct LibraryState {
    pub(crate) font: String,
    pub(crate) shapes: Vec<String>,
}

pub(crate) struct Walker<'a> {
    pub(crate) drawing: &'a Drawing,
    /// Built once per pass: `Drawing::block` is a linear scan per INSERT.
    pub(crate) blocks: BlockIndex<'a>,
    pub(crate) viewport: &'a Viewport,
    pub(crate) libraries: &'a crate::Libraries,
    pub(crate) diagnostics: Vec<String>,
    pub(crate) context_failed: bool,
    /// Whole-frame aggregate work; once exhausted every walk returns nothing
    /// and the owner loop discards the current owner (`flatten_owners`).
    pub(crate) budget: FrameBudget,
    pub(crate) reported: std::collections::HashSet<String>,
    /// Distinct diagnostics dropped after `MAX_DIAGNOSTICS`.
    pub(crate) suppressed: usize,
    /// First message of each incompleteness kind that arrived after the cap;
    /// always reported so "see diagnostics" is satisfiable.
    pub(crate) pinned: Vec<String>,
    /// Whether a message of each [`Reason`] is already kept or pinned.
    pub(crate) reason_seen: [bool; 2],
}

/// Kinds of truncation that make a pass incomplete (besides a budget stop,
/// whose message is always last).
#[derive(Clone, Copy)]
pub(crate) enum Reason {
    ContextFailure = 0,
    OwnerSkipped = 1,
}

/// Distinct diagnostics kept per render pass; bounds diagnostic memory/time.
pub(crate) const MAX_DIAGNOSTICS: usize = 64;
/// Characters of a TEXT value quoted in a diagnostic.
const QUOTED_TEXT_CHARS: usize = 40;

/// `Header::entity_is_visible`, counting the records it inspects.
fn visible_counted(header: &acad_model::Header, entity: &Entity, scanned: &mut usize) -> bool {
    *scanned += 1;
    match entity {
        Entity::Erased(_) => false,
        Entity::Load { .. } => true,
        Entity::Repeat(repeat) => repeat
            .entities
            .iter()
            .any(|e| visible_counted(header, e, scanned)),
        Entity::OnLayer { layer, entity } => {
            if matches!(entity.as_ref(), Entity::Load { .. }) {
                true
            } else {
                header.layer_is_visible(*layer)
                    && match entity.as_ref() {
                        Entity::OnLayer { .. } | Entity::Repeat(_) | Entity::Erased(_) => {
                            visible_counted(header, entity, scanned)
                        }
                        _ => true,
                    }
            }
        }
        _ => header.layer_is_visible(1),
    }
}

/// A drawing-supplied name for a diagnostic: unchanged up to 40 characters,
/// otherwise its first 40 characters and `...`.
pub(crate) fn short(name: &str) -> std::borrow::Cow<'_, str> {
    match name.char_indices().nth(QUOTED_TEXT_CHARS) {
        None => name.into(),
        Some((end, _)) => format!("{}...", &name[..end]).into(),
    }
}

fn quoted_text(value: &str) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(QUOTED_TEXT_CHARS).collect();
    if chars.next().is_some() {
        format!("{head:?}...")
    } else {
        format!("{head:?}")
    }
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
        // Charge every generated cell copy before any of them allocate, and
        // at least one unit per cell so an empty base is never free.
        let cells = usize::from(repeat.rows).saturating_mul(usize::from(repeat.columns));
        let per_cell = prim_units(&base).saturating_add(1);
        if !self.budget.charge(per_cell.saturating_mul(cells)) || base.is_empty() {
            return Vec::new();
        }
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
    /// Distinct diagnostics, at most `MAX_DIAGNOSTICS` per pass; the rest are
    /// only counted (summarized by `finish_diagnostics`). Dedup is hashed.
    pub(crate) fn report(&mut self, message: String) {
        if self.reported.contains(&message) {
            return;
        }
        if self.diagnostics.len() >= MAX_DIAGNOSTICS {
            self.suppressed += 1;
            return;
        }
        self.reported.insert(message.clone());
        self.diagnostics.push(message);
    }
    /// Like `report`, but the first message of each kind is kept even when
    /// the cap is full.
    pub(crate) fn report_reason(&mut self, reason: Reason, message: String) {
        let seen = std::mem::replace(&mut self.reason_seen[reason as usize], true);
        if self.reported.contains(&message) {
            return;
        }
        if self.diagnostics.len() < MAX_DIAGNOSTICS || seen {
            self.report(message);
            return;
        }
        self.reported.insert(message.clone());
        self.pinned.push(message);
    }
    /// Append the suppressed-count summary and any must-report final message
    /// (the budget stop), which bypasses the cap.
    pub(crate) fn finish_diagnostics(&mut self, last: Option<String>) -> Vec<String> {
        let mut diagnostics = std::mem::take(&mut self.diagnostics);
        diagnostics.append(&mut self.pinned);
        if self.suppressed > 0 {
            diagnostics.push(format!(
                "... and {} more render diagnostic(s) not listed",
                self.suppressed
            ));
        }
        diagnostics.extend(last);
        diagnostics
    }
    pub(crate) fn entity(
        &mut self,
        entity: &Entity,
        state: &mut LibraryState,
        depth: u32,
    ) -> Vec<Prim> {
        if !self.budget.charge(1) {
            return Vec::new();
        }
        // The visibility gate scans REPEAT subtrees and layer wrappers; charge
        // every record it inspects beyond this visit.
        let mut scanned = 0;
        let visible = visible_counted(&self.drawing.header, entity, &mut scanned);
        debug_assert_eq!(visible, self.drawing.header.entity_is_visible(entity));
        if !self.budget.charge(scanned.saturating_sub(1)) {
            return Vec::new();
        }
        if !visible {
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
        if !self.budget.charge(1) {
            return;
        }
        if stored_depth > 256 || *remaining == 0 {
            self.context_failed = true;
            self.report_reason(
                Reason::ContextFailure,
                "LOAD context exceeds stored traversal budget".into(),
            );
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
                self.report_reason(
                    Reason::ContextFailure,
                    "LOAD context: block recursion limit reached".into(),
                );
            }
            Entity::Insert { name, .. } if depth > 0 => {
                if !self.budget.charge(name_units(name)) {
                    return;
                }
                if let Some(block) = self.blocks.get(name) {
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
                if !self.budget.charge(name_units(name)) {
                    return Vec::new();
                }
                match self.libraries.get(name) {
                    Some(library) if library.cap_height.is_some() => state.font = name.clone(),
                    Some(library) => {
                        // Keep one entry per resolved library, the most recent
                        // name on top: SHAPE search order and the reported
                        // name are unchanged, and the stack is bounded by the
                        // supplied libraries instead of the LOAD count.
                        let libraries = self.libraries;
                        state.shapes.retain(|older| {
                            !libraries
                                .get(older)
                                .is_some_and(|other| std::ptr::eq(other, library))
                        });
                        state.shapes.push(name.clone());
                    }
                    None => {
                        let shown = short(name);
                        self.report(format!("LOAD: missing SHP library {shown}"));
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
                if !self.budget.charge(name_units(name)) {
                    return Vec::new();
                }
                let Some(library) = self.libraries.get(name) else {
                    let shown = short(name);
                    self.report(format!("TEXT: missing SHP font {shown}"));
                    return Vec::new();
                };
                let (glyph, executed) = library.text_counted(value);
                // Failed programs are charged for the instructions they ran.
                if !self.budget.charge(executed) {
                    return Vec::new();
                }
                match glyph {
                    Ok(glyph) => self.place(
                        glyph,
                        *origin,
                        *height / library.cap_height.unwrap(),
                        *rotation_deg,
                    ),
                    Err(e) => {
                        let text = quoted_text(value);
                        let shown = short(name);
                        self.report(format!("TEXT {text}, font {shown}: {e}"));
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
                let scan = state.shapes.iter().map(|name| 1 + name_units(name)).sum();
                if !self.budget.charge(scan) {
                    return Vec::new();
                }
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
                let (glyph, executed) = library.shape_counted(*number);
                if !self.budget.charge(executed) {
                    return Vec::new();
                }
                match glyph {
                    Ok(glyph) => self.place(glyph, *origin, *height, *rotation_deg),
                    Err(e) => {
                        let shown = short(name);
                        self.report(format!("SHAPE {number}, library {shown}: {e}"));
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
                if !self.budget.charge(name_units(name)) {
                    return Vec::new();
                }
                if depth == 0 {
                    let shown = short(name);
                    self.report(format!("INSERT {shown}: block recursion limit reached"));
                    return Vec::new();
                }
                let Some(block) = self.blocks.get(name) else {
                    let shown = short(name);
                    self.report(format!("INSERT: missing block {shown}"));
                    return Vec::new();
                };
                let (sin, cos) = rotation_deg.to_radians().sin_cos();
                let mut out = Vec::new();
                for inner in &block.entities {
                    let prims = self.entity(inner, state, depth - 1);
                    // Each nesting level re-transforms its children's vertices.
                    if !self.budget.charge(prim_units(&prims)) {
                        return Vec::new();
                    }
                    for prim in prims {
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
                self.emit(out)
            }
            other => {
                let prims = flatten_entity(other, self.viewport);
                self.emit(prims)
            }
        }
    }
    fn emit(&mut self, prims: Vec<Prim>) -> Vec<Prim> {
        if self.budget.charge(prim_units(&prims)) {
            prims
        } else {
            Vec::new()
        }
    }
    fn place(
        &mut self,
        glyph: crate::shp::Glyph,
        origin: Point,
        scale: f64,
        rotation: f64,
    ) -> Vec<Prim> {
        let (sin, cos) = rotation.to_radians().sin_cos();
        let prims = glyph
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
            .collect();
        self.emit(prims)
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
        blocks: drawing.block_index(),
        viewport: &viewport,
        libraries,
        diagnostics: Vec::new(),
        context_failed: false,
        // Command-time font lookup, not a frame: per-owner LOAD replay bounds
        // it (100,000 visits per owner); see docs/native-render-budget.md.
        budget: FrameBudget::new(usize::MAX),
        reported: Default::default(),
        suppressed: 0,
        pinned: Vec::new(),
        reason_seen: [false; 2],
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
