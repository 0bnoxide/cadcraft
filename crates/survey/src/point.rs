//! Types shared by every survey module. Changing them needs both owners to agree (see
//! `docs/agents/slice-1-survey.md`).

/// One surveyed point in the drawing's own units and coordinate system.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurveyPoint {
    /// Point number or name as recorded in the field; unique within one import.
    pub id: String,
    pub northing: f64,
    pub easting: f64,
    pub elevation: f64,
    /// Field code that selects the point's layer, e.g. `MW` or `BM`.
    pub feature_code: String,
    pub description: String,
}

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// One QA message, in the order found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

impl Finding {
    pub fn warning(message: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, message: message.into() }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { severity: Severity::Error, message: message.into() }
    }
}
