//! PIX4D report parsing, UTF-8 text only (synthetic v1 and PIX4Dmapper 4.10 layouts). Reference:
//! AIGIS3D `parsers.py`, `pix4dmapper.py`. Owner: Codex (see `docs/agents/slice-3-droneqa.md`).

use crate::{ParseResult, ParserDiagnostic};
use std::collections::BTreeSet;

const HEADER: &str = "AIGIS3D SYNTHETIC PIX4D QA v1";

fn diagnostic(result: &mut ParseResult, code: &str, message: &str) {
    if result.diagnostics.len() >= 256 {
        return;
    }
    result.diagnostics.push(ParserDiagnostic { code: code.into(), message: message.into() });
}

/// Parse only qualified UTF-8 text layouts; diagnostics never quote rejected input.
pub fn parse(bytes: &[u8], content_type: &str) -> ParseResult {
    let mut result = ParseResult { profile: "synthetic-pix4d-v1".into(), ..Default::default() };
    if bytes.len() > 25_000_000 {
        diagnostic(&mut result, "REPORT_TOO_LARGE", "Report exceeds the parser byte limit.");
        return result;
    }
    if content_type != "text/plain" {
        diagnostic(
            &mut result,
            "UNSUPPORTED_CONTENT_TYPE",
            if content_type == "application/pdf" { "PDF reports are not supported yet; supply UTF-8 text." } else { "Supply UTF-8 text." },
        );
        return result;
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        diagnostic(&mut result, "MALFORMED_REPORT", "Report could not be decoded.");
        return result;
    };
    let mut lines = text.trim_start_matches('\u{feff}').lines().map(str::trim).filter(|line| !line.is_empty());
    if text.trim_start_matches('\u{feff}').trim_start().starts_with("Generated with PIX4Dmapper") {
        return mapper(text);
    }
    if lines.next() != Some(HEADER) {
        diagnostic(&mut result, "UNSUPPORTED_LAYOUT", "Supply a qualified report layout.");
        return result;
    }
    let labels = [
        ("RMSE X", "rmse_x_feet"),
        ("RMSE Y", "rmse_y_feet"),
        ("RMSE Z", "rmse_z_feet"),
        ("Images calibrated", "calibrated_images"),
        ("Images total", "total_images"),
        ("Software", "processing_software"),
        ("Version", "processing_version"),
    ];
    let mut seen = BTreeSet::new();
    let mut values = serde_json::Map::new();
    for line in lines {
        let Some((label, raw)) = line.split_once(':') else {
            diagnostic(&mut result, "UNRECOGNIZED_LINE", "An unrecognized report line was ignored.");
            continue;
        };
        let Some((_, name)) = labels.iter().find(|(known, _)| *known == label) else {
            diagnostic(&mut result, "UNRECOGNIZED_LINE", "An unrecognized report line was ignored.");
            continue;
        };
        if !seen.insert(*name) {
            values.remove(*name);
            diagnostic(&mut result, "DUPLICATE_METRIC", "Duplicate values cannot establish a metric.");
            continue;
        }
        let raw = raw.trim();
        let value = if label.starts_with("RMSE") {
            raw.strip_suffix(" ft")
                .filter(|number| decimal(number))
                .and_then(|number| number.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
        } else if label.starts_with("Images") {
            if !raw.is_empty() && raw.len() <= 20 && raw.bytes().all(|byte| byte.is_ascii_digit()) {
                raw.parse::<u64>().ok().map(serde_json::Value::from)
            } else {
                None
            }
        } else {
            Some(serde_json::Value::String(raw.into()))
        };
        if let Some(value) = value {
            values.insert((*name).into(), value);
        } else {
            diagnostic(&mut result, "INVALID_METRIC", "Metric has an invalid value or unit.");
        }
    }
    for (_, name) in labels {
        if !values.contains_key(name) {
            diagnostic(&mut result, "MISSING_METRIC", &format!("{name}: no usable value."));
        }
    }
    if let Ok(metrics) = serde_json::from_value(serde_json::Value::Object(values)) {
        result.metrics = metrics;
    }
    if matches!((result.metrics.calibrated_images, result.metrics.total_images), (Some(calibrated), Some(total)) if calibrated > total) {
        result.metrics.calibrated_images = None;
        result.metrics.total_images = None;
        diagnostic(&mut result, "INCONSISTENT_IMAGES", "Calibrated images exceed total images.");
    }
    result
}

fn decimal(raw: &str) -> bool {
    let mut parts = raw.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.is_none_or(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && parts.next().is_none()
}

fn mapper(text: &str) -> ParseResult {
    use crate::{ReportEvidence, ReportResiduals};
    let mut result = ParseResult { profile: "pix4dmapper-4.10.0-gcp-v1".into(), ..Default::default() };
    if text.lines().take(100_001).count() > 100_000 {
        diagnostic(&mut result, "REPORT_TOO_LARGE", "Report exceeds the parser line limit.");
        return result;
    }
    let lines: Vec<_> = text.trim_start_matches('\u{feff}').lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let flat = text.split_whitespace().fold(String::new(), |mut output, word| {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(word);
        output
    });
    let sections = [
        "Summary",
        "Quality Check",
        "Calibration Details",
        "Geolocation Details",
        "Ground Control Points",
        "Absolute Geolocation Variance",
        "Relative Geolocation Variance",
        "Initial Processing Details",
        "Coordinate Systems",
        "Point Cloud Densification details",
        "DSM, Orthomosaic and Index Details",
    ];
    let positions: Option<Vec<_>> =
        sections
            .iter()
            .map(|section| {
                if lines.iter().filter(|line| *line == section).count() == 1 && flat.matches(section).count() == 1 {
                    flat.find(section)
                } else {
                    None
                }
            })
            .collect();
    let checkpoints = regex::Regex::new(r"(?i-u:\bcheck\s*points?\b)").map_or(true, |pattern| pattern.is_match(&flat));
    if lines.first() != Some(&"Generated with PIX4Dmapper version 4.10.0")
        || lines.get(1) != Some(&"Quality Report")
        || positions.as_ref().is_none_or(|values| values.windows(2).any(|pair| pair.first() >= pair.get(1)))
        || checkpoints
    {
        diagnostic(&mut result, "UNSUPPORTED_LAYOUT", "PIX4Dmapper 4.10.0 GCP-only layout required.");
        return result;
    }
    let block =
        |start: &str, end: &str| -> &str { flat.split_once(start).and_then(|(_, tail)| tail.split_once(end)).map_or("", |(value, _)| value.trim()) };
    let summary = block("Summary", "Quality Check");
    let quality = block("Quality Check", "Calibration Details");
    let calibration = block("Calibration Details", "Geolocation Details");
    let ground = block("Ground Control Points", "Absolute Geolocation Variance");
    let camera = block("Absolute Geolocation Variance", "Relative Geolocation Variance");
    let mut triple = |source: &str, label: &str, nonnegative: bool| -> Option<[f64; 3]> {
        if source.matches(label).count() != 1 {
            diagnostic(
                &mut result,
                if source.matches(label).count() > 1 { "DUPLICATE_METRIC" } else { "MISSING_METRIC" },
                "Residual statistic unavailable.",
            );
            return None;
        }
        let tail = source.split_once(label)?.1.trim_start();
        let Some(tail) = tail.strip_prefix("[US survey foot] ") else {
            diagnostic(&mut result, "UNSUPPORTED_UNIT", "Residual statistic requires US survey foot.");
            return None;
        };
        let mut tokens = tail.split_whitespace();
        let mut values = [0.0; 3];
        for value in &mut values {
            let token = tokens.next()?;
            let unsigned = token.strip_prefix(['-', '+']).unwrap_or(token);
            if unsigned.split('.').any(|part| part.len() > 20) || !decimal(unsigned) {
                diagnostic(&mut result, "INVALID_METRIC", "Residual statistic invalid.");
                return None;
            }
            let parsed = token.parse::<f64>().ok()?;
            if !parsed.is_finite() || (nonnegative && parsed < 0.0) {
                diagnostic(&mut result, "INVALID_METRIC", "Residual statistic invalid.");
                return None;
            }
            *value = parsed;
        }
        if tokens.next().is_some_and(|token| !token.starts_with(char::is_alphabetic)) {
            diagnostic(&mut result, "INVALID_METRIC", "Residual statistic has extra values.");
            return None;
        }
        Some(values)
    };
    let gcp_mean = triple(ground, "Mean", false);
    let gcp_sigma = triple(ground, "Sigma", true);
    let gcp_rms = triple(ground, "RMS Error", true);
    let camera_mean = triple(camera, "Mean", false);
    let camera_sigma = triple(camera, "Sigma", true);
    let camera_rms = triple(camera, "RMS Error", true);
    let translation = triple(camera, "Translation", false);
    let residual = |provenance: &str, mean, sigma, rms| match (mean, sigma, rms) {
        (Some(mean_xyz), Some(sigma_xyz), Some(rms_xyz)) => {
            Some(ReportResiduals { provenance: provenance.into(), unit: "US survey foot".into(), mean_xyz, sigma_xyz, rms_xyz })
        }
        _ => None,
    };
    let mut evidence = ReportEvidence {
        gcp_fit: residual("fitted_gcps", gcp_mean, gcp_sigma, gcp_rms),
        camera_offsets: residual("initial_computed_camera_positions", camera_mean, camera_sigma, camera_rms),
        camera_translation_xyz: translation,
        camera_translation_unit: translation.map(|_| "US survey foot".into()),
        ..Default::default()
    };
    let capture = |source: &str, label: &str, pattern: &str| -> Option<Vec<String>> {
        if source.matches(label).count() != 1 {
            return None;
        }
        let regex = regex::Regex::new(pattern).ok()?;
        let mut matches = regex.captures_iter(source);
        let matched = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(matched.iter().skip(1).filter_map(|value| value.map(|value| value.as_str().into())).collect())
    };
    let pair = |values: &[String]| -> Option<(u64, u64)> {
        let a = values.first()?.parse::<u64>().ok()?;
        let b = values.get(1)?.parse::<u64>().ok()?;
        (a <= b && b > 0).then_some((a, b))
    };
    let dataset = capture(quality, "Dataset", r"Dataset ([0-9]{1,20}) out of ([0-9]{1,20}) images calibrated \(([0-9]{1,3}(?:\.[0-9]{1,20})?)%\)");
    let calibrated = capture(calibration, "Number of Calibrated Images", r"Number of Calibrated Images ([0-9]{1,20}) out of ([0-9]{1,20})(?: |$)");
    let images = dataset.as_ref().and_then(|values| {
        let counts = pair(values)?;
        let percent_text = values.get(2)?;
        percentage_consistent(counts.0, counts.1, percent_text).then_some(counts)
    });
    if images.is_some() && images == calibrated.as_ref().and_then(|values| pair(values)) {
        if let Some((a, b)) = images {
            result.metrics.calibrated_images = Some(a);
            result.metrics.total_images = Some(b);
        }
    } else {
        diagnostic(&mut result, "INCONSISTENT_IMAGES", "Calibrated image counts or percentage disagree.");
    }
    if let Some(values) = capture(
        summary,
        "Average Ground Sampling Distance (GSD)",
        r"Average Ground Sampling Distance \(GSD\) ([0-9]{1,20}(?:\.[0-9]{1,20})?) cm / ([0-9]{1,20}(?:\.[0-9]{1,20})?) in(?: |$)",
    ) && let (Some(cm), Some(inch)) = (values.first(), values.get(1))
    {
        let rounding = |raw: &str| 0.5 * 10_f64.powi(-i32::try_from(raw.split_once('.').map_or(0, |(_, part)| part.len())).unwrap_or(20));
        if let (Ok(a), Ok(b)) = (cm.parse::<f64>(), inch.parse::<f64>())
            && a > 0.0
            && b > 0.0
            && (a - b * 2.54).abs() <= rounding(cm) + 2.54 * rounding(inch)
        {
            evidence.gsd_cm = Some(a);
        }
    }
    if evidence.gsd_cm.is_none() {
        diagnostic(&mut result, "INVALID_METRIC", "Ground sampling distance unavailable or invalid.");
    }
    if let Some(values) = capture(quality, "Georeferencing", r"Georeferencing yes, ([0-9]{1,20}) GCPs \(([0-9]{1,20}) 3D\)")
        && let Some((a, b)) = pair(&values)
        && a == b
        && regex::Regex::new(r"\S+ \(3D\)").ok().is_some_and(|pattern| pattern.find_iter(ground).count() as u64 == a)
    {
        evidence.gcp_count = Some(a);
    }
    if evidence.gcp_count.is_none() {
        diagnostic(&mut result, "INCONSISTENT_CONTROL", "Ground control count unavailable or inconsistent.");
    }
    let next_line = |label: &str, end: &str| -> Option<String> {
        let starts: Vec<_> = lines.iter().enumerate().filter(|(_, line)| **line == label).map(|(index, _)| index).collect();
        if starts.len() != 1 {
            return None;
        }
        let index = *starts.first()?;
        let section_start = lines.iter().position(|line| *line == "Coordinate Systems")?;
        let section_end = lines.iter().position(|line| *line == "Point Cloud Densification details")?;
        if index <= section_start || index + 2 >= section_end {
            return None;
        }
        if lines.get(index + 2) != Some(&end) {
            return None;
        }
        lines.get(index + 1).filter(|value| value.len() <= 2000).map(|value| (*value).into())
    };
    evidence.image_crs = next_line("Image Coordinate System", "Ground Control Point (GCP) Coordinate System");
    evidence.gcp_crs = next_line("Ground Control Point (GCP) Coordinate System", "Output Coordinate System");
    evidence.output_crs = next_line("Output Coordinate System", "Processing Options");
    for (label, start, end, pattern, field) in [
        (
            "Time for Point Cloud Classification",
            "Point Cloud Densification details",
            "DSM, Orthomosaic and Index Details",
            r"\A(?:NA|(?:[0-9]{1,5}h:)?[0-9]{1,5}m:[0-9]{1,2}s|[0-9]{1,5}s)\z",
            &mut evidence.classification_time,
        ),
        (
            "DTM Resolution",
            "DSM, Orthomosaic and Index Details",
            "",
            r"\A[0-9]{1,20} x GSD \([0-9]{1,20}(?:\.[0-9]{1,20})? \[cm/pixel\]\)\z",
            &mut evidence.dtm_resolution_label,
        ),
    ] {
        let indexes: Vec<_> = lines.iter().enumerate().filter(|(_, line)| **line == label).map(|(index, _)| index).collect();
        if indexes.len() == 1
            && let Some(index) = indexes.first().copied()
        {
            let section_start = lines.iter().position(|line| *line == start).unwrap_or(usize::MAX);
            let section_end = lines.iter().position(|line| *line == end).unwrap_or(lines.len());
            if index <= section_start || index + 1 >= section_end {
                diagnostic(&mut result, "MISPLACED_METRIC", "Derived metric outside qualified section.");
                continue;
            }
            if let Some(value) = lines.get(index + 1)
                && regex::Regex::new(pattern).ok().is_some_and(|regex| regex.is_match(value))
            {
                *field = Some((*value).into());
            }
        }
        if field.is_none() {
            diagnostic(&mut result, "INVALID_METRIC", "Derived metric unavailable or invalid.");
        }
    }
    for (code, message) in [
        ("INDEPENDENT_CHECKPOINTS_UNAVAILABLE", "Independent checkpoint measurements unavailable."),
        ("VERTICAL_REFERENCE_UNAVAILABLE", "Vertical datum and geoid unavailable."),
        ("BARE_GROUND_UNVERIFIED", "Independent bare-ground measurements unavailable."),
        ("DTM_RESOLUTION_UNQUALIFIED", "Effective DTM cell resolution unqualified."),
    ] {
        diagnostic(&mut result, code, message);
    }
    result.metrics.processing_software = Some("PIX4Dmapper".into());
    for (source, label) in [(summary, "Average Ground Sampling Distance (GSD)"), (quality, "Dataset"), (quality, "Georeferencing")] {
        if source.matches(label).count() > 1 {
            diagnostic(&mut result, "DUPLICATE_METRIC", "Duplicate metric labels cannot establish evidence.");
        }
    }
    for value in [&evidence.image_crs, &evidence.gcp_crs, &evidence.output_crs] {
        if value.is_none() {
            diagnostic(&mut result, "MISSING_METRIC", "Coordinate system unavailable or ambiguous.");
        }
    }
    result.metrics.processing_version = Some("4.10.0".into());
    result.report_evidence = Some(evidence);
    result
}

fn percentage_consistent(calibrated: u64, total: u64, raw: &str) -> bool {
    let digits = raw.split_once('.').map_or(0, |(_, fraction)| fraction.len());
    if total == 0 || calibrated > total || digits > 20 || !decimal(raw) {
        return false;
    }
    let Ok(displayed) = raw.replace('.', "").parse::<u128>() else {
        return false;
    };
    let divisor = u128::from(total);
    let numerator = 100 * u128::from(calibrated);
    let mut quotient = numerator / divisor;
    let mut remainder = numerator % divisor;
    // Long division keeps products within u128 even for u64 counts and 20 decimal places.
    for _ in 0..digits {
        remainder *= 10;
        quotient = quotient * 10 + remainder / divisor;
        remainder %= divisor;
    }
    (quotient == displayed && remainder * 2 <= divisor) || (quotient.checked_add(1) == Some(displayed) && remainder * 2 >= divisor)
}
