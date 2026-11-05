//! Spreadsheet I/O and subsurface drawings: boring-log import, stick logs, subsurface
//! profiles, boring-log reports and data extraction tables.
//!
//! Work in this crate is split between agents by module; see `docs/agents/slice-6-subsurface.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod boring;
pub mod import;
pub mod profile;
pub mod report;
pub mod sheet;
pub mod sticklog;
pub mod table;

pub use boring::{Boring, BoringSet, Finding, Severity};
pub use table::{Cell, Table};
