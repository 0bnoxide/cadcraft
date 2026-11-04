//! Streaming reader for the one LandXML 1.2 TIN surface a package carries.
//!
//! The document must be well-formed, carry no document type declaration, and hold exactly one
//! named `Surface` with one `TIN` `Definition` of points (`P id="n">northing easting elevation`)
//! and faces (`F>id id id`). Problems are reported with the first issue found, in document order.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::{NsReader, XmlVersion};

use crate::{Issue, LinearUnit, StageError, codes};

const NS: &str = "http://www.landxml.org/schema/LandXML-1.2";
const MAX_TOKENS: usize = 4;
const MAX_TOKEN_LEN: usize = 128;

/// One TIN vertex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TinPoint {
    pub id: u64,
    pub northing: f64,
    pub easting: f64,
    pub elevation: f64,
}

/// A validated TIN surface: points in document order, faces as point ids.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TinSurface {
    pub name: String,
    pub epsg_code: i32,
    pub points: Vec<TinPoint>,
    pub faces: Vec<[u64; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerticalFamily {
    Metre,
    Foot,
}

#[derive(Debug)]
pub(crate) struct Summary {
    pub surface_name: String,
    pub point_count: u64,
    pub face_count: u64,
    pub epsg_code: i32,
    pub horizontal_unit: LinearUnit,
    pub vertical_family: VerticalFamily,
    pub geometry: Option<TinSurface>,
}

/// Why parsing stopped early.
enum Stop {
    Malformed,
    Doctype,
    Stage(StageError),
}

impl From<quick_xml::Error> for Stop {
    fn from(e: quick_xml::Error) -> Self {
        match e {
            quick_xml::Error::Io(io) => Stop::from_io(&io),
            _ => Stop::Malformed,
        }
    }
}

impl Stop {
    fn from_io(e: &io::Error) -> Self {
        if let Some(issue) = crate::archive::fault_of(e) {
            return Stop::Stage(StageError::Entry(issue));
        }
        if e.get_ref().is_some_and(|inner| inner.is::<Forbidden>()) {
            return Stop::Doctype;
        }
        Stop::Stage(StageError::Io(io::Error::new(e.kind(), e.to_string())))
    }
}

/// Marker error: the byte stream contains `<!DOCTYPE`.
#[derive(Debug)]
struct Forbidden;

impl std::fmt::Display for Forbidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LandXML document type declarations are forbidden.")
    }
}

impl std::error::Error for Forbidden {}

/// Passes bytes through and fails as soon as `<!DOCTYPE` appears in any common encoding, so a
/// DTD is refused even where the XML parser would fail on something else first.
struct DoctypeScanner<R> {
    inner: R,
    tail: Vec<u8>,
    found: bool,
    patterns: Vec<Vec<u8>>,
}

impl<R: Read> DoctypeScanner<R> {
    fn new(inner: R) -> Self {
        let text = "<!DOCTYPE";
        let utf16le: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let utf16be: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let utf32le: Vec<u8> = text.chars().flat_map(|c| u32::from(c).to_le_bytes()).collect();
        let utf32be: Vec<u8> = text.chars().flat_map(|c| u32::from(c).to_be_bytes()).collect();
        Self { inner, tail: Vec::new(), found: false, patterns: vec![text.as_bytes().to_vec(), utf16le, utf16be, utf32le, utf32be] }
    }
}

impl<R: Read> Read for DoctypeScanner<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.found {
            return Err(io::Error::other(Forbidden));
        }
        let n = self.inner.read(buf)?;
        let fresh = buf.get(..n).unwrap_or_default();
        let mut window = std::mem::take(&mut self.tail);
        window.extend_from_slice(fresh);
        if self.patterns.iter().any(|p| window.windows(p.len()).any(|w| w == p.as_slice())) {
            self.found = true;
            return Err(io::Error::other(Forbidden));
        }
        let keep = 36usize.min(window.len());
        self.tail = window.split_off(window.len() - keep);
        Ok(n)
    }
}

/// Parses the surface stream. `Ok(Err(issue))` is an invalid surface.
pub(crate) fn parse<R: Read>(input: R, keep_geometry: bool) -> Result<Result<Summary, Issue>, StageError> {
    let mut buffered = BufReader::new(DoctypeScanner::new(input));
    match skip_bom(&mut buffered) {
        Ok(()) => {}
        Err(e) => {
            return match Stop::from_io(&e) {
                Stop::Stage(stage) => Err(stage),
                Stop::Doctype => Ok(Err(doctype())),
                Stop::Malformed => Ok(Err(malformed())),
            };
        }
    }
    let mut reader = NsReader::from_reader(buffered);
    {
        let config = reader.config_mut();
        config.check_end_names = true;
        config.check_comments = true;
        config.expand_empty_elements = false;
    }
    let mut state = State { keep_geometry, ..State::default() };
    match run(&mut reader, &mut state) {
        Ok(()) => Ok(state.finish()),
        Err(Stop::Malformed) => Ok(Err(malformed())),
        Err(Stop::Doctype) => Ok(Err(doctype())),
        Err(Stop::Stage(stage)) => Err(stage),
    }
}

fn skip_bom<R: BufRead>(reader: &mut R) -> io::Result<()> {
    let has_bom = reader.fill_buf()?.starts_with(b"\xEF\xBB\xBF");
    if has_bom {
        reader.consume(3);
    }
    Ok(())
}

fn malformed() -> Issue {
    Issue::error(codes::LANDXML_MALFORMED, "surface.landxml is not well-formed XML.")
}

fn doctype() -> Issue {
    Issue::error(codes::LANDXML_FORBIDDEN_DTD, "LandXML document type declarations are forbidden.")
}

#[derive(Default)]
struct State {
    keep_geometry: bool,
    /// Open elements as (local name, in the LandXML namespace).
    ancestors: Vec<(String, bool)>,
    root_seen: bool,
    root_closed: bool,
    points: HashMap<u64, TinPoint>,
    ordered: Vec<TinPoint>,
    faces: Vec<[u64; 3]>,
    issues: Vec<Issue>,
    surface_name: Option<String>,
    epsg_code: i32,
    horizontal_unit: Option<LinearUnit>,
    vertical_family: Option<VerticalFamily>,
    surface_count: u64,
    canonical_surface_count: u64,
    definition_count: u64,
    point_count: u64,
    face_count: u64,
    coordinate_system_count: u32,
    unit_count: u32,
    envelope_invalid: bool,
    definition_invalid: bool,
    surface_invalid: bool,
}

impl State {
    fn path_is(&self, path: &[&str]) -> bool {
        self.ancestors.len() == path.len() && self.ancestors.iter().zip(path).all(|((name, in_ns), want)| *in_ns && name == want)
    }

    fn add_once(&mut self, code: &'static str, message: &str) {
        if !self.issues.iter().any(|i| i.code == code) {
            self.issues.push(Issue::error(code, message));
        }
    }

    fn finish(self) -> Result<Summary, Issue> {
        let envelope_bad = self.envelope_invalid
            || self.unit_count != 1
            || self.coordinate_system_count != 1
            || self.horizontal_unit.is_none()
            || self.vertical_family.is_none()
            || self.epsg_code <= 0;
        if envelope_bad {
            return Err(Issue::error(
                codes::LANDXML_UNSUPPORTED_VERSION,
                "LandXML must use the supported 1.2 envelope, units, and EPSG declaration.",
            ));
        }
        let Some(name) = self.surface_name.filter(|_| self.surface_count == 1 && self.canonical_surface_count == 1 && !self.surface_invalid) else {
            return Err(Issue::error(codes::LANDXML_INVALID_SURFACE_COUNT, "LandXML must contain exactly one named surface."));
        };
        if self.definition_count != 1 || self.definition_invalid {
            return Err(Issue::error(codes::LANDXML_INVALID_DEFINITION_COUNT, "The surface must contain exactly one TIN definition."));
        }
        if self.point_count == 0 {
            return Err(Issue::error(codes::LANDXML_INVALID_POINT, "The TIN definition must contain valid points."));
        }
        if let Some(first) = self.issues.into_iter().next() {
            return Err(first);
        }
        if self.face_count == 0 {
            return Err(Issue::error(codes::LANDXML_INVALID_FACE, "The TIN definition must contain faces."));
        }
        let (Some(horizontal_unit), Some(vertical_family)) = (self.horizontal_unit, self.vertical_family) else {
            return Err(Issue::error(
                codes::LANDXML_UNSUPPORTED_VERSION,
                "LandXML must use the supported 1.2 envelope, units, and EPSG declaration.",
            ));
        };
        let geometry =
            self.keep_geometry.then(|| TinSurface { name: name.clone(), epsg_code: self.epsg_code, points: self.ordered, faces: self.faces });
        Ok(Summary {
            surface_name: name,
            point_count: self.point_count,
            face_count: self.face_count,
            epsg_code: self.epsg_code,
            horizontal_unit,
            vertical_family,
            geometry,
        })
    }
}

/// Resolves an element's namespace. An undeclared prefix is a well-formedness error.
fn element_name<R>(reader: &NsReader<R>, e: &BytesStart<'_>) -> Result<(String, bool), Stop> {
    let (ns, local) = reader.resolver().resolve_element(e.name());
    let in_ns = match ns {
        ResolveResult::Bound(ns) => ns.as_ref() == NS,
        ResolveResult::Unbound => false,
        ResolveResult::Unknown(_) => return Err(Stop::Malformed),
    };
    let local = local.as_ref().to_owned();
    Ok((local, in_ns))
}

/// Checks every attribute is well-formed and returns the unprefixed ones by name.
fn attributes(e: &BytesStart<'_>) -> Result<Vec<(String, String)>, Stop> {
    let mut out = Vec::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|_| Stop::Malformed)?;
        let value = attr.normalized_value(XmlVersion::Explicit1_0).map_err(|_| Stop::Malformed)?.into_owned();
        if attr.key.prefix().is_none() {
            let key = attr.key.as_ref().to_owned();
            out.push((key, value));
        }
    }
    Ok(out)
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// Text of an entity or character reference, or `None` if it is not allowed here.
fn reference(r: &quick_xml::events::BytesRef<'_>) -> Option<String> {
    if r.is_char_ref() {
        return r.resolve_char_ref().ok().flatten().map(String::from);
    }
    quick_xml::escape::resolve_predefined_entity(r).map(str::to_owned)
}

fn is_xml_space(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}

fn run<R: BufRead>(reader: &mut NsReader<R>, state: &mut State) -> Result<(), Stop> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let event = reader.read_event_into(&mut buf)?;
        let outside = state.ancestors.is_empty();
        match event {
            Event::Decl(decl) => {
                let version = decl.version().map_err(|_| Stop::Malformed)?;
                if version.as_ref() != "1.0" || state.root_seen {
                    return Err(Stop::Malformed);
                }
            }
            Event::DocType(_) => return Err(Stop::Doctype),
            Event::PI(_) | Event::Comment(_) => {}
            Event::Text(t) => {
                if outside && !is_xml_space(&t) {
                    return Err(Stop::Malformed);
                }
            }
            Event::CData(_) => {
                if outside {
                    return Err(Stop::Malformed);
                }
            }
            Event::GeneralRef(r) => {
                if outside || reference(&r).is_none() {
                    return Err(Stop::Malformed);
                }
            }
            Event::Start(_) | Event::Empty(_) if state.root_closed => return Err(Stop::Malformed),
            Event::Start(e) => {
                let e = e.into_owned();
                element(reader, state, &e, false)?;
            }
            Event::Empty(e) => {
                let e = e.into_owned();
                element(reader, state, &e, true)?;
                if state.ancestors.is_empty() {
                    state.root_closed = true;
                }
            }
            Event::End(_) => {
                if state.ancestors.pop().is_none() {
                    return Err(Stop::Malformed);
                }
                if state.ancestors.is_empty() {
                    state.root_closed = true;
                }
            }
            Event::Eof => {
                if !state.root_seen || !state.ancestors.is_empty() {
                    return Err(Stop::Malformed);
                }
                return Ok(());
            }
        }
    }
}

fn element<R: BufRead>(reader: &mut NsReader<R>, state: &mut State, e: &BytesStart<'_>, empty: bool) -> Result<(), Stop> {
    let (local, in_ns) = element_name(reader, e)?;
    let attrs = attributes(e)?;
    if !state.root_seen {
        state.root_seen = true;
        state.envelope_invalid = !(in_ns && local == "LandXML" && attr(&attrs, "version") == Some("1.2"));
    }
    let is = |name: &str| in_ns && local == name;
    if is("Metric") || is("Imperial") {
        if state.path_is(&["LandXML", "Units"]) {
            state.unit_count += 1;
            let horizontal = match attr(&attrs, "linearUnit") {
                Some("meter") => Some(LinearUnit::Metre),
                Some("foot") => Some(LinearUnit::InternationalFoot),
                Some("USSurveyFoot") => Some(LinearUnit::UsSurveyFoot),
                _ => None,
            };
            let vertical = match attr(&attrs, "elevationUnit") {
                Some("meter") => Some(VerticalFamily::Metre),
                Some("feet") => Some(VerticalFamily::Foot),
                _ => None,
            };
            match (horizontal, vertical) {
                (Some(h), Some(v)) => {
                    state.horizontal_unit = Some(h);
                    state.vertical_family = Some(v);
                }
                _ => state.envelope_invalid = true,
            }
        }
    } else if is("CoordinateSystem") {
        if state.path_is(&["LandXML"]) {
            state.coordinate_system_count += 1;
            let code = attr(&attrs, "epsgCode").and_then(positive_int).and_then(|n| i32::try_from(n).ok());
            if code.is_none() {
                state.envelope_invalid = true;
            }
            state.epsg_code = code.unwrap_or_default();
        }
    } else if is("Surface") {
        state.surface_count += 1;
        if state.path_is(&["LandXML", "Surfaces"]) {
            state.canonical_surface_count += 1;
            match attr(&attrs, "name").map(str::trim).filter(|n| !n.is_empty()) {
                None => state.surface_invalid = true,
                Some(name) => {
                    if state.surface_name.is_none() {
                        state.surface_name = Some(name.to_owned());
                    }
                }
            }
        }
    } else if is("Definition") && state.path_is(&["LandXML", "Surfaces", "Surface"]) {
        state.definition_count += 1;
        if attr(&attrs, "surfType") != Some("TIN") {
            state.definition_invalid = true;
        }
    } else if is("P") && state.path_is(&["LandXML", "Surfaces", "Surface", "Definition", "Pnts"]) {
        let leaf = read_leaf(reader, empty)?;
        state.surface_count += leaf.surfaces;
        if leaf.has_child {
            state.add_once(codes::LANDXML_INVALID_POINT, "A TIN point is malformed.");
        } else {
            point(state, attr(&attrs, "id"), &leaf);
        }
        return Ok(());
    } else if is("F") && state.path_is(&["LandXML", "Surfaces", "Surface", "Definition", "Faces"]) {
        let leaf = read_leaf(reader, empty)?;
        state.surface_count += leaf.surfaces;
        state.face_count += 1;
        if leaf.has_child {
            state.add_once(codes::LANDXML_INVALID_FACE, "A TIN face is malformed.");
        } else {
            face(state, &leaf);
        }
        return Ok(());
    }
    if !empty {
        state.ancestors.push((local, in_ns));
    }
    Ok(())
}

/// The whitespace-separated tokens directly inside a `P` or `F` element.
#[derive(Default)]
struct Leaf {
    tokens: Vec<String>,
    invalid: bool,
    has_child: bool,
    surfaces: u64,
    current: String,
    started: bool,
}

impl Leaf {
    fn append(&mut self, text: &str) {
        for c in text.chars() {
            if c.is_whitespace() {
                self.complete();
                continue;
            }
            self.started = true;
            if self.current.chars().count() < MAX_TOKEN_LEN {
                self.current.push(c);
            } else {
                self.invalid = true;
            }
        }
    }

    fn complete(&mut self) {
        if !self.started {
            return;
        }
        if self.tokens.len() < MAX_TOKENS {
            self.tokens.push(std::mem::take(&mut self.current));
        } else {
            self.invalid = true;
            self.current.clear();
        }
        self.started = false;
    }
}

fn read_leaf<R: BufRead>(reader: &mut NsReader<R>, empty: bool) -> Result<Leaf, Stop> {
    let mut leaf = Leaf::default();
    if empty {
        return Ok(leaf);
    }
    let mut depth = 1usize;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let e = e.into_owned();
                child(reader, &mut leaf, &e)?;
                depth += 1;
            }
            Event::Empty(e) => {
                let e = e.into_owned();
                child(reader, &mut leaf, &e)?;
            }
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    leaf.complete();
                    return Ok(leaf);
                }
            }
            Event::Text(t) => {
                if depth == 1 {
                    leaf.append(&t);
                }
            }
            Event::CData(c) => {
                if depth == 1 {
                    leaf.append(&c);
                }
            }
            Event::GeneralRef(r) => {
                let text = reference(&r).ok_or(Stop::Malformed)?;
                if depth == 1 {
                    leaf.append(&text);
                }
            }
            Event::Comment(_) | Event::PI(_) => {}
            Event::DocType(_) => return Err(Stop::Doctype),
            Event::Decl(_) | Event::Eof => return Err(Stop::Malformed),
        }
    }
}

fn child<R>(reader: &NsReader<R>, leaf: &mut Leaf, e: &BytesStart<'_>) -> Result<(), Stop> {
    let (local, in_ns) = element_name(reader, e)?;
    attributes(e)?;
    leaf.has_child = true;
    if in_ns && local == "Surface" {
        leaf.surfaces += 1;
    }
    Ok(())
}

/// A positive integer written with digits only.
fn positive_int(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok().filter(|n| *n > 0).and_then(|n| u64::try_from(n).ok())
}

/// A decimal number as the reference validator reads it (`Infinity`/`NaN` spelled out are
/// numbers, though not finite ones; Rust's `inf` spelling is not).
fn number(text: &str) -> Option<f64> {
    let unsigned = text.trim_start_matches(['+', '-']);
    let lower = unsigned.to_ascii_lowercase();
    if lower == "inf" {
        return None;
    }
    if unsigned == "∞" {
        return Some(if text.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY });
    }
    if text.len() - unsigned.len() > 1 {
        return None;
    }
    text.parse::<f64>().ok()
}

fn point(state: &mut State, id: Option<&str>, leaf: &Leaf) {
    let parsed = match (id.and_then(positive_int), leaf.invalid, leaf.tokens.as_slice()) {
        (Some(id), false, [n, e, z]) => match (number(n), number(e), number(z)) {
            (Some(n), Some(e), Some(z)) => Some(TinPoint { id, northing: n, easting: e, elevation: z }),
            _ => None,
        },
        _ => None,
    };
    let Some(p) = parsed else {
        state.add_once(codes::LANDXML_INVALID_POINT, "A TIN point is malformed.");
        return;
    };
    if state.points.contains_key(&p.id) {
        state.add_once(codes::LANDXML_DUPLICATE_POINT_ID, "TIN point identifiers must be unique.");
        return;
    }
    if !(p.northing.is_finite() && p.easting.is_finite() && p.elevation.is_finite()) {
        state.add_once(codes::LANDXML_NONFINITE_COORDINATE, "TIN point coordinates must be finite.");
        return;
    }
    state.points.insert(p.id, p);
    if state.keep_geometry {
        state.ordered.push(p);
    }
    state.point_count += 1;
}

fn face(state: &mut State, leaf: &Leaf) {
    let ids = match (leaf.invalid, leaf.tokens.as_slice()) {
        (false, [a, b, c]) => match (positive_int(a), positive_int(b), positive_int(c)) {
            (Some(a), Some(b), Some(c)) => Some([a, b, c]),
            _ => None,
        },
        _ => None,
    };
    let Some([a, b, c]) = ids else {
        state.add_once(codes::LANDXML_INVALID_FACE, "A TIN face is malformed.");
        return;
    };
    let (pa, pb, pc) = (state.points.get(&a).copied(), state.points.get(&b).copied(), state.points.get(&c).copied());
    let repeated = a == b || b == c || c == a;
    let (Some(pa), Some(pb), Some(pc)) = (pa, pb, pc) else {
        state.add_once(codes::LANDXML_UNKNOWN_POINT_REFERENCE, "A TIN face references an unknown point.");
        if repeated {
            state.add_once(codes::LANDXML_REPEATED_FACE_VERTEX, "A TIN face must reference three distinct points.");
        }
        return;
    };
    if repeated {
        state.add_once(codes::LANDXML_REPEATED_FACE_VERTEX, "A TIN face must reference three distinct points.");
        return;
    }
    if degenerate(pa, pb, pc) {
        state.add_once(codes::LANDXML_DEGENERATE_FACE, "A TIN face has zero or near-zero horizontal area.");
    }
    if state.keep_geometry {
        state.faces.push([a, b, c]);
    }
}

/// 2^floor(log2(x)) for finite non-zero `x` (exact, also for subnormals).
fn power_of_two_below(x: f64) -> f64 {
    let bits = x.abs().to_bits();
    let exponent = (bits >> 52) & 0x7ff;
    if exponent == 0 {
        // subnormal: keep only the highest set mantissa bit
        let mantissa = bits & ((1u64 << 52) - 1);
        let top = 63 - mantissa.leading_zeros();
        f64::from_bits(1u64 << top)
    } else {
        f64::from_bits(exponent << 52)
    }
}

/// Zero or near-zero horizontal area, computed on coordinates scaled by a power of two so the
/// test is independent of the coordinates' magnitude.
fn degenerate(a: TinPoint, b: TinPoint, c: TinPoint) -> bool {
    let max = [a.northing, a.easting, b.northing, b.easting, c.northing, c.easting].iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let scale = if max == 0.0 { 1.0 } else { power_of_two_below(max) };
    let d = |l: f64, r: f64| (l / scale) - (r / scale);
    let (be, bn) = (d(b.easting, a.easting), d(b.northing, a.northing));
    let (ce, cn) = (d(c.easting, a.easting), d(c.northing, a.northing));
    let cross = be * cn - bn * ce;
    let sq = |p: TinPoint, q: TinPoint| {
        let (n, e) = (d(q.northing, p.northing), d(q.easting, p.easting));
        n * n + e * e
    };
    let max_edge = sq(a, b).max(sq(b, c)).max(sq(c, a));
    !cross.is_finite() || !max_edge.is_finite() || max_edge == 0.0 || cross.abs() <= 1e-12 * max_edge
}

#[cfg(test)]
mod tests {
    use super::{number, positive_int, power_of_two_below};

    #[test]
    fn numbers() {
        assert_eq!(number("1.5"), Some(1.5));
        assert_eq!(number("-2e3"), Some(-2000.0));
        assert!(number("Infinity").is_some_and(f64::is_infinite));
        assert!(number("NaN").is_some_and(f64::is_nan));
        assert_eq!(number("inf"), None);
        assert_eq!(number("1,5"), None);
        assert_eq!(positive_int("007"), Some(7));
        assert_eq!(positive_int("+7"), None);
        assert_eq!(positive_int("0"), None);
    }

    #[test]
    fn scale() {
        assert_eq!(power_of_two_below(10.0), 8.0);
        assert_eq!(power_of_two_below(1.0), 1.0);
        assert_eq!(power_of_two_below(0.75), 0.5);
        assert_eq!(power_of_two_below(f64::from_bits(5)), f64::from_bits(4));
    }
}
