//! Plane geometry shared by every labels module, in drawing units. Changing it needs both owners
//! to agree by Paseo message (see `docs/agents/slice-4-labels.md`).

use serde::{Deserialize, Serialize};

/// A point in the drawing plane.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle. A valid one has `min_x <= max_x` and `min_y <= max_y`, all finite.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Rect {
    /// The rectangle of `width` x `height` whose lower-left corner is `at`. `None` if any value is
    /// not finite or a size is negative.
    pub fn from_corner(at: Pt, width: f64, height: f64) -> Option<Self> {
        let r = Self { min_x: at.x, min_y: at.y, max_x: at.x + width, max_y: at.y + height };
        (width >= 0.0 && height >= 0.0 && r.is_valid()).then_some(r)
    }

    pub fn is_valid(&self) -> bool {
        [self.min_x, self.min_y, self.max_x, self.max_y].iter().all(|v| v.is_finite()) && self.min_x <= self.max_x && self.min_y <= self.max_y
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}
