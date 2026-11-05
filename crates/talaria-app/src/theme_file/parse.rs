use super::{ThemeFile, ThemeFileError};
pub fn parse(_: &str) -> Result<ThemeFile, ThemeFileError> {
    Err(ThemeFileError {
        key: String::new(),
        line: 1,
        column: 1,
        message: "not implemented".into(),
    })
}
