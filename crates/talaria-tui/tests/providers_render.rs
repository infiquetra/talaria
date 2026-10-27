//! Providers overlay drawing. The stub writes no cells.

mod support;

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use support::Vt;
use talaria_core::providers::{
    AccountStatus, CardKind, CardView, OpenTarget, PollReply, PollStatus, ProvidersInput,
    ProvidersState, ProvidersTab, ProvidersView, SignInStart, next_deadline, update, view,
};

fn paint(shown: &ProvidersView, secret_len: usize, cols: u16, rows: u16) -> String {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    talaria_tui::providers::draw_providers(shown, secret_len, area, &mut buf);
    let mut screen = Vt::new(cols, rows);
    for y in 0..rows {
        let mut line = String::new();
        for x in 0..cols {
            line.push_str(buf[(x, y)].symbol());
        }
        if y + 1 < rows {
            line.push_str("\r\n");
        }
        screen.feed(line.as_bytes());
    }
    screen.rows().join("\n")
}

fn base(tab: ProvidersTab) -> ProvidersView {
    ProvidersView {
        title: "Providers · profile fixture".into(),
        tab,
        rows: Vec::new(),
        filter: String::new(),
        selected: Some(0),
        card: None,
        notice: None,
        banner: None,
        secret_focus: false,
        footer: String::new(),
    }
}

fn loaded_keys() -> ProvidersView {
    let mut shown = base(ProvidersTab::Keys);
    shown.rows = vec![
        "Ollama Cloud      OLLAMA_API_KEY        ready · 12 models · key …wxyz".into(),
        "OpenRouter        OPENROUTER_API_KEY    no key".into(),
        "Qwen Cloud        DASHSCOPE_API_KEY     key …abcd saved · not ready".into(),
        "also used by Alibaba Cloud (Coding Plan)".into(),
    ];
    shown.footer =
        "enter set key · ctrl+d remove · ctrl+r reload · tab next tab · type to filter".into();
    shown
}

#[test]
fn tabs_snapshots() {
    let loaded = paint(&loaded_keys(), 0, 160, 48);
    assert!(
        loaded.contains("[Keys]"),
        "the keys tab is marked: {loaded}"
    );
    assert!(loaded.contains("Accounts"), "{loaded}");
    assert!(loaded.contains("Endpoints"), "{loaded}");
    assert!(loaded.contains("Providers · profile fixture"), "{loaded}");
    assert!(
        loaded.contains("ready · 12 models · key …wxyz"),
        "a ready row is missing: {loaded}"
    );
    assert!(
        loaded.contains("also used by Alibaba Cloud (Coding Plan)"),
        "{loaded}"
    );
    assert!(loaded.contains("enter set key"), "{loaded}");

    let mut loading = base(ProvidersTab::Keys);
    loading.rows = vec!["Loading…".into()];
    loading.selected = None;
    assert!(
        paint(&loading, 0, 160, 48).contains("Loading…"),
        "a loading list is blank"
    );

    let mut failed = base(ProvidersTab::Keys);
    failed.rows =
        vec!["Could not load keys: the gateway refused the request. ctrl+r tries again.".into()];
    assert!(
        paint(&failed, 0, 160, 48).contains("ctrl+r tries again."),
        "a failed list is blank"
    );

    let mut empty = base(ProvidersTab::Endpoints);
    empty.rows = vec![
        "+ Add an endpoint".into(),
        "No custom endpoints in this profile.".into(),
    ];
    empty.footer = "enter edit · ctrl+d delete · ctrl+r reload · tab next tab".into();
    let empty_text = paint(&empty, 0, 160, 48);
    assert!(
        empty_text.contains("No custom endpoints in this profile."),
        "{empty_text}"
    );
    assert!(empty_text.contains("+ Add an endpoint"), "{empty_text}");

    let mut unmatched = base(ProvidersTab::Keys);
    unmatched.filter = "zzz".into();
    unmatched.rows = vec!["No provider matches \"zzz\".".into()];
    unmatched.selected = None;
    assert!(
        paint(&unmatched, 0, 160, 48).contains("No provider matches \"zzz\"."),
        "a filter with no match is blank"
    );

    let mut banner = loaded_keys();
    banner.banner =
        Some("Not connected to Hermes. Changes are off until Talaria reconnects.".into());
    assert!(
        paint(&banner, 0, 160, 48)
            .contains("Not connected to Hermes. Changes are off until Talaria reconnects."),
        "the banner is missing"
    );

    let long = format!("OpenRouter {}", "x".repeat(70));
    assert!(long.chars().count() > 80);
    let mut wide_row = loaded_keys();
    wide_row.rows.push(long.clone());
    let wide = paint(&wide_row, 0, 160, 48);
    let narrow = paint(&wide_row, 0, 80, 24);
    assert!(
        wide.contains(&long),
        "the wide screen drops the row: {wide}"
    );
    assert!(
        narrow.contains('…') && !narrow.contains(&long),
        "the narrow screen does not truncate the middle column: {narrow}"
    );
}

#[test]
fn cards_snapshots() {
    assert_eq!(
        talaria_tui::providers::osc52("ab"),
        "\u{1b}]52;c;YWI=\u{7}",
        "the clipboard sequence is empty"
    );

    let mut key = loaded_keys();
    key.card = Some(CardView {
        kind: CardKind::Key,
        lines: vec![
            "API key for OpenRouter".into(),
            "Saved to profile fixture's key file as OPENROUTER_API_KEY.".into(),
        ],
        footer: "enter save · ctrl+u clear · esc cancel".into(),
    });
    key.secret_focus = true;
    let masked = paint(&key, 41, 160, 48);
    let bullets = "•".repeat(40);
    assert!(
        masked.contains(&format!("{bullets}…")),
        "41 typed characters are not 40 bullets and an ellipsis: {masked}"
    );
    assert!(
        !masked.contains(&"•".repeat(41)),
        "the mask grew past 40 bullets: {masked}"
    );
    assert!(
        masked.contains("enter save · ctrl+u clear · esc cancel"),
        "{masked}"
    );

    let mut waiting = base(ProvidersTab::Accounts);
    waiting.card = Some(CardView {
        kind: CardKind::Waiting,
        lines: vec![
            "1. Open https://example.invalid/device".into(),
            "2. Enter the code ABCD-1234".into(),
            "expires in 01:30".into(),
        ],
        footer: "o open link · c copy code · esc cancel".into(),
    });
    let narrow = paint(&waiting, 0, 80, 24);
    assert!(
        narrow.contains("ABCD-1234"),
        "the sign-in code was truncated: {narrow}"
    );
    assert!(
        narrow.contains("o open link · c copy code · esc cancel"),
        "{narrow}"
    );

    let mut form = base(ProvidersTab::Endpoints);
    form.card = Some(CardView {
        kind: CardKind::EndpointForm,
        lines: vec![
            "Name".into(),
            "Base URL".into(),
            "https://example.invalid/v1".into(),
        ],
        footer: "ctrl+t test · ctrl+s save · tab next field · esc close".into(),
    });
    let form_text = paint(&form, 0, 160, 48);
    assert!(
        form_text.contains("ctrl+t test · ctrl+s save · tab next field · esc close"),
        "{form_text}"
    );

    for (kind, line) in [
        (
            CardKind::RemoveConfirm,
            "Remove the saved key for OpenRouter?",
        ),
        (CardKind::SignOutConfirm, "Sign out of Nous?"),
        (CardKind::DeleteConfirm, "Delete this endpoint?"),
    ] {
        let mut shown = base(ProvidersTab::Keys);
        shown.card = Some(CardView {
            kind,
            lines: vec![line.into()],
            footer: "y confirm · n cancel".into(),
        });
        let text = paint(&shown, 0, 160, 48);
        assert!(text.contains(line), "a confirmation is missing: {text}");
        assert!(text.contains("y confirm · n cancel"), "{text}");
    }
}

fn sign_in() -> SignInStart {
    SignInStart {
        session_id: "sess-1".into(),
        user_code: "ABCD-1234".into(),
        verification_url: "https://example.invalid/device".into(),
        expires_in: Some(90),
        poll_interval: Some(5),
    }
}

#[test]
fn opens_without_waiting_and_idles() {
    let mut state = ProvidersState::new();
    let began = Instant::now();
    let _ = update(
        &mut state,
        ProvidersInput::Open {
            target: OpenTarget::Tab(ProvidersTab::Keys),
            profile_name: "fixture".into(),
            connected: true,
        },
        began,
    );
    let shown = view(&state, began);
    let text = paint(&shown, 0, 160, 48);
    assert!(
        text.contains("Loading…"),
        "opening draws before any reply: {text}"
    );

    let area = Rect::new(0, 0, 160, 48);
    let mut buf = Buffer::empty(area);
    let mut worst = Duration::ZERO;
    for _ in 0..20 {
        let start = Instant::now();
        talaria_tui::providers::draw_providers(&shown, 0, area, &mut buf);
        worst = worst.max(start.elapsed());
    }
    assert!(worst < Duration::from_millis(50), "a draw took {worst:?}");
    assert_eq!(
        next_deadline(&state),
        None,
        "a list with no card has a deadline"
    );

    let _ = update(&mut state, ProvidersInput::EnvLoaded(Ok(Vec::new())), began);
    let _ = update(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(hermes_contract::schemas::ModelOptionsResult {
            model: String::new(),
            provider: String::new(),
            providers: Vec::new(),
        })),
        began,
    );
    let _ = update(
        &mut state,
        ProvidersInput::OauthLoaded(Ok(Vec::new())),
        began,
    );
    let _ = update(
        &mut state,
        ProvidersInput::EndpointsLoaded(Ok(talaria_core::providers::EndpointList {
            endpoints: Vec::new(),
            current_provider: String::new(),
            current_model: String::new(),
        })),
        began,
    );
    assert_eq!(next_deadline(&state), None, "loaded lists arm a deadline");

    let _ = update(
        &mut state,
        ProvidersInput::Key(talaria_core::providers::ProvidersKey::Tab),
        began,
    );
    let account = talaria_core::providers::Account {
        id: "nous".into(),
        name: "Nous".into(),
        device_code: true,
        cli_command: None,
        docs_url: None,
        disconnect_hint: None,
        disconnect_command: None,
        disconnectable: true,
        status: AccountStatus::NotSignedIn,
    };
    let _ = update(
        &mut state,
        ProvidersInput::OauthLoaded(Ok(vec![account])),
        began,
    );
    let _ = update(
        &mut state,
        ProvidersInput::Key(talaria_core::providers::ProvidersKey::Enter),
        began,
    );
    let _ = update(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(sign_in()),
        },
        began,
    );
    let armed = next_deadline(&state).expect("a waiting card arms a deadline");
    assert!(
        armed.saturating_duration_since(began) <= Duration::from_secs(1),
        "the deadline is more than one second ahead"
    );
    let later = began + Duration::from_secs(1);
    let _ = update(&mut state, ProvidersInput::Tick, later);
    let moved = next_deadline(&state).expect("the countdown continues");
    assert!(
        moved.saturating_duration_since(armed) >= Duration::from_millis(900),
        "a tick did not move the deadline"
    );

    let _ = update(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "sess-1".into(),
            result: Ok(PollReply {
                status: PollStatus::Approved,
                error_message: None,
                account_email: None,
                model: None,
                retryable: None,
                retry_after: None,
            }),
        },
        later,
    );
    assert_eq!(
        next_deadline(&state),
        None,
        "an ended card keeps a deadline"
    );
}
