//! Pure readers for capability and integration screens.

use crate::wording4b::*;

#[derive(Debug, PartialEq, Eq)]
pub struct McpRow {
    pub name: String,
    pub state: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CatalogueRow {
    pub name: String,
    pub transport: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CapabilityRow {
    pub name: String,
    pub enabled: Option<bool>,
    pub source: Option<String>,
    pub tools: Option<usize>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CronRow {
    pub name: String,
    pub schedule: String,
    pub state: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformRow {
    pub name: String,
    pub state: String,
}
#[derive(Debug, PartialEq, Eq)]
pub enum FieldState {
    Set,
    NotSet,
    Stored(String),
}

fn row(line: &str) -> &str {
    let line = line.trim();
    line.strip_prefix(ROW_SELECTION_MARKS)
        .unwrap_or(line)
        .trim()
}

fn columns(line: &str) -> Vec<&str> {
    line.split("  ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn overlay_title_found(text: &str, title_fmt: &str, profile: &str) -> bool {
    let title = title_fmt.replace("<name>", profile);
    text.lines()
        .any(|line| columns(line.trim()).first() == Some(&title.as_str()))
}

pub fn mcp_rows(text: &str) -> Vec<McpRow> {
    text.lines()
        .filter_map(|line| {
            let c = columns(row(line));
            if c.len() >= 3 && MCP_KINDS.contains(&c[1]) {
                Some(McpRow {
                    name: c[0].into(),
                    state: c[2].into(),
                })
            } else if c.len() == 2
                && MCP_STATE_PREFIXES
                    .iter()
                    .any(|state| c[1].starts_with(*state))
            {
                Some(McpRow {
                    name: c[0].into(),
                    state: c[1].into(),
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn catalogue_rows(text: &str) -> Vec<CatalogueRow> {
    text.lines()
        .filter_map(|line| {
            let c = columns(row(line));
            if c.len() >= 2 && c[1] == MCP_REMOTE {
                Some(CatalogueRow {
                    name: c[0].into(),
                    transport: c[1].into(),
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn catalogue_transport_found(text: &str) -> bool {
    text.contains(MCP_BAD_TRANSPORT)
}

pub fn capabilities_tab_rows(text: &str) -> Vec<CapabilityRow> {
    text.lines()
        .filter_map(|line| {
            let line = row(line);
            let (enabled, rest) = if let Some(s) = line.strip_prefix(CAPABILITIES_ON) {
                (true, s)
            } else {
                (false, line.strip_prefix(CAPABILITIES_OFF)?)
            };
            let c = columns(rest.trim());
            let name = *c.first()?;
            if name.is_empty() {
                return None;
            }
            let tools = c.iter().skip(1).find_map(|s| {
                s.strip_suffix(CAPABILITIES_TOOLS_SUFFIX)
                    .and_then(|n| n.parse().ok())
            });
            let source = c
                .iter()
                .skip(1)
                .find(|s| CAPABILITIES_SOURCES.contains(s))
                .map(|s| (*s).to_string());
            Some(CapabilityRow {
                name: name.into(),
                enabled: Some(enabled),
                source,
                tools,
            })
        })
        .collect()
}

pub fn capabilities_categories(text: &str) -> Vec<String> {
    let mut previous = None;
    let mut categories = Vec::new();
    for line in text.lines() {
        if !capabilities_tab_rows(line).is_empty() {
            if let Some(category) = previous.take() {
                categories.push(category);
            }
        } else {
            let s = line.trim();
            previous = if !s.is_empty() && !s.contains("  ") && !s.starts_with('>') {
                Some(s.to_string())
            } else {
                None
            };
        }
    }
    categories
}

pub fn memory_sections(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            MEMORY_SECTIONS
                .iter()
                .find(|section| {
                    line == **section
                        || line
                            .strip_prefix(**section)
                            .is_some_and(|rest| rest.starts_with("  "))
                })
                .map(|s| (*s).to_string())
        })
        .collect()
}

pub fn cron_rows(text: &str) -> Vec<CronRow> {
    text.lines()
        .filter_map(|line| {
            let c = columns(row(line));
            if c.len() >= 3 && (c[2] == CRON_STATES[1] || c[2].starts_with(CRON_STATES[0])) {
                Some(CronRow {
                    name: c[0].into(),
                    schedule: c[1].into(),
                    state: c[2].into(),
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn cron_empty_found(text: &str, profile: &str) -> bool {
    text.contains(&CRON_EMPTY_FMT.replace("<name>", profile))
}

pub fn platform_rows(text: &str) -> Vec<PlatformRow> {
    text.lines()
        .filter_map(|line| {
            let line = row(line);
            let c = columns(line);
            let c = if c.len() == 1 {
                line.split_once(" · ")
                    .map_or(c, |(name, state)| vec![name, state])
            } else {
                c
            };
            if c.len() == 2
                && PLATFORM_STATES.iter().any(|state| {
                    c[1] == *state
                        || c[1].strip_prefix(*state).is_some_and(|tail| {
                            tail.starts_with(':')
                                || tail.starts_with(" · ")
                                || tail.starts_with(" (")
                        })
                })
            {
                Some(PlatformRow {
                    name: c[0].into(),
                    state: c[1].into(),
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn field_state(text: &str, field: &str) -> Option<FieldState> {
    for line in text.lines() {
        let visible = row(line);
        if let Some((label, state)) = visible.split_once(": ")
            && (label == field || label.contains(&format!("({field})")))
        {
            return match state.trim() {
                FIELD_SET => Some(FieldState::Set),
                FIELD_NOT_SET => Some(FieldState::NotSet),
                value => [FIELD_SET, FIELD_NOT_SET].iter().find_map(|prefix| {
                    value
                        .strip_prefix(&format!("{prefix} "))
                        .map(|stored| FieldState::Stored(stored.trim().into()))
                }),
            };
        }
        let c = columns(line.trim());
        if c.len() < 2 {
            continue;
        }
        let label = c[0];
        if label != field && !label.contains(&format!("({field})")) {
            continue;
        }
        let state = c[1];
        if matches!(state, FIELD_SET | FIELD_NOT_SET) && c.len() > 2 {
            return Some(FieldState::Stored(c[2..].join("  ")));
        }
        if state == FIELD_NOT_SET {
            return Some(FieldState::NotSet);
        }
        if state == FIELD_SET {
            return Some(FieldState::Set);
        }
        for state_prefix in [FIELD_SET, FIELD_NOT_SET] {
            if let Some(value) = state
                .strip_prefix(state_prefix)
                .and_then(|rest| rest.strip_prefix(' '))
            {
                return Some(FieldState::Stored(value.into()));
            }
        }
    }
    None
}

pub fn refusal_found(text: &str, exact: &str) -> bool {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains(&exact.split_whitespace().collect::<Vec<_>>().join(" "))
}
pub fn write_banner_found(text: &str) -> bool {
    WRITE_BANNERS.iter().any(|banner| text.contains(banner))
}
pub fn consent_found(text: &str, profile: &str) -> bool {
    refusal_found(text, &CONNECTORS_CONSENT_FMT.replace("<p>", profile)) && consent_hint_found(text)
}
pub(crate) fn consent_hint_found(text: &str) -> bool {
    text.to_lowercase().contains(CONNECTORS_CONSENT_HINT)
}
pub fn idle_screen(text: &str) -> bool {
    let titles = [
        MCP_TITLE_FMT,
        CAPABILITIES_TITLE_FMT,
        MEMORY_TITLE_FMT,
        CRON_TITLE_FMT,
        MESSAGING_TITLE_FMT,
    ];
    let has_title = text.lines().any(|line| {
        titles.iter().any(|title| {
            line.trim()
                .starts_with(title.split("<name>").next().unwrap_or(title))
        })
    });
    !has_title
        && !consent_hint_found(text)
        && !text.lines().any(|line| line.contains("∴ thinking"))
        && text
            .lines()
            .map(|line| {
                let line = line.trim();
                line.strip_prefix('│')
                    .map(|line| line.strip_suffix('│').unwrap_or(line).trim())
                    .unwrap_or(line)
            })
            .rfind(|line| line.starts_with(COMPOSER))
            .is_some_and(|line| {
                matches!(
                    line.trim(),
                    ">" | "> Type a message. Enter sends, Shift+Enter adds a line."
                )
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_values_in_later_columns_are_never_treated_as_masked() {
        for state in [FIELD_SET, FIELD_NOT_SET] {
            assert_eq!(
                field_state(
                    &format!("{SIGNAL_URL_FIELD}  {state}  unexpected"),
                    SIGNAL_URL_FIELD
                ),
                Some(FieldState::Stored("unexpected".into()))
            );
        }
    }

    #[test]
    fn composer_can_have_a_footer_but_a_draft_is_never_idle() {
        assert!(idle_screen(">\nfooter"));
        assert!(!idle_screen(">\n> draft\nfooter"));
        assert!(!idle_screen("│ > ─ │\nfooter"));
        assert!(!idle_screen("│ > ││\nfooter"));
        assert!(!idle_screen("Enter continue · Esc back\n>"));
    }

    #[test]
    fn colon_fields_keep_trailing_values_visible() {
        for state in [FIELD_SET, FIELD_NOT_SET] {
            assert_eq!(
                field_state(
                    &format!("{SIGNAL_URL_FIELD}: {state}  set"),
                    SIGNAL_URL_FIELD
                ),
                Some(FieldState::Stored("set".into()))
            );
        }
    }

    #[test]
    fn documented_selection_and_narrow_rows_keep_their_identity() {
        assert_eq!(catalogue_rows("› deepwiki  remote")[0].name, MCP_FILTER);
        assert_eq!(
            mcp_rows("› docs  connected · 3 tools")[0].state,
            "connected · 3 tools"
        );
        assert_eq!(
            platform_rows("› Signal  retrying: waiting · 1 waiting to pair")[0].name,
            SIGNAL_ROW
        );
        assert_eq!(platform_rows("Matrix  restart needed").len(), 1);
    }
}
