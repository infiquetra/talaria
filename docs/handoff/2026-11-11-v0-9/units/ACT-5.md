# ACT-5: A note typed on an approval is lost when Hermes refuses the follow-up prompt

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. The issue states REQ-1 through REQ-6, the tests, and the stop conditions. This file is the code map those requirements point at: what the repository does today, with the line the behaviour lives on. It does not add a requirement.

## The note row

`Card::answer` attaches the note only for an approval. A non-empty trimmed field becomes `PromptIntent::Approve { key, choice, note }`. A clarify answer does not carry a note.

```106:117:crates/talaria-tui/src/prompts/stack.rs
    fn answer(&mut self, value: String) -> Option<PromptIntent> {
        let Content::Prompt(cell) = &self.content else {
            return None;
        };
        if cell.kind == PromptKind::Approval {
            let note = (!self.text.trim().is_empty()).then(|| self.text.clone());
            self.sent = true;
            Some(PromptIntent::Approve {
                key: cell.key.clone(),
                choice: value,
                note,
```

The approval arm draws that field with the prefix `note: `, then the hint. REQ-1 changes the prefix only. The hint stays, including the middle dot.

```604:609:crates/talaria-tui/src/prompts/stack.rs
                if card.field || !card.text.is_empty() {
                    add_field(&mut rows, "note: ", &card.text, width);
                }
                format!(
                    "1-{} choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop",
                    choices.len()
                )
```

`add_field` keeps the trailing characters that fit and concatenates them onto the prefix. There is no extra separator. The trailing space in the REQ-1 prefix is what puts a space before the typed text.

```723:734:crates/talaria-tui/src/prompts/stack.rs
fn add_field(rows: &mut Vec<Line<'static>>, prefix: &str, text: &str, width: u16) {
    let room = usize::from(width).saturating_sub(prefix.len());
    let sanitized = sanitize(text);
    let mut suffix = String::new();
    for ch in sanitized.chars().rev() {
        let candidate = format!("{ch}{suffix}");
        if Line::from(candidate.as_str()).width() > room {
            break;
        }
        suffix = candidate;
    }
    add(rows, &format!("{prefix}{suffix}"), width);
}
```

The card tests compare a whole row. `draw` trims trailing spaces, which is why the expected row for typed `go 2 ` has no trailing space while the stored note keeps it. `has_row` is equality, not a substring check.

```226:236:crates/talaria-tui/tests/prompt_cards.rs
fn draw(stack: &PromptStack, width: u16, rows: u16) -> Vec<String> {
    let section = stack.section().expect("a card should be drawn");
    let height = section.height(width).min(rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, width, rows));
    section.render(Rect::new(0, 0, width, height), &mut buf);
    let mut out: Vec<String> = (0..rows)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
```

```255:256:crates/talaria-tui/tests/prompt_cards.rs
fn has_row(rows: &[String], wanted: &str) -> bool {
    rows.iter().any(|row| row == wanted)
```

The two rows REQ-1 names are asserted at `prompt_cards.rs:529` (`note: go 2`) and `prompt_cards.rs:1428` (`note: 1`). `standard_approval` offers `once`, `deny`, `session`, and `always` (`prompt_cards.rs:101-105`). No other Rust test asserts the string `note: `.

## From the stored note to prompt.submit

`PromptService::set_note` stores non-blank text and drops a blank note (`prompts.rs:232-238`). `execute` takes the note off the map before it looks at the reply (`prompts.rs:273`). On `PromptResponseResultStatus::Resolved` it emits one `Event::Submit` whose `text` and `display` are that note, whose `attachments` are empty, and whose `submission_id` comes from the service's closure. Notices are empty. The service does not call `prompt.submit`.

```281:290:crates/talaria-app/src/prompts.rs
                    events: note
                        .into_iter()
                        .map(|text| Event::Submit {
                            display: text.clone(),
                            text,
                            submission_id: (self.next_submission_id)(),
                            attachments: Vec::new(),
                        })
                        .collect(),
                    notices: Vec::new(),
```

Any other reply keeps the note out of `Event::Submit` and appends `; note not sent: ` through `with_note` (`prompts.rs:390-395`). The stems are `answered elsewhere or timed out` (`prompts.rs:298`) and `could not answer: {reason}` (`prompts.rs:307`). `note_becomes_submit_event_only_after_resolved` (`prompts_respond.rs:173`) locks both the resolved event and those notices. REQ-4 leaves that test green.

`approval_params` sends `session_id`, `execution_generation`, `prompt_id`, `choice`, and `profile: None` (`prompts.rs:373-380`). `deny_before_interrupt` removes any stored note and sends choice `deny` only (`prompts.rs:329-370`).

`UserAction::Approve` stores the note beside `Event::AnswerApproval` (`app.rs:6758-6767`). `route` puts that pair on `AppEffect::Respond` (`app.rs:6199-6203`). The executor calls `set_note` and then `execute` (`app.rs:12604-12610`). `prompt_task` returns the events as `AppMessage::PromptDone` (`app.rs:11053-11057`). The `PromptDone` arm reduces each event (`app.rs:4086-4099`).

`Event::Submit` has `text`, `display`, `submission_id`, and `attachments` (`reducer.rs:98-103`). It has no note flag. `submit` inserts a ledger row with `retried: false` and returns `Effect::SendPrompt` (`reducer.rs:230-270`, `flow.rs:48-59`). `route` turns that into `AppEffect::Send` (`app.rs:6192`).

## Why a first refusal drops the text

`LedgerEntry` (`queue/mod.rs:96-109`) has `text`, `admission_id`, `attachments`, `cell_drawn`, `awaiting_reconcile`, and `retried`. It has no origin flag. A composer submit and the note's follow-up are the same shape once they are in the ledger.

`submit_outcome` (`app.rs:9515`) offers a draft only when `retried` is true, and it reads the ledger before `Event::SubmitFailed`:

```9566:9590:crates/talaria-app/src/app.rs
    // A refused retry of a lost submit hands its text back (#232); the reducer forgets it.
    let returned = state.opened.as_ref().and_then(|opened| {
        opened
            .state
            .queue
            .ledger
            .get(&SubmissionId(id.clone()))
            .filter(|entry| entry.retried)
            .map(|entry| entry.text.clone())
    });
    let mut effects = reduce(
        state,
        Event::SubmitFailed {
            submission_id: id,
            reason: reason.to_string(),
        },
    );
    // ...
    effects.extend(returned.map(AppEffect::OfferDraft));
```

`Event::SubmitFailed` calls `submit_failed`, which removes the ledger row (`reducer.rs:104-117`, `flow.rs:63-72`). A first refusal therefore has nothing left to offer. The notice is separate: `prompt_refused_notice` (`failure.rs:364-374`) is `Hermes refused the prompt ({reason}).`, plus a reconnect sentence only when the reason is `permission_denied`. REQ-2 uses `runtime_draining`, so the notice has no reconnect sentence.

REQ-2 does not clone that ledger text for the note. `PromptDone` (`app.rs:4086-4099`) moves each event into `reduce`. The implementation copies the submission id and the text from `Event::Submit` before that move, into one new private map on `AppState`, empty in `AppState::new` (`app.rs:1046`). On `Refused`, the draft is that remembered text, including when `state.opened` has no row. A session switch stores the previous session in `retained_sessions` (`app.rs:9195-9205`). `submit_outcome` does not read `retained_sessions`. REQ-2 still offers the remembered text to the composer that is open then, and it does not check that the open session sent the note. The test does not switch sessions: its text is `go 2` on both the event and the opened ledger row. The test function runs blocks B, C, and D, then block A. Blocks B, C, and D pass on today's code. The new test fails on the block A `OfferDraft` assertion. `ExecuteOutcome::VoiceStopped` (`app.rs:9564`) is not the note path. A note follow-up is `prompt.submit`, and that arm does not read the new map.

`ExecuteOutcome::Sent` returns at `app.rs:9556` and does not offer a draft. `ExecuteOutcome::OutcomeUnknown` goes to `lost_submit` (`app.rs:9596-9613`), which reduces `Event::QueueCallUnknown`. That sets `awaiting_reconcile` on the existing ledger row (`flow.rs:932-943`) and does not offer a draft. REQ-3 leaves both arms in place.

`AppEffect::OfferDraft` is applied at `app.rs:12613` by `screen.offer_draft`. The trait default ignores the text (`app.rs:591-595`). The screen that loads a composer is `offer_draft` in `crates/talaria-cli/src/run.rs:4613-4616`: it loads only when `composer.is_empty()` and `editing` is `None`. REQ-2 forgets the id after the offer. When this function does not load the text, REQ-5 keeps no second copy. The loss named in the settled decision is today's refusal, which offers nothing. The hand-back is this offer. A different `OfferDraft` also exists when the queue's returned list is drained (`app.rs:4533-4539`). REQ-2 does not use that list. The note text is pushed directly.

The private map in REQ-2 is how `PromptDone`'s `Event::Submit` is told from a composer submit without a new public field. It holds the submission id and that event's text. Composer submit is `send_expanded` (`app.rs:8644-8651`). Rewind submit is `app.rs:2978-2986`. The fleet foreground reply is `app.rs:14456-14463`. Voice submit is `app.rs:16672-16679` and `app.rs:16740-16749`. None of those go through `PromptDone`.

## The peeked session

`PeekSession::answer` calls `set_note` and `execute`, then sends each note text through `PeekSession::reply` (`peek.rs:774-801`). `reply` builds its own `Event::Submit` and, on `ReplyOutcome::Refused`, applies `Event::SubmitFailed` (`peek.rs:497-547`). It does not call `AppEffect::OfferDraft`. The issue leaves this path alone. The label still changes for a peek card, because the card is drawn by the same approval arm in `stack.rs` and there is only one prefix. Peek already sends the note after the answer. Its refusal is not the follow-up this issue hands back.

## What the tests already lock

`refused_or_unknown_submit_tells_the_reducer_and_is_not_resent` (`session_flow.rs:1555`) opens with `new_request`, submits the composer text `hello`, and checks a refused `prompt.submit`. The wire keys it asserts are `session_id`, `submission_id`, and `text` (`session_flow.rs:1601-1603`). It does not assert the absence of `OfferDraft`. A note-only draft leaves it green. The mock default for `approval.respond` is `Reply::Resolved` (`crates/talaria-app/tests/support/mock_gateway.rs:724`).

This issue cites no Hermes file. The refusal it handles is `ExecuteOutcome::Refused { reason }` (`sessions.rs:125-129`), which `submit_outcome` already matches.

The implementation commit also adds one entry at the top of `docs/engineering-journal/LEARNINGS.md` and one at the top of `docs/engineering-journal/DECISIONS.md`. The issue's Notes section says what each entry contains. The newest `LEARNINGS.md` entry labels its third part Rule. The heading date is the calendar date of the second commit. The test commit does not touch them.
