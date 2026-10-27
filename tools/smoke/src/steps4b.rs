//! Read-only capability and integration screen exercises.

use crate::screen4b::*;
use crate::steps::{Abort, Run, Step};
use crate::terminal::{PaneId, ReadSource};
use crate::wording4b::*;
use std::time::Duration;

fn error(error: impl std::fmt::Display) -> Abort {
    Abort::Failed(error.to_string())
}
fn fail(reason: impl Into<String>) -> Result<Step, Abort> {
    Ok(Step::Fail {
        reason: reason.into(),
    })
}

fn raw(run: &Run<'_>) -> Result<String, Abort> {
    run.check_stop()?;
    if run.clock.now() >= run.deadline {
        return Err(Abort::Deadline);
    }
    run.term
        .read(&run.pane, ReadSource::Visible, None)
        .map(|text| crate::screen4a::plain(&text))
        .map_err(error)
}
fn guard(text: &str) -> Result<(), Abort> {
    if write_banner_found(text) {
        return Err(Abort::Failed(
            "a write banner appeared; the suite must never change anything".into(),
        ));
    }
    Ok(())
}
fn read(run: &Run<'_>) -> Result<String, Abort> {
    loop {
        let text = raw(run)?;
        guard(&text)?;
        if !text.contains("Reading profile …")
            && !text.lines().any(|line| line.trim().starts_with("Loading "))
        {
            return Ok(text);
        }
        run.clock.sleep(Duration::from_millis(200));
    }
}
fn key(run: &Run<'_>, key: &str) -> Result<(), Abort> {
    run.check_stop()?;
    run.term.send_keys(&run.pane, &[key]).map_err(error)?;
    run.clock
        .sleep(Duration::from_millis(if key == "esc" { 400 } else { 200 }));
    Ok(())
}
fn text(run: &Run<'_>, text: &str) -> Result<(), Abort> {
    run.check_stop()?;
    run.term.send_text(&run.pane, text).map_err(error)?;
    run.clock.sleep(Duration::from_millis(200));
    Ok(())
}
pub(crate) fn submit(run: &Run<'_>, pane: &PaneId, value: &str) -> Result<(), Abort> {
    run.check_stop()?;
    run.term.send_text(pane, value).map_err(error)?;
    run.clock.sleep(Duration::from_millis(300));
    run.check_stop()?;
    run.term.send_keys(pane, &["enter"]).map_err(error)?;
    run.clock.sleep(Duration::from_millis(300));
    Ok(())
}
fn overlay_open(text: &str) -> bool {
    [
        MCP_TITLE_FMT,
        CAPABILITIES_TITLE_FMT,
        MEMORY_TITLE_FMT,
        CRON_TITLE_FMT,
        MESSAGING_TITLE_FMT,
    ]
    .iter()
    .any(|format| {
        let prefix = format.split("<name>").next().unwrap_or(format);
        text.lines().any(|line| line.trim().starts_with(prefix))
    }) || consent_hint_found(text)
}
pub(crate) fn safe_fallback(text: &str, submitted: &str) -> bool {
    !overlay_open(text)
        && !crate::steps::card_words(text)
        && !text.contains(SIGNAL_URL_FIELD)
        && !text.contains(CONFIGURE_SECRET_REFUSAL)
        && !text.contains(REPLY_PREFIX)
        && !text.contains(
            CONNECTORS_CONSENT_FMT
                .split("<p>")
                .next()
                .unwrap_or(CONNECTORS_CONSENT_FMT),
        )
        && text
            .lines()
            .any(|line| line.trim() == format!("{COMPOSER} {submitted}"))
}
fn title_format(screen: &str) -> &'static str {
    match screen {
        "MCP servers" => MCP_TITLE_FMT,
        "capabilities" => CAPABILITIES_TITLE_FMT,
        "memory" => MEMORY_TITLE_FMT,
        "scheduled jobs" => CRON_TITLE_FMT,
        _ => MESSAGING_TITLE_FMT,
    }
}
fn title(screen: &str, text: &str) -> bool {
    overlay_title_found(text, title_format(screen), "fixture")
        || (screen == "connectors" && consent_hint_found(text))
}
fn open(run: &mut Run<'_>, command: &str, screen: &str, seconds: u64) -> Result<String, Abort> {
    submit(run, &run.pane, command)?;
    let start = run.clock.now();
    let mut fallback = false;
    loop {
        let value = match read(run) {
            Err(Abort::Deadline) => {
                return Err(Abort::Failed(format!(
                    "{screen} did not show \"{}\" within {} s",
                    title_format(screen).replace("<name>", "fixture"),
                    run.limit_secs(seconds)
                )));
            }
            other => other?,
        };
        if title(screen, &value) {
            return Ok(value);
        }
        let elapsed = run.clock.now().saturating_sub(start);
        if !fallback
            && elapsed >= Duration::from_secs(3)
            && elapsed <= Duration::from_secs(5)
            && safe_fallback(&value, command)
        {
            key(run, "enter")?;
            fallback = true;
            run.fallbacks += 1;
            run.step_fallbacks += 1;
        }
        if elapsed >= run.limit(seconds) {
            return Err(Abort::Failed(format!(
                "{screen} did not show \"{}\" within {} s",
                title_format(screen).replace("<name>", "fixture"),
                run.limit_secs(seconds)
            )));
        }
        run.clock.sleep(Duration::from_millis(200));
    }
}
fn teardown(run: &Run<'_>, screen: &str, mut value: String) -> Result<String, Abort> {
    for _ in 0..3 {
        if !title(screen, &value) {
            return Ok(value);
        }
        key(run, "esc")?;
        value = read(run)?;
    }
    if title(screen, &value) {
        Err(Abort::Failed(format!(
            "the {screen} overlay did not close after 3 Esc"
        )))
    } else {
        Ok(value)
    }
}
fn plugin_empty(text: &str) -> bool {
    let pattern = regex::escape(&CAPABILITIES_PLUGINS_EMPTY_FMT.replace("<p>", "fixture"))
        .replace("<n>", "[0-9]+");
    regex::Regex::new(&pattern).is_ok_and(|p| p.is_match(text))
}
pub(crate) fn screen_step(run: &mut Run<'_>, name: &'static str) -> Result<Step, Abort> {
    match screen_step_inner(run, name) {
        Err(Abort::Failed(reason)) => fail(reason),
        other => other,
    }
}
fn screen_step_inner(run: &mut Run<'_>, name: &'static str) -> Result<Step, Abort> {
    let initial = raw(run)?;
    if !idle_screen(&initial) {
        return fail("the step did not start from an idle composer");
    }
    guard(&initial)?;
    match name {
        "mcp-browse" => mcp(run, &initial)?,
        "capabilities-browse" => capabilities(run)?,
        "memory-open" => memory(run)?,
        "memory-commands" => memory_commands(run)?,
        "cron-browse" => cron(run)?,
        "messaging-browse" => messaging(run)?,
        "connectors-consent" => connectors(run)?,
        "configure-secret-refused" => configure(run)?,
        _ => return fail(format!("there is no step named {name}")),
    }
    require(
        idle_screen(&read(run)?),
        "the step did not return to an idle composer",
    )?;
    Ok(Step::Pass)
}

fn require(condition: bool, reason: impl Into<String>) -> Result<(), Abort> {
    if condition {
        Ok(())
    } else {
        Err(Abort::Failed(reason.into()))
    }
}

fn mcp(run: &mut Run<'_>, initial: &str) -> Result<(), Abort> {
    let r1 = open(run, "/mcp", "MCP servers", 30)?;
    require(
        !mcp_rows(&r1).is_empty() || r1.contains(&MCP_EMPTY_FMT.replace("<name>", "fixture")),
        "the server list shows neither rows nor the empty text",
    )?;
    key(run, "tab")?;
    let r2 = read(run)?;
    let rows = catalogue_rows(&r2);
    require(
        rows.len() >= 10,
        format!(
            "the Catalogue tab shows {} remote rows, not at least 10",
            rows.len()
        ),
    )?;
    require(
        !catalogue_transport_found(&r2),
        "the Catalogue tab shows \"TransportSpec(\"",
    )?;
    text(run, MCP_FILTER)?;
    let rows = catalogue_rows(&read(run)?);
    require(
        rows.len() == 1 && rows[0].name == MCP_FILTER,
        format!("the filter \"deepwiki\" shows {} rows, not 1", rows.len()),
    )?;
    key(run, "esc")?;
    read(run)?;
    key(run, "esc")?;
    let last = teardown(run, "MCP servers", read(run)?)?;
    require(
        crate::screen2a::normalised(initial) == crate::screen2a::normalised(&last),
        "the screen after closing MCP servers differs from the screen before",
    )
}
fn capabilities(run: &mut Run<'_>) -> Result<(), Abort> {
    let r1 = open(run, "/capabilities", "capabilities", 40)?;
    require(
        !capabilities_categories(&r1).is_empty()
            && capabilities_tab_rows(&r1)
                .iter()
                .any(|r| r.enabled.is_some()),
        "the Skills tab shows no category header or no [x] or [ ] row",
    )?;
    text(run, "3")?;
    require(
        capabilities_tab_rows(&read(run)?)
            .iter()
            .any(|r| r.tools.is_some()),
        "the Toolsets tab shows no \"<n> tools\" row",
    )?;
    text(run, "4")?;
    let r3 = read(run)?;
    require(
        !capabilities_tab_rows(&r3).is_empty() || plugin_empty(&r3),
        "the Plugins tab shows neither rows nor the empty text",
    )?;
    text(run, "a")?;
    require(
        capabilities_tab_rows(&read(run)?)
            .iter()
            .any(|r| r.source.as_deref() == Some(CAPABILITIES_BUNDLED)),
        "the plugin filter \"a\" shows no bundled row",
    )?;
    key(run, "esc")?;
    let last = teardown(run, "capabilities", read(run)?)?;
    require(
        idle_screen(&last),
        "the composer is not idle after closing capabilities",
    )
}
fn memory(run: &mut Run<'_>) -> Result<(), Abort> {
    let r1 = open(run, "/memory-settings", "memory", 30)?;
    let sections = memory_sections(&r1);
    let mut offset = 0;
    for section in MEMORY_SECTIONS {
        let Some(index) = sections[offset..].iter().position(|s| s == section) else {
            return Err(Abort::Failed(format!("memory section {section} missing")));
        };
        offset += index + 1;
    }
    require(
        r1.contains(MEMORY_BUILT_IN),
        "the memory provider row lacks \"built-in only\"",
    )?;
    let size = regex::Regex::new(MEMORY_SIZE_RE).map_err(error)?;
    require(
        MEMORY_FILE_ROWS.iter().all(|name| {
            r1.lines().any(|line| {
                line.contains(name) && (line.contains(MEMORY_EMPTY) || size.is_match(line))
            })
        }),
        "a built-in memory file row shows neither a size nor \"empty\"",
    )?;
    require(
        r1.split_once(MEMORY_SECTIONS[2])
            .map(|(_, rest)| rest.trim_start())
            .into_iter()
            .filter_map(|rest| rest.lines().next())
            .any(|state| {
                MEMORY_CURATOR_STATES
                    .iter()
                    .any(|s| state.split_whitespace().any(|word| word == *s))
            }),
        "the curator row shows none of active, paused, off",
    )?;
    key(run, "esc")?;
    let last = read(run)?;
    let closed = !title("memory", &last);
    teardown(run, "memory", last)?;
    require(closed, "the memory title is still shown after Esc")
}
fn memory_commands(run: &Run<'_>) -> Result<(), Abort> {
    submit(run, &run.pane, "/memory pending")?;
    run.clock.sleep(Duration::from_secs(10));
    let r1 = read(run)?;
    require(
        r1.contains(MEMORY_PENDING_NONE) || r1.contains(MEMORY_PENDING_SOME),
        "\"/memory pending\" printed neither \"No pending memory writes.\" nor \"Pending memory writes (\"",
    )?;
    require(
        idle_screen(&r1),
        "the memory command did not return to an idle composer",
    )?;
    submit(run, &run.pane, "/curator status")?;
    run.clock.sleep(Duration::from_secs(10));
    let r2 = read(run)?;
    require(
        regex::Regex::new(CURATOR_STATUS_RE)
            .map_err(error)?
            .is_match(&r2),
        "\"/curator status\" printed no curator state",
    )?;
    require(
        ![&r1, &r2].into_iter().any(|s| {
            title("memory", s) || s.contains(UNKNOWN_COMMAND) || s.contains(UNKNOWN_COMMAND_PREFIX)
        }),
        "a Talaria memory overlay or an unknown-command text answered a Hermes command",
    )?;
    require(
        idle_screen(&r2),
        "the step did not return to an idle composer",
    )
}
fn cron(run: &mut Run<'_>) -> Result<(), Abort> {
    let r1 = open(run, "/cron", "scheduled jobs", 30)?;
    require(
        !cron_rows(&r1).is_empty() || cron_empty_found(&r1, "fixture"),
        "the job list shows neither rows with a state nor the empty text",
    )?;
    key(run, "esc")?;
    let last = teardown(run, "scheduled jobs", read(run)?)?;
    require(
        idle_screen(&last),
        "the composer is not idle after closing scheduled jobs",
    )
}
fn messaging(run: &mut Run<'_>) -> Result<(), Abort> {
    let r1 = open(run, "/messaging", "messaging", 40)?;
    let rows = platform_rows(&r1);
    require(
        rows.len() >= 10,
        format!(
            "the platform list shows {} rows, not at least 10",
            rows.len()
        ),
    )?;
    text(run, "/signal")?;
    let rows = platform_rows(&read(run)?);
    let count = rows.iter().filter(|r| r.name == SIGNAL_ROW).count();
    require(
        rows.len() == 1 && count == 1,
        format!("the filter \"/signal\" shows {count} Signal rows, not 1"),
    )?;
    key(run, "enter")?;
    let r3 = read(run)?;
    require(
        r3.contains(SIGNAL_URL_FIELD),
        "SIGNAL_HTTP_URL is missing from the Signal card",
    )?;
    match field_state(&r3, SIGNAL_URL_FIELD) {
        None => {
            return Err(Abort::Failed(
                "SIGNAL_HTTP_URL shows neither \"set\" nor \"not set\"".into(),
            ));
        }
        Some(FieldState::Stored(_)) => {
            return Err(Abort::Failed(
                "SIGNAL_HTTP_URL shows stored characters".into(),
            ));
        }
        Some(FieldState::Set | FieldState::NotSet) => {}
    }
    let last = teardown(run, "messaging", r3)?;
    require(
        idle_screen(&last),
        "the composer is not idle after closing messaging",
    )
}
fn connectors(run: &Run<'_>) -> Result<(), Abort> {
    submit(run, &run.pane, "/connectors")?;
    let r1 = read(run)?;
    require(
        refusal_found(&r1, &CONNECTORS_CONSENT_FMT.replace("<p>", "fixture")),
        "the connectors consent text is missing or does not name profile fixture",
    )?;
    require(
        consent_hint_found(&r1),
        "the connectors consent text lacks \"enter continue · esc back\"",
    )?;
    let last = teardown(run, "connectors", r1)?;
    require(
        idle_screen(&last),
        "the composer is not idle after leaving the consent text",
    )
}
fn configure(run: &Run<'_>) -> Result<(), Abort> {
    submit(run, &run.pane, CONFIGURE_SECRET_LINE)?;
    let r1 = read(run)?;
    require(
        refusal_found(&r1, CONFIGURE_SECRET_REFUSAL),
        "the refusal text of a secret in a /configure request is missing",
    )?;
    require(
        !overlay_open(&r1),
        "an overlay opened for a secret in a /configure request",
    )?;
    key(run, "up")?;
    require(
        !read(run)?.contains(CONFIGURE_SECRET_LINE),
        "Up brought the secret line back",
    )?;
    key(run, "ctrl+c")?;
    run.clock.sleep(Duration::from_millis(2500));
    require(
        idle_screen(&read(run)?),
        "the composer is not empty after Ctrl+C",
    )
}
