#[path = "cron_support/mod.rs"]
mod support;

use talaria_core::cron::{duration_text, parse_iso_instant, relative};

#[test]
fn parse_iso_instant_accepts_hermes_shapes() {
    let zoned = parse_iso_instant("2026-10-23T09:00:00-04:00").expect("offset");
    assert_eq!(zoned.unix, 1_792_760_400);
    assert_eq!(zoned.wall, "2026-10-23 09:00");
    let fraction = parse_iso_instant("2026-10-21T23:00:04.120931-04:00").expect("fraction");
    assert_eq!(fraction.unix, 1_792_638_004);
    assert_eq!(fraction.wall, "2026-10-21 23:00");
    let zulu = parse_iso_instant("2026-10-23T13:00:00Z").expect("zulu");
    assert_eq!(zulu.unix, 1_792_760_400);
    assert_eq!(zulu.wall, "2026-10-23 13:00");
    let half = parse_iso_instant("2026-10-23T18:30:00+05:30").expect("half hour");
    assert_eq!(half.unix, zulu.unix);
    assert_eq!(half.wall, "2026-10-23 18:30");
    assert!(parse_iso_instant("2026-11-01T09:00").is_none());
    assert!(parse_iso_instant("soon").is_none());
}

#[test]
fn relative_text_boundaries() {
    let cases = [
        (59, "now"),
        (-59, "now"),
        (60, "in 1m"),
        (-60, "1m ago"),
        (3599, "in 59m"),
        (-3599, "59m ago"),
        (3600, "in 1h"),
        (-3600, "1h ago"),
        (86399, "in 23h 59m"),
        (-86399, "23h 59m ago"),
        (86400, "in 1d"),
        (-86400, "1d ago"),
    ];
    for (delta, text) in cases {
        assert_eq!(relative(0, delta), text, "relative {delta}");
    }
    let durations = [
        (0, "0s"),
        (45, "45s"),
        (185, "3m 05s"),
        (3600, "1h 00m"),
        (3720, "1h 02m"),
        (90000, "25h 00m"),
    ];
    for (seconds, text) in durations {
        assert_eq!(duration_text(seconds), text, "duration {seconds}");
    }
    let _ = support::WALL;
}
