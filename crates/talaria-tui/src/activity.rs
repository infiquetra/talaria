//! The working row above the composer while a turn runs. Pure.
//!
//! The row reads `✻ <activity> · <elapsed> · <key> twice to interrupt`; after one press of the key
//! it reads `<key> again to interrupt` until the confirmation window ends. Under full motion a soft glow
//! travels across its words and back, eased at both ends, and fades the colours around it on
//! either side; under reduced motion the row is static and only its seconds change.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

use crate::history::{grapheme_width, sanitize};
use crate::live::LiveSection;
use crate::status::Tone;
use crate::theme::color::{blend, constrain_color, depth_color, rgb_of};
use crate::theme::{ColorDepth, Motion, Polarity, Rgb, TerminalColors, Theme};

/// One full sweep, out and back, in milliseconds.
pub const SWEEP_PERIOD_MS: u64 = 2_000;
/// The gap between two frames of the sweep: about 12 frames a second.
pub const FRAME_MS: u64 = 80;
/// Below this many cells of text the row does not sweep.
pub const MIN_SWEEP_WIDTH: u16 = 6;

const GLYPH: &str = "\u{273b}";
const SEPARATOR: &str = " \u{b7} ";
const DEFAULT_ACTIVITY: &str = "Working";

/// The glow's half-width in cells for a row of `width` cells: an eighth, from 3 to 6.
pub fn sweep_radius(width: u16) -> f64 {
    (f64::from(width) / 8.0).clamp(3.0, 6.0)
}

/// Where the glow's centre is, in cells, `elapsed_ms` into the sweep.
///
/// It starts `radius` cells off the left edge, eases across to `radius` cells past the last
/// cell at half the period, and eases back.
pub fn sweep_head(elapsed_ms: u64, period_ms: u64, width: u16, radius: f64) -> f64 {
    let period = period_ms.max(1);
    let phase = (elapsed_ms % period) as f64 / period as f64;
    let eased = (1.0 - (std::f64::consts::TAU * phase).cos()) / 2.0;
    -radius + eased * (f64::from(width) - 1.0 + 2.0 * radius)
}

/// Whether the glow is moving right `elapsed_ms` into the sweep.
fn moving_right(elapsed_ms: u64, period_ms: u64) -> bool {
    let period = period_ms.max(1);
    elapsed_ms % period < period / 2
}

/// The radius of the glow's trailing side `elapsed_ms` into the sweep.
///
/// It grows to twice `radius` while the glow moves fastest, leaving a soft afterglow behind it,
/// and shrinks back to `radius` as the glow slows to turn. Both sides are equal at the turn, so
/// the afterglow fades into the turn instead of being cut off when the direction changes.
pub fn trailing_radius(elapsed_ms: u64, period_ms: u64, radius: f64) -> f64 {
    let period = period_ms.max(1);
    let phase = (elapsed_ms % period) as f64 / period as f64;
    radius * (1.0 + (std::f64::consts::TAU * phase).sin().abs())
}

/// How strongly a cell `distance` cells from the glow's centre glows, 0.0 to 1.0.
///
/// `(1 - (d / r)²)²` inside the radius and nothing outside it.
pub fn glow_weight(distance: f64, radius: f64) -> f64 {
    if radius <= 0.0 {
        return 0.0;
    }
    let ratio = distance.abs() / radius;
    if ratio >= 1.0 {
        return 0.0;
    }
    (1.0 - ratio * ratio).powi(2)
}

/// The colours and depth the row is drawn with, resolved from the theme.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityStyle {
    /// The resting colour of the words: the status bar's muted colour.
    pub base: Rgb,
    /// The colour at the glow's centre: halfway between the text and the accent.
    pub peak: Rgb,
    /// The terminal's colour depth.
    pub depth: ColorDepth,
    /// Full or reduced motion.
    pub motion: Motion,
    /// The static row's style.
    pub text: Style,
    /// The static row's glyph style.
    pub glyph: Style,
    /// The 16-colour glow colour, when colours are on.
    pub accent16: Option<Color>,
}

impl ActivityStyle {
    /// Resolve the row's colours from the theme and the start-up probe colours.
    ///
    /// A theme colour the skin does not set falls back to the probed terminal colours, then to
    /// reference colours for the decided polarity.
    pub fn from_theme(theme: &Theme, terminal: TerminalColors) -> Self {
        let (reference_fg, reference_bg) = match theme.report.polarity.0 {
            Polarity::Dark => (Rgb(0xd0, 0xd0, 0xd0), Rgb(0x10, 0x10, 0x14)),
            Polarity::Light => (Rgb(0x20, 0x20, 0x20), Rgb(0xff, 0xff, 0xff)),
        };
        let foreground = terminal.foreground.unwrap_or(reference_fg);
        let background = terminal.background.unwrap_or(reference_bg);
        let muted = theme.status.style(Tone::Muted);
        let accent = theme.status.style(Tone::Accent);
        let text = theme
            .status
            .style(Tone::Text)
            .fg
            .and_then(rgb_of)
            .unwrap_or(foreground);
        let base = muted
            .fg
            .and_then(rgb_of)
            .unwrap_or_else(|| blend(text, background, 0.45));
        let accent_rgb = accent.fg.and_then(rgb_of).unwrap_or(text);
        let depth = theme.report.depth.0;
        Self {
            base,
            peak: blend(text, accent_rgb, 0.5),
            depth,
            motion: theme.motion,
            text: muted,
            glyph: accent,
            accent16: accent
                .fg
                .and_then(|color| constrain_color(color, ColorDepth::Ansi16)),
        }
    }

    /// The style of one cell whose glow is `weight`.
    fn cell(&self, weight: f64) -> Style {
        let band = || {
            if weight >= 0.6 {
                Style::new().add_modifier(Modifier::BOLD)
            } else if weight >= 0.25 {
                Style::new()
            } else {
                Style::new().add_modifier(Modifier::DIM)
            }
        };
        match self.depth {
            ColorDepth::TrueColor | ColorDepth::Ansi256 => {
                let mut style = Style::new();
                style.fg = depth_color(blend(self.base, self.peak, weight), self.depth);
                style
            }
            ColorDepth::Ansi16 if weight >= 0.6 => {
                let mut style = band();
                style.fg = self
                    .accent16
                    .or_else(|| depth_color(self.peak, ColorDepth::Ansi16));
                style
            }
            ColorDepth::Ansi16 | ColorDepth::None => band(),
        }
    }
}

/// The working row: one row while the foreground turn runs.
pub struct ActivityLine<'a> {
    /// Hermes's latest activity text, if it sent one.
    pub activity: Option<&'a str>,
    /// How long the turn has run; it also places the glow.
    pub elapsed_ms: u64,
    /// The interrupt key's label, if one is bound.
    pub interrupt: Option<&'a str>,
    /// One press of the interrupt key is waiting for a second.
    pub interrupt_armed: bool,
    /// The resolved colours.
    pub style: &'a ActivityStyle,
}

impl ActivityLine<'_> {
    /// The row's text at `width`. A narrow row drops the interrupt hint first, then cuts the
    /// activity text, down to the glyph and the elapsed time.
    pub fn row(&self, width: u16) -> String {
        let activity = self.activity_text();
        let elapsed = elapsed_label(self.elapsed_ms);
        let short = format!("{GLYPH} {activity}{SEPARATOR}{elapsed}");
        if let Some(key) = self.interrupt {
            let (full, brief) = if self.interrupt_armed {
                (
                    format!("{short}{SEPARATOR}{key} again to interrupt"),
                    Some(format!("{short}{SEPARATOR}{key} again")),
                )
            } else {
                (format!("{short}{SEPARATOR}{key} twice to interrupt"), None)
            };
            if columns(&full) <= width {
                return full;
            }
            // The armed hint is the only sign that the first press registered: keep a
            // shorter form of it before dropping it.
            if let Some(brief) = brief.filter(|brief| columns(brief) <= width) {
                return brief;
            }
        }
        if columns(&short) <= width {
            return short;
        }
        let fixed = columns(GLYPH) + 1 + columns(SEPARATOR) + columns(&elapsed);
        let room = width.saturating_sub(fixed);
        if room >= 2 {
            return format!("{GLYPH} {}{SEPARATOR}{elapsed}", ellipsize(&activity, room));
        }
        format!("{GLYPH} {elapsed}")
    }

    /// The last non-empty line of the activity text, sanitised, or `Working`.
    fn activity_text(&self) -> String {
        self.activity
            .and_then(|text| {
                text.lines()
                    .rev()
                    .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
                    .find(|line| !line.is_empty())
            })
            .map_or_else(
                || DEFAULT_ACTIVITY.to_owned(),
                |line| sanitize(&line).into_owned(),
            )
    }
}

impl LiveSection for ActivityLine<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::from(width > 0)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let row = self.row(area.width);
        let used = columns(&row).min(area.width);
        let sweeping = self.style.motion == Motion::Full && used >= MIN_SWEEP_WIDTH;
        if !sweeping {
            let mut spans = vec![Span::styled(GLYPH, self.style.glyph)];
            if let Some(rest) = row.strip_prefix(GLYPH) {
                spans.push(Span::styled(rest.to_owned(), self.style.text));
            }
            buf.set_line(area.x, area.y, &Line::from(spans), area.width);
            return;
        }
        buf.set_line(area.x, area.y, &Line::from(row), area.width);
        let radius = sweep_radius(used);
        let head = sweep_head(self.elapsed_ms, SWEEP_PERIOD_MS, used, radius);
        let right = moving_right(self.elapsed_ms, SWEEP_PERIOD_MS);
        let trail = trailing_radius(self.elapsed_ms, SWEEP_PERIOD_MS, radius);
        for x in 0..used {
            let offset = f64::from(x) - head;
            let trailing = if right { offset < 0.0 } else { offset > 0.0 };
            let weight = glow_weight(offset, if trailing { trail } else { radius });
            buf.set_style(Rect::new(area.x + x, area.y, 1, 1), self.style.cell(weight));
        }
    }
}

/// When the turn started and whether the row was drawn in the latest frame, so the screen wakes
/// only while the row is on screen.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActivityClock {
    started: Option<(u64, Instant)>,
    shown_at: Option<Instant>,
}

impl ActivityClock {
    /// A new frame starts: the row is not shown until [`Self::shown`] says so.
    pub fn begin_frame(&mut self) {
        self.shown_at = None;
    }

    /// Note whether the turn runs at `now_ms`; while it runs, how long it has run.
    pub fn turn(&mut self, running: bool, now_ms: u64, now: Instant) -> Option<u64> {
        if !running {
            self.started = None;
            return None;
        }
        let (since_ms, _) = *self.started.get_or_insert((now_ms, now));
        Some(now_ms.saturating_sub(since_ms))
    }

    /// The row was drawn in this frame.
    pub fn shown(&mut self, now: Instant) {
        self.shown_at = Some(now);
    }

    /// The next frame the row needs: one frame after the last under full motion, the next whole
    /// second of the turn under reduced motion, and nothing while the row is not on screen or no
    /// turn runs.
    pub fn next_wake(&self, motion: Motion, now: Instant) -> Option<Instant> {
        let shown_at = self.shown_at?;
        let (_, start) = self.started?;
        match motion {
            Motion::Full => Some(shown_at + Duration::from_millis(FRAME_MS)),
            Motion::Reduced => {
                let seconds = now.saturating_duration_since(start).as_secs() + 1;
                Some(start + Duration::from_secs(seconds))
            }
        }
    }
}

/// `12s` under a minute, `1m 05s` after.
fn elapsed_label(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1_000;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

fn columns(text: &str) -> u16 {
    text.graphemes(true).fold(0u16, |width, grapheme| {
        width.saturating_add(grapheme_width(grapheme))
    })
}

fn ellipsize(text: &str, width: u16) -> String {
    if columns(text) <= width {
        return text.to_owned();
    }
    let mut result = String::new();
    let mut used = 0u16;
    for grapheme in text.graphemes(true) {
        let next = used.saturating_add(grapheme_width(grapheme));
        if next > width.saturating_sub(1) {
            break;
        }
        result.push_str(grapheme);
        used = next;
    }
    result.push('\u{2026}');
    result
}
