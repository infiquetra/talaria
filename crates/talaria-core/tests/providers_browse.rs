#[path = "support/providers.rs"]
mod support;
use support::{account, catalogue, endpoint, env, list_of, open, provider, screen, step};
use talaria_core::providers::browse::{
    BrowseKey, BrowseRow, BrowseView, ProviderStatus, SetupMethod,
};
use talaria_core::providers::*;

fn key(state: &mut ProvidersState, key: ProvidersKey) -> Vec<ProvidersEffect> {
    step(state, ProvidersInput::Key(key))
}
fn loaded(n: usize) -> ProvidersState {
    let (mut state, _) = open(OpenTarget::Tab(ProvidersTab::All), true);
    let keys = (0..n)
        .filter(|p| *p != 2)
        .map(|p| {
            env(
                &format!("EXAMPLE_{p:02}_API_KEY"),
                &format!("example-{p:02}"),
                &format!("Example provider {p:02}"),
                p == 1,
            )
        })
        .collect();
    step(&mut state, ProvidersInput::EnvLoaded(Ok(keys)));
    let providers = (0..n)
        .map(|p| {
            provider(
                &format!("example-{p:02}"),
                if p == 4 || p == 5 {
                    "Same display name"
                } else {
                    "Example provider"
                },
                p == 0,
                0,
            )
        })
        .collect();
    step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(providers))),
    );
    let accounts = if n > 2 {
        vec![
            account(
                "example-00",
                "Example provider 00",
                true,
                AccountStatus::NotSignedIn,
            ),
            account(
                "example-02",
                "Example provider 02",
                true,
                AccountStatus::SignedIn {
                    source_label: "account available".into(),
                    expires_in_s: None,
                },
            ),
        ]
    } else {
        vec![]
    };
    step(&mut state, ProvidersInput::OauthLoaded(Ok(accounts)));
    let mut saved = endpoint(
        "fixture-endpoint",
        "Example endpoint",
        "https://example.invalid/v1",
        "example-model",
    );
    saved.has_api_key = true;
    step(
        &mut state,
        ProvidersInput::EndpointsLoaded(Ok(list_of(vec![saved]))),
    );
    state
}
fn browse(state: &ProvidersState) -> BrowseView {
    screen(state).browse.expect("provider browse projection")
}
fn row<'a>(view: &'a BrowseView, slug: &str) -> &'a BrowseRow {
    view.rows
        .iter()
        .find(|r| r.key == Some(BrowseKey::Provider(slug.into())))
        .expect("provider retained by slug")
}
fn select(state: &mut ProvidersState, target: &BrowseKey) {
    key(state, ProvidersKey::Home);
    for _ in 0..200 {
        let view = browse(state);
        if view
            .selected
            .and_then(|i| view.rows.get(i))
            .and_then(|r| r.key.as_ref())
            == Some(target)
        {
            return;
        }
        key(state, ProvidersKey::Down);
    }
    panic!("selectable provider route was not reachable");
}
fn changes(effects: &[ProvidersEffect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            ProvidersEffect::CheckKey { .. }
                | ProvidersEffect::SaveKey { .. }
                | ProvidersEffect::Disconnect { .. }
                | ProvidersEffect::StartSignIn { .. }
                | ProvidersEffect::SignOut { .. }
                | ProvidersEffect::CheckEndpoint(_)
                | ProvidersEffect::SaveEndpoint(_)
                | ProvidersEffect::DeleteEndpoint { .. }
        )
    })
}

#[test]
fn merge_by_slug() {
    let state = loaded(6);
    let view = browse(&state);
    assert_eq!(
        view.rows
            .iter()
            .filter(|r| matches!(&r.key, Some(BrowseKey::Provider(_))))
            .count(),
        6
    );
    let methods = &row(&view, "example-00").methods;
    assert_eq!(methods.len(), 2);
    assert!(methods.contains(&SetupMethod::Key) && methods.contains(&SetupMethod::Account));
    assert_eq!(
        view.rows
            .iter()
            .filter(|r| r.label == "Same display name")
            .count(),
        2,
        "display labels never merge identities"
    );
}

#[test]
fn groups_in_all() {
    let state = loaded(6);
    let view = browse(&state);
    let keys = view
        .rows
        .iter()
        .filter_map(|r| r.key.clone())
        .collect::<Vec<_>>();
    let expected = (0..6)
        .map(|p| BrowseKey::Provider(format!("example-{p:02}")))
        .chain([
            BrowseKey::AddEndpoint,
            BrowseKey::Endpoint("fixture-endpoint".into()),
        ])
        .collect::<Vec<_>>();
    assert_eq!(
        keys, expected,
        "catalogue order inside ready/setup groups; Add precedes user endpoints"
    );
    assert!(view.rows.iter().filter(|r| r.key.is_none()).count() >= 3);
}

#[test]
fn scopes_and_tab_cycle() {
    let mut state = loaded(6);
    key(&mut state, ProvidersKey::Paste("example".into()));
    for expected in [
        ProvidersTab::Keys,
        ProvidersTab::Accounts,
        ProvidersTab::Endpoints,
        ProvidersTab::All,
    ] {
        key(&mut state, ProvidersKey::Tab);
        assert_eq!(screen(&state).tab, expected);
        assert_eq!(screen(&state).filter, "example");
        let view = browse(&state);
        for row in view.rows.iter().filter(|r| r.key.is_some()) {
            match expected {
                ProvidersTab::Keys => assert!(row.methods.contains(&SetupMethod::Key)),
                ProvidersTab::Accounts => assert!(row.methods.contains(&SetupMethod::Account)),
                ProvidersTab::Endpoints => assert!(matches!(
                    row.key,
                    Some(BrowseKey::Endpoint(_) | BrowseKey::AddEndpoint)
                )),
                _ => {}
            }
        }
    }
    key(&mut state, ProvidersKey::BackTab);
    assert_eq!(screen(&state).tab, ProvidersTab::Endpoints);
    assert_eq!(screen(&state).filter, "example");
}

#[test]
fn statuses_follow_the_rules() {
    let mut state = loaded(6);
    let view = browse(&state);
    assert_eq!(
        row(&view, "example-00").status,
        Some(ProviderStatus::ListedReady)
    );
    assert_eq!(
        row(&view, "example-01").status,
        Some(ProviderStatus::KeySaved)
    );
    assert_eq!(
        row(&view, "example-02").status,
        Some(ProviderStatus::NeedsSetup),
        "signed-in account alone does not prove readiness"
    );
    let before = view.rows.clone();
    step(&mut state, ProvidersInput::Connection { connected: false });
    let shown = screen(&state);
    assert!(
        shown
            .banner
            .unwrap_or_default()
            .to_lowercase()
            .contains("offline")
    );
    let view = browse(&state);
    assert_eq!(
        view.rows.iter().map(|r| r.status).collect::<Vec<_>>(),
        before.iter().map(|r| r.status).collect::<Vec<_>>()
    );
    assert!(
        row(&view, "example-00")
            .details
            .join(" ")
            .to_lowercase()
            .contains("offline")
    );
    let endpoint = view
        .rows
        .iter()
        .find(|r| matches!(&r.key, Some(BrowseKey::Endpoint(_))))
        .unwrap();
    assert_eq!(endpoint.status, Some(ProviderStatus::KeySaved));
}

#[test]
fn partial_failures_stay_unknown() {
    let (mut state, _) = open(OpenTarget::Tab(ProvidersTab::All), true);
    step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![
            provider("example-00", "Example provider 00", false, 0),
            provider("example-01", "Example provider 01", false, 0),
        ]))),
    );
    assert_eq!(
        row(&browse(&state), "example-00").status,
        Some(ProviderStatus::Checking)
    );
    step(
        &mut state,
        ProvidersInput::EnvLoaded(Err(Failure::BadReply)),
    );
    assert_eq!(
        row(&browse(&state), "example-00").status,
        Some(ProviderStatus::Unknown)
    );
    step(
        &mut state,
        ProvidersInput::EnvLoaded(Ok(vec![env(
            "EXAMPLE_API_KEY",
            "example-00",
            "Example provider 00",
            true,
        )])),
    );
    step(
        &mut state,
        ProvidersInput::OauthLoaded(Err(Failure::NotSent("unavailable".into()))),
    );
    let view = browse(&state);
    assert_eq!(view.counts.providers, 2);
    assert!(!view.counts.complete);
    assert_eq!(
        row(&view, "example-00").status,
        Some(ProviderStatus::KeySaved)
    );
    assert!(
        row(&view, "example-00")
            .details
            .join(" ")
            .to_lowercase()
            .contains("unknown")
    );
    step(
        &mut state,
        ProvidersInput::OptionsLoaded(Err(Failure::BadReply)),
    );
    assert!(
        browse(&state)
            .rows
            .iter()
            .all(|r| r.status != Some(ProviderStatus::NeedsSetup)),
        "failed readiness never means setup needed"
    );
}

#[test]
fn counts_are_computed() {
    for n in [3, 7, 32, 41] {
        let view = browse(&loaded(n));
        assert_eq!(view.counts.providers, n);
        assert!(view.counts.complete);
        assert_eq!(
            view.rows
                .iter()
                .filter(|r| matches!(&r.key, Some(BrowseKey::Provider(_))))
                .count(),
            n
        );
    }
}

#[test]
fn unmatched_inventory_rows_follow_catalogue_then_key_then_account_order() {
    let mut state = loaded(3);
    step(
        &mut state,
        ProvidersInput::EnvLoaded(Ok(vec![
            env("EXAMPLE_B_KEY", "example-key-b", "Example key B", false),
            env("EXAMPLE_A_KEY", "example-key-a", "Example key A", false),
        ])),
    );
    step(
        &mut state,
        ProvidersInput::OauthLoaded(Ok(vec![
            account(
                "example-account-z",
                "Example account Z",
                true,
                AccountStatus::NotSignedIn,
            ),
            account(
                "example-account-y",
                "Example account Y",
                true,
                AccountStatus::NotSignedIn,
            ),
        ])),
    );
    let keys = browse(&state)
        .rows
        .into_iter()
        .filter_map(|r| match r.key {
            Some(BrowseKey::Provider(slug)) => Some(slug),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "example-00",
            "example-01",
            "example-02",
            "example-key-b",
            "example-key-a",
            "example-account-z",
            "example-account-y"
        ]
    );
}

#[test]
fn remove_prefers_key_then_account_then_endpoint_and_names_the_target() {
    for (target, expected, phrase) in [
        (
            BrowseKey::Provider("example-00".into()),
            CardKind::RemoveConfirm,
            "Example provider 00",
        ),
        (
            BrowseKey::Provider("example-02".into()),
            CardKind::SignOutConfirm,
            "Example provider 02",
        ),
        (
            BrowseKey::Endpoint("fixture-endpoint".into()),
            CardKind::DeleteConfirm,
            "Example endpoint",
        ),
    ] {
        let mut state = loaded(6);
        let mut key_row = env(
            "EXAMPLE_00_API_KEY",
            "example-00",
            "Example provider 00",
            true,
        );
        key_row.shared_with = vec![("example-shared".into(), "Example shared".into())];
        step(
            &mut state,
            ProvidersInput::EnvLoaded(Ok(vec![
                key_row,
                env(
                    "EXAMPLE_01_API_KEY",
                    "example-01",
                    "Example provider 01",
                    true,
                ),
            ])),
        );
        select(&mut state, &target);
        let effects = key(&mut state, ProvidersKey::Action(ProvidersAction::Remove));
        assert!(!changes(&effects), "removal first opens confirmation");
        let card = screen(&state).card.expect("named removal confirmation");
        assert_eq!(card.kind, expected);
        let text = card.lines.join(" ");
        assert!(text.contains(phrase), "{text}");
        if target == BrowseKey::Provider("example-00".into()) {
            assert!(
                text.contains("Example shared"),
                "shared credential effects are explicit: {text}"
            );
        }
    }
}

#[test]
fn enter_opens_methods() {
    let mut state = loaded(6);
    select(&mut state, &BrowseKey::Provider("example-00".into()));
    assert!(
        !changes(&key(&mut state, ProvidersKey::Enter)),
        "multiple methods open a chooser without starting a write"
    );
    let shown = screen(&state);
    let card = shown.card.expect("method chooser");
    let text = card.lines.join(" ").to_lowercase();
    assert!(text.contains("key") && text.contains("sign-in"));
    key(&mut state, ProvidersKey::Esc);
    select(&mut state, &BrowseKey::Provider("example-01".into()));
    key(&mut state, ProvidersKey::Enter);
    assert_eq!(screen(&state).card.unwrap().kind, CardKind::Key);
    key(&mut state, ProvidersKey::Esc);
    step(
        &mut state,
        ProvidersInput::OauthLoaded(Ok(vec![account(
            "example-02",
            "Example provider 02",
            true,
            AccountStatus::NotSignedIn,
        )])),
    );
    select(&mut state, &BrowseKey::Provider("example-02".into()));
    assert!(
        key(&mut state, ProvidersKey::Enter)
            .iter()
            .any(|e| matches!(e,ProvidersEffect::StartSignIn{id,..} if id=="example-02"))
    );
    let (mut outside, _) = open(OpenTarget::Tab(ProvidersTab::All), true);
    step(&mut outside, ProvidersInput::EnvLoaded(Ok(vec![])));
    step(
        &mut outside,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider(
            "example-outside",
            "Example outside",
            false,
            0,
        )]))),
    );
    step(&mut outside, ProvidersInput::OauthLoaded(Ok(vec![])));
    select(&mut outside, &BrowseKey::Provider("example-outside".into()));
    key(&mut outside, ProvidersKey::Enter);
    assert!(
        screen(&outside)
            .notice
            .unwrap_or_default()
            .contains("is set up outside Talaria")
    );
}

#[test]
fn no_credential_text_in_rows_or_details() {
    let (mut state, _) = open(OpenTarget::Tab(ProvidersTab::Keys), true);
    let mut saved = env("EXAMPLE_API_KEY", "example-00", "Example provider 00", true);
    saved.tail = Some("«redacted:TEST...WXYZ»".into());
    let mut raw = env(
        "EXAMPLE_OTHER_API_KEY",
        "example-01",
        "Example provider 01",
        true,
    );
    raw.tail = Some("SENTINEL_PROVIDER_SECRET_012345".into());
    step(&mut state, ProvidersInput::EnvLoaded(Ok(vec![saved, raw])));
    step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![
            provider("example-00", "Example provider 00", false, 0),
            provider("example-01", "Example provider 01", false, 0),
        ]))),
    );
    let shown = screen(&state);
    let browse = shown.browse.unwrap();
    let all = format!(
        "{} {}",
        shown.rows.join(" "),
        browse
            .rows
            .iter()
            .flat_map(|r| r.details.clone())
            .collect::<Vec<_>>()
            .join(" ")
    );
    for secret in [
        "WXYZ",
        "TEST",
        "SENTINEL_PROVIDER_SECRET_012345",
        "«redacted:",
    ] {
        assert!(!all.contains(secret), "credential preview leaked: {all}");
    }
    key(&mut state, ProvidersKey::Action(ProvidersAction::Details));
    assert!(screen(&state).browse.unwrap().details_open);
    key(&mut state, ProvidersKey::Esc);
    key(&mut state, ProvidersKey::Enter);
    let card = screen(&state).card.unwrap().lines.join(" ");
    assert!(card.contains("Saved key ends in") && card.contains("WXYZ"));
    assert!(!card.contains("TEST"));
}

#[test]
fn escape_order_and_details_layer() {
    let mut state = loaded(6);
    key(&mut state, ProvidersKey::Paste("example".into()));
    let selected = browse(&state).selected;
    key(&mut state, ProvidersKey::Action(ProvidersAction::Details));
    assert!(browse(&state).details_open);
    for action in [
        ProvidersAction::Save,
        ProvidersAction::Remove,
        ProvidersAction::Test,
        ProvidersAction::NextTab,
        ProvidersAction::OpenLink,
        ProvidersAction::Copy,
    ] {
        assert!(!changes(&key(&mut state, ProvidersKey::Action(action))));
        assert_eq!(browse(&state).selected, selected);
        assert_eq!(screen(&state).filter, "example");
    }
    key(&mut state, ProvidersKey::PageDown);
    assert!(browse(&state).details_open);
    assert_eq!(browse(&state).selected, selected);
    key(&mut state, ProvidersKey::Esc);
    assert!(!browse(&state).details_open);
    assert_eq!(screen(&state).filter, "example");
    key(&mut state, ProvidersKey::Esc);
    assert!(screen(&state).filter.is_empty());
    assert!(state.is_open());
    assert!(key(&mut state, ProvidersKey::Esc).contains(&ProvidersEffect::Close));
}

#[test]
fn disconnected_and_busy_disable_changes() {
    let mut state = loaded(6);
    select(&mut state, &BrowseKey::Provider("example-01".into()));
    step(&mut state, ProvidersInput::Connection { connected: false });
    for action in [
        ProvidersAction::Remove,
        ProvidersAction::Save,
        ProvidersAction::Test,
    ] {
        assert!(!changes(&key(&mut state, ProvidersKey::Action(action))));
    }
    step(&mut state, ProvidersInput::Connection { connected: true });
    key(&mut state, ProvidersKey::Enter);
    step(&mut state, ProvidersInput::SecretEdited { empty: false });
    let first = step(&mut state, ProvidersInput::SecretSubmitted);
    assert!(changes(&first));
    assert!(
        !changes(&step(&mut state, ProvidersInput::SecretSubmitted)),
        "one write/check in flight"
    );
    step(
        &mut state,
        ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: true,
            reachable: true,
            message: String::new(),
        })),
    );
    let unknown = step(&mut state, ProvidersInput::KeySaved(Err(Failure::Unknown)));
    assert!(
        !unknown
            .iter()
            .any(|e| matches!(e, ProvidersEffect::SaveKey { .. })),
        "lost replies reload rather than resend"
    );
    assert!(
        unknown
            .iter()
            .any(|e| matches!(e, ProvidersEffect::LoadEnv | ProvidersEffect::LoadOptions))
    );
}
