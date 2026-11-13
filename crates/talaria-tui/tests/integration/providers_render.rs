//! Providers overlay drawing. The stub writes no cells.

use crate::support;

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
        browse: None,
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

#[test]
fn endpoint_form_shows_each_focused_field_after_a_wrapped_url() {
    let fields = [
        "Name",
        "Base URL",
        "API key",
        "API mode",
        "Model",
        "Context length",
    ];
    for selected in 0..fields.len() {
        let mut shown = base(ProvidersTab::Endpoints);
        let lines = fields
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let value = if index == 1 {
                    format!("https://example.test/{}", "x".repeat(85))
                } else {
                    String::new()
                };
                format!(
                    "{}{label}: {value}",
                    if index == selected { "› " } else { "" }
                )
            })
            .collect();
        shown.card = Some(CardView {
            step: None,
            kind: CardKind::EndpointForm,
            lines,
            footer: "ctrl+s save · esc close".into(),
        });
        let screen = paint(&shown, 0, 60, 12);
        assert!(
            screen.contains(&format!("› {}", fields[selected])),
            "{screen}"
        );
        assert!(screen.contains("ctrl+s save"), "{screen}");
    }
}

#[test]
fn real_endpoint_form_tracks_focus_after_editing_a_long_url() {
    use talaria_core::providers::{EndpointList, ProvidersKey};
    let now = Instant::now();
    let mut state = ProvidersState::new();
    update(
        &mut state,
        ProvidersInput::Open {
            target: OpenTarget::Tab(ProvidersTab::Endpoints),
            profile_name: "fixture".into(),
            connected: true,
        },
        now,
    );
    update(
        &mut state,
        ProvidersInput::EndpointsLoaded(Ok(EndpointList {
            endpoints: vec![],
            current_provider: String::new(),
            current_model: String::new(),
        })),
        now,
    );
    update(&mut state, ProvidersInput::Key(ProvidersKey::Enter), now);
    update(&mut state, ProvidersInput::Key(ProvidersKey::Tab), now);
    for ch in format!("https://example.test/{}", "x".repeat(85)).chars() {
        update(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)), now);
    }
    for label in ["Base URL", "API key", "API mode", "Model", "Context length"] {
        let screen = paint(&view(&state, now), 0, 60, 12);
        assert!(screen.contains(&format!("› {label}")), "{screen}");
        assert!(screen.contains("ctrl+s save"), "{screen}");
        update(&mut state, ProvidersInput::Key(ProvidersKey::Tab), now);
    }
}

fn loaded_keys() -> ProvidersView {
    let mut shown = base(ProvidersTab::Keys);
    shown.rows = vec![
        "Ollama Cloud      OLLAMA_API_KEY        ready · 12 models · key saved".into(),
        "OpenRouter        OPENROUTER_API_KEY    no key".into(),
        "Qwen Cloud        DASHSCOPE_API_KEY     key saved · not ready".into(),
        "also used by Alibaba Cloud (Coding Plan)".into(),
    ];
    shown.footer =
        "enter set key · ctrl+d remove · ctrl+r reload · tab next tab · type to filter".into();
    shown
}

#[test]
fn tabs_snapshots() {
    use talaria_core::providers::browse::{
        BrowseCounts, BrowseKey, BrowseRow, BrowseView, ProviderStatus,
    };
    for tab in [
        ProvidersTab::All,
        ProvidersTab::Keys,
        ProvidersTab::Accounts,
        ProvidersTab::Endpoints,
    ] {
        let mut view = base(tab);
        view.browse = Some(BrowseView {
            rows: vec![BrowseRow {
                key: Some(BrowseKey::Provider("example-00".into())),
                label: "Example provider 00".into(),
                status: Some(ProviderStatus::ListedReady),
                methods: vec![],
                details: vec!["Catalogue lists this provider as authenticated".into()],
            }],
            selected: Some(0),
            counts: BrowseCounts {
                providers: 1,
                complete: true,
            },
            ..Default::default()
        });
        let text = paint(&view, 0, 80, 24);
        for label in [
            "All",
            "Keys",
            "Accounts",
            "Endpoints",
            "Example provider 00",
            "listed ready",
            "F2/Ctrl+E",
        ] {
            assert!(text.contains(label), "{label}: {text}");
        }
    }
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
        step: Some("Step 1 of 3: Paste key".into()),
        kind: CardKind::Key,
        lines: vec![
            "API key for OpenRouter".into(),
            "Saved to profile fixture's key file as OPENROUTER_API_KEY.".into(),
        ],
        footer: "enter save · ctrl+u clear · esc cancel".into(),
    });
    key.secret_focus = true;
    let masked = paint(&key, 41, 160, 48);
    assert!(masked.contains("Step 1 of 3: Paste key"), "{masked}");
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
        step: Some("Step 2 of 3: Approve in your browser".into()),
        kind: CardKind::Waiting,
        lines: vec![
            "1. Open https://example.invalid/device".into(),
            "2. Enter the code ABCD-1234".into(),
            "expires in 01:30".into(),
        ],
        footer: "o open link · c copy code · esc cancel".into(),
    });
    let narrow = paint(&waiting, 0, 80, 24);
    assert!(narrow.contains("Step 2 of 3: Approve in your browser"));
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
        step: Some("Step 1 of 3: Fill in the endpoint".into()),
        kind: CardKind::EndpointForm,
        lines: vec![
            "Name".into(),
            "Base URL".into(),
            "https://example.invalid/v1".into(),
        ],
        footer: "ctrl+t test · ctrl+s save · tab next field · esc close".into(),
    });
    let form_text = paint(&form, 0, 160, 48);
    assert!(form_text.contains("Step 1 of 3: Fill in the endpoint"));
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
            step: None,
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
