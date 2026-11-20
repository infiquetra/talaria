//! Prompt-service caps: open masked cards, answered-request memory, and field size.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use hermes_client::liveness::LinkEvent;
use hermes_client::{Generation, Inbound, InboundEvent, Responder, ServerRequest};
use hermes_contract::events::EventFrame;
use hermes_contract::schemas::{
    CanonicalSnapshot, SecretRequestParams, SudoRequestParams, VaultCodeRequestParams,
    VaultUnlockRequestParams,
};
use hermes_contract::server_requests::ServerRequest as ContractRequest;
use talaria_app::app::{AppEffect, AppMessage, AppState, update};
use talaria_app::prompts::PromptService;
use talaria_app::sessions::OpenedSession;

const OPEN_CAP: usize = 64;
const ANSWERED_CAP: usize = 256;
const FIELD_CAP: usize = 4096;
const SUFFIX: &str = "… (truncated)";

fn service() -> PromptService {
    PromptService::new(Box::new(|| "sub-1".to_owned()))
}

fn detached(id: &str, request: ContractRequest, swallowed: Arc<AtomicU64>) -> ServerRequest {
    ServerRequest {
        generation: Generation(1),
        id: id.to_owned(),
        session_id: Some("sess-1".to_owned()),
        request,
        responder: Responder::detached(swallowed),
    }
}

fn sudo(id: &str, command: &str, swallowed: Arc<AtomicU64>) -> ServerRequest {
    detached(
        id,
        ContractRequest::Sudo(SudoRequestParams {
            command: command.to_owned(),
            session_id: "sess-1".to_owned(),
        }),
        swallowed,
    )
}

#[tokio::test]
async fn masked_prompt_requests_are_capped_without_evicting_open_ones() {
    let mut prompts = service();
    let swallowed = Arc::new(AtomicU64::new(0));
    for n in 0..70 {
        let id = format!("srq-{n:03}");
        prompts
            .on_server_request(sudo(&id, "true", Arc::clone(&swallowed)))
            .await;
    }
    let open = prompts.masked_requests();
    assert_eq!(open.len(), OPEN_CAP);
    assert_eq!(open[0].id, "srq-000");
    assert!(open.iter().all(|info| info.id != "srq-064"));
    assert_eq!(
        swallowed.load(Ordering::SeqCst),
        u64::try_from(70 - OPEN_CAP).expect("count fits")
    );
}

#[tokio::test]
async fn masked_prompt_fields_are_truncated_to_4kib() {
    let mut prompts = service();
    let swallowed = Arc::new(AtomicU64::new(0));
    let long = "é".repeat(3000);
    prompts
        .on_server_request(sudo("sudo-1", &long, Arc::clone(&swallowed)))
        .await;
    prompts
        .on_server_request(detached(
            "secret-1",
            ContractRequest::Secret(SecretRequestParams {
                env_var: "API_TOKEN".to_owned(),
                metadata: None,
                prompt: long.clone(),
                session_id: "sess-1".to_owned(),
            }),
            Arc::clone(&swallowed),
        ))
        .await;
    prompts
        .on_server_request(detached(
            "code-1",
            ContractRequest::VaultCode(VaultCodeRequestParams {
                hint: Some(long.clone()),
                session_id: "sess-1".to_owned(),
                site: Some(long.clone()),
            }),
            Arc::clone(&swallowed),
        ))
        .await;
    prompts
        .on_server_request(detached(
            "unlock-1",
            ContractRequest::VaultUnlockPrompt(VaultUnlockRequestParams {
                backend: "age".to_owned(),
                display_name: long.clone(),
                session_id: "sess-1".to_owned(),
            }),
            swallowed,
        ))
        .await;

    let open = prompts.masked_requests();
    assert_eq!(open.len(), 4);
    for info in &open {
        for (name, value) in &info.fields {
            assert!(
                value.len() <= FIELD_CAP,
                "{name} stored {} bytes",
                value.len()
            );
            if value.len() > 64 {
                let body = value
                    .strip_suffix(SUFFIX)
                    .unwrap_or_else(|| panic!("{name} was stored in full ({})", value.len()));
                assert!(long.starts_with(body), "{name} was not a char-safe prefix");
                assert!(value.len() <= FIELD_CAP);
            }
        }
    }
    let command = open[0]
        .fields
        .iter()
        .find(|(name, _)| *name == "command")
        .map(|(_, value)| value.as_str())
        .expect("command");
    assert!(command.ends_with(SUFFIX));
    assert!(command.len() <= FIELD_CAP);
}

#[tokio::test]
async fn answered_prompt_requests_stay_bounded() {
    let mut prompts = service();
    let swallowed = Arc::new(AtomicU64::new(0));
    for n in 0..=ANSWERED_CAP {
        prompts
            .on_server_request(detached(
                &format!("id-{n}"),
                ContractRequest::Unknown(serde_json::json!({})),
                Arc::clone(&swallowed),
            ))
            .await;
    }
    let oldest = prompts.on_request_cancel("id-0", "unknown", "timeout");
    assert!(
        oldest.is_empty(),
        "the oldest answered request was kept: {oldest:?}"
    );
    let newest = prompts.on_request_cancel(&format!("id-{ANSWERED_CAP}"), "unknown", "timeout");
    assert!(
        !newest.is_empty(),
        "the newest answered request was dropped"
    );
}

fn approval_message(state: &AppState, seq: u64, type_name: &str, prompt_id: &str) -> AppMessage {
    let opened = state.opened.as_ref().expect("session");
    let raw = serde_json::json!({
        "session_id": opened.session_id,
        "replay_epoch": opened.cursor.replay_epoch,
        "seq": seq,
        "type": type_name,
        "execution_generation": 1,
        "payload": {
            "prompt_id": prompt_id,
            "execution_generation": 1,
            "command": "true",
            "choices": ["once", "deny"],
        }
    });
    let frame = EventFrame::from_params(&raw).expect("approval event");
    AppMessage::Link(LinkEvent::Inbound {
        item: Inbound::Event(InboundEvent {
            generation: Generation(1),
            frame,
            raw,
        }),
        replayed: false,
    })
}

#[test]
fn pruning_a_committed_prompt_still_commits_the_new_final_cell() {
    let mut snapshot: CanonicalSnapshot =
        serde_json::from_str(include_str!("../data/open_approval.json")).expect("snapshot");
    snapshot.prompts.clear();
    snapshot.messages.clear();
    snapshot.display_messages = Some(Vec::new());
    let mut state = AppState::new("0123456789abcdef");
    state.profile = "fixture".to_string();
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
    let mut seq = state.opened.as_ref().expect("opened").cursor.last_sequence;
    for n in 0..=256 {
        let id = format!("p{n}");
        seq += 1;
        let request = approval_message(&state, seq, "approval.request", &id);
        let _ = update(&mut state, request);
        seq += 1;
        let settled = approval_message(&state, seq, "approval.settled", &id);
        let effects = update(&mut state, settled);
        if n == 256 {
            assert!(
                effects.iter().any(|effect| matches!(
                    effect,
                    AppEffect::CommitCells(range) if !range.is_empty()
                )),
                "the cell that became final during compaction was skipped: {effects:?}"
            );
        }
    }
}
