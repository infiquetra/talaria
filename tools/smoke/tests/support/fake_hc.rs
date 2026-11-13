//! A recorded fake `hc` executable for testing `HcCli`.
//!
//! Environment: `FAKE_HC_DIR` holds `replies.json`, the argument `log` (one JSON array of
//! arguments per line), and `env.log` (one JSON array per call naming which of
//! `HC_TERMINAL_ID` and `HC_SCOPE` were set). Recording files are loaded from
//! `tests/support/hc_recordings/`.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Map, Value, json};

fn append_line(path: &Path, value: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    writeln!(file, "{value}").map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn prior_calls_for(log_path: &Path, command: Option<&str>) -> usize {
    let Some(command) = command else {
        return 0;
    };
    let Ok(text) = fs::read_to_string(log_path) else {
        return 0;
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Vec<String>>(line).ok())
        .filter(|args| args.first().map(String::as_str) == Some(command))
        .count()
}

fn load_replies(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| format!("invalid {}: {error}", path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

fn next_reply(
    replies: &Value,
    command: Option<&str>,
    index: usize,
) -> Result<Option<(String, i64)>, String> {
    let Some(command) = command else {
        return Ok(None);
    };
    let Some(list) = replies.get(command) else {
        return Ok(None);
    };
    let list = list
        .as_array()
        .ok_or_else(|| format!("replies.{command} is not an array"))?;
    let Some(entry) = list.get(index) else {
        return Ok(None);
    };
    let file = entry
        .get("file")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("replies.{command}[{index}].file is not a string"))?
        .to_owned();
    let status = entry
        .get("status")
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("replies.{command}[{index}].status is not an integer"))?;
    Ok(Some((file, status)))
}

fn run() -> Result<ExitCode, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var_os("FAKE_HC_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "FAKE_HC_DIR is not set".to_owned())?;

    let command = args.first().map(String::as_str);
    let log_path = dir.join("log");
    let call_index = prior_calls_for(&log_path, command);
    append_line(&log_path, &json!(args))?;

    let mut seen_env = Vec::new();
    if std::env::var_os("HC_TERMINAL_ID").is_some() {
        seen_env.push("HC_TERMINAL_ID");
    }
    if std::env::var_os("HC_SCOPE").is_some() {
        seen_env.push("HC_SCOPE");
    }
    append_line(&dir.join("env.log"), &json!(seen_env))?;

    let replies = load_replies(&dir.join("replies.json"))?;
    let Some((file_name, status)) = next_reply(&replies, command, call_index)? else {
        return Ok(ExitCode::SUCCESS);
    };

    let recording_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support/hc_recordings")
        .join(&file_name);
    let bytes = fs::read(&recording_path).map_err(|error| {
        format!(
            "cannot read recording {}: {error}",
            recording_path.display()
        )
    })?;

    if status == 0 {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(&bytes)
            .and_then(|()| stdout.flush())
            .map_err(|error| format!("cannot write stdout: {error}"))?;
        Ok(ExitCode::SUCCESS)
    } else {
        let mut stderr = std::io::stderr().lock();
        stderr
            .write_all(&bytes)
            .and_then(|()| stderr.flush())
            .map_err(|error| format!("cannot write stderr: {error}"))?;
        let code = u8::try_from(status).ok().filter(|&c| c != 0).unwrap_or(1);
        Ok(ExitCode::from(code))
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("fake-hc: {message}");
            ExitCode::from(2)
        }
    }
}
