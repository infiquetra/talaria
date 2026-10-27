//! The raw editor for soul text: exact bytes in and out. Issue #253.
//!
//! The composer editor cleans text, which would silently rewrite a soul. This
//! module only handles the file and the process; the terminal hand-off around
//! it belongs to the application loop.

use std::path::Path;

pub use talaria_core::profiles::RawEdit;

/// The largest soul text the editor opens.
pub const MAX_SOUL_BYTES: usize = 262_144;

/// Run `argv` plus a file path on `text`, byte for byte. Blocking: files and
/// the process only, never the terminal. Issue #253.
pub fn edit_text_raw(
    _argv: &[String],
    _text: &str,
    _cwd: &Path,
    _tmp_dir: &Path,
    _max_bytes: usize,
) -> RawEdit {
    todo!("profiles stub")
}
