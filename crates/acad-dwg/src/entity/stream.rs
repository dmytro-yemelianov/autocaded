//! Assemble independent physical records into bounded logical groups.
use super::record::{read_record_body, RecordBody};
use crate::{header::HeaderMeta, DwgError};
use acad_model::{
    group_codec::{MAX_GROUP_DEPTH, MAX_STORED_RECORDS},
    Block, Entity, Item, Repeat,
};

fn region<'a>(bytes: &'a [u8], meta: &HeaderMeta) -> Result<&'a [u8], DwgError> {
    let end = meta.entity_end as usize;
    if meta.entity_count as usize > MAX_STORED_RECORDS {
        return Err(DwgError::ReadLimit { at: end });
    }
    if end > bytes.len() {
        return Err(DwgError::TruncatedEntity {
            index: 0,
            at: end,
            len: bytes.len(),
        });
    }
    Ok(bytes)
}
fn finished(pos: usize, index: u32, meta: &HeaderMeta) -> Result<(), DwgError> {
    if pos != meta.entity_end as usize {
        return Err(DwgError::WalkOverran {
            pos,
            entity_end: meta.entity_end as usize,
        });
    }
    if index != meta.entity_count {
        return Err(DwgError::EntityCountMismatch {
            want: meta.entity_count,
            got: index,
        });
    }
    Ok(())
}
/// Geometry-only convenience view in physical record order, without layer metadata.
/// Use read_items/parse for drawing structure and layers.
pub fn read_entities(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<Entity>, DwgError> {
    let bytes = region(bytes, meta)?;
    let mut pos = meta.version.entity_start();
    let mut index = 0;
    let mut out = Vec::new();
    let mut signed_group = false;
    while pos < meta.entity_end as usize {
        if index as usize >= MAX_STORED_RECORDS {
            return Err(DwgError::ReadLimit { at: pos });
        }
        let (body, next, logical) = read_record_body(bytes, pos, index, meta.version)?;
        if next > meta.entity_end as usize {
            return Err(DwgError::WalkOverran {
                pos: next,
                entity_end: meta.entity_end as usize,
            });
        }
        signed_group |= matches!(
            body,
            RecordBody::RepeatStart { erased: true, .. }
                | RecordBody::RepeatEnd { erased: true, .. }
        );
        if let RecordBody::Entity(entity) = body {
            out.push(match entity {
                Entity::OnLayer { entity, .. } => *entity,
                other => other,
            });
        }
        pos = next;
        index += logical;
    }
    finished(pos, index, meta)?;
    // Preserve this legacy geometry-only view for live streams, but never let a
    // malformed signed group expose positive members while erasing its owner.
    if signed_group {
        read_items(bytes, meta)?;
    }
    Ok(out)
}
struct OpenRepeat {
    erased: bool,
    at: usize,
    layer: u8,
    entities: Vec<Entity>,
}
fn push_entity(
    entity: Entity,
    repeats: &mut [OpenRepeat],
    blocks: &mut [(usize, Block)],
    out: &mut Vec<Item>,
) {
    if let Some(repeat) = repeats.last_mut() {
        repeat.entities.push(entity);
    } else if let Some((_, block)) = blocks.last_mut() {
        block.entities.push(entity);
    } else {
        out.push(Item::Entity(entity));
    }
}
/// Preserve ordered items, nested patterns and flat sibling block definitions.
/// BLOCKs nested in BLOCKs close into the global table as established by SELEXOL.
/// REPEATs cannot cross BLOCK/ENDBLK boundaries; unsupported erasure is never dropped.
pub fn read_items(bytes: &[u8], meta: &HeaderMeta) -> Result<Vec<Item>, DwgError> {
    let bytes = region(bytes, meta)?;
    let mut pos = meta.version.entity_start();
    let mut index = 0;
    let mut out = Vec::new();
    let mut blocks: Vec<(usize, Block)> = Vec::new();
    let mut repeats: Vec<OpenRepeat> = Vec::new();
    while pos < meta.entity_end as usize {
        if index as usize >= MAX_STORED_RECORDS {
            return Err(DwgError::ReadLimit { at: pos });
        }
        let (body, next, logical) = read_record_body(bytes, pos, index, meta.version)?;
        if next > meta.entity_end as usize {
            return Err(DwgError::WalkOverran {
                pos: next,
                entity_end: meta.entity_end as usize,
            });
        }
        match body {
            RecordBody::Entity(e) => {
                if repeats.last().is_some_and(|open| open.erased) {
                    return Err(DwgError::InvalidGroupStream {
                        at: pos,
                        reason: "mixed signs in erased REPEAT owner",
                    });
                }
                push_entity(e, &mut repeats, &mut blocks, &mut out);
            }
            RecordBody::ErasedEntity(e) => {
                if repeats.last().is_some_and(|open| open.erased) {
                    push_entity(e, &mut repeats, &mut blocks, &mut out);
                } else if !blocks.is_empty() || !repeats.is_empty() {
                    push_entity(
                        Entity::Erased(Box::new(e)),
                        &mut repeats,
                        &mut blocks,
                        &mut out,
                    );
                } else {
                    out.push(Item::Erased(e));
                }
            }
            RecordBody::RepeatStart { layer, erased } => {
                if repeats.last().is_some_and(|open| open.erased != erased)
                    || (erased && !blocks.is_empty())
                {
                    return Err(DwgError::InvalidGroupStream {
                        at: pos,
                        reason: "mixed signs or erased subgroup without a top-level erased owner",
                    });
                }
                if blocks.len() + repeats.len() >= MAX_GROUP_DEPTH {
                    return Err(DwgError::ReadLimit { at: pos });
                }
                repeats.push(OpenRepeat {
                    erased,
                    at: pos,
                    layer,
                    entities: Vec::new(),
                });
            }
            RecordBody::RepeatEnd {
                erased,
                layer,
                columns,
                rows,
                column_spacing,
                row_spacing,
            } => {
                let open = repeats.pop().ok_or(DwgError::StrayEndrep { at: pos })?;
                if erased != open.erased {
                    return Err(DwgError::InvalidGroupStream {
                        at: pos,
                        reason: "mixed signs in erased REPEAT owner",
                    });
                }
                if open.entities.is_empty()
                    || columns == 0
                    || rows == 0
                    || !column_spacing.is_finite()
                    || !row_spacing.is_finite()
                {
                    return Err(DwgError::InvalidGroupStream {
                        at: pos,
                        reason: "REPEAT needs members, nonzero dimensions and finite spacings",
                    });
                }
                let repeat = Repeat {
                    start_layer: open.layer,
                    end_layer: layer,
                    entities: open.entities,
                    columns,
                    rows,
                    column_spacing,
                    row_spacing,
                };
                if blocks.is_empty() && repeats.is_empty() {
                    out.push(if open.erased {
                        Item::Erased(Entity::Repeat(repeat))
                    } else {
                        Item::Repeat(repeat)
                    });
                } else {
                    push_entity(Entity::Repeat(repeat), &mut repeats, &mut blocks, &mut out);
                }
            }
            RecordBody::BlockStart { name, base } => {
                if !repeats.is_empty() {
                    return Err(DwgError::InvalidGroupStream {
                        at: pos,
                        reason: "BLOCK crosses an open REPEAT",
                    });
                }
                if blocks.len() >= MAX_GROUP_DEPTH {
                    return Err(DwgError::ReadLimit { at: pos });
                }
                blocks.push((
                    pos,
                    Block {
                        name,
                        base,
                        entities: Vec::new(),
                    },
                ));
            }
            RecordBody::BlockEnd => {
                if !repeats.is_empty() {
                    return Err(DwgError::InvalidGroupStream {
                        at: pos,
                        reason: "ENDBLK crosses an open REPEAT",
                    });
                }
                let (_, block) = blocks.pop().ok_or(DwgError::StrayEndblk { at: pos })?;
                out.push(Item::Block(block));
            }
        }
        pos = next;
        index += logical;
    }
    if pos != meta.entity_end as usize {
        finished(pos, index, meta)?;
    }
    if let Some((at, block)) = blocks.pop() {
        return Err(DwgError::UnterminatedBlock {
            name: block.name,
            at,
        });
    }
    if let Some(open) = repeats.pop() {
        return Err(DwgError::UnterminatedRepeat { at: open.at });
    }
    finished(pos, index, meta)?;
    Ok(out)
}
