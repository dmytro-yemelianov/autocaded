//! External drawing INSERT (docs/native-external-insert.md). The application
//! reads and decodes the file; this module closes over its definitions,
//! applies the conflict policy and stages the result in the prompt state, so
//! nothing changes until the final prompt commits it as one UNDO step.

use crate::entity_ops::load_library_name;
use crate::input_state::InputState;
use crate::star_insert::{place_items, Placement};
use crate::{Editor, Effect};
use acad_model::{Block, Drawing, Entity, Item, Point};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The block an INSERT prompt chain places, plus any staged file import.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InsertTarget {
    pub(crate) name: String,
    pub(crate) import: Option<Arc<Import>>,
    /// Native star-form scale/rotation (`S` at the insertion point).
    pub(crate) placement: Placement,
}

impl InsertTarget {
    pub(crate) fn existing(name: String) -> Self {
        Self {
            name,
            import: None,
            placement: Placement::default(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Import {
    /// Reachable source definitions absent from the host, in source order.
    definitions: Vec<Block>,
    base: Point,
    payload: Payload,
    layers: BTreeMap<u8, u8>,
    off_layers: BTreeSet<u8>,
    /// Live LOAD library names reachable from the imported records,
    /// including through reused host-identical definitions.
    loads: Vec<String>,
    /// Erased whole REPEAT owners the block form cannot store as members.
    omitted_erased_groups: usize,
}

#[derive(Debug, PartialEq)]
enum Payload {
    /// Members of the new definition named after the file.
    Block(Vec<Entity>),
    /// Root records appended after translation by insertion point minus base.
    Star(Vec<Item>),
}

/// `*` prefix and the file specification of an INSERT name answer.
fn split_request(input: &str) -> (bool, &str) {
    let line = input.trim();
    match line.strip_prefix('*') {
        Some(spec) => (true, spec.trim()),
        None => (false, line),
    }
}

/// File base name without drive, directories or extension, upper-cased.
fn block_name(spec: &str) -> String {
    let leaf = spec.rsplit([':', '/', '\\']).next().unwrap_or(spec);
    let stem = leaf.rsplit_once('.').map_or(leaf, |(stem, _)| stem);
    stem.to_ascii_uppercase()
}

fn references(entity: &Entity, out: &mut Vec<String>) {
    match entity {
        Entity::OnLayer { entity, .. } | Entity::Erased(entity) => references(entity, out),
        Entity::Insert { name, .. } => out.push(name.clone()),
        Entity::Repeat(repeat) => repeat.entities.iter().for_each(|e| references(e, out)),
        _ => {}
    }
}

fn item_references(item: &Item, out: &mut Vec<String>) {
    match item {
        Item::Entity(entity) | Item::Erased(entity) => references(entity, out),
        Item::Repeat(repeat) => repeat.entities.iter().for_each(|e| references(e, out)),
        Item::Block(_) => {}
    }
}

fn layers(entity: &Entity, out: &mut BTreeSet<u8>) {
    match entity {
        Entity::OnLayer { layer, entity } => {
            out.insert(*layer);
            if let Entity::Erased(inner) = entity.as_ref() {
                nested_layers(inner, out);
            } else {
                nested_layers(entity, out);
            }
        }
        Entity::Erased(inner) => layers(inner, out),
        Entity::Repeat(repeat) => {
            out.extend([repeat.start_layer, repeat.end_layer]);
            repeat.entities.iter().for_each(|e| layers(e, out));
        }
        // Unwrapped ordinary records are stored on the default layer.
        _ => {
            out.insert(1);
        }
    }
}

/// Below an explicit layer wrapper only structural members add layers.
fn nested_layers(entity: &Entity, out: &mut BTreeSet<u8>) {
    if let Entity::Repeat(repeat) = entity {
        out.extend([repeat.start_layer, repeat.end_layer]);
        repeat.entities.iter().for_each(|e| layers(e, out));
    }
}

/// Inverse of the root promotion: an erased ordinary root record becomes an
/// erased member. An erased whole REPEAT owner has no member form.
fn erased_member(entity: Entity) -> Option<Entity> {
    match entity {
        Entity::OnLayer { layer, entity } => match *entity {
            Entity::Repeat(_) => None,
            inner => Some(Entity::OnLayer {
                layer,
                entity: Box::new(Entity::Erased(Box::new(inner))),
            }),
        },
        Entity::Repeat(_) => None,
        inner => Some(Entity::Erased(Box::new(inner))),
    }
}

fn prepare(host: &Drawing, input: &str, source: Drawing) -> Result<(String, bool, Import), String> {
    let (explode, spec) = split_request(input);
    let name = block_name(spec);
    if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(format!(
            "INSERT: {spec} does not give a printable ASCII block name"
        ));
    }
    if !explode {
        if let Some(existing) = host
            .blocks()
            .find(|block| block.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!(
                "INSERT: block {} already exists; enter {} alone to insert it",
                existing.name, existing.name
            ));
        }
    }

    let Drawing { header, items } = source;
    let mut definitions = Vec::new();
    let mut roots = Vec::new();
    for item in items {
        match item {
            Item::Block(block) => definitions.push(block),
            other => roots.push(other),
        }
    }
    let mut omitted_erased_groups = 0;
    let payload = if explode {
        Payload::Star(roots)
    } else {
        Payload::Block(
            roots
                .into_iter()
                .filter_map(|item| match item {
                    Item::Entity(entity) => Some(entity),
                    Item::Repeat(repeat) => Some(Entity::Repeat(repeat)),
                    Item::Erased(entity) => {
                        let member = erased_member(entity);
                        omitted_erased_groups += usize::from(member.is_none());
                        member
                    }
                    Item::Block(_) => None,
                })
                .collect(),
        )
    };
    let mut pending = Vec::new();
    match &payload {
        Payload::Block(entities) if !entities.is_empty() => {
            entities.iter().for_each(|e| references(e, &mut pending))
        }
        Payload::Star(items) if !items.is_empty() => {
            items.iter().for_each(|i| item_references(i, &mut pending))
        }
        _ => return Err(format!("INSERT: {spec} contains no entities to insert")),
    }

    // Reachable definition closure by exact stored name.
    let mut reachable = BTreeSet::new();
    while let Some(reference) = pending.pop() {
        if !reachable.insert(reference.clone()) {
            continue;
        }
        let mut matches = definitions.iter().filter(|block| block.name == reference);
        let block = matches
            .next()
            .ok_or_else(|| format!("INSERT: {spec} references undefined block {reference}"))?;
        if matches.next().is_some() {
            return Err(format!(
                "INSERT: {spec} defines block {reference} more than once"
            ));
        }
        block
            .entities
            .iter()
            .for_each(|e| references(e, &mut pending));
    }
    let mut folded = BTreeMap::new();
    for reference in &reachable {
        if let Some(other) = folded.insert(reference.to_ascii_uppercase(), reference) {
            return Err(format!(
                "INSERT: {spec} defines block {reference} more than once (as {other})"
            ));
        }
        if !explode && reference.eq_ignore_ascii_case(&name) {
            return Err(format!(
                "INSERT: block {reference} in {spec} conflicts with the inserted block name {name}"
            ));
        }
    }
    definitions.retain(|block| reachable.contains(&block.name));
    reject_cycles(&definitions, spec)?;

    // LOADs of every reachable definition count, including reused ones.
    let mut reachable_loads = BTreeSet::new();
    for block in &definitions {
        block
            .entities
            .iter()
            .for_each(|e| live_loads(e, &mut reachable_loads));
    }

    // Host definitions are never redefined; an identical one is reused.
    let mut imported = Vec::new();
    for block in definitions {
        match host
            .blocks()
            .find(|existing| existing.name.eq_ignore_ascii_case(&block.name))
        {
            Some(existing) if *existing == block => {}
            Some(existing) if existing.name != block.name => {
                let same_content =
                    existing.base == block.base && existing.entities == block.entities;
                return Err(format!(
                    "INSERT: block {} in {spec} differs from existing block {} in name case{}; \
                     only an identically named and identical definition is reused",
                    block.name,
                    existing.name,
                    if same_content { "" } else { " and content" }
                ));
            }
            Some(existing) => {
                return Err(format!(
                    "INSERT: block {} already exists with different content than in {spec}",
                    existing.name
                ))
            }
            None => imported.push(block),
        }
    }

    let mut used = BTreeSet::new();
    for block in &imported {
        block.entities.iter().for_each(|e| layers(e, &mut used));
    }
    match &payload {
        Payload::Block(entities) => entities.iter().for_each(|e| layers(e, &mut used)),
        Payload::Star(items) => {
            for item in items {
                match item {
                    Item::Entity(entity) | Item::Erased(entity) => layers(entity, &mut used),
                    Item::Repeat(repeat) => layers(&Entity::Repeat(repeat.clone()), &mut used),
                    Item::Block(_) => {}
                }
            }
        }
    }
    let mut new_layers = BTreeMap::new();
    let mut off_layers = BTreeSet::new();
    for layer in used {
        if host.header.layers.contains_key(&layer) {
            continue;
        }
        if let Some(color) = header.layers.get(&layer) {
            new_layers.insert(layer, *color);
            if header.off_layers.contains(&layer) {
                off_layers.insert(layer);
            }
        }
    }

    let mut loads = reachable_loads;
    match &payload {
        Payload::Block(entities) => entities.iter().for_each(|e| live_loads(e, &mut loads)),
        Payload::Star(items) => {
            for item in items {
                match item {
                    Item::Entity(entity) => live_loads(entity, &mut loads),
                    Item::Repeat(repeat) => repeat
                        .entities
                        .iter()
                        .for_each(|e| live_loads(e, &mut loads)),
                    Item::Erased(_) | Item::Block(_) => {}
                }
            }
        }
    }

    let import = Import {
        definitions: imported,
        base: header.base,
        payload,
        layers: new_layers,
        off_layers,
        loads: loads.into_iter().collect(),
        omitted_erased_groups,
    };
    import.validate(host, &name)?;
    Ok((name, explode, import))
}

/// Stored LOAD names with live effect; erased records have none.
fn live_loads(entity: &Entity, out: &mut BTreeSet<String>) {
    match entity {
        Entity::OnLayer { entity, .. } => live_loads(entity, out),
        Entity::Load { name } => {
            out.insert(name.clone());
        }
        Entity::Repeat(repeat) => repeat.entities.iter().for_each(|e| live_loads(e, out)),
        _ => {}
    }
}

/// Iterative depth-first search over the closed definition graph.
fn reject_cycles(definitions: &[Block], spec: &str) -> Result<(), String> {
    let index: BTreeMap<&str, &Block> = definitions
        .iter()
        .map(|block| (block.name.as_str(), block))
        .collect();
    let mut done = BTreeSet::new();
    for start in definitions {
        if done.contains(start.name.as_str()) {
            continue;
        }
        let mut active = BTreeSet::from([start.name.as_str()]);
        let mut stack = vec![(start.name.as_str(), children(start))];
        while let Some((name, children)) = stack.last_mut() {
            let Some(child) = children.pop() else {
                active.remove(*name);
                done.insert(*name);
                stack.pop();
                continue;
            };
            let Some((child, block)) = index.get_key_value(child.as_str()) else {
                continue;
            };
            if active.contains(child) {
                return Err(format!(
                    "INSERT: {spec} has a cyclic block reference through {child}"
                ));
            }
            if done.contains(child) {
                continue;
            }
            active.insert(child);
            stack.push((child, self::children(block)));
        }
    }
    Ok(())
}

fn children(block: &Block) -> Vec<String> {
    let mut out = Vec::new();
    block.entities.iter().for_each(|e| references(e, &mut out));
    out
}

impl Import {
    fn new_items(&self, name: &str) -> Vec<Item> {
        let mut items: Vec<Item> = self.definitions.iter().cloned().map(Item::Block).collect();
        match &self.payload {
            Payload::Block(entities) => items.push(Item::Block(Block {
                name: name.to_owned(),
                base: self.base,
                entities: entities.clone(),
            })),
            Payload::Star(roots) => items.extend(roots.iter().cloned()),
        }
        items
    }

    /// Checked-codec preflight. The imported records must be valid on their
    /// own, and an import never makes a savable host unsavable: when the host
    /// passes the checks, host plus import (including the block form's INSERT)
    /// must too, so the whole-drawing record budget is respected.
    fn validate(&self, host: &Drawing, name: &str) -> Result<(), String> {
        use acad_model::group_codec::validate_group_items;
        if !self.base.x.is_finite() || !self.base.y.is_finite() {
            return Err("INSERT: drawing base point must be finite".into());
        }
        let mut items = self.new_items(name);
        if matches!(self.payload, Payload::Block(_)) {
            items.push(Item::Entity(Entity::OnLayer {
                layer: host.header.current_layer,
                entity: Box::new(Entity::Insert {
                    origin: Point { x: 0.0, y: 0.0 },
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: name.to_owned(),
                }),
            }));
        }
        let message = |error: acad_model::group_codec::GroupCodecError| {
            format!("INSERT: {}", error.message())
        };
        validate_group_items(&items).map_err(message)?;
        if validate_group_items(&host.items).is_ok() {
            validate_group_items(host.items.iter().chain(&items)).map_err(message)?;
        }
        Ok(())
    }
}

impl Editor {
    /// The drawing-file specification to resolve when the INSERT name prompt
    /// receives `input`: present only when it names no block of this drawing.
    pub fn insert_file_request(&self, input: &str) -> Option<String> {
        if !matches!(self.state, InputState::InsertName) {
            return None;
        }
        let (_, spec) = split_request(input);
        let known = self
            .drawing
            .blocks()
            .any(|block| block.name.eq_ignore_ascii_case(spec));
        (!spec.is_empty() && !known).then(|| spec.to_owned())
    }

    /// Answer the INSERT name prompt with a decoded drawing file. Validation
    /// failures keep the prompt; success stages the import without mutation.
    pub fn submit_insert_drawing(
        &mut self,
        input: &str,
        source: Drawing,
    ) -> Result<Effect, String> {
        if !matches!(self.state, InputState::InsertName) {
            return Err("INSERT is not waiting for a block or file name".into());
        }
        self.status.clear();
        let (name, explode, import) = prepare(&self.drawing, input, source)?;
        self.status = format!("Read drawing {name}");
        self.state = InputState::InsertOrigin(
            InsertTarget {
                name,
                import: Some(Arc::new(import)),
                placement: Placement::default(),
            },
            explode,
        );
        Ok(Effect::Continue)
    }

    fn add_imported_definitions(&mut self, import: &Import) {
        for (layer, color) in &import.layers {
            self.drawing.header.layers.insert(*layer, *color);
        }
        self.drawing
            .header
            .off_layers
            .extend(import.off_layers.iter().copied());
        self.drawing
            .items
            .extend(import.definitions.iter().cloned().map(Item::Block));
    }

    /// Block form: definitions, the file's block and its INSERT in one step.
    pub(crate) fn commit_external_block(&mut self, name: String, import: &Import, insert: Entity) {
        let Payload::Block(_) = &import.payload else {
            unreachable!("star imports commit at the insertion point")
        };
        self.save_undo();
        self.add_imported_definitions(import);
        self.drawing.items.extend(
            import
                .new_items(&name)
                .into_iter()
                .skip(import.definitions.len()),
        );
        self.drawing.items.push(Item::Entity(Entity::OnLayer {
            layer: self.drawing.header.current_layer,
            entity: Box::new(insert),
        }));
        self.refresh_after_edit();
        self.committed_import_loads = Some(import.loads.clone());
        self.status = match import.omitted_erased_groups {
            0 => format!("Inserted drawing {name}"),
            count => format!(
                "Inserted drawing {name}; {count} erased REPEAT group(s) omitted, use *{name} to keep them"
            ),
        };
    }

    /// Star form: definitions plus the placed root records in one step.
    /// Unrepresentable placements are refused before any change.
    pub(crate) fn commit_external_star(
        &mut self,
        target: &InsertTarget,
        import: &Import,
        origin: Point,
    ) -> Result<(), String> {
        let Payload::Star(roots) = &import.payload else {
            unreachable!("block imports commit at the rotation prompt")
        };
        let placed = place_items(roots, import.base, origin, target.placement)?;
        self.save_undo();
        self.add_imported_definitions(import);
        for item in placed {
            if let Item::Entity(entity) = &item {
                if let Some(library) = load_library_name(entity) {
                    self.active_shape_library = Some(library);
                }
            }
            self.drawing.items.push(item);
        }
        self.refresh_after_edit();
        self.committed_import_loads = Some(import.loads.clone());
        self.status = format!("Inserted drawing {} as separate entities", target.name);
        Ok(())
    }

    /// Representability of a staged star import under `placement`.
    pub(crate) fn check_external_star(import: &Import, placement: Placement) -> Result<(), String> {
        let Payload::Star(roots) = &import.payload else {
            return Ok(());
        };
        place_items(roots, import.base, import.base, placement).map(|_| ())
    }

    /// LOAD library names of the file import committed since the last call.
    pub fn take_committed_import_loads(&mut self) -> Option<Vec<String>> {
        self.committed_import_loads.take()
    }
}
