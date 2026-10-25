//! Export arguments, file names and sizes (#231).

/// An export format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
    Html,
}

impl ExportFormat {
    /// The wire value: `md`, `json` or `html`.
    pub fn wire(self) -> &'static str {
        unimplemented!("#231")
    }

    /// The file extension: `md`, `json` or `html`.
    pub fn extension(self) -> &'static str {
        unimplemented!("#231")
    }

    /// The display label: `Markdown`, `JSON` or `HTML`.
    pub fn label(self) -> &'static str {
        unimplemented!("#231")
    }

    /// The format for a file extension, case-insensitively.
    pub fn from_extension(_ext: &str) -> Option<ExportFormat> {
        unimplemented!("#231")
    }
}

/// Parsed `/export` arguments.
#[derive(Debug, PartialEq, Eq)]
pub struct ExportArgs {
    pub path: Option<String>,
    pub redact: bool,
}

/// Why export arguments were refused.
#[derive(Debug, PartialEq, Eq)]
pub enum ExportArgError {
    UnknownOption(String),
    BadExtension,
    TooManyPaths,
}

/// Parse `/export [--no-redact] [file]`.
pub fn parse_export_args(_arg: &str) -> Result<ExportArgs, ExportArgError> {
    unimplemented!("#231")
}

/// Reduce Hermes's file name to a safe base name, falling back when refused.
pub fn safe_file_name(_hermes_filename: &str, _format: ExportFormat, _session_id: &str) -> String {
    unimplemented!("#231")
}

/// The candidate names in order: the name, then `stem (2).ext` to `stem (99).ext`.
pub fn candidate_names(_name: &str) -> impl Iterator<Item = String> {
    unimplemented!("#231");
    #[allow(unreachable_code)]
    std::iter::empty()
}

/// The size text: bytes, KiB or MiB with one decimal.
pub fn format_size(_bytes: u64) -> String {
    unimplemented!("#231")
}

/// Exports redact secrets unless asked otherwise.
pub const REDACT_BY_DEFAULT: bool = true;
