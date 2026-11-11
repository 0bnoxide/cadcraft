//! Standard entity extension dictionaries keep exact route definitions with their carrier.
use crate::Result;
use cadcraft_alignment::{Alignment, MAX_ELEMENTS};
use cadcraft_doc::foreign::ForeignObject;
use cadcraft_doc::{Drawing, Handle, RawTag};
use cadcraft_doc::{EntityKind, LwPolyline};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stored {
    pub version: u32,
    pub definition: Alignment,
    pub chord_tolerance: f64,
    #[serde(default)]
    pub profiles: cadcraft_alignment::profile::Profiles,
}
pub(super) fn carrier(data: &Stored) -> Result<LwPolyline> {
    let vertices = data
        .definition
        .polyline(data.chord_tolerance)
        .map_err(|e| super::bad("alignment", e.to_string()))?
        .into_iter()
        .map(|p| PolyVertex::new(Vec2::new(p[0], p[1])))
        .collect();
    Ok(LwPolyline { vertices, closed: false, const_width: 0., elevation: 0., plinegen: false })
}
pub(super) fn load(d: &Drawing, handle: Handle) -> Result<Stored> {
    let raw = read_data(d, handle)?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|_| invalid("malformed JSON"))?;
    if value.get("definition").and_then(|v| v.get("elements")).and_then(|v| v.as_array()).is_none_or(|v| v.len() > MAX_ELEMENTS) {
        return Err(invalid("element count limit"));
    }
    if let Some(profiles) = value.get("profiles") {
        if profiles.get("design").and_then(|d| d.get("pvis")).and_then(|v| v.as_array()).is_some_and(|p| p.len() > MAX_ELEMENTS) {
            return Err(invalid("too many PVIs"));
        }
        if let Some(segments) = profiles.get("ground").and_then(|g| g.get("segments")).and_then(|v| v.as_array()) {
            let total = segments.iter().try_fold(0usize, |n, v| n.checked_add(v.as_array().map_or(0, Vec::len)));
            if segments.len() > cadcraft_alignment::MAX_POINTS || total.is_none_or(|n| n > cadcraft_alignment::MAX_POINTS) {
                return Err(invalid("too many ground samples"));
            }
        }
    }
    let data: Stored = serde_json::from_value(value).map_err(|_| invalid("invalid definition fields"))?;
    if data.version != 1 {
        return Err(invalid("unsupported storage version"));
    }
    data.profiles.validate(&data.definition).map_err(|e| invalid(&e.to_string()))?;
    let expected = carrier(&data)?;
    let e = d.entity(handle).ok_or_else(|| invalid("carrier missing"))?;
    let EntityKind::LwPolyline(actual) = &e.kind else { return Err(invalid("carrier is not a lightweight polyline")) };
    let equal = actual.vertices.len() == expected.vertices.len()
        && !actual.closed
        && actual.elevation == 0.
        && actual.const_width == 0.
        && e.common.extrusion == Vec3::Z
        && actual.vertices.iter().zip(&expected.vertices).all(|(a, b)| {
            a.bulge == 0. && a.start_width == 0. && a.end_width == 0. && (a.p.x - b.p.x).abs() <= 1e-7 && (a.p.y - b.p.y).abs() <= 1e-7
        });
    if !equal {
        return Err(super::bad("alignment", "STALE_ALIGNMENT: carrier geometry differs from its exact definition"));
    }
    Ok(data)
}
pub(super) fn save(d: &mut Drawing, handle: Handle, data: &Stored) -> Result<()> {
    data.profiles.validate(&data.definition).map_err(|e| invalid(&e.to_string()))?;
    // Serialization is capped while writing, before an input-sized string can be allocated.
    struct Capped(Vec<u8>);
    impl std::io::Write for Capped {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(b.len()) > MAX_DATA {
                return Err(std::io::Error::other("alignment data exceeds 8MiB"));
            }
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buf = Capped(Vec::new());
    serde_json::to_writer(&mut buf, data).map_err(|_| invalid("data exceeds 8MiB or cannot be serialized"))?;
    let utf8 = String::from_utf8(buf.0).map_err(|_| invalid("invalid JSON encoding"))?;
    let mut ascii = String::new();
    for c in utf8.chars() {
        if ascii.len().saturating_add(12) > MAX_DATA {
            return Err(invalid("ASCII JSON exceeds 8MiB"));
        }
        if c.is_ascii() {
            ascii.push(c)
        } else {
            let mut units = [0; 2];
            for u in c.encode_utf16(&mut units) {
                use std::fmt::Write;
                write!(&mut ascii, "\\u{u:04x}").map_err(|_| invalid("encoding failed"))?;
            }
        }
    }
    write_data(d, handle, &ascii)
}

const KEY: &str = "CADCRAFT_ALIGNMENT";
const MAX_DATA: usize = 8 * 1024 * 1024;
fn tag(code: i32, value: impl ToString) -> RawTag {
    RawTag { code, value: value.to_string() }
}
fn invalid(reason: &str) -> crate::EngineError {
    super::bad("alignment", format!("INVALID_ALIGNMENT_DATA: {reason}"))
}
fn record_handle(d: &Drawing, handle: Handle) -> Result<Option<Handle>> {
    let Some(dict) = d.foreign.entities.get(&handle).and_then(|x| x.xdict) else { return Ok(None) };
    let dict =
        d.foreign.objects.iter().find(|o| o.handle == dict && o.kind == "DICTIONARY").ok_or_else(|| invalid("missing extension dictionary"))?;
    let mut out = None;
    for pair in dict.tags.windows(2) {
        if let [key, value] = pair
            && key.code == 3
            && key.value == KEY
        {
            if out.is_some() || !matches!(value.code, 350 | 360) {
                return Err(invalid("duplicate or malformed alignment entry"));
            }
            out = Some(Handle::parse_hex(&value.value).ok_or_else(|| invalid("invalid XRECORD handle"))?);
        }
    }
    Ok(out)
}
fn write_data(d: &mut Drawing, handle: Handle, data: &str) -> Result<()> {
    if data.is_empty() || data.len() > MAX_DATA || !data.is_ascii() {
        return Err(invalid("ASCII JSON payload exceeds 8MiB or is empty"));
    }
    if d.entity(handle).is_none() {
        return Err(invalid("carrier does not exist"));
    }
    let existing = record_handle(d, handle)?;
    if let Some(record) = existing {
        let f = Arc::make_mut(&mut d.foreign);
        let obj = f.objects.iter_mut().find(|o| o.handle == record && o.kind == "XRECORD").ok_or_else(|| invalid("missing alignment XRECORD"))?;
        obj.tags.retain(|t| t.code != 1);
        obj.tags.extend(data.as_bytes().chunks(240).map(|b| tag(1, String::from_utf8_lossy(b))));
        return Ok(());
    }
    let maximum = d.foreign.objects.iter().map(|o| o.handle.0).max().unwrap_or(0);
    let next = d.handseed.max(maximum.saturating_add(1));
    if next > u64::MAX - 3 {
        return Err(invalid("handle space exhausted"));
    }
    d.handseed = next;
    let old_dict = d.foreign.entities.get(&handle).and_then(|x| x.xdict);
    let dict = old_dict.unwrap_or_else(|| d.new_handle());
    let record = d.new_handle();
    let f = Arc::make_mut(&mut d.foreign);
    if old_dict.is_none() {
        f.objects.push(ForeignObject {
            handle: dict,
            kind: "DICTIONARY".into(),
            tags: vec![tag(5, dict.hex()), tag(330, handle.hex()), tag(100, "AcDbDictionary"), tag(280, 1), tag(281, 1)],
        });
        f.entities.entry(handle).or_default().xdict = Some(dict);
    }
    let object = f.objects.iter_mut().find(|o| o.handle == dict).ok_or_else(|| invalid("missing extension dictionary"))?;
    object.tags.extend([tag(3, KEY), tag(360, record.hex())]);
    let mut tags = vec![tag(5, record.hex()), tag(330, dict.hex()), tag(100, "AcDbXrecord"), tag(280, 1)];
    tags.extend(data.as_bytes().chunks(240).map(|b| tag(1, String::from_utf8_lossy(b))));
    f.objects.push(ForeignObject { handle: record, kind: "XRECORD".into(), tags });
    Ok(())
}
fn read_data(d: &Drawing, handle: Handle) -> Result<String> {
    let record = record_handle(d, handle)?.ok_or_else(|| invalid("object is not an alignment"))?;
    let obj = d.foreign.objects.iter().find(|o| o.handle == record && o.kind == "XRECORD").ok_or_else(|| invalid("missing alignment XRECORD"))?;
    let mut data = String::new();
    for t in obj.tags.iter().filter(|t| t.code == 1) {
        if !t.value.is_ascii() || t.value.len() > 240 || data.len().saturating_add(t.value.len()) > MAX_DATA {
            return Err(invalid("invalid or oversized JSON chunk"));
        }
        data.push_str(&t.value);
    }
    if data.is_empty() {
        return Err(invalid("empty alignment data"));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{Common, EntityKind, LwPolyline, Space};
    use cadcraft_geom::{PolyVertex, Vec2};

    #[test]
    fn rewrite_preserves_foreign_dictionary_entries() {
        let mut d = Drawing::new_metric();
        let h = d.add(&Space::Model, Common::default(), EntityKind::Point(cadcraft_doc::Point { p: cadcraft_geom::Vec3::ZERO, angle: 0. })).unwrap();
        write_data(&mut d, h, "{\"version\":1}").unwrap();
        let dict = d.foreign.entities.get(&h).unwrap().xdict.unwrap();
        let foreign = d.new_handle();
        let object = ForeignObject {
            handle: foreign,
            kind: "XRECORD".into(),
            tags: vec![tag(5, foreign.hex()), tag(330, dict.hex()), tag(100, "AcDbXrecord"), tag(280, 1), tag(1, "user data")],
        };
        let f = Arc::make_mut(&mut d.foreign);
        f.objects.iter_mut().find(|o| o.handle == dict).unwrap().tags.extend([tag(3, "OTHER_APP"), tag(360, foreign.hex())]);
        f.objects.push(object.clone());
        write_data(&mut d, h, "{\"version\":2}").unwrap();
        assert_eq!(d.foreign.objects.iter().find(|o| o.handle == foreign).unwrap(), &object);
        assert!(d.foreign.objects.iter().find(|o| o.handle == dict).unwrap().tags.iter().any(|t| t.code == 3 && t.value == "OTHER_APP"));
        assert_eq!(read_data(&d, h).unwrap(), "{\"version\":2}");
    }

    #[test]
    fn large_xrecord_survives_dxf_and_dwg_reopen() {
        let mut d = Drawing::new_metric();
        let h = d
            .add(
                &Space::Model,
                Common::default(),
                EntityKind::LwPolyline(LwPolyline {
                    vertices: vec![PolyVertex::new(Vec2::ZERO), PolyVertex::new(Vec2::new(10., 0.))],
                    closed: false,
                    const_width: 0.,
                    elevation: 0.,
                    plinegen: false,
                }),
            )
            .unwrap();
        // Exceeds the entire 16KiB xdata budget, forcing an XRECORD rather than xdata.
        let data = format!("{{\"name\":\"\\u8def\\u7ebf\",\"data\":\"{}\"}}", "0123456789".repeat(4000));
        write_data(&mut d, h, &data).unwrap();
        for file in ["alignment.dxf", "alignment.dwg"] {
            let bytes = cadcraft_io::write(&d, file).unwrap();
            let reopened = cadcraft_io::read(&bytes, file).unwrap();
            let carrier = reopened.model.iter().find(|e| matches!(e.kind, EntityKind::LwPolyline(_))).unwrap().handle;
            assert_eq!(read_data(&reopened, carrier).unwrap(), data, "{file}");
            let bytes = cadcraft_io::write(&reopened, file).unwrap();
            let twice = cadcraft_io::read(&bytes, file).unwrap();
            let carrier = twice.model.iter().find(|e| matches!(e.kind, EntityKind::LwPolyline(_))).unwrap().handle;
            assert_eq!(read_data(&twice, carrier).unwrap(), data, "second {file}");
        }
    }
}
