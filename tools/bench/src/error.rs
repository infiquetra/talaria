//! Errors that end a run with exit status 2.

#[derive(Debug)]
pub enum BenchError {
    BinaryNotFound(String),
    FixturesMissing(String),
    TemplateMissing { file: String, frame_type: String },
    BudgetsInvalid(String),
    PtyUnavailable(String),
    UnsupportedOs,
    Io(String),
}

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BinaryNotFound(path) => {
                let shown = if path.contains('\\') || path.starts_with('~') {
                    path.rsplit(['/', '\\']).next().unwrap_or("<binary>")
                } else if std::path::Path::new(path).is_absolute() {
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("<binary>")
                } else {
                    path
                };
                let shown: String = shown
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(100)
                    .collect();
                write!(
                    f,
                    "binary not found: {shown}; build it with cargo build --release -p talaria-cli"
                )
            }
            Self::FixturesMissing(_) => f.write_str(
                "fixtures missing or invalid; supply recorded frame logs with --fixtures",
            ),
            Self::TemplateMissing { file, frame_type } => {
                write!(f, "template frame {frame_type} not found in {file}")
            }
            Self::BudgetsInvalid(_) => f.write_str("budgets invalid; supply a valid budgets file"),
            Self::PtyUnavailable(_) => {
                f.write_str("pseudo-terminal unavailable; run on a host with terminal support")
            }
            Self::UnsupportedOs => f.write_str("benchmark requires Linux or macOS"),
            Self::Io(_) => {
                f.write_str("benchmark I/O failed; check input files and output permissions")
            }
        }
    }
}

impl std::error::Error for BenchError {}
