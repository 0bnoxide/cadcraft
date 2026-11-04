//! Types shared by the parser, the QA engine and the workflow (ported from AIGIS3D `models.py`).
//! Changing them needs both owners to agree by Paseo message (see `docs/agents/slice-3-droneqa.md`).

use serde::{Deserialize, Serialize};

/// Result of one QA rule, or of the whole analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Pass,
    Conditional,
    Fail,
    InsufficientData,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

/// Headline metrics read from a processing report. `None` means the report did not say.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QAMetrics {
    pub checkpoints_independent: Option<bool>,
    pub checkpoints_bare_ground: Option<bool>,
    pub rmse_x_feet: Option<f64>,
    pub rmse_y_feet: Option<f64>,
    pub rmse_z_feet: Option<f64>,
    pub calibrated_images: Option<u64>,
    pub total_images: Option<u64>,
    pub processing_software: Option<String>,
    pub processing_version: Option<String>,
}

/// Original-unit residual statistics. These are never independent checkpoint accuracy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportResiduals {
    /// `fitted_gcps` or `initial_computed_camera_positions`.
    pub provenance: String,
    /// Always `US survey foot` in the qualified layouts.
    pub unit: String,
    pub mean_xyz: [f64; 3],
    pub sigma_xyz: [f64; 3],
    pub rms_xyz: [f64; 3],
}

/// Further evidence the PIX4Dmapper layout carries.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReportEvidence {
    pub gsd_cm: Option<f64>,
    pub gcp_count: Option<u64>,
    pub gcp_fit: Option<ReportResiduals>,
    pub camera_offsets: Option<ReportResiduals>,
    pub camera_translation_xyz: Option<[f64; 3]>,
    pub camera_translation_unit: Option<String>,
    pub image_crs: Option<String>,
    pub gcp_crs: Option<String>,
    pub output_crs: Option<String>,
    pub classification_time: Option<String>,
    pub dtm_resolution_label: Option<String>,
    pub vertical_datum: Option<String>,
    pub geoid_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserDiagnostic {
    /// Stable code such as `MALFORMED_REPORT`.
    pub code: String,
    pub message: String,
}

/// What a report parser returns. Diagnostics never echo report content.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParseResult {
    pub profile: String,
    pub metrics: QAMetrics,
    pub report_evidence: Option<ReportEvidence>,
    pub diagnostics: Vec<ParserDiagnostic>,
}

/// One rule's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub outcome: Outcome,
    pub severity: Severity,
    pub message: String,
    /// The observed value, already redacted where an input was sensitive.
    pub observed: Option<serde_json::Value>,
    pub requirement: Option<String>,
    pub unit: Option<String>,
    pub source: Option<String>,
    pub next_action: Option<String>,
}

/// The whole QA result for one mission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub findings: Vec<Finding>,
    pub outcome: Outcome,
    pub risk_level: Severity,
    pub ruleset_version: String,
}
