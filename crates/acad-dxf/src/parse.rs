use crate::{
    error::DxfError,
    lex::{lex, Record},
};
use acad_model::{Block, Drawing, DwgView, Entity, Extents, Header, Item, Mode, Point};

/// The 1983 LAYERC record is a fixed 8x16 grid; 255 marks an unused slot.
pub(crate) const LAYER_SLOTS: usize = 128;
pub(crate) const LAYER_UNUSED: u8 = 255;

/// Parse exactly `N` comma-separated numbers from a record row. The arity is
/// part of the record's definition, so a row of the wrong length is a named
/// error rather than an out-of-bounds index on the caller's side.
fn nums<const N: usize>(rec: &Record, row: usize) -> Result<[f64; N], DxfError> {
    let parts: Vec<&str> = rec.rows[row].split(',').collect();
    if parts.len() != N {
        return Err(DxfError::WrongFieldCount {
            keyword: rec.keyword.clone(),
            want: N,
            got: parts.len(),
            line: rec.line,
        });
    }
    let mut out = [0.0f64; N];
    for (i, t) in parts.iter().enumerate() {
        out[i] = t.trim().parse::<f64>().map_err(|_| DxfError::BadNumber {
            row: rec.rows[row].clone(),
            line: rec.line,
        })?;
    }
    Ok(out)
}

fn default_header() -> Header {
    let zero = Extents {
        xmin: 0.0,
        xmax: 0.0,
        ymin: 0.0,
        ymax: 0.0,
    };
    Header {
        extents: zero,
        limits: zero,
        base: Point { x: 0.0, y: 0.0 },
        view: DwgView {
            center: Point { x: 0.0, y: 0.0 },
            height: 0.0,
        },
        snap: Mode {
            on: false,
            spacing: 0.0,
        },
        grid: Mode {
            on: false,
            spacing: 0.0,
        },
        ortho: false,
        fill: false,
        text_size: 0.0,
        trace_width: 0.0,
        current_layer: 0,
        layers: Default::default(),
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
                let v = nums::<4>(rec, 0)?;
                let e = Extents {
                    xmin: v[0],
                    xmax: v[1],
                    ymin: v[2],
                    ymax: v[3],
                };
                if rec.keyword == "EXTENTS" {
                    header.extents = e
                } else {
                    header.limits = e
                }
                continue;
            }
            "BASE" => {
                let v = nums::<2>(rec, 0)?;
                header.base = Point { x: v[0], y: v[1] };
                continue;
            }
            "DWGVIEW" => {
                let v = nums::<3>(rec, 0)?;
                header.view = DwgView {
                    center: Point { x: v[0], y: v[1] },
                    height: v[2],
                };
                continue;
            }
            "MODERES" | "MODEGRID" => {
                let v = nums::<2>(rec, 0)?;
                let m = Mode {
                    on: v[0] != 0.0,
                    spacing: v[1],
                };
                if rec.keyword == "MODERES" {
                    header.snap = m
                } else {
                    header.grid = m
                }
                continue;
            }
            "MODEORTHO" => {
                header.ortho = nums::<1>(rec, 0)?[0] != 0.0;
                continue;
            }
            "MODEFILL" => {
                header.fill = nums::<1>(rec, 0)?[0] != 0.0;
                continue;
            }
            "TXTSIZE" => {
                header.text_size = nums::<1>(rec, 0)?[0];
                continue;
            }
            "TRACEWID" => {
                header.trace_width = nums::<1>(rec, 0)?[0];
                continue;
            }
            "LAYER" => {
                header.current_layer = nums::<1>(rec, 0)?[0] as u8;
                continue;
            }
            "LAYERC" => {
                let mut slot = 0usize;
                for row in rec.rows.iter() {
                    for tok in row.split(',') {
                        if slot >= LAYER_SLOTS {
                            return Err(DxfError::WrongFieldCount {
                                keyword: "LAYERC".into(),
                                want: LAYER_SLOTS,
                                got: slot + 1,
                                line: rec.line,
                            });
                        }
                        let color = tok.trim().parse::<u8>().map_err(|_| DxfError::BadNumber {
                            row: row.clone(),
                            line: rec.line,
                        })?;
                        if color != LAYER_UNUSED {
                            header.layers.insert(slot as u8, color);
                        }
                        slot += 1;
                    }
                }
                continue;
            }
            "BLOCK" => {
                let v = nums::<2>(rec, 0)?;
                if let Some(open) = &open_block {
                    return Err(DxfError::UnterminatedBlock {
                        name: open.name.clone(),
                        line: rec.line,
                    });
                }
                open_block = Some(Block {
                    name: rec.rows[1].clone(),
                    base: Point { x: v[0], y: v[1] },
                    entities: Vec::new(),
                });
                continue;
            }
            "ENDBLK" => {
                if let Some(b) = open_block.take() {
                    items.push(Item::Block(b))
                }
                continue;
            }
            // REPEAT's embedded entity has its own DXF record, unlike the
            // fused binary DWG record. Preserve that entity and discard the
            // container markers, as acad-dwg's flat model does.
            "REPEAT" => continue,
            "ENDREP" => {
                let _ = nums::<4>(rec, 0)?;
                continue;
            }
            "LINE" => {
                let v = nums::<4>(rec, 0)?;
                Entity::Line {
                    start: Point { x: v[0], y: v[1] },
                    end: Point { x: v[2], y: v[3] },
                }
            }
            "CIRCLE" => {
                let v = nums::<3>(rec, 0)?;
                Entity::Circle {
                    center: Point { x: v[0], y: v[1] },
                    radius: v[2],
                }
            }
            "POINT" => {
                let v = nums::<2>(rec, 0)?;
                Entity::Point {
                    origin: Point { x: v[0], y: v[1] },
                }
            }
            "TRACE" | "SOLID" => {
                let a = nums::<4>(rec, 0)?;
                let b = nums::<4>(rec, 1)?;
                let p1 = Point { x: a[0], y: a[1] };
                let p2 = Point { x: a[2], y: a[3] };
                let p3 = Point { x: b[0], y: b[1] };
                let p4 = Point { x: b[2], y: b[3] };
                if rec.keyword == "TRACE" {
                    Entity::Trace { p1, p2, p3, p4 }
                } else {
                    Entity::Solid { p1, p2, p3, p4 }
                }
            }
            "ARC" => {
                let v = nums::<5>(rec, 0)?;
                Entity::Arc {
                    center: Point { x: v[0], y: v[1] },
                    radius: v[2],
                    start_deg: v[3],
                    end_deg: v[4],
                }
            }
            "LOAD" => Entity::Load {
                name: rec.rows[0].clone(),
            },
            "SHAPE" => {
                let v = nums::<5>(rec, 0)?;
                if v[4] < 0.0 || v[4] > u16::MAX as f64 || v[4].fract() != 0.0 {
                    return Err(DxfError::BadNumber {
                        row: rec.rows[0].clone(),
                        line: rec.line,
                    });
                }
                Entity::Shape {
                    origin: Point { x: v[0], y: v[1] },
                    height: v[2],
                    rotation_deg: v[3],
                    number: v[4] as u16,
                }
            }
            "TEXT" => {
                let v = nums::<4>(rec, 0)?;
                Entity::Text {
                    origin: Point { x: v[0], y: v[1] },
                    height: v[2],
                    rotation_deg: v[3],
                    value: rec.rows[1].clone(),
                }
            }
            "INSERT" => {
                let v = nums::<5>(rec, 0)?;
                Entity::Insert {
                    origin: Point { x: v[0], y: v[1] },
                    x_scale: v[2],
                    y_scale: v[3],
                    rotation_deg: v[4],
                    name: rec.rows[1].clone(),
                }
            }
            other => {
                return Err(DxfError::UnknownKeyword {
                    keyword: other.into(),
                    line: rec.line,
                })
            }
        };
        match open_block.as_mut() {
            Some(b) => b.entities.push(entity),
            None => items.push(Item::Entity(entity)),
        }
    }

    if let Some(open) = open_block {
        let line = records.last().map(|r| r.line).unwrap_or(0);
        return Err(DxfError::UnterminatedBlock {
            name: open.name,
            line,
        });
    }

    let drawing = Drawing { header, items };
    // Scan entities inside block definitions too, not only top-level ones.
    for e in drawing
        .entities()
        .chain(drawing.blocks().flat_map(|b| b.entities.iter()))
    {
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
             TXTSIZE,1\r\n0.200000\r\nTRACEWID,1\r\n0.050000\r\nLAYER,1\r\n1\r\n",
        );
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
    fn load_and_shape_keep_order_and_block_membership() {
        let d = parse(&with_header("LOAD,0\r\nB:ES\r\nSHAPE,7\r\n2.250000,3.500000,0.750000,30.000000,129\r\nBLOCK,1\r\n0,0\r\nB1\r\nLOAD,1\r\nITALIC\r\nTEXT,1\r\n1,2,0.5,45\r\nA\r\nENDBLK,1\r\n")).unwrap();
        assert_eq!(d.items.len(), 3);
        assert_eq!(
            d.items[0],
            Item::Entity(Entity::Load {
                name: "B:ES".into()
            })
        );
        assert_eq!(
            d.items[1],
            Item::Entity(Entity::Shape {
                origin: Point { x: 2.25, y: 3.5 },
                height: 0.75,
                rotation_deg: 30.0,
                number: 129
            })
        );
        let block = d.block("B1").unwrap();
        assert_eq!(block.entities.len(), 2);
        assert_eq!(
            block.entities[0],
            Entity::Load {
                name: "ITALIC".into()
            }
        );
        assert!(matches!(&block.entities[1], Entity::Text { value, .. } if value == "A"));
        let written = crate::write(&d);
        let expected = b"SHAPE,1\r\n2.250000,3.500000,0.750000,30.000000,129\r\n";
        assert!(written.windows(expected.len()).any(|s| s == expected));
        assert_eq!(parse(&written).unwrap(), d);
    }

    #[test]
    fn shape_numbers_must_be_representable_in_a_dwg_word() {
        for number in ["-1", "65536", "129.5", "NaN", "inf"] {
            assert!(
                matches!(
                    parse(&with_header(&format!("SHAPE,1\r\n0,0,1,0,{number}\r\n"))),
                    Err(DxfError::BadNumber { .. })
                ),
                "{number}"
            );
        }
        assert!(matches!(
            parse(&with_header("SHAPE,1\r\n0,0,1,0\r\n")),
            Err(DxfError::WrongFieldCount {
                want: 5,
                got: 4,
                ..
            })
        ));
    }

    #[test]
    fn parses_the_four_scalar_entity_kinds() {
        let d = parse(&with_header(
            "LINE,1\r\n8.800000,5.700000,19.000000,1.899999\r\n\
             CIRCLE,1\r\n16.464680,12.562830,0.420620\r\n\
             ARC,1\r\n3.037728,2.906527,2.339596,78.604100,22.450900\r\n\
             TEXT,1\r\n9.624130,12.295160,0.346010,0.000000\r\nA\r\n",
        ))
        .unwrap();
        let es: Vec<&Entity> = d.entities().collect();
        assert_eq!(es.len(), 4);
        assert_eq!(
            *es[0],
            Entity::Line {
                start: Point { x: 8.8, y: 5.7 },
                end: Point {
                    x: 19.0,
                    y: 1.899999
                }
            }
        );
        assert_eq!(
            *es[2],
            Entity::Arc {
                center: Point {
                    x: 3.037728,
                    y: 2.906527
                },
                radius: 2.339596,
                start_deg: 78.6041,
                end_deg: 22.4509
            }
        );
        assert_eq!(
            *es[3],
            Entity::Text {
                origin: Point {
                    x: 9.62413,
                    y: 12.29516
                },
                height: 0.34601,
                rotation_deg: 0.0,
                value: "A".into()
            }
        );
    }

    #[test]
    fn document_order_is_preserved_across_blocks_and_entities() {
        // SUBDIV.DXF interleaves them; a writer that buckets by kind cannot round-trip.
        let d = parse(&with_header(
            "LINE,1\r\n0,0,1,1\r\n\
             BLOCK,1\r\n0,0\r\nB1\r\nCIRCLE,1\r\n0,0,1\r\nENDBLK,1\r\n\
             INSERT,1\r\n1,1,1,1,0\r\nB1\r\n",
        ))
        .unwrap();
        assert!(matches!(d.items[0], Item::Entity(Entity::Line { .. })));
        assert!(matches!(d.items[1], Item::Block(_)));
        assert!(matches!(d.items[2], Item::Entity(Entity::Insert { .. })));
    }

    #[test]
    fn block_definition_collects_entities_until_endblk() {
        let d = parse(&with_header(
            "BLOCK,1\r\n8.500000,3.799999\r\nHOUSEA\r\n\
             LINE,1\r\n0,0,1,1\r\nENDBLK,1\r\n\
             INSERT,1\r\n10.437350,12.218680,1.000000,1.000000,0.000000\r\nHOUSEA\r\n",
        ))
        .unwrap();
        let blocks: Vec<_> = d.blocks().collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "HOUSEA");
        assert_eq!(blocks[0].entities.len(), 1);
        assert_eq!(d.entities().count(), 1); // the INSERT only
    }

    #[test]
    fn short_rows_are_errors_not_panics() {
        // Every one of these panicked before: nums() returned a Vec of whatever
        // length the row had and each call site indexed it positionally.
        for (kw, body) in [
            ("LINE", "LINE,1\r\n1.0,2.0\r\n"),
            ("CIRCLE", "CIRCLE,1\r\n1.0,2.0\r\n"),
            ("ARC", "ARC,1\r\n1.0,2.0,3.0\r\n"),
            ("TEXT", "TEXT,1\r\n1.0,2.0\r\nA\r\n"),
            ("INSERT", "INSERT,1\r\n1.0,2.0,1.0\r\nB\r\n"),
            ("EXTENTS", "EXTENTS,1\r\n1.0,2.0\r\n"),
            ("BASE", "BASE,1\r\n1.0\r\n"),
            ("DWGVIEW", "DWGVIEW,1\r\n1.0,2.0\r\n"),
            ("MODERES", "MODERES,1\r\n0\r\n"),
        ] {
            let got = parse(&with_header(body));
            assert!(got.is_err(), "{kw}: short row must be an error, got Ok");
        }
    }

    #[test]
    fn a_row_cut_short_at_end_of_file_is_an_error_not_a_panic() {
        // A file truncated mid-row reaches the parser as a well-formed-looking
        // record with too few fields. Review Focus 1: never a panic.
        let mut b = with_header("LINE,1\r\n1.0,2.0");
        b.pop(); // drop the DOS EOF so the row really is the last thing present
        assert!(parse(&b).is_err());
    }

    #[test]
    fn overlong_layerc_row_is_an_error_not_an_out_of_bounds_write() {
        let mut row = vec!["0"; 17].join(",");
        row.push_str("\r\n");
        let mut body = String::from("LAYERC,1\r\n");
        body.push_str(&row);
        for _ in 0..7 {
            body.push_str("255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255\r\n");
        }
        assert!(parse(&with_header(&body)).is_err());
    }

    #[test]
    fn block_left_open_at_end_of_file_is_an_error_not_silent_loss() {
        let got = parse(&with_header(
            "BLOCK,1\r\n0,0\r\nB1\r\nLINE,1\r\n0,0,1,1\r\n",
        ));
        assert!(
            got.is_err(),
            "unterminated BLOCK must not load as an empty drawing"
        );
    }

    #[test]
    fn block_reopened_before_endblk_is_an_error_not_silent_loss() {
        let got = parse(&with_header(
            "BLOCK,1\r\n0,0\r\nB1\r\nLINE,1\r\n0,0,1,1\r\n\
             BLOCK,1\r\n0,0\r\nB2\r\nENDBLK,1\r\n",
        ));
        assert!(
            got.is_err(),
            "re-opening a block must not discard the first"
        );
    }

    #[test]
    fn undefined_block_referenced_from_inside_a_block_is_caught() {
        let got = parse(&with_header(
            "BLOCK,1\r\n0,0\r\nOUTER\r\n\
             INSERT,1\r\n1.0,2.0,1.0,1.0,0.0\r\nGHOST\r\nENDBLK,1\r\n",
        ));
        assert_eq!(
            got.unwrap_err(),
            DxfError::UndefinedBlock {
                name: "GHOST".into()
            }
        );
    }

    #[test]
    fn high_bit_latin1_text_loads_rather_than_being_called_corrupt() {
        // 1983 DXF is Latin-1. 0xB0 is the degree sign; rejecting it reports an
        // intact drawing as corrupt.
        let mut b = with_header("TEXT,1\r\n1.0,2.0,0.2,0.0\r\n");
        b.pop();
        b.extend_from_slice(&[0xB0]);
        b.extend_from_slice(b"C\r\n");
        b.push(0x1a);
        let d = parse(&b).expect("Latin-1 text must load");
        assert_eq!(d.entities().count(), 1);
    }

    #[test]
    fn undefined_block_is_a_named_error() {
        let err = parse(&with_header("INSERT,1\r\n1.0,2.0,1.0,1.0,0.0\r\nGHOST\r\n")).unwrap_err();
        assert_eq!(
            err,
            DxfError::UndefinedBlock {
                name: "GHOST".into()
            }
        );
    }
}
