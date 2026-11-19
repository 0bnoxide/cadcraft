use super::{Finding, Node, Unit, alignment_header, check_element_type, direction, number, point, radius, rotation};
use crate::geometry::{angle_delta, cross, distance, sub};
use crate::{Alignment, DIRECTION_TOLERANCE, Element, Kind, MAX_ELEMENTS, POSITION_TOLERANCE, Pose, Result, error};
use std::f64::consts::{SQRT_2, TAU};

// A bearing expressed to three decimal degrees can be off by half the last place.
const BEARING_TOLERANCE: f64 = 0.0005_f64.to_radians() + DIRECTION_TOLERANCE;

pub(super) fn reconstruct(n: &Node, scale: f64, step: f64, unit: Unit) -> Result<(Alignment, Finding)> {
    let mut alignment = alignment_header(n, scale)?;
    // The anchor and each source point can each move by half the diagonal of a rounding cell.
    let limit = (SQRT_2 * step).max(POSITION_TOLERANCE);
    let mut residual = Residual { distance: 0., element: 0, station: alignment.start_station, field: "Start" };
    let mut station = alignment.start_station;
    let mut previous = None;
    for (index, node) in n.child("CoordGeom")?.children.iter().enumerate() {
        if index >= MAX_ELEMENTS {
            return Err(error("ELEMENT_LIMIT", "more than 4096 elements"));
        }
        let (element, next) = rebuild_element(node, scale, limit, previous, station, index, &mut residual).map_err(|mut e| {
            e.reason = format!("element {index} at station {station}: {}", e.reason);
            e
        })?;
        station += element.length;
        alignment.elements.push(element);
        previous = Some(next);
    }
    let length = alignment.validated()?.length();
    if let Some(declared) = n.attrs.get("length") {
        let declared = number(declared)? * scale;
        check_difference("Alignment length", (declared - length).abs(), step.max(POSITION_TOLERANCE))?;
    }
    let warning = Finding {
        name: alignment.name.clone(),
        code: "ROUNDED_GEOMETRY_ADJUSTED".into(),
        reason: format!(
            "Reconstructed horizontal geometry. Maximum point displacement {:.9} {} (limit {:.9}) at element {} {} (station {}).",
            residual.distance,
            unit.name(),
            limit,
            residual.element,
            residual.field,
            residual.station
        ),
    };
    Ok((alignment, warning))
}

struct Previous {
    pose: Pose,
    stated_end: [f64; 2],
    line: bool,
}

#[derive(Clone, Copy)]
struct Residual {
    distance: f64,
    element: usize,
    station: f64,
    field: &'static str,
}

impl Residual {
    fn observe(&mut self, d: f64, limit: f64, element: usize, station: f64, field: &'static str) -> Result<()> {
        check_difference(field, d, limit)?;
        if d > self.distance {
            *self = Self { distance: d, element, station, field };
        }
        Ok(())
    }
}

fn check_difference(field: &str, difference: f64, limit: f64) -> Result<()> {
    if !difference.is_finite() || difference > limit {
        return Err(error("GEOMETRY_MISMATCH", format!("{field} residual {difference} exceeds {limit}")));
    }
    Ok(())
}

fn bearing(vector: [f64; 2], limit: f64) -> Result<f64> {
    if !vector.iter().all(|x| x.is_finite()) || vector[0].hypot(vector[1]) <= 2. * limit {
        return Err(error("GEOMETRY_MISMATCH", "insufficient point separation to recover a bearing"));
    }
    Ok(vector[1].atan2(vector[0]).rem_euclid(TAU))
}

fn point_bearing(node: &Node, start: [f64; 2], end: [f64; 2], kind: Kind, scale: f64, limit: f64) -> Result<f64> {
    match kind {
        Kind::Line => bearing(sub(end, start), limit),
        Kind::Arc { turn, .. } => {
            let inward = sub(point(node.child("Center")?, scale)?, start);
            bearing([turn.sign() * inward[1], -turn.sign() * inward[0]], limit)
        }
        Kind::Spiral { .. } => bearing(sub(point(node.child("PI")?, scale)?, start), limit),
    }
}

fn rebuild_element(
    node: &Node,
    scale: f64,
    limit: f64,
    previous: Option<Previous>,
    station: f64,
    index: usize,
    residual: &mut Residual,
) -> Result<(Element, Previous)> {
    check_element_type(node)?;
    let stated_start = point(node.child("Start")?, scale)?;
    let stated_end = point(node.child("End")?, scale)?;
    let length = node.number("length")? * scale;
    let kind = match node.name.as_str() {
        "Line" => Kind::Line,
        "Curve" => Kind::Arc { radius: node.number("radius")? * scale, turn: rotation(node)? },
        _ => Kind::Spiral { radius_start: radius(node, "radiusStart", scale)?, radius_end: radius(node, "radiusEnd", scale)?, turn: rotation(node)? },
    };
    let line = matches!(kind, Kind::Line);
    let mut start = stated_start;
    let mut inherited_heading = None;
    if let Some(prev) = previous {
        check_difference("stated joint gap", distance(prev.stated_end, stated_start), limit)?;
        start = prev.pose.point;
        // Consecutive lines can form a corner. Preserve the second line's own bearing.
        if !(prev.line && line) {
            inherited_heading = Some(prev.pose.direction);
        }
    }
    let fit = |heading| -> Result<(Element, Pose, Residual)> {
        let e = Element { start, direction: heading, length, kind };
        let end = e.at(length)?;
        let mut observed = *residual;
        observed.observe(distance(start, stated_start), limit, index, station, "Start")?;
        observed.observe(distance(end.point, stated_end), limit, index, station + length, "End")?;
        for (key, expected) in [("dir", heading), ("dirStart", heading), ("dirEnd", end.direction)] {
            if let Some(declared) = node.attrs.get(key) {
                check_difference(key, angle_delta(direction(number(declared)?), expected), BEARING_TOLERANCE)?;
            }
        }
        if let Some((difference, field)) = shape_residual(node, &e, &end, scale)? {
            observed.observe(difference, limit, index, station, field)?;
        }
        Ok((e, end, observed))
    };
    let derive_heading = || point_bearing(node, start, stated_end, kind, scale, limit);
    let (e, end, observed) = if let Some(heading) = inherited_heading {
        fit(heading)
    } else if let Some(declared) = node.attrs.get("dir").or_else(|| node.attrs.get("dirStart")) {
        match fit(direction(number(declared)?)) {
            // A three-decimal bearing can move a long element beyond the point budget.
            Err(err) if err.code == "GEOMETRY_MISMATCH" => match derive_heading() {
                Ok(heading) => fit(heading),
                Err(_) => Err(err),
            },
            result => result,
        }
    } else {
        fit(derive_heading()?)
    }?;
    *residual = observed;
    Ok((e, Previous { pose: end, stated_end, line }))
}

fn shape_residual(node: &Node, e: &Element, end: &Pose, scale: f64) -> Result<Option<(f64, &'static str)>> {
    let start = e.start;
    let (sin, cos) = e.direction.sin_cos();
    match e.kind {
        Kind::Arc { radius, turn } => {
            let offset = turn.sign() * radius;
            let center = [start[0] - sin * offset, start[1] + cos * offset];
            Ok(Some((distance(center, point(node.child("Center")?, scale)?), "Center")))
        }
        Kind::Spiral { .. } if node.children.iter().any(|child| child.name == "PI") => {
            let (end_sin, end_cos) = end.direction.sin_cos();
            let tangent = [cos, sin];
            let end_tangent = [end_cos, end_sin];
            let determinant = cross(tangent, end_tangent);
            if determinant.abs() <= DIRECTION_TOLERANCE {
                return Err(error("GEOMETRY_MISMATCH", "ill-conditioned PI tangent intersection"));
            }
            let along = cross(sub(end.point, start), end_tangent) / determinant;
            let intersection = [start[0] + cos * along, start[1] + sin * along];
            Ok(Some((distance(intersection, point(node.child("PI")?, scale)?), "PI")))
        }
        _ => Ok(None),
    }
}
