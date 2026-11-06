//! The proposal inputs and the issue type every module reports. Shared; change only by
//! agreement (see the slice 7 brief).

use serde::{Deserialize, Serialize};

/// What the user types into New Proposal. Optional fields are `None` when left blank.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inputs {
    pub client_name: String,
    pub site_name: String,
    pub proposal_year: i32,
    pub orientation: String,
    pub sheet_size: String,
    pub client_number: Option<String>,
    pub project_number: Option<String>,
    pub proposal_number: Option<String>,
    pub site_address: Option<String>,
    pub project_manager: Option<String>,
}

/// A problem that stops or qualifies a plan or a run. `code` is stable (agreed by message);
/// `location` names the input field, manifest path or plan action when there is one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub code: String,
    pub message: String,
    pub location: Option<String>,
}

impl Issue {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Issue { code: code.to_string(), message: message.into(), location: None }
    }

    pub fn at(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
}
