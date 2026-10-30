//! `talaria fleet list [--json] [--profile <name>] [--watch [--for <seconds>]]`.
//!
//! The listing and the watch lines are pure functions of snapshots, so they are tested with a
//! fixed clock; `run` only parses, connects, and prints.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Arg, ArgAction, Command, value_parser};
use serde_json::{Value, json};
use talaria_app::fleet::{
    FleetConfig, FleetLinkState, FleetListError, FleetNotice, FleetService, FleetSnapshot,
    check_fleet_profile, list_once,
};
use talaria_app::side_connection::SideScope;
use talaria_core::attention::{AlertKind, NeedSummary};
use talaria_core::directory::rows::format_age;
use talaria_core::fleet::{
    FleetRow, NeedDetail, NeedId, NeedItem, NeedSlot, Outcome, ProfileStatus, RowKey, RowState,
};
use talaria_tui::history::sanitize;
use unicode_width::UnicodeWidthStr;

/// The `format` of every JSON object this command prints.
pub const FORMAT: &str = "talaria.fleet.v1";
const TITLE_CHARS: usize = 40;
const DETAIL_CHARS: usize = 60;
const OUT_OF_SCOPE: &str = "outside the profile-only view";
const NO_GATEWAY: &str = "talaria fleet: no Hermes gateway is running for this Hermes home. Start one with \"hermes gateway ensure\", or start talaria.";

/// Pad `text` with spaces until its terminal-cell width is `width`.
/// Wide characters count two cells. Combining characters count zero.
pub fn pad_cells(text: &str, width: usize) -> String {
    let cells = UnicodeWidthStr::width(text);
    let mut padded = String::with_capacity(text.len() + width.saturating_sub(cells));
    padded.push_str(text);
    padded.extend(std::iter::repeat_n(' ', width.saturating_sub(cells)));
    padded
}

/// What the command line asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetArgs {
    pub json: bool,
    pub profile: Option<String>,
    pub watch: bool,
    pub for_seconds: Option<u64>,
}

fn command() -> Command {
    Command::new("talaria")
        .bin_name("talaria")
        .subcommand_required(true)
        .subcommand(
            Command::new("fleet")
                .about("Every Hermes session across served profiles, and what waits for you")
                .subcommand_required(true)
                .subcommand(
                    Command::new("list")
                        .about("List live sessions grouped by state")
                        .arg(
                            Arg::new("json")
                                .long("json")
                                .action(ArgAction::SetTrue)
                                .help("Print talaria.fleet.v1 JSON"),
                        )
                        .arg(
                            Arg::new("profile")
                                .long("profile")
                                .value_name("name")
                                .help("Only this profile"),
                        )
                        .arg(
                            Arg::new("watch")
                                .long("watch")
                                .action(ArgAction::SetTrue)
                                .help("Keep printing one line per change"),
                        )
                        .arg(
                            Arg::new("for")
                                .long("for")
                                .value_name("seconds")
                                .requires("watch")
                                .value_parser(value_parser!(u64).range(1..=3600))
                                .help("Stop watching after this many seconds"),
                        ),
                ),
        )
}

/// Parse `fleet ...` (the arguments after the program name).
pub fn parse(args: &[String]) -> Result<FleetArgs, clap::Error> {
    let matches = command()
        .try_get_matches_from(std::iter::once("talaria".to_owned()).chain(args.iter().cloned()))?;
    let list = matches
        .subcommand_matches("fleet")
        .and_then(|fleet| fleet.subcommand_matches("list"));
    let Some(list) = list else {
        return Err(command().error(
            clap::error::ErrorKind::MissingSubcommand,
            "talaria fleet needs a command: list",
        ));
    };
    Ok(FleetArgs {
        json: list.get_flag("json"),
        profile: list.get_one::<String>("profile").cloned(),
        watch: list.get_flag("watch"),
        for_seconds: list.get_one::<u64>("for").copied(),
    })
}

/// Run `talaria fleet ...` and return the exit code.
pub fn run(args: &[String]) -> u8 {
    // The local offset can be read only while the process has one thread.
    let offset = time::UtcOffset::current_local_offset()
        .map(time::UtcOffset::whole_seconds)
        .unwrap_or(0);
    let parsed = match parse(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            use clap::error::ErrorKind;
            let text = error.render().to_string();
            return match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    print!("{text}");
                    0
                }
                _ => {
                    eprint!("{text}");
                    2
                }
            };
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!(
                "talaria fleet: could not attach to the Hermes gateway: the async runtime could not start"
            );
            return 1;
        }
    };
    let home = hermes_client::default_home();
    let mut cfg = user_fleet_config();
    cfg.only_profile = parsed.profile.clone();
    runtime.block_on(async move {
        if parsed.watch {
            watch(&home, cfg, &parsed, offset).await
        } else {
            once(&home, &cfg, &parsed).await
        }
    })
}

/// `[fleet]` timings from the configuration file. `enabled` does not apply to this command.
fn user_fleet_config() -> FleetConfig {
    let loaded = match std::env::var_os("HOME") {
        Some(home) => talaria_app::config::load_user_config(&PathBuf::from(home)),
        None => talaria_app::config::parse(""),
    };
    loaded.config.fleet
}

fn list_error(error: &FleetListError, profile: Option<&str>) -> (String, u8) {
    let name = sanitize(profile.unwrap_or_default()).into_owned();
    match error {
        FleetListError::NoGateway => (NO_GATEWAY.to_owned(), 1),
        FleetListError::Attach(reason) => (
            format!(
                "talaria fleet: could not attach to the Hermes gateway: {}",
                sanitize(reason)
            ),
            1,
        ),
        FleetListError::UnknownProfile { served } => (
            format!(
                "talaria fleet: profile \"{name}\" is not served by this gateway ({served} profiles are served; run without --profile to see them)."
            ),
            2,
        ),
        FleetListError::ParkedProfile { reason } => (
            format!(
                "talaria fleet: profile \"{name}\" is parked by this gateway: {}.",
                sanitize(reason)
            ),
            2,
        ),
    }
}

async fn once(home: &std::path::Path, cfg: &FleetConfig, args: &FleetArgs) -> u8 {
    match list_once(home, cfg).await {
        Ok(snapshot) => {
            let now = now_secs();
            if args.json {
                println!("{}", snapshot_json(&snapshot));
            } else {
                print!("{}", render_text(&snapshot, args.profile.as_deref(), now));
            }
            0
        }
        Err(error) => {
            let (text, code) = list_error(&error, args.profile.as_deref());
            eprintln!("{text}");
            code
        }
    }
}

async fn watch(home: &std::path::Path, cfg: FleetConfig, args: &FleetArgs, offset: i32) -> u8 {
    let profile = match check_fleet_profile(home, &cfg).await {
        Ok(profile) => profile,
        Err(error) => {
            let (text, code) = list_error(&error, args.profile.as_deref());
            eprintln!("{text}");
            return code;
        }
    };
    let (sender, mut notices) = tokio::sync::mpsc::unbounded_channel();
    let handle = FleetService::start(home.to_path_buf(), profile, cfg, move |notice| {
        let _ = sender.send(notice);
    });
    let stop = async {
        match args.for_seconds {
            Some(seconds) => tokio::time::sleep(Duration::from_secs(seconds)).await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(stop);
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    let mut shown: Option<std::sync::Arc<FleetSnapshot>> = None;
    let code = loop {
        tokio::select! {
            () = &mut stop => break 0,
            _ = &mut interrupt => break 130,
            notice = notices.recv() => {
                let Some(notice) = notice else { break 0 };
                let lines = match notice {
                    FleetNotice::Changed(next) => match &shown {
                        None => {
                            if next.link != FleetLinkState::Connected {
                                continue;
                            }
                            let first = if args.json {
                                vec![snapshot_json(&next).to_string()]
                            } else {
                                vec![render_text(&next, args.profile.as_deref(), now_secs())]
                            };
                            shown = Some(next);
                            first
                        }
                        Some(previous) => {
                            let clock = clock_text(now_secs(), offset);
                            let lines = if args.json {
                                watch_json(previous, &next)
                                    .into_iter()
                                    .map(|line| line.to_string())
                                    .collect()
                            } else {
                                watch_lines(previous, &next, &clock)
                            };
                            shown = Some(next);
                            lines
                        }
                    },
                    FleetNotice::Attention { kind, key, .. } => {
                        if args.json && shown.is_some() {
                            vec![attention_json(kind, &key).to_string()]
                        } else {
                            Vec::new()
                        }
                    }
                };
                for line in lines {
                    if line.ends_with('\n') {
                        print!("{line}");
                    } else {
                        println!("{line}");
                    }
                }
            }
        }
    };
    handle.close().await;
    code
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0)
}

/// Local time `HH:MM:SS.mmm` for seconds since 1970 at a UTC offset in seconds.
pub fn clock_text(seconds: f64, offset_seconds: i32) -> String {
    let millis = (seconds * 1000.0).floor() as i64 + i64::from(offset_seconds) * 1000;
    let day = millis.rem_euclid(86_400_000);
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        day / 3_600_000,
        day / 60_000 % 60,
        day / 1000 % 60,
        day % 1000
    )
}

/// Untrusted text with visible stand-ins, cut to `limit` characters with an ellipsis.
fn shown(text: &str, limit: usize) -> String {
    let clean = sanitize(text);
    if clean.chars().count() <= limit {
        return clean.into_owned();
    }
    let mut cut: String = clean.chars().take(limit.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn title_of(title: &str) -> String {
    if title.is_empty() {
        "(untitled)".to_owned()
    } else {
        shown(title, TITLE_CHARS)
    }
}

fn state_word(state: RowState) -> &'static str {
    match state {
        RowState::NeedsYou => "needs you",
        RowState::Working => "working",
        RowState::Failed => "failed",
        RowState::Done => "done",
        RowState::Idle => "idle",
    }
}

fn state_json(state: RowState) -> &'static str {
    match state {
        RowState::NeedsYou => "needs_you",
        RowState::Working => "working",
        RowState::Failed => "failed",
        RowState::Done => "done",
        RowState::Idle => "idle",
    }
}

fn need_text(item: &NeedItem) -> String {
    let text = match &item.detail {
        NeedDetail::Waiting => "waiting for an answer".to_owned(),
        NeedDetail::Known(summary) => match summary {
            NeedSummary::Approval {
                command,
                description,
            } => {
                let shown = if command.is_empty() {
                    description
                } else {
                    command
                };
                format!("approval: {shown}")
            }
            NeedSummary::Question(question) => format!("question: {question}"),
            NeedSummary::Sudo => "sudo password".to_owned(),
            NeedSummary::Secret => "secret".to_owned(),
            NeedSummary::VaultCode => "vault code".to_owned(),
            NeedSummary::VaultUnlock => "vault unlock".to_owned(),
        },
    };
    shown(&text, DETAIL_CHARS)
}

fn row_detail(row: &FleetRow, queue: &[NeedItem]) -> String {
    match row.state {
        RowState::NeedsYou => queue
            .iter()
            .find(|item| item.id.key == row.key)
            .map(need_text)
            .unwrap_or_else(|| "waiting for an answer".to_owned()),
        RowState::Working => "working".to_owned(),
        RowState::Failed => "failed".to_owned(),
        RowState::Done => match row.outcome {
            Some(Outcome::Completed) => "completed".to_owned(),
            Some(Outcome::Interrupted) => "interrupted".to_owned(),
            _ => "finished".to_owned(),
        },
        RowState::Idle => "idle".to_owned(),
    }
}

fn status_reason(status: &ProfileStatus) -> Option<String> {
    match status {
        ProfileStatus::Loading => Some("loading".to_owned()),
        ProfileStatus::Listed { .. } => None,
        ProfileStatus::Failed { reason } | ProfileStatus::Parked { reason } => Some(reason.clone()),
        ProfileStatus::OutOfScope => Some(OUT_OF_SCOPE.to_owned()),
    }
}

/// The listing of one snapshot: groups, then profiles that are not listed.
pub fn render_text(snapshot: &FleetSnapshot, only: Option<&str>, now: f64) -> String {
    let mut out = String::new();
    if let SideScope::ProfileOnly { profile, why } = &snapshot.scope {
        out.push_str(&format!(
            "Showing profile {} only: {}.\n",
            shown(profile, TITLE_CHARS),
            sanitize(why)
        ));
    }
    let cells: Vec<[String; 5]> = snapshot
        .rows
        .iter()
        .map(|row| {
            [
                shown(&row.key.profile, TITLE_CHARS),
                title_of(&row.title),
                row.model
                    .as_deref()
                    .map(|model| shown(model, TITLE_CHARS))
                    .unwrap_or_else(|| "-".to_owned()),
                row_detail(row, &snapshot.queue),
                format_age(Some(row.state_since), now),
            ]
        })
        .collect();
    let mut widths = [0usize; 5];
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(UnicodeWidthStr::width(cell.as_str()));
        }
    }
    for group in [
        RowState::NeedsYou,
        RowState::Working,
        RowState::Failed,
        RowState::Done,
        RowState::Idle,
    ] {
        let members: Vec<usize> = snapshot
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.state == group)
            .map(|(index, _)| index)
            .collect();
        if members.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "{} ({})\n",
            state_word(group).to_uppercase(),
            members.len()
        ));
        for index in members {
            let row = &snapshot.rows[index];
            let line: Vec<String> = cells[index]
                .iter()
                .zip(widths)
                .enumerate()
                .map(|(column, (cell, width))| {
                    if column == 4 {
                        cell.clone()
                    } else {
                        pad_cells(cell, width)
                    }
                })
                .collect();
            let mut text = format!("  {}", line.join("  "));
            for (flag, word) in [
                (row.stale, " (stale)"),
                (row.gone, " (gone)"),
                (row.unlisted, " (hidden)"),
            ] {
                if flag {
                    text.push_str(word);
                }
            }
            out.push_str(text.trim_end());
            out.push('\n');
        }
    }
    if snapshot.rows.is_empty() {
        match only {
            Some(profile) => out.push_str(&format!(
                "No live sessions in profile {}.\n",
                shown(profile, TITLE_CHARS)
            )),
            None => out.push_str("No live sessions in the served profiles.\n"),
        }
    }
    for (profile, status) in &snapshot.profiles {
        if let Some(reason) = status_reason(status) {
            out.push_str(&format!(
                "not listed: {} ({})\n",
                shown(profile, TITLE_CHARS),
                shown(&reason, DETAIL_CHARS)
            ));
        }
    }
    for (profile, status) in &snapshot.profiles {
        if matches!(status, ProfileStatus::Listed { truncated: true }) {
            out.push_str(&format!(
                "truncated: {} (only its 200 newest live sessions are listed; working sessions beyond them are not shown)\n",
                shown(profile, TITLE_CHARS)
            ));
        }
    }
    out
}

fn scope_json(scope: &SideScope) -> Value {
    match scope {
        SideScope::Host { .. } => json!({"host": true, "profile": null, "why": null}),
        SideScope::ProfileOnly { profile, why } => {
            json!({"host": false, "profile": profile, "why": why})
        }
    }
}

fn profile_json(name: &str, status: &ProfileStatus) -> Value {
    let (word, truncated, reason) = match status {
        ProfileStatus::Loading => ("loading", false, None),
        ProfileStatus::Listed { truncated } => ("listed", *truncated, None),
        ProfileStatus::Failed { reason } => ("failed", false, Some(reason.clone())),
        ProfileStatus::Parked { reason } => ("parked", false, Some(reason.clone())),
        ProfileStatus::OutOfScope => ("out_of_scope", false, Some(OUT_OF_SCOPE.to_owned())),
    };
    json!({"name": name, "status": word, "truncated": truncated, "reason": reason})
}

fn row_json(row: &FleetRow) -> Value {
    json!({
        "profile": row.key.profile,
        "session_id": row.key.session_id,
        "title": row.title,
        "model": row.model,
        "source": row.source,
        "state": state_json(row.state),
        "state_since": row.state_since,
        "outcome": row.outcome.map(|outcome| match outcome {
            Outcome::Completed => "completed",
            Outcome::Interrupted => "interrupted",
            Outcome::Failed => "failed",
            Outcome::Unknown => "unknown",
        }),
        "message_count": row.message_count,
        "foreground": row.foreground,
        "stale": row.stale,
        "gone": row.gone,
        "unlisted": row.unlisted,
    })
}

fn slot_json(slot: &NeedSlot) -> (&'static str, Option<&str>) {
    match slot {
        NeedSlot::Prompt(id) => ("prompt", Some(id)),
        NeedSlot::Masked(id) => ("masked", Some(id)),
        NeedSlot::Waiting => ("waiting", None),
        NeedSlot::External(_) => todo!("add external fleet sessions"),
    }
}

fn need_json(item: &NeedItem) -> Value {
    let (slot, prompt_id) = slot_json(&item.id.slot);
    let (kind, summary) = match &item.detail {
        NeedDetail::Waiting => ("waiting", None),
        NeedDetail::Known(summary) => match summary {
            NeedSummary::Approval { command, .. } => ("approval", Some(command.clone())),
            NeedSummary::Question(question) => ("question", Some(question.clone())),
            NeedSummary::Sudo => ("sudo", None),
            NeedSummary::Secret => ("secret", None),
            NeedSummary::VaultCode => ("vault_code", None),
            NeedSummary::VaultUnlock => ("vault_unlock", None),
        },
    };
    json!({
        "profile": item.id.key.profile,
        "session_id": item.id.key.session_id,
        "slot": slot,
        "prompt_id": prompt_id,
        "kind": kind,
        "summary": summary,
        "choices": item.choices,
        "seq": item.seq,
        "since": item.since,
        "exact": item.exact,
        "execution_generation": item.execution_generation,
    })
}

/// One `talaria.fleet.v1` snapshot object.
pub fn snapshot_json(snapshot: &FleetSnapshot) -> Value {
    json!({
        "format": FORMAT,
        "kind": "snapshot",
        "as_of": snapshot.as_of,
        "scope": scope_json(&snapshot.scope),
        "profiles": snapshot
            .profiles
            .iter()
            .map(|(name, status)| profile_json(name, status))
            .collect::<Vec<_>>(),
        "rows": snapshot.rows.iter().map(row_json).collect::<Vec<_>>(),
        "needs_you": snapshot.queue.iter().map(need_json).collect::<Vec<_>>(),
        "diagnostics": snapshot.diagnostics,
    })
}

/// One attention line.
pub fn attention_json(kind: AlertKind, key: &RowKey) -> Value {
    let alert = match kind {
        AlertKind::NeedsYou => "needs_you",
        AlertKind::Done => "done",
        AlertKind::Failed => "failed",
    };
    json!({
        "format": FORMAT,
        "kind": "attention",
        "alert": alert,
        "profile": key.profile,
        "session_id": key.session_id,
    })
}

fn link_text(link: &FleetLinkState) -> String {
    match link {
        FleetLinkState::Connecting => "connecting".to_owned(),
        FleetLinkState::Connected => "connected".to_owned(),
        FleetLinkState::Reconnecting { reason } => format!("reconnecting: {}", sanitize(reason)),
        FleetLinkState::Failed { reason } => format!("failed: {}", sanitize(reason)),
    }
}

/// A need that left only because a placeholder took its real identity keeps its `seq`.
fn refined(item: &NeedItem, other: &[NeedItem]) -> bool {
    other
        .iter()
        .any(|held| held.seq == item.seq && held.id != item.id)
}

/// The text lines for the change from `previous` to `next`.
pub fn watch_lines(previous: &FleetSnapshot, next: &FleetSnapshot, clock: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let before: HashMap<&RowKey, &FleetRow> =
        previous.rows.iter().map(|row| (&row.key, row)).collect();
    let titles: HashMap<&RowKey, &FleetRow> = next.rows.iter().map(|row| (&row.key, row)).collect();
    let name = |row: &FleetRow| {
        format!(
            "{} {}",
            shown(&row.key.profile, TITLE_CHARS),
            title_of(&row.title)
        )
    };
    for row in &next.rows {
        match before.get(&row.key) {
            Some(old) if old.state != row.state => lines.push(format!(
                "{clock} {} {} -> {}",
                name(row),
                state_word(old.state),
                state_word(row.state)
            )),
            Some(_) => {}
            None => lines.push(format!(
                "{clock} {} new -> {}",
                name(row),
                state_word(row.state)
            )),
        }
    }
    let old_ids: HashSet<&NeedId> = previous.queue.iter().map(|item| &item.id).collect();
    let new_ids: HashSet<&NeedId> = next.queue.iter().map(|item| &item.id).collect();
    for item in &next.queue {
        if old_ids.contains(&item.id) || refined(item, &previous.queue) {
            continue;
        }
        if let Some(row) = titles.get(&item.id.key) {
            lines.push(format!(
                "{clock} needs you: {} {}",
                name(row),
                need_text(item)
            ));
        }
    }
    for item in &previous.queue {
        if new_ids.contains(&item.id) || refined(item, &next.queue) {
            continue;
        }
        if let Some(row) = titles.get(&item.id.key) {
            lines.push(format!("{clock} cleared: {}", name(row)));
        }
    }
    if previous.link != next.link {
        lines.push(format!("{clock} link: {}", link_text(&next.link)));
    }
    lines
}

fn line(kind: &str, mut fields: Value) -> Value {
    if let Some(object) = fields.as_object_mut() {
        object.insert("format".into(), json!(FORMAT));
        object.insert("kind".into(), json!(kind));
    }
    fields
}

/// The JSON lines for the change from `previous` to `next`.
pub fn watch_json(previous: &FleetSnapshot, next: &FleetSnapshot) -> Vec<Value> {
    let mut lines = Vec::new();
    let before: HashMap<&RowKey, &FleetRow> =
        previous.rows.iter().map(|row| (&row.key, row)).collect();
    let after: HashSet<&RowKey> = next.rows.iter().map(|row| &row.key).collect();
    for row in &next.rows {
        if before
            .get(&row.key)
            .is_none_or(|old| row_json(old) != row_json(row))
        {
            lines.push(line("row", json!({"row": row_json(row)})));
        }
    }
    for row in &previous.rows {
        if !after.contains(&row.key) {
            lines.push(line(
                "row_removed",
                json!({"profile": row.key.profile, "session_id": row.key.session_id}),
            ));
        }
    }
    let old_ids: HashSet<&NeedId> = previous.queue.iter().map(|item| &item.id).collect();
    let new_ids: HashSet<&NeedId> = next.queue.iter().map(|item| &item.id).collect();
    for item in &next.queue {
        if !old_ids.contains(&item.id) {
            lines.push(line("need_opened", json!({"need": need_json(item)})));
        }
    }
    for item in &previous.queue {
        if !new_ids.contains(&item.id) {
            let (slot, prompt_id) = slot_json(&item.id.slot);
            lines.push(line(
                "need_closed",
                json!({
                    "profile": item.id.key.profile,
                    "session_id": item.id.key.session_id,
                    "slot": slot,
                    "prompt_id": prompt_id,
                }),
            ));
        }
    }
    let old_profiles: HashMap<&str, &ProfileStatus> = previous
        .profiles
        .iter()
        .map(|(name, status)| (name.as_str(), status))
        .collect();
    for (name, status) in &next.profiles {
        if old_profiles.get(name.as_str()) != Some(&status) {
            lines.push(line(
                "profile",
                json!({"profile": profile_json(name, status)}),
            ));
        }
    }
    if previous.link != next.link {
        let (word, reason) = match &next.link {
            FleetLinkState::Connecting => ("connecting", None),
            FleetLinkState::Connected => ("connected", None),
            FleetLinkState::Reconnecting { reason } => ("reconnecting", Some(reason.clone())),
            FleetLinkState::Failed { reason } => ("failed", Some(reason.clone())),
        };
        lines.push(line("link", json!({"link": word, "reason": reason})));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_counts_cells() {
        assert_eq!(pad_cells("ab", 4), "ab  ");
        assert_eq!(pad_cells("宽", 4), "宽  ");
        assert_eq!(pad_cells("e\u{301}", 2), "e\u{301} ");
        assert_eq!(pad_cells("long", 2), "long");
    }

    #[test]
    fn clock_is_local_time_with_millis() {
        assert_eq!(clock_text(3_723.456, 0), "01:02:03.456");
        assert_eq!(clock_text(3_723.456, 3_600), "02:02:03.456");
        assert_eq!(clock_text(0.0, -3_600), "23:00:00.000");
    }

    #[test]
    fn values_are_cut_with_an_ellipsis() {
        assert_eq!(shown(&"x".repeat(41), 40).chars().count(), 40);
        assert!(shown(&"x".repeat(41), 40).ends_with('…'));
        assert_eq!(title_of(""), "(untitled)");
    }

    #[test]
    fn usage_rules() {
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert!(parse(&args(&["fleet", "list", "--for", "5"])).is_err());
        assert!(parse(&args(&["fleet", "list", "--watch", "--for", "0"])).is_err());
        assert!(parse(&args(&["fleet", "list", "--watch", "--for", "3601"])).is_err());
        assert_eq!(
            parse(&args(&[
                "fleet",
                "list",
                "--watch",
                "--for",
                "1",
                "--json",
                "--profile",
                "fixture"
            ]))
            .expect("valid"),
            FleetArgs {
                json: true,
                profile: Some("fixture".into()),
                watch: true,
                for_seconds: Some(1),
            }
        );
        assert!(parse(&args(&["fleet"])).is_err());
    }
}
