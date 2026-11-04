//! Single-loop differential levelling. Reference: AutoGIS `core/envmon/level_loop.py` (ADR-0026).
//! Owner: Codex (see `docs/agents/slice-1-survey.md`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

use crate::export::{MAX_ROWS, column, finite, invalid, number, read_csv, value};
use crate::{Finding, Severity};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observation {
    pub run_id: String,
    pub setup_id: String,
    pub point_id: String,
    pub backsight: Option<f64>,
    pub foresight: Option<f64>,
    pub intermediate_sight: Option<f64>,
    pub hi: Option<f64>,
    pub elevation: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct LoopOptions {
    pub run_id: String,
    pub site_id: String,
    /// Opaque survey-date metadata, carried unchanged into the update plan.
    pub survey_date: String,
    pub benchmark_id: String,
    pub known_elevation: f64,
    pub tolerance: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct LevelLoopRun {
    pub run_id: String,
    pub site_id: String,
    pub survey_date: String,
    pub benchmark_id: String,
    pub known_elevation: f64,
    pub misclosure: Option<f64>,
    pub closure_tolerance: f64,
    pub adjusted: bool,
    /// Any ERROR finding; promotion also refuses structurally invalid observations.
    pub has_errors: bool,
}

#[derive(Debug)]
pub struct LoopResult {
    pub run: LevelLoopRun,
    pub observations: Vec<Observation>,
    pub findings: Vec<Finding>,
}

pub fn read_observations(reader: impl Read) -> io::Result<Vec<Observation>> {
    let mut observations = Vec::new();
    read_csv(reader, |header, row| {
        let run = column(header, &["run_id"])?;
        let setup = column(header, &["setup_id"])?;
        let point = column(header, &["point_id"])?;
        let bs = column(header, &["backsight"])?;
        let fs = column(header, &["foresight"])?;
        let side = column(header, &["intermediate_sight"])?;
        if row.is_empty() {
            return Ok(());
        }
        let reading = |i| {
            let s = value(row, i).trim();
            if s.is_empty() { Ok(None) } else { number(s).map(Some) }
        };
        observations.push(Observation {
            run_id: value(row, run).into(),
            setup_id: value(row, setup).into(),
            point_id: value(row, point).into(),
            backsight: reading(bs)?,
            foresight: reading(fs)?,
            intermediate_sight: reading(side)?,
            ..Default::default()
        });
        Ok(())
    })?;
    Ok(observations)
}

fn info(message: String) -> Finding {
    Finding { severity: Severity::Info, message }
}

/// AutoGIS HI rules: foresight before backsight; side shots do not advance HI.
/// shortcut: equal-per-setup adjustment, add distance weighting only with a new survey contract.
pub fn process_level_loop(observations: &[Observation], options: &LoopOptions) -> io::Result<LoopResult> {
    if observations.len() > MAX_ROWS {
        return Err(invalid("level loop exceeds 1,000,000 observations"));
    }
    finite(options.known_elevation)?;
    if options.tolerance.is_some_and(|t| !t.is_finite() || t < 0.0) {
        return Err(invalid("tolerance must be finite and nonnegative"));
    }
    let mut findings = Vec::new();
    if observations.first().is_some_and(|row| row.point_id != options.benchmark_id) {
        findings.push(Finding::error("benchmark_mismatch: the first observation must be on the starting benchmark"));
    }
    for obs in observations {
        for (name, reading) in [("backsight", obs.backsight), ("foresight", obs.foresight), ("intermediate_sight", obs.intermediate_sight)] {
            if let Some(reading) = reading {
                finite(reading)?;
                if reading < 0.0 {
                    findings.push(Finding::error(format!("negative_reading: {name} {reading} at setup={} point={}", obs.setup_id, obs.point_id)));
                }
            }
        }
    }
    let mut elevations = BTreeMap::from([(options.benchmark_id.as_str(), options.known_elevation)]);
    let mut hi = None;
    let mut setups = 0_u32;
    let mut turning_points = Vec::new();
    let mut used = BTreeSet::new();
    let mut closing = None;
    let mut setup_open = false;
    let mut rows = Vec::new();
    for (row_index, obs) in observations.iter().enumerate() {
        let mut row = Observation { hi: None, elevation: None, ..obs.clone() };
        if let Some(fs) = obs.foresight.filter(|r| *r >= 0.0) {
            if let Some(height) = hi {
                let elevation = finite(height - fs)?;
                row.elevation = Some(elevation);
                setup_open = false;
                hi = None;
                if obs.point_id == options.benchmark_id {
                    closing = Some(elevation);
                } else {
                    elevations.insert(&obs.point_id, elevation);
                    if !used.insert(obs.point_id.as_str()) {
                        findings.push(Finding::warning(format!("duplicate_turning_point: point {:?} used more than once", obs.point_id)));
                    }
                    turning_points.push(row_index);
                }
            } else {
                findings.push(Finding::error(format!("missing_backsight: foresight at point={} has no HI", obs.point_id)));
            }
        }
        if let Some(bs) = obs.backsight.filter(|r| *r >= 0.0) {
            if let Some(elevation) = elevations.get(obs.point_id.as_str()) {
                if setup_open {
                    findings
                        .push(Finding::error(format!("missing_foresight: a new backsight at point={} replaces an unfinished setup", obs.point_id)));
                }
                hi = Some(finite(elevation + bs)?);
                closing = None;
                row.hi = hi;
                setup_open = true;
                setups += 1; // Bounded by MAX_ROWS.
            } else {
                findings.push(Finding::error(format!("missing_backsight: elevation of {:?} is unknown", obs.point_id)));
            }
        }
        if let Some(side) = obs.intermediate_sight.filter(|r| *r >= 0.0) {
            if let Some(height) = hi {
                let elevation = finite(height - side)?;
                row.elevation = Some(elevation);
                elevations.insert(&obs.point_id, elevation);
            } else {
                findings.push(Finding::error(format!("missing_backsight: intermediate sight at point={} has no HI", obs.point_id)));
            }
        }
        rows.push(row);
    }
    if setup_open {
        findings.push(Finding::error("missing_foresight: the final setup has no closing foresight"));
    }
    let misclosure = closing.map(|e| finite(e - options.known_elevation)).transpose()?;
    if closing.is_none() {
        findings.push(Finding::warning(format!("unclosed_loop: no closing foresight on {:?}", options.benchmark_id)));
        findings.push(Finding::error(format!("benchmark_mismatch: no closing shot onto {:?}", options.benchmark_id)));
    }
    let tolerance = options.tolerance.unwrap_or_else(|| 0.05 * f64::from(setups.max(1)).sqrt());
    if options.tolerance.is_none() {
        findings.push(info(format!("closure_tolerance_default: 0.05 * sqrt({setups}) = {tolerance:.6} (input units)")));
    }
    let misclosure_text = misclosure.map_or_else(|| "N/A".into(), |m| format!("{m:.6}"));
    if misclosure.is_some_and(|m| m.abs() > tolerance) {
        findings.push(Finding::error(format!("misclosure_exceeds_tolerance: {misclosure_text} exceeds {tolerance:.6}")));
    }
    let adjusted = misclosure.is_some_and(|m| m != 0.0) && setups > 0;
    if adjusted {
        let correction = -misclosure.unwrap_or_default() / f64::from(setups);
        let mut corrections = BTreeMap::new();
        for (i, row_index) in turning_points.into_iter().enumerate() {
            corrections.insert(row_index, finite((i + 1) as f64 * correction)?);
        }
        let mut active = 0.0;
        for (row_index, row) in rows.iter_mut().enumerate() {
            if row.foresight.is_some() && row.point_id != options.benchmark_id {
                active = corrections.get(&row_index).copied().unwrap_or(active);
                row.elevation = row.elevation.map(|e| finite(e + active)).transpose()?;
            } else if row.intermediate_sight.is_some() {
                row.elevation = row.elevation.map(|e| finite(e + active)).transpose()?;
            }
        }
    }
    findings.push(info(format!("level_loop_complete: run={} setups={setups} misclosure={misclosure_text} adjusted={adjusted}", options.run_id)));
    Ok(LoopResult {
        run: LevelLoopRun {
            run_id: options.run_id.clone(),
            site_id: options.site_id.clone(),
            survey_date: options.survey_date.clone(),
            benchmark_id: options.benchmark_id.clone(),
            known_elevation: options.known_elevation,
            misclosure,
            closure_tolerance: tolerance,
            adjusted,
            has_errors: findings.iter().any(|f| f.severity == Severity::Error),
        },
        observations: rows,
        findings,
    })
}

#[derive(Debug)]
pub struct ElevationUpdatePlan {
    pub run_id: String,
    pub survey_date: String,
    pub updates: BTreeMap<String, f64>,
    pub skipped: Vec<String>,
    pub block_reason: Option<String>,
}

/// Headless selection only. The benchmark is excluded, and the last elevation per point wins.
pub fn select_elevations_for_update(
    run: &LevelLoopRun,
    observations: &[Observation],
    well_ids: &BTreeSet<String>,
) -> io::Result<ElevationUpdatePlan> {
    if observations.len() > MAX_ROWS {
        return Err(invalid("level loop exceeds 1,000,000 observations"));
    }
    finite(run.closure_tolerance)?;
    if let Some(m) = run.misclosure {
        finite(m)?;
    }
    if run.closure_tolerance < 0.0 {
        return Err(invalid("tolerance must be nonnegative"));
    }
    let block_reason = if !run.adjusted {
        Some("not_adjusted")
    } else if run.misclosure.is_some_and(|m| m.abs() > run.closure_tolerance) {
        Some("misclosure_exceeds_tolerance")
    } else if run.has_errors {
        Some("invalid_observations")
    } else {
        None
    };
    let mut plan = ElevationUpdatePlan {
        run_id: run.run_id.clone(),
        survey_date: run.survey_date.clone(),
        updates: BTreeMap::new(),
        skipped: Vec::new(),
        block_reason: block_reason.map(str::to_string),
    };
    if plan.block_reason.is_some() {
        return Ok(plan);
    }
    let mut skipped = BTreeSet::new();
    for obs in observations {
        if obs.point_id == run.benchmark_id {
            continue;
        }
        if let Some(elevation) = obs.elevation {
            finite(elevation)?;
            if well_ids.contains(&obs.point_id) {
                plan.updates.insert(obs.point_id.clone(), elevation);
            } else if skipped.insert(&obs.point_id) {
                plan.skipped.push(obs.point_id.clone());
            }
        }
    }
    Ok(plan)
}
