//! A script with no model calls never needs a gateway or a root card.
use std::process::Command;
#[test]
fn no_calls_exits_without_discovering_a_gateway() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("empty.js");
    std::fs::write(
        &script,
        "export const meta = {name:'empty',description:'No work'};\nreturn 1;\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(["workflow", "compile"])
        .arg(&script)
        .env("TALARIA_CONFIG_DIR", dir.path())
        .env("HERMES_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "nothing to compile: the script starts no agents\n"
    );
    assert!(output.stderr.is_empty());
}
