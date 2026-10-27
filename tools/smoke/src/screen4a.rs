//! Pure readers for configuration screens and doctor output.

use regex::Regex;
use serde_json::Value;

use crate::screen2a::normalised;
use crate::wording4a::{
    DOCTOR_SUMMARY_RE, MODELS_FAILED, MODELS_TITLE_FMT, PROFILES_HEADER, PROVIDERS_KEY_TAIL,
    PROVIDERS_TITLE_FMT, SETTINGS_CATEGORIES, SETTINGS_TITLE_FMT, WRITE_BANNERS,
};

#[derive(Debug, PartialEq, Eq)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
    pub flags: Vec<String>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ProviderRow {
    pub name: String,
    pub status: String,
    pub key_tail: Option<String>,
}

// Strip control sequences before inspecting any captured screen or command output.
pub(crate) fn plain(text: &str) -> String {
    let Ok(escapes) = Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
    else {
        return text.to_owned();
    };
    escapes.replace_all(text, "").into_owned()
}

fn lines(text: &str) -> Vec<String> {
    normalised(&plain(text))
}

fn columns(line: &str) -> Vec<&str> {
    line.split("  ")
        .map(str::trim)
        .map(|column| column.strip_prefix("> ").unwrap_or(column))
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn settings_title_found(text: &str, profile: &str) -> bool {
    let title = format!("{SETTINGS_TITLE_FMT}{profile}");
    lines(text).iter().any(|line| line == &title)
}

pub fn settings_categories(text: &str) -> Vec<String> {
    lines(text)
        .iter()
        .flat_map(|line| {
            line.split_whitespace()
                .filter(|word| SETTINGS_CATEGORIES.contains(word))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn setting_row(text: &str, key: &str) -> Option<SettingRow> {
    lines(text).iter().find_map(|line| {
        let parts = columns(line);
        let index = parts.iter().position(|part| *part == key)?;
        let value = parts.get(index + 1)?;
        Some(SettingRow {
            key: key.to_owned(),
            value: value
                .strip_suffix(" (r reveals once)")
                .unwrap_or(value)
                .to_string(),
            flags: parts[index + 2..]
                .iter()
                .map(|part| (*part).to_owned())
                .collect(),
        })
    })
}

pub fn providers_row(text: &str, name: &str) -> Option<ProviderRow> {
    lines(text).iter().find_map(|line| {
        let line = line.trim_start_matches(['>', '›', ' ']);
        let status = if let Some(rest) = line.strip_prefix(&format!("{name} · ")) {
            // Key rows include the environment-variable name before their status.
            rest.split_once(" · ")?.1.to_owned()
        } else {
            let parts = columns(line);
            let index = parts.iter().position(|part| *part == name)?;
            parts.get(index + 1..)?.join("  ")
        };
        if status.is_empty() {
            return None;
        }
        let key_tail = status.split_once(PROVIDERS_KEY_TAIL).and_then(|(_, tail)| {
            let tail = tail.split_whitespace().next()?;
            (!tail.is_empty()).then(|| tail.to_owned())
        });
        Some(ProviderRow {
            name: name.to_owned(),
            status,
            key_tail,
        })
    })
}

pub fn providers_tabs(text: &str) -> Vec<String> {
    const TABS: [&str; 3] = ["Keys", "Accounts", "Endpoints"];
    lines(text)
        .iter()
        .flat_map(|line| {
            line.split(|c: char| c.is_whitespace() || matches!(c, '[' | ']'))
                .filter(|word| TABS.contains(word))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn models_sections(text: &str) -> Vec<String> {
    const SECTIONS: [&str; 5] = [
        "Main",
        "Fallbacks",
        "Helpers",
        "Subagents",
        "Mixture of Agents",
    ];
    let mut section_found = false;
    lines(text)
        .iter()
        .filter_map(|line| {
            let line = line.trim_start_matches(['>', ' ']);
            if line.starts_with(MODELS_TITLE_FMT) {
                section_found = false;
                return None;
            }
            if section_found {
                return None;
            }
            let section = if SECTIONS.contains(&line) {
                Some(line.to_owned())
            } else if line.starts_with("Model:") {
                Some("Main".to_owned())
            } else if line.starts_with("1. ") && line.contains("(via ") {
                Some("Fallbacks".to_owned())
            } else if line.starts_with("Vision - ") {
                Some("Helpers".to_owned())
            } else if line.starts_with("Route:") {
                Some("Subagents".to_owned())
            } else if line.starts_with("No presets of your own.") {
                Some("Mixture of Agents".to_owned())
            } else {
                None
            };
            section_found = section.is_some();
            section
        })
        .collect()
}

pub fn models_failed(text: &str) -> bool {
    lines(text)
        .iter()
        .any(|line| line.split_whitespace().any(|word| word == MODELS_FAILED))
}

pub fn profiles_rows(text: &str) -> Vec<String> {
    lines(text)
        .into_iter()
        .filter(|line| {
            let parts = columns(line);
            parts.len() >= 2 && matches!(parts[1], "served" | "not served" | "on disk")
        })
        .collect()
}

pub fn refusal_found(text: &str, exact: &str) -> bool {
    let lines = lines(text);
    lines.iter().any(|line| line == exact)
        || lines
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains(exact)
}

pub fn write_banner_found(text: &str) -> bool {
    plain(text).lines().any(|line| {
        // The key card describes its destination before a key is submitted.
        !(line.contains("Saved to profile ") && line.contains("key file as "))
            && WRITE_BANNERS.iter().any(|banner| line.contains(banner))
    })
}

pub fn doctor_rows(text: &str) -> Vec<String> {
    lines(text)
        .into_iter()
        .filter(|line| {
            ["ok", "warn", "warning", "fail", "skip"]
                .iter()
                .any(|status| {
                    line.split_once(' ').is_some_and(|(first, rest)| {
                        first.eq_ignore_ascii_case(status) && !rest.trim().is_empty()
                    })
                })
        })
        .collect()
}

pub fn doctor_json(text: &str) -> Option<(String, usize)> {
    let text = plain(text);
    let parse = |s: &str| -> Option<(String, usize)> {
        let value: Value = serde_json::from_str(s.trim()).ok()?;
        Some((
            value.get("format")?.as_str()?.to_owned(),
            value.get("checks")?.as_array()?.len(),
        ))
    };
    parse(&text).or_else(|| text.lines().find_map(parse))
}

pub fn doctor_summary(text: &str) -> Option<(usize, usize, usize)> {
    let pattern = Regex::new(DOCTOR_SUMMARY_RE).ok()?;
    let text = plain(text);
    let captures = pattern.captures(&text)?;
    Some((
        captures.get(1)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(3)?.as_str().parse().ok()?,
    ))
}

pub fn idle_screen(text: &str) -> bool {
    let text = plain(text);
    let no_overlay = [
        SETTINGS_TITLE_FMT,
        PROVIDERS_TITLE_FMT,
        MODELS_TITLE_FMT,
        PROFILES_HEADER,
        "Doctor:",
    ]
    .iter()
    .all(|title| !text.contains(title));
    no_overlay
        && normalised(&text).iter().any(|line| {
            matches!(
                line.trim(),
                ">" | "> Type a message. Enter sends, Shift+Enter adds a line."
            )
        })
}
