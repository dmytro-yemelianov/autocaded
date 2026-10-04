//! Selection and picking helpers: window selection, selection-set parsing, and pick distance.

use acad_model::{Drawing, Entity, Item, Point};
use std::collections::BTreeSet;

use crate::entity_ops::bare;
pub(crate) mod dialogue;
pub(crate) mod visible_bounds;

/// A live top-level object. REPEAT is selected as a whole group; its members
/// and generated instances do not receive independent editor IDs.
#[derive(Clone, Copy)]
pub struct SelectableItem<'a> {
    pub id: usize,
    pub item_index: usize,
    pub item: &'a Item,
}

/// Canonical editor numbering in drawing order, excluding definitions,
/// erased records and LOAD metadata. IDs are recomputed after edits.
pub fn selectable_items(drawing: &Drawing) -> impl Iterator<Item = SelectableItem<'_>> {
    drawing
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| match item {
            Item::Entity(entity) => {
                !entity.is_erased() && !matches!(bare(entity), Entity::Load { .. })
            }
            Item::Repeat(_) => true,
            Item::Block(_) | Item::Erased(_) => false,
        })
        .enumerate()
        .map(|(index, (item_index, item))| SelectableItem {
            id: index + 1,
            item_index,
            item,
        })
}

pub(crate) fn item_entity(item: &Item) -> Option<Entity> {
    match item {
        Item::Entity(entity) => Some(entity.clone()),
        Item::Repeat(repeat) => Some(Entity::Repeat(repeat.clone())),
        Item::Block(_) | Item::Erased(_) => None,
    }
}

/// Whole-drawing hit-test budget (RB2, `docs/native-render-budget.md`): one
/// pick or window spends at most this many stored-record visits across all
/// owners, on top of the unchanged per-owner limits. When it runs out the
/// whole pick/window fails; it never returns a hit from a partial scan.
pub(crate) const MAX_DRAWING_HIT_TEST_VISITS: usize = 1_000_000;
pub(crate) const HIT_TEST_BUDGET_EXCEEDED: &str =
    "selection exceeds the drawing hit-test work budget; select objects by number";

/// Spend one owner's work (at least one visit) from the drawing budget.
pub(crate) fn charge_hit_test(remaining: &mut usize, used: usize) -> Result<(), String> {
    let used = used.max(1);
    if used > *remaining {
        return Err(HIT_TEST_BUDGET_EXCEEDED.into());
    }
    *remaining -= used;
    Ok(())
}

#[cfg(test)]
pub(crate) fn entities_in_window(drawing: &Drawing, first: Point, second: Point) -> Vec<usize> {
    checked_entities_in_window(drawing, first, second).expect("window within hit-test budget")
}

pub(crate) fn checked_entities_in_window(
    drawing: &Drawing,
    first: Point,
    second: Point,
) -> Result<Vec<usize>, String> {
    let mut remaining = MAX_DRAWING_HIT_TEST_VISITS;
    let min = Point {
        x: first.x.min(second.x),
        y: first.y.min(second.y),
    };
    let max = Point {
        x: first.x.max(second.x),
        y: first.y.max(second.y),
    };
    let mut ids = Vec::new();
    let scene = visible_bounds::Scene::new(drawing);
    for selected in selectable_items(drawing) {
        let (bounds, used) = visible_bounds::item_bounds(&scene, selected.item);
        charge_hit_test(&mut remaining, used)?;
        let Some(points) = bounds else {
            continue;
        };
        if points.iter().all(|point| {
            point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
        }) {
            ids.push(selected.id);
        }
    }
    Ok(ids)
}

pub(crate) fn selection_extents_points(entity: &Entity) -> Option<Vec<Point>> {
    Some(match bare(entity) {
        Entity::Line { start, end } => vec![*start, *end],
        Entity::Circle { center, radius } | Entity::Arc { center, radius, .. } => vec![
            Point {
                x: center.x - radius,
                y: center.y - radius,
            },
            Point {
                x: center.x + radius,
                y: center.y + radius,
            },
        ],
        Entity::Point { origin } | Entity::Text { origin, .. } | Entity::Shape { origin, .. } => {
            vec![*origin]
        }
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            vec![*p1, *p2, *p3, *p4]
        }
        Entity::Repeat(repeat) => return repeat_extents_points(repeat),
        Entity::Insert { .. }
        | Entity::Load { .. }
        | Entity::OnLayer { .. }
        | Entity::Erased(_) => return None,
    })
}

fn repeat_extents_points(repeat: &acad_model::Repeat) -> Option<Vec<Point>> {
    crate::geometry::repeat_bounds_points(repeat, true).map(Vec::from)
}

pub(crate) fn selectable_count(drawing: &Drawing) -> usize {
    selectable_items(drawing).count()
}

pub(crate) fn selection(input: &str, count: usize) -> Result<Vec<usize>, String> {
    if input.eq_ignore_ascii_case("ALL") {
        if count == 0 {
            return Err("there are no selectable entities".into());
        }
        return Ok((1..=count).collect());
    }
    if input.eq_ignore_ascii_case("LAST") || input.eq_ignore_ascii_case("L") {
        return if count == 0 {
            Err("there are no selectable entities".into())
        } else {
            Ok(vec![count])
        };
    }
    let mut ids = BTreeSet::new();
    for part in input.split(',') {
        let id = part
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("invalid entity number: {}", part.trim()))?;
        if id == 0 || id > count {
            return Err(format!("entity number {id} is out of range (1..={count})"));
        }
        ids.insert(id);
    }
    if ids.is_empty() {
        return Err("select at least one entity".into());
    }
    Ok(ids.into_iter().collect())
}

pub(crate) fn selected_item_indexes(drawing: &Drawing, ids: &[usize]) -> BTreeSet<usize> {
    let wanted: BTreeSet<_> = ids.iter().copied().collect();
    selectable_items(drawing)
        .filter(|selected| wanted.contains(&selected.id))
        .map(|selected| selected.item_index + 1)
        .collect()
}

// Bound repeat picking without expanding stored geometry. Oversized groups
// remain available by ID; we never return a hit from a partially scanned group.
const MAX_PICK_MEMBERS: usize = 100_000;

/// One owner's pick distance (`None` when missed, hidden or over the
/// per-owner budget), plus the visits spent.
pub(crate) fn item_pick_distance(
    scene: &visible_bounds::Scene<'_>,
    point: Point,
    item: &Item,
) -> (Option<f64>, usize) {
    let mut remaining = MAX_PICK_MEMBERS;
    let distance = match item {
        Item::Entity(entity) => bounded_pick_distance(scene, point, entity, &mut remaining, 0)
            .ok()
            .flatten(),
        Item::Repeat(repeat) => repeat_pick_distance(scene, point, repeat, &mut remaining, 0)
            .ok()
            .flatten(),
        _ => None,
    };
    (distance, MAX_PICK_MEMBERS - remaining)
}

fn bounded_pick_distance(
    scene: &visible_bounds::Scene<'_>,
    point: Point,
    entity: &Entity,
    remaining: &mut usize,
    depth: usize,
) -> Result<Option<f64>, ()> {
    if depth > 32 || *remaining == 0 {
        return Err(());
    }
    *remaining -= 1;
    // Layer wrappers are walked by the visibility gate: charge them first.
    let mut leaf = entity;
    while let Entity::OnLayer { entity: child, .. } = leaf {
        if *remaining == 0 {
            return Err(());
        }
        *remaining -= 1;
        leaf = child;
    }
    if !visible_bounds::visible_record(scene.drawing, entity) {
        return Ok(None);
    }
    match bare(entity) {
        Entity::Repeat(repeat) => repeat_pick_distance(scene, point, repeat, remaining, depth + 1),
        Entity::Insert { .. } => {
            if visible_bounds::entity_bounds(scene, entity, remaining, depth, 0)?.is_some() {
                Ok(entity_pick_distance(point, entity))
            } else {
                Ok(None)
            }
        }
        _ => Ok(entity_pick_distance(point, entity)),
    }
}

fn repeat_pick_distance(
    scene: &visible_bounds::Scene<'_>,
    point: Point,
    repeat: &acad_model::Repeat,
    remaining: &mut usize,
    depth: usize,
) -> Result<Option<f64>, ()> {
    let mut nearest: Option<f64> = None;
    if repeat.entities.is_empty() || repeat.rows == 0 || repeat.columns == 0 {
        return Ok(None);
    }
    let cells = usize::from(repeat.rows) * usize::from(repeat.columns);
    if depth > 32 || cells.saturating_mul(repeat.entities.len()) > *remaining {
        return Err(());
    }
    for row in 0..repeat.rows {
        for column in 0..repeat.columns {
            let local = Point {
                x: point.x - f64::from(column) * repeat.column_spacing,
                y: point.y - f64::from(row) * repeat.row_spacing,
            };
            for entity in &repeat.entities {
                if let Some(distance) =
                    bounded_pick_distance(scene, local, entity, remaining, depth)?
                {
                    nearest = Some(nearest.map_or(distance, |old| old.min(distance)));
                }
            }
        }
    }
    Ok(nearest)
}

pub(crate) fn entity_pick_distance(point: Point, entity: &Entity) -> Option<f64> {
    let distance = |a: Point, b: Point| (a.x - b.x).hypot(a.y - b.y);
    let segment_distance = |start: Point, end: Point| {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx * dx + dy * dy;
        if length_squared == 0.0 {
            return distance(point, start);
        }
        let along = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared)
            .clamp(0.0, 1.0);
        distance(
            point,
            Point {
                x: start.x + along * dx,
                y: start.y + along * dy,
            },
        )
    };
    match bare(entity) {
        Entity::Line { start, end } => Some(segment_distance(*start, *end)),
        Entity::Circle { center, radius } => Some((distance(point, *center) - radius).abs()),
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            let radial = distance(point, *center);
            let angle = (point.y - center.y)
                .atan2(point.x - center.x)
                .to_degrees()
                .rem_euclid(360.0);
            let sweep = end_deg - start_deg;
            let offset = (angle - start_deg).rem_euclid(360.0);
            if sweep.abs() >= 360.0 || offset <= sweep.rem_euclid(360.0) {
                Some((radial - radius).abs())
            } else {
                let endpoint = |degrees: f64| Point {
                    x: center.x + radius * degrees.to_radians().cos(),
                    y: center.y + radius * degrees.to_radians().sin(),
                };
                Some(distance(point, endpoint(*start_deg)).min(distance(point, endpoint(*end_deg))))
            }
        }
        Entity::Point { origin }
        | Entity::Text { origin, .. }
        | Entity::Shape { origin, .. }
        | Entity::Insert { origin, .. } => Some(distance(point, *origin)),
        // Stored corner order is the TRACE/SOLID winding: the outline runs
        // p1-p2-p4-p3, so p2-p3 and p4-p1 are diagonals, not edges.
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => Some(
            [(*p1, *p2), (*p2, *p4), (*p4, *p3), (*p3, *p1)]
                .into_iter()
                .map(|(start, end)| segment_distance(start, end))
                .fold(f64::INFINITY, f64::min),
        ),
        Entity::Repeat(_) | Entity::Load { .. } | Entity::OnLayer { .. } | Entity::Erased(_) => {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;

    fn point_lattice(columns: u16, rows: u16) -> Item {
        Item::Repeat(acad_model::Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![Entity::Point {
                origin: Point { x: 1.0, y: 1.0 },
            }],
            columns,
            rows,
            column_spacing: 1.0,
            row_spacing: 1.0,
        })
    }

    /// RB2: owners each inside the per-owner pick limit cannot add up to
    /// unbounded hit-test work; the pick fails as a whole, never partially.
    #[test]
    fn pick_shares_one_drawing_hit_test_budget_across_owners() {
        let far = Point {
            x: -1_000.0,
            y: -1_000.0,
        };
        let mut editor = Editor::default();
        editor.drawing_mut().items = vec![point_lattice(250, 199); 5];
        assert_eq!(editor.try_pick_entity_at(far, 0.5), Ok(None));
        assert_eq!(
            editor.try_pick_entity_at(Point { x: 3.0, y: 2.0 }, 0.5),
            Ok(Some(1))
        );
        editor.drawing_mut().items = vec![point_lattice(250, 199); 30];
        assert_eq!(
            editor.try_pick_entity_at(far, 0.5),
            Err(HIT_TEST_BUDGET_EXCEEDED.to_string())
        );
        assert_eq!(editor.pick_entity_at(Point { x: 3.0, y: 2.0 }, 0.5), None);
        editor.submit("ERASE").unwrap();
        let before = editor.prompt().to_string();
        assert_eq!(
            editor.pick_selection_at(Point { x: 3.0, y: 2.0 }, 0.5),
            Err(HIT_TEST_BUDGET_EXCEEDED.to_string())
        );
        assert_eq!(editor.prompt(), before, "failed pick leaves the prompt");
    }

    /// Review pass 2 (h2): pick and window resolve INSERTs through one
    /// block index per pass instead of a linear scan per INSERT visit.
    #[test]
    fn hit_tests_resolve_blocks_through_a_per_pass_index() {
        let insert = |name: &str| Entity::Insert {
            name: name.into(),
            origin: Point { x: 1.0, y: 1.0 },
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
        };
        let mut editor = Editor::default();
        let items = &mut editor.drawing_mut().items;
        items.clear();
        items.push(Item::Block(acad_model::Block {
            name: "A".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![insert("Z"); 2_000],
        }));
        items.extend(std::iter::repeat_n(
            Item::Erased(Entity::Point {
                origin: Point { x: 0.0, y: 0.0 },
            }),
            65_335,
        ));
        items.push(Item::Block(acad_model::Block {
            name: "Z".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![Entity::Point {
                origin: Point { x: 0.0, y: 0.0 },
            }],
        }));
        items.extend(std::iter::repeat_n(Item::Entity(insert("A")), 190));
        let started = std::time::Instant::now();
        assert_eq!(
            editor.try_pick_entity_at(Point { x: 1.0, y: 1.0 }, 0.5),
            Ok(Some(1))
        );
        let window = checked_entities_in_window(
            editor.drawing(),
            Point { x: -5.0, y: -5.0 },
            Point { x: 5.0, y: 5.0 },
        )
        .unwrap();
        assert_eq!(window.len(), 190);
        let elapsed = started.elapsed();
        assert!(elapsed < std::time::Duration::from_secs(30), "{elapsed:?}");
    }

    #[test]
    fn window_selection_shares_one_drawing_hit_test_budget() {
        let mut editor = Editor::default();
        let points = (0..1_000)
            .map(|i| Entity::Point {
                origin: Point {
                    x: f64::from(i % 10),
                    y: f64::from(i / 10),
                },
            })
            .collect();
        editor.drawing_mut().items = vec![Item::Block(acad_model::Block {
            name: "DOTS".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: points,
        })];
        let insert = Item::Entity(Entity::Insert {
            name: "DOTS".into(),
            origin: Point { x: 0.0, y: 0.0 },
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
        });
        let (a, b) = (Point { x: -1.0, y: -1.0 }, Point { x: 20.0, y: 200.0 });
        editor
            .drawing_mut()
            .items
            .extend(std::iter::repeat_n(insert.clone(), 10));
        assert_eq!(
            checked_entities_in_window(editor.drawing(), a, b)
                .unwrap()
                .len(),
            10
        );
        editor
            .drawing_mut()
            .items
            .extend(std::iter::repeat_n(insert, 2_000));
        assert_eq!(
            checked_entities_in_window(editor.drawing(), a, b),
            Err(HIT_TEST_BUDGET_EXCEEDED.to_string())
        );
    }

    #[test]
    fn repeat_window_uses_all_lattice_extremes_and_ignores_empty_groups() {
        let mut editor = Editor::default();
        let group = acad_model::Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![Entity::Point {
                origin: Point { x: 1.0, y: 2.0 },
            }],
            columns: 3,
            rows: 2,
            column_spacing: -10.0,
            row_spacing: 5.0,
        };
        editor.drawing_mut().items = vec![
            Item::Repeat(group.clone()),
            Item::Repeat(acad_model::Repeat {
                start_layer: 1,
                end_layer: 1,
                entities: vec![],
                ..group
            }),
        ];
        assert!(entities_in_window(
            editor.drawing(),
            Point { x: 0.0, y: 0.0 },
            Point { x: 2.0, y: 3.0 }
        )
        .is_empty());
        assert_eq!(
            entities_in_window(
                editor.drawing(),
                Point { x: 2.0, y: 8.0 },
                Point { x: -20.0, y: 0.0 }
            ),
            vec![1]
        );
    }
}

#[cfg(test)]
mod nested_bounds_tests {
    use super::*;
    #[test]
    fn nested_repeat_window_bounds_stay_compact_without_a_generated_cell_budget() {
        let mut child = Entity::Point {
            origin: Point { x: 1.0, y: 2.0 },
        };
        for _ in 0..128 {
            child = Entity::Repeat(acad_model::Repeat {
                start_layer: 1,
                end_layer: 1,
                entities: vec![child],
                columns: 1,
                rows: 1,
                column_spacing: 0.0,
                row_spacing: 0.0,
            });
        }
        assert_eq!(
            selection_extents_points(&child),
            Some(vec![Point { x: 1.0, y: 2.0 }, Point { x: 1.0, y: 2.0 }])
        );
        let mut editor = crate::Editor::default();
        editor.drawing_mut().items.push(Item::Entity(child));
        assert_eq!(
            entities_in_window(
                editor.drawing(),
                Point { x: 0.0, y: 0.0 },
                Point { x: 3.0, y: 4.0 }
            ),
            vec![1]
        );
    }
}

#[cfg(test)]
mod visibility_tests;
