//! LandXML 1.2 surface export (Pnts/Faces). Owner: Claude (see `docs/agents/slice-2-tin.md`).
//!
//! One named TIN `Surface` with explicit `Units` and an EPSG `CoordinateSystem`, points as
//! `northing easting elevation` with 1-based ids, the shape the AutoGIS LandXML handoff
//! contract (v1) and Civil 3D import expect.

use std::fmt::{self, Write as _};

use crate::Tin;
use crate::triangulate::signed_area2;

/// The LandXML linear unit (horizontal and vertical are the same family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearUnit {
    Meter,
    Foot,
    UsSurveyFoot,
}

impl LinearUnit {
    /// Parses the LandXML name (`meter`, `foot`, `USSurveyFoot`) or the handoff name (`metre`,
    /// `international_foot`, `us_survey_foot`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "meter" | "metre" | "m" => Some(Self::Meter),
            "foot" | "international_foot" | "ft" => Some(Self::Foot),
            "USSurveyFoot" | "us_survey_foot" | "usft" => Some(Self::UsSurveyFoot),
            _ => None,
        }
    }
    /// The handoff manifest's name for the unit.
    pub fn handoff_name(self) -> &'static str {
        match self {
            Self::Meter => "metre",
            Self::Foot => "international_foot",
            Self::UsSurveyFoot => "us_survey_foot",
        }
    }
}

/// What to write besides the TIN.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportOptions {
    pub name: String,
    pub unit: LinearUnit,
    pub epsg: u32,
    /// Seconds since 1970 (UTC) for the root's `date`/`time`; omitted when `None`.
    pub unix_time: Option<u64>,
}

/// Why a TIN could not be exported.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportError(pub String);

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExportError {}

fn escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&apos;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// `YYYY-MM-DD` and `hh:mm:ss` (UTC) for seconds since 1970; `None` past chrono's range.
fn date_time(secs: u64) -> Option<(String, String)> {
    let t = chrono::DateTime::from_timestamp(i64::try_from(secs).ok()?, 0)?;
    Some((t.format("%Y-%m-%d").to_string(), t.format("%H:%M:%S").to_string()))
}

/// The TIN as a LandXML 1.2 document. Every vertex is written (ids are 1-based vertex indices);
/// the TIN must have a face, finite coordinates and no face without area seen from above.
pub fn export(tin: &Tin, opts: &ExportOptions) -> Result<String, ExportError> {
    let e = |m: String| ExportError(m);
    let name = opts.name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(e("the surface name must be 1 to 100 characters with no control characters".into()));
    }
    if opts.epsg == 0 {
        return Err(e("an EPSG code is required".into()));
    }
    if tin.triangles.is_empty() {
        return Err(e("the surface has no triangles".into()));
    }
    if let Some(i) = tin.vertices.iter().position(|v| !v.iter().all(|c| c.is_finite())) {
        return Err(e(format!("vertex {} has a non-finite coordinate", i + 1)));
    }
    let (linear, elevation, system, extra) = match opts.unit {
        LinearUnit::Meter => {
            ("meter", "meter", "Metric", r#"areaUnit="squareMeter" volumeUnit="cubicMeter" temperatureUnit="celsius" pressureUnit="milliBars""#)
        }
        LinearUnit::Foot => {
            ("foot", "feet", "Imperial", r#"areaUnit="squareFoot" volumeUnit="cubicFeet" temperatureUnit="fahrenheit" pressureUnit="inHG""#)
        }
        LinearUnit::UsSurveyFoot => {
            ("USSurveyFoot", "feet", "Imperial", r#"areaUnit="squareFoot" volumeUnit="cubicFeet" temperatureUnit="fahrenheit" pressureUnit="inHG""#)
        }
    };
    let stamp = opts.unix_time.and_then(date_time).map(|(d, t)| format!(r#" date="{d}" time="{t}""#)).unwrap_or_default();
    let mut x = String::with_capacity(200 + tin.vertices.len() * 48 + tin.triangles.len() * 24);
    // Writing to a String cannot fail.
    let _ = write!(
        x,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<LandXML xmlns=\"http://www.landxml.org/schema/LandXML-1.2\" version=\"1.2\"{stamp}>\n  <Units>\n    <{system} linearUnit=\"{linear}\" elevationUnit=\"{elevation}\" {extra}/>\n  </Units>\n  <CoordinateSystem name=\"EPSG:{epsg}\" epsgCode=\"{epsg}\"/>\n  <Surfaces>\n    <Surface name=\"{name}\">\n      <Definition surfType=\"TIN\">\n        <Pnts>\n",
        epsg = opts.epsg,
        name = escape(name),
    );
    for (i, v) in tin.vertices.iter().enumerate() {
        let _ = writeln!(x, "          <P id=\"{}\">{} {} {}</P>", i + 1, v[1], v[0], v[2]);
    }
    x.push_str("        </Pnts>\n        <Faces>\n");
    for (fi, t) in tin.triangles.iter().enumerate() {
        let p = t.map(|k| tin.vertices.get(k as usize).copied());
        let [Some(a), Some(b), Some(c)] = p else {
            return Err(e(format!("triangle {} names a vertex that does not exist", fi + 1)));
        };
        if signed_area2(a, b, c).1 || t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            return Err(e(format!("triangle {} has no area seen from above", fi + 1)));
        }
        let _ = writeln!(x, "          <F>{} {} {}</F>", u64::from(t[0]) + 1, u64::from(t[1]) + 1, u64::from(t[2]) + 1);
    }
    x.push_str("        </Faces>\n      </Definition>\n    </Surface>\n  </Surfaces>\n</LandXML>\n");
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_from_unix_seconds() {
        assert_eq!(date_time(0), Some(("1970-01-01".into(), "00:00:00".into())));
        assert_eq!(date_time(1_793_784_896), Some(("2026-11-04".into(), "09:34:56".into())));
        assert_eq!(date_time(u64::MAX), None);
    }
}
