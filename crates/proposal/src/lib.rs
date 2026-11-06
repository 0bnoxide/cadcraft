//! New Proposal planning: the standards manifest, the project plan built from it, path
//! safety, plan approval and run receipts. Pure functions; the engine command writes files.
//!
//! Work in this crate is split between agents by module; see `docs/agents/slice-7-proposal.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod approval;
pub mod issue;
pub mod manifest;
pub mod paths;
pub mod planner;

pub use issue::{Inputs, Issue};
