//! Drone (UAS) survey QA: parse a PIX4D quality report, check it against an accuracy ruleset, and
//! take the result through a draft, review and commit workflow with an audit history.
//!
//! Work in this crate is split between agents by module; see `docs/agents/slice-3-droneqa.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod checkpoint;
pub mod mission;
pub mod model;
pub mod pix4d;
pub mod qa;
pub mod redact;
pub mod workflow;

pub use model::{Analysis, Finding, Outcome, ParseResult, ParserDiagnostic, QAMetrics, ReportEvidence, ReportResiduals, Severity};
