//! Names of audio crates in a lockfile.

/// Crate names in `lockfile_text` that would pull audio into the binary.
#[must_use]
pub fn audio_crates(lockfile_text: &str) -> Vec<String> {
    let Ok(value) = toml::from_str::<toml::Value>(lockfile_text) else {
        return vec!["invalid lockfile".into()];
    };
    let mut names: Vec<_> = value
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|package| package.get("name").and_then(toml::Value::as_str))
        .filter(|name| {
            *name == "cpal"
                || *name == "rodio"
                || name.starts_with("portaudio")
                || name.starts_with("alsa")
                || name.starts_with("coreaudio")
        })
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    names
}
