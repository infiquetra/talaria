//! Server requests from the gateway: every kind is answered or shown at once, and withdrawn,
//! settled and replaced prompts leave with the right notice, once.
//!
//! The gateway side is the in-process mock in `support/mock_gateway.rs`. A real `Client` reads the
//! mock's frames; the tests then hand each inbound item to `PromptService` the way the
//! application does. All waits use real time, and the 100 ms budgets are measured from the
//! mock's own write and read stamps.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use hermes_client::{Client, Inbound, InboundEvent, InboundStream, RespondError, ServerRequest};
use serde_json::{Value, json};
use talaria_app::prompts::{
    MaskedKind, MaskedRequestInfo, PromptNotice, PromptService, ServerRequestPolicy, policy_for,
};
use talaria_core::session::{Effect, Event, PromptKey, SessionId, SessionState, update};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::prompts::{MaskedView, PromptStack};
use zeroize::Zeroizing;

use mock_gateway::{
    MockGateway, Reply, SESSION_ID, contract_server_request_kinds, next_inbound, session_event,
};

const BUDGET: Duration = Duration::from_millis(100);

/// The four kinds that get a masked card, written out by hand so that the comparison with the
/// vendored contract is not circular.
const MASKED_KINDS: [(&str, MaskedKind); 4] = [
    ("sudo", MaskedKind::Sudo),
    ("secret", MaskedKind::Secret),
    ("vault.code", MaskedKind::VaultCode),
    ("vault.unlock_prompt", MaskedKind::VaultUnlockPrompt),
];

/// The twelve kinds that are refused at once, written out by hand.
const REFUSED_KINDS: [&str; 12] = [
    "vault.save_login",
    "terminal.read",
    "setup_beat",
    "theme.pick",
    "theme.apply",
    "preview.act",
    "preview.read",
    "window.read",
    "tour",
    "display.install.sudo",
    "clarify",
    "approval",
];

const ELSEWHERE: &str = "answered elsewhere or timed out";
const TIMED_OUT: &str = "timed out; the agent continued without it";
const INTERRUPTED: &str = "withdrawn: the turn was interrupted";
const ANSWERED_ELSEWHERE_WINDOW: &str = "answered in another window";
const LOST: &str = "lost when the connection dropped";
const DENY_NOT_CONFIRMED: &str = "deny not confirmed; interrupting anyway";

fn refusal_message(kind: &str) -> String {
    format!("talaria does not support server request: {kind}")
}

fn refusal_notice(kind: &str) -> String {
    format!("Talaria does not support the {kind} request; Hermes continued without it")
}

fn not_accepted_notice(kind: &str) -> String {
    format!("Hermes did not accept the answer to {kind}; the agent continued without it")
}

// ---------------------------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------------------------

fn new_service() -> PromptService {
    let mut count = 0_u32;
    PromptService::new(Box::new(move || {
        count += 1;
        format!("sub-{count}")
    }))
}

/// The next server request, skipping the end markers of replaced connections.
async fn next_request(inbound: &mut InboundStream) -> ServerRequest {
    loop {
        match next_inbound(inbound).await {
            Inbound::ServerRequest(request) => return request,
            Inbound::Closed(_) => {}
            _ => panic!("an inbound item other than a server request arrived"),
        }
    }
}

/// The next event, skipping the end markers of replaced connections.
async fn next_session_event(inbound: &mut InboundStream) -> InboundEvent {
    loop {
        match next_inbound(inbound).await {
            Inbound::Event(event) => return event,
            Inbound::Closed(_) => {}
            _ => panic!("an inbound item other than an event arrived"),
        }
    }
}

struct Rig {
    mock: MockGateway,
    client: Client,
    inbound: InboundStream,
    service: PromptService,
    state: SessionState,
}

impl Rig {
    async fn new() -> Self {
        let (mock, client, inbound) = MockGateway::start().await;
        Self {
            mock,
            client,
            inbound,
            service: new_service(),
            state: SessionState::empty(SessionId::new(SESSION_ID)),
        }
    }

    /// The mock writes one server request; the service handles it as the application does.
    async fn offer(&mut self, kind: &str, extra: Value) -> (String, Vec<PromptNotice>) {
        let id = self.mock.server_request_with(kind, extra).await;
        let request = next_request(&mut self.inbound).await;
        assert_eq!(request.id, id, "the client delivered another request");
        let notices = self.service.on_server_request(request).await;
        (id, notices)
    }

    /// Deliver the next event to the reducer.
    async fn deliver_event(&mut self) -> InboundEvent {
        let event = next_session_event(&mut self.inbound).await;
        update(&mut self.state, session_event(&event));
        event
    }

    async fn open_approval(&mut self, prompt_id: &str, generation: i64) -> PromptKey {
        self.mock
            .approval_request(
                prompt_id,
                generation,
                "rm -rf .pytest_cache",
                &["once", "deny", "session", "always"],
            )
            .await;
        self.deliver_event().await;
        prompt_key(prompt_id, generation)
    }

    async fn open_clarification(&mut self, prompt_id: &str, generation: i64) -> PromptKey {
        self.mock
            .clarify_request(
                prompt_id,
                generation,
                "Which environment?",
                &["staging"],
                false,
            )
            .await;
        self.deliver_event().await;
        prompt_key(prompt_id, generation)
    }

    fn open_keys(&self) -> Vec<PromptKey> {
        self.state
            .open_prompts()
            .map(|cell| cell.key.clone())
            .collect()
    }

    /// Route the next `request.cancel` event to the service, as the application does.
    async fn route_cancel(&mut self) -> Vec<PromptNotice> {
        let event = next_session_event(&mut self.inbound).await;
        assert_eq!(
            event.raw["type"], "request.cancel",
            "the next event was not a withdrawal"
        );
        let payload = &event.raw["payload"];
        let id = payload["id"].as_str().unwrap_or_default().to_owned();
        let method = payload["method"].as_str().unwrap_or_default().to_owned();
        let reason = payload["reason"].as_str().unwrap_or_default().to_owned();
        self.service.on_request_cancel(&id, &method, &reason)
    }

    /// The mock withdraws a request; the service hears about it.
    async fn withdraw(&mut self, id: &str, method: &str, reason: &str) -> Vec<PromptNotice> {
        self.mock.request_cancel(id, method, reason).await;
        self.route_cancel().await
    }
}

fn prompt_key(prompt_id: &str, generation: i64) -> PromptKey {
    PromptKey {
        session_id: SessionId::new(SESSION_ID),
        prompt_id: prompt_id.to_owned(),
        execution_generation: generation,
    }
}

fn texts(notices: &[PromptNotice]) -> Vec<&str> {
    notices.iter().map(|notice| notice.text.as_str()).collect()
}

fn masked_info(service: &PromptService, id: &str) -> Option<MaskedRequestInfo> {
    service
        .masked_requests()
        .into_iter()
        .find(|info| info.id == id)
}

fn secret(text: &str) -> Zeroizing<String> {
    Zeroizing::new(text.to_owned())
}

// ---------------------------------------------------------------------------------------------
// Drawing a card into a vt100 screen
// ---------------------------------------------------------------------------------------------

/// An empty buffer of whichever type the section's `render` takes.
fn blank_buffer<R, B: Default>(_render: fn(&(dyn LiveSection + 'static), R, &mut B)) -> B {
    B::default()
}

/// Draw the stack's card alone in an otherwise empty region of `cols` by `rows` and return the
/// rows of a vt100 screen that was fed the result.
fn screen_rows(stack: &PromptStack, cols: u16, rows: u16) -> Vec<String> {
    let section = stack.section().expect("a card should be drawn");
    let view = LiveView {
        sections: vec![&section as &dyn LiveSection],
    };
    let area = view.layout(cols, rows)[0];
    let mut buf = blank_buffer(<dyn LiveSection>::render);
    buf.resize(area);
    section.render(area, &mut buf);
    let mut bytes = Vec::new();
    for y in 0..area.height {
        let line: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
        bytes.extend_from_slice(format!("\x1b[{};1H{}", y + 1, line.trim_end()).as_bytes());
    }
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(&bytes);
    parser
        .screen()
        .rows(0, cols)
        .map(|row| row.trim_end().to_owned())
        .collect()
}

fn masked_view_of(info: &MaskedRequestInfo) -> MaskedView {
    let method = match info.kind {
        MaskedKind::Sudo => "sudo",
        MaskedKind::Secret => "secret",
        MaskedKind::VaultCode => "vault.code",
        MaskedKind::VaultUnlockPrompt => "vault.unlock_prompt",
    };
    MaskedView {
        request_id: info.id.clone(),
        session_id: info.session_id.clone(),
        method: method.to_owned(),
        fields: info
            .fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// Row 16
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn sixteen_kinds_match_pinned_contract() {
    let mut expected: Vec<String> = MASKED_KINDS
        .iter()
        .map(|(kind, _)| (*kind).to_owned())
        .chain(REFUSED_KINDS.iter().map(|kind| (*kind).to_owned()))
        .collect();
    expected.sort();
    let mut unique = expected.clone();
    unique.dedup();
    assert_eq!(unique.len(), 16, "the hand-written lists overlap");

    let mut from_contract = contract_server_request_kinds();
    from_contract.sort();
    assert_eq!(
        from_contract, expected,
        "the vendored contract lists a kind that is not in the masked four and refused twelve, or \
         the other way round"
    );

    let (mock, _client, _inbound) = MockGateway::start().await;
    let mut from_mock = mock.server_request_kinds();
    from_mock.sort();
    assert_eq!(
        from_mock, expected,
        "the mock can send other kinds than the contract lists"
    );

    for (kind, masked) in MASKED_KINDS {
        assert_eq!(
            policy_for(kind),
            ServerRequestPolicy::Masked(masked),
            "{kind} should get a masked card"
        );
    }
    for kind in REFUSED_KINDS {
        assert_eq!(
            policy_for(kind),
            ServerRequestPolicy::Refuse,
            "{kind} should be refused"
        );
    }
    for kind in [
        "future.kind",
        "",
        "SUDO",
        "sudo ",
        "sudo.",
        "approval.request",
        "clarify.request",
        "vault",
    ] {
        assert_eq!(
            policy_for(kind),
            ServerRequestPolicy::Refuse,
            "a name outside the contract should be refused"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Row 17
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn each_kind_is_answered_or_shown_within_100ms() {
    let mut rig = Rig::new().await;
    let mut refused = Vec::new();
    let mut masked = Vec::new();

    for (kind, masked_kind) in MASKED_KINDS {
        let (id, notices) = rig.offer(kind, json!({})).await;
        let shown_after = rig
            .mock
            .written_at(&id)
            .expect("the mock stamps every request it writes")
            .elapsed();
        assert!(
            shown_after < BUDGET,
            "the {kind} request was exposed {shown_after:?} after the mock wrote it"
        );
        let info = masked_info(&rig.service, &id)
            .unwrap_or_else(|| panic!("{kind} should be an open masked request"));
        assert_eq!(info.kind, masked_kind);
        assert_eq!(info.session_id.as_deref(), Some(SESSION_ID));
        assert!(notices.is_empty(), "a masked card needs no notice");
        masked.push(id);
    }

    for kind in REFUSED_KINDS {
        let (id, notices) = rig.offer(kind, json!({})).await;
        let answer = rig.mock.wait_for_answer(&id, 0).await;
        assert!(answer.is_error, "{kind} should be answered with an error");
        assert_eq!(answer.params["code"], json!(-32601), "{kind}");
        assert_eq!(answer.params["message"], json!(refusal_message(kind)));
        let delay = rig
            .mock
            .answer_delay(&id)
            .expect("both stamps exist once the answer was read");
        assert!(
            delay < BUDGET,
            "the {kind} refusal took {delay:?} to reach the mock"
        );
        assert!(
            masked_info(&rig.service, &id).is_none(),
            "a refused request must not become a masked card"
        );
        assert_eq!(texts(&notices), [refusal_notice(kind).as_str()], "{kind}");
        refused.push(id);
    }

    assert!(
        rig.mock
            .no_new_frames_within(Duration::from_millis(100))
            .await,
        "the client wrote something more than the twelve refusals"
    );
    assert_eq!(rig.mock.answers().len(), 12, "one answer per refused kind");
    for id in &masked {
        assert!(
            rig.mock.answers_for(id).is_empty(),
            "a masked request waits for the person"
        );
    }
    assert_eq!(rig.service.masked_requests().len(), 4);
    rig.mock.assert_no_duplicate_answers();
    for id in refused.iter().chain(masked.iter()) {
        rig.mock.assert_answered_at_most_once(id);
    }
}

#[tokio::test]
async fn masked_requests_expose_their_fields() {
    let mut rig = Rig::new().await;
    let (sudo, _) = rig
        .offer("sudo", json!({"command": "sudo apt-get install jq"}))
        .await;
    let (secret_id, _) = rig
        .offer(
            "secret",
            json!({"env_var": "EXAMPLE_API_KEY", "prompt": "Paste the key", "metadata": {"a": 1}}),
        )
        .await;
    let (code, _) = rig
        .offer(
            "vault.code",
            json!({"site": "example.test", "hint": "from the app"}),
        )
        .await;
    let (unlock, _) = rig
        .offer(
            "vault.unlock_prompt",
            json!({"backend": "keyring", "display_name": "Example Vault"}),
        )
        .await;

    let has = |id: &str, name: &str, value: &str| {
        masked_info(&rig.service, id)
            .unwrap_or_else(|| panic!("the request should be open"))
            .fields
            .iter()
            .any(|(field, text)| *field == name && text == value)
    };
    assert!(has(&sudo, "command", "sudo apt-get install jq"));
    assert!(has(&secret_id, "env_var", "EXAMPLE_API_KEY"));
    assert!(has(&secret_id, "prompt", "Paste the key"));
    assert!(has(&code, "site", "example.test"));
    assert!(has(&code, "hint", "from the app"));
    assert!(has(&unlock, "backend", "keyring"));
    assert!(has(&unlock, "display_name", "Example Vault"));
    let secret_info = masked_info(&rig.service, &secret_id).expect("open");
    assert!(
        secret_info
            .fields
            .iter()
            .all(|(name, _)| *name != "metadata"),
        "the secret request's metadata is ignored"
    );
}

#[tokio::test]
async fn display_install_sudo_without_a_session_has_an_untied_notice() {
    let mut rig = Rig::new().await;
    let (id, notices) = rig
        .offer("display.install.sudo", json!({"session_id": ""}))
        .await;
    let answer = rig.mock.wait_for_answer(&id, 0).await;
    assert!(answer.is_error);
    assert_eq!(answer.params["code"], json!(-32601));
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].text, refusal_notice("display.install.sudo"));
    assert_eq!(
        notices[0].session_id, None,
        "the notice is not tied to a session"
    );
}

// ---------------------------------------------------------------------------------------------
// Row 18
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn card_visible_in_vt100_within_100ms() {
    let mut rig = Rig::new().await;

    // A sudo request becomes a masked card.
    let (id, _) = rig
        .offer("sudo", json!({"command": "sudo apt-get install jq"}))
        .await;
    let written = rig.mock.written_at(&id).expect("stamped");
    let view = masked_view_of(&masked_info(&rig.service, &id).expect("open masked request"));
    let mut stack = PromptStack::new();
    stack.sync(Some(SESSION_ID), &[], &[view], true, Instant::now());
    let rows = screen_rows(&stack, 80, 24);
    let visible_after = written.elapsed();
    assert!(
        rows.iter().any(|row| row.contains("sudo password")),
        "the sudo card title should be on the screen"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("$ sudo apt-get install jq")),
        "the sudo command should be on the screen"
    );
    assert!(
        visible_after < BUDGET,
        "the sudo card took {visible_after:?} to reach the screen"
    );

    // An approval event becomes an approval card.
    let mut rig = Rig::new().await;
    rig.open_approval("p-1", 3).await;
    let written = rig
        .mock
        .written()
        .into_iter()
        .rev()
        .find(|frame| frame.event_type.as_deref() == Some("approval.request"))
        .expect("the mock stamps every event it writes")
        .at;
    let cells: Vec<_> = rig.state.open_prompts().collect();
    assert_eq!(cells.len(), 1, "the reducer should hold one open prompt");
    let mut stack = PromptStack::new();
    stack.sync(Some(SESSION_ID), &cells, &[], true, Instant::now());
    let rows = screen_rows(&stack, 80, 24);
    let visible_after = written.elapsed();
    assert!(
        rows.iter().any(|row| row.contains("Approval needed")),
        "the approval card title should be on the screen"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("$ rm -rf .pytest_cache")),
        "the approval command should be on the screen"
    );
    assert!(
        visible_after < BUDGET,
        "the approval card took {visible_after:?} to reach the screen"
    );
}

// ---------------------------------------------------------------------------------------------
// Row 19
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn refusal_notice_once_per_kind_per_connection() {
    let mut rig = Rig::new().await;

    let (first, notices) = rig.offer("theme.pick", json!({})).await;
    assert_eq!(texts(&notices), [refusal_notice("theme.pick").as_str()]);
    assert_eq!(notices[0].session_id.as_deref(), Some(SESSION_ID));
    let (second, notices) = rig.offer("theme.pick", json!({})).await;
    assert!(
        notices.is_empty(),
        "a second request of the same kind on one connection gets no second notice"
    );
    for id in [&first, &second] {
        let answer = rig.mock.wait_for_answer(id, 0).await;
        assert!(answer.is_error, "every request is refused, notice or not");
        assert_eq!(answer.params["code"], json!(-32601));
        assert_eq!(
            answer.params["message"],
            json!(refusal_message("theme.pick"))
        );
    }

    // Another kind has its own notice.
    let (third_req, notices) = rig.offer("terminal.read", json!({})).await;
    assert_eq!(texts(&notices), [refusal_notice("terminal.read").as_str()]);
    let (fourth_req, notices) = rig.offer("terminal.read", json!({})).await;
    assert!(notices.is_empty());
    for id in [&third_req, &fourth_req] {
        let _ = rig.mock.wait_for_answer(id, 0).await;
    }
    assert_eq!(rig.mock.answers().len(), 4);

    // A new connection starts the count again.
    let generation = rig.mock.replace_connection(&rig.client).await;
    assert!(
        rig.service.on_connection_replaced(generation.0).is_empty(),
        "no masked card was open, so nothing is announced"
    );
    let (third, notices) = rig.offer("theme.pick", json!({})).await;
    assert_eq!(
        texts(&notices),
        [refusal_notice("theme.pick").as_str()],
        "the first refusal of a kind on a new connection gets a notice"
    );
    let answer = rig.mock.wait_for_answer(&third, 0).await;
    assert!(answer.is_error);
    assert_eq!(answer.params["code"], json!(-32601));
    assert_eq!(
        answer.connection, 2,
        "the refusal goes out on the new socket"
    );
    let (_, notices) = rig.offer("theme.pick", json!({})).await;
    assert!(notices.is_empty());
    rig.mock.assert_no_duplicate_answers();
}

// ---------------------------------------------------------------------------------------------
// Row 20
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn request_cancel_reasons() {
    let mut rig = Rig::new().await;

    // Each reason has its own notice for a card that is still open, and the card goes.
    for (reason, text) in [
        ("timeout", TIMED_OUT.to_owned()),
        ("interrupted", INTERRUPTED.to_owned()),
        ("resolved", ANSWERED_ELSEWHERE_WINDOW.to_owned()),
        ("superseded", "withdrawn by Hermes (superseded)".to_owned()),
    ] {
        let (id, _) = rig.offer("sudo", json!({})).await;
        assert!(masked_info(&rig.service, &id).is_some());
        let notices = rig.withdraw(&id, "sudo", reason).await;
        assert_eq!(texts(&notices), [text.as_str()], "reason {reason}");
        assert_eq!(notices[0].session_id.as_deref(), Some(SESSION_ID));
        assert!(
            masked_info(&rig.service, &id).is_none(),
            "a withdrawn card must leave ({reason})"
        );
    }

    // A reason that Hermes made up is shown with its control characters made visible.
    let (id, _) = rig.offer("secret", json!({})).await;
    let notices = rig.service.on_request_cancel(&id, "secret", "odd\u{7}x");
    assert_eq!(texts(&notices), ["withdrawn by Hermes (odd\u{2407}x)"]);

    // An answer after a withdrawal writes nothing.
    let (late, _) = rig.offer("sudo", json!({})).await;
    let notices = rig.withdraw(&late, "sudo", "timeout").await;
    assert_eq!(texts(&notices), [TIMED_OUT]);
    let result = rig.service.answer_masked(&late, secret("late-value")).await;
    assert!(
        result.is_err(),
        "an answer to a withdrawn request must fail"
    );
    assert!(
        rig.mock
            .no_new_frames_within(Duration::from_millis(100))
            .await,
        "nothing may be written after the withdrawal"
    );
    assert!(rig.mock.answers_for(&late).is_empty());

    // Hermes sends `resolved` to every viewer, the one that answered included: silent for an id
    // answered here, and for an id refused here.
    rig.mock.publish_cancel_after_response(Some("resolved"));
    let (answered, _) = rig.offer("sudo", json!({})).await;
    rig.service
        .answer_masked(&answered, secret("typed-value"))
        .await
        .expect("the answer should be written");
    rig.mock.wait_for_answer(&answered, 0).await;
    let notices = rig.route_cancel().await;
    assert!(
        notices.is_empty(),
        "`resolved` for an id answered here is silent, got {:?}",
        texts(&notices)
    );
    let (refused, _) = rig.offer("theme.pick", json!({})).await;
    rig.mock.wait_for_answer(&refused, 0).await;
    let notices = rig.route_cancel().await;
    assert!(
        notices.is_empty(),
        "`resolved` for an id refused here is silent, got {:?}",
        texts(&notices)
    );
    rig.mock.publish_cancel_after_response(None);

    // `timeout` or `interrupted` for an id answered here means Hermes did not accept the answer.
    for reason in ["timeout", "interrupted"] {
        let (id, _) = rig.offer("sudo", json!({})).await;
        rig.service
            .answer_masked(&id, secret("typed-value"))
            .await
            .expect("the answer should be written");
        rig.mock.wait_for_answer(&id, 0).await;
        let notices = rig.withdraw(&id, "sudo", reason).await;
        assert_eq!(
            texts(&notices),
            [not_accepted_notice("sudo").as_str()],
            "reason {reason}"
        );
        assert_eq!(notices[0].session_id.as_deref(), Some(SESSION_ID));
    }

    // An id this client never received gives nothing, whatever the reason.
    for reason in ["timeout", "interrupted", "resolved", "superseded"] {
        let notices = rig
            .service
            .on_request_cancel("srq-ffffffffffff", "sudo", reason);
        assert!(
            notices.is_empty(),
            "an unknown id must be silent ({reason}), got {:?}",
            texts(&notices)
        );
    }
    rig.mock.assert_no_duplicate_answers();
}

// ---------------------------------------------------------------------------------------------
// Row 21
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn settled_prompt_leaves_with_notice_once() {
    // Settled elsewhere: each unanswered prompt gives one notice, and only one.
    {
        let mut rig = Rig::new().await;
        rig.open_approval("p-1", 3).await;
        rig.open_clarification("c-1", 2).await;
        let open = rig.open_keys();
        assert_eq!(open.len(), 2);
        assert!(
            rig.service.on_prompts_changed(&open).is_empty(),
            "prompts that are still open give no notice"
        );
        rig.mock.approval_settled("p-1", 3).await;
        rig.deliver_event().await;
        rig.mock.clarify_settled("c-1", 2).await;
        rig.deliver_event().await;
        let open = rig.open_keys();
        assert!(open.is_empty(), "both prompts were settled");
        let notices = rig.service.on_prompts_changed(&open);
        assert_eq!(texts(&notices), [ELSEWHERE, ELSEWHERE]);
        for notice in &notices {
            assert_eq!(notice.session_id.as_deref(), Some(SESSION_ID));
        }
        assert!(
            rig.service.on_prompts_changed(&open).is_empty(),
            "a prompt that left gives its notice once"
        );
    }

    // Answered here: no notice before or after the settled event.
    {
        let mut rig = Rig::new().await;
        let key = rig.open_approval("p-1", 3).await;
        assert!(rig.service.on_prompts_changed(&rig.open_keys()).is_empty());
        let effects = update(
            &mut rig.state,
            Event::AnswerApproval {
                key: key.clone(),
                choice: "once".to_owned(),
            },
        );
        assert_eq!(effects.len(), 1);
        let effect = effects.into_iter().next().expect("one effect");
        assert!(matches!(effect, Effect::RespondApproval { .. }));
        let executed = rig.service.execute(&rig.client, effect).await;
        for event in executed.events {
            update(&mut rig.state, event);
        }
        assert!(
            rig.service.on_prompts_changed(&rig.open_keys()).is_empty(),
            "an answer in flight is not a departure"
        );
        rig.mock.approval_settled("p-1", 3).await;
        rig.deliver_event().await;
        let open = rig.open_keys();
        assert!(open.is_empty(), "the settled prompt should have left");
        assert!(
            rig.service.on_prompts_changed(&open).is_empty(),
            "a prompt answered here gives no notice when it settles"
        );
    }

    // Denied before an interrupt and confirmed: no notice.
    {
        let mut rig = Rig::new().await;
        rig.open_approval("p-1", 3).await;
        assert!(rig.service.on_prompts_changed(&rig.open_keys()).is_empty());
        let notices = rig
            .service
            .deny_before_interrupt(&rig.client, &rig.state, SESSION_ID, Duration::from_secs(2))
            .await;
        assert!(
            notices.is_empty(),
            "a confirmed deny needs no notice, got {:?}",
            texts(&notices)
        );
        assert_eq!(rig.mock.requests("approval.respond").len(), 1);
        rig.mock.approval_settled("p-1", 3).await;
        rig.deliver_event().await;
        let open = rig.open_keys();
        assert!(open.is_empty());
        assert!(
            rig.service.on_prompts_changed(&open).is_empty(),
            "a denied prompt gives no \"answered elsewhere\" notice"
        );
    }

    // Denied before an interrupt and not confirmed: one notice about the deny, none later.
    {
        let mut rig = Rig::new().await;
        rig.open_approval("p-1", 3).await;
        assert!(rig.service.on_prompts_changed(&rig.open_keys()).is_empty());
        rig.mock.set_respond_reply(Reply::Silent);
        let started = Instant::now();
        let notices = rig
            .service
            .deny_before_interrupt(
                &rig.client,
                &rig.state,
                SESSION_ID,
                Duration::from_millis(300),
            )
            .await;
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the wait must end at the given limit"
        );
        assert_eq!(texts(&notices), [DENY_NOT_CONFIRMED]);
        rig.mock.approval_settled("p-1", 3).await;
        rig.deliver_event().await;
        let open = rig.open_keys();
        assert!(open.is_empty());
        assert!(
            rig.service.on_prompts_changed(&open).is_empty(),
            "a deny that was not confirmed still counts as answered here"
        );
    }

    // `already_resolved`: one notice from the answer, none when the prompt settles.
    {
        let mut rig = Rig::new().await;
        let key = rig.open_approval("p-1", 3).await;
        assert!(rig.service.on_prompts_changed(&rig.open_keys()).is_empty());
        rig.mock.queue_respond_reply(Reply::AlreadyResolved);
        let effects = update(
            &mut rig.state,
            Event::AnswerApproval {
                key: key.clone(),
                choice: "once".to_owned(),
            },
        );
        let effect = effects.into_iter().next().expect("one effect");
        let executed = rig.service.execute(&rig.client, effect).await;
        assert_eq!(texts(&executed.notices), [ELSEWHERE]);
        assert_eq!(executed.events.len(), 1);
        assert!(matches!(
            &executed.events[0],
            Event::AnswerAlreadyResolved { key: got } if *got == key
        ));
        for event in executed.events {
            update(&mut rig.state, event);
        }
        assert!(
            rig.service.on_prompts_changed(&rig.open_keys()).is_empty(),
            "the prompt left once: it already gave its notice"
        );
        rig.mock.approval_settled("p-1", 3).await;
        rig.deliver_event().await;
        let open = rig.open_keys();
        assert!(open.is_empty());
        assert!(
            rig.service.on_prompts_changed(&open).is_empty(),
            "the settled event must not add a second notice"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Row 22
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn connection_replaced_drops_masked_cards() {
    let mut rig = Rig::new().await;
    let (orphan, _) = rig
        .offer("sudo", json!({"command": "sudo apt-get install jq"}))
        .await;
    assert_eq!(rig.service.masked_requests().len(), 1);

    let generation = rig.mock.replace_connection(&rig.client).await;
    assert_eq!(rig.mock.connection_count(), 2);
    let notices = rig.service.on_connection_replaced(generation.0);
    assert_eq!(texts(&notices), [LOST]);
    assert_eq!(notices[0].session_id.as_deref(), Some(SESSION_ID));
    assert!(
        rig.service.masked_requests().is_empty(),
        "the masked card must close with the old connection"
    );

    // The old request can no longer be answered, and nothing goes out on either connection.
    let result = rig
        .service
        .answer_masked(&orphan, secret("late-value"))
        .await;
    assert!(
        result.is_err(),
        "an answer to a request of a dropped connection must fail"
    );
    assert!(matches!(
        result,
        Err(RespondError::Cancelled | RespondError::NotConnected | RespondError::AlreadyAnswered)
    ));
    assert!(
        rig.mock
            .no_new_frames_within(Duration::from_millis(100))
            .await,
        "nothing may be written for an orphaned request"
    );
    assert!(rig.mock.answers_for(&orphan).is_empty());
    assert!(
        rig.mock.seen().iter().all(|frame| frame.connection != 2),
        "nothing may be written on the new connection"
    );

    // A request on the new connection is a fresh card that can be answered there.
    let (fresh, notices) = rig.offer("sudo", json!({})).await;
    assert_ne!(fresh, orphan);
    assert!(notices.is_empty());
    let open = rig.service.masked_requests();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, fresh);
    rig.service
        .answer_masked(&fresh, secret("typed-value"))
        .await
        .expect("an answer on the new connection should be written");
    let answer = rig.mock.wait_for_answer(&fresh, 0).await;
    assert_eq!(answer.connection, 2);
    assert!(rig.mock.answers_for(&orphan).is_empty());
    rig.mock.assert_no_duplicate_answers();
}
