//! Closed-boundary area and perimeter calculations.
use super::*;

pub(crate) fn area_perimeter(entities: &[&Entity]) -> Result<(f64, f64), String> {
    if entities.is_empty() {
        return Err("AREA requires at least one entity".into());
    }
    if entities
        .iter()
        .all(|entity| matches!(bare(entity), Entity::Line { .. }))
    {
        let (area, perimeter) = line_loop_area(entities)?;
        return checked_area_metrics(area, perimeter);
    }

    let mut total_area = 0.0;
    let mut total_perimeter = 0.0;
    for entity in entities {
        match bare(entity) {
            Entity::Circle { center: _, radius } if *radius > 0.0 => {
                total_area += std::f64::consts::PI * radius * radius;
                total_perimeter += std::f64::consts::TAU * radius;
            }
            Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
                let (area, perimeter) = polygon_metrics(&[*p1, *p2, *p3, *p4]);
                total_area += area;
                total_perimeter += perimeter;
            }
            Entity::Circle { .. } => {
                return Err("AREA cannot measure a circle with nonpositive radius".into());
            }
            _ => return Err("AREA supports circles, TRACE/SOLID and closed LINE loops".into()),
        }
    }
    checked_area_metrics(total_area, total_perimeter)
}

pub(crate) fn checked_area_metrics(area: f64, perimeter: f64) -> Result<(f64, f64), String> {
    if !area.is_finite() || !perimeter.is_finite() {
        return Err("AREA result exceeds the finite numeric range".into());
    }
    Ok((area, perimeter))
}

pub(crate) fn line_loop_area(entities: &[&Entity]) -> Result<(f64, f64), String> {
    if entities.len() < 3 {
        return Err("AREA needs at least three lines for a closed loop".into());
    }
    let first = line_points(entities[0]).expect("the caller checked every entity is a line");
    if (first.1.x - first.0.x).hypot(first.1.y - first.0.y) <= 1e-12 {
        return Err("AREA cannot measure a zero-length line".into());
    }
    let mut points = vec![first.0, first.1];
    let mut remaining: Vec<_> = entities[1..]
        .iter()
        .map(|entity| line_points(entity).expect("the caller checked every entity is a line"))
        .collect();
    let coordinate_scale = entities
        .iter()
        .filter_map(|entity| line_points(entity))
        .flat_map(|(start, end)| [start.x.abs(), start.y.abs(), end.x.abs(), end.y.abs()])
        .fold(1.0, f64::max);
    let tolerance = coordinate_scale * 1e-9;
    let mut current = first.1;
    while !remaining.is_empty() {
        let matches: Vec<_> = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, (start, end))| {
                let starts_here = (start.x - current.x).hypot(start.y - current.y) <= tolerance;
                let ends_here = (end.x - current.x).hypot(end.y - current.y) <= tolerance;
                (starts_here || ends_here).then_some((index, starts_here, ends_here))
            })
            .collect();
        if matches.len() != 1 {
            return Err("AREA line selection must form one unbranched closed loop".into());
        }
        let (index, starts_here, ends_here) = matches[0];
        if starts_here && ends_here {
            return Err("AREA cannot measure a zero-length line".into());
        }
        let (start, end) = remaining.remove(index);
        let next = if starts_here { end } else { start };
        points.push(next);
        current = next;
    }
    if (current.x - first.0.x).hypot(current.y - first.0.y) > tolerance {
        return Err("AREA line selection is open".into());
    }
    Ok(polygon_metrics(&points))
}

pub(crate) fn polygon_metrics(points: &[Point]) -> (f64, f64) {
    let pairs = points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len());
    let mut twice_area = 0.0;
    let mut perimeter = 0.0;
    for (a, b) in pairs {
        twice_area += a.x * b.y - b.x * a.y;
        perimeter += (b.x - a.x).hypot(b.y - a.y);
    }
    (twice_area.abs() / 2.0, perimeter)
}
