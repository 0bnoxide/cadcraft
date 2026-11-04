//! PIX4DIMPORT and QAREVIEW (draft, review, commit). Owner: Codex (see
//! `docs/agents/slice-3-droneqa.md`).

use super::{CommandSpec, always, bad};
use crate::{Result, Session};
use serde_json::Value;
use std::io::Read;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("pix4dimport", "Import PIX4D Report", import).enabled(always).noundo()
            .params("{text?: UTF-8 report | path?: report file, content_type?: text/plain} → ParseResult; PDF is not supported yet"),
        CommandSpec::new("qareview", "QA Review", review).enabled(always).noundo()
            .params("{action: draft/review/commit/history, history: explicit JSONL sidecar path; draft: mission/ruleset/report or report_text/sensitive_fields; review: draft_id/expected_draft_hash/decision/reviewer/note; commit: draft_id/expected_draft_hash/note plus original draft inputs} → immutable audit event or history"),
    ]
}

fn import(_: &mut Session, params: &Value) -> Result<Value> {
    let id = "pix4dimport";
    let content_type = match params.get("content_type") {
        None | Some(Value::Null) => "text/plain",
        Some(Value::String(value)) => value,
        _ => return Err(bad(id, "content_type must be a string")),
    };
    let bytes = match (params.get("text"), params.get("path")) {
        (Some(Value::String(text)), None) => {
            if text.len() > 25_000_000 {
                return Err(bad(id, "report exceeds 25 MB"));
            }
            text.as_bytes().to_vec()
        }
        (None, Some(Value::String(path))) => {
            let file = std::fs::File::open(path).map_err(|_| bad(id, "cannot open report"))?;
            let mut bytes = Vec::new();
            file.take(25_000_001).read_to_end(&mut bytes).map_err(|_| bad(id, "cannot read report"))?;
            bytes
        }
        _ => return Err(bad(id, "give exactly one string: text or path")),
    };
    serde_json::to_value(cadcraft_droneqa::pix4d::parse(&bytes, content_type)).map_err(|_| bad(id, "cannot serialize parser result"))
}

fn review(_: &mut Session, params: &Value) -> Result<Value> {
    let path = params
        .get("history")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| bad("qareview", "history path required"))?;
    cadcraft_droneqa::workflow::execute(std::path::Path::new(path), params).map_err(|error| bad("qareview", error))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn import_analyze_draft_review_commit_share_the_same_analysis() {
        let mut session = Session::new();
        let path = std::env::temp_dir().join(format!(
            "cadcraft-qa-engine-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let report = session.execute("pix4dimport", &json!({"text":include_str!("../../../droneqa/tests/fixtures/valid.txt")})).expect("import");
        let defaulted = session
            .execute("pix4dimport", &json!({"text":include_str!("../../../droneqa/tests/fixtures/valid.txt"),"content_type":null}))
            .expect("null content type defaults");
        assert_eq!(defaulted, report);
        assert_eq!(report["diagnostics"], json!([]));
        let mut params = json!({"mission":{"global_id":"11111111-1111-4111-8111-111111111111","mission_id":"synthetic-engine","pilot":"Private Person"},"ruleset":{"version":"fixture-1","required_properties":[]},"report":report});
        let preview = session.execute("droneqa", &params).expect("preview");
        params["history"] = json!(path);
        params["action"] = json!("draft");
        let draft = session.execute("qareview", &params).expect("draft");
        assert_eq!(draft["draft"]["analysis"], preview["analysis"]);
        session.execute("qareview", &json!({"history":path,"action":"review","draft_id":draft["draft_id"],"expected_draft_hash":draft["draft_hash"],"decision":"APPROVED","reviewer":"Synthetic Reviewer","note":"Checked synthetic evidence"})).expect("review");
        params["action"] = json!("commit");
        params["draft_id"] = draft["draft_id"].clone();
        params["expected_draft_hash"] = draft["draft_hash"].clone();
        params["note"] = json!("Approve local record");
        assert_eq!(session.execute("qareview", &params).expect("commit")["state"], "COMMITTED");
        let history = session.execute("qareview", &json!({"history":path,"action":"history"})).expect("history");
        assert_eq!(history.as_array().expect("array").len(), 3);
        assert!(!history.to_string().contains("Private Person"));
        std::fs::remove_file(path).expect("remove synthetic history");
    }
}
