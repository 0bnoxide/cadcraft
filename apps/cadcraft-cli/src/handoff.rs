//! `cadcraft-cli handoff`: validate an AutoGIS LandXML handoff package (contract v1).

use crate::args::HandoffArgs;
use crate::output::Streams;

/// Validates the package and prints the report. Returns the exit code of the reference
/// validator: 0 valid, 1 invalid, 2 valid with warnings, 3 the package could not be read.
pub(crate) fn handoff(a: &HandoffArgs, io: &mut Streams) -> i32 {
    match cadcraft_handoff::validate_file(&a.package) {
        Ok(report) => {
            io.outln(format_args!("{}", report.to_text().trim_end()));
            report.status.exit_code()
        }
        Err(e) => {
            io.errln(format_args!("Operational failure: {e}"));
            3
        }
    }
}
