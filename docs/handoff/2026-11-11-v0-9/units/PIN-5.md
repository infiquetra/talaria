# PIN-5: Regression tests for a started prompt.cancel in print mode

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`HeadlessEffect::Cancel` is sent as `prompt.cancel` and `HeadlessEffect::Interrupt` as `session.interrupt` (`crates/talaria-app/src/headless/service.rs:475-484`).

`answer_prompts` pushes `Cancel` when a foreign prompt arrives, the run is not started, the phase is `Waiting`, and cancel has not been sent (`run.rs:674-676`). `stop_waiting` pushes `Interrupt` and `StartSettleTimer` when `started` is already true and a generation is known (`run.rs:934-944`). It pushes `Cancel` only when `started` is false and cancel has not been sent (`run.rs:950-952`).

`cancelled` (`run.rs:699`) handles the reply. A `status` of `started` sets `started` and, when a stop is in progress, returns only `StartSettleTimer` (`run.rs:710-713`). When no stop is in progress it returns an empty list. A `stale_generation` refusal during a stop calls `stop_waiting` again (`run.rs:715-717`).

The inline test `foreign_prompt_while_queued_cancels_ours` (`run.rs:1263`) already asserts that the foreign event's effects are exactly `Cancel`. It does not feed a `started` receipt. The new tests do. They construct `HeadlessInput` in `headless_flow.rs` because `opened`, `admitted` and `event` at `run.rs:1084-1110` are private to that file's tests.

`headless_receipt` (`mock_gateway.rs:4169`) returns `status: terminal`. The new tests do not go through that mock. They pass `CallReply::Ok` with `status: started` straight into `HeadlessRun::step`.

`timeout_while_queued_cancels_without_interrupt` and `sigterm_interrupts_own_turn_only` (`headless_flow.rs:145-157`) stay the queued and known-started cases.

## Tests

Both new tests pass on the first run. `cancelled` already returns the effects they assert. Decision 2 in the issue wins over the fail-first line and the second-commit line: one commit holds the tests, and `cancelled` is not edited. `HeadlessRun::new` takes a `String`. The reply values are the JSON objects in the issue. `cancelled` reads `status` and `execution_generation` from that value and does not decode an admission schema. Test 1's admission is `queued` when the stop is sent.
