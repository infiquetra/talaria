//! Names of audio crates in a lockfile. The stub reports none.

/// Crate names in `lockfile_text` that would pull audio into the binary.
/// The stub returns an empty list.
#[must_use]
pub fn audio_crates(lockfile_text: &str) -> Vec<String> {
    let _ = lockfile_text;
    Vec::new()
}
