//! Process-level remote commands.

mod support;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Gateway {
    child: Child,
    base: String,
}
impl Gateway {
    fn start() -> Self {
        static BUILD_MOCK: std::sync::Once = std::sync::Once::new();
        let binary = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples")
            .join("mock_gated_gateway");
        BUILD_MOCK.call_once(|| {
            if !binary.exists() {
                let target_dir = binary
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
                    .expect("target dir");
                let status =
                    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
                        .args([
                            "build",
                            "-p",
                            "hermes-client",
                            "--example",
                            "mock_gated_gateway",
                        ])
                        .arg("--target-dir")
                        .arg(target_dir)
                        .status()
                        .expect("build mock_gated_gateway example");
                assert!(status.success(), "build mock_gated_gateway example failed");
            }
        });
        let mut child = Command::new(binary)
            .arg("--auto-approve")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start mock gateway example");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .expect("mock address");
        assert!(
            line.starts_with("http://127.0.0.1:"),
            "mock did not announce a loopback address"
        );
        Self {
            child,
            base: line.trim().to_owned(),
        }
    }
}
impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn run(home: &Path, args: &[&str]) -> Output {
    let mut cmd = support::talaria(args, home);
    cmd.env("TALARIA_CONFIG_DIR", home);
    support::captured(cmd, Duration::from_secs(10))
}
fn seed(home: &Path) {
    let folder = home.join("remote-tokens");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 11 * 3600;
    let file = folder.join("work.json");
    std::fs::write(&file,serde_json::json!({"version":1,"access_token":"SENTINEL-access","refresh_token":"SENTINEL-refresh","expires_at":expires,"provider":"basic","user_id":"fixture","saved_at":1}).to_string()).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn add_list_status_logout_remove() {
    let home = tempfile::tempdir().unwrap();
    let gateway = Gateway::start();
    let added = run(
        home.path(),
        &[
            "remote",
            "add",
            "work",
            &gateway.base,
            "--provider",
            "basic",
        ],
    );
    assert_eq!(added.status.code(), Some(0));
    assert!(home.path().join("remotes.toml").exists());
    let list = run(home.path(), &["remote", "list"]);
    assert_eq!(list.status.code(), Some(0));
    let text = String::from_utf8_lossy(&list.stdout);
    assert!(
        text.contains("NAME")
            && text.contains("URL")
            && text.contains("STATE")
            && text.contains("EXPIRES")
    );
    assert!(text.contains("work") && text.contains("signed out"));
    seed(home.path());
    let status = run(home.path(), &["remote", "status", "work"]);
    assert_eq!(status.status.code(), Some(0));
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(text.contains("user: fixture\n"));
    assert!(text.contains("name: fixture\n"));
    assert!(text.contains("provider: basic\n"));
    assert!(text.contains("expires: in "));
    assert!(!text.contains("SENTINEL-"), "status leaked a sentinel");
    assert_eq!(
        run(home.path(), &["remote", "logout", "work"])
            .status
            .code(),
        Some(0)
    );
    assert!(!home.path().join("remote-tokens/work.json").exists());
    let list = run(home.path(), &["remote", "list"]);
    assert!(String::from_utf8_lossy(&list.stdout).contains("signed out"));
    assert_eq!(
        run(home.path(), &["remote", "remove", "work"])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        run(home.path(), &["remote", "status", "work"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        run(home.path(), &["remote", "add", "Bad_Name", &gateway.base])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn remote_attach_without_tokens_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let gateway = Gateway::start();
    std::fs::write(
        home.path().join("remotes.toml"),
        format!("[remotes.work]\nurl = {:?}\n", gateway.base),
    )
    .unwrap();
    let marker = home.path().join("called");
    let browser = home.path().join("browser");
    std::fs::write(&browser, "#!/bin/sh\nprintf called > \"$BROWSER_MARKER\"\n").unwrap();
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut cmd = support::talaria(&["--remote", "work"], home.path());
    cmd.env("TALARIA_CONFIG_DIR", home.path())
        .env("BROWSER", browser)
        .env("BROWSER_MARKER", &marker);
    let result = support::captured(cmd, Duration::from_secs(10));
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&result.stderr),
        "talaria: not signed in to work.\nNext step: run talaria remote login work\n"
    );
    assert!(!marker.exists());
}
