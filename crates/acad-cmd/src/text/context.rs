//! Standalone TXT eligibility. Iterator frames retain O(stored/INSERT depth)
//! pending state even for branching cyclic blocks; no sibling work is copied.
use acad_model::{Block, Drawing, EngineLimits, Entity, Item};

const MAX_VISITS: usize = EngineLimits::DEFAULT_1983.max_traversal_visits;
const MAX_STORED_DEPTH: usize = EngineLimits::DEFAULT_1983.max_stored_depth;
const MAX_INSERT_DEPTH: usize = EngineLimits::DEFAULT_1983.max_insert_depth;
struct Frame<'a> {
    entities: std::slice::Iter<'a, Entity>,
    stored_depth: usize,
    insert_depth: usize,
    block: Option<&'a Block>,
}

pub(super) fn startup_txt_context(drawing: &Drawing, before: usize) -> Result<(), String> {
    if before > drawing.items.len() {
        return Err("TEXT font position is outside the drawing".into());
    }
    let mut remaining = MAX_VISITS;
    let mut frames = Vec::new();
    for item in drawing.items.iter().take(before) {
        let entities = match item {
            Item::Entity(entity) => std::slice::from_ref(entity),
            Item::Repeat(repeat) => &repeat.entities,
            Item::Block(_) | Item::Erased(_) => continue,
        };
        frames.push(Frame {
            entities: entities.iter(),
            stored_depth: 0,
            insert_depth: 0,
            block: None,
        });
        while let Some(frame) = frames.last_mut() {
            let Some(entity) = frame.entities.next() else {
                frames.pop();
                continue;
            };
            let stored_depth = frame.stored_depth;
            let insert_depth = frame.insert_depth;
            if remaining == 0 || stored_depth > MAX_STORED_DEPTH {
                return Err("TEXT font context exceeds stored traversal budget".into());
            }
            remaining -= 1;
            let (children, block, insert_depth) = match entity {
                // Erased members have no library effect on startup TXT metrics.
                Entity::Erased(_) => continue,
                Entity::Load { .. } => {
                    return Err(
                        "TEXT metrics require an application font provider after LOAD".into(),
                    )
                }
                Entity::OnLayer { entity, .. } => {
                    (std::slice::from_ref(entity.as_ref()), None, insert_depth)
                }
                Entity::Repeat(repeat) => (repeat.entities.as_slice(), None, insert_depth),
                Entity::Insert { name, .. } => {
                    if insert_depth >= MAX_INSERT_DEPTH {
                        return Err("TEXT font context: block recursion limit reached".into());
                    }
                    let block = drawing
                        .block(name)
                        .ok_or_else(|| format!("TEXT font context: unknown block {name}"))?;
                    if frames.iter().any(|frame| {
                        frame
                            .block
                            .is_some_and(|ancestor| std::ptr::eq(ancestor, block))
                    }) {
                        return Err("TEXT font context: cyclic INSERT reference".into());
                    }
                    (block.entities.as_slice(), Some(block), insert_depth + 1)
                }
                _ => continue,
            };
            frames.push(Frame {
                entities: children.iter(),
                stored_depth: stored_depth + 1,
                insert_depth,
                block,
            });
        }
    }
    Ok(())
}
