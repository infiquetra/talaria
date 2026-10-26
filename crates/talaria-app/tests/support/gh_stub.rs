//! Stub `gh` programs for review tests. Issue #245.
//!
//! A reply stub answers with recorded replies only: the test names reply files
//! in `fixtures/gh/`, and the stub matches the start of its argument list. A
//! test that asserts only an argument list or a timing uses an argument-logging
//! stub, which logs and exits 0 with empty output.

#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// One recorded reply: the file name under `fixtures/gh/` it was read from.
pub struct Reply {
    /// The start of gh's argument list this reply answers.
    pub args_prefix: Vec<String>,
    /// The exit status to leave with.
    pub exit: i32,
    /// The standard output to print.
    pub stdout: String,
    /// The standard error to print.
    pub stderr: String,
}

impl Reply {
    /// Read a reply file in the recorded format.
    pub fn read(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).expect("reply file");
        let value: serde_json::Value = serde_json::from_str(&text).expect("reply json");
        let args_prefix = value["args_prefix"]
            .as_array()
            .expect("args_prefix")
            .iter()
            .map(|word| word.as_str().expect("prefix word").to_owned())
            .collect();
        Self {
            args_prefix,
            exit: value["exit"].as_i64().expect("exit") as i32,
            stdout: value["stdout"].as_str().expect("stdout").to_owned(),
            stderr: value["stderr"].as_str().expect("stderr").to_owned(),
        }
    }
}

/// A stub program and its argument log.
pub struct GhStub {
    /// The stub program to pass to `GhRunner::with_binary`.
    pub program: PathBuf,
    /// The log file. One line per call, arguments joined by `\x1f`.
    pub log: PathBuf,
}

/// Read the log: one argument list per call.
pub fn read_log(log: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    text.lines()
        .map(|line| line.split('\x1f').map(str::to_owned).collect())
        .collect()
}

/// Write a stub that answers with `replies`, first prefix match wins. With no
/// match it exits 127 and prints `stub: no recorded reply for <args>`.
pub fn write_reply_stub(dir: &Path, replies: &[Reply]) -> GhStub {
    let log = dir.join("gh-args.log");
    let mut script = format!(
        "#!/bin/sh\nlog={}\nrecord_args() {{\n  first=1\n  for arg in \"$@\"; do\n    if [ \"$first\" = 1 ]; then\n      first=0\n    else\n      printf '\\037' >> \"$log\"\n    fi\n    printf '%s' \"$arg\" >> \"$log\"\n  done\n  printf '\\n' >> \"$log\"\n}}\nrecord_args \"$@\"\n",
        shell_quote(&log.to_string_lossy())
    );
    for (index, reply) in replies.iter().enumerate() {
        let out = dir.join(format!("reply-{index}.out"));
        let err = dir.join(format!("reply-{index}.err"));
        std::fs::write(&out, &reply.stdout).expect("reply stdout");
        std::fs::write(&err, &reply.stderr).expect("reply stderr");
        let prefix = shell_quote(&reply.args_prefix.join(" "));
        script.push_str(&format!(
            "case \"$*\" in\n{prefix}*)\n  cat {}\n  cat {} >&2\n  exit {}\n  ;;\nesac\n",
            shell_quote(&out.to_string_lossy()),
            shell_quote(&err.to_string_lossy()),
            reply.exit
        ));
    }
    script.push_str("echo \"stub: no recorded reply for $*\" >&2\nexit 127\n");
    GhStub {
        program: write_script(dir, "gh-stub", &script),
        log,
    }
}

/// Write a stub that answers with `replies` and logs `start` before and `end`
/// after each call, sleeping `sleep_secs` between them. Overlap means two
/// processes ran at once.
pub fn write_counting_stub(dir: &Path, replies: &[Reply], sleep_secs: u64) -> GhStub {
    let log = dir.join("gh-args.log");
    let mut script = format!(
        "#!/bin/sh\nlog={}\necho \"start $*\" >> \"$log\"\nsleep {sleep_secs}\necho \"end $*\" >> \"$log\"\n",
        shell_quote(&log.to_string_lossy())
    );
    for (index, reply) in replies.iter().enumerate() {
        let out = dir.join(format!("reply-{index}.out"));
        let err = dir.join(format!("reply-{index}.err"));
        std::fs::write(&out, &reply.stdout).expect("reply stdout");
        std::fs::write(&err, &reply.stderr).expect("reply stderr");
        let prefix = shell_quote(&reply.args_prefix.join(" "));
        script.push_str(&format!(
            "case \"$*\" in\n{prefix}*)\n  cat {}\n  cat {} >&2\n  exit {}\n  ;;\nesac\n",
            shell_quote(&out.to_string_lossy()),
            shell_quote(&err.to_string_lossy()),
            reply.exit
        ));
    }
    script.push_str("echo \"stub: no recorded reply for $*\" >&2\nexit 127\n");
    GhStub {
        program: write_script(dir, "gh-counting-stub", &script),
        log,
    }
}

/// Write a stub that logs its arguments, optionally sleeps `sleep_secs`, and
/// exits 0 with empty output. For argument and timing tests only.
pub fn write_arg_stub(dir: &Path, sleep_secs: u64) -> GhStub {
    let log = dir.join("gh-args.log");
    let mut script = format!(
        "#!/bin/sh\nlog={}\nfirst=1\nfor arg in \"$@\"; do\n  if [ \"$first\" = 1 ]; then\n    first=0\n  else\n    printf '\\037' >> \"$log\"\n  fi\n  printf '%s' \"$arg\" >> \"$log\"\ndone\nprintf '\\n' >> \"$log\"\n",
        shell_quote(&log.to_string_lossy())
    );
    if sleep_secs > 0 {
        script.push_str(&format!("sleep {sleep_secs}\n"));
    }
    GhStub {
        program: write_script(dir, "gh-arg-stub", &script),
        log,
    }
}

fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("stub script");
    let mut permissions = std::fs::metadata(&path)
        .expect("stub metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("stub mode");
    path
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
