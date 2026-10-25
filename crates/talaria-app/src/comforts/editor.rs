//! External editor: resolution, hand-off, and result handling.
//!
//! Behavior contract: issue 228.

use std::io::{Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Why an editor command could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    /// Shell-style split failed; holds the reason.
    Parse(String),
}

/// What an editor run produced. `Failed` holds the notice text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditResult {
    /// Replace the draft with this cleaned text, unsent.
    Loaded(String),
    /// The edited text was empty; the draft was cleared.
    Cleared,
    /// The draft is unchanged; `hint` asks for the wait-flag notice.
    Unchanged {
        /// Whether to show the wait-flag notice.
        hint: bool,
    },
    /// The draft is unchanged; holds the notice text.
    Failed(String),
}

/// Pick the editor argument list: config, environment, well-known names.
pub fn resolve_editor(
    config: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
    is_executable_on_path: &dyn Fn(&str) -> bool,
) -> Result<Vec<String>, EditorError> {
    for command in [config.map(str::to_owned), env("VISUAL"), env("EDITOR")]
        .into_iter()
        .flatten()
    {
        if !command.trim().is_empty() {
            let args = shell_words::split(&command)
                .map_err(|error| EditorError::Parse(error.to_string()))?;
            if !args.is_empty() {
                return Ok(args);
            }
        }
    }
    Ok(vec![
        ["editor", "nano", "pico", "vi", "emacs"]
            .into_iter()
            .find(|name| is_executable_on_path(name))
            .unwrap_or("vi")
            .to_owned(),
    ])
}

/// Run the editor on the draft and classify the result. Blocking.
pub fn edit_draft(argv: &[String], draft: &str, cwd: &Path, tmp_dir: &Path) -> EditResult {
    let mut file = match tempfile::NamedTempFile::new_in(tmp_dir) {
        Ok(file) => file,
        Err(error) => {
            return EditResult::Failed(format!(
                "Could not create a temporary file for the editor ({}); the draft is unchanged.",
                io_reason(&error)
            ));
        }
    };
    if let Err(error) = file.write_all(draft.as_bytes()).and_then(|()| file.flush()) {
        return EditResult::Failed(format!(
            "Could not create a temporary file for the editor ({}); the draft is unchanged.",
            io_reason(&error)
        ));
    }
    if let Err(error) = super::suspend::catch_interrupts_while_released() {
        return EditResult::Failed(format!(
            "Could not start the editor \"{}\": {}. Set $VISUAL or $EDITOR, or [editor] command in ~/.talaria/config.toml.",
            argv.first().map_or("", String::as_str),
            io_reason(&error)
        ));
    }
    let started = Instant::now();
    let command = argv.first().map_or("", String::as_str);
    let status = Command::new(command)
        .args(argv.iter().skip(1))
        .arg(file.path())
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            return EditResult::Failed(format!(
                "Could not start the editor \"{command}\": {}. Set $VISUAL or $EDITOR, or [editor] command in ~/.talaria/config.toml.",
                io_reason(&error)
            ));
        }
    };
    if !status.success() {
        return EditResult::Failed(if let Some(signal) = status.signal() {
            format!("The editor was stopped by signal {signal}; the draft is unchanged.")
        } else {
            format!(
                "The editor exited with status {}; the draft is unchanged.",
                status.code().unwrap_or(1)
            )
        });
    }
    let bytes = std::fs::File::open(file.path()).and_then(|reader| {
        let mut bytes = Vec::new();
        reader.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(error) => return read_failure(&io_reason(&error)),
    };
    if bytes.len() > 1024 * 1024 {
        return read_failure("larger than 1 MiB");
    }
    let raw = match String::from_utf8(bytes) {
        Ok(raw) => raw,
        Err(_) => return read_failure("not valid UTF-8"),
    };
    if raw == draft {
        return EditResult::Unchanged {
            hint: started.elapsed() < Duration::from_secs(1),
        };
    }
    let cleaned: String = raw
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    let cleaned = cleaned.trim_end().to_owned();
    if cleaned.is_empty() {
        if draft.is_empty() {
            EditResult::Unchanged { hint: false }
        } else {
            EditResult::Cleared
        }
    } else {
        EditResult::Loaded(cleaned)
    }
}

/// The notice for an unreadable editor command. The caller shows it on `Parse`.
pub fn parse_notice(reason: &str) -> String {
    format!("Could not read the editor command: {reason}.")
}

/// The notice for an editor that returned at once without changes.
pub fn unchanged_hint_notice() -> String {
    "The editor returned at once without changes. A graphical editor needs its wait flag, for example \"code --wait\".".to_owned()
}

/// The notice for an edit that cleared the draft.
pub fn cleared_notice() -> String {
    "The edited text was empty; the draft was cleared.".to_owned()
}

fn read_failure(reason: &str) -> EditResult {
    EditResult::Failed(format!(
        "The edited text could not be read ({reason}); the draft is unchanged."
    ))
}

pub fn io_reason(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "not found".to_owned(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        kind => kind.to_string(),
    }
}
