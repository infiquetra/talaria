//! Private tmux fixture: workspace_demo --lines N --interval-ms M --workspace starts in the
//! workspace. `i` returns to inline without exiting; `q` restores and exits. No gateway is used.
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Tmux(String);
impl Tmux {
    fn run(&self, args: &[&str]) -> String {
        let out = Command::new("tmux")
            .env_remove("TMUX")
            .args(["-L", &self.0])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("tmux installed");
        assert!(
            out.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn capture(&self) -> String {
        self.run(&["capture-pane", "-p", "-S", "-", "-t", &self.0])
    }
    fn wait(&self, mut condition: impl FnMut() -> bool) {
        let start = Instant::now();
        while !condition() {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "timed out: {}",
                self.capture()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Drop for Tmux {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .env_remove("TMUX")
            .args(["-L", &self.0, "kill-session", "-t", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
#[ignore = "needs tmux and builds the builder-owned workspace_demo example"]
fn workspace_survives_resizes_and_returns_every_line_once() {
    let target = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR").expect("shared CARGO_TARGET_DIR required"),
    );
    assert!(
        Command::new("cargo")
            .args(["build", "-p", "talaria-tui", "--example", "workspace_demo"])
            .status()
            .unwrap()
            .success()
    );
    let examples = target.join("debug/examples");
    let tmux = Tmux(format!("talaria-workspace-{}", std::process::id()));
    tmux.run(&[
        "new-session",
        "-d",
        "-s",
        &tmux.0,
        "-x",
        "120",
        "-y",
        "40",
        "-c",
        examples.to_str().unwrap(),
        "bash --noprofile --norc",
    ]);
    tmux.run(&[
        "send-keys",
        "-t",
        &tmux.0,
        "./workspace_demo --lines 200 --interval-ms 20 --workspace",
        "Enter",
    ]);
    tmux.wait(|| tmux.capture().contains("line 0100"));
    tmux.run(&["resize-window", "-t", &tmux.0, "-x", "80", "-y", "24"]);
    thread::sleep(Duration::from_millis(150));
    tmux.run(&["resize-window", "-t", &tmux.0, "-x", "120", "-y", "40"]);
    tmux.wait(|| tmux.capture().contains("line 0200"));
    tmux.run(&["send-keys", "-t", &tmux.0, "i"]);
    tmux.wait(|| {
        tmux.run(&["display-message", "-p", "-t", &tmux.0, "#{alternate_on}"])
            .trim()
            == "0"
    });
    tmux.run(&["send-keys", "-t", &tmux.0, "q"]);
    tmux.wait(|| {
        tmux.run(&[
            "display-message",
            "-p",
            "-t",
            &tmux.0,
            "#{pane_current_command}",
        ])
        .trim()
            == "bash"
    });
    let captured = tmux.capture();
    let mut previous = None;
    for n in 1..=200 {
        let tag = format!("line {n:04}");
        let positions = captured
            .lines()
            .enumerate()
            .filter(|(_, line)| line.trim_end() == tag)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(positions.len(), 1, "{tag}: {captured}");
        assert!(
            previous.is_none_or(|before| before < positions[0]),
            "{tag} order"
        );
        previous = Some(positions[0]);
    }
}
