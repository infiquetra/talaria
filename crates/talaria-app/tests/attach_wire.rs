//! The find step against the mock gateway. Issue #246.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::Duration;

use mock_gateway::{StandaloneGateway, list_row};
use serde_json::Value;
use talaria_app::attach::{AttachRequest, find_session};

const NOW: f64 = 1_792_700_000.0;
const SERVED: [&str; 2] = ["default", "fixture"];

fn served() -> Vec<String> {
    SERVED.iter().map(|name| (*name).to_owned()).collect()
}

fn request(reference: &str, profile: Option<&str>) -> AttachRequest {
    AttachRequest {
        reference: reference.to_owned(),
        profile: profile.map(str::to_owned),
        no_ensure: false,
    }
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn profile_scope(frame: &mock_gateway::Seen) -> (String, String) {
    (
        frame.params["profile"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        frame.params["scope"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    )
}

async fn wait_for_lists(gateway: &StandaloneGateway, count: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while gateway.requests("session.list").len() < count {
        assert!(
            std::time::Instant::now() < deadline,
            "only {} of {count} list requests arrived",
            gateway.requests("session.list").len()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn lists_both_scopes_per_profile_at_once_with_exact_params() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let live_id = format!("local-{}1a2b", "c".repeat(60));
    gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![list_row(
            &live_id,
            "attach probe",
            Some(NOW - 30.0),
            Some("minimax-m3.1-flash"),
            4,
            true,
        )],
    );
    gateway.set_hold_lists(true);

    let query = request(&live_id, None);
    let profiles = served();
    let (found, ()) = tokio::join!(find_session(home.path(), &query, &profiles, NOW), async {
        wait_for_lists(&gateway, 4).await;
        assert_eq!(gateway.held_list_count(), 4);
        let mut seen: Vec<(String, String)> = Vec::new();
        for frame in gateway.requests("session.list") {
            assert_eq!(sorted_keys(&frame.params), ["limit", "profile", "scope"]);
            assert_eq!(frame.params["limit"], 200);
            seen.push(profile_scope(&frame));
        }
        seen.sort();
        assert_eq!(
            seen,
            [
                ("default".to_owned(), "live".to_owned()),
                ("default".to_owned(), "stored".to_owned()),
                ("fixture".to_owned(), "live".to_owned()),
                ("fixture".to_owned(), "stored".to_owned()),
            ]
        );
        assert!(
            gateway
                .written()
                .iter()
                .all(|frame| frame.method != "session.list"),
            "a list reply arrived before all four requests"
        );
        gateway.release_held_lists();
    });
    let found = found.expect("one match");
    assert_eq!(found.key.profile, "fixture");
    assert_eq!(found.key.session_id, live_id);
    assert!(found.notes.is_empty());
}

#[tokio::test]
async fn profile_only_scope_searches_one_profile_with_note() {
    // No --profile: the host ticket is refused, only the ticket profile is
    // searched, and the A6 note says so.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_refuse_host_scope(true);
    let failure = find_session(home.path(), &request("no such thing", None), &served(), NOW)
        .await
        .expect_err("nothing is listed");
    assert_eq!(
        failure.failure.what,
        "no session matches \"no such thing\" in the profile default."
    );
    assert_eq!(
        failure.failure.next_step,
        "run \"talaria fleet list\" or \"talaria --resume\" to see sessions, or pass the full id with --profile <name>."
    );
    assert_eq!(failure.failure.exit_code, 1);
    assert_eq!(
        failure.notes,
        ["talaria: note: searched only the profile default (ticket refused)."]
    );
    let frames = gateway.requests("session.list");
    assert_eq!(frames.len(), 2);
    for frame in &frames {
        assert_eq!(sorted_keys(&frame.params), ["limit", "scope"]);
        assert!(frame.params.get("profile").is_none());
    }

    // With --profile the search is exactly what was asked: no A6.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_refuse_host_scope(true);
    let failure = find_session(
        home.path(),
        &request("no such thing", Some("fixture")),
        &served(),
        NOW,
    )
    .await
    .expect_err("nothing is listed");
    assert_eq!(
        failure.failure.what,
        "no session matches \"no such thing\" in the profile fixture."
    );
    assert!(
        failure.notes.is_empty(),
        "no A6 with --profile: {:?}",
        failure.notes
    );
    assert_eq!(gateway.requests("session.list").len(), 2);
}

#[tokio::test]
async fn failed_profile_is_noted_and_others_used() {
    // Both fixture calls refused: the default match wins, two A5 lines.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_list_error_for("fixture", "live", Some("profile_mismatch"));
    gateway.set_list_error_for("fixture", "stored", Some("profile_mismatch"));
    let live_id = format!("local-{}2b3c", "d".repeat(60));
    gateway.set_list_rows_for(
        "default",
        "live",
        vec![list_row(
            &live_id,
            "default work",
            Some(NOW - 30.0),
            None,
            2,
            false,
        )],
    );
    let found = find_session(home.path(), &request("default work", None), &served(), NOW)
        .await
        .expect("the default match");
    assert_eq!(found.key.profile, "default");
    assert_eq!(
        found.notes,
        [
            "talaria: note: could not list the live sessions of fixture (profile_mismatch); those were not searched.",
            "talaria: note: could not list the stored sessions of fixture (profile_mismatch); those were not searched.",
        ]
    );

    // Only the stored call refused: a live match is found, one A5 line.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_list_error_for("fixture", "stored", Some("profile_mismatch"));
    gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![list_row(
            &live_id,
            "live work",
            Some(NOW - 30.0),
            None,
            2,
            false,
        )],
    );
    let found = find_session(home.path(), &request("live work", None), &served(), NOW)
        .await
        .expect("the live match");
    assert_eq!(found.key.profile, "fixture");
    assert_eq!(
        found.notes,
        [
            "talaria: note: could not list the stored sessions of fixture (profile_mismatch); those were not searched."
        ]
    );
}

#[tokio::test]
async fn unmatched_id_with_one_profile_falls_through_to_hermes() {
    // One profile in scope: the unknown short id passes through unchanged.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let short = "local-abcdef1234567890";
    let found = find_session(
        home.path(),
        &request(short, Some("fixture")),
        &served(),
        NOW,
    )
    .await
    .expect("the pass-through");
    assert_eq!(found.key.profile, "fixture");
    assert_eq!(found.key.session_id, short);
    assert_eq!(gateway.requests("session.list").len(), 2);

    // Two profiles in scope: the same reference is no match.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let failure = find_session(home.path(), &request(short, None), &served(), NOW)
        .await
        .expect_err("no pass-through across profiles");
    assert_eq!(
        failure.failure.what,
        format!("no session matches \"{short}\" in the profiles default, fixture.")
    );
    assert_eq!(failure.failure.exit_code, 1);
    assert_eq!(gateway.requests("session.list").len(), 4);
}

#[tokio::test]
async fn unserved_profile_refused_before_listing() {
    for profile in ["nosuch", ".."] {
        let home = tempfile::tempdir().expect("a home");
        let gateway = StandaloneGateway::start(home.path());
        let failure = find_session(home.path(), &request("abc", Some(profile)), &served(), NOW)
            .await
            .expect_err("an unserved profile");
        assert_eq!(
            failure.failure.what,
            format!(
                "the profile {profile} is not served by the gateway (served: default, fixture)."
            )
        );
        assert_eq!(
            failure.failure.next_step,
            format!("pick a served profile, or ask the operator to serve {profile}.")
        );
        assert_eq!(failure.failure.exit_code, 2);
        assert!(failure.notes.is_empty());
        assert!(
            gateway.ticket_records().is_empty(),
            "no ticket for {profile}"
        );
        assert!(
            gateway.requests("session.list").is_empty(),
            "no listing for {profile}"
        );
    }
}

async fn wait_for_close(gateway: &StandaloneGateway) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while gateway.client_closed().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the side connection stayed open"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn side_connection_closed_on_every_path() {
    // One match.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let live_id = format!("local-{}3c4d", "e".repeat(60));
    gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![list_row(
            &live_id,
            "one match",
            Some(NOW - 30.0),
            None,
            1,
            false,
        )],
    );
    find_session(home.path(), &request("one match", None), &served(), NOW)
        .await
        .expect("one match");
    wait_for_close(&gateway).await;

    // Many matches.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![
            list_row(
                &format!("local-{}aa", "f".repeat(60)),
                "dup",
                Some(NOW - 30.0),
                None,
                1,
                false,
            ),
            list_row(
                &format!("local-{}bb", "0".repeat(60)),
                "dup",
                Some(NOW - 40.0),
                None,
                1,
                false,
            ),
        ],
    );
    let failure = find_session(home.path(), &request("dup", None), &served(), NOW)
        .await
        .expect_err("ambiguous");
    assert_eq!(failure.failure.exit_code, 2);
    assert!(
        failure
            .failure
            .what
            .starts_with("2 sessions match \"dup\":"),
        "ambiguous text: {}",
        failure.failure.what
    );
    wait_for_close(&gateway).await;

    // No match.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let failure = find_session(home.path(), &request("nothing here", None), &served(), NOW)
        .await
        .expect_err("no match");
    assert_eq!(failure.failure.exit_code, 1);
    wait_for_close(&gateway).await;

    // Every profile failed.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    for profile in ["default", "fixture"] {
        gateway.set_list_error_for(profile, "live", Some("profile_mismatch"));
        gateway.set_list_error_for(profile, "stored", Some("profile_mismatch"));
    }
    let failure = find_session(home.path(), &request("abc", None), &served(), NOW)
        .await
        .expect_err("all failed");
    assert_eq!(
        failure.failure.what,
        "could not list sessions (profile_mismatch)."
    );
    assert_eq!(
        failure.failure.next_step,
        "try again; if it keeps failing, pass the full id with --profile <name>."
    );
    assert_eq!(failure.failure.exit_code, 1);
    wait_for_close(&gateway).await;

    // Tickets refused: no connection opens, so none closes.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_refuse_tickets(true);
    let failure = find_session(home.path(), &request("abc", None), &served(), NOW)
        .await
        .expect_err("no ticket");
    assert_eq!(
        failure.failure.what,
        "could not list sessions (ticket refused)."
    );
    assert_eq!(failure.failure.exit_code, 1);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(gateway.client_closed().is_empty());
}
