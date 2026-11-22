//! Status-row notice and warning count rendering.

use talaria_tui::status::{Connection, NoticeLine, Segment, StatusInputs, Tone, notice_tone, plan};
use unicode_width::UnicodeWidthStr;

fn base() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        ..StatusInputs::default()
    }
}

#[test]
fn current_notice_uses_the_level_tone() {
    let cases = [
        ("warn", Tone::Warning),
        ("error", Tone::Error),
        ("success", Tone::Success),
        ("info", Tone::Text),
        ("other", Tone::Muted),
    ];

    for (level, expected_tone) in cases {
        let inputs = StatusInputs {
            notice: Some(NoticeLine {
                text: "head".to_owned(),
                tone: notice_tone(level),
            }),
            warning_count: 0,
            ..base()
        };
        let planned = plan(&inputs, None, 80);
        let (notice_text, notice_tone_actual) = planned
            .notice
            .as_ref()
            .expect("notice should be present on LinePlan");
        assert_eq!(notice_text, "head", "level={level}");
        assert_eq!(*notice_tone_actual, expected_tone, "level={level}");
    }

    assert_eq!(Segment::ALL.len(), 15);
    assert_eq!(Segment::ALL.last().copied(), Some(Segment::Ultracode));
}

#[test]
fn notice_text_is_cut_to_half_the_row() {
    let long = "a".repeat(40);
    let inputs = StatusInputs {
        notice: Some(NoticeLine {
            text: long.clone(),
            tone: Tone::Text,
        }),
        warning_count: 0,
        ..base()
    };

    let narrow = plan(&inputs, None, 20);
    let (narrow_text, _) = narrow
        .notice
        .as_ref()
        .expect("notice should appear at width 20");
    assert!(
        narrow_text.width() <= 10,
        "expected notice width <= 10, got {} ({narrow_text:?})",
        narrow_text.width()
    );
    assert!(
        narrow_text.ends_with('\u{2026}'),
        "expected truncated notice to end with ellipsis, got {narrow_text:?}"
    );

    let wide = plan(&inputs, None, 80);
    let (wide_text, _) = wide
        .notice
        .as_ref()
        .expect("notice should appear at width 80");
    assert!(
        !wide_text.ends_with('\u{2026}'),
        "expected wide notice not to end with ellipsis, got {wide_text:?}"
    );
    assert_eq!(wide_text, &long);
}

#[test]
fn warning_count_appears_on_the_status_line() {
    let two = StatusInputs {
        notice: None,
        warning_count: 2,
        ..base()
    };
    let two_plan = plan(&two, None, 80);
    assert!(
        two_plan.text().contains("2 warnings"),
        "expected '2 warnings' in {:?}",
        two_plan.text()
    );
    let (warnings_text, warnings_tone) = two_plan
        .warnings
        .as_ref()
        .expect("warnings should be present on LinePlan");
    assert_eq!(warnings_text, "2 warnings");
    assert_eq!(*warnings_tone, Tone::Muted);

    let one = StatusInputs {
        notice: None,
        warning_count: 1,
        ..base()
    };
    let one_plan = plan(&one, None, 80);
    let one_text = one_plan.text();
    assert!(
        one_text.contains("1 warning") && !one_text.contains("1 warnings"),
        "expected '1 warning' in {one_text:?}"
    );
    assert_eq!(
        one_plan.warnings.as_ref().map(|(text, _)| text.as_str()),
        Some("1 warning")
    );

    let zero = StatusInputs {
        notice: None,
        warning_count: 0,
        ..base()
    };
    let zero_plan = plan(&zero, None, 80);
    assert!(zero_plan.warnings.is_none());
    let zero_text = zero_plan.text();
    assert!(
        !zero_text.contains("warning"),
        "expected no warning text when count is 0, got {zero_text:?}"
    );
}
