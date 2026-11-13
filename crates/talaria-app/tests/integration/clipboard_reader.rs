//! Clipboard image tools. Issue 224.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use talaria_app::attachments::{AttachmentEvent, AttachmentsState};
use talaria_app::clipboard::{
    ClipboardImageReader, ClipboardResult, CommandError, CommandOutput, CommandRunner, Platform,
};
use talaria_core::attachments::CollapseRule;
use talaria_core::session::SessionId;

#[derive(Clone)]
struct Script {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    calls: Vec<Call>,
    answers: VecDeque<Result<CommandOutput, CommandError>>,
}

struct Call {
    program: String,
    args: Vec<String>,
    timeout: Duration,
}

impl Script {
    fn new(answers: Vec<Result<CommandOutput, CommandError>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                calls: Vec::new(),
                answers: VecDeque::from(answers),
            })),
        }
    }

    fn calls(&self) -> Vec<(String, Vec<String>, Duration)> {
        self.inner
            .lock()
            .expect("script")
            .calls
            .iter()
            .map(|call| (call.program.clone(), call.args.clone(), call.timeout))
            .collect()
    }
}

impl CommandRunner for Script {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdout_to: Option<&Path>,
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError> {
        let mut inner = self.inner.lock().expect("script");
        inner.calls.push(Call {
            program: program.to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            timeout,
        });
        let answer = inner
            .answers
            .pop_front()
            .unwrap_or(Err(CommandError::NotFound));
        if let (Some(path), Ok(output)) = (stdout_to, &answer)
            && output.success
        {
            std::fs::write(path, &output.stdout).expect("clip file");
        }
        if program == "osascript"
            && let Some(script) = args.last()
            && script.contains("set imgData to the clipboard")
            && let Some(tail) = script.split("open for access POSIX file \"").nth(1)
            && let Some(destination) = tail.split('"').next()
            && let Ok(output) = &answer
            && output.success
            && output.stdout.starts_with(b"\x89PNG\r\n\x1a\n")
        {
            std::fs::write(destination, &output.stdout).expect("AppleScript image file");
        }
        answer
    }
}

fn png() -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/red-1x1.png"))
        .expect("red image")
}

fn ok(stdout: &[u8]) -> Result<CommandOutput, CommandError> {
    Ok(CommandOutput {
        success: true,
        stdout: stdout.to_vec(),
    })
}

fn timeouts_are_three(calls: &[(String, Vec<String>, Duration)]) {
    assert!(
        calls
            .iter()
            .all(|(_, _, timeout)| *timeout == Duration::from_secs(3))
    );
}

/// Tool order, 3 s limits, fall-through, a quiet probe, and the missing-tool sentence.
#[test]
fn tool_order_and_messages() {
    let image = png();
    let macos = Script::new(vec![
        Err(CommandError::NotFound),
        ok("«class PNGf»".as_bytes()),
        ok(&image),
    ]);
    let reader = ClipboardImageReader::new(Arc::new(macos.clone()), Platform::MacOs, false);
    let result = reader.read();
    let calls = macos.calls();
    timeouts_are_three(&calls);
    assert_eq!(calls[0].0, "pngpaste");
    assert_eq!(calls[1].0, "osascript");
    assert!(calls[1].1.iter().any(|arg| arg.contains("clipboard info")));
    assert_eq!(calls[2].0, "osascript");
    assert!(calls[2].1.iter().any(|arg| arg.contains("PNGf")));
    assert_eq!(result, ClipboardResult::Image(image.clone()));

    let failed = Script::new(vec![Err(CommandError::NotFound), ok(b"text"), ok(b"fail")]);
    let reader = ClipboardImageReader::new(Arc::new(failed), Platform::MacOs, false);
    assert_eq!(reader.read(), ClipboardResult::Empty);

    let wayland = Script::new(vec![ok(b"text/plain\n"), ok(b"image/png\n"), ok(&image)]);
    let reader = ClipboardImageReader::new(Arc::new(wayland.clone()), Platform::Linux, true);
    assert_eq!(reader.read(), ClipboardResult::Image(image));
    let calls = wayland.calls();
    timeouts_are_three(&calls);
    assert_eq!(calls[0].0, "wl-paste");
    assert!(calls[0].1.contains(&"--list-types".to_string()));
    assert_eq!(calls[1].0, "xclip");
    assert_eq!(calls[2].0, "xclip");

    let missing = Script::new(vec![
        Err(CommandError::NotFound),
        Err(CommandError::NotFound),
    ]);
    let reader = ClipboardImageReader::new(Arc::new(missing.clone()), Platform::Linux, true);
    assert_eq!(reader.read(), ClipboardResult::NoTool);
    assert_eq!(missing.calls().len(), 2);

    let idle = Script::new(Vec::new());
    let reader = ClipboardImageReader::new(Arc::new(idle.clone()), Platform::Other, false);
    assert_eq!(reader.read(), ClipboardResult::NoTool);
    assert!(idle.calls().is_empty());

    let mut state = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    let session = SessionId::new("local-session-1");
    let effects = state.on_event(
        AttachmentEvent::ClipboardEmpty {
            quiet: true,
            notice: Some(
                "No image on the clipboard. To paste text, use your terminal's paste \
                 (Cmd+V or Ctrl+Shift+V)."
                    .to_string(),
            ),
            fallback_text: Some("  \n".to_string()),
        },
        &session,
        "",
        0,
    );
    assert!(effects.iter().all(|effect| !matches!(
        effect,
        talaria_app::attachments::AttachmentEffect::Notice(_)
    )));
    let effects = state.on_event(
        AttachmentEvent::ClipboardEmpty {
            quiet: false,
            notice: Some(
                "Reading clipboard images needs wl-paste (Wayland) or xclip (X11).".to_string(),
            ),
            fallback_text: None,
        },
        &session,
        "",
        0,
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        talaria_app::attachments::AttachmentEffect::Notice(text)
            if text == "Reading clipboard images needs wl-paste (Wayland) or xclip (X11)."
    )));
}

fn program_exists(name: &str) -> bool {
    std::process::Command::new("/usr/bin/which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn skip(reason: &str) {
    println!("SKIPPED: {reason}");
}

/// The operator's clipboard, overwritten with the known red image, then read back.
#[test]
#[ignore = "overwrites the clipboard"]
fn real_tools_read_a_known_image() {
    if cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
    {
        skip("no display");
        return;
    }
    let png = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/red-1x1.png");
    let bytes = std::fs::read(&png).expect("red image");
    let wrote = if cfg!(target_os = "macos") {
        if !program_exists("osascript") {
            skip("no clipboard tool");
            return;
        }
        let script = format!(
            "set the clipboard to (read (POSIX file \"{}\") as «class PNGf»)",
            png.display()
        );
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .status()
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if !program_exists("wl-copy") {
            skip("no clipboard tool");
            return;
        }
        std::process::Command::new("wl-copy")
            .args(["--type", "image/png"])
            .stdin(std::fs::File::open(&png).expect("png"))
            .status()
    } else if !program_exists("xclip") {
        skip("no clipboard tool");
        return;
    } else {
        std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "image/png", "-i"])
            .arg(&png)
            .status()
    };
    if wrote.is_err() || wrote.is_ok_and(|status| !status.success()) {
        skip("clipboard tool failed");
        return;
    }
    let reader = ClipboardImageReader::system();
    assert_eq!(reader.read(), ClipboardResult::Image(bytes));
}
