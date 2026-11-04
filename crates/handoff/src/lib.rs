//! Validator for the AutoGIS LandXML handoff package, contract version 1.0.
//!
//! A package is one ZIP holding exactly `handoff.json` (the manifest) and `surface.landxml` (one
//! LandXML 1.2 TIN surface). Validation runs in stages and stops at the first stage that reports
//! an error: archive safety, manifest, surface checksum, LandXML surface, then manifest/LandXML
//! cross-checks. The issue codes, their order and the CLI exit codes match the reference .NET
//! validator in `0bnoxide/AutoGIS-Civil3D` so both accept and reject the same packages.
//!
//! Contract-valid is not the same as "imports correctly into a civil design application".
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::fmt;
use std::io::{self, Read, Seek};
use std::path::Path;

mod archive;
pub mod codes;
mod landxml;
mod manifest;

pub use landxml::{TinPoint, TinSurface};

/// How serious an issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Warning => "Warning",
            Severity::Error => "Error",
        })
    }
}

/// One validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Stable code such as `ZIP001` (see [`codes`]).
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    /// Manifest field the issue refers to, when there is one.
    pub location: Option<String>,
}

impl Issue {
    pub(crate) fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, severity: Severity::Error, message: message.into(), location: None }
    }

    pub(crate) fn error_at(code: &'static str, message: impl Into<String>, location: &str) -> Self {
        Self { code, severity: Severity::Error, message: message.into(), location: Some(location.to_owned()) }
    }
}

/// Overall outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Valid,
    ValidWithWarnings,
    Invalid,
}

impl Status {
    /// The reference CLI's exit code: 0 valid, 1 invalid, 2 valid with warnings.
    pub fn exit_code(self) -> i32 {
        match self {
            Status::Valid => 0,
            Status::Invalid => 1,
            Status::ValidWithWarnings => 2,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Status::Valid => "VALID",
            Status::ValidWithWarnings => "VALID WITH WARNINGS",
            Status::Invalid => "INVALID",
        })
    }
}

/// Facts confirmed by a package that passed every stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedMetadata {
    /// Lower-case hyphenated UUID.
    pub package_id: String,
    pub surface_name: String,
    pub point_count: u64,
    pub face_count: u64,
    pub epsg_code: i32,
}

/// The result of validating one package.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub status: Status,
    /// In the order found; the first is the primary issue.
    pub issues: Vec<Issue>,
    pub metadata: Option<VerifiedMetadata>,
    /// The surface geometry, when requested and the package is valid.
    pub surface: Option<TinSurface>,
}

impl Report {
    fn new(issues: Vec<Issue>, metadata: Option<VerifiedMetadata>, surface: Option<TinSurface>) -> Self {
        let status = if issues.iter().any(|i| i.severity == Severity::Error) {
            Status::Invalid
        } else if issues.is_empty() {
            Status::Valid
        } else {
            Status::ValidWithWarnings
        };
        Self { status, issues, metadata, surface }
    }

    /// The plain-text report the reference CLI prints.
    pub fn to_text(&self) -> String {
        let mut out = format!("Status: {}\nIssues:\n", self.status);
        if self.issues.is_empty() {
            out.push_str("- None\n");
        }
        for issue in &self.issues {
            out.push_str(&format!("- [{}] {}: {}\n", issue.code, issue.severity, issue.message));
        }
        out.push_str("Verified metadata:\n");
        match &self.metadata {
            None => out.push_str("- None\n"),
            Some(m) => out.push_str(&format!(
                "- Package ID: {}\n- Surface name: {}\n- Point count: {}\n- Face count: {}\n- EPSG code: {}\n",
                m.package_id, m.surface_name, m.point_count, m.face_count, m.epsg_code
            )),
        }
        out.push_str("Contract-valid is not equivalent to Civil 3D import-tested.\n");
        out
    }
}

/// An error that stops validation without a verdict (the file could not be read at all).
/// The reference CLI reports these as operational failures with exit code 3.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct OperationalError(#[from] pub io::Error);

/// Validates the package at `path`.
pub fn validate_file(path: &Path) -> Result<Report, OperationalError> {
    validate(open_regular(path)?, false)
}

/// Validates a package and, when it is valid, also returns its TIN surface.
pub fn read_surface_file(path: &Path) -> Result<Report, OperationalError> {
    validate(open_regular(path)?, true)
}

/// Opens `path` for reading only if it is a regular file. On unix the open is nonblocking, so a
/// FIFO without a writer cannot hang it; the type check then refuses FIFOs and devices.
fn open_regular(path: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} is not a regular file", path.display())));
    }
    Ok(file)
}

/// Validates a package read from `source`. With `keep_surface`, a valid package's TIN
/// geometry is returned in [`Report::surface`].
pub fn validate<R: Read + Seek>(source: R, keep_surface: bool) -> Result<Report, OperationalError> {
    let mut issues = Vec::new();
    let mut bundle = match archive::Bundle::open(source)? {
        Ok(bundle) => bundle,
        Err(issue) => return Ok(Report::new(vec![issue], None, None)),
    };

    let manifest_bytes = match stage(bundle.read_manifest())? {
        Ok(bytes) => bytes,
        Err(issue) => return Ok(Report::new(vec![issue], None, None)),
    };
    let manifest = match manifest::parse(&manifest_bytes) {
        Ok((manifest, warnings)) => {
            issues.extend(warnings);
            manifest
        }
        Err(manifest_issues) => return Ok(Report::new(manifest_issues, None, None)),
    };

    let checksum = match stage(bundle.surface_sha256())? {
        Ok(sum) => sum,
        Err(issue) => {
            issues.push(issue);
            return Ok(Report::new(issues, None, None));
        }
    };
    if checksum != manifest.surface_sha256 {
        issues.push(Issue::error_at(codes::CHECKSUM_MISMATCH, "surface.landxml does not match the manifest SHA-256 digest.", "surface.sha256"));
        return Ok(Report::new(issues, None, None));
    }

    let parsed = match bundle.open_surface().map_err(StageError::from).and_then(|reader| landxml::parse(reader, keep_surface)) {
        Ok(parsed) => parsed,
        Err(StageError::Entry(issue)) => {
            issues.push(issue);
            return Ok(Report::new(issues, None, None));
        }
        Err(StageError::Io(e)) => return Err(e.into()),
    };
    let summary = match parsed {
        Ok(summary) => summary,
        Err(issue) => {
            issues.push(issue);
            return Ok(Report::new(issues, None, None));
        }
    };

    let before = issues.len();
    cross_check(&manifest, &summary, &mut issues);
    if issues.len() > before {
        return Ok(Report::new(issues, None, None));
    }

    let metadata = VerifiedMetadata {
        package_id: manifest.package_id.clone(),
        surface_name: summary.surface_name.clone(),
        point_count: summary.point_count,
        face_count: summary.face_count,
        epsg_code: summary.epsg_code,
    };
    Ok(Report::new(issues, Some(metadata), summary.geometry))
}

fn cross_check(manifest: &manifest::Manifest, summary: &landxml::Summary, issues: &mut Vec<Issue>) {
    if manifest.surface_name != summary.surface_name {
        issues.push(Issue::error_at(codes::SURFACE_NAME_MISMATCH, "The manifest surface name does not match LandXML.", "surface.name"));
    }
    if manifest.point_count != summary.point_count {
        issues.push(Issue::error_at(codes::POINT_COUNT_MISMATCH, "The manifest point count does not match LandXML.", "surface.point_count"));
    }
    if manifest.face_count != summary.face_count {
        issues.push(Issue::error_at(codes::FACE_COUNT_MISMATCH, "The manifest face count does not match LandXML.", "surface.face_count"));
    }
    if manifest.epsg_code != summary.epsg_code {
        issues.push(Issue::error_at(codes::EPSG_MISMATCH, "The manifest EPSG code does not match LandXML.", "coordinate_reference.horizontal.code"));
    }
    if manifest.horizontal_unit != summary.horizontal_unit {
        issues.push(Issue::error_at(
            codes::HORIZONTAL_UNIT_MISMATCH,
            "The manifest horizontal unit does not match LandXML.",
            "coordinate_reference.horizontal.unit",
        ));
    }
    let vertical_matches = match summary.vertical_family {
        landxml::VerticalFamily::Metre => manifest.vertical_unit == LinearUnit::Metre,
        landxml::VerticalFamily::Foot => matches!(manifest.vertical_unit, LinearUnit::InternationalFoot | LinearUnit::UsSurveyFoot),
    };
    if !vertical_matches {
        issues.push(Issue::error_at(
            codes::VERTICAL_UNIT_FAMILY_MISMATCH,
            "The manifest vertical unit family does not match LandXML.",
            "coordinate_reference.vertical.unit",
        ));
    }
}

/// Linear units the contract allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearUnit {
    Metre,
    InternationalFoot,
    UsSurveyFoot,
}

/// Why a stage stopped: a reportable entry fault, or an I/O failure with no verdict.
#[derive(Debug)]
pub(crate) enum StageError {
    Entry(Issue),
    Io(io::Error),
}

impl From<io::Error> for StageError {
    fn from(e: io::Error) -> Self {
        match archive::fault_of(&e) {
            Some(issue) => StageError::Entry(issue),
            None => StageError::Io(e),
        }
    }
}

/// Turns a stage's I/O result into an issue (entry faults) or an operational error.
fn stage<T>(result: io::Result<T>) -> Result<Result<T, Issue>, OperationalError> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(e) => match StageError::from(e) {
            StageError::Entry(issue) => Ok(Err(issue)),
            StageError::Io(e) => Err(e.into()),
        },
    }
}
