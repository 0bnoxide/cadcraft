//! TIN surfaces: build from points and breaklines, contour, compare, export as LandXML, and
//! analyse (volumes, interpolation, audits).
//!
//! Work in this crate is split between agents by module; see `docs/agents/slice-2-tin.md` and
//! `docs/agents/slice-5-analysis.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod audit;
pub mod compare;
pub mod contour;
pub mod interpolate;
pub mod landxml;
pub mod region;
pub mod tin;
pub mod triangulate;
pub mod volume;

pub use region::Region;
pub use tin::Tin;
