//! The QA rules: `analyze` and `summarize`. Reference: AIGIS3D `qa.py`. Owner: Claude (see
//! `docs/agents/slice-3-droneqa.md`).
//!
//! Deterministic project QA: a missing observation or a missing criterion never passes.
//! Redaction boundary (agreed with Codex by Paseo message, see `docs/agents/slice-3-notes-claude.md`):
//! `analyze` redacts any finding whose inputs include a sensitive mission field, and passes every
//! observed value and requirement through the credential scrubber. The workflow does the rest.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::mission::{COORDINATE_FIELDS, Mission, Ruleset, attachment_pattern};
use crate::model::{Analysis, Finding, Outcome, ParseResult, ReportResiduals, Severity};

const REDACTED: &str = "[REDACTED]";

fn rank(o: Outcome) -> u8 {
    match o {
        Outcome::Info => 0,
        Outcome::Pass => 1,
        Outcome::Conditional => 2,
        Outcome::InsufficientData => 3,
        Outcome::Fail => 4,
    }
}

fn severity(o: Outcome) -> Severity {
    match o {
        Outcome::Info | Outcome::Pass => Severity::Info,
        Outcome::Conditional => Severity::Low,
        Outcome::InsufficientData => Severity::Medium,
        Outcome::Fail => Severity::High,
    }
}

/// The JSON spelling of an outcome (`INSUFFICIENT_DATA`).
pub fn outcome_name(o: Outcome) -> &'static str {
    match o {
        Outcome::Pass => "PASS",
        Outcome::Conditional => "CONDITIONAL",
        Outcome::Fail => "FAIL",
        Outcome::InsufficientData => "INSUFFICIENT_DATA",
        Outcome::Info => "INFO",
    }
}

/// Credential scrubbing for derived values (`redaction.redact` with no configured fields).
fn scrub(v: Value) -> Value {
    crate::redact::redact(&v, &[])
}

/// Optional fields of one finding; `add` fills the defaults.
#[derive(Default)]
struct Extra<'a> {
    fields: &'a [&'a str],
    observed: Option<Value>,
    requirement: Option<String>,
    unit: Option<&'a str>,
    source: Option<String>,
    next_action: Option<String>,
}

struct Rules<'a> {
    private: BTreeSet<String>,
    findings: Vec<Finding>,
    mission: &'a Map<String, Value>,
}

impl Rules<'_> {
    fn add(&mut self, rule: &str, outcome: Outcome, message: &str, x: Extra) {
        let (mut observed, mut requirement) = (x.observed, x.requirement);
        // Derived values inherit every input's privacy, including report attachments.
        if x.fields.iter().any(|f| self.private.contains(*f)) {
            observed = observed.map(|_| json!(REDACTED));
            requirement = requirement.map(|_| REDACTED.to_string());
        }
        let default_action = match outcome {
            Outcome::Fail => Some("Reconcile the evidence with the configured requirement."),
            Outcome::InsufficientData => Some("Provide missing evidence or configure the project requirement."),
            Outcome::Conditional => Some("Review the documented exception before approval."),
            _ => None,
        };
        let joined = x.fields.iter().map(|f| format!("mission.{f}")).collect::<Vec<_>>().join(", ");
        self.findings.push(Finding {
            rule_id: rule.to_string(),
            outcome,
            severity: severity(outcome),
            message: message.to_string(),
            observed: observed.map(scrub).filter(|v| !v.is_null()),
            requirement: requirement.map(|r| match scrub(Value::String(r)) {
                Value::String(s) => s,
                _ => REDACTED.to_string(),
            }),
            unit: x.unit.map(str::to_string),
            source: x.source.or_else(|| (!joined.is_empty()).then_some(joined)),
            next_action: x.next_action.or_else(|| default_action.map(str::to_string)),
        });
    }

    /// One observation against a maximum (or, with `minimum`, a minimum) criterion.
    #[allow(clippy::too_many_arguments)]
    fn compare(
        &mut self,
        rule: &str,
        observed: Option<Value>,
        threshold: Option<Value>,
        minimum: bool,
        x: Extra,
        missing_action: Option<&str>,
        failure_action: Option<&str>,
    ) {
        let (outcome, message, action) = match (&threshold, observed.as_ref().and_then(Value::as_f64)) {
            (None, _) => (
                Outcome::InsufficientData,
                "No project criterion configured.".to_string(),
                Some(format!("Configure a project criterion for {rule}.")),
            ),
            (Some(_), None) => (
                Outcome::InsufficientData,
                "Required observation is missing.".to_string(),
                Some(missing_action.map_or_else(|| format!("Document {} before reassessing.", x.fields.join(", ")), str::to_string)),
            ),
            (Some(t), Some(o)) => {
                let t = t.as_f64().unwrap_or(f64::NAN);
                let accepted = if minimum { o >= t } else { o <= t };
                let kind = if minimum { "minimum" } else { "maximum" };
                let action = (!accepted)
                    .then(|| failure_action.map_or_else(|| format!("Review {rule} evidence against the project criterion."), str::to_string));
                (if accepted { Outcome::Pass } else { Outcome::Fail }, format!("Observation compared with the configured {kind} criterion."), action)
            }
        };
        let requirement = threshold.map(|t| format!("{} {t}", if minimum { ">=" } else { "<=" }));
        self.add(rule, outcome, &message, Extra { observed, requirement, next_action: action, ..x });
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.mission.get(name).filter(|v| !v.is_null())
    }

    fn f64(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(Value::as_f64)
    }

    fn str(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Value::as_str)
    }
}

fn num(v: Option<f64>) -> Option<Value> {
    v.map(Value::from)
}

fn count(v: Option<u64>) -> Option<Value> {
    v.map(Value::from)
}

/// Run every rule on a mission, its ruleset and an optional parsed report.
///
/// A report that fails [`validate_report`] is ignored, as if none were given.
/// `sensitive_fields` names the private mission fields; `None` treats every field as private
/// (fail closed). Findings that derive from a private field carry `[REDACTED]` instead of values.
pub fn analyze(mission: &Mission, ruleset: &Ruleset, report: Option<&ParseResult>, sensitive_fields: Option<&[String]>) -> Analysis {
    // Fail closed: a report that breaks the model rules is not evidence (callers report why).
    let report = report.filter(|r| validate_report(r).is_ok());
    let map = mission.to_map();
    let private = match sensitive_fields {
        None => Mission::field_names().into_iter().collect(),
        Some(s) => s.iter().cloned().collect(),
    };
    let mut r = Rules { private, findings: Vec::new(), mission: &map };

    completeness(&mut r, mission, ruleset);
    coordinates(&mut r, ruleset);
    let independent = control(&mut r, mission, ruleset);
    accuracy(&mut r, mission, ruleset, report, independent);
    flight(&mut r, mission, ruleset);
    corrections(&mut r, mission, ruleset);
    processing(&mut r, ruleset, report);
    deliverables(&mut r, mission, ruleset);

    let mut findings = r.findings;
    let outcome = findings.iter().map(|f| f.outcome).max_by_key(|o| rank(*o)).unwrap_or(Outcome::InsufficientData);
    findings.sort_by_key(|f| std::cmp::Reverse(rank(f.outcome)));
    Analysis { findings, outcome, risk_level: severity(outcome), ruleset_version: ruleset.version.clone() }
}

fn completeness(r: &mut Rules, mission: &Mission, ruleset: &Ruleset) {
    let missing: Vec<&String> = ruleset
        .required_properties
        .iter()
        .filter(|name| match r.mission.get(name.as_str()) {
            None | Some(Value::Null) => true,
            Some(Value::String(s)) => s.is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            _ => false,
        })
        .collect();
    if ruleset.required_properties.is_empty() {
        r.add(
            "COMPLETENESS.PROPERTIES",
            Outcome::InsufficientData,
            "Required property list is not configured.",
            Extra { next_action: Some("Configure the mission properties required by this project.".into()), ..Extra::default() },
        );
    } else {
        let names = missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        let any = !missing.is_empty();
        r.add(
            "COMPLETENESS.PROPERTIES",
            if any { Outcome::InsufficientData } else { Outcome::Pass },
            &if any { format!("Missing: {names}") } else { "All configured required properties are present.".into() },
            Extra {
                observed: Some(json!(missing)),
                requirement: Some("All configured required properties present".into()),
                source: Some("mission".into()),
                next_action: any.then(|| "Document the listed missing mission properties.".into()),
                ..Extra::default()
            },
        );
    }
    if ruleset.required_attachment_patterns.is_empty() {
        r.add("COMPLETENESS.ATTACHMENTS", Outcome::Info, "No required attachment patterns configured.", Extra::default());
    }
    for (index, pattern) in ruleset.required_attachment_patterns.iter().enumerate() {
        // A pattern that does not compile matches nothing; `Ruleset::from_value` rejects it up front.
        let present = attachment_pattern(pattern).is_ok_and(|re| mission.attachments.iter().any(|a| re.is_match(&a.name)));
        r.add(
            &format!("COMPLETENESS.ATTACHMENT_{}", index.saturating_add(1)),
            if present { Outcome::Pass } else { Outcome::InsufficientData },
            if present { "Required attachment present." } else { "Required attachment absent." },
            Extra {
                fields: &["attachments"],
                observed: Some(json!(present)),
                requirement: Some(pattern.clone()),
                next_action: (!present).then(|| "Supply an attachment matching the configured requirement.".into()),
                ..Extra::default()
            },
        );
    }
}

fn coordinates(r: &mut Rules, ruleset: &Ruleset) {
    let missing: Vec<&str> = COORDINATE_FIELDS.into_iter().filter(|n| r.str(n).is_none_or(str::is_empty)).collect();
    let any = !missing.is_empty();
    r.add(
        "COORDINATES.DOCUMENTATION",
        if any { Outcome::InsufficientData } else { Outcome::Pass },
        &if any {
            format!("Missing coordinate documentation: {}", missing.join(", "))
        } else {
            "Horizontal and vertical systems, datums and units are documented.".into()
        },
        Extra {
            observed: Some(json!(missing)),
            requirement: Some("Horizontal and vertical CRS, datums and units documented".into()),
            source: Some("mission".into()),
            next_action: any.then(|| "Document the listed missing coordinate properties.".into()),
            ..Extra::default()
        },
    );
    if ruleset.coordinates.is_empty() {
        r.add(
            "COORDINATES.REQUIREMENTS",
            Outcome::InsufficientData,
            "Coordinate requirements are not configured.",
            Extra { next_action: Some("Configure this project's required coordinate systems, datums and units.".into()), ..Extra::default() },
        );
    }
    for (name, expected) in &ruleset.coordinates {
        let observed = r.str(name).map(str::to_string);
        let outcome = match &observed {
            None => Outcome::InsufficientData,
            Some(o) if o == expected => Outcome::Pass,
            Some(_) => Outcome::Fail,
        };
        let field = [name.as_str()];
        r.add(
            &format!("COORDINATES.{}", name.to_uppercase()),
            outcome,
            &format!("{name} compared with the configured coordinate requirement."),
            Extra {
                fields: &field,
                observed: observed.map(Value::String),
                requirement: Some(expected.clone()),
                next_action: (outcome != Outcome::Pass).then(|| format!("Document or reconcile {name} with the project requirement.")),
                ..Extra::default()
            },
        );
    }
}

/// Control rules. Returns whether the mission documents independent checkpoints.
fn control(r: &mut Rules, mission: &Mission, ruleset: &Ruleset) -> bool {
    let c = &ruleset.control;
    let counts = [
        ("CONTROL.GCP_COUNT", mission.gcp_count, c.minimum_gcp_count, "gcp_count"),
        ("CONTROL.CHECKPOINT_COUNT", mission.checkpoint_count, c.minimum_checkpoint_count, "checkpoint_count"),
    ];
    for (rule, observed, threshold, field) in counts {
        r.compare(rule, count(observed), count(threshold), true, Extra { fields: &[field], unit: Some("count"), ..Extra::default() }, None, None);
    }
    let accuracies = [
        ("CONTROL.SOURCE_ACCURACY", mission.control_accuracy_feet, c.maximum_control_accuracy_feet, "control_accuracy_feet"),
        ("CONTROL.CHECKPOINT_ACCURACY", mission.checkpoint_accuracy_feet, c.maximum_checkpoint_accuracy_feet, "checkpoint_accuracy_feet"),
    ];
    for (rule, observed, threshold, field) in accuracies {
        r.compare(rule, num(observed), num(threshold), false, Extra { fields: &[field], unit: Some("ft"), ..Extra::default() }, None, None);
    }
    let a = &ruleset.accuracy;
    let accuracy_claim =
        [a.maximum_rmse_horizontal_feet, a.maximum_rmse_x_feet, a.maximum_rmse_y_feet, a.maximum_rmse_z_feet].iter().any(Option::is_some);
    let independent = mission.checkpoint_count.is_some_and(|n| n > 0) && mission.checkpoints_independent == Some(true);
    if accuracy_claim || c.require_independent_checkpoints {
        let outcome = match mission.checkpoint_count {
            None => Outcome::InsufficientData,
            Some(0) if c.permit_zero_checkpoints => Outcome::Conditional,
            Some(0) => Outcome::InsufficientData,
            Some(_) if independent => Outcome::Pass,
            Some(_) => Outcome::InsufficientData,
        };
        let known = mission.checkpoint_count.is_some() && mission.checkpoints_independent.is_some();
        r.add(
            "CONTROL.INDEPENDENCE",
            outcome,
            "Independent checkpoints are required for verification; any zero-checkpoint exception remains conditional.",
            Extra {
                fields: &["checkpoint_count", "checkpoints_independent"],
                observed: known.then(|| json!(independent)),
                requirement: Some("Positive checkpoint count and documented independence".into()),
                next_action: (outcome != Outcome::Pass).then(|| "Document independent checkpoints; fitted GCPs do not establish accuracy.".into()),
                ..Extra::default()
            },
        );
    } else {
        r.add("CONTROL.INDEPENDENCE", Outcome::Info, "Independent verification requirement is not configured.", Extra::default());
    }
    independent
}

fn accuracy(r: &mut Rules, mission: &Mission, ruleset: &Ruleset, report: Option<&ParseResult>, independent: bool) {
    let c = &ruleset.control;
    let a = &ruleset.accuracy;
    let qualified = report.filter(|p| independent && p.metrics.checkpoints_independent == Some(true));
    for axis in ['x', 'y', 'z'] {
        let name = format!("rmse_{axis}_feet");
        let bare_ground = axis == 'z' && c.require_bare_ground_checkpoints;
        let value = r.f64(&name).filter(|_| independent && (!bare_ground || mission.checkpoints_bare_ground == Some(true)));
        let parsed = qualified.filter(|p| !bare_ground || p.metrics.checkpoints_bare_ground == Some(true)).and_then(|p| match axis {
            'x' => p.metrics.rmse_x_feet,
            'y' => p.metrics.rmse_y_feet,
            _ => p.metrics.rmse_z_feet,
        });
        let mut fields = vec![name.as_str(), "checkpoint_count", "checkpoints_independent"];
        if bare_ground {
            fields.push("checkpoints_bare_ground");
        }
        let up = axis.to_ascii_uppercase();
        if let (Some(v), Some(p)) = (value, parsed)
            && v != p
        {
            let conflict = [fields.as_slice(), &["attachments"]].concat();
            r.add(
                &format!("ACCURACY.SOURCE_CONFLICT_{up}"),
                Outcome::Fail,
                "Mission and report metrics disagree; reconcile the source before approval.",
                Extra {
                    fields: &conflict,
                    observed: Some(json!([format!("mission: {}", Value::from(v)), format!("report: {}", Value::from(p))])),
                    requirement: Some("Mission and qualified report metrics agree".into()),
                    unit: Some("ft"),
                    source: Some(format!("mission.{name}, parser.metrics.{name}")),
                    next_action: Some("Reconcile the independent checkpoint measurements before approval.".into()),
                },
            );
        }
        let observed = parsed.or(value);
        let source = if parsed.is_some() { format!("parser.metrics.{name}") } else { format!("mission.{name}") };
        if parsed.is_some() {
            fields.push("attachments");
        }
        let threshold = match axis {
            'x' => a.maximum_rmse_x_feet,
            'y' => a.maximum_rmse_y_feet,
            _ => a.maximum_rmse_z_feet,
        };
        let rule = format!("ACCURACY.RMSE_{up}");
        if axis != 'z' && threshold.is_none() && a.maximum_rmse_horizontal_feet.is_some() {
            r.add(
                &rule,
                Outcome::Info,
                "Horizontal accuracy is assessed with the configured radial RMSE criterion.",
                Extra {
                    fields: &fields,
                    observed: num(observed),
                    requirement: Some("Use the configured radial horizontal RMSE criterion".into()),
                    unit: Some("ft"),
                    source: Some(source),
                    ..Extra::default()
                },
            );
        } else if bare_ground && threshold.is_some() && observed.is_none() {
            r.add(
                &rule,
                Outcome::InsufficientData,
                "Independent bare-ground checkpoint RMSE is required.",
                Extra {
                    fields: &fields,
                    requirement: threshold.map(|t| format!("<= {}", Value::from(t))),
                    unit: Some("ft"),
                    source: Some(source),
                    next_action: Some("Provide independent bare-ground checkpoint RMSE and its provenance.".into()),
                    ..Extra::default()
                },
            );
        } else {
            r.compare(
                &rule,
                num(observed),
                num(threshold),
                false,
                Extra { fields: &fields, unit: Some("ft"), source: Some(source), ..Extra::default() },
                Some("Provide independent checkpoint RMSE and its provenance."),
                Some("Verify independent checkpoint results and resolve the accuracy exceedance."),
            );
        }
    }

    if let Some(limit) = a.maximum_rmse_horizontal_feet {
        let mut fields = vec!["rmse_x_feet", "rmse_y_feet", "checkpoint_count", "checkpoints_independent"];
        let mut source = "hypot(mission.rmse_x_feet, mission.rmse_y_feet)";
        let mut xy = if independent { (mission.rmse_x_feet, mission.rmse_y_feet) } else { (None, None) };
        if let Some(p) = qualified.filter(|p| p.metrics.rmse_x_feet.is_some() && p.metrics.rmse_y_feet.is_some()) {
            xy = (p.metrics.rmse_x_feet, p.metrics.rmse_y_feet);
            source = "hypot(parser.metrics.rmse_x_feet, parser.metrics.rmse_y_feet)";
            fields.push("attachments");
        }
        let radial = match xy {
            (Some(x), Some(y)) => Some(x.hypot(y)),
            _ => None,
        };
        r.compare(
            "ACCURACY.RMSE_HORIZONTAL",
            num(radial),
            Some(Value::from(limit)),
            false,
            Extra { fields: &fields, unit: Some("ft"), source: Some(source.into()), ..Extra::default() },
            Some("Provide a complete X/Y pair from one qualified independent checkpoint source."),
            Some("Verify independent checkpoint results and resolve the horizontal accuracy exceedance."),
        );
    }
}

fn flight(r: &mut Rules, mission: &Mission, ruleset: &Ruleset) {
    let t = &ruleset.tolerances;
    let diff = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) => Some(Value::from((a - b).abs())),
        _ => None,
    };
    let images = match (mission.planned_image_count, mission.actual_image_count) {
        (Some(a), Some(b)) => Some(Value::from(a.abs_diff(b))),
        _ => None,
    };
    let checks = [
        (
            "ALTITUDE",
            "planned_altitude_agl_feet",
            "actual_altitude_agl_feet",
            num(t.maximum_altitude_difference_feet),
            "ft",
            diff(mission.planned_altitude_agl_feet, mission.actual_altitude_agl_feet),
        ),
        (
            "SPEED",
            "planned_speed_mph",
            "actual_speed_mph",
            num(t.maximum_speed_difference_mph),
            "mph",
            diff(mission.planned_speed_mph, mission.actual_speed_mph),
        ),
        (
            "FRONT_OVERLAP",
            "planned_front_overlap_percent",
            "front_overlap_percent",
            num(t.maximum_overlap_difference_percent),
            "percentage points",
            diff(mission.planned_front_overlap_percent, mission.front_overlap_percent),
        ),
        (
            "SIDE_OVERLAP",
            "planned_side_overlap_percent",
            "side_overlap_percent",
            num(t.maximum_overlap_difference_percent),
            "percentage points",
            diff(mission.planned_side_overlap_percent, mission.side_overlap_percent),
        ),
        ("IMAGE_COUNT", "planned_image_count", "actual_image_count", count(t.maximum_image_count_difference), "images", images),
    ];
    for (rule, planned, actual, threshold, unit, observed) in checks {
        r.compare(
            &format!("FLIGHT.{rule}"),
            observed,
            threshold,
            false,
            Extra {
                fields: &[planned, actual],
                unit: Some(unit),
                source: Some(format!("abs(mission.{planned} - mission.{actual})")),
                ..Extra::default()
            },
            None,
            Some("Reconcile the planned and actual flight observations with the allowed tolerance."),
        );
    }
    if t.require_matching_flight_pattern {
        let outcome = match (&mission.flight_pattern, &mission.planned_flight_pattern) {
            (Some(a), Some(p)) if a == p => Outcome::Pass,
            (Some(_), Some(_)) => Outcome::Fail,
            _ => Outcome::InsufficientData,
        };
        r.add(
            "FLIGHT.PATTERN",
            outcome,
            "Planned and actual flight patterns must match.",
            Extra {
                fields: &["flight_pattern", "planned_flight_pattern"],
                observed: mission.flight_pattern.clone().map(Value::String),
                requirement: mission.planned_flight_pattern.clone(),
                next_action: (outcome != Outcome::Pass).then(|| "Document or reconcile the planned and actual flight patterns.".into()),
                ..Extra::default()
            },
        );
    } else {
        r.add("FLIGHT.PATTERN", Outcome::Info, "Pattern matching is not required by configuration.", Extra::default());
    }
}

fn corrections(r: &mut Rules, mission: &Mission, ruleset: &Ruleset) {
    if ruleset.allowed_correction_statuses.is_empty() {
        r.add(
            "CORRECTIONS.STATUS",
            Outcome::InsufficientData,
            "Correction requirements are not configured.",
            Extra { next_action: Some("Configure the correction statuses allowed by this project.".into()), ..Extra::default() },
        );
    } else {
        let outcome = match &mission.rtk_ppk_status {
            None => Outcome::InsufficientData,
            Some(s) if ruleset.allowed_correction_statuses.contains(s) => Outcome::Pass,
            Some(_) => Outcome::Fail,
        };
        r.add(
            "CORRECTIONS.STATUS",
            outcome,
            "Correction status compared with configured allowed values.",
            Extra {
                fields: &["rtk_ppk_status"],
                observed: mission.rtk_ppk_status.clone().map(Value::String),
                requirement: Some(format!("Allowed: {}", ruleset.allowed_correction_statuses.join(", "))),
                next_action: (outcome != Outcome::Pass).then(|| "Document or reconcile the RTK/PPK correction status.".into()),
                ..Extra::default()
            },
        );
    }
    if ruleset.require_correction_source {
        let present = mission.correction_source.as_deref().is_some_and(|s| !s.is_empty());
        r.add(
            "CORRECTIONS.SOURCE",
            if present { Outcome::Pass } else { Outcome::InsufficientData },
            "Correction source must be documented.",
            Extra {
                fields: &["correction_source"],
                observed: mission.correction_source.clone().map(Value::String),
                requirement: Some("Documented correction source".into()),
                next_action: (!present).then(|| "Document the RTK/PPK correction source.".into()),
                ..Extra::default()
            },
        );
    } else {
        r.add("CORRECTIONS.SOURCE", Outcome::Info, "Correction-source requirement is not configured.", Extra::default());
    }
}

fn processing(r: &mut Rules, ruleset: &Ruleset, report: Option<&ParseResult>) {
    match report {
        None => r.add(
            "PROCESSING.REPORT",
            Outcome::InsufficientData,
            "No supported processing report was parsed.",
            Extra {
                fields: &["attachments"],
                requirement: Some("Supported report with usable evidence".into()),
                source: Some("parser".into()),
                next_action: Some("Supply a supported quality report and select its qualified parser profile.".into()),
                ..Extra::default()
            },
        ),
        Some(p) if !p.diagnostics.is_empty() => {
            let codes: BTreeSet<&str> = p.diagnostics.iter().map(|d| d.code.as_str()).collect();
            let list = codes.iter().copied().collect::<Vec<_>>();
            r.add(
                "PROCESSING.REPORT",
                Outcome::InsufficientData,
                &format!("Parser diagnostics require review: {}", list.join(", ")),
                Extra {
                    fields: &["attachments"],
                    observed: Some(json!(list)),
                    requirement: Some("Resolve parser diagnostics and unqualified evidence".into()),
                    source: Some("parser.diagnostics".into()),
                    next_action: Some("Resolve the parser diagnostics; supply missing or qualified report evidence.".into()),
                    ..Extra::default()
                },
            );
        }
        Some(_) => r.add(
            "PROCESSING.REPORT",
            Outcome::Pass,
            "Configured parser completed without diagnostics; this does not certify vendor compatibility.",
            Extra {
                fields: &["attachments"],
                observed: Some(json!("No diagnostics")),
                requirement: Some("Report parsed without diagnostics".into()),
                source: Some("parser.diagnostics".into()),
                ..Extra::default()
            },
        ),
    }
    let metrics = report.and_then(|p| serde_json::to_value(&p.metrics).ok());
    for name in &ruleset.processing.required_metrics {
        let value = metrics.as_ref().and_then(|m| m.get(name)).filter(|v| !v.is_null()).cloned();
        let unit = if name.ends_with("_feet") {
            Some("ft")
        } else if name.ends_with("_images") {
            Some("images")
        } else {
            None
        };
        let fields = [name.as_str(), "attachments"];
        let present = value.is_some();
        r.add(
            &format!("PROCESSING.{}", name.to_uppercase()),
            if present { Outcome::Pass } else { Outcome::InsufficientData },
            &format!("Required processing metric: {name}."),
            Extra {
                fields: &fields,
                observed: value,
                requirement: Some("Present in the selected report profile; presence does not establish accuracy".into()),
                unit,
                source: Some(format!("parser.metrics.{name}")),
                next_action: (!present).then(|| format!("Supply a supported report with usable {name}.")),
            },
        );
    }
    #[allow(clippy::cast_precision_loss)] // image counts are far below 2^52
    let percent = report.and_then(|p| match (p.metrics.calibrated_images, p.metrics.total_images) {
        (Some(c), Some(t)) if t > 0 => Some(100.0 * c as f64 / t as f64),
        _ => None,
    });
    r.compare(
        "PROCESSING.CALIBRATED_PERCENT",
        num(percent),
        num(ruleset.processing.minimum_calibrated_percent),
        true,
        Extra {
            fields: &["calibrated_images", "total_images", "attachments"],
            unit: Some("%"),
            source: Some("100 * parser.metrics.calibrated_images / parser.metrics.total_images".into()),
            ..Extra::default()
        },
        Some("Supply usable calibrated and total image counts in a qualified report."),
        Some("Review image calibration results against the project's required percentage."),
    );
}

fn deliverables(r: &mut Rules, mission: &Mission, ruleset: &Ruleset) {
    if ruleset.required_deliverables.is_empty() {
        r.add(
            "DELIVERABLES.REQUIREMENTS",
            Outcome::InsufficientData,
            "Deliverable requirements are not configured.",
            Extra { next_action: Some("Configure the deliverables required by this project.".into()), ..Extra::default() },
        );
        return;
    }
    let missing = ruleset.required_deliverables.iter().any(|d| !mission.present_deliverables.contains(d));
    r.add(
        "DELIVERABLES.PRESENCE",
        if missing { Outcome::InsufficientData } else { Outcome::Pass },
        if missing { "Required deliverables are missing." } else { "Required deliverables are present; coordinates checked separately." },
        Extra {
            fields: &["present_deliverables"],
            observed: Some(json!(mission.present_deliverables)),
            requirement: Some(format!("Required: {}", ruleset.required_deliverables.join(", "))),
            next_action: missing.then(|| "Supply the required deliverables absent from the documented inventory.".into()),
            ..Extra::default()
        },
    );
}

/// Most diagnostics a report may carry (the parser's own cap).
pub const MAX_DIAGNOSTICS: usize = 256;

/// Check a parse result that came from outside the parser (a caller's JSON), with the reference
/// `QAMetrics`/`ReportEvidence` rules: finite, non-negative measurements, calibrated images no more
/// than the total, bounded text and lists, and the qualified residual provenance and unit.
/// Errors name the field, never its value.
pub fn validate_report(report: &ParseResult) -> Result<(), String> {
    let bad = |m: &str| Err(format!("report: {m}"));
    let text = |s: &str| !s.trim().is_empty() && s.chars().count() <= crate::mission::MAX_TEXT;
    let non_negative = |v: f64| v.is_finite() && v >= 0.0;
    if !text(&report.profile) {
        return bad("profile must be 1 to 2000 characters");
    }
    let m = &report.metrics;
    for (name, v) in [("rmse_x_feet", m.rmse_x_feet), ("rmse_y_feet", m.rmse_y_feet), ("rmse_z_feet", m.rmse_z_feet)] {
        if v.is_some_and(|v| !non_negative(v)) {
            return bad(&format!("metrics.{name} must be finite and not negative"));
        }
    }
    if let (Some(c), Some(t)) = (m.calibrated_images, m.total_images)
        && c > t
    {
        return bad("metrics.calibrated_images exceeds metrics.total_images");
    }
    for (name, v) in [("processing_software", &m.processing_software), ("processing_version", &m.processing_version)] {
        if v.as_deref().is_some_and(|s| !text(s)) {
            return bad(&format!("metrics.{name} must be 1 to 2000 characters"));
        }
    }
    if report.diagnostics.len() > MAX_DIAGNOSTICS {
        return bad("too many diagnostics");
    }
    if report.diagnostics.iter().any(|d| !text(&d.code) || !text(&d.message)) {
        return bad("diagnostic code and message must be 1 to 2000 characters");
    }
    let Some(e) = &report.report_evidence else { return Ok(()) };
    if e.gsd_cm.is_some_and(|v| !non_negative(v)) {
        return bad("report_evidence.gsd_cm must be finite and not negative");
    }
    for (name, r) in [("gcp_fit", &e.gcp_fit), ("camera_offsets", &e.camera_offsets)] {
        if r.as_ref().is_some_and(|r| !residuals_ok(r)) {
            return bad(&format!("report_evidence.{name} is not a qualified residual summary"));
        }
    }
    if e.camera_translation_xyz.is_some_and(|xyz| xyz.iter().any(|v| !v.is_finite())) {
        return bad("report_evidence.camera_translation_xyz must be finite");
    }
    if e.camera_translation_unit.as_deref().is_some_and(|u| u != US_FOOT) {
        return bad("report_evidence.camera_translation_unit must be US survey foot");
    }
    let texts = [
        ("image_crs", &e.image_crs),
        ("gcp_crs", &e.gcp_crs),
        ("output_crs", &e.output_crs),
        ("classification_time", &e.classification_time),
        ("dtm_resolution_label", &e.dtm_resolution_label),
        ("vertical_datum", &e.vertical_datum),
        ("geoid_model", &e.geoid_model),
    ];
    if let Some((name, _)) = texts.iter().find(|(_, v)| v.as_deref().is_some_and(|s| !text(s))) {
        return bad(&format!("report_evidence.{name} must be 1 to 2000 characters"));
    }
    Ok(())
}

const US_FOOT: &str = "US survey foot";

fn residuals_ok(r: &ReportResiduals) -> bool {
    matches!(r.provenance.as_str(), "fitted_gcps" | "initial_computed_camera_positions")
        && r.unit == US_FOOT
        && r.mean_xyz.iter().all(|v| v.is_finite())
        && r.sigma_xyz.iter().chain(&r.rms_xyz).all(|v| v.is_finite() && *v >= 0.0)
}

/// A review brief of at most 500 characters, built from presentation-safe findings only.
pub fn summarize(analysis: &Analysis) -> String {
    let order = [Outcome::Fail, Outcome::InsufficientData, Outcome::Conditional, Outcome::Pass, Outcome::Info];
    let counts = order
        .iter()
        .filter_map(|o| {
            let n = analysis.findings.iter().filter(|f| f.outcome == *o).count();
            (n > 0).then(|| format!("{n} {}", outcome_name(*o)))
        })
        .collect::<Vec<_>>()
        .join(", ");
    if analysis.outcome == Outcome::Pass {
        return format!("PASS: All configured checks passed. {counts}.").chars().take(500).collect();
    }
    let mut sorted: Vec<&Finding> = analysis.findings.iter().collect();
    sorted.sort_by_key(|f| std::cmp::Reverse(rank(f.outcome)));
    let actions = sorted
        .iter()
        .filter(|f| matches!(f.outcome, Outcome::Fail | Outcome::InsufficientData | Outcome::Conditional))
        .take(3)
        .map(|f| format!("{}: {}", f.rule_id, f.next_action.as_deref().unwrap_or("Review this finding.")))
        .collect::<Vec<_>>();
    let summary = format!("{}: {counts}. {}", outcome_name(analysis.outcome), actions.join(" "));
    if summary.chars().count() <= 500 {
        return summary;
    }
    let head: String = summary.chars().take(497).collect();
    format!("{}...", head.trim_end())
}
