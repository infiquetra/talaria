//! Run terminal-color assertions in a child with a known color policy.

pub fn run_with_terminal_colors(test: &str) -> bool {
    const CHILD: &str = "TALARIA_COLOR_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, "1")
        .env_remove("NO_COLOR")
        .output()
        .expect("run color test child");
    assert!(
        output.status.success(),
        "color test child failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}
