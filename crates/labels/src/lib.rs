//! Labels and callouts: content formatting, batch label templates, callout tables, and
//! collision-aware placement.
//!
//! Work in this crate is split between agents by module; see `docs/agents/slice-4-labels.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod callout;
pub mod format;
pub mod geom;
pub mod placement;
pub mod template;

pub use geom::{Pt, Rect};
