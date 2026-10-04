//! `Editor` mutation methods invoked by command dispatch.

use crate::entity_ops::{
    bare, change_entity_layer, collect_insert_names, entity_anchor, has_repeat_lattice, root_item,
    transform_entity, transform_item,
};
use crate::geometry::{
    area_perimeter, break_entity_geometry, entity_layer, entity_points, hatch_geometry,
    repeat_points, rotate_point, HatchRequest,
};
use crate::input_state::Transform;
use crate::selection::{item_entity, selected_item_indexes};
use crate::{Editor, Effect, ErasedItem, UndoSnapshot};
use acad_model::{Block, Drawing, Entity, Extents, Item, Point};
use std::collections::BTreeSet;

impl Editor {
    pub(crate) fn cancel(&mut self) -> Result<Effect, String> {
        self.state = crate::InputState::Command;
        Ok(Effect::Continue)
    }

    pub(crate) fn add(&mut self, entity: Entity) {
        self.save_undo();
        self.drawing.items.push(Item::Entity(Entity::OnLayer {
            layer: self.drawing.header.current_layer,
            entity: Box::new(entity),
        }));
        self.refresh_after_edit();
    }

    pub(crate) fn ensure_layer(&mut self, layer: u8) {
        self.drawing.header.layers.entry(layer).or_insert(15);
    }

    pub(crate) fn set_view(&mut self, center: Point, height: f64) {
        self.previous_view = Some(self.drawing.header.view);
        self.drawing.header.view = acad_model::DwgView { center, height };
    }

    pub(crate) fn create_block(&mut self, name: String, base: Point, ids: &[usize]) {
        let selected = selected_item_indexes(&self.drawing, ids);
        self.save_undo();
        let mut retained = Vec::with_capacity(self.drawing.items.len() + 1);
        let mut entities = Vec::with_capacity(selected.len());
        for (index, item) in std::mem::take(&mut self.drawing.items)
            .into_iter()
            .enumerate()
        {
            if selected.contains(&(index + 1)) {
                let entity = item_entity(&item).expect("selected live object");
                entities.push(entity.clone());
                retained.push(Item::Erased(entity));
            } else {
                retained.push(item);
            }
        }
        retained.push(Item::Block(acad_model::Block {
            name: name.clone(),
            base,
            entities,
        }));
        self.drawing.items = retained;
        self.refresh_after_edit();
        self.status = format!("Created block {name}");
    }

    pub(crate) fn wblock_entire_drawing(&self) -> Result<Drawing, String> {
        self.wblock_closure(false)
    }

    /// Live-only whole export omits erased root records. A named-block export
    /// instead retains them: there every root erased record is a promoted erased
    /// block member, kept with its exact sign/layer/fields and dependencies.
    fn wblock_closure(&self, retain_erased: bool) -> Result<Drawing, String> {
        acad_model::group_codec::validate_group_items(
            self.drawing
                .items
                .iter()
                .filter(|item| retain_erased || !matches!(item, Item::Erased(_))),
        )
        .map_err(|error| format!("WBLOCK: {}", error.message()))?;
        let mut referenced = BTreeSet::new();
        for item in &self.drawing.items {
            match item {
                Item::Entity(entity) => collect_insert_names(entity, &mut referenced),
                Item::Erased(entity) if retain_erased => {
                    collect_insert_names(entity, &mut referenced)
                }
                Item::Repeat(repeat) => {
                    for entity in &repeat.entities {
                        collect_insert_names(entity, &mut referenced);
                    }
                }
                Item::Block(_) | Item::Erased(_) => {}
            }
        }

        // A referenced block can itself insert other blocks. Follow that
        // closure so WBLOCK * keeps the complete block graph while dropping
        // definitions no entity in the drawing can reach.
        loop {
            let before = referenced.len();
            for block in self.drawing.blocks() {
                if referenced.contains(&block.name.to_ascii_uppercase()) {
                    for entity in &block.entities {
                        collect_insert_names(entity, &mut referenced);
                    }
                }
            }
            if referenced.len() == before {
                break;
            }
        }

        let items = self
            .drawing
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Block(block) if referenced.contains(&block.name.to_ascii_uppercase()) => {
                    Some(Item::Block(block.clone()))
                }
                Item::Erased(entity) if retain_erased => Some(Item::Erased(entity.clone())),
                Item::Block(_) | Item::Erased(_) => None,
                Item::Entity(entity) => Some(Item::Entity(entity.clone())),
                Item::Repeat(repeat) => Some(Item::Repeat(repeat.clone())),
            })
            .collect();
        Ok(Drawing {
            header: self.drawing.header.clone(),
            items,
        })
    }

    pub(crate) fn wblock_named_block(&self, name: &str) -> Result<Drawing, String> {
        let block = self
            .drawing
            .blocks()
            .find(|block| block.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("WBLOCK block not found: {name}"))?;
        let mut header = self.drawing.header.clone();
        header.base = block.base;
        // Preserve the referenced definition closure, including INSERTs inside
        // stored Repeat members; the named block itself becomes root geometry.
        acad_model::group_codec::validate_group_items(
            self.drawing
                .items
                .iter()
                .filter(|item| matches!(item, Item::Block(_))),
        )
        .map_err(|error| format!("WBLOCK: {}", error.message()))?;
        let mut context = crate::export_context::Context::new(&self.drawing);
        let mut items: Vec<Item> = self.drawing.blocks().cloned().map(Item::Block).collect();
        for item in &self.drawing.items {
            if matches!(item, Item::Block(candidate) if std::ptr::eq(candidate, block)) {
                break;
            }
            items.extend(context.project(item)?);
        }
        // Members become root records; erased members stay negative records.
        items.extend(block.entities.iter().cloned().map(root_item));
        let exporter = Editor::new(Drawing { header, items });
        exporter.wblock_closure(true)
    }

    pub(crate) fn wblock_selected_entities(
        &self,
        ids: &[usize],
        base: Point,
    ) -> Result<Drawing, String> {
        let selected = selected_item_indexes(&self.drawing, ids);
        let mut header = self.drawing.header.clone();
        header.base = base;
        // Reuse whole-drawing reachability with only the selected live objects:
        // dependencies may be nested through repeat members and block INSERTs.
        acad_model::group_codec::validate_group_items(
            self.drawing
                .items
                .iter()
                .enumerate()
                .filter(|(index, item)| {
                    selected.contains(&(index + 1)) || matches!(item, Item::Block(_))
                })
                .map(|(_, item)| item),
        )
        .map_err(|error| format!("WBLOCK: {}", error.message()))?;
        let mut context = crate::export_context::Context::new(&self.drawing);
        let mut items = Vec::new();
        for (index, item) in self.drawing.items.iter().enumerate() {
            if selected.contains(&(index + 1)) || matches!(item, Item::Block(_)) {
                items.push(item.clone());
            } else {
                items.extend(context.project(item)?);
            }
        }
        let exporter = Editor::new(Drawing {
            header: self.drawing.header.clone(),
            items,
        });
        let items = exporter.wblock_entire_drawing()?.items;
        Ok(Drawing { header, items })
    }

    pub(crate) fn explode_block(&mut self, name: &str, origin: Point) -> Result<(), String> {
        let block = self
            .drawing
            .blocks()
            .find(|block| block.name == name)
            .ok_or_else(|| format!("unknown block: {name}"))?;
        let delta = Point {
            x: origin.x - block.base.x,
            y: origin.y - block.base.y,
        };
        let mut copies = block.entities.clone();
        for entity in &mut copies {
            transform_entity(entity, Transform::Translate(delta));
        }
        // Erased members are copied as root erased records (transformed like their
        // live siblings), never revived. They were not erased by this session's
        // ERASE, so OOPS does not restore them; the single UNDO removes them.
        self.save_undo();
        self.drawing.items.extend(copies.into_iter().map(root_item));
        self.refresh_after_edit();
        self.status = format!("Inserted {name} as separate entities");
        Ok(())
    }

    pub(crate) fn erase(&mut self, ids: &[usize]) {
        self.save_undo();
        let selected = selected_item_indexes(&self.drawing, ids);
        let mut erased = Vec::new();
        for (index, item) in self.drawing.items.iter_mut().enumerate() {
            if selected.contains(&(index + 1)) {
                let previous = item.clone();
                let entity = item_entity(&previous).expect("selected live object");
                *item = Item::Erased(entity);
                erased.push(ErasedItem {
                    index,
                    item: previous,
                });
            }
        }
        self.last_erased = Some(erased);
        self.refresh_after_edit();
        self.status = format!("Erased {} entities", ids.len());
    }

    pub(crate) fn oops(&mut self) {
        let Some(erased) = self.last_erased.clone() else {
            self.status = "Nothing to restore".into();
            return;
        };
        self.save_undo();
        self.last_erased = None;
        for erased_item in &erased {
            if matches!(
                self.drawing.items.get(erased_item.index),
                Some(Item::Erased(_))
            ) {
                self.drawing.items[erased_item.index] = erased_item.item.clone();
            } else {
                let index = erased_item.index.min(self.drawing.items.len());
                self.drawing.items.insert(index, erased_item.item.clone());
            }
        }
        self.refresh_after_edit();
        self.status = format!("Restored {} erased entities", erased.len());
    }

    pub(crate) fn array(
        &mut self,
        ids: &[usize],
        rows: usize,
        columns: usize,
        row_spacing: f64,
        column_spacing: f64,
    ) {
        let source_indexes = selected_item_indexes(&self.drawing, ids);
        let sources: Vec<Entity> = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                source_indexes
                    .contains(&(index + 1))
                    .then(|| item_entity(item))?
            })
            .collect();
        let copies_count = rows * columns - 1;
        let mut copies = Vec::with_capacity(sources.len().saturating_mul(copies_count));
        for column in 0..columns {
            for row in 0..rows {
                if row == 0 && column == 0 {
                    continue;
                }
                let offset = Point {
                    x: column_spacing * column as f64,
                    y: row_spacing * row as f64,
                };
                copies.extend(sources.iter().cloned().map(|mut entity| {
                    transform_entity(&mut entity, Transform::Translate(offset));
                    Item::Entity(entity)
                }));
            }
        }
        let copy_count = copies.len();
        if copy_count > 0 {
            // An array that adds nothing records no undo step.
            self.save_undo();
            self.drawing.items.extend(copies);
            self.refresh_after_edit();
        }
        self.status = format!("Created {rows}x{columns} array with {copy_count} copies");
    }

    pub(crate) fn circular_array(
        &mut self,
        ids: &[usize],
        center: Point,
        angle: f64,
        count: usize,
        rotate_blocks: bool,
    ) -> Result<(), String> {
        let source_indexes = selected_item_indexes(&self.drawing, ids);
        let sources: Vec<Entity> = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                source_indexes
                    .contains(&(index + 1))
                    .then(|| item_entity(item))?
            })
            .collect();
        if sources.iter().any(|source| entity_anchor(source).is_none()) {
            return Err(
                "circular ARRAY requires a geometric anchor for each selected object".into(),
            );
        }
        let mut copies = Vec::with_capacity(sources.len().saturating_mul(count.saturating_sub(1)));
        for item in 1..count {
            let degrees = angle * item as f64;
            for source in &sources {
                let Some(anchor) = entity_anchor(source) else {
                    continue;
                };
                let target = rotate_point(anchor, center, degrees);
                let delta = Point {
                    x: target.x - anchor.x,
                    y: target.y - anchor.y,
                };
                let mut entity = source.clone();
                transform_entity(&mut entity, Transform::Translate(delta));
                if rotate_blocks {
                    rotate_insert(&mut entity, degrees);
                }
                copies.push(Item::Entity(entity));
            }
        }
        let copy_count = copies.len();
        if copy_count > 0 {
            // An array that adds nothing records no undo step.
            self.save_undo();
            self.drawing.items.extend(copies);
            self.refresh_after_edit();
        }
        self.status = format!("Created circular array with {copy_count} copies");
        Ok(())
    }

    pub(crate) fn change_layer(&mut self, ids: &[usize], layer: u8) {
        let selected = selected_item_indexes(&self.drawing, ids);
        let mut replacements = Vec::new();
        for (index, item) in self.drawing.items.iter().enumerate() {
            if selected.contains(&(index + 1)) {
                let mut changed = item.clone();
                match &mut changed {
                    Item::Entity(entity) => change_entity_layer(entity, layer),
                    Item::Repeat(repeat) => {
                        for entity in &mut repeat.entities {
                            change_entity_layer(entity, layer);
                        }
                    }
                    _ => unreachable!("selected live object"),
                }
                if changed != *item {
                    replacements.push((index, changed));
                }
            }
        }
        if !replacements.is_empty() || !self.drawing.header.layers.contains_key(&layer) {
            self.save_undo();
            for (index, item) in replacements {
                self.drawing.items[index] = item;
            }
            self.ensure_layer(layer);
        }
        self.status = format!("Changed {} entities to layer {layer}", ids.len());
    }

    /// Whether the selection is exactly one top-level INSERT, the only case
    /// in which circular ARRAY offers to rotate copies.
    pub(crate) fn single_insert_selected(&self, ids: &[usize]) -> bool {
        let indexes = selected_item_indexes(&self.drawing, ids);
        indexes.len() == 1
            && indexes.first().is_some_and(|index| {
                matches!(
                    self.drawing.items.get(index - 1),
                    Some(Item::Entity(entity)) if matches!(bare(entity), Entity::Insert { .. })
                )
            })
    }

    /// World aperture for a typed BREAK object point: 1% of the effective
    /// view height (the view, else LIMITS, else one unit), as the renderer
    /// chooses the shown box.
    pub(crate) fn typed_pick_aperture(&self) -> f64 {
        let header = &self.drawing.header;
        let limits = header.limits.ymax - header.limits.ymin;
        let height = [header.view.height, limits]
            .into_iter()
            .find(|height| height.is_finite() && *height > 0.0)
            .unwrap_or(1.0);
        height * 0.01
    }

    /// Point-to-object step of BREAK: pick the nearest visible live object and
    /// use the pick as the first break point. Misses return `None` and leave
    /// the prompt; unbreakable objects are refused without changing state.
    pub(crate) fn break_pick(
        &mut self,
        pick: Point,
        aperture: f64,
    ) -> Result<Option<usize>, String> {
        let Some(id) = self.pick_entity_at(pick, aperture) else {
            self.status = "No object found".into();
            return Ok(None);
        };
        let index = *selected_item_indexes(&self.drawing, &[id])
            .first()
            .expect("picked a selectable object");
        match &self.drawing.items[index - 1] {
            Item::Entity(entity) => breakable_kind(entity)?,
            _ => return Err("BREAK does not support REPEAT groups".into()),
        };
        self.state = crate::InputState::BreakSecondPoint(vec![id], pick, true);
        self.status = format!("Selected entity {id}");
        Ok(Some(id))
    }

    pub(crate) fn break_entity(
        &mut self,
        ids: &[usize],
        first: Point,
        second: Point,
    ) -> Result<(), String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
        let item_index = *indexes
            .first()
            .ok_or_else(|| "BREAK selection is empty".to_owned())?;
        let Some(Item::Entity(entity)) = self.drawing.items.get(item_index - 1) else {
            return Err("BREAK selection is not a top-level entity".into());
        };
        let entity_kind = breakable_kind(entity)?;
        let pieces = break_entity_geometry(entity, first, second)?;
        let layer = entity_layer(entity);
        let source = entity.clone();
        let layered = |piece: Entity| Entity::OnLayer {
            layer,
            entity: Box::new(piece),
        };

        self.save_undo();
        // Record policy measured on the original: the start-side piece keeps
        // the record; without one, the record is erased and any end-side
        // remainder appended.
        self.drawing.items[item_index - 1] = match pieces.start {
            Some(start) => Item::Entity(layered(start)),
            None => Item::Erased(source),
        };
        let whole =
            pieces.end.is_none() && matches!(self.drawing.items[item_index - 1], Item::Erased(_));
        if let Some(end) = pieces.end {
            self.drawing.items.push(Item::Entity(layered(end)));
        }
        self.refresh_after_edit();
        // BREAK's erased record is not an ERASE set: OOPS keeps restoring the
        // last ERASE only, as the original does (in-tree anchored).
        self.status = if whole {
            format!("Broke {entity_kind}: whole object removed")
        } else {
            format!("Broke {entity_kind}")
        };
        Ok(())
    }

    pub(crate) fn measure_area(&mut self, ids: &[usize]) -> Result<(), String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
        if indexes
            .iter()
            .any(|index| matches!(self.drawing.items[index - 1], Item::Repeat(_)))
        {
            return Err("ENTITYAREA does not support REPEAT groups".into());
        }
        let entities: Vec<_> = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if indexes.contains(&(index + 1)) {
                    match item {
                        Item::Entity(entity) => Some(entity),
                        Item::Block(_) | Item::Repeat(_) | Item::Erased(_) => None,
                    }
                } else {
                    None
                }
            })
            .collect();
        let (area, perimeter) = area_perimeter(&entities)?;
        self.status = format!("Area={area:.6}, Perimeter={perimeter:.6}");
        Ok(())
    }

    pub(crate) fn add_hatch(
        &mut self,
        request: &HatchRequest,
        scale: f64,
        angle_deg: f64,
        ids: &[usize],
    ) -> Result<(), String> {
        let lines = hatch_geometry(&self.drawing, ids, request, scale, angle_deg)?;
        if lines.is_empty() {
            return Err("HATCH boundary produced no pattern lines".into());
        }
        let mut suffix = 1usize;
        let name = loop {
            let candidate = format!("*X{suffix}");
            if self
                .drawing
                .blocks()
                .all(|block| !block.name.eq_ignore_ascii_case(&candidate))
            {
                break candidate;
            }
            suffix += 1;
        };
        self.save_undo();
        self.drawing.items.push(Item::Block(Block {
            name: name.clone(),
            base: Point { x: 0.0, y: 0.0 },
            entities: lines
                .into_iter()
                .map(|entity| Entity::OnLayer {
                    layer: 127,
                    entity: Box::new(entity),
                })
                .collect(),
        }));
        self.drawing.items.push(Item::Entity(Entity::OnLayer {
            layer: self.drawing.header.current_layer,
            entity: Box::new(Entity::Insert {
                origin: Point { x: 0.0, y: 0.0 },
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 0.0,
                name,
            }),
        }));
        Ok(())
    }

    pub(crate) fn save_undo(&mut self) {
        self.undo.push(UndoSnapshot {
            drawing: self.drawing.clone(),
            last_curve: self.last_curve.clone(),
            last_text: self.last_text.clone(),
            last_dimension: self.last_dimension,
            last_erased: self.last_erased.clone(),
            active_shape_library: self.active_shape_library.clone(),
        });
    }

    pub(crate) fn transform(
        &mut self,
        ids: &[usize],
        transform: Transform,
        copy: bool,
    ) -> Result<(), String> {
        let selected = selected_item_indexes(&self.drawing, ids);
        if matches!(transform, Transform::Rotate { .. })
            && selected.iter().any(|index| {
                item_entity(&self.drawing.items[index - 1])
                    .is_some_and(|entity| has_repeat_lattice(&entity))
            })
        {
            return Err("ROTATE of a REPEAT lattice is not supported; its rectangular axes cannot store rotation".into());
        }
        self.save_undo();
        if copy {
            let mut copies = Vec::new();
            for (index, item) in self.drawing.items.iter().enumerate() {
                if selected.contains(&(index + 1)) {
                    let mut item = item.clone();
                    transform_item(&mut item, transform);
                    copies.push(item);
                }
            }
            self.drawing.items.extend(copies);
        } else {
            for (index, item) in self.drawing.items.iter_mut().enumerate() {
                if selected.contains(&(index + 1)) {
                    transform_item(item, transform);
                }
            }
        }
        self.refresh_after_edit();
        self.status = format!(
            "{} {} entities",
            if copy { "Copied" } else { "Transformed" },
            ids.len()
        );
        Ok(())
    }

    fn refresh_limits(&mut self) {
        let mut points = Vec::new();
        for item in &self.drawing.items {
            match item {
                Item::Entity(entity) => entity_points(entity, &mut points),
                Item::Repeat(repeat) => repeat_points(repeat, &mut points),
                Item::Block(_) | Item::Erased(_) => {}
            }
        }
        if points.is_empty() {
            return;
        }
        let mut bounds = Extents {
            xmin: points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
            xmax: points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
            ymin: points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
            ymax: points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
        };
        if bounds.width() == 0.0 {
            bounds.xmin -= 0.5;
            bounds.xmax += 0.5;
        }
        if bounds.height() == 0.0 {
            bounds.ymin -= 0.5;
            bounds.ymax += 0.5;
        }
        self.drawing.header.extents = bounds;
        self.drawing.header.limits = bounds;
        self.drawing.header.view = acad_model::DwgView {
            center: Point {
                x: (bounds.xmin + bounds.xmax) / 2.0,
                y: (bounds.ymin + bounds.ymax) / 2.0,
            },
            height: bounds.height(),
        };
    }

    pub(crate) fn refresh_after_edit(&mut self) {
        // A changed/erased source cannot regain continuation merely by later
        // acquiring identical coordinates. UNDO restores its own history.
        if self.curve_tangent().is_none() {
            self.last_curve = None;
        }
        let view = self.drawing.header.view;
        let limits = self.drawing.header.limits;
        self.refresh_limits();
        self.drawing.header.view = view;
        self.drawing.header.limits = limits;
        if !self
            .drawing
            .entities()
            .any(|entity| !matches!(bare(entity), Entity::Load { .. }))
        {
            self.drawing.header.extents = Extents {
                xmin: 0.0,
                xmax: 0.0,
                ymin: 0.0,
                ymax: 0.0,
            };
        }
    }
}

fn breakable_kind(entity: &Entity) -> Result<&'static str, String> {
    match bare(entity) {
        Entity::Line { .. } => Ok("line"),
        Entity::Trace { .. } => Ok("trace"),
        Entity::Circle { .. } => Ok("circle"),
        Entity::Arc { .. } => Ok("arc"),
        Entity::Insert { .. } => Err("Can't break a block".into()),
        Entity::Repeat(_) => Err("BREAK does not support REPEAT groups".into()),
        _ => Err("BREAK needs a LINE, TRACE, CIRCLE or ARC".into()),
    }
}

/// Rotate a copied INSERT's own angle, normalized to `[0, 360)` as the
/// original stores it.
fn rotate_insert(entity: &mut Entity, degrees: f64) {
    match entity {
        Entity::OnLayer { entity, .. } => rotate_insert(entity, degrees),
        Entity::Insert { rotation_deg, .. } => {
            *rotation_deg = (*rotation_deg + degrees).rem_euclid(360.0);
        }
        _ => {}
    }
}
