//! Hostile titles and commands do not survive into the terminal bytes.

#![allow(unreachable_code)]

use talaria_core::attention::{AlertKind, NeedSummary};
use talaria_core::fleet::{NeedDetail, RowKey};
use talaria_core::fleet_attention::{fleet_alert_text, row_label};
use talaria_tui::terminal::alert_bytes;

#[test]
fn hostile_titles_and_commands_leave_no_escape() {
    let title = "a\u{1b}]0;x\u{7}b\u{202e}c";
    let command = "rm\u{1b}[31m -rf\u{7} x\u{202e}";
    let key = RowKey {
        profile: "fixture".to_owned(),
        session_id: "abcd9f3c2a".to_owned(),
    };
    let label = row_label(&key, title, "fixture");
    let detail = NeedDetail::Known(NeedSummary::Approval {
        command: command.to_owned(),
        description: String::new(),
    });
    let text =
        fleet_alert_text(AlertKind::NeedsYou, &label, Some(&detail), None).expect("needs-you text");
    let bytes = alert_bytes(&text, true, true, false);

    assert_eq!(bytes.first().copied(), Some(0x07), "opening bell");
    assert_eq!(
        &bytes[1..5],
        &[0x1b, b']', b'9', b';'],
        "desktop introducer"
    );
    assert_eq!(bytes.last().copied(), Some(0x07), "closing bell");
    let body = &bytes[5..bytes.len() - 1];
    assert!(
        body.iter().all(|byte| *byte >= 0x20 && *byte != 0x7f),
        "the body keeps no control byte"
    );
    let rendered = String::from_utf8_lossy(body);
    assert!(rendered.starts_with("Talaria: "), "{rendered}");
    assert!(!rendered.contains('\u{202e}'), "{rendered}");
}
