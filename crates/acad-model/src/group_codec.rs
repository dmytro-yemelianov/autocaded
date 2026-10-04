//! Shared native codec safety policy. Limits apply to stored records, never instances.
use crate::{Drawing, Entity, Item, Repeat};

pub const MAX_GROUP_DEPTH: usize = 64;
pub const MAX_STORED_RECORDS: usize = u16::MAX as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupCodecError {
    OwnerLayer,
    ErasedWrapper,
    ErasedRoot,
    AmbiguousOwner,
    LayerWrappers,
    InvalidRepeat,
    InvalidEntity,
    DepthLimit,
    RecordLimit,
}
impl GroupCodecError {
    pub fn message(self) -> &'static str {
        match self {
            Self::ErasedRoot => "erased member tag at drawing root must use Item::Erased",
            Self::ErasedWrapper => "erased member must wrap one ordinary record, without stacked erasure",
            Self::AmbiguousOwner => "whole erased owner contains already erased members; DWG cannot retain their prior status; use OOPS or UNDO before saving",
            Self::OwnerLayer => "explicit REPEAT owner layer has no lossless file mapping",
            Self::LayerWrappers => "multiple layer wrappers have no lossless file mapping",
            Self::InvalidEntity => "entity fields and block bases must be finite",
            Self::InvalidRepeat => "REPEAT needs members, nonzero dimensions and finite spacings",
            Self::DepthLimit => "group nesting exceeds the native codec limit of 64",
            Self::RecordLimit => "stored record count exceeds the native codec limit of 65535",
        }
    }
}

/// Validate ordinary fields after wrappers/structure have been handled by a bounded caller.
pub fn entity_fields_are_finite(entity: &Entity) -> bool {
    let point = |p: &crate::Point| p.x.is_finite() && p.y.is_finite();
    match entity {
        Entity::Point { origin } => point(origin),
        Entity::Line { start, end } => point(start) && point(end),
        Entity::Circle { center, radius } => point(center) && radius.is_finite(),
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            point(center)
                && [*radius, *start_deg, *end_deg]
                    .iter()
                    .all(|v| v.is_finite())
        }
        Entity::Text {
            origin,
            height,
            rotation_deg,
            ..
        }
        | Entity::Shape {
            origin,
            height,
            rotation_deg,
            ..
        } => point(origin) && height.is_finite() && rotation_deg.is_finite(),
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            ..
        } => {
            point(origin)
                && [*x_scale, *y_scale, *rotation_deg]
                    .iter()
                    .all(|v| v.is_finite())
        }
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            [p1, p2, p3, p4].into_iter().all(point)
        }
        Entity::Load { .. } => true,
        Entity::OnLayer { .. } | Entity::Repeat(_) | Entity::Erased(_) => false,
    }
}

fn repeat_valid(repeat: &Repeat) -> Result<(), GroupCodecError> {
    if repeat.entities.is_empty()
        || repeat.columns == 0
        || repeat.rows == 0
        || !repeat.column_spacing.is_finite()
        || !repeat.row_spacing.is_finite()
    {
        Err(GroupCodecError::InvalidRepeat)
    } else {
        Ok(())
    }
}
fn add(count: &mut usize, amount: usize) -> Result<(), GroupCodecError> {
    *count += amount;
    if *count > MAX_STORED_RECORDS {
        Err(GroupCodecError::RecordLimit)
    } else {
        Ok(())
    }
}

/// A bounded record-wrapper view shared by checked codecs. Metadata does not
/// turn a bare structural Repeat into an explicit runtime layer owner.
pub struct StoredRecord<'a> {
    pub entity: &'a Entity,
    pub layer: u8,
    pub has_layer: bool,
    pub erased: bool,
}
pub fn stored_record(mut entity: &Entity) -> Result<StoredRecord<'_>, GroupCodecError> {
    let mut layer = 1;
    let mut has_layer = false;
    let mut erased = false;
    loop {
        match entity {
            Entity::OnLayer {
                layer: value,
                entity: inner,
            } => {
                if has_layer {
                    return Err(GroupCodecError::LayerWrappers);
                }
                has_layer = true;
                layer = *value;
                entity = inner;
            }
            Entity::Erased(inner) => {
                if erased {
                    return Err(GroupCodecError::ErasedWrapper);
                }
                erased = true;
                entity = inner;
            }
            Entity::Repeat(_) if erased => return Err(GroupCodecError::ErasedWrapper),
            _ => {
                return Ok(StoredRecord {
                    entity,
                    layer,
                    has_layer,
                    erased,
                })
            }
        }
    }
}
#[derive(Clone, Copy)]
enum History {
    Preserve,
    LiveOnly,
}
fn records(
    entities: &[Entity],
    depth: usize,
    count: &mut usize,
    erased_owner: bool,
    history: History,
) -> Result<(), GroupCodecError> {
    let mut stack = vec![(entities.iter(), depth)];
    while let Some((entities, depth)) = stack.last_mut() {
        let Some(entity) = entities.next() else {
            stack.pop();
            continue;
        };
        let depth = *depth;
        let record = stored_record(entity)?;
        if erased_owner && record.erased && matches!(history, History::Preserve) {
            return Err(GroupCodecError::AmbiguousOwner);
        }
        if let Entity::Repeat(repeat) = record.entity {
            if record.has_layer {
                return Err(GroupCodecError::OwnerLayer);
            }
            repeat_valid(repeat)?;
            if depth >= MAX_GROUP_DEPTH {
                return Err(GroupCodecError::DepthLimit);
            }
            add(count, 2)?;
            stack.push((repeat.entities.iter(), depth + 1));
        } else {
            if !entity_fields_are_finite(record.entity) {
                return Err(GroupCodecError::InvalidEntity);
            }
            add(count, 1)?;
        }
    }
    Ok(())
}
/// Validate retained DWG history before output allocation. INSERTs are not expanded.
pub fn validate_group_records(drawing: &Drawing) -> Result<(), GroupCodecError> {
    validate_group_items(&drawing.items)
}
/// DXF deliberately omits whole erased owner history, including prior member status.
/// All fields, wrappers, structural shape and stored budgets remain checked.
pub fn validate_live_export_records(drawing: &Drawing) -> Result<(), GroupCodecError> {
    validate_items(&drawing.items, History::LiveOnly)
}
/// Borrowing preflight for DWG exports before cloning selected owners/definitions.
pub fn validate_group_items<'a>(
    items: impl IntoIterator<Item = &'a Item>,
) -> Result<(), GroupCodecError> {
    validate_items(items, History::Preserve)
}
fn validate_items<'a>(
    items: impl IntoIterator<Item = &'a Item>,
    history: History,
) -> Result<(), GroupCodecError> {
    let mut count = 0;
    for item in items {
        match item {
            Item::Entity(entity) => {
                if stored_record(entity)?.erased {
                    return Err(GroupCodecError::ErasedRoot);
                }
                records(std::slice::from_ref(entity), 0, &mut count, false, history)?
            }
            Item::Erased(entity) => {
                if stored_record(entity)?.erased {
                    return Err(GroupCodecError::ErasedWrapper);
                }
                records(std::slice::from_ref(entity), 0, &mut count, true, history)?
            }
            Item::Repeat(repeat) => {
                repeat_valid(repeat)?;
                add(&mut count, 2)?;
                records(&repeat.entities, 1, &mut count, false, history)?;
            }
            Item::Block(block) => {
                if !block.base.x.is_finite() || !block.base.y.is_finite() {
                    return Err(GroupCodecError::InvalidEntity);
                }
                add(&mut count, 2)?;
                records(&block.entities, 1, &mut count, false, history)?;
            }
        }
    }
    Ok(())
}
