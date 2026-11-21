//! A recorded fake `hc` executable for testing `HcCli`.
//!
//! Environment: `FAKE_HC_DIR` holds `replies.json`, the argument `log` (one JSON array of
//! arguments per line), and `env.log` (one JSON array per call naming which of
//! `HC_TERMINAL_ID` and `HC_SCOPE` were set). Recording files are loaded from
//! `tests/support/hc_recordings/`.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
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

    if command == Some("connect") {
        return connect(&dir).map(|()| ExitCode::SUCCESS);
    }
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

fn read_frame(input: &mut impl Read) -> Result<(u32, u8, Vec<u8>), String> {
    let mut header = [0; 9];
    input.read_exact(&mut header).map_err(|e| e.to_string())?;
    let channel = u32::from_le_bytes(header[..4].try_into().unwrap());
    let len = u32::from_le_bytes(header[5..].try_into().unwrap()) as usize;
    if len > 1_048_576 {
        return Err("unexpected frame size".to_owned());
    }
    let mut payload = vec![0; len];
    input.read_exact(&mut payload).map_err(|e| e.to_string())?;
    Ok((channel, header[4], payload))
}

fn write_frame(
    output: &mut impl Write,
    channel: u32,
    kind: u8,
    payload: &[u8],
) -> Result<(), String> {
    output
        .write_all(&channel.to_le_bytes())
        .map_err(|e| e.to_string())?;
    output.write_all(&[kind]).map_err(|e| e.to_string())?;
    output
        .write_all(&u32::try_from(payload.len()).unwrap().to_le_bytes())
        .map_err(|e| e.to_string())?;
    output.write_all(payload).map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}

fn connect(dir: &Path) -> Result<(), String> {
    let config = load_replies(&dir.join("connect.json"))?;
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut output = std::io::stdout().lock();
    let mut line = String::new();
    input.read_line(&mut line).map_err(|e| e.to_string())?;
    let hello: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if hello
        != json!({"jsonrpc":"2.0","id":1,"method":"hello","params":{"framed":true,"features":["framed","attach","automatic_input"]}})
    {
        return Err("unexpected hello".to_owned());
    }
    let hello_reply = if config["hello_error"] == true {
        json!({"id":1,"error":{"message":"hello refused"}})
    } else {
        json!({"id":1,"result":{"framed":config["framed"].as_bool().unwrap_or(true)}})
    };
    writeln!(output, "{hello_reply}").map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    if config["hello_error"] == true || config["framed"] == false {
        return Ok(());
    }
    let (channel, kind, payload) = read_frame(&mut input)?;
    let attach: Value = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
    if channel != 0
        || kind != 0
        || attach
            != json!({"jsonrpc":"2.0","id":2,"method":"terminal.attach","params":{"terminal":"term_x","cols":0,"rows":0,"history":0}})
    {
        return Err("unexpected terminal.attach or nonzero resize".to_owned());
    }
    if config["attach_error"] == true {
        write_frame(
            &mut output,
            0,
            0,
            br#"{"id":2,"error":{"message":"attach refused"}}"#,
        )?;
        return Ok(());
    }
    let channel: u32 = 0x0102_0304;
    let mut reply = json!({"id":2,"result":{"channel":channel}});
    if let Some(automatic) = config["automatic_input"].as_bool() {
        reply["result"]["automatic_input"] = json!(automatic);
    }
    write_frame(
        &mut output,
        channel,
        1,
        b"terminal output before attach reply",
    )?;
    write_frame(&mut output, 0, 0, br#"{"id":99,"result":{}}"#)?;
    write_frame(&mut output, 0, 0, reply.to_string().as_bytes())?;
    let (input_channel, kind, payload) = read_frame(&mut input)?;
    let expected_kind = if config["automatic_input"] == true {
        6
    } else {
        0
    };
    if input_channel != channel || kind != expected_kind {
        return Err("unexpected input channel or kind".to_owned());
    }
    fs::write(dir.join("input.bin"), payload).map_err(|e| e.to_string())?;
    let mut remaining = Vec::new();
    input
        .read_to_end(&mut remaining)
        .map_err(|e| e.to_string())?;
    if !remaining.is_empty() {
        return Err("unexpected bytes after raw input".to_owned());
    }
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(dir.join("connect-exited"), []).map_err(|e| e.to_string())?;
    if config["exit_failure"] == true {
        return Err("connect failed after input".to_owned());
    }
    Ok(())
}
