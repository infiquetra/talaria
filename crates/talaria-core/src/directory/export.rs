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
        match self {
            ExportFormat::Markdown => "md",
            ExportFormat::Json => "json",
            ExportFormat::Html => "html",
        }
    }

    /// The file extension: `md`, `json` or `html`.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Markdown => "md",
            ExportFormat::Json => "json",
            ExportFormat::Html => "html",
        }
    }

    /// The display label: `Markdown`, `JSON` or `HTML`.
    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Markdown => "Markdown",
            ExportFormat::Json => "JSON",
            ExportFormat::Html => "HTML",
        }
    }

    /// The format for a file extension, case-insensitively.
    pub fn from_extension(ext: &str) -> Option<ExportFormat> {
        match ext.to_lowercase().as_str() {
            "md" => Some(ExportFormat::Markdown),
            "json" => Some(ExportFormat::Json),
            "html" => Some(ExportFormat::Html),
            _ => None,
        }
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
pub fn parse_export_args(arg: &str) -> Result<ExportArgs, ExportArgError> {
    let mut redact = true;
    let mut path: Option<String> = None;
    for word in arg.split_whitespace() {
        if word == "--no-redact" {
            redact = false;
            continue;
        }
        if word.starts_with("--") {
            return Err(ExportArgError::UnknownOption(word.to_owned()));
        }
        if path.is_some() {
            return Err(ExportArgError::TooManyPaths);
        }
        path = Some(word.to_owned());
    }
    if let Some(path) = path.as_deref() {
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str());
        let Some(extension) = extension else {
            return Err(ExportArgError::BadExtension);
        };
        if ExportFormat::from_extension(extension).is_none() {
            return Err(ExportArgError::BadExtension);
        }
    }
    Ok(ExportArgs { path, redact })
}

/// Reduce Hermes's file name to a safe base name, falling back when refused.
pub fn safe_file_name(hermes_filename: &str, format: ExportFormat, session_id: &str) -> String {
    let fallback = fallback_name(session_id, format);
    let last = hermes_filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(hermes_filename);
    // Only letters, digits, `-`, `_` and `.` survive, so the name can never leave the folder.
    if last.is_empty()
        || last.starts_with('.')
        || last
            .chars()
            .any(|character| !(character.is_alphanumeric() || matches!(character, '-' | '_' | '.')))
    {
        return fallback;
    }
    let extension = std::path::Path::new(last)
        .extension()
        .and_then(|extension| extension.to_str());
    let Some(extension) = extension else {
        return fallback;
    };
    if ExportFormat::from_extension(extension) != Some(format) {
        return fallback;
    }
    last.to_owned()
}

/// The name an export gets when Hermes's name cannot be used: `hermes_session_<id>.<extension>`.
/// The id keeps only ASCII letters, digits, `-` and `_`, and is `session` when nothing is left,
/// so a session id can never name another folder.
pub fn fallback_name(session_id: &str, format: ExportFormat) -> String {
    let id: String = session_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect();
    let id = if id.is_empty() {
        "session"
    } else {
        id.as_str()
    };
    format!("hermes_session_{id}.{}", format.extension())
}

/// The candidate names in order: the name, then `stem (2).ext` to `stem (99).ext`.
pub fn candidate_names(name: &str) -> impl Iterator<Item = String> {
    let path = std::path::Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    (1..=99).map(move |index| {
        if index == 1 {
            return name.to_owned();
        }
        if extension.is_empty() {
            format!("{stem} ({index})")
        } else {
            format!("{stem} ({index}).{extension}")
        }
    })
}

/// The size text: bytes, KiB or MiB with one decimal.
pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes < KIB {
        return format!("{bytes} B");
    }
    if bytes < MIB {
        return format!("{:.1} KiB", bytes as f64 / KIB as f64);
    }
    format!("{:.1} MiB", bytes as f64 / MIB as f64)
}

/// Exports redact secrets unless asked otherwise.
pub const REDACT_BY_DEFAULT: bool = true;
