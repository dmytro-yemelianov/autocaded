//! Bounded projection of ordered library effects, without generated geometry.
use acad_model::{Drawing, EngineLimits, Entity, Item};

pub(crate) struct Context<'a> {
    drawing: &'a Drawing,
    remaining: usize,
    inserts: Vec<String>,
}
impl<'a> Context<'a> {
    pub(crate) fn new(drawing: &'a Drawing) -> Self {
        Self {
            drawing,
            remaining: EngineLimits::DEFAULT_1983.max_traversal_visits,
            inserts: Vec::new(),
        }
    }
    pub(crate) fn project(&mut self, item: &Item) -> Result<Vec<Item>, String> {
        if self.remaining == 0 {
            return Err("WBLOCK: LOAD context exceeds the stored traversal budget".into());
        }
        self.remaining -= 1;
        let mut loads = Vec::new();
        match item {
            Item::Entity(entity) => self.entity(entity, 0, None, &mut loads)?,
            Item::Repeat(repeat) => {
                for entity in &repeat.entities {
                    self.entity(entity, 1, None, &mut loads)?;
                }
            }
            Item::Block(_) | Item::Erased(_) => {}
        }
        Ok(loads.into_iter().map(Item::Entity).collect())
    }
    fn entity(
        &mut self,
        entity: &Entity,
        depth: usize,
        layer: Option<u8>,
        out: &mut Vec<Entity>,
    ) -> Result<(), String> {
        if depth > EngineLimits::DEFAULT_1983.max_stored_depth || self.remaining == 0 {
            return Err("WBLOCK: LOAD context exceeds the stored traversal budget".into());
        }
        self.remaining -= 1;
        match entity {
            // Erased stored members keep their payload for persistence only: never
            // unwrap them into ordered LOAD/INSERT context.
            Entity::Erased(_) => {}
            Entity::OnLayer { layer, entity } => {
                self.entity(entity, depth + 1, Some(*layer), out)?
            }
            Entity::Load { .. } => out.push(match layer {
                Some(layer) => Entity::OnLayer {
                    layer,
                    entity: Box::new(entity.clone()),
                },
                None => entity.clone(),
            }),
            // The resource walker executes the stored body once, then copies its
            // geometry. Project that same order, even when the owner is hidden.
            Entity::Repeat(repeat) => {
                for child in &repeat.entities {
                    self.entity(child, depth + 1, None, out)?;
                }
            }
            Entity::Insert { name, .. } => {
                if self.inserts.len() >= EngineLimits::DEFAULT_1983.max_insert_depth
                    || self
                        .inserts
                        .iter()
                        .any(|active| active.eq_ignore_ascii_case(name))
                {
                    return Err(format!(
                        "WBLOCK: unresolved cyclic/deep LOAD context in INSERT {name}"
                    ));
                }
                let block = self.drawing.block(name).ok_or_else(|| {
                    format!("WBLOCK: unresolved LOAD context in missing INSERT {name}")
                })?;
                self.inserts.push(name.clone());
                for child in &block.entities {
                    self.entity(child, depth + 1, None, out)?;
                }
                self.inserts.pop();
            }
            _ => {}
        }
        Ok(())
    }
}
