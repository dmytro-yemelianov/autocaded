//! Preflight owner work before any generated repeat cells allocate.
use acad_model::{Drawing, Entity, Item, Repeat};

const MAX_GENERATED_RECORDS: usize = 100_000;
const MAX_STORED_DEPTH: usize = 256;

pub(crate) fn bounded_owner(drawing: &Drawing, item: &Item) -> bool {
    let mut budget = MAX_GENERATED_RECORDS;
    match item {
        Item::Entity(entity) => entity_cost(Some(drawing), entity, 1, &mut budget, 0, 0),
        Item::Repeat(repeat) => repeat_cost(Some(drawing), repeat, 1, &mut budget, 0, 0),
        _ => true,
    }
}
pub(crate) fn bounded_geometry(entity: &Entity) -> bool {
    let mut budget = MAX_GENERATED_RECORDS;
    entity_cost(None, entity, 1, &mut budget, 0, 0)
}
fn spend(amount: usize, budget: &mut usize) -> bool {
    if amount > *budget {
        return false;
    }
    *budget -= amount;
    true
}
fn entity_cost(
    drawing: Option<&Drawing>,
    entity: &Entity,
    copies: usize,
    budget: &mut usize,
    depth: usize,
    inserts: usize,
) -> bool {
    if depth > MAX_STORED_DEPTH {
        return false;
    }
    if matches!(entity, Entity::Erased(_)) {
        return spend(1, budget);
    }
    if !spend(copies.max(1), budget) {
        return false;
    }
    match entity {
        Entity::OnLayer { entity, .. } => {
            entity_cost(drawing, entity, copies, budget, depth + 1, inserts)
        }
        Entity::Repeat(repeat) => repeat_cost(drawing, repeat, copies, budget, depth + 1, inserts),
        Entity::Insert { name, .. } => {
            if inserts >= 16 {
                return false;
            }
            drawing
                .and_then(|drawing| drawing.block(name))
                .is_none_or(|block| {
                    block.entities.iter().all(|child| {
                        entity_cost(drawing, child, copies, budget, depth + 1, inserts + 1)
                    })
                })
        }
        _ => true,
    }
}
fn repeat_cost(
    drawing: Option<&Drawing>,
    repeat: &Repeat,
    copies: usize,
    budget: &mut usize,
    depth: usize,
    inserts: usize,
) -> bool {
    // Children are rendered into a base before the cell loop. A zero-sized
    // outer lattice must never hide the work needed by an inner lattice.
    if repeat.rows == 0 || repeat.columns == 0 {
        return false;
    }
    let cells = usize::from(repeat.rows).saturating_mul(usize::from(repeat.columns));
    let generated = copies.saturating_mul(cells);
    if depth > MAX_STORED_DEPTH || !spend(generated.max(1), budget) {
        return false;
    }
    repeat
        .entities
        .iter()
        .all(|child| entity_cost(drawing, child, generated, budget, depth + 1, inserts))
}
