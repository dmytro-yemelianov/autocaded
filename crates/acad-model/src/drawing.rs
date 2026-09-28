use crate::{entity::{Block, Entity}, header::Header};

/// A 1983 DXF file is a flat sequence of records in which block definitions and
/// loose entities are interleaved. `SUBDIV.DXF` has 47 lines before its first
/// block and more blocks between its inserts, so document order is content and
/// the model preserves it rather than bucketing by kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Item { Entity(Entity), Block(Block) }

#[derive(Debug, Clone, PartialEq)]
pub struct Drawing { pub header: Header, pub items: Vec<Item> }

impl Drawing {
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.items.iter().filter_map(|i| match i { Item::Entity(e) => Some(e), _ => None })
    }
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|i| match i { Item::Block(b) => Some(b), _ => None })
    }
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks().find(|b| b.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geom::{Extents, Point}, header::{DwgView, Mode}};

    fn test_header() -> Header {
        let zero = Extents { xmin: 0.0, xmax: 0.0, ymin: 0.0, ymax: 0.0 };
        Header {
            extents: zero, limits: zero, base: Point { x: 0.0, y: 0.0 },
            view: DwgView { center: Point { x: 0.0, y: 0.0 }, height: 0.0 },
            snap: Mode { on: false, spacing: 0.0 }, grid: Mode { on: false, spacing: 0.0 },
            ortho: false, fill: false, text_size: 0.0, trace_width: 0.0,
            current_layer: 0, layer_colors: [255; 128],
        }
    }

    fn interleaved() -> Drawing {
        Drawing { header: test_header(), items: vec![
            Item::Entity(Entity::Line {
                start: Point { x: 0.0, y: 0.0 }, end: Point { x: 1.0, y: 1.0 } }),
            Item::Block(Block { name: "B1".into(), base: Point { x: 0.0, y: 0.0 },
                entities: vec![Entity::Circle {
                    center: Point { x: 0.0, y: 0.0 }, radius: 1.0 }] }),
            Item::Entity(Entity::Insert { origin: Point { x: 1.0, y: 1.0 },
                x_scale: 1.0, y_scale: 1.0, rotation_deg: 0.0, name: "B1".into() }),
        ] }
    }

    #[test]
    fn entities_skips_block_definitions_and_keeps_document_order() {
        let d = interleaved();
        let es: Vec<&Entity> = d.entities().collect();
        assert_eq!(es.len(), 2);
        assert!(matches!(es[0], Entity::Line { .. }));
        assert!(matches!(es[1], Entity::Insert { .. }));
    }

    #[test]
    fn blocks_yields_only_block_definitions() {
        assert_eq!(interleaved().blocks().count(), 1);
    }

    #[test]
    fn block_looks_up_by_name_and_misses_cleanly() {
        let d = interleaved();
        assert_eq!(d.block("B1").map(|b| b.name.as_str()), Some("B1"));
        assert!(d.block("NOPE").is_none());
    }
}
