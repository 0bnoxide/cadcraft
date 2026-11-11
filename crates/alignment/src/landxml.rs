//! Public LandXML 1.2 alignment exchange; deliberately independent of the frozen handoff contract.
use crate::geometry::{angle_delta, cross, distance, dot, sub};
use crate::{Alignment, DIRECTION_TOLERANCE, Element, Kind, MAX_ELEMENTS, POSITION_TOLERANCE, Result, StationFormat, Turn, error};
use quick_xml::{
    NsReader, XmlVersion,
    events::{BytesStart, Event},
    name::ResolveResult,
};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{FRAC_PI_2, TAU};
use std::fmt::Write;
pub const MAX_XML_BYTES: usize = 16 * 1024 * 1024;
const NS: &str = "http://www.landxml.org/schema/LandXML-1.2";
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Meter,
    Foot,
    USSurveyFoot,
}
impl Unit {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "meter" => Some(Self::Meter),
            "foot" => Some(Self::Foot),
            "USSurveyFoot" => Some(Self::USSurveyFoot),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Meter => "meter",
            Self::Foot => "foot",
            Self::USSurveyFoot => "USSurveyFoot",
        }
    }
    pub fn meters(self) -> f64 {
        match self {
            Self::Meter => 1.,
            Self::Foot => 0.3048,
            Self::USSurveyFoot => 1200. / 3937.,
        }
    }
}
#[derive(Debug, Default)]
pub struct Import {
    pub alignments: Vec<Alignment>,
    pub refusals: Vec<Finding>,
    pub warnings: Vec<Finding>,
}
#[derive(Debug, serde::Serialize)]
pub struct Finding {
    pub name: String,
    pub code: String,
    pub reason: String,
}

#[derive(Debug)]
pub(crate) struct Node {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub text: String,
    pub children: Vec<Node>,
}
impl Node {
    pub(crate) fn attr(&self, key: &str) -> Result<&str> {
        self.attrs.get(key).map(String::as_str).ok_or_else(|| error("MISSING_ATTRIBUTE", format!("{} requires {key}", self.name)))
    }
    pub(crate) fn number(&self, key: &str) -> Result<f64> {
        number(self.attr(key)?)
    }
    pub(crate) fn child(&self, name: &str) -> Result<&Node> {
        let mut children = self.children.iter().filter(|c| c.name == name);
        let child = children.next().ok_or_else(|| error("MISSING_ELEMENT", format!("{} requires {name}", self.name)))?;
        if children.next().is_some() {
            return Err(error("DUPLICATE_ELEMENT", name));
        }
        Ok(child)
    }
}
fn number(s: &str) -> Result<f64> {
    s.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| error("INVALID_NUMBER", "finite numeric value required"))
}
fn node(reader: &NsReader<&[u8]>, e: &BytesStart<'_>) -> Result<Node> {
    let (ns, local) = reader.resolver().resolve_element(e.name());
    if !matches!(ns,ResolveResult::Bound(ns) if ns.as_ref()==NS) {
        return Err(error("INVALID_NAMESPACE", "LandXML 1.2 namespace required"));
    }
    if local.as_ref().len() > 128 {
        return Err(error("XML_LIMIT", "element name too long"));
    }
    let name = local.as_ref().to_string();
    let mut attrs = BTreeMap::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|_| error("INVALID_XML", "malformed or duplicate attribute"))?;
        if attr.value.contains('<') {
            return Err(error("INVALID_XML", "unescaped '<' in attribute"));
        }
        if attrs.len() >= 64 || attr.key.as_ref().len() > 128 || attr.value.len() > 2048 {
            return Err(error("XML_LIMIT", "attribute limit"));
        }
        let key = attr.key.as_ref().to_string();
        let value = attr.normalized_value(XmlVersion::Explicit1_0).map_err(|_| error("INVALID_XML", "invalid attribute value"))?.into_owned();
        attrs.insert(key, value);
    }
    Ok(Node { name, attrs, text: String::new(), children: Vec::new() })
}
fn attach(stack: &mut [Node], root: &mut Option<Node>, value: Node) -> Result<()> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(value);
    } else if root.replace(value).is_some() {
        return Err(error("INVALID_XML", "multiple roots"));
    }
    Ok(())
}
fn tree(bytes: &[u8]) -> Result<Node> {
    if bytes.len() > MAX_XML_BYTES {
        return Err(error("XML_LIMIT", "XML exceeds 16MiB"));
    }
    let source = std::str::from_utf8(bytes).map_err(|_| error("INVALID_XML", "UTF-8 required"))?;
    if source.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
        return Err(error("INVALID_XML", "forbidden XML control character"));
    }
    let mut reader = NsReader::from_reader(bytes);
    reader.config_mut().check_comments = true;
    let mut stack = Vec::new();
    let mut root = None;
    let mut tokens = 0usize;
    let mut elements = 0usize;
    loop {
        let event = reader.read_event().map_err(|_| error("INVALID_XML", "malformed or truncated XML"))?;
        tokens += 1;
        if tokens > 200_000 {
            return Err(error("XML_LIMIT", "XML token limit"));
        }
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                if stack.len() >= 32 {
                    return Err(error("XML_LIMIT", "XML nesting exceeds 32"));
                }
                let value = node(&reader, e)?;
                if matches!(value.name.as_str(), "Line" | "Curve" | "Spiral") {
                    elements += 1;
                    if elements > MAX_ELEMENTS {
                        return Err(error("ELEMENT_LIMIT", "file exceeds 4096 route elements"));
                    }
                }
                if matches!(event, Event::Empty(_)) {
                    attach(&mut stack, &mut root, value)?;
                } else {
                    stack.push(value);
                }
            }
            Event::End(_) => {
                let value = stack.pop().ok_or_else(|| error("INVALID_XML", "unexpected closing tag"))?;
                attach(&mut stack, &mut root, value)?;
            }
            Event::Text(text) => {
                append_text(&mut stack, &text)?;
            }
            Event::CData(text) => {
                append_text(&mut stack, &text)?;
            }
            Event::GeneralRef(r) => {
                let value = if r.is_char_ref() {
                    r.resolve_char_ref().ok().flatten().map(|c| c.to_string())
                } else {
                    quick_xml::escape::resolve_predefined_entity(&r).map(str::to_owned)
                }
                .ok_or_else(|| error("INVALID_XML", "external entities forbidden"))?;
                append_text(&mut stack, &value)?;
            }
            Event::DocType(_) => return Err(error("FORBIDDEN_DTD", "document types and entities are forbidden")),
            Event::Decl(d) => {
                if tokens != 1 || !stack.is_empty() || root.is_some() || !declaration_ok(&d) {
                    return Err(error("INVALID_XML", "a single UTF-8 XML 1.0 declaration must be first"));
                }
            }
            Event::Eof => break,
            Event::PI(_) | Event::Comment(_) => {}
        }
    }
    if !stack.is_empty() {
        return Err(error("INVALID_XML", "truncated XML"));
    }
    root.ok_or_else(|| error("INVALID_XML", "missing root"))
}
fn declaration_ok(d: &quick_xml::events::BytesDecl<'_>) -> bool {
    let content = BytesStart::from_content(&**d, 3);
    let mut expected = ["version", "encoding", "standalone"].into_iter();
    let mut first = true;
    for attr in content.attributes() {
        let Ok(a) = attr else { return false };
        let key = a.key.as_ref();
        if (first && key != "version") || expected.find(|k| *k == key).is_none() {
            return false;
        }
        first = false;
        let valid = match key {
            "version" => a.value == "1.0",
            "encoding" => a.value.eq_ignore_ascii_case("UTF-8"),
            "standalone" => matches!(a.value.as_ref(), "yes" | "no"),
            _ => false,
        };
        if !valid {
            return false;
        }
    }
    !first
}
fn append_text(stack: &mut [Node], text: &str) -> Result<()> {
    if let Some(n) = stack.last_mut() {
        if n.text.len().saturating_add(text.len()) > 4 * 1024 * 1024 {
            return Err(error("XML_LIMIT", "text exceeds 4MiB"));
        }
        n.text.push_str(text);
    } else if !text.trim().is_empty() {
        return Err(error("INVALID_XML", "text outside root"));
    }
    Ok(())
}

pub fn import(bytes: &[u8], unit: Unit) -> Result<Import> {
    let root = tree(bytes)?;
    if root.name != "LandXML" || root.attr("version")? != "1.2" {
        return Err(error("INVALID_XML", "LandXML version 1.2 required"));
    }
    let units = root.child("Units")?;
    let [u] = units.children.as_slice() else { return Err(error("UNSUPPORTED_UNITS", "exactly one Metric/Imperial definition required")) };
    if !matches!(u.name.as_str(), "Metric" | "Imperial") || u.attrs.get("directionUnit").is_some_and(|s| s != "decimal degrees") {
        return Err(error("UNSUPPORTED_UNITS", "decimal degrees direction units required"));
    }
    let from = Unit::parse(u.attr("linearUnit")?).ok_or_else(|| error("UNSUPPORTED_UNITS", "linear unit must be meter, foot or USSurveyFoot"))?;
    let scale = from.meters() / unit.meters();
    let mut out = Import::default();
    for group in root.children.iter().filter(|n| n.name == "Alignments") {
        for n in group.children.iter().filter(|n| n.name == "Alignment") {
            if out.alignments.len() + out.refusals.len() >= 128 {
                return Err(error("ALIGNMENT_LIMIT", "at most 128 alignments per file"));
            }
            let name = n.attrs.get("name").cloned().unwrap_or_default();
            match alignment(n, scale) {
                Ok(a) => {
                    warnings(n, &name, &mut out.warnings);
                    out.alignments.push(a);
                }
                Err(e) => out.refusals.push(Finding { name, code: e.code.into(), reason: e.reason }),
            }
        }
    }
    if out.alignments.is_empty() && out.refusals.is_empty() {
        return Err(error("MISSING_ALIGNMENT", "no Alignments/Alignment found"));
    }
    Ok(out)
}
fn warnings(n: &Node, name: &str, out: &mut Vec<Finding>) {
    for c in &n.children {
        if c.name == "StaEquation" {
            out.push(Finding {
                name: name.into(),
                code: "STATION_EQUATIONS_IGNORED".into(),
                reason: format!("{name}: StaEquation {:?}; plain stationing retained", c.attrs),
            });
        }
        if c.name == "Profile" {
            out.push(Finding {
                name: name.into(),
                code: "PROFILES_NOT_SUPPORTED".into(),
                reason: "horizontal geometry imported; profile support follows in the stacked change".into(),
            });
        }
    }
}
fn point(n: &Node, scale: f64) -> Result<[f64; 2]> {
    if !n.children.is_empty() {
        return Err(error("UNSUPPORTED_ELEMENT", "point references/children are unsupported"));
    }
    let values = n.text.split_whitespace().take(4).map(number).collect::<Result<Vec<_>>>()?;
    match values.as_slice() {
        [north, east] => Ok([east * scale, north * scale]),
        [north, east, z] if *z == 0. => Ok([east * scale, north * scale]),
        _ => Err(error("UNSUPPORTED_COORDINATE", "horizontal points require northing easting and optional zero elevation")),
    }
}
fn direction(value: f64) -> f64 {
    // Schema direction: "Assume 0 degrees = north"; angle: "Assume 0 degrees = east".
    // Compass bearings run clockwise; internal radians run CCW. Accept decimal degrees only.
    (FRAC_PI_2 - value.to_radians()).rem_euclid(TAU)
}
fn check_attr(n: &Node, key: &str, expected: f64, scale: f64) -> Result<()> {
    if let Some(v) = n.attrs.get(key) {
        let actual = number(v)? * scale;
        let mismatch = if key.starts_with("dir") { angle_delta(direction(actual), expected) } else { (actual - expected).abs() };
        let tolerance = if key.starts_with("dir") { DIRECTION_TOLERANCE } else { POSITION_TOLERANCE };
        if mismatch > tolerance {
            return Err(error("GEOMETRY_MISMATCH", format!("{key} mismatch {mismatch}, expected {expected}")));
        }
    }
    Ok(())
}
fn rotation(n: &Node) -> Result<Turn> {
    match n.attr("rot")? {
        "ccw" => Ok(Turn::Left),
        "cw" => Ok(Turn::Right),
        _ => Err(error("INVALID_ROTATION", "rot must be cw or ccw")),
    }
}
fn radius(n: &Node, key: &str, scale: f64) -> Result<Option<f64>> {
    let v = n.attr(key)?;
    if v == "INF" { Ok(None) } else { Ok(Some(number(v)? * scale)) }
}
fn element(n: &Node, scale: f64) -> Result<Element> {
    if n.name == "Spiral" && n.attrs.get("spiType").is_none_or(|v| v != "clothoid") {
        return Err(error("UNSUPPORTED_SPIRAL", "only explicit spiType=clothoid is supported"));
    }
    if !matches!(n.name.as_str(), "Line" | "Curve" | "Spiral") {
        return Err(error("UNSUPPORTED_ELEMENT", format!("CoordGeom/{} cannot be represented exactly", n.name)));
    }
    let start = point(n.child("Start")?, scale)?;
    let end = point(n.child("End")?, scale)?;
    let e = match n.name.as_str() {
        "Line" => {
            let d = sub(end, start);
            Element { start, direction: d[1].atan2(d[0]).rem_euclid(TAU), length: distance(start, end), kind: Kind::Line }
        }
        "Curve" => curve(n, start, end, scale)?,
        _ => spiral(n, start, scale)?,
    };
    e.validate()?;
    let pose = e.at(e.length)?;
    let gap = distance(pose.point, end);
    if gap > POSITION_TOLERANCE {
        return Err(error("GEOMETRY_MISMATCH", format!("End gap={gap}")));
    }
    check_attr(n, "length", e.length, scale)?;
    check_attr(n, "dir", e.direction, 1.)?;
    check_attr(n, "dirStart", e.direction, 1.)?;
    check_attr(n, "dirEnd", pose.direction, 1.)?;
    Ok(e)
}
fn curve(n: &Node, start: [f64; 2], end: [f64; 2], scale: f64) -> Result<Element> {
    let center = point(n.child("Center")?, scale)?;
    let measured_radius = distance(start, center);
    let r = n.attrs.get("radius").map(|v| number(v).map(|r| r * scale)).transpose()?.unwrap_or(measured_radius);
    // Generated center coordinates and their hypot incur a few rounding steps.
    // Preserve the stated radius within that error, not a looser geometry limit:
    // element() still checks the evaluated End to POSITION_TOLERANCE.
    let radius_tolerance = POSITION_TOLERANCE.max(4. * f64::EPSILON * r);
    if !r.is_finite() || !measured_radius.is_finite() || (measured_radius - r).abs() > radius_tolerance {
        return Err(error("GEOMETRY_MISMATCH", "arc radius mismatch"));
    }
    let turn = rotation(n)?;
    // Derive the tangent directly and measure the sweep in its local frame.
    // Subtracting endpoint polar angles loses precision for a very shallow arc.
    let inward = sub(center, start);
    let direction = (-turn.sign() * inward[0]).atan2(turn.sign() * inward[1]).rem_euclid(TAU);
    let tangent = [direction.cos(), direction.sin()];
    let chord = sub(end, start);
    let sweep = dot(chord, tangent).atan2(r - turn.sign() * cross(tangent, chord)).rem_euclid(TAU);
    if distance(start, end) == 0. || (distance(end, center) - r).abs() > radius_tolerance {
        return Err(error("GEOMETRY_MISMATCH", "arc full circle or unequal endpoint radii"));
    }
    Ok(Element { start, direction, length: sweep * r, kind: Kind::Arc { radius: r, turn } })
}
fn spiral(n: &Node, start: [f64; 2], scale: f64) -> Result<Element> {
    let pi = n.children.iter().find(|c| c.name == "PI").map(|p| point(p, scale)).transpose()?;
    let d = if let Some(pi) = pi {
        let v = sub(pi, start);
        if distance(pi, start) == 0. {
            return Err(error("GEOMETRY_MISMATCH", "PI coincides with Start"));
        }
        v[1].atan2(v[0]).rem_euclid(TAU)
    } else {
        direction(n.number("dirStart")?)
    };
    let e = Element {
        start,
        direction: d,
        length: n.number("length")? * scale,
        kind: Kind::Spiral { radius_start: radius(n, "radiusStart", scale)?, radius_end: radius(n, "radiusEnd", scale)?, turn: rotation(n)? },
    };
    e.validate()?;
    if let Some(pi) = pi {
        let end = e.at(e.length)?;
        let v = sub(pi, end.point);
        let tangent = [end.direction.cos(), end.direction.sin()];
        let off = cross(tangent, v).abs();
        if off > POSITION_TOLERANCE {
            return Err(error("GEOMETRY_MISMATCH", format!("PI end-tangent gap={off}")));
        }
    }
    Ok(e)
}
fn alignment(n: &Node, scale: f64) -> Result<Alignment> {
    let mut out = Alignment {
        name: n.attr("name")?.into(),
        start_station: n.number("staStart")? * scale,
        format: StationFormat::default(),
        elements: Vec::new(),
    };
    let geom = n.child("CoordGeom")?;
    for (index, c) in geom.children.iter().enumerate() {
        if out.elements.len() >= MAX_ELEMENTS {
            return Err(error("ELEMENT_LIMIT", "more than 4096 elements"));
        }
        let e = element(c, scale).map_err(|mut e| {
            e.reason = format!("element {index}: {}", e.reason);
            e
        })?;
        out.elements.push(e);
    }
    out.checked()?;
    check_attr(n, "length", out.length()?, scale)?;
    Ok(out)
}

fn escaped(s: &str) -> String {
    quick_xml::escape::escape(s).into_owned()
}
fn append(out: &mut String, text: &str) -> Result<()> {
    if out.len().saturating_add(text.len()) > MAX_XML_BYTES {
        return Err(error("XML_LIMIT", "export exceeds 16MiB"));
    }
    out.push_str(text);
    Ok(())
}
fn xml_point(out: &mut String, name: &str, p: [f64; 2]) -> Result<()> {
    append(out, &format!("<{name}>{} {}</{name}>", p[1], p[0]))
}
fn compass(d: f64) -> f64 {
    (FRAC_PI_2 - d.rem_euclid(TAU)).to_degrees().rem_euclid(360.)
}
fn export_element(out: &mut String, e: &Element) -> Result<()> {
    let end = e.at(e.length)?;
    let direction = e.direction.rem_euclid(TAU);
    let close = match e.kind {
        Kind::Line => {
            append(out, &format!("<Line length=\"{}\" dir=\"{}\">", e.length, compass(e.direction)))?;
            "Line"
        }
        Kind::Arc { radius, turn } => {
            append(out, &format!("<Curve length=\"{}\" radius=\"{radius}\" rot=\"{}\">", e.length, if turn == Turn::Left { "ccw" } else { "cw" }))?;
            "Curve"
        }
        Kind::Spiral { radius_start, radius_end, turn } => {
            let r = |r: Option<f64>| r.map_or_else(|| "INF".into(), |r| r.to_string());
            append(
                out,
                &format!(
                    "<Spiral spiType=\"clothoid\" length=\"{}\" radiusStart=\"{}\" radiusEnd=\"{}\" rot=\"{}\" dirStart=\"{}\" dirEnd=\"{}\">",
                    e.length,
                    r(radius_start),
                    r(radius_end),
                    if turn == Turn::Left { "ccw" } else { "cw" },
                    compass(e.direction),
                    compass(end.direction)
                ),
            )?;
            "Spiral"
        }
    };
    xml_point(out, "Start", e.start)?;
    match e.kind {
        Kind::Arc { radius, turn } => {
            xml_point(out, "Center", [e.start[0] - turn.sign() * radius * direction.sin(), e.start[1] + turn.sign() * radius * direction.cos()])?
        }
        Kind::Spiral { .. } => {
            let a = [direction.cos(), direction.sin()];
            let b = [end.direction.cos(), end.direction.sin()];
            let det = cross(a, b);
            if det.abs() < 1e-14 {
                return Err(error("UNREPRESENTABLE_SPIRAL", "parallel endpoint tangents have no finite LandXML PI"));
            }
            let t = cross(sub(end.point, e.start), b) / det;
            if t <= 0. {
                return Err(error("UNREPRESENTABLE_SPIRAL", "LandXML PI is behind the start tangent"));
            }
            xml_point(out, "PI", [e.start[0] + t * a[0], e.start[1] + t * a[1]])?;
        }
        _ => {}
    }
    xml_point(out, "End", end.point)?;
    append(out, &format!("</{close}>"))
}
pub fn export(alignments: &[Alignment], unit: Unit) -> Result<String> {
    if alignments.is_empty() || alignments.len() > 128 {
        return Err(error("ALIGNMENT_LIMIT", "export requires 1..128 alignments"));
    }
    let mut names = BTreeSet::new();
    let mut out = String::new();
    let family = if unit == Unit::Meter { "Metric" } else { "Imperial" };
    // Fixed epoch is deterministic; no hand-written calendar conversion or clock is needed.
    write!(&mut out,"<?xml version=\"1.0\" encoding=\"UTF-8\"?><LandXML xmlns=\"{NS}\" version=\"1.2\" date=\"1970-01-01\" time=\"00:00:00\"><Units><{family} linearUnit=\"{}\" areaUnit=\"{}\" volumeUnit=\"{}\" temperatureUnit=\"{}\" pressureUnit=\"{}\" angularUnit=\"decimal degrees\" directionUnit=\"decimal degrees\"/></Units><Alignments name=\"Alignments\">",unit.name(),if unit==Unit::Meter{"squareMeter"}else{"squareFoot"},if unit==Unit::Meter{"cubicMeter"}else{"cubicFeet"},if unit==Unit::Meter{"celsius"}else{"fahrenheit"},if unit==Unit::Meter{"milliBars"}else{"inHG"}).map_err(|_|error("XML_LIMIT","formatting failed"))?;
    let mut total = 0usize;
    for a in alignments {
        a.checked()?;
        total += a.elements.len();
        if total > MAX_ELEMENTS {
            return Err(error("ELEMENT_LIMIT", "file exceeds 4096 elements"));
        }
        if !names.insert(&a.name) {
            return Err(error("DUPLICATE_NAME", "LandXML alignment names must be unique"));
        }
        append(
            &mut out,
            &format!("<Alignment name=\"{}\" staStart=\"{}\" length=\"{}\"><CoordGeom>", escaped(&a.name), a.start_station, a.length()?),
        )?;
        for e in &a.elements {
            export_element(&mut out, e)?;
        }
        append(&mut out, "</CoordGeom></Alignment>")?;
    }
    append(&mut out, "</Alignments></LandXML>")?;
    Ok(out)
}
