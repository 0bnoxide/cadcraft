//! Host-free route geometry. Stations are start station plus distance along the route.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod geometry;
pub mod landxml;
mod projection;
pub use geometry::*;

pub const MAX_ELEMENTS: usize = 4096;
pub const MAX_POINTS: usize = 100_000;
pub const POSITION_TOLERANCE: f64 = 1e-6;
pub const DIRECTION_TOLERANCE: f64 = 1e-8;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{code}: {reason}")]
pub struct Error {
    pub code: &'static str,
    pub reason: String,
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn error(code: &'static str, reason: impl Into<String>) -> Error {
    Error { code, reason: reason.into() }
}
