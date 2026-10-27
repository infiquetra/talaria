#[path = "support/mock_rest.rs"]
mod mock_rest;

use hermes_client::{
    capabilities_api as api,
    rest::{RestClient, RestError, RestOptions},
};
use mock_rest::{CountingMinter, MockRest};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

async fn setup() -> (MockRest, RestClient, Arc<CountingMinter>) {
    let mock = MockRest::start()
        .await
        .allow_own_profile_selector("fixture")
        .with_capabilities();
    let minter = Arc::new(CountingMinter::default());
    let rest = RestClient::new(&mock.origin(), minter.clone(), RestOptions::default()).unwrap();
    (mock, rest, minter)
}

#[tokio::test]
async fn routes_decode_fixtures() {
    route_errors_and_profile_encoding_are_preserved().await;
    hub_reads_use_a_forty_five_second_deadline().await;

    let (mock, rest, minter) = setup().await;
    let skills = api::skills_list(&rest, "fixture").await.unwrap();
    assert_eq!(skills.len(), 4);
    assert_eq!(
        (&*skills[0].name, skills[0].enabled),
        ("fixture-skill-a", true)
    );
    assert_eq!(skills[2].category, None);
    assert_eq!(skills[3].description, "");
    assert!(
        api::skill_content(&rest, "fixture", "fixture-skill-a")
            .await
            .unwrap()
            .starts_with("# Fixture")
    );
    let reply = api::skill_toggle(&rest, "fixture", "fixture-skill-a", false)
        .await
        .unwrap();
    assert!(reply.ok && !reply.enabled);
    assert!(!api::skills_list(&rest, "fixture").await.unwrap()[0].enabled);
    let search = api::hub_search(&rest, "fixture", "a & b/é").await.unwrap();
    assert_eq!(search.timed_out, ["github"]);
    assert!(search.installed.get("official/fixture-skill-b").is_some());
    assert!(
        api::hub_search(&rest, "fixture", "zzz")
            .await
            .unwrap()
            .results
            .is_empty()
    );
    let preview = api::hub_preview(&rest, "fixture", "official/fixture-skill-b")
        .await
        .unwrap();
    assert!(preview.skill_md.starts_with("# Fixture"));
    for policy in ["allow", "ask", "block"] {
        assert_eq!(
            api::hub_scan(&rest, "fixture", &format!("official/fixture-{policy}"))
                .await
                .unwrap()
                .policy,
            policy
        );
    }
    let toolsets = api::toolsets_list(&rest, "fixture").await.unwrap();
    assert_eq!(toolsets.len(), 4);
    assert!(!toolsets[1].configured);
    let toggled = api::toolset_toggle(&rest, "fixture", "image_gen", true)
        .await
        .unwrap();
    assert!(toggled.ok && toggled.enabled);
    assert_eq!(toggled.post_setup_started.as_deref(), Some("fixture-setup"));
    let config = api::toolset_config(&rest, "fixture", "image_gen")
        .await
        .unwrap();
    assert_eq!(config.providers[0]["env_vars"][0]["is_set"], true);
    assert_eq!(api::plugin_catalog(&rest).await.unwrap().entries.len(), 2);
    let requests = mock.requests().await;
    let search = requests
        .iter()
        .find(|request| request.path.contains("q=a"))
        .unwrap();
    assert_eq!(
        search.path,
        "/api/skills/hub/search?q=a%20%26%20b%2F%C3%A9&source=all&limit=20&profile=fixture"
    );
    let toggle = requests
        .iter()
        .find(|request| request.path == "/api/skills/toggle")
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&toggle.body).unwrap(),
        json!({"name":"fixture-skill-a","enabled":false,"profile":"fixture"})
    );
    assert_eq!(minter.count(), requests.len());
}

async fn route_errors_and_profile_encoding_are_preserved() {
    let (mock, rest, _) = setup().await;
    assert!(matches!(
        api::toolset_toggle(&rest, "fixture", "nosuch", true).await,
        Err(RestError::Refused { status: 400, .. })
    ));
    assert!(
        matches!(api::skills_list(&rest,"other").await, Err(RestError::Forbidden { detail }) if detail == "profile_scope_mismatch")
    );
    assert!(matches!(
        api::skill_content(&rest, "fixture", "nosuch").await,
        Err(RestError::NotFound { .. })
    ));
    for read in ["preview", "scan"] {
        let error = if read == "preview" {
            api::hub_preview(&rest, "fixture", "nosuch")
                .await
                .unwrap_err()
        } else {
            api::hub_scan(&rest, "fixture", "nosuch").await.unwrap_err()
        };
        assert!(matches!(error, RestError::NotFound { .. }));
    }
    mock.fail_status("GET", "/api/skills?profile=fixture", 502, None)
        .await;
    assert!(matches!(
        api::skills_list(&rest, "fixture").await,
        Err(RestError::Server { status: 502 })
    ));
    mock.fail_status("GET", "/api/dashboard/plugins/catalog", 401, None)
        .await;
    assert!(matches!(
        api::plugin_catalog(&rest).await,
        Err(RestError::Unauthorized)
    ));
    let encoded = MockRest::start()
        .await
        .allow_own_profile_selector("fixture & /é")
        .with_capabilities();
    let rest = RestClient::new(
        &encoded.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .unwrap();
    api::skills_list(&rest, "fixture & /é").await.unwrap();
    assert_eq!(
        encoded.requests().await[0].path,
        "/api/skills?profile=fixture%20%26%20%2F%C3%A9"
    );
}

async fn hub_reads_use_a_forty_five_second_deadline() {
    let (_mock, rest, _) = setup().await;
    api::hub_search(&rest, "fixture", "fixture").await.unwrap();
    for route in [
        "search?q=fixture&source=all&limit=20",
        "preview?identifier=fixture",
        "scan?identifier=fixture",
    ] {
        let (mock, rest, _) = setup().await;
        let path = format!("/api/skills/hub/{route}&profile=fixture");
        mock.delay("GET", &path, Duration::from_secs(46)).await;
        let kind = route.split('?').next().unwrap().to_owned();
        let call = tokio::spawn(async move {
            match kind.as_str() {
                "search" => api::hub_search(&rest, "fixture", "fixture")
                    .await
                    .map(|_| ()),
                "preview" => api::hub_preview(&rest, "fixture", "fixture")
                    .await
                    .map(|_| ()),
                _ => api::hub_scan(&rest, "fixture", "fixture").await.map(|_| ()),
            }
        });
        while mock.requests().await.is_empty() {
            assert!(!call.is_finished(), "hub call ended before sending");
            tokio::task::yield_now().await;
        }
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(44)).await;
        tokio::task::yield_now().await;
        assert!(!call.is_finished(), "hub read expired before 45 seconds");
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(matches!(
            call.await.unwrap(),
            Err(RestError::Timeout { sent: true })
        ));
        tokio::time::resume();
    }
}
