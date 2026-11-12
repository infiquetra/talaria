# SIG-2: Show the current Hermes notice on the status row, with a count of warnings

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the status row and the notice cell that exist today. The requirements, the tests, and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`plan` (`crates/talaria-tui/src/status/segments.rs:260`) builds a `LinePlan` whose `pieces` are `(Segment, String, Tone)`. `Segment::ALL` has 14 values and `Ultracode` is last (`segments.rs:162-177`). `StatusInputs` (`segments.rs:43-98`) has `approvals: Option<ApprovalsSegment>` and no notice field. `StatusSection::render_into` (`segments.rs:634`) draws `plan(...).pieces` only. REQ-5 adds `notice` and `warnings` on `LinePlan` and does not put the notice into `pieces`, because every `pieces` entry needs a `Segment`. REQ-7 subtracts their width from the budget before pieces are dropped, and joins them with `SEPARATOR`. REQ-6 runs the notice text through `sanitize`. REQ-8 refreshes the fleet count from `reduce`, including the return at `app.rs:6076` when no session is open, from the tick, and from `open_fleet_view`. The count is `opened.state.notices().warning_count()`. `OpenedSession` has no such field. REQ-6 maps the level with `notice_tone`. `plan` sees the tone only. REQ-9 clones the previous row before `apply_show`. `cut_to_width` (`segments.rs:589`) always ends with `ELLIPSIS` (`segments.rs:19`), so REQ-6 cannot call it for a text that already fits. `status.rs` re-exports `StatusInputs` and does not yet re-export `NoticeLine`. `examples/status_demo.rs` `full` (`status_demo.rs:43`) names every `StatusInputs` field.

`status_inputs` (`crates/talaria-cli/src/run.rs:1222`) fills `StatusInputs` from `View`. It does not read a notice store.

`SessionState::notice` (`crates/talaria-core/src/session/state.rs:362`) appends one final `NoticeCell`. `NoticeKind` (`cell.rs:146`) is `Interrupted`, `Error`, `Warning`, `Automation`, `Diagnostic`, and `System`. REQ-9 uses `Error`, `Warning`, and `System`.

`header_text` (`crates/talaria-tui/src/fleet_view/rows.rs:133`) puts `link_note` in `Header.note` when it fits (`rows.rs:144-151`). REQ-8 uses that slot for a non-zero count and leaves it alone at 0, so the fleet screen tests that pass a link note and no count stay green.

`AppMessage::Tick { now_ms }` sets `state.now_ms` (`crates/talaria-app/src/app.rs:4227-4228`). The foreground session is `OpenedSession.state` (`crates/talaria-app/src/sessions.rs:34-35`). REQ-3 expires there. `run_inner` (`app.rs:14669`) computes the next wake with `earliest_of` (`app.rs:14600`) at `app.rs:14819-14866`, and applies a tick only from the condition at `app.rs:14911-14913`. REQ-3 adds the earliest notice expiry to both.

`FleetViewInput::Snapshot` is constructed in many tests. REQ-8 does not add a field to it. `set_warning_count` is a method on the state after the snapshot is applied.

`AlertKind` is `NeedsYou`, `Done`, and `Failed` (`crates/talaria-core/src/attention.rs:22-28`). REQ-10 does not add a variant. `pr_segment_priority` (`crates/talaria-tui/tests/status_pr_segment.rs:8`) asserts the drop order. REQ-5 leaves that order alone.

The store names REQ-1 copies are the ones `SIG-1` publishes. This page does not restate their replace and expire rules.

## The timed-notice wake

A timed notice must expire when nothing else is due, and the run loop applies a tick only when one of its deadlines is due (`app.rs:14911-14913`). `SIG-1`'s store stamps a row's start at the first `expire` call that sees it, so the row also needs one tick soon after it is shown. Add two things.

First, `NoticeStore::next_deadline(&self) -> Option<u64>` (new, in `notices.rs`). It reads the store and changes no rule. It returns the earliest time at which `expire` has work to do: 0 for a row that expires and has no start stamp yet, otherwise that row's start plus its `ttl_ms`, and `None` when no row expires. The store keeps each expiring row's start stamp privately (`SIG-1`, REQ-4); `next_deadline` is written inside the store, so it reads that stamp directly. `SessionState::next_notice_deadline(&self) -> Option<u64>` (new) returns it.

Second, `notice_wake(state: &AppState, executor: &Executor) -> Option<Instant>` (new) beside `background_wake` (`app.rs:11251`) and built the same way: `None` when no session is open or the session is gone, otherwise that deadline converted with `executor.started.checked_add`. In `run_inner`, compute `notice_due` beside `background_due` (`app.rs:14816`), add it to the deadline with `earliest_of` as `queue_due` is added (`app.rs:14866`), and add `notice_due.is_some_and(|due| due <= now)` to the condition at `app.rs:14911`.

The first tick stamps the row, and the tick at the stamp plus `ttl_ms` removes it. A deadline of 0 is in the past, so the loop wakes at once; after that tick the deadline is in the future, so the loop does not spin. No unit test drives the run loop; the reviewer checks these three edits against this section. Test 10 in the issue covers the deadline.
