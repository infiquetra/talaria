//! The working row above the composer: what it says, how it fits a narrow terminal, how the
//! glow travels across it at each colour depth, and when the screen wakes to move it.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use talaria_tui::activity::{
    ActivityClock, ActivityLine, ActivityStyle, FRAME_MS, SWEEP_PERIOD_MS, glow_weight, sweep_head,
    sweep_radius, trailing_radius,
};
use talaria_tui::live::LiveSection;
use talaria_tui::theme::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, Rgb, TerminalColors,
    ThemeInputs, resolve,
};

const TERMINAL: TerminalColors = TerminalColors {
    background: Some(Rgb(0, 0, 0)),
    foreground: Some(Rgb(200, 200, 200)),
};

fn style(depth: ColorDepth, motion: Motion) -> ActivityStyle {
    let theme = resolve(&ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: true,
        polarity: (Polarity::Dark, PolaritySource::Default),
        depth: (depth, DepthSource::Default),
        motion: (motion, MotionSource::Default),
        terminal: TERMINAL,
    });
    ActivityStyle::from_theme(&theme, TERMINAL)
}

fn line<'a>(
    activity: Option<&'a str>,
    elapsed_ms: u64,
    style: &'a ActivityStyle,
) -> ActivityLine<'a> {
    ActivityLine {
        activity,
        elapsed_ms,
        interrupt: Some("Esc"),
        interrupt_armed: false,
        style,
    }
}

fn render(section: &ActivityLine<'_>, width: u16) -> Buffer {
    let area = Rect::new(0, 0, width, section.height(width));
    let mut buf = Buffer::empty(area);
    section.render(area, &mut buf);
    buf
}

fn text(buf: &Buffer) -> String {
    let area = buf.area;
    (0..area.width)
        .map(|x| buf[(x, 0)].symbol().to_owned())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The elapsed time at which the head sits on `cell` while moving right.
fn elapsed_with_head_on(cell: f64, width: u16) -> u64 {
    let radius = sweep_radius(width);
    (0..SWEEP_PERIOD_MS / 2)
        .min_by(|a, b| {
            let da = (sweep_head(*a, SWEEP_PERIOD_MS, width, radius) - cell).abs();
            let db = (sweep_head(*b, SWEEP_PERIOD_MS, width, radius) - cell).abs();
            da.total_cmp(&db)
        })
        .expect("a time in the first half of the sweep")
}

#[test]
fn row_names_activity_elapsed_and_interrupt_key() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    let section = line(Some("(o_o) pondering..."), 12_000, &style);
    assert_eq!(section.height(80), 1);
    assert_eq!(
        text(&render(&section, 80)),
        "✻ (o_o) pondering... · 12s · Esc twice to interrupt"
    );
}

#[test]
fn armed_row_says_the_key_again_and_keeps_a_short_form_on_a_narrow_row() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    let mut section = line(Some("pondering"), 12_000, &style);
    section.interrupt_armed = true;
    assert_eq!(
        text(&render(&section, 80)),
        "✻ pondering · 12s · Esc again to interrupt"
    );
    // Too narrow for the whole hint: the short form stays, because it is the only sign that
    // the first press registered.
    assert_eq!(text(&render(&section, 30)), "✻ pondering · 12s · Esc again");
    // Too narrow for either: the hint goes, as it does for the plain row.
    assert_eq!(text(&render(&section, 20)), "✻ pondering · 12s");
    section.interrupt_armed = false;
    assert_eq!(text(&render(&section, 30)), "✻ pondering · 12s");
}

#[test]
fn row_falls_back_to_working_without_activity() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    for activity in [None, Some(""), Some("  \n  ")] {
        let section = line(activity, 3_000, &style);
        assert_eq!(
            text(&render(&section, 80)),
            "✻ Working · 3s · Esc twice to interrupt"
        );
    }
    let multi = line(Some("first line\nwaiting on provider\n"), 0, &style);
    assert_eq!(
        text(&render(&multi, 80)),
        "✻ waiting on provider · 0s · Esc twice to interrupt"
    );
    let hostile = line(Some("bad\u{1b}[2Jtext"), 0, &style);
    assert!(!text(&render(&hostile, 80)).contains('\u{1b}'));
}

#[test]
fn elapsed_switches_to_minutes_after_sixty_seconds() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    for (elapsed_ms, shown) in [
        (59_999, "59s"),
        (60_000, "1m 00s"),
        (65_000, "1m 05s"),
        (3_725_000, "62m 05s"),
    ] {
        let section = line(None, elapsed_ms, &style);
        assert_eq!(
            text(&render(&section, 80)),
            format!("✻ Working · {shown} · Esc twice to interrupt")
        );
    }
}

#[test]
fn narrow_widths_drop_the_hint_then_cut_the_activity() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    let section = line(Some("(o_o) pondering..."), 12_000, &style);
    for (width, shown) in [
        (80, "✻ (o_o) pondering... · 12s · Esc twice to interrupt"),
        (30, "✻ (o_o) pondering... · 12s"),
        (12, "✻ (o_… · 12s"),
        (5, "✻ 12s"),
    ] {
        assert_eq!(text(&render(&section, width)), shown, "width {width}");
    }
}

#[test]
fn zero_width_has_no_height() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    assert_eq!(line(None, 0, &style).height(0), 0);
}

#[test]
fn sweep_head_travels_left_to_right_and_back() {
    let width = 40;
    let radius = sweep_radius(width);
    assert_eq!(radius, 5.0);
    let at = |ms| sweep_head(ms, SWEEP_PERIOD_MS, width, radius);
    let quarter = SWEEP_PERIOD_MS / 4;
    assert!(at(0) < 0.0, "it starts off the left edge: {}", at(0));
    assert!(
        (at(quarter) - 19.5).abs() < 0.01,
        "the middle: {}",
        at(quarter)
    );
    assert!(
        at(2 * quarter) > f64::from(width - 1),
        "past the right edge"
    );
    assert!(
        (at(3 * quarter) - 19.5).abs() < 0.01,
        "back through the middle"
    );
    assert!((at(SWEEP_PERIOD_MS) - at(0)).abs() < 1e-9, "periodic");
    assert!(
        at(quarter / 2) < at(quarter),
        "moving right in the first half"
    );
    assert!(
        at(5 * quarter / 2) > at(3 * quarter),
        "moving left in the second half"
    );
    assert_eq!(glow_weight(0.0, radius), 1.0);
    assert_eq!(glow_weight(radius, radius), 0.0);
    let trail = |ms| trailing_radius(ms, SWEEP_PERIOD_MS, radius);
    assert!(
        (trail(quarter) - 2.0 * radius).abs() < 1e-9,
        "the afterglow is twice as long at full speed"
    );
    assert!(glow_weight(radius, trail(quarter)) > 0.0);
    for turn in [0, 2 * quarter, SWEEP_PERIOD_MS] {
        assert!(
            (trail(turn) - radius).abs() < 1e-9,
            "both sides are equal at the turn at {turn} ms"
        );
    }
}

/// How far each cell's colour sits from the base towards the peak, 0.0 to 1.0.
fn rendered_weights(buf: &Buffer, width: u16, style: &ActivityStyle) -> Vec<f64> {
    let channels = |rgb: Rgb| [rgb.0, rgb.1, rgb.2].map(f64::from);
    let base = channels(style.base);
    let peak = channels(style.peak);
    let widest = (0..3)
        .max_by(|a, b| {
            (peak[*a] - base[*a])
                .abs()
                .total_cmp(&(peak[*b] - base[*b]).abs())
        })
        .expect("three channels");
    (0..width)
        .map(|x| {
            let Color::Rgb(r, g, b) = buf[(x, 0)].fg else {
                panic!("cell {x} has a 24-bit colour: {:?}", buf[(x, 0)].fg);
            };
            let cell = channels(Rgb(r, g, b));
            (cell[widest] - base[widest]) / (peak[widest] - base[widest])
        })
        .collect()
}

#[test]
fn afterglow_fades_through_each_turn_instead_of_popping() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    let width = u16::try_from(text(&render(&line(None, 0, &style), 80)).chars().count()).unwrap();
    let half = SWEEP_PERIOD_MS / 2;
    let weights =
        |elapsed| rendered_weights(&render(&line(None, elapsed, &style), 80), width, &style);
    // The right-hand turn, and the left-hand turn that ends the sweep and starts the next.
    for turn in [half, SWEEP_PERIOD_MS + half, 2 * SWEEP_PERIOD_MS] {
        // The glow sits in the same place just before and just after the turn, so each cell
        // looks the same whichever way the glow is about to move.
        for delta in [20, 40] {
            let (before, after) = (weights(turn - delta), weights(turn + delta));
            for (x, (after, before)) in after.iter().zip(&before).enumerate() {
                let jump = (after - before).abs();
                assert!(
                    jump < 0.1,
                    "cell {x} pops by {jump:.2} across the turn at {turn} ms (±{delta} ms)"
                );
            }
        }
        // Frames 40 ms apart through the turn change no cell by more than the glow's own fading.
        let frames: Vec<Vec<f64>> = (0..=6)
            .map(|step| weights(turn - 120 + step * 40))
            .collect();
        for pair in frames.windows(2) {
            for (x, (next, previous)) in pair[1].iter().zip(&pair[0]).enumerate() {
                let jump = (next - previous).abs();
                assert!(
                    jump < 0.35,
                    "cell {x} jumps by {jump:.2} between frames near the turn at {turn} ms"
                );
            }
        }
    }
}

#[test]
fn truecolor_glow_brightens_cells_near_the_head_and_leaves_far_cells_at_base() {
    let style = style(ColorDepth::TrueColor, Motion::Full);
    let plain = line(None, 0, &style);
    let width: u16 = u16::try_from(text(&render(&plain, 80)).chars().count()).unwrap();
    let elapsed = elapsed_with_head_on(10.0, width);
    let section = line(None, elapsed, &style);
    let buf = render(&section, 80);
    // The seconds digit may differ from the plain row; the words are the same.
    assert!(text(&buf).starts_with("✻ Working · "));
    let peak = Color::Rgb(style.peak.0, style.peak.1, style.peak.2);
    let base = Color::Rgb(style.base.0, style.base.1, style.base.2);
    let Color::Rgb(r, g, b) = buf[(10, 0)].fg else {
        panic!("the head cell has a 24-bit colour: {:?}", buf[(10, 0)].fg);
    };
    let near = |a: u8, b: u8| a.abs_diff(b) <= 8;
    assert!(
        near(r, style.peak.0) && near(g, style.peak.1) && near(b, style.peak.2),
        "the head is near the peak {peak:?}, got {:?}",
        buf[(10, 0)].fg
    );
    let far = 10 + 3 * (sweep_radius(width) as u16);
    assert_eq!(
        buf[(far, 0)].fg,
        base,
        "a far cell stays at the base colour"
    );
    assert!(!buf[(far, 0)].modifier.contains(Modifier::DIM));
    assert_ne!(style.base, style.peak);
}

#[test]
fn ansi256_glow_uses_indexed_colours() {
    let style = style(ColorDepth::Ansi256, Motion::Full);
    let section = line(None, SWEEP_PERIOD_MS / 4, &style);
    let buf = render(&section, 80);
    let width = u16::try_from(text(&buf).chars().count()).unwrap();
    for x in 0..width {
        assert!(
            matches!(buf[(x, 0)].fg, Color::Indexed(_)),
            "cell {x} is {:?}",
            buf[(x, 0)].fg
        );
    }
}

#[test]
fn sixteen_and_no_colour_use_dim_normal_bold_bands() {
    for depth in [ColorDepth::Ansi16, ColorDepth::None] {
        let style = style(depth, Motion::Full);
        let plain = line(None, 0, &style);
        let width = u16::try_from(text(&render(&plain, 80)).chars().count()).unwrap();
        let section = line(None, elapsed_with_head_on(10.0, width), &style);
        let buf = render(&section, 80);
        assert!(
            buf[(10, 0)].modifier.contains(Modifier::BOLD),
            "{depth:?}: the head is bold"
        );
        let far = 10 + 3 * 6;
        assert!(
            buf[(far, 0)].modifier.contains(Modifier::DIM),
            "{depth:?}: a far cell is dim"
        );
        assert!(
            (0..80).any(|x| {
                let modifier = buf[(x, 0)].modifier;
                buf[(x, 0)].symbol() != " "
                    && !modifier.contains(Modifier::DIM)
                    && !modifier.contains(Modifier::BOLD)
            }),
            "{depth:?}: a band of normal cells sits between"
        );
        for x in 0..80 {
            assert!(
                !matches!(buf[(x, 0)].fg, Color::Rgb(..) | Color::Indexed(_)),
                "{depth:?}: cell {x} is {:?}",
                buf[(x, 0)].fg
            );
            if depth == ColorDepth::None {
                assert_eq!(buf[(x, 0)].fg, Color::Reset, "no colour at depth none");
            }
        }
    }
}

#[test]
fn reduced_motion_is_identical_at_any_time_within_a_second() {
    let style = style(ColorDepth::TrueColor, Motion::Reduced);
    let early = render(&line(None, 1_000, &style), 80);
    let late = render(&line(None, 1_900, &style), 80);
    assert_eq!(early, late, "nothing moves within a second");
    let next = render(&line(None, 2_000, &style), 80);
    let changed: Vec<u16> = (0..80).filter(|&x| next[(x, 0)] != early[(x, 0)]).collect();
    assert_eq!(text(&next), "✻ Working · 2s · Esc twice to interrupt");
    assert_eq!(
        changed.len(),
        1,
        "only the seconds digit changes: {changed:?}"
    );
}

#[test]
fn idle_clock_schedules_nothing() {
    let start = Instant::now();
    let mut clock = ActivityClock::default();
    clock.begin_frame();
    assert_eq!(clock.turn(false, 5_000, start), None);
    for motion in [Motion::Full, Motion::Reduced] {
        assert_eq!(clock.next_wake(motion, start), None, "{motion:?}");
    }
    // Even a row wrongly marked shown with no turn running wakes nothing.
    clock.shown(start);
    for motion in [Motion::Full, Motion::Reduced] {
        assert_eq!(clock.next_wake(motion, start), None, "{motion:?}");
    }
}

#[test]
fn clock_animates_only_while_the_row_shows() {
    let start = Instant::now();
    let mut clock = ActivityClock::default();
    clock.begin_frame();
    assert_eq!(clock.turn(true, 10_000, start), Some(0));
    clock.shown(start);
    assert_eq!(
        clock.next_wake(Motion::Full, start),
        Some(start + Duration::from_millis(FRAME_MS))
    );
    assert_eq!(
        clock.next_wake(Motion::Reduced, start),
        Some(start + Duration::from_secs(1)),
        "reduced motion wakes on the next whole second"
    );

    let later = start + Duration::from_millis(2_300);
    clock.begin_frame();
    assert_eq!(clock.turn(true, 12_300, later), Some(2_300));
    clock.shown(later);
    assert_eq!(
        clock.next_wake(Motion::Reduced, later),
        Some(start + Duration::from_secs(3))
    );

    // A prompt card or an overlay owns the screen: the row is not drawn and nothing is woken.
    clock.begin_frame();
    assert_eq!(clock.turn(true, 12_400, later), Some(2_400));
    assert_eq!(clock.next_wake(Motion::Full, later), None);

    // The turn ends: the next turn counts from zero again.
    clock.begin_frame();
    assert_eq!(clock.turn(false, 13_000, later), None);
    assert_eq!(clock.next_wake(Motion::Full, later), None);
    clock.begin_frame();
    assert_eq!(clock.turn(true, 20_000, later), Some(0));
}
