//! The boring-log data model every module reads. Shared; change only by agreement (see the
//! slice 6 brief). Depths are below ground surface in the input's own units; nothing converts
//! feet to metres.

use serde::{Deserialize, Serialize};

/// A boring or well location.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Boring {
    pub boring_id: String,
    pub site_id: String,
    pub location_type: String,
    pub northing: Option<f64>,
    pub easting: Option<f64>,
    pub ground_elevation: Option<f64>,
    pub toc_elevation: Option<f64>,
    pub total_depth: Option<f64>,
    pub status: String,
    pub completion_type: String,
    pub coordinate_system: String,
    pub vertical_datum: String,
}

/// One logged soil or rock interval.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lithology {
    pub boring_id: String,
    pub top_depth: f64,
    pub bottom_depth: f64,
    /// USCS group symbol as logged, for example "SM" or "CL-ML".
    pub uscs: String,
    pub primary_material: String,
    pub color: String,
    pub moisture: String,
    pub consistency: String,
    pub pid_ppm: Option<f64>,
    pub description: String,
}

/// One sample interval.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub sample_id: String,
    pub boring_id: String,
    pub sample_type: String,
    pub top_depth: f64,
    pub bottom_depth: f64,
    pub recovery: Option<f64>,
    pub blow_counts: String,
    pub lab_submitted: bool,
}

/// One well-construction component (casing, screen, filter pack, seal, grout).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WellComponent {
    pub boring_id: String,
    pub component: String,
    pub top_depth: f64,
    pub bottom_depth: f64,
    pub diameter: Option<f64>,
    pub material: String,
    pub slot_size: String,
}

/// One groundwater observation. `observed` is ISO 8601 text, or empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WaterLevel {
    pub boring_id: String,
    pub observed: String,
    pub depth_to_water: Option<f64>,
    pub kind: String,
}

/// Everything imported from one boring-log package, in input order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoringSet {
    pub borings: Vec<Boring>,
    pub lithology: Vec<Lithology>,
    pub samples: Vec<Sample>,
    pub wells: Vec<WellComponent>,
    pub water: Vec<WaterLevel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A message about the input, in the order found. `code` is stable (agreed by message).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub boring_id: Option<String>,
    /// Source sheet or file name and 1-based row, when the finding has one.
    pub source: Option<(String, usize)>,
}
