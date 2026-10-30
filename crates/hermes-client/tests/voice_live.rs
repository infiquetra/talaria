//! Ignored live voice calls. `cargo test` does not run them.
//!
//! A missing gateway or a missing `fixture` profile prints `SKIPPED:` and returns.
//! These tests never turn voice on.

mod support;

use hermes_client::voice_api::{voice_record, voice_requirements, voice_speak};
use hermes_client::{
    CallError, Client, InboundStream, Purpose, Scope, connect, handshake, mint_ticket,
};
use hermes_contract::methods::canonical::{session_create, session_detach};
use hermes_contract::schemas::{
    CanonicalCreateParams, CanonicalSnapshot, SessionDetachParams, VoiceRecordStatus,
};
use support::{live_preconditions, report_skip};

#[tokio::test]
#[ignore]
async fn status_reports_requirements() {
    let Some((client, _inbound)) = prepare().await else {
        return;
    };
    let result = voice_requirements(&client)
        .await
        .expect("voice status decodes");
    println!(
        "audio={} speech-to-text={} ready={}",
        word(result.audio_available),
        word(result.stt_available),
        word(result.available)
    );
}

#[tokio::test]
#[ignore]
async fn record_refused_while_voice_off() {
    let Some((client, _inbound)) = prepare().await else {
        return;
    };
    let snapshot = client
        .call::<session_create::Marker>(fresh_session())
        .await
        .unwrap_or_else(|err| panic!("session.create failed: {err}"));
    let started = voice_record(&client, &snapshot.session_id, true).await;
    detach(&client, &snapshot).await;
    match started {
        Ok(result)
            if result.status == VoiceRecordStatus::Busy
                && result.reason.as_deref() == Some("wake_owned") =>
        {
            println!("BLOCKED: wake word held by another client");
        }
        Err(CallError::Rpc(err)) if err.code == 4015 => {}
        Err(CallError::Rpc(err)) if err.code == -32601 => {
            panic!("voice.record is not on this gateway");
        }
        other => panic!("voice off did not answer 4015: {other:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn tts_accepts_short_text() {
    if std::env::var("TALARIA_LIVE_AUDIO").ok().as_deref() != Some("1") {
        skip("TALARIA_LIVE_AUDIO is not 1");
        return;
    }
    let Some((client, _inbound)) = prepare().await else {
        return;
    };
    let result = voice_speak(&client, "Talaria voice check.")
        .await
        .expect("voice.tts");
    assert_eq!(result.status, "speaking");
}

async fn prepare() -> Option<(Client, InboundStream)> {
    match live_preconditions().await {
        Ok(env) => Some(attach(&env).await),
        Err(reason) => {
            skip(&reason);
            None
        }
    }
}

fn skip(reason: &str) {
    println!("SKIPPED: {reason}");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live gateway required");
    }
    let _ = report_skip;
}

async fn attach(env: &support::LiveEnv) -> (Client, InboundStream) {
    let ticket = mint_ticket(
        &env.home,
        &env.endpoint,
        &env.fixture,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    let mut connection = connect(&env.endpoint, ticket)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    handshake(&mut connection)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    Client::start(connection.into_parts())
}

fn fresh_session() -> CanonicalCreateParams {
    CanonicalCreateParams {
        model: Some("minimax-m3.1-flash".to_owned()),
        provider: Some("ollama-cloud".to_owned()),
        max_turns: hermes_contract::schemas::MaxTurns::from(30),
        ..CanonicalCreateParams::default()
    }
}

async fn detach(client: &Client, snapshot: &CanonicalSnapshot) {
    let params = SessionDetachParams {
        session_id: snapshot.session_id.clone(),
        subscription_id: snapshot.subscription_id.clone(),
        end_reason: None,
    };
    client
        .call::<session_detach::Marker>(params)
        .await
        .unwrap_or_else(|err| panic!("session.detach failed: {err}"));
}

fn word(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}
