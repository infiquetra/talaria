//! The workspace lockfile must not pull an audio crate into the binary.

#[path = "support/lockfile_guard.rs"]
mod lockfile_guard;

use std::fs;
use std::path::PathBuf;

use lockfile_guard::audio_crates;

#[test]
fn detector_accepts_clean_lockfile() {
    let text = "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    assert!(
        audio_crates(text).is_empty(),
        "a lockfile without audio crates"
    );
}

#[test]
fn detector_flags_planted_crate() {
    let text = "[[package]]\nname = \"cpal\"\nversion = \"0.15.0\"\n";
    let names = audio_crates(text);
    assert!(
        names.iter().any(|name| name == "cpal"),
        "planted cpal was not flagged: {names:?}"
    );
}

#[test]
fn lockfile_has_no_audio_crates() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    let text = fs::read_to_string(&path).expect("workspace lockfile");
    let names = audio_crates(&text);
    assert!(
        names.is_empty(),
        "audio crates in the workspace lockfile: {names:?}"
    );
}
