//! Live profiles reads and one guarded write. Issue #253.

use std::path::PathBuf;
use std::time::Duration;

use hermes_client::{
    CallOptions, Client, DiscoverOptions, Endpoint, EnsurePolicy, InboundStream, Purpose, Scope,
    connect, default_home, discover_with, handshake, mint_ticket,
};
use hermes_contract::methods::canonical::profiles_list;
use hermes_contract::methods::legacy::{profiles_configure, profiles_describe};
use hermes_contract::schemas::{ProfileNameParams, ProfilesConfigureParams};

const PROFILE: &str = "fixture";
const PROBE_TEXT: &str = "Talaria live probe (restored after the test)";

struct LiveEnv {
    home: PathBuf,
    endpoint: Endpoint,
    client: Client,
    inbound: InboundStream,
}

fn skip(tag: &str, reason: &str) {
    if std::env::var_os("TALARIA_LIVE_REQUIRE").is_some() {
        panic!("live profiles required but unavailable: {reason}");
    }
    println!("{tag}: SKIPPED ({reason})");
}

fn finish(line: &str) {
    assert_public(line);
    println!("{line}");
}

fn assert_public(text: &str) {
    assert!(
        !text.contains("hermes-gateway-ticket"),
        "ticket marker in live output"
    );
    assert!(!has_long_token(text), "live output has a long token");
}

fn has_long_token(text: &str) -> bool {
    let mut run = 0;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            run += 1;
            if run >= 40 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

async fn prepare() -> Option<LiveEnv> {
    let home = default_home();
    let endpoint = match discover_with(
        &home,
        &DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        },
    )
    .await
    {
        Ok(endpoint) => endpoint,
        Err(err) => {
            skip("LIVE_PROFILES_RESULT", &err.to_string());
            return None;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skip("LIVE_PROFILES_RESULT", &reason);
        return None;
    }
    let profile = match endpoint.resolve_profile(PROFILE) {
        Ok(profile) => profile,
        Err(err) => {
            skip("LIVE_PROFILES_RESULT", &err.to_string());
            return None;
        }
    };
    let ticket = mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    let mut connection = connect(&endpoint, ticket)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    handshake(&mut connection)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    let (client, inbound) = Client::start(connection.into_parts());
    Some(LiveEnv {
        home,
        endpoint,
        client,
        inbound,
    })
}

#[tokio::test]
#[ignore]
async fn live_profiles_list_classifies() {
    let Some(env) = prepare().await else { return };
    let _ = &env.inbound;
    let params = hermes_contract::schemas::CanonicalProfilesParams {
        include_sessions: false,
        ..Default::default()
    };
    let result = env
        .client
        .call_with::<profiles_list::Marker>(params, CallOptions::default())
        .await
        .unwrap_or_else(|err| panic!("profiles.list failed: {err:?}"));
    let value = serde_json::to_value(&result).expect("the result encodes");
    let roster = talaria_app::profiles::parse_roster(&value).expect("the roster parses");
    let (served, parked) =
        hermes_client::control_verbs::served_and_parked(&env.endpoint, &env.home);
    let gateway = talaria_core::profiles::GatewayView { served, parked };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock runs")
        .as_millis() as u64;
    let rows = talaria_core::profiles::join_roster(Some(&roster), &gateway, None, &[], now_ms);
    for name in ["default", PROFILE] {
        let row = rows
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("{name} is listed"));
        assert_eq!(
            row.state,
            talaria_core::profiles::ServeState::Served,
            "{name}"
        );
        assert_eq!(
            talaria_core::profiles::explain(&row.state, name).label,
            "served"
        );
    }
    let served_count = rows
        .iter()
        .filter(|row| row.state == talaria_core::profiles::ServeState::Served)
        .count();
    finish(&format!(
        "LIVE_PROFILES_RESULT: PASSED (profiles={} served={})",
        rows.len(),
        served_count
    ));
}

/// Restores the fixture description when the probe ends early. The restore runs
/// on its own thread with a fresh runtime, retries once, and reports.
struct RestoreGuard {
    client: Option<Client>,
    original: String,
}

impl RestoreGuard {
    fn armed(client: Client, original: String) -> Self {
        Self {
            client: Some(client),
            original,
        }
    }

    fn disarm(mut self) {
        self.client = None;
    }

    fn restore(client: &Client, original: &str) -> Result<(), String> {
        let params = ProfilesConfigureParams {
            name: Some(PROFILE.to_owned()),
            description: Some(original.to_owned()),
            ..Default::default()
        };
        std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| err.to_string())?;
                runtime.block_on(async {
                    for _ in 0..2 {
                        let outcome = client
                            .call_with::<profiles_configure::Marker>(
                                params.clone(),
                                CallOptions::default(),
                            )
                            .await;
                        match outcome {
                            Ok(answer) if answer.ok => return Ok(()),
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }
                    Err("the restore was not acknowledged".to_owned())
                })
            });
            handle
                .join()
                .map_err(|_| "the restore thread ended".to_owned())?
        })
    }
}

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        let Some(client) = self.client.take() else {
            return;
        };
        match Self::restore(&client, &self.original) {
            Ok(()) => println!("LIVE_PROFILES_RESTORE: restored"),
            Err(words) => println!("LIVE_PROFILES_RESTORE: FAILED ({words})"),
        }
    }
}

#[tokio::test]
#[ignore]
async fn live_profiles_configure_round_trip() {
    if std::env::var_os("TALARIA_PROBE_WRITES").as_deref() != Some(std::ffi::OsStr::new("1")) {
        skip("LIVE_PROFILES_RESULT", "TALARIA_PROBE_WRITES != 1");
        return;
    }
    let wanted = std::env::var("TALARIA_PROBE_PROFILE").unwrap_or_else(|_| PROFILE.to_owned());
    assert_eq!(
        wanted, PROFILE,
        "this probe only runs against the fixture profile"
    );
    let Some(env) = prepare().await else { return };
    let _ = &env.inbound;

    let params = ProfileNameParams {
        name: Some(PROFILE.to_owned()),
        ..Default::default()
    };
    let described = env
        .client
        .call_with::<profiles_describe::Marker>(params, CallOptions::default())
        .await
        .unwrap_or_else(|err| panic!("profiles.describe failed: {err:?}"));
    let original = described.description.clone();

    let guard = RestoreGuard::armed(env.client.clone(), original.clone());
    let params = ProfilesConfigureParams {
        name: Some(PROFILE.to_owned()),
        description: Some(PROBE_TEXT.to_owned()),
        ..Default::default()
    };
    let answer = env
        .client
        .call_with::<profiles_configure::Marker>(params, CallOptions::default())
        .await
        .unwrap_or_else(|err| panic!("profiles.configure failed: {err:?}"));
    assert!(answer.ok, "the probe write applied");
    assert_eq!(answer.applied.description, Some(true));

    let params = ProfileNameParams {
        name: Some(PROFILE.to_owned()),
        ..Default::default()
    };
    let reread = env
        .client
        .call_with::<profiles_describe::Marker>(params, CallOptions::default())
        .await
        .unwrap_or_else(|err| panic!("profiles.describe failed: {err:?}"));
    assert_eq!(reread.description, PROBE_TEXT, "the probe text reads back");

    let params = ProfilesConfigureParams {
        name: Some(PROFILE.to_owned()),
        description: Some(original.clone()),
        ..Default::default()
    };
    let restored = env
        .client
        .call_with::<profiles_configure::Marker>(params, CallOptions::default())
        .await
        .unwrap_or_else(|err| panic!("profiles.configure failed: {err:?}"));
    assert!(restored.ok, "the restore applied");

    let params = ProfileNameParams {
        name: Some(PROFILE.to_owned()),
        ..Default::default()
    };
    let reread = env
        .client
        .call_with::<profiles_describe::Marker>(params, CallOptions::default())
        .await
        .unwrap_or_else(|err| panic!("profiles.describe failed: {err:?}"));
    assert_eq!(
        reread.description, original,
        "the original reads back byte for byte"
    );

    guard.disarm();
    finish("LIVE_PROFILES_RESULT: PASSED (restore verified)");
}
