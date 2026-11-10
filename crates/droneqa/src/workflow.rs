//! Draft, review and commit with an append-only audit history, stored without a database (owner
//! decision: in the drawing or a JSON file). Reference: AIGIS3D `workflow.py`, `storage.py`. Owner:
//! Codex (see `docs/agents/slice-3-droneqa.md`).

use crate::mission::{Mission, Ruleset, check_field_names};
use crate::{Analysis, ParseResult, pix4d, qa, redact::redact};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_HISTORY: u64 = 32_000_000;
const MAX_EVENTS: usize = 10_000;

fn hash<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "INVALID_INPUT: cannot serialize evidence")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn bounded(value: &Value) -> Result<(), String> {
    crate::mission::check_shape("qareview", value)?;
    let mut pending = vec![(value, 0)];
    let mut count = 0usize;
    let mut text_bytes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        count += 1;
        if depth > 48 || count > 50_000 || pending.len() > 50_000 {
            return Err("INVALID_INPUT: input is too deeply nested or has too many values".into());
        }
        match value {
            Value::Object(fields) if fields.len() > 50_000 => return Err("INVALID_INPUT: too many fields".into()),
            Value::Array(values) if values.len() > 50_000 => return Err("INVALID_INPUT: too many values".into()),
            Value::Object(fields) => {
                for key in fields.keys() {
                    text_bytes = text_bytes.saturating_add(key.len());
                }
                if text_bytes > 25_000_000 {
                    return Err("INVALID_INPUT: text exceeds byte limit".into());
                }
                pending.extend(fields.values().map(|value| (value, depth + 1)));
            }
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, depth + 1))),
            Value::String(text) => {
                text_bytes = text_bytes.saturating_add(text.len());
                if text_bytes > 25_000_000 {
                    return Err("INVALID_INPUT: text exceeds byte limit".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn string<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params.get(key).and_then(Value::as_str).ok_or_else(|| format!("INVALID_INPUT: {key} must be a string"))
}

fn note(params: &Value, key: &str, max: usize) -> Result<String, String> {
    let text = string(params, key)?.trim();
    if text.is_empty() || text.chars().count() > max {
        return Err(format!("INVALID_INPUT: {key} must be nonempty and bounded"));
    }
    if redact(&json!(text), &[]) != json!(text) {
        return Err("SENSITIVE_INPUT: remove credential-like input".into());
    }
    Ok(text.into())
}

struct Context {
    mission: Mission,
    ruleset: Ruleset,
    report: Option<ParseResult>,
    sensitive: Vec<String>,
    source_hash: String,
    checkpoint_evidence: Option<Value>,
}

fn context(params: &Value) -> Result<Context, String> {
    bounded(params)?;
    if redact(params, &[]) != *params {
        return Err("SENSITIVE_SOURCE: remove credential-like source content".into());
    }
    let mission =
        Mission::from_value(params.get("mission").ok_or("INVALID_INPUT: mission required")?).map_err(|_| "INVALID_INPUT: invalid mission")?;
    let ruleset =
        Ruleset::from_value(params.get("ruleset").ok_or("INVALID_INPUT: ruleset required")?).map_err(|_| "INVALID_INPUT: invalid ruleset")?;
    let sensitive: Vec<String> = match params.get("sensitive_fields") {
        None | Some(Value::Null) => vec!["pilot".into()],
        Some(value) => serde_json::from_value(value.clone()).map_err(|_| "INVALID_INPUT: invalid sensitive_fields")?,
    };
    let mission_sensitive: Vec<_> = sensitive.iter().filter(|s| !CHECKPOINT_FIELDS.contains(&s.as_str())).cloned().collect();
    check_field_names("sensitive_fields", &mission_sensitive).map_err(|_| "INVALID_INPUT: unknown sensitive field")?;
    let content_type = match params.get("content_type") {
        None | Some(Value::Null) => "text/plain",
        Some(Value::String(value)) => value,
        _ => return Err("INVALID_INPUT: content_type must be a string".into()),
    };
    let mut report = match (params.get("report").filter(|value| !value.is_null()), params.get("report_text").filter(|value| !value.is_null())) {
        (Some(_), Some(_)) => return Err("INVALID_INPUT: give report or report_text, not both".into()),
        (Some(value), None) => Some(serde_json::from_value(value.clone()).map_err(|_| "INVALID_INPUT: invalid report")?),
        (None, Some(Value::String(text))) => Some(pix4d::parse(text.as_bytes(), content_type)),
        (None, Some(_)) => return Err("INVALID_INPUT: report_text must be a string".into()),
        (None, None) => None,
    };
    let mut checkpoint_evidence = None;
    let mut checkpoint_hash = None;
    if let Some(csv) = params.get("checkpoint_csv").filter(|v| !v.is_null()) {
        if report.is_some() {
            return Err("INVALID_CHECKPOINT_CSV: give only one report input".into());
        }
        let text = csv.as_str().ok_or("INVALID_CHECKPOINT_CSV: text required")?;
        let units = params.get("checkpoint_units").and_then(Value::as_str).ok_or("INVALID_CHECKPOINT_CSV: checkpoint_units required")?;
        // The JSONL record is bounded too; refuse before constructing per-point evidence.
        if text.len() > 1_000_000 || text.lines().filter(|l| !l.trim().is_empty()).count() > 1001 {
            return Err("CHECKPOINT_EVIDENCE_LIMIT: at most 1000 bounded checkpoint rows".into());
        }
        let (mut parsed, summary) =
            crate::checkpoint::evaluate(text, units, ruleset.accuracy.maximum_rmse_horizontal_feet, ruleset.accuracy.maximum_rmse_z_feet)?;
        if summary.results.iter().any(|p| p.gcp_id.len() > 256) {
            return Err("CHECKPOINT_EVIDENCE_LIMIT: checkpoint id exceeds 256 bytes".into());
        }
        let raw = serde_json::to_value(&summary).map_err(|_| "INVALID_CHECKPOINT_CSV: cannot serialize evidence")?;
        if serde_json::to_vec(&raw).map_err(|_| "INVALID_CHECKPOINT_CSV: cannot serialize evidence")?.len() > 250_000 {
            return Err("CHECKPOINT_EVIDENCE_LIMIT: checkpoint evidence exceeds 250000 bytes".into());
        }
        checkpoint_hash = Some(hash(&json!({"units":units,"summary":raw,"csv_sha256":format!("{:x}",Sha256::digest(text.as_bytes()))}))?);
        checkpoint_evidence = Some(redact(&raw, &sensitive));
        parsed.metrics.checkpoints_independent = mission.checkpoints_independent;
        parsed.metrics.checkpoints_bare_ground = mission.checkpoints_bare_ground;
        report = Some(parsed);
    }
    if let Some(report) = &report {
        qa::validate_report(report)?;
    }
    let mut source = json!({"mission":mission,"ruleset":ruleset,"report":report,"sensitive_fields":sensitive});
    if let Some(checkpoints) = checkpoint_hash {
        source["checkpoint_sha256"] = json!(checkpoints);
    }
    let source_hash = hash(&source)?;
    Ok(Context { mission, ruleset, report, sensitive, source_hash, checkpoint_evidence })
}

const CHECKPOINT_FIELDS: [&str; 5] = ["gcp_id", "delta_xyz_feet", "horizontal_error_feet", "vertical_error_feet", "point_pass"];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_evidence: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_draft_id: Option<String>,
    pub schema_version: u32,
    pub draft_id: String,
    pub mission_id: Option<String>,
    pub ruleset_version: String,
    pub analysis: Analysis,
    pub sensitive_fields: Vec<String>,
    pub state: String,
    pub timestamp: String,
    pub source_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    supersedes_draft_id: Option<String>,
    schema_version: u32,
    draft_id: String,
    state: String,
    timestamp: String,
    draft_hash: String,
    previous_hash: String,
    draft: Option<Draft>,
    reviewer: Option<String>,
    note: Option<String>,
}

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn load(path: &Path) -> Result<Vec<Event>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("HISTORY_IO: cannot open history".into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_HISTORY + 1).read_to_end(&mut bytes).map_err(|_| "HISTORY_IO: cannot read history")?;
    if bytes.len() as u64 > MAX_HISTORY {
        return Err("HISTORY_LIMIT: history exceeds byte limit".into());
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err("INVALID_HISTORY: incomplete record".into());
    }
    let mut events: Vec<Event> = Vec::new();
    let mut previous = String::new();
    for line in bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
        if events.len() >= MAX_EVENTS || line.len() > 2_000_000 {
            return Err("HISTORY_LIMIT: history exceeds record limit".into());
        }
        let event: Event = serde_json::from_slice(line).map_err(|_| "INVALID_HISTORY: malformed record")?;
        if event.schema_version != 1 || event.previous_hash != previous {
            return Err("INVALID_HISTORY: schema or chain mismatch".into());
        }
        let prior = events.iter().rev().find(|prior| prior.draft_id == event.draft_id);
        let valid = match (event.state.as_str(), prior) {
            ("DRAFT", None) => event.draft.as_ref().is_some_and(|draft| {
                draft.schema_version == 1
                    && draft.state == "DRAFT"
                    && draft.draft_id == event.draft_id
                    && hash(draft).ok().as_ref() == Some(&event.draft_hash)
            }),
            ("APPROVED" | "REJECTED", Some(prior)) => {
                prior.state != "COMMITTED"
                    && !events.iter().any(|e| e.supersedes_draft_id.as_deref() == Some(&event.draft_id))
                    && event.draft.is_none()
                    && event.draft_hash == prior.draft_hash
                    && event.reviewer.as_ref().is_some_and(|value| !value.trim().is_empty())
                    && event.note.as_ref().is_some_and(|value| !value.trim().is_empty())
            }
            ("COMMITTED", Some(prior)) => {
                prior.state == "APPROVED"
                    && event.draft.is_none()
                    && event.draft_hash == prior.draft_hash
                    && event.note.as_ref().is_some_and(|value| !value.trim().is_empty())
            }
            _ => false,
        };
        if !valid {
            return Err("INVALID_HISTORY: invalid transition or draft integrity".into());
        }
        if let Some(old_id) = &event.supersedes_draft_id {
            let replacement = events.iter().find_map(|e| e.draft.as_ref().filter(|d| d.draft_id == event.draft_id));
            let old = events.iter().rev().find(|e| &e.draft_id == old_id);
            let old_draft = events.iter().find_map(|e| e.draft.as_ref().filter(|d| &d.draft_id == old_id));
            if event.state != "COMMITTED"
                || old_id == &event.draft_id
                || old.is_none_or(|e| e.state != "COMMITTED")
                || events.iter().any(|e| e.supersedes_draft_id.as_ref() == Some(old_id))
                || replacement.zip(old_draft).is_none_or(|(new, old)| {
                    new.supersedes_draft_id.as_ref() != Some(old_id)
                        || new.mission_key.is_none()
                        || new.mission_key != old.mission_key
                        || new.mission_id.is_none()
                        || new.mission_id.as_deref() == Some("[REDACTED]")
                        || new.mission_id != old.mission_id
                })
            {
                return Err("INVALID_HISTORY: invalid supersession".into());
            }
        }
        let value = serde_json::to_value(&event).map_err(|_| "INVALID_HISTORY: invalid record")?;
        if redact(&value, &[]) != value {
            return Err("INVALID_HISTORY: sensitive content".into());
        }
        previous = hash(&event)?;
        events.push(event);
    }
    Ok(events)
}

/// Execute the local review protocol. Every mutation is locked and appends one durable JSON line.
pub fn execute(path: &Path, params: &Value) -> Result<Value, String> {
    bounded(params)?;
    let action = string(params, "action")?;
    let mut lock_name = path.as_os_str().to_os_string();
    lock_name.push(".lock");
    let lock_path = PathBuf::from(lock_name);
    let lock_file =
        OpenOptions::new().write(true).create_new(true).open(&lock_path).map_err(|_| "HISTORY_BUSY: lock unavailable; retry after the writer finishes. If a writer was terminated, confirm no review is running before deleting the history .lock file.")?;
    let _lock = Lock(lock_path);
    drop(lock_file);
    let events = load(path)?;
    if action == "history" {
        return serde_json::to_value(events).map_err(|_| "INVALID_HISTORY: cannot present records".into());
    }
    if events.len() >= MAX_EVENTS {
        return Err("HISTORY_LIMIT: maximum event count reached".into());
    }
    let timestamp = chrono::DateTime::<Utc>::from(std::time::SystemTime::now()).to_rfc3339();
    let previous_hash = events.last().map(hash).transpose()?.unwrap_or_default();
    let mut event = Event {
        supersedes_draft_id: None,
        schema_version: 1,
        draft_id: String::new(),
        state: String::new(),
        timestamp: timestamp.clone(),
        draft_hash: String::new(),
        previous_hash,
        draft: None,
        reviewer: None,
        note: None,
    };
    match action {
        "draft" => {
            let ctx = context(params)?;
            let analysis = qa::analyze(&ctx.mission, &ctx.ruleset, ctx.report.as_ref(), Some(&ctx.sensitive));
            let draft_id = hash(&json!([ctx.source_hash, timestamp, events.len()]))?;
            let mission_id = if ctx.sensitive.iter().any(|field| field == "mission_id") { Some("[REDACTED]".into()) } else { ctx.mission.mission_id };
            let draft = Draft {
                checkpoint_evidence: ctx.checkpoint_evidence,
                mission_key: Some(hash(&ctx.mission.global_id)?),
                supersedes_draft_id: params
                    .get("supersedes_draft_id")
                    .filter(|v| !v.is_null())
                    .map(|_| note(params, "supersedes_draft_id", 128))
                    .transpose()?,
                schema_version: 1,
                draft_id: draft_id.clone(),
                mission_id,
                ruleset_version: ctx.ruleset.version,
                analysis,
                sensitive_fields: ctx.sensitive,
                state: "DRAFT".into(),
                timestamp,
                source_hash: ctx.source_hash,
            };
            event.draft_id = draft_id;
            event.state = "DRAFT".into();
            event.draft_hash = hash(&draft)?;
            event.draft = Some(draft);
        }
        "review" | "commit" | "supersede" => {
            let draft_id = string(params, "draft_id")?;
            let prior = events.iter().rev().find(|event| event.draft_id == draft_id).ok_or("DRAFT_NOT_FOUND: prepare a draft")?;
            if string(params, "expected_draft_hash")? != prior.draft_hash {
                return Err("PAYLOAD_HASH_MISMATCH: review must bind the exact draft".into());
            }
            if prior.state == "COMMITTED" {
                return Err("ALREADY_COMMITTED: no additional event written".into());
            }
            if events.iter().any(|e| e.supersedes_draft_id.as_deref() == Some(draft_id)) {
                return Err("SUPERSESSION_TARGET: record is superseded".into());
            }
            event.draft_id = draft_id.into();
            event.draft_hash = prior.draft_hash.clone();
            event.note = Some(note(params, "note", 2000)?);
            if action == "review" {
                let decision = string(params, "decision")?;
                if !["APPROVED", "REJECTED"].contains(&decision) {
                    return Err("INVALID_REVIEW: decision must be APPROVED or REJECTED".into());
                }
                event.reviewer = Some(note(params, "reviewer", 200)?);
                event.state = decision.into();
            } else {
                if prior.state != "APPROVED" {
                    return Err("REVIEW_REQUIRED: approve the exact draft first".into());
                }
                let draft = events
                    .iter()
                    .find_map(|event| event.draft.as_ref().filter(|draft| draft.draft_id == draft_id))
                    .ok_or("INVALID_HISTORY: draft missing")?;
                let ctx = context(params)?;
                if ctx.source_hash != draft.source_hash {
                    return Err("SOURCE_CHANGED: prepare and review a new draft".into());
                }
                let analysis = qa::analyze(&ctx.mission, &ctx.ruleset, ctx.report.as_ref(), Some(&ctx.sensitive));
                if analysis != draft.analysis {
                    return Err("ANALYSIS_CHANGED: prepare and review a new draft".into());
                }
                if action == "supersede" {
                    let old_id = string(params, "prior_draft_id")?;
                    if old_id == draft_id || draft.supersedes_draft_id.as_deref() != Some(old_id) {
                        return Err("SUPERSESSION_NOT_REVIEWED: replacement must name the prior record".into());
                    }
                    let old = events.iter().rev().find(|e| e.draft_id == old_id).ok_or("SUPERSESSION_TARGET: prior record missing")?;
                    let old_draft = events
                        .iter()
                        .find_map(|e| e.draft.as_ref().filter(|d| d.draft_id == old_id))
                        .ok_or("INVALID_HISTORY: prior draft missing")?;
                    if old_draft.mission_key.is_none() {
                        return Err("LEGACY_SUPERSESSION_TARGET: prior record has no verified mission identity".into());
                    }
                    if old.state != "COMMITTED"
                        || events.iter().any(|e| e.supersedes_draft_id.as_deref() == Some(old_id))
                        || draft.mission_id.as_deref().is_none_or(|id| id.is_empty() || id == "[REDACTED]")
                        || draft.mission_id != old_draft.mission_id
                        || draft.mission_key.is_none()
                        || draft.mission_key != old_draft.mission_key
                    {
                        return Err("SUPERSESSION_TARGET: prior must be a committed current record for this mission".into());
                    }
                    event.supersedes_draft_id = Some(old_id.into());
                } else if draft.supersedes_draft_id.is_some() {
                    return Err("SUPERSESSION_NOT_REVIEWED: use supersede for a replacement".into());
                }
                event.state = "COMMITTED".into();
            }
        }
        _ => return Err("INVALID_INPUT: action must be draft, review, commit, supersede or history".into()),
    }
    let value = serde_json::to_value(&event).map_err(|_| "INVALID_INPUT: cannot serialize event")?;
    if redact(&value, &[]) != value {
        return Err("SENSITIVE_INPUT: event contains credential-like content".into());
    }
    let mut bytes = serde_json::to_vec(&event).map_err(|_| "INVALID_INPUT: cannot serialize event")?;
    bytes.push(b'\n');
    let current = fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0);
    if bytes.len() > 2_000_000 || current.saturating_add(bytes.len() as u64) > MAX_HISTORY {
        return Err("HISTORY_LIMIT: event exceeds history capacity".into());
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(|_| "HISTORY_IO: cannot append event")?;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| "HISTORY_IO: append uncertain; inspect history before retry")?;
    Ok(value)
}
