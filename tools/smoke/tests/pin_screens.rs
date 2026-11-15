#[test]
fn doctor_screens_have_pin_tokens() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/fixtures/smoke/phase4a");
    let pin = include_str!("../../../crates/hermes-contract/PINNED");
    let tag = pin
        .lines()
        .find_map(|s| s.strip_prefix("tag="))
        .unwrap_or("");
    let commit = pin
        .lines()
        .find_map(|s| s.strip_prefix("commit="))
        .unwrap_or("");
    let prefix: String = commit.chars().take(8).collect();
    let version = tag.strip_prefix('v').unwrap_or(tag);
    for name in [
        "doctor-overlay-r1.txt",
        "doctor-cli-r1.txt",
        "no-gateway-r1.txt",
    ] {
        let text = std::fs::read_to_string(root.join(name)).expect("recorded doctor screen");
        for literal in [tag, version, prefix.as_str()] {
            assert!(
                !text.contains(literal),
                "{name} still contains pinned release spelling {literal:?}"
            );
        }
    }
}
