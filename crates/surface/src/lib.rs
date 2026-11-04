//! TIN surfaces: build from points and breaklines, contour, compare, and export as LandXML.
//!
//! Work in this crate is split between agents by module; see `docs/agents/slice-2-tin.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod compare;
pub mod contour;
pub mod landxml;
pub mod tin;
pub mod triangulate;

pub use tin::Tin;
