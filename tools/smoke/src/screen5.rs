//! Pure readers for workflow screens and command output.
use crate::wording5::*;

pub fn overlay_title_found(text: &str, title_fmt: &str, profile: &str) -> bool {
    let title = title_fmt.replace("<name>", profile);
    text.lines().any(|line| {
        line.trim()
            .trim_start_matches(['┌', '╭'])
            .trim_end_matches(['─', '┐', '╮'])
            .trim()
            .split("  ")
            .next()
            == Some(title.as_str())
    })
}
pub fn workflows_empty(text: &str) -> bool {
    text.contains(WORKFLOWS_EMPTY)
}
pub fn ultracode_status(text: &str) -> bool {
    text.contains(ULTRACODE_STATUS)
}
pub fn created_folder(text: &str) -> bool {
    text.lines().any(|l| l.trim().starts_with(CREATED_FOLDER))
}
pub fn created(text: &str) -> bool {
    text.lines()
        .filter(|l| l.trim().starts_with(CREATED) && !l.trim().starts_with(CREATED_FOLDER))
        .count()
        == 4
}
pub fn unchanged(text: &str) -> bool {
    text.lines()
        .filter(|l| l.trim().starts_with(UNCHANGED))
        .count()
        == 4
}
pub fn installed(text: &str) -> bool {
    let mut current = None;
    let mut found = std::collections::BTreeSet::new();
    for line in text.lines().map(str::trim) {
        if let Some(name) = BUILTINS
            .iter()
            .find(|name| line.starts_with(&format!("{name}:")))
        {
            current = Some(*name);
            if line == format!("{name}: {INSTALLED}") {
                found.insert(*name);
            }
        }
        if line == format!("installed: {INSTALLED}")
            && let Some(name) = current
        {
            found.insert(name);
        }
    }
    found.len() == BUILTINS.len()
}
pub fn missing_tiers(text: &str) -> bool {
    text.lines().any(|l| l.trim().starts_with(MISSING_TIERS))
}
pub fn empty_list(text: &str) -> bool {
    text.lines().any(|l| l.trim() == EMPTY_LIST)
}
pub fn idle_screen(text: &str) -> bool {
    !overlay_title_found(text, WORKFLOWS_TITLE, "")
        && !text.lines().any(|line| {
            line.trim_start_matches(['┌', '╭'])
                .starts_with(WORKFLOW_RUN_TITLE)
        })
        && !turn_active(text)
        && text
            .lines()
            .map(composer_line)
            .rfind(|line| line.starts_with(COMPOSER))
            .is_some_and(empty_composer)
}
fn composer_line(line: &str) -> &str {
    line.trim().trim_matches('│').trim()
}
fn empty_composer(line: &str) -> bool {
    line == COMPOSER || line == COMPOSER_HINT || line == COMPOSER_HINT_ALT
}
pub(crate) fn turn_active(text: &str) -> bool {
    text.contains(THINKING)
        || text
            .lines()
            .flat_map(|line| line.split(['│', '·']))
            .any(|part| {
                part.trim().split_once(' ').is_some_and(|(count, state)| {
                    state == WORKING && count.parse::<u64>().is_ok_and(|n| n > 0)
                })
            })
}
pub(crate) fn composer_found(text: &str) -> bool {
    text.lines().map(composer_line).any(empty_composer)
}
pub(crate) fn toggle_on(text: &str) -> bool {
    text.lines().any(|l| {
        l.trim() == ULTRACODE_BADGE
            || l.split(['·', '│'])
                .any(|part| part.trim() == ULTRACODE_BADGE)
    })
}
