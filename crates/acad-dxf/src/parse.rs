use crate::{error::DxfError, lex::{lex, Record}};
use acad_model::{Block, Drawing, DwgView, Entity, Extents, Header, Item, Mode, Point};

fn nums(rec: &Record, row: usize) -> Result<Vec<f64>, DxfError> {
    rec.rows[row].split(',').map(|t| t.trim().parse::<f64>()
        .map_err(|_| DxfError::BadNumber { row: rec.rows[row].clone(), line: rec.line }))
        .collect()
}

fn default_header() -> Header {
    let zero = Extents { xmin: 0.0, xmax: 0.0, ymin: 0.0, ymax: 0.0 };
    Header {
        extents: zero, limits: zero, base: Point { x: 0.0, y: 0.0 },
        view: DwgView { center: Point { x: 0.0, y: 0.0 }, height: 0.0 },
        snap: Mode { on: false, spacing: 0.0 }, grid: Mode { on: false, spacing: 0.0 },
        ortho: false, fill: false, text_size: 0.0, trace_width: 0.0,
        current_layer: 0, layer_colors: [255; 128],
    }
}

pub fn parse(bytes: &[u8]) -> Result<Drawing, DxfError> {
    let records = lex(bytes)?;
    let mut header = default_header();
    let mut items: Vec<Item> = Vec::new();
    let mut open_block: Option<Block> = None;

    for rec in &records {
        let entity = match rec.keyword.as_str() {
            "EXTENTS" | "LIMITS" => {
                let v = nums(rec, 0)?;
                let e = Extents { xmin: v[0], xmax: v[1], ymin: v[2], ymax: v[3] };
                if rec.keyword == "EXTENTS" { header.extents = e } else { header.limits = e }
                continue;
            }
            "BASE" => { let v = nums(rec, 0)?; header.base = Point { x: v[0], y: v[1] }; continue }
            "DWGVIEW" => { let v = nums(rec, 0)?;
                header.view = DwgView { center: Point { x: v[0], y: v[1] }, height: v[2] }; continue }
            "MODERES" | "MODEGRID" => {
                let v = nums(rec, 0)?;
                let m = Mode { on: v[0] != 0.0, spacing: v[1] };
                if rec.keyword == "MODERES" { header.snap = m } else { header.grid = m }
                continue;
            }
            "MODEORTHO" => { header.ortho = nums(rec, 0)?[0] != 0.0; continue }
            "MODEFILL" => { header.fill = nums(rec, 0)?[0] != 0.0; continue }
            "TXTSIZE" => { header.text_size = nums(rec, 0)?[0]; continue }
            "TRACEWID" => { header.trace_width = nums(rec, 0)?[0]; continue }
            "LAYER" => { header.current_layer = nums(rec, 0)?[0] as u8; continue }
            "LAYERC" => {
                for (r, row) in rec.rows.iter().enumerate() {
                    for (c, tok) in row.split(',').enumerate() {
                        header.layer_colors[r * 16 + c] =
                            tok.trim().parse::<u8>().map_err(|_| DxfError::BadNumber {
                                row: row.clone(), line: rec.line })?;
                    }
                }
                continue;
            }
            "BLOCK" => {
                let v = nums(rec, 0)?;
                open_block = Some(Block { name: rec.rows[1].clone(),
                    base: Point { x: v[0], y: v[1] }, entities: Vec::new() });
                continue;
            }
            "ENDBLK" => {
                if let Some(b) = open_block.take() { items.push(Item::Block(b)) }
                continue;
            }
            "LINE" => { let v = nums(rec, 0)?;
                Entity::Line { start: Point { x: v[0], y: v[1] }, end: Point { x: v[2], y: v[3] } } }
            "CIRCLE" => { let v = nums(rec, 0)?;
                Entity::Circle { center: Point { x: v[0], y: v[1] }, radius: v[2] } }
            "ARC" => { let v = nums(rec, 0)?;
                Entity::Arc { center: Point { x: v[0], y: v[1] }, radius: v[2],
                    start_deg: v[3], end_deg: v[4] } }
            "TEXT" => { let v = nums(rec, 0)?;
                Entity::Text { origin: Point { x: v[0], y: v[1] }, height: v[2],
                    rotation_deg: v[3], value: rec.rows[1].clone() } }
            "INSERT" => { let v = nums(rec, 0)?;
                Entity::Insert { origin: Point { x: v[0], y: v[1] }, x_scale: v[2],
                    y_scale: v[3], rotation_deg: v[4], name: rec.rows[1].clone() } }
            other => return Err(DxfError::UnknownKeyword { keyword: other.into(), line: rec.line }),
        };
        match open_block.as_mut() {
            Some(b) => b.entities.push(entity),
            None => items.push(Item::Entity(entity)),
        }
    }

    let drawing = Drawing { header, items };
    for e in drawing.entities() {
        if let Entity::Insert { name, .. } = e {
            if drawing.block(name).is_none() {
                return Err(DxfError::UndefinedBlock { name: name.clone() });
            }
        }
    }
    Ok(drawing)
}


#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::{Entity, Item, Point};

    fn with_header(body: &str) -> Vec<u8> {
        let mut s = String::from(
            "EXTENTS,1\r\n-1.750000,19.000000,-1.750000,14.000000\r\n\
             LIMITS,1\r\n-2.000000,19.000000,-2.000000,14.000000\r\n\
             BASE,1\r\n0.000000,0.000000\r\n\
             DWGVIEW,1\r\n8.500000,6.163380,16.326761\r\n\
             MODERES,1\r\n0,0.250000\r\nMODEGRID,1\r\n0,0.250000\r\n\
             MODEORTHO,1\r\n0\r\nMODEFILL,1\r\n1\r\n\
             TXTSIZE,1\r\n0.200000\r\nTRACEWID,1\r\n0.050000\r\nLAYER,1\r\n1\r\n");
        s.push_str(body);
        let mut b = s.into_bytes();
        b.push(0x1a);
        b
    }

    #[test]
    fn parses_header_fields_from_subdiv_values() {
        let d = parse(&with_header("")).unwrap();
        assert_eq!(d.header.limits.xmin, -2.0);
        assert_eq!(d.header.view.height, 16.326761);
        assert!(!d.header.snap.on);
        assert!(d.header.fill);
        assert_eq!(d.header.text_size, 0.2);
        assert_eq!(d.header.current_layer, 1);
    }

    #[test]
    fn parses_the_four_scalar_entity_kinds() {
        let d = parse(&with_header(
            "LINE,1\r\n8.800000,5.700000,19.000000,1.899999\r\n\
             CIRCLE,1\r\n16.464680,12.562830,0.420620\r\n\
             ARC,1\r\n3.037728,2.906527,2.339596,78.604100,22.450900\r\n\
             TEXT,1\r\n9.624130,12.295160,0.346010,0.000000\r\nA\r\n")).unwrap();
        let es: Vec<&Entity> = d.entities().collect();
        assert_eq!(es.len(), 4);
        assert_eq!(*es[0], Entity::Line {
            start: Point { x: 8.8, y: 5.7 }, end: Point { x: 19.0, y: 1.899999 } });
        assert_eq!(*es[2], Entity::Arc {
            center: Point { x: 3.037728, y: 2.906527 },
            radius: 2.339596, start_deg: 78.6041, end_deg: 22.4509 });
        assert_eq!(*es[3], Entity::Text {
            origin: Point { x: 9.62413, y: 12.29516 },
            height: 0.34601, rotation_deg: 0.0, value: "A".into() });
    }

    #[test]
    fn document_order_is_preserved_across_blocks_and_entities() {
        // SUBDIV.DXF interleaves them; a writer that buckets by kind cannot round-trip.
        let d = parse(&with_header(
            "LINE,1\r\n0,0,1,1\r\n\
             BLOCK,1\r\n0,0\r\nB1\r\nCIRCLE,1\r\n0,0,1\r\nENDBLK,1\r\n\
             INSERT,1\r\n1,1,1,1,0\r\nB1\r\n")).unwrap();
        assert!(matches!(d.items[0], Item::Entity(Entity::Line { .. })));
        assert!(matches!(d.items[1], Item::Block(_)));
        assert!(matches!(d.items[2], Item::Entity(Entity::Insert { .. })));
    }

    #[test]
    fn block_definition_collects_entities_until_endblk() {
        let d = parse(&with_header(
            "BLOCK,1\r\n8.500000,3.799999\r\nHOUSEA\r\n\
             LINE,1\r\n0,0,1,1\r\nENDBLK,1\r\n\
             INSERT,1\r\n10.437350,12.218680,1.000000,1.000000,0.000000\r\nHOUSEA\r\n")).unwrap();
        let blocks: Vec<_> = d.blocks().collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "HOUSEA");
        assert_eq!(blocks[0].entities.len(), 1);
        assert_eq!(d.entities().count(), 1); // the INSERT only
    }

    #[test]
    fn undefined_block_is_a_named_error() {
        let err = parse(&with_header(
            "INSERT,1\r\n1.0,2.0,1.0,1.0,0.0\r\nGHOST\r\n")).unwrap_err();
        assert_eq!(err, DxfError::UndefinedBlock { name: "GHOST".into() });
    }
}
