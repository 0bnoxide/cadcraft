//! Checkpoint coordinates become ordinary QA report metrics, in feet.
use crate::{ParseResult, QAMetrics};

fn fields(line: &str) -> Result<Vec<String>, String> {
    let invalid = || "INVALID_CHECKPOINT_CSV: malformed CSV row".to_string();
    let mut out = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (at, byte) in line.bytes().enumerate().chain(std::iter::once((line.len(), b','))) {
        if byte == b'"' {
            quoted = !quoted;
        }
        if byte != b',' || quoted {
            continue;
        }
        if out.len() >= 512 {
            return Err(invalid());
        }
        let raw = line.get(start..at).ok_or_else(invalid)?.trim();
        let value = if raw.starts_with('"') {
            let inner = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')).ok_or_else(invalid)?;
            if inner.replace("\"\"", "").contains('"') {
                return Err(invalid());
            }
            inner.replace("\"\"", "\"")
        } else {
            if raw.contains('"') {
                return Err(invalid());
            }
            raw.to_string()
        };
        out.push(value);
        start = at + 1;
    }
    if quoted {
        return Err(invalid());
    }
    Ok(out)
}

pub fn parse(text: &str, units: &str) -> Result<ParseResult, String> {
    let invalid = || "INVALID_CHECKPOINT_CSV: invalid columns, coordinates or units".to_string();
    let factor = match units {
        "m" => 1.0 / 0.3048,
        "ft" => 1.0,
        _ => return Err(invalid()),
    };
    if text.len() > 25_000_000 {
        return Err(invalid());
    }
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let headers: Vec<_> = fields(lines.next().ok_or_else(invalid)?)?.into_iter().map(|v| v.trim().to_ascii_lowercase()).collect();
    let names = ["gcp_id", "expected_x", "expected_y", "expected_z", "measured_x", "measured_y", "measured_z"];
    let mut positions = Vec::new();
    for name in names {
        let matches: Vec<_> = headers
            .iter()
            .enumerate()
            .filter(|(_, h)| h.as_str() == name || name == "gcp_id" && matches!(h.as_str(), "id" | "point_id"))
            .map(|(i, _)| i)
            .collect();
        if matches.len() != 1 {
            return Err(invalid());
        }
        positions.push(*matches.first().ok_or_else(invalid)?);
    }
    let mut rms = [0.0_f64; 3];
    let mut count = 0_u32;
    for line in lines.filter(|l| !l.trim().is_empty()) {
        if count >= 100_000 {
            return Err(invalid());
        }
        let fields = fields(line)?;
        if fields.len() != headers.len() || fields.get(*positions.first().ok_or_else(invalid)?).is_none_or(|v| v.is_empty()) {
            return Err(invalid());
        }
        count += 1;
        for (axis, value) in rms.iter_mut().enumerate() {
            let coordinate = |column: usize| -> Result<f64, String> {
                let at = positions.get(column).ok_or_else(invalid)?;
                fields.get(*at).ok_or_else(invalid)?.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(invalid)
            };
            let delta = (coordinate(axis + 4)? - coordinate(axis + 1)?) * factor;
            if !delta.is_finite() {
                return Err(invalid());
            }
            // Hypot avoids overflowing the sum of squared residuals.
            *value = value.hypot(delta);
        }
    }
    if count == 0 {
        return Err("NO_CHECKPOINTS: no checkpoint rows".into());
    }
    for value in &mut rms {
        *value /= f64::from(count).sqrt();
        if !value.is_finite() {
            return Err(invalid());
        }
    }
    let [x, y, z] = rms;
    Ok(ParseResult {
        profile: "checkpoint_csv".into(),
        metrics: QAMetrics { rmse_x_feet: Some(x), rmse_y_feet: Some(y), rmse_z_feet: Some(z), ..Default::default() },
        ..Default::default()
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    const HEADER: &str = "gcp_id,expected_x,expected_y,expected_z,measured_x,measured_y,measured_z\n";
    #[test]
    fn reference_perfect_horizontal_and_symmetric_vertical_errors() {
        let perfect = parse(&format!("{HEADER}A,100,200,50,100,200,50\n"), "m").unwrap();
        assert_eq!(perfect.metrics.rmse_z_feet, Some(0.0));
        let result = parse(&format!("{HEADER}A,0,0,0,3,4,0.05\nB,0,0,0,3,4,-0.05\n"), "ft").unwrap();
        let m = result.metrics;
        assert!((m.rmse_x_feet.unwrap().hypot(m.rmse_y_feet.unwrap()) - 5.0).abs() < 1e-12);
        assert!((m.rmse_z_feet.unwrap() - 0.05).abs() < 1e-12);
    }
    #[test]
    fn empty_malformed_nonfinite_and_overflow_fail_closed() {
        assert!(parse(HEADER, "ft").unwrap_err().starts_with("NO_CHECKPOINTS"));
        for text in ["", "id,x\nA,0\n", "gcp_id,gcp_id\nA,A\n"] {
            assert!(parse(text, "ft").is_err());
        }
        for number in ["NaN", "inf", "garbage", "1e309"] {
            assert!(parse(&format!("{HEADER}A,0,0,0,0,0,{number}\n"), "ft").is_err());
        }
        assert!(parse(&format!("{HEADER}A,-1e308,0,0,1e308,0,0\n"), "ft").is_err());
        assert!(parse(HEADER, "metres").is_err());
    }
    #[test]
    fn header_alias_and_metre_conversion() {
        let text = format!("{}A,0,0,0,0.3048,0,0\n", HEADER.replace("gcp_id", " POINT_ID "));
        assert!((parse(&text, "m").unwrap().metrics.rmse_x_feet.unwrap() - 1.0).abs() < 1e-12);
    }
    #[test]
    fn quoted_csv_fields_are_accepted() {
        let header = HEADER.trim_end().split(',').map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(",");
        let text = format!("{header}\n\"A, one\",\"0\",0,0,\"0.3048\",0,0\n");
        assert!((parse(&text, "m").unwrap().metrics.rmse_x_feet.unwrap() - 1.0).abs() < 1e-12);
    }
}
