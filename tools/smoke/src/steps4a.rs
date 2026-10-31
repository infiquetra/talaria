//! Read-only configuration screen exercises.

use std::time::Duration;

use crate::screen4a::*;
use crate::steps::{Abort, Run, Step};
use crate::terminal::ReadSource;
use crate::wording4a::*;

fn fail(reason: impl Into<String>) -> Result<Step, Abort> {
    Ok(Step::Fail {
        reason: reason.into(),
    })
}

fn read(run: &Run<'_>) -> Result<String, Abort> {
    run.check_stop()?;
    if run.clock.now() >= run.deadline {
        return Err(Abort::Deadline);
    }
    let text = run
        .term
        .read(&run.pane, ReadSource::Visible, None)
        .map_err(|e| Abort::Failed(e.to_string()))?;
    let text = crate::screen4a::plain(&text);
    if write_banner_found(&text) {
        return Err(Abort::Failed(
            "a write banner appeared; the suite must never change anything".into(),
        ));
    }
    Ok(text)
}

fn text(run: &Run<'_>, value: &str) -> Result<(), Abort> {
    run.check_stop()?;
    run.term
        .send_text(&run.pane, value)
        .map_err(|e| Abort::Failed(e.to_string()))?;
    run.clock.sleep(Duration::from_millis(300));
    Ok(())
}

fn key(run: &Run<'_>, value: &str) -> Result<(), Abort> {
    run.check_stop()?;
    run.term
        .send_keys(&run.pane, &[value])
        .map_err(|e| Abort::Failed(e.to_string()))?;
    if value == "esc" {
        run.clock.sleep(Duration::from_millis(400));
    } else {
        run.clock.sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn submit(run: &Run<'_>, value: &str) -> Result<(), Abort> {
    text(run, value)?;
    key(run, "enter")
}

fn wait_for(
    run: &Run<'_>,
    seconds: u64,
    found: impl Fn(&str) -> bool,
) -> Result<Option<String>, Abort> {
    let start = run.clock.now();
    loop {
        let value = read(run)?;
        if found(&value) {
            return Ok(Some(value));
        }
        if run.clock.now().saturating_sub(start) >= run.limit(seconds) {
            return Ok(None);
        }
        run.clock.sleep(Duration::from_millis(200));
    }
}

fn loaded_read(run: &Run<'_>) -> Result<String, Abort> {
    if let Some(value) = wait_for(run, 10, |s| !s.contains("Loading"))? {
        Ok(value)
    } else {
        read(run)
    }
}

fn title(screen: &str, value: &str) -> bool {
    match screen {
        "settings" => settings_title_found(value, "fixture"),
        "providers" => value.contains(&format!("{PROVIDERS_TITLE_FMT}fixture")),
        "model roles" => value.contains(&format!("{MODELS_TITLE_FMT}fixture")),
        "profiles" => value.contains(PROFILES_HEADER),
        "doctor" => doctor_summary(value).is_some(),
        _ => false,
    }
}

fn teardown(run: &Run<'_>, screen: &str, mut value: String) -> Result<Option<String>, Abort> {
    for _ in 0..3 {
        if !title(screen, &value) {
            return Ok(Some(value));
        }
        key(run, "esc")?;
        value = read(run)?;
    }
    if title(screen, &value) {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn same_tail(a: &str, b: &str) -> bool {
    fn tail(s: &str) -> Vec<&str> {
        let mut lines: Vec<_> = s
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .rev()
            .take(3)
            .collect();
        lines.reverse();
        lines
    }
    tail(a) == tail(b)
}

fn open(run: &Run<'_>, command: &str, screen: &str, seconds: u64) -> Result<Option<String>, Abort> {
    submit(run, command)?;
    wait_for(run, seconds, |s| {
        title(screen, s) && !s.contains("Loading") && !s.contains("checking…")
    })
}

/// Runs one configuration screen exercise, including its idle and write guards.
pub(crate) fn screen_step(run: &mut Run<'_>, name: &'static str) -> Result<Step, Abort> {
    // Convert guard failures into ordinary step failures so the caller captures the pane.
    match screen_step_inner(run, name) {
        Err(Abort::Failed(reason)) => fail(reason),
        other => other,
    }
}

fn screen_step_inner(run: &Run<'_>, name: &'static str) -> Result<Step, Abort> {
    let initial = read(run)?;
    if !idle_screen(&initial) {
        return fail("the step did not start from an idle composer");
    }
    let screen = match name {
        "config-browse" | "config-invalid-input" | "config-secret-refused" => "settings",
        "providers-tabs" | "providers-key-masked" => "providers",
        "models-page" | "models-bad-preset" => "model roles",
        "profiles-list" | "profiles-bad-names" => "profiles",
        "doctor-overlay" => "doctor",
        _ => return fail(format!("there is no step named {name}")),
    };
    let mut skip = None;
    let mut equality: Option<(&str, String, String)> = None;
    let mut doctor_closed = true;
    let last = match name {
        "config-browse" => {
            let Some(r1) = open(run, "/config", screen, 3)? else {
                return fail(format!(
                    "settings did not show \"Settings · profile fixture\" within {} s",
                    run.limit_secs(3)
                ));
            };
            let cats = settings_categories(&r1);
            let mut index = 0;
            for category in SETTINGS_CATEGORIES {
                match cats[index..].iter().position(|s| s == category) {
                    Some(pos) => index += pos + 1,
                    None => return fail(format!("settings category {category} missing")),
                }
            }
            key(run, "right")?;
            key(run, "right")?;
            text(run, "/")?;
            text(run, "compact")?;
            let r2 = read(run)?;
            let Some(row) = setting_row(&r2, "display.compact") else {
                return fail("display.compact row missing after the filter \"compact\"");
            };
            if !matches!(row.value.as_str(), "true" | "false")
                || !row
                    .flags
                    .iter()
                    .any(|s| s == SETTINGS_SET || s == SETTINGS_DEFAULT)
                || !row.flags.iter().any(|s| {
                    s.contains(SETTINGS_RESTART_FLAG)
                        || (s.starts_with("may need a restar")
                            && SETTINGS_RESTART_FLAG.starts_with(s.as_str()))
                })
            {
                return fail(
                    "display.compact row lacks a value, set or default, or \"may need a restart\"",
                );
            }
            key(run, "esc")?;
            text(run, "/")?;
            text(run, "api_key")?;
            let r3 = read(run)?;
            let Some(row) = setting_row(&r3, "delegation.api_key") else {
                return fail("delegation.api_key missing after the filter \"api_key\"");
            };
            let value = row.value.trim();
            if value != SETTINGS_NOT_SET
                && !(value.starts_with('•')
                    && value.trim_end_matches('…').chars().all(|c| c == '•'))
            {
                return fail("delegation.api_key shows plain text, not (not set) or bullets");
            }
            key(run, "esc")?;
            key(run, "esc")?;
            let r4 = read(run)?;
            equality = Some((
                "the screen after closing settings differs from the screen before",
                initial.clone(),
                r4.clone(),
            ));
            r4
        }
        "config-invalid-input" => {
            if open(run, "/config", screen, 20)?.is_none() {
                return fail("settings did not open");
            }
            text(run, "/")?;
            text(run, "resume_exchanges")?;
            key(run, "enter")?;
            key(run, "enter")?;
            text(run, "abc")?;
            key(run, "enter")?;
            let r1 = read(run)?;
            if !refusal_found(&r1, SETTINGS_NUMBER_REFUSAL) {
                return fail("no whole-number refusal after \"abc\"");
            }
            if !r1.contains("display.resume_exchanges") {
                return fail("the number editor closed on invalid input");
            }
            key(run, "esc")?;
            let r2 = read(run)?;
            if r2.contains('*') {
                return fail("a \"*\" mark remained after Esc");
            }
            text(run, "a")?;
            let r3 = read(run)?;
            if !refusal_found(&r3, NO_CHANGES_TO_REVIEW) {
                return fail("\"No changes to review.\" missing after the refused edit");
            }
            r3
        }
        "config-secret-refused" => {
            submit(run, "/config delegation.api_key=not-a-real-key")?;
            let r1 = read(run)?;
            if !refusal_found(&r1, SETTINGS_SECRET_REFUSAL) {
                return fail("the refusal text of a secret on the command line is missing");
            }
            if title(screen, &r1) {
                return fail("a settings overlay opened for a secret");
            }
            key(run, "up")?;
            let r2 = read(run)?;
            if r2.contains("not-a-real-key") {
                return fail("Up brought the secret line back");
            }
            key(run, "ctrl+c")?;
            run.clock.sleep(Duration::from_millis(2500));
            let r3 = read(run)?;
            if r3.contains("not-a-real-key") || !idle_screen(&r3) {
                return fail("the composer is not empty after Ctrl+C");
            }
            let _ = read(run)?;
            return Ok(Step::Pass);
        }
        "providers-tabs" => {
            let Some(r1) = open(run, "/providers", screen, 30)? else {
                return fail(format!(
                    "providers did not show \"Providers · profile fixture\" within {} s",
                    run.limit_secs(30)
                ));
            };
            let ollama = providers_row(&r1, PROVIDERS_OLLAMA);
            if !ollama
                .as_ref()
                .is_some_and(|r| r.status.starts_with(PROVIDERS_READY))
            {
                return fail("Ollama Cloud is not ready");
            }
            if !ollama
                .as_ref()
                .and_then(|r| r.key_tail.as_ref())
                .is_some_and(|s| s.chars().count() == 4)
            {
                return fail("Ollama Cloud shows no key tail \"key …\" plus four characters");
            }
            let Some(openrouter) = providers_row(&r1, PROVIDERS_OPENROUTER) else {
                return fail("the OpenRouter row is missing after the filter \"openrouter\"");
            };
            if !openrouter.status.contains(PROVIDERS_NO_KEY) {
                if openrouter.key_tail.is_some() {
                    skip = Some(
                        "OpenRouter already has a key on profile fixture (operator precondition: no OpenRouter key)",
                    );
                } else {
                    return fail("the OpenRouter row is missing after the filter \"openrouter\"");
                }
            }
            key(run, "tab")?;
            let r2 = loaded_read(run)?;
            for line in crate::screen2a::normalised(&r2)
                .iter()
                .skip_while(|l| !l.contains("Accounts"))
                .skip(1)
                .filter(|l| !l.is_empty() && !l.starts_with("enter "))
            {
                if !line.contains(PROVIDERS_SIGNED_IN) && !line.contains(PROVIDERS_NOT_SIGNED_IN) {
                    return fail(format!(
                        "the Accounts tab shows no sign-in state on row {}",
                        line.trim_start_matches(['›', ' '])
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                    ));
                }
            }
            key(run, "tab")?;
            let r3 = loaded_read(run)?;
            if !r3.contains(PROVIDERS_ADD_ENDPOINT) {
                return fail("the Endpoints tab has no \"+ Add an endpoint\" row");
            }
            r3
        }
        "providers-key-masked" => {
            if open(run, "/providers", screen, 30)?.is_none() {
                return fail("providers did not open");
            }
            text(run, "openrouter")?;
            let r1 = read(run)?;
            let Some(row) = providers_row(&r1, PROVIDERS_OPENROUTER) else {
                return fail("the OpenRouter row is missing after the filter \"openrouter\"");
            };
            key(run, "enter")?;
            let r2 = read(run)?;
            let card = format!("{PROVIDERS_KEY_CARD_TITLE_FMT}{PROVIDERS_OPENROUTER}");
            if !r2.contains(&card) {
                return fail("the key card for OpenRouter did not open");
            }
            text(run, &format!("sk-or-v1-{}", "0".repeat(64)))?;
            let r3 = read(run)?;
            if r3.contains("sk-or") || r3.contains("0000") {
                return fail("the key card showed the typed key");
            }
            if !r3
                .match_indices(PROVIDERS_KEY_MASK)
                .any(|(pos, _)| !r3[..pos].ends_with('•'))
            {
                return fail("the key card did not show 40 bullets and \"…\"");
            }
            key(run, "esc")?;
            let r4 = read(run)?;
            if r4.contains(&card) {
                return fail("the key card did not close on Esc");
            }
            if providers_row(&r4, PROVIDERS_OPENROUTER).is_none_or(|r| r.status != row.status) {
                return fail("the OpenRouter row changed after a cancelled key card");
            }
            r4
        }
        "models-page" => {
            let Some(r1) = open(run, "/models", screen, 40)? else {
                return fail(format!(
                    "model roles did not show its title within {} s",
                    run.limit_secs(40)
                ));
            };
            text(run, "2")?;
            let r2 = read(run)?;
            text(run, "3")?;
            let r3 = read(run)?;
            text(run, "4")?;
            let r4 = read(run)?;
            text(run, "5")?;
            let r5 = read(run)?;
            for (section, value) in [
                ("Main", &r1),
                ("Fallbacks", &r2),
                ("Helpers", &r3),
                ("Subagents", &r4),
                ("Mixture of Agents", &r5),
            ] {
                if models_failed(value) {
                    return fail(format!("section {section} says Failed"));
                }
            }
            let main_lines = crate::screen2a::normalised(&r1);
            let main = main_lines.iter().any(|l| {
                l.trim_start_matches(['>', ' '])
                    .strip_prefix("Model:")
                    .is_some_and(|value| {
                        !value.split(" (via ").next().unwrap_or("").trim().is_empty()
                    })
            }) || main_lines
                .iter()
                .skip_while(|l| l.as_str() != "Main")
                .skip(1)
                .any(|l| !l.is_empty());
            if !main {
                return fail("Main shows no model");
            }
            let helper_lines = crate::screen2a::normalised(&r3);
            let helpers: Vec<_> = helper_lines
                .iter()
                .map(|l| l.trim_start_matches(['>', ' ']))
                .filter(|l| !l.is_empty())
                .skip_while(|l| !l.starts_with(MODELS_HELPERS_FIRST))
                .collect();
            if helpers
                .first()
                .is_none_or(|s| !s.starts_with(MODELS_HELPERS_FIRST))
                || helpers
                    .get(11)
                    .is_none_or(|s| !s.starts_with(MODELS_HELPERS_TWELFTH))
            {
                return fail("Helpers does not list Vision first and Curator twelfth");
            }
            let subagents = r4.to_ascii_lowercase();
            if !subagents.contains("route:")
                || !(subagents.contains("chain mode:")
                    || subagents.contains("fallback: inherits the main chain"))
            {
                return fail("Subagents shows no route or no chain mode");
            }
            if !r5.contains("No presets")
                && !r5.lines().any(|l| {
                    l.to_lowercase().contains("preset") && !l.contains("Mixture of Agents")
                })
            {
                return fail("Mixture of Agents shows neither a preset nor the notice");
            }
            r5
        }
        "models-bad-preset" => {
            if open(run, "/models", screen, 30)?.is_none() {
                return fail("model roles did not open");
            }
            text(run, "5")?;
            key(run, "left")?;
            text(run, "a")?;
            text(run, "uat-test")?;
            key(run, "enter")?;
            key(run, "right")?;
            text(run, "a")?;
            key(run, "esc")?;
            key(run, "ctrl+s")?;
            let r1 = read(run)?;
            if !refusal_found(&r1, MODELS_PRESET_INCOMPLETE_FMT) {
                return fail("the incomplete-preset refusal is missing");
            }
            if r1.contains(MODELS_REVIEW_TITLE_FMT) {
                return fail("a diff was offered for an incomplete preset");
            }
            key(run, "esc")?;
            let r2 = read(run)?;
            if !refusal_found(&r2, MODELS_UNSAVED_PROMPT_FMT) {
                return fail("Esc did not ask to apply, discard or stay");
            }
            text(run, "d")?;
            let r3 = read(run)?;
            if title(screen, &r3) {
                return fail("d did not close the model roles page");
            }
            if open(run, "/models", screen, 30)?.is_none() {
                return fail("model roles did not reopen");
            }
            text(run, "5")?;
            let r4 = read(run)?;
            if r4.contains("uat-test") {
                return fail("a preset \"uat-test\" exists after discard");
            }
            r4
        }
        "profiles-list" => {
            if open(run, "/profiles", screen, 30)?.is_none() {
                return fail("profiles header missing");
            }
            text(run, "/fixture")?;
            let r1 = read(run)?;
            if !r1.contains(PROFILES_HEADER) {
                return fail("profiles header missing");
            }
            let rows = profiles_rows(&r1);
            let row = rows.iter().find(|r| r.contains("fixture"));
            if !row.is_some_and(|s| s.contains(PROFILES_SERVED)) {
                return fail("profile fixture is not served");
            }
            if !row.is_some_and(|s| s.contains("minimax-m3.1-flash · ollama-cloud")) {
                return fail("the fixture row lacks \"minimax-m3.1-flash · ollama-cloud\"");
            }
            key(run, "enter")?;
            let r2 = read(run)?;
            if !r2.contains(PROFILES_DETAIL_LAUNCH) {
                return fail("the detail of fixture lacks \"talaria --profile fixture\"");
            }
            if !r2.contains(PROFILES_DETAIL_SKILLS) {
                return fail("the detail of fixture lacks a \"Skills:\" line");
            }
            key(run, "esc")?;
            key(run, "esc")?;
            let r3 = read(run)?;
            equality = Some(("the list changed after Esc", initial.clone(), r3.clone()));
            r3
        }
        "profiles-bad-names" => {
            let Some(list) = open(run, "/profiles", screen, 20)? else {
                return fail("profiles header missing");
            };
            text(run, "n")?;
            text(run, "Bad Name!")?;
            key(run, "enter")?;
            let r1 = read(run)?;
            if !refusal_found(&r1, PROFILES_NAME_REFUSALS[0]) {
                return fail("name refusal 1 of 3 missing or out of order");
            }
            if !r1.contains("Name:") {
                return fail("focus left the Name field");
            }
            key(run, "ctrl+u")?;
            text(run, "default")?;
            key(run, "enter")?;
            let r2 = read(run)?;
            if !refusal_found(&r2, PROFILES_NAME_REFUSALS[1]) {
                return fail("name refusal 2 of 3 missing or out of order");
            }
            if !r2.contains("Name:") {
                return fail("focus left the Name field");
            }
            key(run, "ctrl+u")?;
            text(run, " Fixture ")?;
            key(run, "enter")?;
            let r3 = read(run)?;
            if !refusal_found(&r3, PROFILES_NAME_REFUSALS[2]) {
                return fail("name refusal 3 of 3 missing or out of order");
            }
            if !r3.contains("Name:") {
                return fail("focus left the Name field");
            }
            key(run, "esc")?;
            let r4 = read(run)?;
            if !profiles_rows(&r4)
                .iter()
                .any(|row| row.contains("fixture") && row.contains(PROFILES_SERVED))
            {
                return fail("the profile list changed after the refused names");
            }
            equality = Some((
                "the profile list changed after the refused names",
                list,
                r4.clone(),
            ));
            r4
        }
        "doctor-overlay" => {
            let Some(_) = open(run, "/doctor", screen, 30)? else {
                return fail(format!(
                    "doctor summary line missing within {} s",
                    run.limit_secs(30)
                ));
            };
            key(run, "down")?;
            key(run, "down")?;
            key(run, "enter")?;
            let r1 = read(run)?;
            let rows = doctor_rows(&r1);
            if rows.len() != 7 {
                return fail(format!("doctor shows {} rows, not 7", rows.len()));
            }
            if r1
                .lines()
                .rfind(|l| !l.trim().is_empty())
                .is_none_or(|l| !l.contains(DOCTOR_HINT))
            {
                return fail("the key hint line is not the last overlay line");
            }
            if !r1.contains(DOCTOR_RUNNING) {
                return fail("\"running:\" detail missing");
            }
            let pin = include_str!("../../../crates/hermes-contract/PINNED");
            let tag = pin
                .lines()
                .find_map(|s| s.strip_prefix("tag="))
                .unwrap_or("");
            let commit = pin
                .lines()
                .find_map(|s| s.strip_prefix("commit="))
                .unwrap_or("");
            let prefix: String = commit.chars().take(8).collect();
            if !r1.contains(&format!("{DOCTOR_BUILT_FOR}{tag} ({prefix})")) {
                return fail("\"built for:\" detail missing");
            }
            key(run, "esc")?;
            let r2 = read(run)?;
            doctor_closed = !title(screen, &r2) && idle_screen(&r2);
            if !title(screen, &r2) && !idle_screen(&r2) {
                return fail("doctor did not close on Esc");
            }
            r2
        }
        _ => unreachable!(),
    };
    let Some(_) = teardown(run, screen, last)? else {
        return fail(format!("the {screen} overlay did not close after 3 Esc"));
    };
    if !doctor_closed {
        return fail("doctor did not close on Esc");
    }
    if let Some((reason, before, after)) = equality {
        let equal = if name == "profiles-bad-names" {
            // Profile details arrive independently and close when a card is cancelled.
            // Compare the complete list, including every row's model and launch hint.
            let header = |text: &str| {
                text.lines()
                    .find(|l| l.starts_with(PROFILES_HEADER))
                    .unwrap_or("")
                    .to_owned()
            };
            header(&before) == header(&after) && profiles_rows(&before) == profiles_rows(&after)
        } else {
            same_tail(&before, &after)
        };
        if !equal {
            return fail(reason);
        }
    }
    let _ = read(run)?;
    if let Some(reason) = skip {
        return Ok(Step::Skip {
            reason: reason.to_owned(),
        });
    }
    Ok(Step::Pass)
}
