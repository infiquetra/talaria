//! `talaria profiles`: list profiles and why one is not served. Issue #253.

use std::process::ExitCode;

use talaria_core::profiles::ProfileRow;

/// Parsed `talaria profiles` arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilesArgs {
    /// Print JSON instead of the table.
    pub json: bool,
    /// The typed names, normalised, duplicates dropped.
    pub names: Vec<String>,
}

/// Parse the arguments after the program name, including `profiles` itself.
pub fn parse(_args: &[String]) -> Result<ProfilesArgs, clap::Error> {
    todo!("profiles stub")
}

/// Run the arguments after the program name, including `profiles` itself.
pub fn run(_args: &[String]) -> ExitCode {
    todo!("profiles stub")
}

/// The text table: a header row, then one row per profile. Issue #253.
#[must_use]
pub fn render_text(_rows: &[ProfileRow]) -> String {
    todo!("profiles stub")
}

/// The `--json` document with its warnings: one line, no trailing newline, keys in
/// the order the specification's example shows. Issue #253.
#[must_use]
pub fn render_json(_rows: &[ProfileRow], _warnings: &[String]) -> String {
    todo!("profiles stub")
}
