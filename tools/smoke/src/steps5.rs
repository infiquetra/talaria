//! Workflow screens and commands, with ordered checks and bounded waits.
use crate::steps::{Abort, RECENT_LINES, Run, Step};
use crate::suite5::Phase5Config;
use crate::terminal::{PaneId, ReadSource};
use crate::{screen5, wording5::*};
use rand::RngExt;
use std::{
    fs,
    path::{Component, Path},
    time::Duration,
};

fn error(e: impl std::fmt::Display) -> Abort {
    Abort::Failed(e.to_string())
}
fn require(ok: bool, reason: &str) -> Result<(), Abort> {
    if ok {
        Ok(())
    } else {
        Err(Abort::Failed(reason.into()))
    }
}
fn check(run: &Run<'_>, reason: &str) -> Result<(), Abort> {
    run.check_stop()?;
    require(run.clock.now() < run.deadline, reason)
}
fn read(run: &Run<'_>, pane: &PaneId, source: ReadSource) -> Result<String, Abort> {
    run.check_stop()?;
    let text = run
        .term
        .read(
            pane,
            source,
            (source == ReadSource::RecentUnwrapped).then_some(RECENT_LINES),
        )
        .map_err(error)?;
    run.check_stop()?;
    Ok(crate::screen4a::plain(&text))
}
fn visible(run: &Run<'_>, pane: &PaneId) -> Result<String, Abort> {
    read(run, pane, ReadSource::Visible)
}
fn poll(
    run: &Run<'_>,
    pane: &PaneId,
    reason: &str,
    predicate: impl Fn(&str) -> bool,
) -> Result<String, Abort> {
    loop {
        check(run, reason)?;
        let text = visible(run, pane)?;
        if predicate(&text) {
            return Ok(text);
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn submit(run: &Run<'_>, pane: &PaneId, text: &str) -> Result<(), Abort> {
    check(run, "deadline reached")?;
    run.term.send_text(pane, text).map_err(error)?;
    run.clock.sleep(Duration::from_millis(300));
    check(run, "deadline reached")?;
    run.term.send_keys(pane, &["enter"]).map_err(error)?;
    run.clock.sleep(Duration::from_millis(300));
    run.check_stop()
}
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
fn config_dir(run: &Run<'_>) -> std::path::PathBuf {
    run.scratch.join("config")
}
fn runs_dir(run: &Run<'_>) -> std::path::PathBuf {
    config_dir(run).join("runs")
}
fn env(run: &Run<'_>) -> String {
    format!(
        "TALARIA_CONFIG_DIR={} TALARIA_RUNS_DIR={}",
        quote(&config_dir(run).to_string_lossy()),
        quote(&runs_dir(run).to_string_lossy())
    )
}
fn launch(run: &Run<'_>, resumed: bool) -> String {
    let config = config_dir(run).to_string_lossy().into_owned();
    let runs = runs_dir(run).to_string_lossy().into_owned();
    let args = if resumed {
        vec!["--no-ensure", "-c", "--profile", run.cfg.profile.as_str()]
    } else {
        vec![
            "--no-ensure",
            "--profile",
            run.cfg.profile.as_str(),
            "--model",
            run.cfg.model.as_str(),
            "--provider",
            run.cfg.provider.as_str(),
        ]
    };
    crate::config::launch_line_env(
        &[("TALARIA_CONFIG_DIR", &config), ("TALARIA_RUNS_DIR", &runs)],
        &run.cfg.talaria,
        &args,
    )
}
fn shell(run: &Run<'_>, pane: &PaneId, reason: &str) -> Result<(), Abort> {
    loop {
        check(run, reason)?;
        let foreground = run.term.foreground(pane).map_err(error)?;
        let text = visible(run, pane)?;
        if foreground.len() == 1
            && !foreground[0].contains(&run.cfg.talaria.to_string_lossy().to_string())
            && !text.trim().is_empty()
        {
            return Ok(());
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn startup(run: &mut Run<'_>) -> Result<(), Abort> {
    shell(run, &run.pane, "the composer was not shown")?;
    run.term
        .run(&run.pane, &launch(run, false))
        .map_err(error)?;
    let mut previous = String::new();
    let mut since = run.clock.now();
    loop {
        check(run, "the composer was not shown")?;
        let text = visible(run, &run.pane)?;
        if let Some(code) = crate::screen::exit_code(&text) {
            return Err(error(format!(
                "talaria exited with code {code} before showing its screen"
            )));
        }
        if previous != text {
            previous = text.clone();
            since = run.clock.now();
        }
        let foreground = run.term.foreground(&run.pane).map_err(error)?;
        if screen5::composer_found(&text)
            && foreground
                .iter()
                .any(|line| line.contains(&run.cfg.talaria.to_string_lossy().to_string()))
            && run.clock.now().saturating_sub(since) >= Duration::from_secs(1)
        {
            return Ok(());
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}

fn answer(run: &Run<'_>, pane: &PaneId, token: &crate::Token, missing: &str) -> Result<(), Abort> {
    submit(run, pane, &token.prompt())?;
    let mut previous = String::new();
    let mut since = run.clock.now();
    let mut seen = false;
    loop {
        check(
            run,
            if seen {
                "a turn did not settle"
            } else {
                missing
            },
        )?;
        let recent = read(run, pane, ReadSource::RecentUnwrapped)?;
        let screen = visible(run, pane)?;
        seen |= crate::count_token(&recent, &token.join()) > 0;
        if recent != previous {
            previous = recent;
            since = run.clock.now();
        }
        if seen
            && !screen5::turn_active(&screen)
            && run.clock.now().saturating_sub(since) >= Duration::from_secs(3)
        {
            return Ok(());
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn screen_step(run: &Run<'_>, workflows: bool) -> Result<(), Abort> {
    let pane = &run.pane;
    require(
        screen5::idle_screen(&visible(run, pane)?),
        "the step did not start idle",
    )?;
    if workflows {
        submit(run, pane, "/workflows")?;
        poll(run, pane, "the empty-runs text was not shown", |s| {
            screen5::workflows_empty(s)
                && screen5::overlay_title_found(s, WORKFLOWS_TITLE, &run.cfg.profile)
        })?;
    } else {
        submit(run, pane, "/ultracode status")?;
        let text = poll(
            run,
            pane,
            "the status text was not shown",
            screen5::ultracode_status,
        )?;
        require(!screen5::turn_active(&text), "a turn started")?;
        require(!screen5::toggle_on(&text), "the toggle did not stay off")?;
    }
    run.check_stop()?;
    run.term.send_keys(pane, &["esc"]).map_err(error)?;
    for _ in 0..3 {
        run.check_stop()?;
        run.clock.sleep(Duration::from_millis(400));
        let text = visible(run, pane)?;
        if !screen5::overlay_title_found(&text, WORKFLOWS_TITLE, &run.cfg.profile) {
            return require(
                screen5::idle_screen(&visible(run, pane)?),
                "the step did not end idle",
            );
        }
    }
    Err(error("Esc did not return to the idle composer"))
}
fn quit(run: &Run<'_>, pane: &PaneId, reason: &str) -> Result<(), Abort> {
    submit(run, pane, "/quit")?;
    poll(run, pane, reason, |s| {
        crate::screen::exit_code(s) == Some(0)
    })?;
    Ok(())
}
fn resume(run: &mut Run<'_>) -> Result<(), Abort> {
    let pane = run
        .term
        .split_pane(&run.pane, &run.scratch)
        .map_err(error)?;
    run.second = Some(pane.clone());
    shell(run, &pane, "the token was not shown after resume")?;
    run.term.run(&pane, &launch(run, true)).map_err(error)?;
    let old = run
        .tokens
        .first()
        .map(crate::Token::join)
        .unwrap_or_default();
    loop {
        check(run, "the token was not shown after resume")?;
        let screen = visible(run, &pane)?;
        let recent = read(run, &pane, ReadSource::RecentUnwrapped)?;
        if screen5::composer_found(&screen) && crate::count_token(&recent, &old) > 0 {
            break;
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
    let token = run.token();
    answer(run, &pane, &token, "the fresh token did not answer")?;
    quit(run, &pane, "Talaria did not exit on /quit")?;
    shell(run, &pane, "the shell was not shown")
}

// A command must finish in its own output, never in old scrollback.
fn command(run: &mut Run<'_>, suffix: &str, reason: &str) -> Result<String, Abort> {
    command_env(run, suffix, reason, None)
}
fn command_env(
    run: &mut Run<'_>,
    suffix: &str,
    reason: &str,
    home: Option<&Path>,
) -> Result<String, Abort> {
    check(run, reason)?;
    let marker = format!("SMOKE_BEGIN_{:016x}", run.rng.random::<u64>());
    let before = read(run, &run.pane, ReadSource::RecentUnwrapped)?;
    let home = home
        .map(|p| format!("HERMES_HOME={} ", quote(&p.to_string_lossy())))
        .unwrap_or_default();
    let line = format!(
        "printf '\\n{marker}\\n'; {home}{} {} {suffix}; echo \"exit=$?\"; echo SMOKE_EXIT:0",
        env(run),
        quote(&run.cfg.talaria.to_string_lossy())
    );
    run.term.run(&run.pane, &line).map_err(error)?;
    loop {
        check(run, reason)?;
        let after = read(run, &run.pane, ReadSource::RecentUnwrapped)?;
        let Some(delta) = appended(&before, &after) else {
            return Err(error(reason));
        };
        let output = if let Some((_, output)) = delta.split_once(&format!("\n{marker}\n")) {
            output
        } else if delta.lines().next() == Some(marker.as_str()) {
            delta
                .strip_prefix(&marker)
                .unwrap_or(delta)
                .trim_start_matches('\n')
        } else if delta.contains(&marker) {
            run.clock.sleep(crate::screen::POLL_INTERVAL);
            continue;
        } else {
            delta
        };
        if let Some(end) = output.lines().position(|l| {
            l.trim()
                .strip_prefix("SMOKE_EXIT:")
                .is_some_and(|n| n.parse::<i32>().is_ok())
        }) {
            return Ok(output.lines().take(end).collect::<Vec<_>>().join("\n"));
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn appended<'a>(before: &str, after: &'a str) -> Option<&'a str> {
    if before.is_empty() {
        return Some(after);
    }
    after.strip_prefix(before).or_else(|| {
        before.match_indices('\n').find_map(|(i, _)| {
            let suffix = &before[i + 1..];
            if suffix.is_empty() {
                None
            } else {
                after.strip_prefix(suffix)
            }
        })
    })
}
fn exit(text: &str) -> Option<i32> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("exit=")?.parse().ok())
}
fn cli(run: &mut Run<'_>, arguments: &str, reason: &str) -> Result<String, Abort> {
    command(run, arguments, reason)
}
fn builtins(run: &mut Run<'_>) -> Result<(), Abort> {
    let reason = "install did not print the expected lines";
    let first = cli(run, "workflow builtins install", reason)?;
    let valid_files = |text: &str, prefix: &str| {
        BUILTINS.iter().all(|name| {
            text.lines()
                .any(|l| l.trim().starts_with(prefix) && l.trim().ends_with(&format!("/{name}.js")))
        })
    };
    require(
        exit(&first) == Some(0)
            && screen5::created_folder(&first)
            && screen5::created(&first)
            && valid_files(&first, CREATED),
        reason,
    )?;
    let again = cli(run, "workflow builtins install", reason)?;
    require(
        exit(&again) == Some(0)
            && screen5::unchanged(&again)
            && !screen5::created_folder(&again)
            && valid_files(&again, UNCHANGED),
        reason,
    )?;
    let reason = "list did not show the four workflows";
    let list = cli(run, "workflow builtins list", reason)?;
    require(exit(&list) == Some(0) && screen5::installed(&list), reason)?;
    let staffing = config_dir(run).join("missing-tiers.toml");
    fs::write(&staffing,"[tiers.survey]\nmodel='minimax-m3.1-flash'\nprovider='ollama-cloud'\n[tiers.judge]\nmodel='minimax-m3.1-flash'\nprovider='ollama-cloud'\n").map_err(error)?;
    let reason = "check did not refuse the missing tiers";
    let check = cli(
        run,
        &format!(
            "workflow builtins check review-branch --staffing {}",
            quote(&staffing.to_string_lossy())
        ),
        reason,
    )?;
    require(
        exit(&check) == Some(2) && screen5::missing_tiers(&check),
        reason,
    )
}
fn inside(path: &str, parent: &Path) -> bool {
    let path = Path::new(path);
    path.is_absolute()
        && path.starts_with(parent)
        && !path.components().any(|p| p == Component::ParentDir)
}
fn roster(run: &mut Run<'_>) -> Result<(), Abort> {
    let reason = "roster did not exit 0 or parse";
    let file = run.scratch.join("roster.json");
    let output = cli(
        run,
        &format!(
            "workflow roster --profile fixture --json > {}; smoke_status=$?; jq -ec . {}; smoke_jq_status=$?; printf '\\nexit=%s\\njq_exit=%s\\n' \"$smoke_status\" \"$smoke_jq_status\"",
            quote(&file.to_string_lossy()),
            quote(&file.to_string_lossy())
        ),
        reason,
    )?;
    require(exit(&output) == Some(0), reason)?;
    require(
        !output
            .lines()
            .any(|l| l.trim().strip_prefix("jq_exit=").is_some_and(|s| s != "0")),
        reason,
    )?;
    let payload = output
        .lines()
        .filter(|l| !l.trim().starts_with("exit=") && !l.trim().starts_with("jq_exit="))
        .collect::<Vec<_>>()
        .join("\n");
    let value: serde_json::Value = serde_json::from_str(&payload).map_err(|_| error(reason))?;
    require(
        value["version"].is_number()
            && value["report"]["providers"].is_array()
            && value["report"]["tiers"].is_array(),
        reason,
    )?;
    let reason = "a required path field is missing or outside the scratch configuration";
    let parent = config_dir(run);
    require(
        ["talaria_home", "runs_dir", "workflows_dir", "drafts_dir"]
            .iter()
            .all(|key| {
                value["paths"][key]
                    .as_str()
                    .is_some_and(|s| inside(s, &parent))
            }),
        reason,
    )?;
    require(
        value["workflows"].as_array().is_some_and(|rows| {
            !rows.is_empty()
                && rows
                    .iter()
                    .all(|r| r["path"].as_str().is_some_and(|s| inside(s, &parent)))
        }),
        reason,
    )
}
fn list(run: &mut Run<'_>) -> Result<(), Abort> {
    let reason = "the empty-list text was not shown";
    let text = cli(run, "workflow list", reason)?;
    require(exit(&text) == Some(0) && screen5::empty_list(&text), reason)?;
    let reason = "the JSON list was not empty";
    let output = cli(run, "workflow list --json", reason)?;
    let json = output
        .lines()
        .filter(|l| !l.trim().starts_with("exit="))
        .collect::<Vec<_>>()
        .join("\n");
    require(
        exit(&output) == Some(0)
            && serde_json::from_str::<serde_json::Value>(&json)
                .is_ok_and(|v| v == serde_json::json!([])),
        reason,
    )
}
fn no_gateway(run: &mut Run<'_>) -> Result<(), Abort> {
    let home = run.scratch.join("empty-home");
    fs::create_dir(&home).map_err(error)?;
    let script = run.scratch.join("one.js");
    fs::write(
        &script,
        include_str!("../tests/fixtures/screens/phase5/one.js"),
    )
    .map_err(error)?;
    let reason = "run did not fail with the gateway line";
    // The temporary home applies only to this one command, before the ordinary suite env.
    let suffix = format!("workflow run {}", quote(&script.to_string_lossy()));
    let original = command_with_home(run, &home, &suffix, reason)?;
    require(
        exit(&original) == Some(1) && original.contains(NO_GATEWAY),
        reason,
    )?;
    require(
        fs::read_dir(runs_dir(run)).map_err(error)?.next().is_none(),
        "run created a run folder",
    )?;
    let text = cli(run, "workflow list", "list did not exit 0")?;
    require(exit(&text) == Some(0), "list did not exit 0")
}
fn command_with_home(
    run: &mut Run<'_>,
    home: &Path,
    suffix: &str,
    reason: &str,
) -> Result<String, Abort> {
    command_env(run, suffix, reason, Some(home))
}

pub(crate) fn step(run: &mut Run<'_>, name: &str, cfg5: &Phase5Config) -> Result<Step, Abort> {
    match name {
        "startup" => {
            require(
                cfg5.profile == run.cfg.profile
                    && cfg5.model == run.cfg.model
                    && cfg5.provider == run.cfg.provider,
                "the workflow configuration differs from preflight",
            )?;
            fs::create_dir_all(runs_dir(run)).map_err(error)?;
            startup(run)?;
        }
        "plain-answer" => {
            let token = run.token();
            answer(run, &run.pane, &token, "the reply did not carry the token")?;
        }
        "workflows-empty" => screen_step(run, true)?,
        "ultracode-status" => screen_step(run, false)?,
        "quit" => {
            quit(run, &run.pane, "Talaria did not exit")?;
            let recent = read(run, &run.pane, ReadSource::RecentUnwrapped)?;
            let token = run
                .tokens
                .first()
                .map(crate::Token::join)
                .unwrap_or_default();
            require(
                crate::count_token(&recent, &token) > 0,
                "the token did not survive",
            )?;
        }
        "resume" => resume(run)?,
        "builtins-cli" => builtins(run)?,
        "roster-cli" => roster(run)?,
        "list-cli" => list(run)?,
        "no-gateway" => no_gateway(run)?,
        _ => return Err(error("unknown workflow smoke step")),
    }
    Ok(Step::Pass)
}
