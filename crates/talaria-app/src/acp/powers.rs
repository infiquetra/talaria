//! Optional, per-agent filesystem and process capabilities.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use talaria_core::fleet::acp::AcpCell;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use super::AcpAgentEntry;

const OUTPUT_LIMIT: usize = 64 * 1024;
const TERMINAL_LIMIT: Duration = Duration::from_secs(120);
const ENVIRONMENT: [&str; 6] = ["PATH", "HOME", "TERM", "LANG", "LC_ALL", "TMPDIR"];

pub(crate) struct PowerEvent {
    pub id: Value,
    pub session_id: String,
    pub result: Result<Value, PowerError>,
    pub cell: Option<AcpCell>,
}

pub(crate) struct PowerError {
    pub code: i64,
    pub message: String,
}

impl PowerError {
    fn refused(message: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: message.into(),
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }
    fn failed(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
        }
    }
}

#[derive(Default)]
pub(crate) struct PowerManager {
    next_terminal: u64,
    terminals: HashMap<(String, String), mpsc::UnboundedSender<TerminalCommand>>,
}

impl PowerManager {
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &mut self,
        agent: &AcpAgentEntry,
        session_id: &str,
        cwd: &Path,
        id: Value,
        method: &str,
        params: Value,
        sender: &mpsc::UnboundedSender<PowerEvent>,
    ) -> Result<(), PowerError> {
        match method {
            "fs/read_text_file" | "fs/write_text_file" if agent.allow_fs => {
                let root = cwd.to_owned();
                let session_id = session_id.to_owned();
                let method = method.to_owned();
                let sender = sender.clone();
                tokio::spawn(async move {
                    let result =
                        tokio::task::spawn_blocking(move || file_call(&root, &method, &params))
                            .await;
                    let (result, cell) = match result {
                        Ok(Ok((value, cell))) => (Ok(value), Some(cell)),
                        Ok(Err(error)) => (Err(error), None),
                        Err(_) => (
                            Err(PowerError::failed("talaria: file operation failed")),
                            None,
                        ),
                    };
                    let _ = sender.send(PowerEvent {
                        id,
                        session_id,
                        result,
                        cell,
                    });
                });
                Ok(())
            }
            method if method.starts_with("fs/") && !agent.allow_fs => Err(PowerError::refused(
                format!("talaria: {method} is not enabled for this agent"),
            )),
            "terminal/create" if agent.allow_terminal => {
                self.create_terminal(session_id, cwd, id, params, sender)
            }
            "terminal/output" | "terminal/wait_for_exit" | "terminal/kill" | "terminal/release"
                if agent.allow_terminal =>
            {
                let terminal = params
                    .get("terminalId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PowerError::invalid("talaria: missing terminal id"))?;
                let key = (session_id.to_owned(), terminal.to_owned());
                let handle = self
                    .terminals
                    .get(&key)
                    .ok_or_else(|| PowerError::invalid("talaria: unknown terminal"))?
                    .clone();
                let operation = match method {
                    "terminal/output" => TerminalOperation::Output,
                    "terminal/wait_for_exit" => TerminalOperation::Wait,
                    "terminal/kill" => TerminalOperation::Kill,
                    _ => TerminalOperation::Release,
                };
                if matches!(operation, TerminalOperation::Release) {
                    self.terminals.remove(&key);
                }
                handle
                    .send(TerminalCommand { id, operation })
                    .map_err(|_| PowerError::failed("talaria: terminal has ended"))
            }
            method if method.starts_with("terminal/") => Err(PowerError::refused(format!(
                "talaria: {method} is not enabled for this agent"
            ))),
            _ => Err(PowerError::refused(format!(
                "talaria: {method} is not supported"
            ))),
        }
    }

    fn create_terminal(
        &mut self,
        session_id: &str,
        cwd: &Path,
        id: Value,
        params: Value,
        sender: &mpsc::UnboundedSender<PowerEvent>,
    ) -> Result<(), PowerError> {
        let command = params
            .get("command")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| PowerError::invalid("talaria: missing terminal command"))?;
        let args = match params.get("args") {
            None | Some(Value::Null) => Vec::new(),
            Some(value) => value
                .as_array()
                .ok_or_else(|| PowerError::invalid("talaria: invalid terminal arguments"))?
                .iter()
                .map(|value| value.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| PowerError::invalid("talaria: invalid terminal arguments"))?,
        };
        let directory = match params.get("cwd").and_then(Value::as_str) {
            Some(path) => confined(cwd, Path::new(path), false)?,
            None => cwd.to_owned(),
        };
        if !directory.is_dir() {
            return Err(PowerError::invalid(
                "talaria: terminal directory does not exist",
            ));
        }
        let mut process = Command::new(command);
        process
            .args(args)
            .current_dir(directory)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for name in ENVIRONMENT {
            if let Some(value) = std::env::var_os(name) {
                process.env(name, value);
            }
        }
        let mut child = process.spawn().map_err(|error| {
            PowerError::failed(format!(
                "talaria: terminal could not start ({})",
                error.kind()
            ))
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PowerError::failed("talaria: terminal output unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| PowerError::failed("talaria: terminal output unavailable"))?;
        self.next_terminal += 1;
        let terminal_id = format!("terminal-{}", self.next_terminal);
        let (commands, receiver) = mpsc::unbounded_channel();
        let (bytes, output) = mpsc::channel(16);
        tokio::spawn(read_terminal(stdout, bytes.clone()));
        tokio::spawn(read_terminal(stderr, bytes));
        tokio::spawn(terminal_task(
            child,
            session_id.to_owned(),
            output,
            receiver,
            sender.clone(),
        ));
        self.terminals
            .insert((session_id.to_owned(), terminal_id.clone()), commands);
        let _ = sender.send(PowerEvent {
            id,
            session_id: session_id.into(),
            result: Ok(json!({"terminalId":terminal_id})),
            cell: None,
        });
        Ok(())
    }

    pub fn close_session(&mut self, session_id: &str) {
        self.terminals.retain(|(id, _), _| id != session_id);
    }
}

fn confined(root: &Path, path: &Path, allow_create: bool) -> Result<PathBuf, PowerError> {
    if !path.is_absolute() {
        return Err(PowerError::refused(
            "talaria: path is outside the session working directory",
        ));
    }
    let resolved = match path.canonicalize() {
        Ok(path) => path,
        Err(error) if allow_create && error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                PowerError::refused("talaria: path is outside the session working directory")
            })?;
            let parent = parent.canonicalize().map_err(|_| {
                PowerError::refused("talaria: path is outside the session working directory")
            })?;
            let name = path
                .file_name()
                .ok_or_else(|| PowerError::invalid("talaria: invalid file name"))?;
            parent.join(name)
        }
        Err(_) => return Err(PowerError::failed("talaria: path cannot be read")),
    };
    if !resolved.starts_with(root) {
        return Err(PowerError::refused(
            "talaria: path is outside the session working directory",
        ));
    }
    Ok(resolved)
}

fn file_call(root: &Path, method: &str, params: &Value) -> Result<(Value, AcpCell), PowerError> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| PowerError::invalid("talaria: missing file path"))?;
    let write = method == "fs/write_text_file";
    let path = confined(root, Path::new(path), write)?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| PowerError::refused("talaria: path is outside the session working directory"))?
        .to_string_lossy();
    let body = talaria_core::text::sanitize(&relative).into_owned();
    let value = if write {
        let content = params
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| PowerError::invalid("talaria: missing file content"))?;
        let mut file = open_confined(root, &path, true)?;
        file.write_all(content.as_bytes())
            .map_err(|_| PowerError::failed("talaria: file could not be written"))?;
        json!({})
    } else {
        let mut file = open_confined(root, &path, false)?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|_| PowerError::failed("talaria: file could not be read"))?;
        let content = if params.get("line").is_some() || params.get("limit").is_some() {
            let start = params
                .get("line")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .saturating_sub(1) as usize;
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX)
                .min(usize::MAX as u64) as usize;
            content
                .lines()
                .skip(start)
                .take(limit)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            content
        };
        json!({"content":content})
    };
    let title = if write { "Write file" } else { "Read file" };
    Ok((
        value,
        AcpCell::Tool {
            id: format!("fs:{}", body),
            title: title.into(),
            body,
            status: Some("completed".into()),
        },
    ))
}

fn open_confined(root: &Path, path: &Path, write: bool) -> Result<std::fs::File, PowerError> {
    use rustix::fs::{Mode, OFlags, open, openat};
    let relative = path.strip_prefix(root).map_err(|_| {
        PowerError::refused("talaria: path is outside the session working directory")
    })?;
    let name = relative
        .file_name()
        .ok_or_else(|| PowerError::invalid("talaria: invalid file name"))?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = open(root, directory_flags, Mode::empty()).map_err(|_| {
        PowerError::refused("talaria: path is outside the session working directory")
    })?;
    for component in parent.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(PowerError::refused(
                "talaria: path is outside the session working directory",
            ));
        };
        directory = openat(&directory, name, directory_flags, Mode::empty()).map_err(|_| {
            PowerError::refused("talaria: path is outside the session working directory")
        })?;
    }
    let flags = if write {
        OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC
    } else {
        OFlags::RDONLY
    };
    let fd = openat(
        &directory,
        name,
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_bits_truncate(0o666),
    )
    .map_err(|_| PowerError::failed("talaria: file could not be opened"))?;
    Ok(std::fs::File::from(fd))
}

#[derive(Clone, Copy)]
enum TerminalOperation {
    Output,
    Wait,
    Kill,
    Release,
}
struct TerminalCommand {
    id: Value,
    operation: TerminalOperation,
}

async fn read_terminal(mut reader: impl AsyncRead + Unpin, sender: mpsc::Sender<Vec<u8>>) {
    let mut bytes = [0u8; 4096];
    while let Ok(count) = reader.read(&mut bytes).await {
        if count == 0 || sender.send(bytes[..count].to_vec()).await.is_err() {
            break;
        }
    }
}

async fn terminal_task(
    mut child: Child,
    session_id: String,
    mut output: mpsc::Receiver<Vec<u8>>,
    mut commands: mpsc::UnboundedReceiver<TerminalCommand>,
    sender: mpsc::UnboundedSender<PowerEvent>,
) {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut output_open = true;
    let mut exit: Option<Value> = None;
    let mut waiters: Vec<Value> = Vec::new();
    let deadline = tokio::time::sleep(TERMINAL_LIMIT);
    tokio::pin!(deadline);
    loop {
        enum Event {
            Bytes(Option<Vec<u8>>),
            Command(Option<TerminalCommand>),
            Exit(std::io::Result<std::process::ExitStatus>),
            Timeout,
        }
        let event = tokio::select! {
            data = output.recv(), if exit.is_none() && output_open => Event::Bytes(data),
            command = commands.recv() => Event::Command(command),
            status = child.wait(), if exit.is_none() => Event::Exit(status),
            _ = &mut deadline, if exit.is_none() => Event::Timeout,
        };
        match event {
            Event::Bytes(Some(bytes)) => {
                if append_output(&mut captured, &mut truncated, &bytes) {
                    note_truncation(&sender, &session_id);
                }
            }
            Event::Bytes(None) => output_open = false,
            Event::Exit(status) => {
                let _ = tokio::time::timeout(Duration::from_secs(2), async {
                    while let Some(bytes) = output.recv().await {
                        if append_output(&mut captured, &mut truncated, &bytes) {
                            note_truncation(&sender, &session_id);
                        }
                    }
                })
                .await;
                exit = Some(match status {
                    Ok(status) => json!({"exitCode":status.code(),"signal":null}),
                    Err(_) => json!({"exitCode":null,"signal":null}),
                });
                for id in waiters.drain(..) {
                    let _ = sender.send(PowerEvent {
                        id,
                        session_id: session_id.clone(),
                        result: Ok(exit.clone().unwrap_or_else(|| json!({}))),
                        cell: None,
                    });
                }
            }
            Event::Timeout => {
                let _ = child.kill().await;
            }
            Event::Command(Some(command)) => {
                let result = match command.operation {
                    TerminalOperation::Output => Some(
                        json!({"output":String::from_utf8_lossy(&captured),"truncated":truncated,"exitStatus":exit}),
                    ),
                    TerminalOperation::Wait => {
                        if exit.is_none() {
                            waiters.push(command.id);
                            continue;
                        }
                        exit.clone()
                    }
                    TerminalOperation::Kill => {
                        let _ = child.kill().await;
                        Some(json!({}))
                    }
                    TerminalOperation::Release => {
                        if exit.is_none() {
                            let _ = child.kill().await;
                        }
                        Some(json!({}))
                    }
                };
                let _ = sender.send(PowerEvent {
                    id: command.id,
                    session_id: session_id.clone(),
                    result: Ok(result.unwrap_or_else(|| json!({}))),
                    cell: None,
                });
                if matches!(command.operation, TerminalOperation::Release) {
                    break;
                }
            }
            Event::Command(None) => {
                if exit.is_none() {
                    let _ = child.kill().await;
                }
                break;
            }
        }
    }
}

fn append_output(captured: &mut Vec<u8>, truncated: &mut bool, bytes: &[u8]) -> bool {
    let before = *truncated;
    let remaining = OUTPUT_LIMIT.saturating_sub(captured.len());
    captured.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
    *truncated |= bytes.len() > remaining;
    !before && *truncated
}

fn note_truncation(sender: &mpsc::UnboundedSender<PowerEvent>, session_id: &str) {
    let _ = sender.send(PowerEvent {
        id: Value::Null,
        session_id: session_id.into(),
        result: Ok(Value::Null),
        cell: Some(AcpCell::Notice("terminal output truncated".into())),
    });
}
