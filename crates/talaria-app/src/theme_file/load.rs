use super::ThemeFile;
use std::path::Path;
pub fn load(_: &Path, _: &str) -> Result<ThemeFile, String> {
    Err("not implemented".into())
}
pub fn valid_id(_: &str) -> bool {
    false
}
pub fn reserved_id(_: &str) -> bool {
    false
}
