//! Scripted local agent used by integration tests.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

struct Mock {
    log: Option<PathBuf>,
    new_count: usize,
    auth_count: usize,
    last_id: Value,
    mode: String,
    vars: HashMap<String, Value>,
}

impl Mock {
    fn log(&self, value: Value) -> io::Result<()> {
        if let Some(path) = &self.log {
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            writeln!(file, "{value}")?;
        }
        Ok(())
    }

    fn send(&self, value: Value) -> io::Result<()> {
        self.log(json!({"sent": value}))?;
        let mut stdout = io::stdout().lock();
        talaria_app::acp::trace_fixture_event(true, "mock_write_begin");
        writeln!(stdout, "{value}")?;
        let result = stdout.flush();
        if result.is_ok() {
            talaria_app::acp::trace_fixture_event(true, "mock_write_flushed");
        }
        result
    }

    fn receive(&mut self, input: &mut impl BufRead) -> io::Result<Option<Value>> {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let value: Value = serde_json::from_str(&line).map_err(io::Error::other)?;
        self.log(json!({"seen": value}))?;
        if value.get("method").and_then(Value::as_str) == Some("session/new") {
            self.new_count += 1;
        }
        if value.get("method").and_then(Value::as_str) == Some("authenticate") {
            self.auth_count += 1;
        }
        if let Some(outcome) = value.pointer("/result/outcome") {
            self.log(json!({"outcome": outcome}))?;
        }
        if value.get("id").is_some() && value.get("method").is_some() {
            self.last_id = value["id"].clone();
        }
        Ok(Some(value))
    }

    fn counts(&self) -> io::Result<()> {
        self.log(
            json!({"counts": {"session/new": self.new_count, "authenticate": self.auth_count}}),
        )
    }

    fn substitute(&self, mut value: Value) -> Value {
        match &mut value {
            Value::String(s) if s.starts_with('$') => {
                if let Some(found) = self.vars.get(&s[1..]) {
                    value = found.clone();
                }
            }
            Value::Array(items) => {
                for item in items {
                    *item = self.substitute(item.take());
                }
            }
            Value::Object(items) => {
                for item in items.values_mut() {
                    *item = self.substitute(item.take());
                }
            }
            _ => {}
        }
        value
    }

    fn scripted(&mut self, script: &str, input: &mut impl BufRead) -> io::Result<()> {
        for line in script.lines().filter(|line| !line.trim().is_empty()) {
            let step: Value = serde_json::from_str(line).map_err(io::Error::other)?;
            match step["kind"].as_str().unwrap_or("") {
                "expect" => {
                    let Some(message) = self.receive(input)? else {
                        break;
                    };
                    if let Some(method) = step.get("method")
                        && &message["method"] != method
                    {
                        return Err(io::Error::other("unexpected protocol method"));
                    }
                    if let Some(id) = step.get("response_id")
                        && (message.get("id") != Some(id) || message.get("method").is_some())
                    {
                        return Err(io::Error::other("unexpected protocol response"));
                    }
                    if let Some(saved) = step.get("save").and_then(Value::as_object) {
                        for (name, pointer) in saved {
                            let pointer = pointer
                                .as_str()
                                .ok_or_else(|| io::Error::other("invalid capture"))?;
                            let value = message
                                .pointer(pointer)
                                .ok_or_else(|| io::Error::other("missing capture"))?;
                            self.vars.insert(name.clone(), value.clone());
                        }
                    }
                }
                "reply" => {
                    let mut result = step["result"].clone();
                    if self.mode == "wrong_version" && result.get("protocolVersion").is_some() {
                        result["protocolVersion"] = json!(99);
                    }
                    self.send(json!({"jsonrpc":"2.0", "id":self.last_id, "result":result}))?;
                }
                "reply_error" => {
                    self.send(json!({"jsonrpc":"2.0", "id":self.last_id, "error":step["error"]}))?
                }
                "emit" | "ask_permission" => self.send(self.substitute(step["message"].clone()))?,
                "stderr" => {
                    eprintln!("mock-acp-agent: diagnostic line, not protocol");
                    if let Some(line) = step.get("protocol") {
                        eprintln!("{line}");
                    }
                }
                "sleep_ms" => std::thread::sleep(std::time::Duration::from_millis(
                    step["ms"].as_u64().unwrap_or(0),
                )),
                "mode" => self.mode = step["value"].as_str().unwrap_or("").to_owned(),
                "exit" => {
                    if let Some(code) = step["code"].as_i64() {
                        self.counts()?;
                        std::process::exit(i32::try_from(code).unwrap_or(1));
                    }
                    break;
                }
                _ => return Err(io::Error::other("unknown mock script step")),
            }
        }
        Ok(())
    }

    fn default_script(&mut self, input: &mut impl BufRead) -> io::Result<()> {
        while let Some(message) = self.receive(input)? {
            if message["method"] == "initialize" {
                let version = if self.mode == "wrong_version" { 99 } else { 1 };
                self.send(json!({"jsonrpc":"2.0", "id":message["id"], "result":{"protocolVersion":version, "agentInfo":{"name":"Mock Agent"}}}))?;
            }
        }
        Ok(())
    }
}

fn main() -> io::Result<()> {
    let _trace_guard = talaria_app::acp::trace_fixture_guard(true);
    let mut mock = Mock {
        log: std::env::var_os("TALARIA_MOCK_ACP_LOG").map(PathBuf::from),
        new_count: 0,
        auth_count: 0,
        last_id: Value::Null,
        mode: std::env::var("TALARIA_MOCK_ACP_MODE").unwrap_or_default(),
        vars: HashMap::new(),
    };
    if let Some(path) = std::env::var_os("TALARIA_MOCK_ACP_PID_FILE") {
        fs::write(path, std::process::id().to_string())?;
    }
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let result = if let Some(path) = std::env::var_os("TALARIA_MOCK_ACP_SCRIPT") {
        mock.scripted(&fs::read_to_string(path)?, &mut input)
    } else {
        mock.default_script(&mut input)
    };
    mock.counts()?;
    result
}
