//! Survey data: points, RTK import and QA, control checks, level loops and point export.
//!
//! Behaviour is ported from the AutoGIS reference modules named in each module's docs. Work in
//! this crate is split between agents by module; see `docs/agents/slice-1-survey.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod control;
pub mod export;
pub mod level_loop;
pub mod point;
pub mod rtk;

pub use point::{Finding, Severity, SurveyPoint};
