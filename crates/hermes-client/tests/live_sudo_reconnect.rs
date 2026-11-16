//! Ignored live test that a waiting sudo request survives a connection drop and restore.

#[path = "support/fault_relay.rs"]
mod fault_relay;
mod support;

use std::net::SocketAddr;
use std::time::Duration;

use fault_relay::FaultRelay;
use hermes_client::liveness::{GatewayDial, Link, LinkConfig, LinkEvent, LinkState};
use hermes_client::replay::Cursor;
use hermes_client::{Client, Inbound};
use hermes_contract::events::ServerRequest as ContractServerRequest;
use hermes_contract::methods::canonical::{
    prompt_submit, session_create, session_detach, session_resume,
};
use hermes_contract::methods::legacy::request_answer;
use hermes_contract::overrides::prompt_submit_params;
use hermes_contract::schemas::{
    CanonicalCreateParams, CanonicalResumeParams, CanonicalSnapshot, MaxTurns, RequestAnswerParams,
    SessionDetachParams,
};
use serde_json::{Map, Value};
use support::{LiveEnv, live_preconditions, report_skip};
use tokio::time::{Instant, timeout_at};

const RESULT: &str = "LIVE_SUDO_RECONNECT_RESULT";
const MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";
const SUDO_PROMPT: &str = "Run this exact shell command with the terminal tool and then tell me its exit code: sudo -k true";
const SUDO_WAIT: Duration = Duration::from_secs(120);
const RECONNECT_BUDGET: Duration = Duration::from_secs(30);
const SKIP_NO_SUDO: &str =
    "no sudo request arrived: sudo must ask for a password and SUDO_PASSWORD must be unset";

async fn start_relay(env: &LiveEnv) -> FaultRelay {
    let address: SocketAddr = env
        .endpoint
        .api_origin()
        .trim_start_matches("http://")
        .parse()
        .expect("the api origin is a loopback address and port");
    FaultRelay::start(address)
        .await
        .expect("the fault relay could not bind")
}

async fn resume(client: &Client, session: &str) -> CanonicalSnapshot {
    let params = CanonicalResumeParams {
        session_id: session.to_string(),
        ..CanonicalResumeParams::default()
    };
    client
        .call::<session_resume::Marker>(params)
        .await
        .unwrap_or_else(|err| panic!("session.resume failed: {err}"))
}

async fn detach(client: &Client, session: &str, subscription_id: &str) {
    let params = SessionDetachParams {
        end_reason: None,
        session_id: session.to_string(),
        subscription_id: subscription_id.to_string(),
    };
    let _ = client.call::<session_detach::Marker>(params).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn waiting_sudo_request_survives_a_reconnect() {
    let env = match live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            report_skip(RESULT, &reason);
            return;
        }
    };

    let relay = start_relay(&env).await;
    let dial = GatewayDial::new(env.home.clone(), hermes_contract::pinned::live_profile())
        .via_origin(relay.origin().to_string());
    let (link, mut rx) = Link::start(dial, LinkConfig::default())
        .await
        .unwrap_or_else(|failure| {
            panic!(
                "link start failed: {} | {}",
                failure.reason(),
                failure.next_step()
            )
        });

    let connect_deadline = Instant::now() + RECONNECT_BUDGET;
    loop {
        match timeout_at(connect_deadline, rx.recv()).await {
            Ok(Some(LinkEvent::State(LinkState::Connected { generation: 1 }))) => break,
            Ok(Some(LinkEvent::State(LinkState::Failed { reason, next_step }))) => {
                panic!("link failed before first connection: {reason} | {next_step}");
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("link ended before first connection"),
            Err(_) => panic!("timed out waiting for first connection"),
        }
    }

    let cwd = tempfile::tempdir().expect("temporary folder");
    let client = link.client();
    let create_params = CanonicalCreateParams {
        cwd: Some(cwd.path().to_string_lossy().into_owned()),
        model: Some(MODEL.to_string()),
        provider: Some(PROVIDER.to_string()),
        max_turns: MaxTurns::from(30),
        ..CanonicalCreateParams::default()
    };
    let snapshot = client
        .call::<session_create::Marker>(create_params)
        .await
        .unwrap_or_else(|err| panic!("session.create failed: {err}"));
    let session = snapshot.session_id.clone();
    let subscription = snapshot.subscription_id.clone();
    let cursor = Cursor {
        replay_epoch: snapshot.replay_epoch.clone(),
        last_sequence: u64::try_from(snapshot.last_sequence).expect("non-negative sequence"),
    };
    link.track(&session, cursor);

    let params = prompt_submit_params(&session, SUDO_PROMPT, "live-sudo-reconnect");
    let sent = serde_json::to_value(&params).expect("prompt.submit params encode");
    let unattended_key = ["unat", "tended"].concat();
    assert!(
        sent.get(&unattended_key)
            .is_none_or(|value| value == &Value::Bool(false)),
        "prompt.submit params carried unattended"
    );
    client
        .call::<prompt_submit::Marker>(params)
        .await
        .unwrap_or_else(|err| panic!("prompt.submit failed: {err}"));

    let sudo_deadline = Instant::now() + SUDO_WAIT;
    let mut sudo_request_id: Option<String> = None;
    while Instant::now() < sudo_deadline {
        let Ok(maybe_event) = timeout_at(sudo_deadline, rx.recv()).await else {
            break;
        };
        let Some(event) = maybe_event else {
            break;
        };
        match event {
            LinkEvent::Inbound {
                item: Inbound::ServerRequest(req),
                ..
            } => {
                if matches!(req.request, ContractServerRequest::Sudo(_))
                    && req.session_id.as_deref() == Some(session.as_str())
                {
                    let id = req.id;
                    req.responder.ignore();
                    sudo_request_id = Some(id);
                    break;
                }
                req.responder.ignore();
            }
            LinkEvent::Inbound {
                item: Inbound::Event(ev),
                ..
            } if ev.frame.session_id.as_deref() == Some(session.as_str())
                && ev.frame.type_name() == Some("message.complete") =>
            {
                break;
            }
            LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                panic!("link failed while waiting for sudo request: {reason} | {next_step}");
            }
            _ => {}
        }
    }

    let Some(req_id) = sudo_request_id else {
        println!("{SKIP_NO_SUDO}");
        detach(&client, &session, &subscription).await;
        link.close().await;
        report_skip(RESULT, SKIP_NO_SUDO);
        return;
    };

    relay.drop();
    relay.restore();

    let reconnect_deadline = Instant::now() + RECONNECT_BUDGET;
    loop {
        match timeout_at(reconnect_deadline, rx.recv()).await {
            Ok(Some(LinkEvent::State(LinkState::Connected { generation }))) if generation >= 2 => {
                break;
            }
            Ok(Some(LinkEvent::State(LinkState::Failed { reason, next_step }))) => {
                panic!("link failed during reconnect: {reason} | {next_step}");
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("link ended during reconnect"),
            Err(_) => panic!("timed out waiting for reconnect"),
        }
    }

    let resumed = resume(&client, &session).await;
    let still_open = resumed
        .open_requests
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|entry| entry.id == req_id);
    assert!(
        still_open,
        "recovered open_requests did not retain sudo request {req_id}"
    );

    let mut result_map = Map::new();
    result_map.insert("value".to_string(), Value::String(String::new()));
    client
        .call::<request_answer::Marker>(RequestAnswerParams {
            id: req_id.clone(),
            profile: None,
            result: result_map,
        })
        .await
        .unwrap_or_else(|err| panic!("request.answer failed for {req_id}: {err}"));

    let after = resume(&client, &session).await;
    let still_listed = after
        .open_requests
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|entry| entry.id == req_id);
    assert!(
        !still_listed,
        "open_requests still listed {req_id} after request.answer"
    );

    detach(&client, &session, &resumed.subscription_id).await;
    link.close().await;
}
