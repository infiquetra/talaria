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
        let _ = f;
        todo!("bench")
    }
}

impl std::error::Error for BenchError {}
