use cadcraft_droneqa::workflow::execute;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

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
