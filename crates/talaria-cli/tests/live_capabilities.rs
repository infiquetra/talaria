#[path = "../../hermes-client/tests/support/mod.rs"]
mod gateway_support;
mod support;

use std::sync::Arc;
use std::time::Duration;

use gateway_support::{LiveEnv, live_preconditions};
use hermes_client::rest::NativeHttpMinter;
use hermes_client::{Client, Purpose, Scope, connect, mint_ticket};
use serde_json::Value;
use talaria_app::capabilities::{CapabilitiesService, ChangeOutcome};
use talaria_app::settings::MinterFactory;
use talaria_core::capabilities::{CapData, CapEffect, CapInput, CapTab, CapabilityChange};

fn skip(prefix: &str, reason: &str) {
    println!("{prefix}: SKIPPED ({reason})");
    assert_ne!(
        std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref(),
        Some("1"),
        "live capabilities test required"
    );
}

#[tokio::test]
#[ignore]
async fn live_capabilities_read() {
    let environment = match live_preconditions().await {
        Ok(environment) => environment,
        Err(reason) => {
            skip("LIVE_CAPABILITIES_RESULT", &reason);
            return;
        }
    };
    let mut counts = Vec::new();
    for profile in ["fixture", "default"] {
        if profile == "default" && environment.endpoint.resolve_profile(profile).is_err() {
            continue;
        }
        for list in ["skills", "toolsets", "plugins"] {
            let result = support::captured(
                support::talaria(
                    &["capabilities", list, "--json", "--profile", profile],
                    &environment.home,
                ),
                Duration::from_secs(60),
            );
            assert!(
                result.status.success(),
                "capabilities read failed for {list}"
            );
            let value: Value = serde_json::from_slice(&result.stdout).expect("one JSON object");
            assert_eq!(value["profile"], profile);
            let rows = value[list].as_array().expect("a list");
            if list == "skills" {
                assert!(!rows.is_empty());
            }
            if list == "toolsets" {
                assert!(
                    rows.iter()
                        .all(|row| row["name"].as_str().is_some_and(|name| !name.is_empty()))
                );
            }
            counts.push(rows.len());
        }
    }
    println!("LIVE_CAPABILITIES_RESULT: PASSED counts={counts:?}");
}

async fn service(environment: LiveEnv) -> (CapabilitiesService, hermes_client::InboundStream) {
    let ticket = mint_ticket(
        &environment.home,
        &environment.endpoint,
        &environment.fixture,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .expect("fixture ticket");
    let connection = connect(&environment.endpoint, ticket)
        .await
        .expect("fixture connection");
    let (socket, inbound) = Client::start(connection.into_parts());
    let endpoint = Arc::new(environment.endpoint);
    let served_profiles = endpoint
        .profiles()
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    let origin = endpoint.api_origin().to_owned();
    let home = environment.home.clone();
    let factory: MinterFactory = Arc::new(move |name| {
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            endpoint.resolve_profile(name).expect("served profile"),
        ))
    });
    (
        CapabilitiesService::new(
            Some(origin),
            served_profiles,
            factory,
            socket,
            environment.home.to_string_lossy().into_owned(),
            std::env::var("HOME").unwrap_or_default(),
        ),
        inbound,
    )
}

async fn read(service: &CapabilitiesService, tab: CapTab) -> Result<CapData, String> {
    let replies = service
        .execute(CapEffect::Fetch {
            profile: "fixture".into(),
            tab,
            generation: 1,
        })
        .await;
    replies
        .into_iter()
        .find_map(|reply| match reply {
            CapInput::Loaded {
                result: Ok(data), ..
            } => Some(data),
            _ => None,
        })
        .ok_or_else(|| "fixture list read failed".into())
}

#[tokio::test]
#[ignore]
async fn live_capabilities_write_on_fixture() {
    assert_eq!(
        std::env::var("TALARIA_PROBE_WRITES").ok().as_deref(),
        Some("1"),
        "operator must opt in to fixture writes"
    );
    let requested = std::env::var("TALARIA_PROBE_PROFILE").unwrap_or_else(|_| "fixture".into());
    assert_eq!(
        requested, "fixture",
        "capability writes refuse every other profile"
    );
    let environment = match live_preconditions().await {
        Ok(environment) => environment,
        Err(reason) => {
            skip("LIVE_CAPABILITIES_WRITE", &reason);
            return;
        }
    };
    let (service, _inbound) = service(environment).await;
    let CapData::Skills(skills) = read(&service, CapTab::Skills).await.expect("skills read") else {
        panic!("skills list");
    };
    let CapData::Toolsets(toolsets) = read(&service, CapTab::Toolsets)
        .await
        .expect("toolsets read")
    else {
        panic!("toolsets list");
    };
    let skill = skills
        .into_iter()
        .find(|row| row.enabled)
        .expect("one enabled fixture skill");
    let toolset = toolsets
        .into_iter()
        .find(|row| row.enabled)
        .expect("one enabled fixture toolset");
    for (tab, name) in [
        (CapTab::Skills, skill.name),
        (CapTab::Toolsets, toolset.name),
    ] {
        let change = |enabled| match tab {
            CapTab::Skills => CapabilityChange::Skill {
                name: name.clone(),
                enabled,
            },
            CapTab::Toolsets => CapabilityChange::Toolset {
                name: name.clone(),
                enabled,
            },
            _ => unreachable!(),
        };
        let off = service.apply("fixture", change(false)).await;
        let off_read = read(&service, tab).await;
        // Always attempt restoration before asserting the off result.
        let on = service.apply("fixture", change(true)).await;
        let on_read = read(&service, tab).await;
        assert_eq!(off, ChangeOutcome::Applied);
        assert_eq!(on, ChangeOutcome::Applied);
        let enabled = |data: CapData| match data {
            CapData::Skills(rows) => {
                rows.into_iter()
                    .find(|row| row.name == name)
                    .unwrap()
                    .enabled
            }
            CapData::Toolsets(rows) => {
                rows.into_iter()
                    .find(|row| row.name == name)
                    .unwrap()
                    .enabled
            }
            _ => unreachable!(),
        };
        assert!(!enabled(off_read.expect("off read-back")));
        assert!(enabled(on_read.expect("restored read-back")));
    }
    println!("LIVE_CAPABILITIES_WRITE: PASSED");
}
