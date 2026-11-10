//! Plan approval and run receipts: plan hashing, stale-approval checks and the receipt record.
//! Owner: Codex (see `docs/agents/slice-7-proposal.md`). Behaviour reference: AutoGIS-Civil3D
//! `ProposalApproval.cs` and `RunReceipt.cs`.

use crate::{Issue, planner::Plan};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn reader_hash(reader: &mut impl std::io::Read) -> std::io::Result<String> {
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let n = reader.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        hash.update(bytes.get(..n).unwrap_or_default());
    }
    Ok(format!("{hash:x}", hash = hash.finalize()))
}
pub fn plan_hash(plan: &Plan) -> Result<String, Issue> {
    serde_json::to_vec(plan).map(|bytes| sha256(&bytes)).map_err(|e| Issue::new("invalid_input", e.to_string()))
}
pub fn check_approval(plan: &Plan, approved: Option<&str>) -> Result<(), Issue> {
    match approved {
        None => Err(Issue::new("approval_required", "Review and approve the current plan hash")),
        Some(s) if s.trim().is_empty() => Err(Issue::new("approval_required", "Review and approve the current plan hash")),
        Some(s) if s == plan_hash(plan)? => Ok(()),
        Some(_) => Err(Issue::new("stale_approval", "The approved plan has changed")),
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub enum Outcome {
    Cancelled,
    Refused,
    Failed,
    Succeeded,
}
#[derive(Clone, Debug, Serialize)]
pub struct Receipt {
    pub version: u32,
    pub run_id: String,
    pub started_at_utc: String,
    pub plan_sha256: String,
    pub outcome: Outcome,
    pub final_root: String,
    pub receipt_path: Option<String>,
    pub verified_artifacts: Vec<String>,
    pub manual_steps: Vec<String>,
    pub failed_action_id: Option<String>,
    pub failed_checks: Vec<Issue>,
    pub failure_detail: Option<String>,
    pub cleanup_failures: Vec<String>,
    pub reporting_failure: Option<String>,
    pub retained_staging_root: Option<String>,
}
