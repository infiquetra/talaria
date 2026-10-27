//! The raw soul editor. Terminal hand-off belongs to the application loop.

use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};

pub use talaria_core::profiles::RawEdit;

/// The largest soul text the editor opens.
pub const MAX_SOUL_BYTES: usize = 262_144;

/// Run the editor on exact text in a private directory, always removing its file.
pub fn edit_text_raw(
    argv: &[String],
    text: &str,
    cwd: &Path,
    tmp_dir: &Path,
    max_bytes: usize,
) -> RawEdit {
    if text.len() > max_bytes {
        return RawEdit::Failed(format!(
            "This soul file is too large to edit here ({} KiB).",
            text.len() / 1024
        ));
    }
    let private = std::fs::symlink_metadata(tmp_dir)
        .is_ok_and(|metadata| metadata.is_dir() && metadata.permissions().mode() & 0o777 == 0o700);
    if !private || cwd != tmp_dir {
        return RawEdit::Failed(
            "Could not create a private file for the soul editor; nothing was sent.".into(),
        );
    }
    let path = tmp_dir.join("SOUL.md");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path);
    let Ok(mut file) = file else {
        return RawEdit::Failed(
            "Could not create a private file for the soul editor; nothing was sent.".into(),
        );
    };
    // Named cleanup is installed only after creation, so an existing file is never removed.
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    if file
        .write_all(text.as_bytes())
        .and_then(|()| file.flush())
        .is_err()
    {
        return RawEdit::Failed("Could not write the soul editor file; nothing was sent.".into());
    }
    drop(file);
    let status = argv
        .first()
        .filter(|command| !command.is_empty())
        .and_then(|command| {
            Command::new(command)
                .args(&argv[1..])
                .arg(&path)
                .current_dir(cwd)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .ok()
        });
    if !status.is_some_and(|status| status.success()) {
        return RawEdit::Failed("The editor exited with an error; nothing was sent.".into());
    }
    let bytes = std::fs::File::open(&path).and_then(|reader| {
        let mut bytes = Vec::new();
        reader
            .take(
                u64::try_from(max_bytes)
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    let Ok(bytes) = bytes else {
        return RawEdit::Failed("The edited soul could not be read; nothing was sent.".into());
    };
    if bytes.len() > max_bytes {
        return RawEdit::Failed("The edited soul is larger than 256 KiB; nothing was sent.".into());
    }
    match String::from_utf8(bytes) {
        Err(_) => RawEdit::Failed("The edited soul is not UTF-8 text; nothing was sent.".into()),
        Ok(edited) if edited == text => RawEdit::Unchanged,
        Ok(edited) => RawEdit::Changed(edited),
    }
}
