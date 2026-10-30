//! Background fleet attention: texts, scopes, quiet window, and the shared rules.

#![allow(unreachable_code)]

use std::time::{Duration, Instant};

use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence};
use talaria_core::attention::{
    Activity, AlertKind, AttentionConfig, AttentionTracker, NeedKey, NeedSummary,
};
use talaria_core::fleet::{FleetRow, NeedDetail, NeedId, NeedSlot, Outcome, RowKey, RowState};
use talaria_core::fleet_attention::{
    FleetAttention, FleetAttentionConfig, FleetScope, QUIET_CAP, fleet_alert_text, need_phrase,
    row_label, startup_notice_text,
};
use talaria_core::session::{PromptKey, SessionId};

fn cfg(needs_you: FleetScope, finished: FleetScope) -> FleetAttentionConfig {
    FleetAttentionConfig {
        bell: vec![AlertKind::NeedsYou, AlertKind::Failed],
        desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        done_after: Duration::from_secs(20),
        needs_you,
        finished,
    }
}

fn all_channels() -> AttentionConfig {
    AttentionConfig {
        bell: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        done_after: Duration::from_secs(20),
    }
}

fn key(profile: &str, session_id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn approval(command: &str, description: &str) -> NeedSummary {
    NeedSummary::Approval {
        command: command.to_owned(),
        description: description.to_owned(),
    }
}

fn row(
    key: RowKey,
    title: &str,
    state: RowState,
    outcome: Option<Outcome>,
    foreground: bool,
    subscribed: bool,
) -> FleetRow {
    FleetRow {
        key,
        title: title.to_owned(),
        model: None,
        source: None,
        started_at: None,
        message_count: 0,
        state,
        state_since: 0.0,
        outcome,
        foreground,
        subscribed,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    }
}

fn prompt(key: &RowKey, id: &str) -> NeedId {
    NeedId {
        key: key.clone(),
        slot: NeedSlot::Prompt(id.to_owned()),
    }
}

fn known(summary: NeedSummary) -> NeedDetail {
    NeedDetail::Known(summary)
}

/// Phrase the foreground tracker emits after `needs you: `.
fn tracker_phrase(summary: &NeedSummary, salt: &str) -> String {
    let mut tracker = AttentionTracker::new(all_channels());
    let out = tracker.observe(Activity::NeedOpened {
        key: NeedKey::Prompt(PromptKey {
            session_id: SessionId::new("fixture"),
            prompt_id: format!("phrase-{salt}"),
            execution_generation: 1,
        }),
        summary: summary.clone(),
        restored: false,
        at: Instant::now(),
    });
    let text = out.alert.expect("the tracker admits the first need").text;
    text.trim_start_matches("needs you: ").to_owned()
}

/// Connect and settle so later marks are `Known` and the window is closed.
fn open_settled(attn: &mut FleetAttention, at: Instant) {
    attn.observe_link(true, at);
    attn.observe_settled(at);
}

fn watch_working(attn: &mut FleetAttention, key: &RowKey, at: Instant, subscribed: bool) {
    let seen = row(
        key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        !subscribed,
        subscribed,
    );
    attn.observe_rows(std::slice::from_ref(&seen), at);
}

#[test]
fn ignores_foreground_effects() {
    let base = Instant::now();
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    let at = base + QUIET_CAP;
    let id_key = key("fixture", "sess-fg");
    let id = prompt(&id_key, "p1");
    let detail = known(approval("ls", ""));
    for kind in [AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed] {
        let alert = attn.on_attention(kind, &id_key, Some(&id), Some(&detail), true, None, at);
        assert!(alert.is_none(), "{kind:?} on the foreground row is ignored");
    }
    let background = row(
        id_key.clone(),
        "api-refactor",
        RowState::NeedsYou,
        None,
        false,
        false,
    );
    let again = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&id),
        Some(&detail),
        false,
        Some(&background),
        at + Duration::from_secs(3),
    );
    assert!(
        again.is_some(),
        "a foreground notice does not remember the need id"
    );
}

#[test]
fn needs_you_texts_for_every_kind() {
    let cases = [
        (approval("rm -rf build/", ""), "approval: rm -rf build/"),
        (approval("", ""), "approval"),
        (approval("\n  \nsecond", ""), "approval"),
        (
            NeedSummary::Question("Which branch?\nmore".into()),
            "question: Which branch?",
        ),
        (NeedSummary::Question(String::new()), "question: "),
        (NeedSummary::Sudo, "password for sudo"),
        (NeedSummary::Secret, "a secret"),
        (NeedSummary::VaultCode, "a vault code"),
        (NeedSummary::VaultUnlock, "a password manager unlock"),
    ];
    for (index, (summary, phrase)) in cases.into_iter().enumerate() {
        assert_eq!(tracker_phrase(&summary, &index.to_string()), phrase);
        let text = fleet_alert_text(
            AlertKind::NeedsYou,
            "api-refactor",
            Some(&known(summary)),
            None,
        );
        assert_eq!(
            text.as_deref(),
            Some(format!("api-refactor: needs you: {phrase}")).as_deref()
        );
    }
    let command = "m".repeat(61);
    let summary = approval(&command, "");
    let phrase = tracker_phrase(&summary, "long");
    assert_eq!(phrase.chars().count(), "approval: ".chars().count() + 60);
    assert!(phrase.ends_with('\u{2026}'));
    let text = fleet_alert_text(
        AlertKind::NeedsYou,
        "api-refactor",
        Some(&known(summary)),
        None,
    );
    assert_eq!(
        text.as_deref(),
        Some(format!("api-refactor: needs you: {phrase}")).as_deref()
    );
    assert_eq!(
        fleet_alert_text(
            AlertKind::NeedsYou,
            "api-refactor",
            Some(&NeedDetail::Waiting),
            None
        )
        .as_deref(),
        Some("api-refactor: needs you")
    );
    assert_eq!(
        fleet_alert_text(AlertKind::NeedsYou, "api-refactor", None, None).as_deref(),
        Some("api-refactor: needs you")
    );
}

#[test]
fn need_phrase_matches_foreground_tracker() {
    let samples = [
        approval("rm -rf build/", ""),
        approval("", ""),
        approval("\n  \nsecond", ""),
        NeedSummary::Question("Which branch?\nmore".into()),
        NeedSummary::Question(String::new()),
        NeedSummary::Sudo,
        NeedSummary::Secret,
        NeedSummary::VaultCode,
        NeedSummary::VaultUnlock,
        approval(&"m".repeat(61), ""),
    ];
    for (index, summary) in samples.into_iter().enumerate() {
        assert_eq!(
            need_phrase(&summary),
            tracker_phrase(&summary, &format!("match-{index}"))
        );
    }
}

#[test]
fn labels_title_fallback_profile_prefix_and_cut() {
    let same = key("fixture", "sess-1");
    assert_eq!(row_label(&same, "api-refactor", "fixture"), "api-refactor");

    let other = key("default", "abcd9f3c2a");
    assert_eq!(row_label(&other, "", "fixture"), "default/#9f3c2a");

    let hostile = key("fixture", "abcd9f3c2a");
    assert_eq!(
        row_label(&hostile, "\u{1b}\u{202e}  ", "fixture"),
        "#9f3c2a"
    );

    let long = "t".repeat(40);
    let cut = row_label(&same, &long, "fixture");
    assert_eq!(cut.chars().count(), 32);
    assert!(cut.ends_with('\u{2026}'));
    assert_eq!(cut.chars().take(31).collect::<String>(), "t".repeat(31));

    let wide = "fixture".repeat(3);
    let prefixed = key(&wide, "sess-x");
    assert_eq!(
        row_label(&prefixed, "x", "fixture"),
        "fixturefixt\u{2026}/x"
    );
}

#[test]
fn finished_outcomes_and_threshold() {
    let base = Instant::now();
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    let at = base + QUIET_CAP;
    let id_key = key("fixture", "sess-done");
    watch_working(&mut attn, &id_key, at, true);

    let done = |outcome: Option<Outcome>| {
        row(
            id_key.clone(),
            "api-refactor",
            RowState::Done,
            outcome,
            false,
            true,
        )
    };
    let notice =
        |attn: &mut FleetAttention, when: Instant, outcome: Option<Outcome>, present: bool| {
            let seen = done(outcome);
            attn.on_attention(
                AlertKind::Done,
                &id_key,
                None,
                None,
                false,
                present.then_some(&seen),
                when,
            )
        };

    let late = notice(
        &mut attn,
        at + Duration::from_secs(25),
        Some(Outcome::Completed),
        true,
    );
    assert_eq!(
        late.as_ref().map(|alert| alert.text.as_str()),
        Some("api-refactor: done")
    );

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    watch_working(&mut attn, &id_key, at, true);
    assert!(
        notice(
            &mut attn,
            at + Duration::from_secs(19),
            Some(Outcome::Completed),
            true
        )
        .is_none()
    );

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    watch_working(&mut attn, &id_key, at, true);
    let failed_row = row(
        id_key.clone(),
        "api-refactor",
        RowState::Failed,
        Some(Outcome::Failed),
        false,
        true,
    );
    let failed = attn.on_attention(
        AlertKind::Failed,
        &id_key,
        None,
        None,
        false,
        Some(&failed_row),
        at + Duration::from_secs(1),
    );
    assert_eq!(
        failed.as_ref().map(|alert| alert.text.as_str()),
        Some("api-refactor: failed")
    );

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    watch_working(&mut attn, &id_key, at, true);
    assert!(
        notice(
            &mut attn,
            at + Duration::from_secs(25),
            Some(Outcome::Interrupted),
            true
        )
        .is_none()
    );

    for outcome in [Some(Outcome::Unknown), None, Some(Outcome::Failed)] {
        let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
        open_settled(&mut attn, base);
        watch_working(&mut attn, &id_key, at, true);
        let alert = notice(&mut attn, at + Duration::from_secs(25), outcome, true);
        assert_eq!(
            alert.as_ref().map(|item| item.text.as_str()),
            Some("api-refactor: finished"),
            "{outcome:?}"
        );
    }

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    watch_working(&mut attn, &id_key, at, true);
    let missing = attn.on_attention(
        AlertKind::Done,
        &id_key,
        None,
        None,
        false,
        None,
        at + Duration::from_secs(25),
    );
    let text = missing
        .expect("a known mark still finishes when the row is absent")
        .text;
    assert!(text.ends_with(": finished"), "{text}");
}

#[test]
fn turn_marks_follow_turns() {
    let base = Instant::now();
    let id_key = key("fixture", "sess-mark");

    // (a) A long turn alerts when it finishes.
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::Watched), "fixture".into());
    open_settled(&mut attn, base);
    let start = base + QUIET_CAP;
    for (offset, state) in [
        (0, RowState::Working),
        (5, RowState::NeedsYou),
        (8, RowState::Working),
        (25, RowState::Done),
    ] {
        let seen = row(
            id_key.clone(),
            "api-refactor",
            state,
            Some(Outcome::Completed),
            false,
            true,
        );
        attn.observe_rows(
            std::slice::from_ref(&seen),
            start + Duration::from_secs(offset),
        );
    }
    let finished = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    let alert = attn.on_attention(
        AlertKind::Done,
        &id_key,
        None,
        None,
        false,
        Some(&finished),
        start + Duration::from_millis(25_100),
    );
    assert!(
        alert.is_some(),
        "a turn watched from the start alerts at 25 s"
    );

    // (b) Shorter than the threshold: no done alert.
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    watch_working(&mut attn, &id_key, start, true);
    let early = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &id_key,
            None,
            None,
            false,
            Some(&early),
            start + Duration::from_secs(10),
        )
        .is_none()
    );

    // (c) A dropped notice still clears the mark, so the next short turn stays quiet.
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::Watched), "fixture".into());
    open_settled(&mut attn, base);
    let unseen = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        false,
        false,
    );
    attn.observe_rows(std::slice::from_ref(&unseen), start);
    let dropped = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        false,
    );
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &id_key,
            None,
            None,
            false,
            Some(&dropped),
            start + Duration::from_secs(30),
        )
        .is_none()
    );
    let watched = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        false,
        true,
    );
    attn.observe_rows(
        std::slice::from_ref(&watched),
        start + Duration::from_secs(40),
    );
    let second = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &id_key,
            None,
            None,
            false,
            Some(&second),
            start + Duration::from_secs(42),
        )
        .is_none(),
        "the second turn is measured from its own start"
    );

    // (d) A new turn already owns the mark, so this notice counts as unknown.
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    watch_working(&mut attn, &id_key, start, true);
    let again = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        false,
        true,
    );
    attn.observe_rows(
        std::slice::from_ref(&again),
        start + Duration::from_secs(30),
    );
    let overlap = attn.on_attention(
        AlertKind::Done,
        &id_key,
        None,
        None,
        false,
        Some(&again),
        start + Duration::from_secs(30),
    );
    assert!(overlap.is_some(), "the finished turn's length is unknown");
    let quick = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &id_key,
            None,
            None,
            false,
            Some(&quick),
            start + Duration::from_secs(31),
        )
        .is_none()
    );

    // (e) A turn already running when the link connects alerts when it ends.
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    attn.observe_link(true, base);
    let running = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        false,
        true,
    );
    attn.observe_settled(base + Duration::from_secs(1));
    attn.observe_rows(
        std::slice::from_ref(&running),
        base + Duration::from_secs(1),
    );
    let ended = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    let alert = attn.on_attention(
        AlertKind::Done,
        &id_key,
        None,
        None,
        false,
        Some(&ended),
        base + Duration::from_secs(6),
    );
    assert_eq!(
        alert.as_ref().map(|item| item.text.as_str()),
        Some("api-refactor: done")
    );

    // (f) No mark: done stays quiet, failed still alerts.
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    let idle_at = base + QUIET_CAP;
    let idle = row(
        id_key.clone(),
        "api-refactor",
        RowState::Idle,
        None,
        false,
        true,
    );
    attn.observe_rows(std::slice::from_ref(&idle), idle_at);
    let completed = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    attn.observe_rows(
        std::slice::from_ref(&completed),
        idle_at + Duration::from_secs(2),
    );
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &id_key,
            None,
            None,
            false,
            Some(&completed),
            idle_at + Duration::from_millis(2_100),
        )
        .is_none()
    );
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    attn.observe_rows(std::slice::from_ref(&idle), idle_at);
    let broke = row(
        id_key.clone(),
        "api-refactor",
        RowState::Failed,
        Some(Outcome::Failed),
        false,
        true,
    );
    attn.observe_rows(
        std::slice::from_ref(&broke),
        idle_at + Duration::from_secs(2),
    );
    let alert = attn.on_attention(
        AlertKind::Failed,
        &id_key,
        None,
        None,
        false,
        Some(&broke),
        idle_at + Duration::from_millis(2_100),
    );
    assert_eq!(
        alert.as_ref().map(|item| item.text.as_str()),
        Some("api-refactor: failed")
    );
}

#[test]
fn scopes_all_watched_off() {
    let base = Instant::now();
    let id_key = key("fixture", "sess-scope");
    let other = key("fixture", "sess-other");
    let scopes = [
        (FleetScope::All, FleetScope::All, true),
        (FleetScope::Watched, FleetScope::Watched, false),
        (FleetScope::Off, FleetScope::Off, false),
    ];
    for (needs_you, finished, expect_unwatched) in scopes {
        let mut attn = FleetAttention::new(cfg(needs_you, finished), "fixture".into());
        open_settled(&mut attn, base);
        let start = base + QUIET_CAP;
        watch_working(&mut attn, &id_key, start, true);
        let idle = row(
            other.clone(),
            "other",
            RowState::Working,
            None,
            false,
            false,
        );
        attn.observe_rows(&[idle], start);
        let detail = known(approval("ls", ""));
        let watched_need = prompt(&id_key, "p");
        let other_need = prompt(&other, "q");
        let watched_row = row(
            id_key.clone(),
            "api-refactor",
            RowState::NeedsYou,
            None,
            false,
            true,
        );
        let other_row = row(
            other.clone(),
            "other",
            RowState::NeedsYou,
            None,
            false,
            false,
        );
        let watched_alert = attn.on_attention(
            AlertKind::NeedsYou,
            &id_key,
            Some(&watched_need),
            Some(&detail),
            false,
            Some(&watched_row),
            start + Duration::from_secs(3),
        );
        let other_alert = attn.on_attention(
            AlertKind::NeedsYou,
            &other,
            Some(&other_need),
            Some(&detail),
            false,
            Some(&other_row),
            start + Duration::from_secs(6),
        );
        assert!(watched_alert.is_some() == (needs_you != FleetScope::Off));
        assert_eq!(
            other_alert.is_some(),
            expect_unwatched && needs_you == FleetScope::All
        );
    }
}

#[test]
fn watched_memory_from_foreground_or_subscribed() {
    let base = Instant::now();
    let start = base + QUIET_CAP;
    let id_key = key("fixture", "sess-watch");

    let mut attn = FleetAttention::new(cfg(FleetScope::Off, FleetScope::Watched), "fixture".into());
    open_settled(&mut attn, base);
    let front = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        true,
        false,
    );
    attn.observe_rows(std::slice::from_ref(&front), start);
    let away = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        false,
        false,
    );
    attn.observe_rows(std::slice::from_ref(&away), start + Duration::from_secs(1));
    let done = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        false,
    );
    let alert = attn.on_attention(
        AlertKind::Done,
        &id_key,
        None,
        None,
        false,
        Some(&done),
        start + Duration::from_secs(25),
    );
    assert!(
        alert.is_some(),
        "a session that was foreground stays watched"
    );

    let mut attn = FleetAttention::new(cfg(FleetScope::Off, FleetScope::Watched), "fixture".into());
    open_settled(&mut attn, base);
    let once = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        false,
        true,
    );
    attn.observe_rows(std::slice::from_ref(&once), start);
    let later = row(
        id_key.clone(),
        "api-refactor",
        RowState::Working,
        None,
        false,
        false,
    );
    attn.observe_rows(std::slice::from_ref(&later), start + Duration::from_secs(1));
    let alert = attn.on_attention(
        AlertKind::Done,
        &id_key,
        None,
        None,
        false,
        Some(&done),
        start + Duration::from_secs(25),
    );
    assert!(alert.is_some(), "one subscribed snapshot is enough");

    let mut attn = FleetAttention::new(cfg(FleetScope::Off, FleetScope::Watched), "fixture".into());
    open_settled(&mut attn, base);
    let rows: Vec<FleetRow> = (0..513)
        .map(|index| {
            row(
                key("fixture", &format!("s{index:04}")),
                "t",
                RowState::Working,
                None,
                false,
                true,
            )
        })
        .collect();
    attn.observe_rows(&rows, start);
    let oldest = key("fixture", "s0000");
    let newest = key("fixture", "s0512");
    let old_done = row(
        oldest.clone(),
        "t",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    let new_done = row(
        newest.clone(),
        "t",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &oldest,
            None,
            None,
            false,
            Some(&old_done),
            start + Duration::from_secs(25),
        )
        .is_none(),
        "the oldest watched key was evicted"
    );
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &newest,
            None,
            None,
            false,
            Some(&new_done),
            start + Duration::from_secs(28),
        )
        .is_some()
    );
}

#[test]
fn same_need_id_alerts_once() {
    let base = Instant::now();
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    let start = base + QUIET_CAP;
    let id_key = key("fixture", "sess-once");
    let id = prompt(&id_key, "p1");
    let detail = known(approval("ls", ""));
    let seen = row(
        id_key.clone(),
        "api-refactor",
        RowState::NeedsYou,
        None,
        false,
        false,
    );
    let first = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&id),
        Some(&detail),
        false,
        Some(&seen),
        start,
    );
    let second = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&id),
        Some(&detail),
        false,
        Some(&seen),
        start + Duration::from_secs(3),
    );
    assert!(first.is_some());
    assert!(second.is_none());

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    let quiet = attn.observe_link(true, base).expect("a window");
    let during = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&id),
        Some(&detail),
        false,
        Some(&seen),
        base + Duration::from_secs(1),
    );
    assert!(during.is_none());
    assert!(quiet > base + Duration::from_secs(1));
    let after = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&id),
        Some(&detail),
        false,
        Some(&seen),
        quiet,
    );
    assert!(
        after.is_none(),
        "an id dropped by the window is still remembered"
    );

    let missing = prompt(&id_key, "p2");
    let once = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&missing),
        None,
        false,
        Some(&seen),
        quiet + Duration::from_secs(3),
    );
    let twice = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&missing),
        None,
        false,
        Some(&seen),
        quiet + Duration::from_secs(6),
    );
    assert!(once.is_some());
    assert!(twice.is_none());
}

#[test]
fn quiet_window_after_connect_and_reconnect() {
    let t0 = Instant::now();
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    assert_eq!(attn.observe_link(true, t0), Some(t0 + QUIET_CAP));
    assert_eq!(
        attn.observe_settled(t0 + Duration::from_millis(500)),
        Some(t0 + Duration::from_secs(4))
    );
    let id_key = key("fixture", "sess-quiet");
    let id = prompt(&id_key, "p");
    let detail = known(approval("ls", ""));
    let seen = row(
        id_key.clone(),
        "api-refactor",
        RowState::NeedsYou,
        None,
        false,
        false,
    );
    assert!(
        attn.on_attention(
            AlertKind::NeedsYou,
            &id_key,
            Some(&id),
            Some(&detail),
            false,
            Some(&seen),
            t0 + Duration::from_millis(3_999),
        )
        .is_none()
    );
    let opened = attn.on_attention(
        AlertKind::NeedsYou,
        &id_key,
        Some(&prompt(&id_key, "later")),
        Some(&detail),
        false,
        Some(&seen),
        t0 + Duration::from_secs(4),
    );
    assert!(opened.is_some(), "an alert exactly at the end passes");
    assert_eq!(attn.observe_link(true, t0 + Duration::from_secs(5)), None);

    let t1 = t0 + Duration::from_secs(30);
    attn.observe_link(false, t1);
    let reopened = attn
        .observe_link(true, t1 + Duration::from_secs(1))
        .expect("a reconnect opens a window");
    assert_eq!(reopened, t1 + Duration::from_secs(1) + QUIET_CAP);
    assert!(
        attn.on_attention(
            AlertKind::NeedsYou,
            &id_key,
            Some(&prompt(&id_key, "reconnect")),
            Some(&detail),
            false,
            Some(&seen),
            t1 + Duration::from_secs(2),
        )
        .is_none()
    );
}

#[test]
fn quiet_window_waits_for_first_listing() {
    let base = Instant::now();
    let id_key = key("fixture", "sess-list");
    let detail = known(approval("ls", ""));
    let seen = row(
        id_key.clone(),
        "api-refactor",
        RowState::NeedsYou,
        None,
        false,
        false,
    );
    let poke = |attn: &mut FleetAttention, at: Instant, id: &str| {
        attn.on_attention(
            AlertKind::NeedsYou,
            &id_key,
            Some(&prompt(&id_key, id)),
            Some(&detail),
            false,
            Some(&seen),
            at,
        )
    };

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    attn.observe_link(true, base);
    assert_eq!(
        attn.observe_settled(base + Duration::from_secs(2)),
        Some(base + Duration::from_secs(5))
    );
    assert!(poke(&mut attn, base + Duration::from_millis(4_500), "a").is_none());
    assert!(poke(&mut attn, base + Duration::from_secs(5), "b").is_some());
    assert_eq!(attn.observe_settled(base + Duration::from_secs(6)), None);

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    attn.observe_link(true, base);
    assert!(poke(&mut attn, base + Duration::from_millis(4_500), "c").is_none());
    assert_eq!(
        attn.observe_settled(base + Duration::from_secs(6)),
        Some(base + Duration::from_secs(9))
    );
    assert!(poke(&mut attn, base + Duration::from_secs(9), "d").is_some());

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    attn.observe_link(true, base);
    assert!(poke(&mut attn, base + Duration::from_millis(14_900), "e").is_none());
    assert!(poke(&mut attn, base + Duration::from_secs(15), "f").is_some());

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    attn.observe_link(true, base);
    assert_eq!(
        attn.observe_settled(base + Duration::from_secs(13)),
        Some(base + QUIET_CAP)
    );
}

#[test]
fn startup_notice_once_after_quiet_window() {
    let base = Instant::now();
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    let end = attn.observe_link(true, base).expect("a window");
    let id_key = key("fixture", "sess-start");
    let detail = known(approval("ls", ""));
    let seen = row(
        id_key.clone(),
        "api-refactor",
        RowState::NeedsYou,
        None,
        false,
        false,
    );
    assert!(
        attn.on_attention(
            AlertKind::NeedsYou,
            &id_key,
            Some(&prompt(&id_key, "p")),
            Some(&detail),
            false,
            Some(&seen),
            base + Duration::from_secs(1),
        )
        .is_none()
    );
    assert_eq!(
        attn.on_wake(end - Duration::from_millis(1), 1, Some("Alt+A")),
        None
    );
    assert_eq!(
        attn.on_wake(end, 1, Some("Alt+A")).as_deref(),
        Some("1 background session needs you; press Alt+A to see it.")
    );
    assert_eq!(
        attn.on_wake(end + Duration::from_secs(1), 1, Some("Alt+A")),
        None
    );

    assert_eq!(
        startup_notice_text(3, Some("Alt+A")),
        "3 background sessions need you; press Alt+A to see the first."
    );
    assert_eq!(
        startup_notice_text(1, None),
        "1 background session needs you."
    );
    assert_eq!(
        startup_notice_text(3, None),
        "3 background sessions need you."
    );

    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    let end = attn.observe_link(true, base).expect("a window");
    assert!(
        attn.on_attention(
            AlertKind::NeedsYou,
            &id_key,
            Some(&prompt(&id_key, "q")),
            Some(&detail),
            false,
            Some(&seen),
            base + Duration::from_secs(1),
        )
        .is_none()
    );
    assert_eq!(attn.on_wake(end, 0, Some("Alt+A")), None);
    assert_eq!(
        attn.on_wake(end + Duration::from_secs(1), 1, Some("Alt+A")),
        None
    );
}

#[test]
fn channels_follow_bell_and_desktop_lists() {
    let base = Instant::now();
    let mut attn = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
    open_settled(&mut attn, base);
    let start = base + QUIET_CAP;
    let id_key = key("fixture", "sess-chan");
    watch_working(&mut attn, &id_key, start, true);
    let done = row(
        id_key.clone(),
        "api-refactor",
        RowState::Done,
        Some(Outcome::Completed),
        false,
        true,
    );
    let alert = attn
        .on_attention(
            AlertKind::Done,
            &id_key,
            None,
            None,
            false,
            Some(&done),
            start + Duration::from_secs(25),
        )
        .expect("done is on the desktop list");
    assert!(!alert.bell);
    assert!(alert.desktop);

    let mut quiet = cfg(FleetScope::All, FleetScope::All);
    quiet.bell.clear();
    quiet.desktop.clear();
    let mut attn = FleetAttention::new(quiet, "fixture".into());
    open_settled(&mut attn, base);
    watch_working(&mut attn, &id_key, start, true);
    assert!(
        attn.on_attention(
            AlertKind::Done,
            &id_key,
            None,
            None,
            false,
            Some(&done),
            start + Duration::from_secs(25),
        )
        .is_none()
    );
}

#[derive(Clone, Debug)]
enum Step {
    Rows(Vec<(RowState, bool, bool)>),
    Attention {
        kind: AlertKind,
        which: u8,
        need: Option<u8>,
        foreground: bool,
        detail: bool,
    },
    Link(bool),
    Settled,
}

fn arb_state() -> impl Strategy<Value = RowState> {
    prop_oneof![
        Just(RowState::NeedsYou),
        Just(RowState::Working),
        Just(RowState::Failed),
        Just(RowState::Done),
        Just(RowState::Idle),
    ]
}

fn arb_kind() -> impl Strategy<Value = AlertKind> {
    prop_oneof![
        Just(AlertKind::NeedsYou),
        Just(AlertKind::Failed),
        Just(AlertKind::Done),
    ]
}

fn step_strategy() -> impl Strategy<Value = Step> {
    let rows = proptest::collection::vec((arb_state(), any::<bool>(), any::<bool>()), 5)
        .prop_map(Step::Rows);
    let attention = (
        arb_kind(),
        0u8..5,
        proptest::option::of(0u8..8),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(kind, which, need, foreground, detail)| Step::Attention {
            kind,
            which,
            need,
            foreground,
            detail,
        });
    prop_oneof![
        rows,
        attention,
        any::<bool>().prop_map(Step::Link),
        Just(Step::Settled),
    ]
}

fn proptest_config() -> Config {
    Config {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(proptest_config())]
    #[test]
    fn prop_never_alerts_foreground_or_twice_per_need(
        steps in proptest::collection::vec(step_strategy(), 0..60)
    ) {
        let base = Instant::now();
        let keys: Vec<RowKey> = (0..5).map(|index| key("fixture", &format!("s{index}"))).collect();
        let mut open = FleetAttention::new(cfg(FleetScope::All, FleetScope::All), "fixture".into());
        let mut silent = FleetAttention::new(cfg(FleetScope::Off, FleetScope::Off), "fixture".into());
        let mut seen: Vec<NeedId> = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let at = base + Duration::from_millis(u64::try_from(index).unwrap_or(0) * 50);
            match step {
                Step::Rows(flags) => {
                    let rows: Vec<FleetRow> = flags
                        .iter()
                        .enumerate()
                        .map(|(nth, (state, foreground, subscribed))| {
                            row(
                                keys[nth].clone(),
                                "t",
                                *state,
                                None,
                                *foreground,
                                *subscribed,
                            )
                        })
                        .collect();
                    open.observe_rows(&rows, at);
                    silent.observe_rows(&rows, at);
                }
                Step::Link(connected) => {
                    open.observe_link(*connected, at);
                    silent.observe_link(*connected, at);
                }
                Step::Settled => {
                    open.observe_settled(at);
                    silent.observe_settled(at);
                }
                Step::Attention {
                    kind,
                    which,
                    need,
                    foreground,
                    detail,
                } => {
                    let id_key = &keys[usize::from(*which)];
                    let id = need.map(|slot| prompt(id_key, &format!("n{slot}")));
                    let body = detail.then(|| known(NeedSummary::Sudo));
                    let seen_row = row(
                        id_key.clone(),
                        "t",
                        RowState::Working,
                        Some(Outcome::Completed),
                        false,
                        false,
                    );
                    let alert = open.on_attention(
                        *kind,
                        id_key,
                        id.as_ref(),
                        body.as_ref(),
                        *foreground,
                        Some(&seen_row),
                        at,
                    );
                    let off = silent.on_attention(
                        *kind,
                        id_key,
                        id.as_ref(),
                        body.as_ref(),
                        *foreground,
                        Some(&seen_row),
                        at,
                    );
                    let foreground_silent = !*foreground || alert.is_none();
                    prop_assert!(foreground_silent);
                    let off_silent = off.is_none();
                    prop_assert!(off_silent);
                    if let Some(id) = id.filter(|_| alert.is_some()) {
                        let fresh = !seen.contains(&id);
                        prop_assert!(fresh);
                        seen.push(id);
                    }
                }
            }
        }
    }
}
