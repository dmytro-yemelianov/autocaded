//! `Editor` mutation methods invoked by command dispatch.

use crate::entity_ops::{
    apply_change_point, bare, can_change_point, collect_insert_names, entity_anchor,
    transform_entity,
};
use crate::geometry::{
    area_perimeter, assign_layer, break_entity_geometry, entity_layer, entity_points,
    fillet_geometry, hatch_line_geometry, line_points, repeat_points, rotate_point,
    set_line_points,
};
use crate::input_state::Transform;
use crate::selection::selected_item_indexes;
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
        self.refresh_limits();
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
                let Item::Entity(entity) = item else {
                    unreachable!("only entities can be selected for BLOCK");
                };
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

    pub(crate) fn wblock_entire_drawing(&self) -> Drawing {
        let mut referenced = BTreeSet::new();
        for item in &self.drawing.items {
            match item {
                Item::Entity(entity) => collect_insert_names(entity, &mut referenced),
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
                Item::Block(_) | Item::Erased(_) => None,
                Item::Entity(entity) => Some(Item::Entity(entity.clone())),
                Item::Repeat(repeat) => Some(Item::Repeat(repeat.clone())),
            })
            .collect();
        Drawing {
            header: self.drawing.header.clone(),
            items,
        }
    }

    pub(crate) fn wblock_named_block(&self, name: &str) -> Result<Drawing, String> {
        let block = self
            .drawing
            .blocks()
            .find(|block| block.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("WBLOCK block not found: {name}"))?;
        let mut header = self.drawing.header.clone();
        header.base = block.base;
        Ok(Drawing {
            header,
            items: block.entities.iter().cloned().map(Item::Entity).collect(),
        })
    }

    pub(crate) fn wblock_selected_entities(&self, ids: &[usize], base: Point) -> Drawing {
        let selected = selected_item_indexes(&self.drawing, ids);
        let mut header = self.drawing.header.clone();
        header.base = base;
        let items = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter(|(index, _)| selected.contains(&(index + 1)))
            .map(|(_, item)| item.clone())
            .collect();
        Drawing { header, items }
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
        self.save_undo();
        self.drawing
            .items
            .extend(copies.into_iter().map(Item::Entity));
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
                let Item::Entity(entity) = previous.clone() else {
                    unreachable!("only live entities are selectable");
                };
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
                source_indexes.contains(&(index + 1)).then(|| match item {
                    Item::Entity(entity) => Some(entity.clone()),
                    Item::Block(_) | Item::Repeat(_) | Item::Erased(_) => None,
                })?
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
        self.save_undo();
        let copy_count = copies.len();
        self.drawing.items.extend(copies);
        self.refresh_after_edit();
        self.status = format!("Created {rows}x{columns} array with {copy_count} copies");
    }

    pub(crate) fn circular_array(
        &mut self,
        ids: &[usize],
        center: Point,
        angle: f64,
        count: usize,
    ) {
        let source_indexes = selected_item_indexes(&self.drawing, ids);
        let sources: Vec<Entity> = self
            .drawing
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                source_indexes.contains(&(index + 1)).then(|| match item {
                    Item::Entity(entity) => Some(entity.clone()),
                    Item::Block(_) | Item::Repeat(_) | Item::Erased(_) => None,
                })?
            })
            .collect();
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
                copies.push(Item::Entity(entity));
            }
        }
        self.save_undo();
        let copy_count = copies.len();
        self.drawing.items.extend(copies);
        self.refresh_after_edit();
        self.status = format!("Created circular array with {copy_count} copies");
    }

    pub(crate) fn change_layer(&mut self, ids: &[usize], layer: u8) {
        self.save_undo();
        let selected = selected_item_indexes(&self.drawing, ids);
        for (index, item) in self.drawing.items.iter_mut().enumerate() {
            if selected.contains(&(index + 1)) {
                if let Item::Entity(entity) = item {
                    assign_layer(entity, layer);
                }
            }
        }
        self.ensure_layer(layer);
        self.status = format!("Changed {} entities to layer {layer}", ids.len());
    }

    pub(crate) fn change_point(&mut self, ids: &[usize], point: Point) -> Result<(), String> {
        let selected = selected_item_indexes(&self.drawing, ids);
        for (index, item) in self.drawing.items.iter().enumerate() {
            if selected.contains(&(index + 1)) {
                let Item::Entity(entity) = item else {
                    return Err("CHANGE point mode only supports top-level entities".into());
                };
                if !can_change_point(entity) {
                    return Err(
                        "CHANGE point mode currently supports LINE, CIRCLE and INSERT".into(),
                    );
                }
            }
        }
        self.save_undo();
        for (index, item) in self.drawing.items.iter_mut().enumerate() {
            if selected.contains(&(index + 1)) {
                if let Item::Entity(entity) = item {
                    apply_change_point(entity, point);
                }
            }
        }
        self.refresh_after_edit();
        self.status = format!("Changed {} entities at the specified point", ids.len());
        Ok(())
    }

    pub(crate) fn fillet(&mut self, ids: &[usize], radius: f64) -> Result<(), String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
        let selected: Vec<_> = indexes.into_iter().collect();
        let get_line = |position: usize| -> Result<(Point, Point), String> {
            match self.drawing.items.get(position - 1) {
                Some(Item::Entity(entity)) => {
                    line_points(entity).ok_or_else(|| "FILLET only supports LINE entities".into())
                }
                _ => Err("FILLET selection is not a top-level entity".into()),
            }
        };
        let (first_start, first_end) = get_line(selected[0])?;
        let (second_start, second_end) = get_line(selected[1])?;
        let fillet = fillet_geometry((first_start, first_end), (second_start, second_end), radius)?;

        self.save_undo();
        for (index, line) in selected.iter().zip([fillet.first, fillet.second]) {
            if let Some(Item::Entity(entity)) = self.drawing.items.get_mut(index - 1) {
                set_line_points(entity, line.0, line.1);
            }
        }
        self.drawing.items.push(Item::Entity(Entity::OnLayer {
            layer: self.drawing.header.current_layer,
            entity: Box::new(Entity::Arc {
                center: fillet.center,
                radius,
                start_deg: fillet.start_deg,
                end_deg: fillet.end_deg,
            }),
        }));
        self.refresh_after_edit();
        self.status = "Filleted two lines".into();
        Ok(())
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
        let entity_kind = match bare(entity) {
            Entity::Line { .. } => "line",
            Entity::Circle { .. } => "circle",
            Entity::Arc { .. } => "arc",
            _ => return Err("BREAK supports LINE, ARC and CIRCLE entities".into()),
        };
        let geometry = break_entity_geometry(entity, first, second)?;
        let layer = entity_layer(entity);

        self.save_undo();
        if let Some(Item::Entity(entity)) = self.drawing.items.get_mut(item_index - 1) {
            *entity = Entity::OnLayer {
                layer,
                entity: Box::new(geometry.primary),
            };
        }
        if let Some(secondary) = geometry.secondary {
            self.drawing.items.push(Item::Entity(Entity::OnLayer {
                layer,
                entity: Box::new(secondary),
            }));
        }
        self.refresh_after_edit();
        self.status = format!("Broke {entity_kind}");
        Ok(())
    }

    pub(crate) fn measure_area(&mut self, ids: &[usize]) -> Result<(), String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
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
        pattern: &str,
        scale: f64,
        angle_deg: f64,
        ids: &[usize],
    ) -> Result<(), String> {
        if pattern != "LINE" {
            return Err(format!(
                "HATCH pattern {pattern} is listed but its geometry is not implemented"
            ));
        }
        let lines = hatch_line_geometry(&self.drawing, ids, scale, angle_deg)?;
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
            last_erased: self.last_erased.clone(),
            active_shape_library: self.active_shape_library.clone(),
        });
    }

    pub(crate) fn transform(&mut self, ids: &[usize], transform: Transform, copy: bool) {
        self.save_undo();
        let selected = selected_item_indexes(&self.drawing, ids);
        if copy {
            let mut copies = Vec::new();
            for (index, item) in self.drawing.items.iter().enumerate() {
                if selected.contains(&(index + 1)) {
                    if let Item::Entity(entity) = item {
                        let mut entity = entity.clone();
                        transform_entity(&mut entity, transform);
                        copies.push(Item::Entity(entity));
                    }
                }
            }
            self.drawing.items.extend(copies);
        } else {
            for (index, item) in self.drawing.items.iter_mut().enumerate() {
                if selected.contains(&(index + 1)) {
                    if let Item::Entity(entity) = item {
                        transform_entity(entity, transform);
                    }
                }
            }
        }
        self.refresh_after_edit();
        self.status = format!(
            "{} {} entities",
            if copy { "Copied" } else { "Transformed" },
            ids.len()
        );
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
