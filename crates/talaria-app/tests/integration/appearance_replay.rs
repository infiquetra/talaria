//! The recorded handshake skin resolves. Behavior contract: issue 227.

use std::path::PathBuf;

use hermes_contract::schemas::RuntimeDescribeResult;
use serde_json::Value;
use talaria_core::appearance::{Skin, TerminalColors};
use talaria_tui::theme::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, Rgb, ThemeInputs,
};

#[test]
fn recorded_describe_skin_resolves() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/plain-answer.jsonl");
    let text = std::fs::read_to_string(&path).expect("the recorded handshake");
    let result = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|record| record.get("frame").cloned())
        .find(|frame| frame.get("id") == Some(&Value::from(2)) && frame.get("result").is_some())
        .and_then(|frame| frame.get("result").cloned())
        .expect("the handshake describe reply");
    let _describe: RuntimeDescribeResult = serde_json::from_value(result.clone())
        .expect("the reply decodes through the generated types");

    let (skin, skipped) = Skin::from_value(result.get("skin").unwrap_or(&Value::Null));
    let skin = skin.expect("the recorded skin applies");
    assert_eq!(skin.name, "default");
    assert!(skipped.is_empty(), "no skipped values for a decision");

    let inputs = || ThemeInputs {
        skin: Some(&skin),
        skipped: &skipped,
        follow_hermes_skin: true,
        polarity: (
            Polarity::Dark,
            PolaritySource::TerminalBackground(Rgb(0x1e, 0x1e, 0x1e)),
        ),
        depth: (ColorDepth::TrueColor, DepthSource::Colorterm),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors {
            background: Some(Rgb(0x1e, 0x1e, 0x1e)),
            foreground: None,
        },
    };
    let first = talaria_tui::theme::resolve(&inputs()).report;
    let second = talaria_tui::theme::resolve(&inputs()).report;
    assert_eq!(first, second, "the same input resolves twice");
    assert_eq!(first.skin.as_deref(), Some("default"));
    assert!(first.ignored.is_empty());
}
