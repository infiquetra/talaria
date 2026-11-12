# FIX-5: A goal first seen already finished gives no completion notice

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for REQ-1 through REQ-4 and the new reducer test. This page maps the goal path those tests stand on. It does not restate the requirements.

## What exists today

`AutomationEvent::Attached` clears `goal_progress` and sets `control` to `ControlStatus::Reading` (`crates/talaria-core/src/automation/state.rs:601-614`). `view` returns the held view only when `control` is `Ready` (`state.rs:353-357`). The first `show` therefore runs while no view is held.

`show` returns without looking at the goal when the new revision equals the held one (`state.rs:380-386`). Otherwise it calls `observe_goal` and then stores `ControlStatus::Ready` (`state.rs:387-398`).

`observe_goal` (`state.rs:401`) clears `goal_progress` when the view has no goal (`state.rs:402-405`). Otherwise it treats a goal as continuing when `created_ms` and `title` match the held goal and the held goal was shown or the new one is not (`state.rs:408-412`). The first sight is not continuing. It stores `completion_noted: !visible` and returns `None` (`state.rs:413-421`). `shown` is true only for status `active` or `paused` (`state.rs:455-457`). A goal that is already `done` is not shown, so that first sight is stored as already noted. The shown-to-finished path sets `completion_noted` true when it returns the sentence (`state.rs:424-428`).

A later change from shown to not shown, when `completion_noted` is false, returns `goal_completion_text` (`state.rs:424-428`). That sentence is built at `state.rs:459-466`.

`ControlRead` calls `show` with the decoded view when the token matches (`state.rs:649-659`). `show` both checks the goal and stores the view. A failed read sets `Unavailable` or `NotRead` and does not call `show` (`state.rs:661-669`), so `view()` stays empty. `ControlUpdated` decodes the payload, clears `pending_control`, and applies the view for the foreground session (`state.rs:678-688`). A `ControlRead` whose token no longer matches returns without applying the view (`state.rs:650-652`). Both success paths call `show`. Attached is the only place that starts a control read (`state.rs:612-616`). `AutomationState` (`state.rs:297`) has no field that records whether that read succeeded, and none that keeps a finished goal's `created_ms` and `title` after `goal_progress` is cleared. `goal_progress` and `pending_goal_completion` are the goal fields beside it (`state.rs:306-307`).

`goal_completion_already_done_at_attach_is_silent` (`crates/talaria-core/tests/automation_reduce.rs:446`) feeds a done goal as that first view, once as `ControlRead` and once as `ControlUpdated`, and expects no notice. `goal_completion_snapshot_emits_verdict_reason_and_turns_once` (`automation_reduce.rs:367`) covers `active` or `paused`, then `done`, and a replay of the same snapshot.

`goal_completion_text` is private to `state.rs`. The tests look for parts of the sentence. `goal_completion_snapshot` (`automation_reduce.rs`) sets the title `Write the release notes`, status, verdict `done`, and the reason `Release notes reviewed and shipped`.

The recorded replay `recorded_shared_gateway_goal_completion_writes_one_notice` (`crates/talaria-app/tests/automation_replay.rs:25`) reads `fixtures/frames/goal-completion-shared.jsonl`. That file's first goal update is `active` and a later one is `done`.
