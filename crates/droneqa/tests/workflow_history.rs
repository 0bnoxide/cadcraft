use cadcraft_droneqa::workflow::execute;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn checkpoint_evidence_roundtrips_and_binds_individual_points() {
    let file = History::new();
    let mut p = inputs();
    p.as_object_mut().unwrap().remove("report");
    p.as_object_mut().unwrap().remove("report_text");
    p["checkpoint_units"] = json!("ft");
    let header = "gcp_id,expected_x,expected_y,expected_z,measured_x,measured_y,measured_z\n";
    p["checkpoint_csv"] = json!(format!("{header}A,0,0,0,1,0,0\nB,0,0,0,0,0,0\n"));
    let event = execute(&file.0, &p).unwrap();
    assert_eq!(event["draft"]["checkpoint_evidence"]["results"][0]["gcp_id"], "A");
    let history = execute(&file.0, &json!({"action":"history"})).unwrap();
    assert_eq!(history[0]["draft"], event["draft"]);
    execute(&file.0, &review(&event)).unwrap();
    p["action"] = json!("commit");
    p["draft_id"] = event["draft_id"].clone();
    p["expected_draft_hash"] = event["draft_hash"].clone();
    p["note"] = json!("checkpoint review");
    // Same RMS, different point errors: aggregate-only hashes miss this edit.
    p["checkpoint_csv"] = json!(format!("{header}A,0,0,0,0,0,0\nB,0,0,0,1,0,0\n"));
    assert!(execute(&file.0, &p).unwrap_err().starts_with("SOURCE_CHANGED"));
    p["checkpoint_csv"] = json!(format!("{header}A,10,10,10,11,10,10\nB,10,10,10,10,10,10\n"));
    assert!(execute(&file.0, &p).unwrap_err().starts_with("SOURCE_CHANGED"), "equal residuals must still bind the original source coordinates");
}

#[test]
fn checkpoint_evidence_privacy_and_hostile_size_fail_before_persistence() {
    let file = History::new();
    let mut p = inputs();
    let header = "gcp_id,expected_x,expected_y,expected_z,measured_x,measured_y,measured_z\n";
    p["checkpoint_units"] = json!("ft");
    p["checkpoint_csv"] = json!(format!("{header}private-checkpoint,0,0,0,0,0,0\n"));
    p["sensitive_fields"] = json!(["pilot", "gcp_id"]);
    let event = execute(&file.0, &p).unwrap();
    assert_eq!(event["draft"]["checkpoint_evidence"]["results"][0]["gcp_id"], "[REDACTED]");
    assert!(!std::fs::read_to_string(&file.0).unwrap().contains("private-checkpoint"));
    let before = std::fs::read(&file.0).unwrap();
    p["checkpoint_csv"] = json!(format!("{header}{}", "x,0,0,0,0,0,0\n".repeat(1001)));
    assert!(execute(&file.0, &p).unwrap_err().starts_with("CHECKPOINT_EVIDENCE_LIMIT"));
    assert_eq!(std::fs::read(&file.0).unwrap(), before);
    p["checkpoint_csv"] = json!(format!("{header}password=SYNTHETIC_VALUE,0,0,0,0,0,0\n"));
    assert!(execute(&file.0, &p).unwrap_err().starts_with("SENSITIVE_SOURCE"));
    assert_eq!(std::fs::read(&file.0).unwrap(), before);
}

#[test]
fn legacy_record_remains_readable_but_cannot_be_superseded() {
    use sha2::{Digest, Sha256};
    let file = History::new();
    let mut old = execute(&file.0, &inputs()).unwrap();
    old["draft"].as_object_mut().unwrap().remove("mission_key");
    let legacy: cadcraft_droneqa::workflow::Draft = serde_json::from_value(old["draft"].clone()).unwrap();
    old["draft_hash"] = json!(format!("{:x}", Sha256::digest(serde_json::to_vec(&legacy).unwrap())));
    let mut line = serde_json::to_vec(&old).unwrap();
    line.push(b'\n');
    std::fs::write(&file.0, line).unwrap();
    execute(&file.0, &review(&old)).unwrap();
    let mut p = inputs();
    p["action"] = json!("commit");
    p["draft_id"] = old["draft_id"].clone();
    p["expected_draft_hash"] = old["draft_hash"].clone();
    p["note"] = json!("legacy commit");
    execute(&file.0, &p).unwrap();
    p = inputs();
    p["supersedes_draft_id"] = old["draft_id"].clone();
    let new = execute(&file.0, &p).unwrap();
    execute(&file.0, &review(&new)).unwrap();
    p["action"] = json!("supersede");
    p["draft_id"] = new["draft_id"].clone();
    p["expected_draft_hash"] = new["draft_hash"].clone();
    p["prior_draft_id"] = old["draft_id"].clone();
    p["note"] = json!("replacement");
    assert!(execute(&file.0, &p).unwrap_err().starts_with("LEGACY_SUPERSESSION_TARGET"));
}

#[test]
fn supersede_commits_replacement_and_marks_prior_atomically() {
    let file = History::new();
    let mut p = inputs();
    let old = execute(&file.0, &p).expect("old draft");
    execute(&file.0, &review(&old)).expect("approve");
    p["action"] = json!("commit");
    p["draft_id"] = old["draft_id"].clone();
    p["expected_draft_hash"] = old["draft_hash"].clone();
    p["note"] = json!("original");
    execute(&file.0, &p).expect("commit");
    p = inputs();
    p["supersedes_draft_id"] = old["draft_id"].clone();
    let replacement = execute(&file.0, &p).expect("replacement");
    execute(&file.0, &review(&replacement)).expect("approve replacement");
    p["action"] = json!("supersede");
    p["draft_id"] = replacement["draft_id"].clone();
    p["expected_draft_hash"] = replacement["draft_hash"].clone();
    p["prior_draft_id"] = old["draft_id"].clone();
    p["note"] = json!("corrected evidence");
    let mut wrong = p.clone();
    wrong["prior_draft_id"] = replacement["draft_id"].clone();
    assert!(execute(&file.0, &wrong).unwrap_err().starts_with("SUPERSESSION_NOT_REVIEWED"));
    wrong["prior_draft_id"] = json!("another-record");
    assert!(execute(&file.0, &wrong).unwrap_err().starts_with("SUPERSESSION_NOT_REVIEWED"));
    let mut other = inputs();
    other["mission"]["global_id"] = json!("22222222-2222-4222-8222-222222222222");
    other["supersedes_draft_id"] = old["draft_id"].clone();
    let foreign = execute(&file.0, &other).expect("other mission draft");
    execute(&file.0, &review(&foreign)).expect("approve other");
    other["action"] = json!("supersede");
    other["draft_id"] = foreign["draft_id"].clone();
    other["expected_draft_hash"] = foreign["draft_hash"].clone();
    other["prior_draft_id"] = old["draft_id"].clone();
    other["note"] = json!("must refuse");
    assert!(execute(&file.0, &other).unwrap_err().starts_with("SUPERSESSION_TARGET"), "global mission identity must match");
    let event = execute(&file.0, &p).expect("supersede");
    assert_eq!(event["state"], "COMMITTED");
    assert_eq!(event["supersedes_draft_id"], old["draft_id"]);
    let history = execute(&file.0, &json!({"action":"history"})).expect("replay");
    assert_eq!(history.as_array().unwrap().len(), 8);
    assert!(execute(&file.0, &p).is_err());
    let mut again = inputs();
    again["supersedes_draft_id"] = old["draft_id"].clone();
    let next = execute(&file.0, &again).unwrap();
    execute(&file.0, &review(&next)).unwrap();
    again["action"] = json!("supersede");
    again["prior_draft_id"] = old["draft_id"].clone();
    again["draft_id"] = next["draft_id"].clone();
    again["expected_draft_hash"] = next["draft_hash"].clone();
    again["note"] = json!("must not supersede twice");
    assert!(execute(&file.0, &again).unwrap_err().starts_with("SUPERSESSION_TARGET"));
}

#[test]
fn replacement_refuses_plain_commit_and_uncommitted_prior_without_appending() {
    let file = History::new();
    let old = execute(&file.0, &inputs()).unwrap();
    let mut p = inputs();
    p["supersedes_draft_id"] = old["draft_id"].clone();
    let replacement = execute(&file.0, &p).unwrap();
    execute(&file.0, &review(&replacement)).unwrap();
    p["draft_id"] = replacement["draft_id"].clone();
    p["expected_draft_hash"] = replacement["draft_hash"].clone();
    p["note"] = json!("replacement");
    p["action"] = json!("commit");
    let before = std::fs::read(&file.0).unwrap();
    assert!(execute(&file.0, &p).unwrap_err().starts_with("SUPERSESSION_NOT_REVIEWED"));
    p["action"] = json!("supersede");
    p["prior_draft_id"] = old["draft_id"].clone();
    assert!(execute(&file.0, &p).unwrap_err().starts_with("SUPERSESSION_TARGET"));
    assert_eq!(std::fs::read(&file.0).unwrap(), before);
}

#[test]
fn hostile_report_and_nested_unknown_input_are_rejected_before_persistence() {
    let file = History::new();
    let mut input = inputs();
    let mut report = serde_json::to_value(cadcraft_droneqa::pix4d::parse(b"AIGIS3D SYNTHETIC PIX4D QA v1", "text/plain")).expect("report");
    report["metrics"]["rmse_z_feet"] = json!(-1.0);
    input["report"] = report;
    assert!(execute(&file.0, &input).expect_err("negative report").contains("rmse_z_feet"));
    assert!(!file.0.exists());
    let mut nested = json!(null);
    for _ in 0..64 {
        nested = json!([nested]);
    }
    input = inputs();
    input["unknown"] = nested;
    assert!(execute(&file.0, &input).is_err());
    assert!(!file.0.exists());
}

#[test]
fn configured_private_mission_id_is_not_persisted() {
    let file = History::new();
    let mut input = inputs();
    input["sensitive_fields"] = json!(["mission_id", "pilot"]);
    let draft = execute(&file.0, &input).expect("draft");
    assert_eq!(draft["draft"]["mission_id"], "[REDACTED]");
    assert!(!std::fs::read_to_string(&file.0).expect("history").contains("synthetic-mission"));
}

#[test]
fn edited_draft_and_truncated_commit_fail_closed() {
    let file = History::new();
    let draft = execute(&file.0, &inputs()).expect("draft");
    let original = std::fs::read(&file.0).expect("history");
    let mut edited = draft.clone();
    edited["draft"]["source_hash"] = json!("edited");
    let mut bytes = serde_json::to_vec(&edited).expect("encode");
    bytes.push(b'\n');
    std::fs::write(&file.0, bytes).expect("edit");
    assert!(execute(&file.0, &review(&draft)).expect_err("integrity").starts_with("INVALID_HISTORY"));
    let mut truncated = original.clone();
    truncated.extend_from_slice(b"{\"state\":\"COMMIT");
    std::fs::write(&file.0, truncated).expect("truncate");
    assert!(execute(&file.0, &review(&draft)).expect_err("truncated").starts_with("INVALID_HISTORY"));
    assert!(std::fs::read(&file.0).expect("unchanged").starts_with(&original));
}
struct History(PathBuf);
impl History {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "cadcraft-qa-{}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for History {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn inputs() -> Value {
    json!({"action":"draft", "mission":{"global_id":"11111111-1111-4111-8111-111111111111", "mission_id":"synthetic-mission","pilot":"Private Person"},"ruleset":{"version":"fixture-1","required_properties":[]}})
}
fn review(draft: &Value) -> Value {
    json!({"action":"review","draft_id":draft["draft_id"],"expected_draft_hash":draft["draft_hash"],"decision":"APPROVED","reviewer":"Synthetic Reviewer","note":"Checked synthetic evidence"})
}

#[test]
fn append_only_draft_review_commit_and_repeat_guard() {
    let file = History::new();
    let mut input = inputs();
    let draft = execute(&file.0, &input).expect("draft");
    let original = std::fs::read(&file.0).expect("read draft");
    let approval = execute(&file.0, &review(&draft)).expect("review");
    assert_eq!(approval["state"], "APPROVED");
    input["action"] = json!("commit");
    input["draft_id"] = draft["draft_id"].clone();
    input["expected_draft_hash"] = draft["draft_hash"].clone();
    input["note"] = json!("Approve local record");
    assert_eq!(execute(&file.0, &input).expect("commit")["state"], "COMMITTED");
    assert!(execute(&file.0, &input).expect_err("duplicate").starts_with("ALREADY_COMMITTED"));
    let bytes = std::fs::read(&file.0).expect("read");
    assert!(bytes.starts_with(&original));
    assert!(!String::from_utf8_lossy(&bytes).contains("Private Person"));
    assert_eq!(execute(&file.0, &json!({"action":"history"})).expect("history").as_array().expect("array").len(), 3);
}

#[test]
fn exact_review_source_and_rejection_guards() {
    let file = History::new();
    let mut input = inputs();
    let draft = execute(&file.0, &input).expect("draft");
    let mut request = review(&draft);
    request["expected_draft_hash"] = json!("wrong");
    assert!(execute(&file.0, &request).is_err());
    request = review(&draft);
    request["decision"] = json!("REJECTED");
    execute(&file.0, &request).expect("reject");
    input["action"] = json!("commit");
    input["draft_id"] = draft["draft_id"].clone();
    input["expected_draft_hash"] = draft["draft_hash"].clone();
    input["note"] = json!("Approve local record");
    assert!(execute(&file.0, &input).expect_err("not approved").starts_with("REVIEW_REQUIRED"));
    execute(&file.0, &review(&draft)).expect("approve");
    input["mission"]["pilot"] = json!("Changed private source");
    assert!(execute(&file.0, &input).expect_err("source changed").starts_with("SOURCE_CHANGED"));
}

#[test]
fn sensitive_sources_corrupt_history_and_busy_lock_fail_closed() {
    let file = History::new();
    let mut input = inputs();
    input["mission"]["pilot"] = json!("token=SYNTHETIC_PRIVATE");
    assert!(execute(&file.0, &input).expect_err("credential").starts_with("SENSITIVE_SOURCE"));
    assert!(!file.0.exists());
    std::fs::write(&file.0, b"{partial").expect("corrupt fixture");
    assert!(execute(&file.0, &json!({"action":"history"})).expect_err("corrupt").starts_with("INVALID_HISTORY"));
    let lock = PathBuf::from(format!("{}.lock", file.0.display()));
    std::fs::write(&lock, b"").expect("lock");
    assert!(execute(&file.0, &inputs()).expect_err("busy").starts_with("HISTORY_BUSY"));
    std::fs::remove_file(lock).expect("unlock");
}
