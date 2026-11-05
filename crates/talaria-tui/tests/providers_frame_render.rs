mod support;
use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::providers::browse::{
    BrowseCounts, BrowseKey, BrowseRow, BrowseView, ProviderStatus, SetupMethod,
};
use talaria_core::providers::{CardKind, CardView, ProvidersTab, ProvidersView};
use talaria_tui::{
    picker_frame::{PickerLayout, PickerMode},
    providers::{SecretField, draw_providers},
    terminal::{InlineTerminal, RestoreLink},
};

fn header(label: &str) -> BrowseRow {
    BrowseRow {
        key: None,
        label: label.into(),
        status: None,
        methods: vec![],
        details: vec![],
    }
}
fn provider(n: usize, status: ProviderStatus) -> BrowseRow {
    BrowseRow {
        key: Some(BrowseKey::Provider(format!("example-{n:02}"))),
        label: format!("Example provider {n:02}"),
        status: Some(status),
        methods: vec![SetupMethod::Key],
        details: vec![
            format!("Provider: Example provider {n:02}"),
            format!("Status: {}", status.label()),
            "Catalogue authentication; reachability unknown".into(),
            "Methods: API key, account sign-in".into(),
            "F2/Ctrl+E full details".into(),
        ],
    }
}
fn fixture() -> ProvidersView {
    let rows = vec![
        header("Configured"),
        provider(0, ProviderStatus::ListedReady),
        header("Needs setup"),
        provider(1, ProviderStatus::KeySaved),
        provider(2, ProviderStatus::NeedsSetup),
        header("Endpoints"),
        BrowseRow {
            key: Some(BrowseKey::AddEndpoint),
            label: "+ Add an endpoint".into(),
            status: None,
            methods: vec![SetupMethod::Endpoint],
            details: vec![],
        },
        BrowseRow {
            key: Some(BrowseKey::Endpoint("fixture-endpoint".into())),
            label: "Example endpoint".into(),
            status: Some(ProviderStatus::KeySaved),
            methods: vec![SetupMethod::Endpoint],
            details: vec![],
        },
    ];
    ProvidersView {
        title: "Providers · profile fixture".into(),
        tab: ProvidersTab::All,
        rows: vec![
            "Example provider 00 · listed ready".into(),
            "Example provider 01 · key saved".into(),
            "Example provider 02 · needs setup".into(),
        ],
        selected: Some(0),
        filter: String::new(),
        card: None,
        notice: None,
        banner: None,
        secret_focus: false,
        footer:
            "Enter set up · Esc close · F2/Ctrl+E details\nTab view · Ctrl+R reload · Ctrl+U clear"
                .into(),
        browse: Some(BrowseView {
            rows,
            selected: Some(1),
            counts: BrowseCounts {
                providers: 3,
                complete: true,
            },
            ..Default::default()
        }),
    }
}
fn buffer(view: &ProvidersView, w: u16, h: u16, secret_len: usize) -> Buffer {
    let mut b = Buffer::empty(Rect::new(0, 0, w, h));
    draw_providers(view, secret_len, b.area, &mut b);
    b
}
fn row_text(b: &Buffer, y: u16, area: Rect) -> String {
    use unicode_width::UnicodeWidthStr;
    let mut text = String::new();
    let mut x = area.x;
    while x < area.right() {
        let symbol = b[(x, y)].symbol();
        text.push_str(symbol);
        x += u16::try_from(symbol.width().max(1)).unwrap();
    }
    text.trim_end().to_owned()
}
fn text(b: &Buffer) -> String {
    (0..b.area.height)
        .map(|y| row_text(b, y, b.area))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn browse_and_offline_snapshots_at_both_sizes() {
    for (w, h, browse_snapshot, offline_snapshot) in [
        (
            80,
            24,
            include_str!("snapshots/picker/providers-80x24.txt"),
            include_str!("snapshots/picker/providers-offline-80x24.txt"),
        ),
        (
            120,
            40,
            include_str!("snapshots/picker/providers-120x40.txt"),
            include_str!("snapshots/picker/providers-offline-120x40.txt"),
        ),
    ] {
        let mut view = fixture();
        assert_eq!(
            text(&buffer(&view, w, h, 0)) + "\n",
            browse_snapshot,
            "browse {w}x{h}"
        );
        view.banner = Some("Offline · cached rows".into());
        assert_eq!(
            text(&buffer(&view, w, h, 0)) + "\n",
            offline_snapshot,
            "offline {w}x{h}"
        );
    }
}

#[test]
fn provider_frame_uses_the_shared_thresholds_and_regions() {
    for (w, h, mode) in [
        (112, 28, PickerMode::Side),
        (111, 28, PickerMode::Below),
        (112, 27, PickerMode::Below),
        (60, 16, PickerMode::Below),
        (59, 16, PickerMode::Compact),
        (60, 15, PickerMode::Compact),
        (40, 10, PickerMode::Compact),
        (39, 10, PickerMode::SizeMessage),
        (40, 9, PickerMode::SizeMessage),
    ] {
        let layout = PickerLayout::new(Rect::new(0, 0, w, h));
        assert_eq!(layout.mode, mode);
        let mut view = fixture();
        view.filter = "typed query".into();
        let b = buffer(&view, w, h, 0);
        let actual = text(&b);
        if mode == PickerMode::SizeMessage {
            assert!(
                actual.contains("40×10") && actual.contains("Esc"),
                "{actual}"
            );
        } else {
            let filter = (layout.filter.x..layout.filter.right())
                .map(|x| b[(x, layout.filter.y)].symbol())
                .collect::<String>();
            assert!(filter.contains("typed query"), "{w}x{h}: {actual}");
            if mode != PickerMode::Compact {
                let chips = (layout.chips.x..layout.chips.right())
                    .map(|x| b[(x, layout.chips.y)].symbol())
                    .collect::<String>();
                assert!(
                    chips.contains("All")
                        && chips.contains("Keys")
                        && chips.contains("Accounts")
                        && chips.contains("Endpoints")
                );
            }
        }
    }
}

#[test]
fn thirty_three_providers_scroll_and_keep_textual_statuses_in_monochrome() {
    for (w, h) in [(80, 24), (120, 40)] {
        let mut view = fixture();
        let browse = view.browse.as_mut().unwrap();
        browse.rows = (0..33)
            .map(|n| {
                provider(
                    n,
                    match n % 5 {
                        0 => ProviderStatus::ListedReady,
                        1 => ProviderStatus::KeySaved,
                        2 => ProviderStatus::NeedsSetup,
                        3 => ProviderStatus::Checking,
                        _ => ProviderStatus::Unknown,
                    },
                )
            })
            .collect();
        browse.counts.providers = 33;
        browse.selected = Some(0);
        let first = text(&buffer(&view, w, h, 0));
        assert!(
            first.contains("33 providers") && first.contains("More below"),
            "{first}"
        );
        for status in [
            "listed ready",
            "key saved",
            "needs setup",
            "checking",
            "unknown",
        ] {
            assert!(first.contains(status), "{status}: {first}");
        }
        view.browse.as_mut().unwrap().selected = Some(32);
        let last = text(&buffer(&view, w, h, 0));
        assert!(
            last.contains("Example provider 32") && last.contains("More above"),
            "{last}"
        );
    }
}

#[test]
fn a_setup_card_hides_every_browse_row_and_hint() {
    for (w, h) in [(80, 24), (120, 40)] {
        let mut view = fixture();
        view.card = Some(CardView {
            kind: CardKind::Key,
            step: Some("Step 1 of 3: Paste key".into()),
            lines: vec![
                "API key for selected provider".into(),
                "EXAMPLE_API_KEY".into(),
            ],
            footer: "Enter check and save · Esc cancel".into(),
        });
        view.secret_focus = true;
        let actual = text(&buffer(&view, w, h, 0));
        assert!(actual.contains("Step 1 of 3: Paste key"));
        for behind in [
            "Example provider 00",
            "Example provider 01",
            "Example endpoint",
            "F2/Ctrl+E details",
            "Tab view",
        ] {
            assert!(
                !actual.contains(behind),
                "card exposes browse backdrop: {actual}"
            );
        }
        assert!(actual.contains("Esc cancel"));
    }
}

#[test]
fn setup_card_content_snapshots_at_both_sizes() {
    for (w, h, expected) in [
        (
            80,
            24,
            include_str!("snapshots/picker/providers-setup-80x24.txt"),
        ),
        (
            120,
            40,
            include_str!("snapshots/picker/providers-setup-120x40.txt"),
        ),
    ] {
        let mut view = fixture();
        view.secret_focus = true;
        view.card = Some(CardView {
            kind: CardKind::Key,
            step: Some("Step 1 of 3: Paste key".into()),
            lines: vec![
                "API key for selected provider".into(),
                "EXAMPLE_API_KEY".into(),
            ],
            footer: "Enter check and save · Esc cancel".into(),
        });
        let actual = text(&buffer(&view, w, h, 8));
        let mut visible = [
            "Step 1 of 3: Paste key",
            "EXAMPLE_API_KEY",
            "••••••••",
            "Enter check and save · Esc cancel",
        ]
        .into_iter()
        .filter_map(|needle| actual.find(needle).map(|at| (at, needle)))
        .collect::<Vec<_>>();
        visible.sort_by_key(|(at, _)| *at);
        let snapshot = visible
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(snapshot, expected, "{w}x{h}: {actual}");
    }
}

#[test]
fn masked_editor_never_shows_the_secret() {
    let sentinel = "SENTINEL_PROVIDER_SECRET_MUST_STAY_MASKED";
    let mut secret = SecretField::new();
    secret.handle_paste(sentinel);
    let mut view = fixture();
    view.secret_focus = true;
    view.card = Some(CardView {
        kind: CardKind::Key,
        step: Some("Step 1 of 3: Paste key".into()),
        lines: vec!["API key for selected provider".into()],
        footer: "Esc cancel".into(),
    });
    let actual = text(&buffer(&view, 80, 24, secret.len()));
    assert!(!actual.contains(sentinel));
    assert!(actual.contains('•'));
    assert!(actual.contains("Step 1 of 3"));
}

#[test]
fn long_provider_names_keep_the_status_and_full_identity_in_details() {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let mut view = fixture();
    let label = "Example 模型e\u{301}👩‍💻".repeat(12);
    let selected = &mut view.browse.as_mut().unwrap().rows[1];
    selected.label = label.clone();
    selected.details = std::iter::once(format!("Provider: {label}"))
        .chain((0..64).map(|n| format!("Provider warning line {n}")))
        .collect();
    let b = buffer(&view, 80, 24, 0);
    let layout = PickerLayout::new(b.area);
    let line = row_text(&b, layout.list.y + 1, layout.list);
    assert!(line.contains("listed ready") && line.contains('…'));
    assert!(line.width() <= usize::from(layout.list.width));
    let kept = line
        .split("listed ready")
        .next()
        .unwrap()
        .trim_end()
        .strip_prefix("> ")
        .expect("selection marker")
        .strip_suffix('…')
        .unwrap();
    assert!(
        label
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([label.len()])
            .any(|i| &label[..i] == kept),
        "label was split inside a grapheme: {line}"
    );
    view.browse.as_mut().unwrap().details_open = true;
    let first = text(&buffer(&view, 80, 24, 0));
    assert!(first.contains("Provider:"));
    view.browse.as_mut().unwrap().details_scroll = 3;
    let next = text(&buffer(&view, 80, 24, 0));
    assert_ne!(first, next);
    view.browse.as_mut().unwrap().details_scroll = usize::MAX;
    assert!(
        text(&buffer(&view, 80, 24, 0)).contains("Provider warning line 63"),
        "full details must reach the end"
    );
}

#[test]
fn states_table_for_providers() {
    for (state, recovery) in [
        ("Loading providers…", "Esc"),
        ("Refreshing…", "Ctrl+R"),
        ("No providers match \"missing\" in All", "Ctrl+U"),
        ("No configured providers; Enter to set up", "Enter"),
        ("This provider is set up outside Talaria", "Esc"),
        ("Could not load providers", "Ctrl+R"),
        ("Account inventory unknown; key data retained", "Ctrl+R"),
        ("Offline; changes disabled", "Esc"),
        ("Outcome unknown; reloading", "Esc"),
        ("Provider unavailable", "Esc"),
    ] {
        let mut view = fixture();
        view.banner = Some(state.into());
        view.footer = format!("{recovery} recover · F2/Ctrl+E details\nEsc close · Ctrl+R reload");
        let actual = text(&buffer(&view, 120, 40, 0));
        assert!(
            actual.contains(state) && actual.contains(recovery),
            "{state}: {actual}"
        );
    }
}

#[test]
fn hostile_text_in_every_area() {
    let mut view = fixture();
    let hostile = "literal\u{1b}[2J\u{7}\u{202e}end";
    view.title = hostile.into();
    view.filter = hostile.into();
    view.banner = Some(hostile.into());
    for row in &mut view.browse.as_mut().unwrap().rows {
        row.label = hostile.into();
        row.details = vec![hostile.into()];
    }
    let actual = text(&buffer(&view, 120, 40, 0));
    assert!(!actual.contains(['\u{1b}', '\u{7}', '\u{202e}']));
    assert!(actual.contains('␛') && actual.contains('␇') && actual.contains("‹U+202E›"));
    assert!(
        actual.matches("literal").count() >= 5,
        "title, filter, groups, rows and details remain literal: {actual}"
    );
}

#[test]
fn idle_dialog_writes_nothing_and_never_enables_mouse() {
    let view = fixture();
    let sink = support::ByteSink::default();
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        support::probe(false, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .unwrap();
    terminal.open_overlay().unwrap();
    terminal
        .draw_overlay(|a, b| draw_providers(&view, 0, a, b))
        .unwrap();
    let first = sink.take();
    let idle = terminal
        .draw_overlay(|a, b| draw_providers(&view, 0, a, b))
        .unwrap();
    assert!(!idle.drew && idle.bytes == 0 && sink.is_empty());
    let first = String::from_utf8_lossy(&first);
    for mode in ["?1000h", "?1002h", "?1003h", "?1006h"] {
        assert!(!first.contains(mode));
    }
}
