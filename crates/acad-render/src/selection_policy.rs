//! Preflight owner work before any generated repeat cells allocate.
use acad_model::{BlockIndex, Entity, Item, Repeat};

const MAX_GENERATED_RECORDS: usize = 100_000;
const MAX_STORED_DEPTH: usize = 256;

/// Whether the owner fits the per-owner budget, plus the stored records the
/// preflight visited (charged to the whole-frame budget by the caller).
pub(crate) fn bounded_owner(blocks: &BlockIndex<'_>, item: &Item) -> (bool, usize) {
    let mut budget = Budget {
        remaining: MAX_GENERATED_RECORDS,
        visits: 0,
    };
    let bounded = match item {
        Item::Entity(entity) => entity_cost(Some(blocks), entity, 1, &mut budget, 0, 0),
        Item::Repeat(repeat) => repeat_cost(Some(blocks), repeat, 1, &mut budget, 0, 0),
        _ => true,
    };
    (bounded, budget.visits)
}
pub(crate) fn bounded_geometry(entity: &Entity) -> bool {
    let mut budget = Budget {
        remaining: MAX_GENERATED_RECORDS,
        visits: 0,
    };
    entity_cost(None, entity, 1, &mut budget, 0, 0)
}
struct Budget {
    remaining: usize,
    visits: usize,
}
fn spend(amount: usize, budget: &mut Budget) -> bool {
    if amount > budget.remaining {
        return false;
    }
    budget.remaining -= amount;
    true
}
fn entity_cost(
    blocks: Option<&BlockIndex<'_>>,
    entity: &Entity,
    copies: usize,
    budget: &mut Budget,
    depth: usize,
    inserts: usize,
) -> bool {
    budget.visits += 1;
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
            entity_cost(blocks, entity, copies, budget, depth + 1, inserts)
        }
        Entity::Repeat(repeat) => repeat_cost(blocks, repeat, copies, budget, depth + 1, inserts),
        Entity::Insert { name, .. } => {
            if inserts >= 16 {
                return false;
            }
            // Long names cost hashing work before the lookup (RB2).
            let name_cost = crate::budget::name_units(name);
            if !spend(name_cost, budget) {
                return false;
            }
            budget.visits += name_cost;
            blocks
                .and_then(|blocks| blocks.get(name))
                .is_none_or(|block| {
                    block.entities.iter().all(|child| {
                        entity_cost(blocks, child, copies, budget, depth + 1, inserts + 1)
                    })
                })
        }
        _ => true,
    }
}
fn repeat_cost(
    blocks: Option<&BlockIndex<'_>>,
    repeat: &Repeat,
    copies: usize,
    budget: &mut Budget,
    depth: usize,
    inserts: usize,
) -> bool {
    budget.visits += 1;
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
        .all(|child| entity_cost(blocks, child, generated, budget, depth + 1, inserts))
}
