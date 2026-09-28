//! AutoCAD's textual SHP libraries (not GIS shapefiles or compiled SHX).
//! Opcodes 0–9 cover all seven libraries shipped on the original disks.
use acad_model::Point;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeError(pub String);

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ShapeError {}

fn error(message: impl Into<String>) -> ShapeError {
    ShapeError(message.into())
}

#[derive(Debug, Clone)]
struct Definition {
    name: String,
    code: Vec<i16>,
}

#[derive(Debug, Clone)]
pub struct Library {
    definitions: BTreeMap<u16, Definition>,
    pub cap_height: Option<f64>,
}

fn number(token: &str) -> Result<i16, ShapeError> {
    let token = token.trim();
    let digits = token.trim_start_matches(['+', '-']);
    let radix = if digits.len() > 1 && digits.starts_with('0') {
        16
    } else {
        10
    };
    let n = i16::from_str_radix(digits, radix)
        .map_err(|_| error(format!("invalid SHP number {token:?}")))?;
    let n = if token.starts_with('-') { -n } else { n };
    if !(-128..=255).contains(&n) {
        return Err(error(format!("SHP byte out of range: {token}")));
    }
    Ok(n)
}

impl Library {
    pub fn contains(&self, number: u16) -> bool {
        self.definitions.contains_key(&number)
    }
    pub fn numbers(&self) -> impl Iterator<Item = u16> + '_ {
        self.definitions.keys().copied()
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, ShapeError> {
        let end = bytes.iter().position(|&b| b == 0x1a).unwrap_or(bytes.len());
        let text: String = bytes[..end].iter().map(|&b| b as char).collect();
        let mut definitions = BTreeMap::new();
        let mut pending: Option<(u16, usize, Definition)> = None;
        fn finish(
            pending: &mut Option<(u16, usize, Definition)>,
            out: &mut BTreeMap<u16, Definition>,
        ) -> Result<(), ShapeError> {
            if let Some((id, count, def)) = pending.take() {
                if def.code.len() != count {
                    return Err(error(format!(
                        "shape {id} ({}): declared {count} bytes, found {}",
                        def.name,
                        def.code.len()
                    )));
                }
                if out.insert(id, def).is_some() {
                    return Err(error(format!("duplicate shape {id}")));
                }
            }
            Ok(())
        }
        for (line, raw) in text.lines().enumerate() {
            let raw = raw.split(';').next().unwrap().trim();
            if raw.is_empty() {
                continue;
            }
            if let Some(header) = raw.strip_prefix('*') {
                finish(&mut pending, &mut definitions)?;
                let parts: Vec<_> = header.splitn(3, ',').collect();
                if parts.len() != 3 {
                    return Err(error(format!("line {}: invalid SHP header", line + 1)));
                }
                let id = parts[0]
                    .trim()
                    .parse::<u16>()
                    .map_err(|_| error("invalid shape number"))?;
                let count = parts[1]
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| error("invalid shape byte count"))?;
                pending = Some((
                    id,
                    count,
                    Definition {
                        name: parts[2].trim().into(),
                        code: Vec::new(),
                    },
                ));
            } else {
                let (_, _, def) = pending
                    .as_mut()
                    .ok_or_else(|| error("SHP data before first definition"))?;
                for token in raw
                    .split(|c: char| c == ',' || c == '(' || c == ')' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                {
                    def.code.push(number(token)?);
                }
            }
        }
        finish(&mut pending, &mut definitions)?;
        if definitions.is_empty() {
            return Err(error("empty SHP library"));
        }
        let cap_height = match definitions.get(&0) {
            Some(def)
                if def.code.len() == 4
                    && def.code[0] > 0
                    && def.code[1] >= 0
                    && def.code[3] == 0 =>
            {
                Some(def.code[0] as f64)
            }
            Some(_) => return Err(error("invalid SHP font metrics")),
            None => None,
        };
        Ok(Self {
            definitions,
            cap_height,
        })
    }

    pub fn shape(&self, number: u16) -> Result<Glyph, ShapeError> {
        let mut vm = Machine::new(self);
        vm.run(number, 0)?;
        if !vm.stack.is_empty() {
            return Err(error(format!("shape {number}: unbalanced position stack")));
        }
        Ok(vm.finish())
    }

    /// Points and advance are in font units; callers scale by height/cap_height.
    pub fn text(&self, text: &str) -> Result<Glyph, ShapeError> {
        if self.cap_height.is_none() {
            return Err(error("TEXT needs a font, not a shape library"));
        }
        let mut vm = Machine::new(self);
        // The supplied fonts use shape 1 to save the initial baseline for CR.
        if self.definitions.contains_key(&1) {
            vm.run(1, 0)?;
        }
        for ch in text.chars() {
            let id = u16::try_from(ch as u32)
                .map_err(|_| error(format!("unsupported text character {ch:?}")))?;
            vm.pen = true;
            vm.run(id, 0)?;
        }
        Ok(vm.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn vectors_use_major_axis_lengths_not_circular_directions() {
        let mut code = Vec::new();
        for d in 0..16 {
            code.extend(["5".to_string(), format!("02{d:X}"), "6".to_string()]);
        }
        code.push("0".into());
        let library =
            Library::parse(format!("*128,49,Directions\n{}\n", code.join(",")).as_bytes()).unwrap();
        let glyph = library.shape(128).unwrap();
        let expected = [
            (2., 0.),
            (2., 1.),
            (2., 2.),
            (1., 2.),
            (0., 2.),
            (-1., 2.),
            (-2., 2.),
            (-2., 1.),
            (-2., 0.),
            (-2., -1.),
            (-2., -2.),
            (-1., -2.),
            (0., -2.),
            (1., -2.),
            (2., -2.),
            (2., -1.),
        ];
        assert_eq!(glyph.strokes.len(), 16);
        for (stroke, (x, y)) in glyph.strokes.iter().zip(expected) {
            assert_eq!(*stroke, vec![p(0., 0.), p(x, y)]);
        }
        assert_eq!(glyph.advance, p(0., 0.));
    }

    #[test]
    fn subshapes_share_pen_scale_and_position_but_pop_does_not_draw() {
        let library = Library::parse(b"; mixed hex, decimal and parenthesized pairs\n*1,10,parent\n5,3,2,7,2,4,2,6,010,0\n*2,13,child\n2,8,(4,2),1,9,(2,0),(0,2),(0,0),0\x1aIGNORED SLACK").unwrap();
        let glyph = library.shape(1).unwrap();
        assert_eq!(
            glyph.strokes,
            vec![
                vec![p(2., 1.), p(3., 1.)],
                vec![p(3., 1.), p(3., 2.)],
                vec![p(0., 0.), p(1., 0.)]
            ]
        );
        assert_eq!(glyph.advance, p(1., 0.));
    }

    #[test]
    fn roman_a_geometry_and_advance_follow_the_definition() {
        // Original System/TXT.SHP's A: two legs, crossbar, then a pen-up
        // advance. Expected coordinates are read from its displacements.
        let library = Library::parse(
            b"*0,4,Roman\n21,7,0,0\n*65,17,A\n8,(8,21),8,(8,-21),2,8,(-13,7),1,0A0,2,8,(9,-7),0",
        )
        .unwrap();
        assert_eq!(library.cap_height, Some(21.));
        let glyph = library.text("AA").unwrap();
        assert_eq!(
            glyph.strokes,
            vec![
                vec![p(0., 0.), p(8., 21.)],
                vec![p(8., 21.), p(16., 0.)],
                vec![p(3., 7.), p(13., 7.)],
                vec![p(22., 0.), p(30., 21.)],
                vec![p(30., 21.), p(38., 0.)],
                vec![p(25., 7.), p(35., 7.)]
            ]
        );
        assert_eq!(glyph.advance, p(44., 0.));
        assert!(library
            .text("B")
            .unwrap_err()
            .0
            .contains("missing shape 66"));
    }

    #[test]
    fn malformed_programs_return_errors_instead_of_partial_geometry() {
        for (source, expected) in [
            ("*1,2,X\n010,0,0", "declared"),
            ("*1,1,X\n0\n*1,1,Y\n0", "duplicate"),
            ("*1,2,X\n256,0", "out of range"),
        ] {
            assert!(Library::parse(source.as_bytes())
                .unwrap_err()
                .0
                .contains(expected));
        }
        for (source, expected) in [
            ("*1,2,X\n8,1", "truncated"),
            ("*1,3,X\n3,0,0", "scale factor"),
            ("*1,2,X\n6,0", "underflow"),
            ("*1,2,X\n5,0", "unbalanced"),
            ("*1,3,X\n7,1,0", "recursion"),
            ("*1,3,X\n7,2,0", "missing shape"),
            ("*1,2,X\n10,0", "unsupported opcode"),
            ("*1,3,X\n0,010,0", "after terminator"),
        ] {
            assert!(
                Library::parse(source.as_bytes())
                    .unwrap()
                    .shape(1)
                    .unwrap_err()
                    .0
                    .contains(expected),
                "{source}"
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub strokes: Vec<Vec<Point>>,
    pub advance: Point,
}

struct Machine<'a> {
    library: &'a Library,
    position: Point,
    pen: bool,
    scale: f64,
    stack: Vec<Point>,
    strokes: Vec<Vec<Point>>,
    budget: usize,
}

impl<'a> Machine<'a> {
    fn new(library: &'a Library) -> Self {
        Self {
            library,
            position: Point { x: 0.0, y: 0.0 },
            pen: true,
            scale: 1.0,
            stack: Vec::new(),
            strokes: Vec::new(),
            budget: 100_000,
        }
    }
    fn finish(self) -> Glyph {
        Glyph {
            strokes: self.strokes,
            advance: self.position,
        }
    }
    fn move_by(&mut self, x: f64, y: f64) -> Result<(), ShapeError> {
        let next = Point {
            x: self.position.x + x * self.scale,
            y: self.position.y + y * self.scale,
        };
        if !next.x.is_finite() || !next.y.is_finite() {
            return Err(error("non-finite SHP geometry"));
        }
        if self.pen && next != self.position {
            self.strokes.push(vec![self.position, next]);
        }
        self.position = next;
        Ok(())
    }
    fn run(&mut self, id: u16, depth: usize) -> Result<(), ShapeError> {
        if depth >= 32 {
            return Err(error(format!("shape {id}: subshape recursion limit")));
        }
        if id == 0 {
            return Err(error("font metrics cannot be drawn as a shape"));
        }
        let definition = self
            .library
            .definitions
            .get(&id)
            .ok_or_else(|| error(format!("missing shape {id}")))?;
        let code = &definition.code;
        let mut i = 0;
        let take = |i: &mut usize| -> Result<i16, ShapeError> {
            let value = code
                .get(*i)
                .copied()
                .ok_or_else(|| error(format!("shape {id}: truncated instruction")))?;
            *i += 1;
            Ok(value)
        };
        loop {
            if self.budget == 0 {
                return Err(error("SHP instruction budget exceeded"));
            }
            self.budget -= 1;
            let op = take(&mut i)?;
            match op {
                0 => {
                    if i != code.len() {
                        return Err(error(format!("shape {id}: data after terminator")));
                    }
                    return Ok(());
                }
                1 => self.pen = true,
                2 => self.pen = false,
                3 | 4 => {
                    let n = take(&mut i)?;
                    if n <= 0 {
                        return Err(error(format!("shape {id}: invalid scale factor {n}")));
                    }
                    if op == 3 {
                        self.scale /= n as f64;
                    } else {
                        self.scale *= n as f64;
                    }
                    if !self.scale.is_finite() || self.scale == 0.0 {
                        return Err(error("SHP scale overflow/underflow"));
                    }
                }
                5 => {
                    if self.stack.len() == 32 {
                        return Err(error("SHP position stack overflow"));
                    }
                    self.stack.push(self.position);
                }
                6 => {
                    self.position = self
                        .stack
                        .pop()
                        .ok_or_else(|| error(format!("shape {id}: position stack underflow")))?
                }
                7 => {
                    let child = take(&mut i)?;
                    if child <= 0 {
                        return Err(error(format!("shape {id}: invalid subshape {child}")));
                    }
                    self.run(child as u16, depth + 1)?;
                }
                8 | 9 => loop {
                    let x = take(&mut i)?;
                    let y = take(&mut i)?;
                    if x > 127 || y > 127 {
                        return Err(error(format!("shape {id}: displacement out of range")));
                    }
                    if op == 9 && x == 0 && y == 0 {
                        break;
                    }
                    self.move_by(x as f64, y as f64)?;
                    if op == 8 {
                        break;
                    }
                },
                16..=255 => {
                    // Diagonals preserve the major-axis length; these are
                    // slopes 0, 1/2, 1, 2, not unit vectors on a circle.
                    const DIR: [(f64, f64); 16] = [
                        (1., 0.),
                        (1., 0.5),
                        (1., 1.),
                        (0.5, 1.),
                        (0., 1.),
                        (-0.5, 1.),
                        (-1., 1.),
                        (-1., 0.5),
                        (-1., 0.),
                        (-1., -0.5),
                        (-1., -1.),
                        (-0.5, -1.),
                        (0., -1.),
                        (0.5, -1.),
                        (1., -1.),
                        (1., -0.5),
                    ];
                    let (x, y) = DIR[(op & 15) as usize];
                    self.move_by(x * (op >> 4) as f64, y * (op >> 4) as f64)?;
                }
                _ => return Err(error(format!("shape {id}: unsupported opcode {op}"))),
            }
        }
    }
}
