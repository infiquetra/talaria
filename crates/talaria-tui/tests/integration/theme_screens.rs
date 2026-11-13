//! Synthetic screens for the built-in themes, pinned before they are drawn.
use std::process::Command;

use talaria_core::appearance::Rgb;
use talaria_tui::theme::color::relative_luminance;
use talaria_tui::theme::screens::{self, ThemeScreenFrame};

const CONVERSATION: &[&str] = &[
    "Theme preview",
    "one plain item",
    "readable notes",
    "quoted remark",
    "swatch cell",
    "sample cell",
    "let colour = 1",
    "checking contrast",
    "running lookup",
    "finished lookup",
    "kept line",
    "dropped line",
    "queued prompt",
    "synthetic status",
    "quiet composer",
];

fn captured() -> Vec<ThemeScreenFrame> {
    screens::frames()
}

fn workspace<'a>(
    frames: &'a [ThemeScreenFrame],
    id: &str,
    width: u16,
    height: u16,
) -> &'a ThemeScreenFrame {
    frames
        .iter()
        .find(|frame| {
            frame.theme_id == id && frame.width == width && frame.height == height && !frame.inline
        })
        .unwrap_or_else(|| panic!("no {id} frame at {width}x{height}"))
}

fn require_conversation(frame: &ThemeScreenFrame) {
    for phrase in CONVERSATION {
        assert!(
            frame.text.contains(phrase),
            "{} {}x{} inline {} is missing {phrase:?}",
            frame.theme_id,
            frame.width,
            frame.height,
            frame.inline
        );
    }
    assert!(
        frame.code_colours >= 2,
        "{} code block needs more than one syntax colour",
        frame.theme_id
    );
}

fn require_workspace(frame: &ThemeScreenFrame) {
    require_conversation(frame);
    for phrase in ["Work", "Choose a model"] {
        assert!(
            frame.text.contains(phrase),
            "{} {}x{} is missing {phrase}",
            frame.theme_id,
            frame.width,
            frame.height
        );
    }
    let paints = matches!(frame.theme_id, "ember" | "paper");
    assert_eq!(
        frame.paints_background, paints,
        "{} should paint its own background only when the theme has one",
        frame.theme_id
    );
}

#[test]
fn terminal_ember_and_paper_frames_at_both_sizes() {
    let frames = captured();
    for id in ["terminal", "ember", "paper"] {
        for (width, height) in [(80, 24), (120, 40)] {
            let frame = workspace(&frames, id, width, height);
            assert_eq!((frame.width, frame.height), (width, height));
            require_workspace(frame);
        }
    }
}

#[test]
fn ember_and_paper_inline_on_light_and_dark() {
    let frames = captured();
    for id in ["ember", "paper"] {
        let inline: Vec<_> = frames
            .iter()
            .filter(|frame| {
                frame.theme_id == id && frame.inline && frame.width == 80 && frame.height == 24
            })
            .collect();
        assert_eq!(
            inline.len(),
            2,
            "{id} needs a light and a dark inline frame"
        );
        let light = inline
            .iter()
            .copied()
            .find(|frame| {
                frame
                    .terminal_background
                    .is_some_and(|color| relative_luminance(color) > 0.5)
            })
            .unwrap_or_else(|| panic!("{id} inline frame on a light terminal background"));
        let dark = inline
            .iter()
            .copied()
            .find(|frame| {
                frame
                    .terminal_background
                    .is_some_and(|color| relative_luminance(color) <= 0.5)
            })
            .unwrap_or_else(|| panic!("{id} inline frame on a dark terminal background"));
        assert_ne!(
            light.terminal_background.map(Rgb::hex),
            dark.terminal_background.map(Rgb::hex)
        );
        for frame in [light, dark] {
            require_conversation(frame);
            assert!(
                !frame.paints_background,
                "{id} inline view paints no theme background"
            );
        }
    }
}

fn forbidden_fragments() -> Vec<String> {
    let mut forbidden = Vec::new();
    for name in ["USER", "HOME", "LOGNAME"] {
        if let Ok(value) = std::env::var(name)
            && value.len() >= 3
        {
            forbidden.push(value);
        }
    }
    if let Ok(output) = Command::new("hostname").output()
        && let Ok(text) = String::from_utf8(output.stdout)
    {
        let host = text.trim();
        if host.len() >= 3 {
            forbidden.push(host.to_owned());
        }
    }
    forbidden
}

#[test]
fn screens_are_synthetic() {
    let forbidden = forbidden_fragments();
    let markers = [
        "http://",
        "https://",
        "localhost",
        "127.0.0.1",
        ".toml",
        ".rs",
        ".md",
        "://",
    ];
    for frame in captured() {
        for marker in markers {
            assert!(
                !frame.text.contains(marker),
                "{} contains {marker}",
                frame.theme_id
            );
        }
        for value in &forbidden {
            assert!(
                !frame.text.contains(value),
                "{} contains a host, user name or path",
                frame.theme_id
            );
        }
    }
}
