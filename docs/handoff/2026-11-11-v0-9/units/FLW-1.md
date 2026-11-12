# FLW-1: Fork from a chosen prompt in the rewind picker

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the rewind actions screen and the branch call that exist today. The requirements, the tests and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`/rewind` is a local command (`crates/talaria-core/src/commands/local.rs`). The overlay state is `RewindState` in `crates/talaria-core/src/rewind/state.rs`. `Call::Mutate` (`state.rs:24-29`) is the conversation rewind. It carries `row_id`, `expected_revision`, and `expected_generation`. There is no branch call.

`RewindKey` (`state.rs:85`) includes `C`, `F`, and `D`. `actions_key` (`state.rs:967`) maps `C` to a conversation confirm (`state.rs:976`) and `F` to a file confirm (`state.rs:981`). `fill_actions` (`state.rs:528`) draws `ACTION_REWIND` (`text.rs:789`) and then the files line (`state.rs:543-556`).

The screen mapping is `crates/talaria-tui/src/rewind.rs`. On `RewindScreen::Actions`, `c`, `f`, and `d` map to those keys (`rewind.rs:142-146`). The turn list (`rewind.rs:134-136`) does not map `b` or `f`.

`RewindTurn` (`crates/talaria-core/src/rewind/turns.rs:7-29`) stores the prompt's own `row_id`, `prompt_text`, `text_parts`, and `attachments`. `rewind_turns` (`turns.rs:47`) skips archived rows and rows with no integer `row_id` (`turns.rs:51-52`). `turn_from_row` (`turns.rs:153`) skips a `row_id` that is not greater than 0 (`turns.rs:162`). `set_loaded` (`state.rs:767`) keeps `running` and `pending` only (`state.rs:317-320`). It does not keep `messages`. `actions_key` (`state.rs:967`) sees `chosen` and `turns`. The only other `RewindTurn` literal is `rewind_checkpoints.rs:8`. `PROMPT_GONE` is `text.rs:134`.

`branch_request` (`crates/talaria-app/src/switch.rs:248`) takes a title and builds operation `"branch"` with that title only (`switch.rs:268-271`). `branch_foreground` (`switch.rs:214`) calls it. `open_branch_child` (`switch.rs:299`) reads `branched_session_id` and detaches the parent after the child opens (`switch.rs:321-324`). It does not set `AppState.draft` (`crates/talaria-app/src/app.rs:862`).

`RewindService` (`rewind.rs:31-35`) holds a client and a session id. `execute` (`rewind.rs:49`) matches `Call` and returns `RewindInput::Reply`. The app spawns that task from `AppEffect::Rewind` (`app.rs:12289`). `update` (`app.rs:3282`) returns effects and does not run private `apply` (`app.rs:11546`). `AppEffect::BranchSession` (`app.rs:2290`) carries only `request`. `start_branch` builds it at `app.rs:7587`. The fence retry builds it again at `app.rs:7902`. `/branch` is `start_branch` (`app.rs:7582`), then `AppEffect::BranchSession`, then `branched` (`app.rs:7864`). [#561](https://github.com/infiquetra/talaria/issues/561) (`TRU-3`) provides `AppMessage::SessionCleared::Started { install, resume_previous }`. Its `cwd` is `folder.cwd` when set, otherwise `launch_folder` (`app.rs:897`). The resume sentence is `The previous session was not ended; resume it with talaria --resume <id>`, omitted when `session_gone` is set. `branch_refusal` is `switch.rs:280`. `CallKind` (`text.rs:7`) has no branch variant. `call` (`state.rs:731`) matches every `Call`.

`/branch` is `branch_command` (`app.rs:7564`). Tests call `branch_request(&mut app, None)` at `crates/talaria-app/tests/mutate_wire.rs:510` and `:523`.

`rewind_input` (`app.rs:2945`) turns each `RewindEffect::Call` into `AppEffect::Rewind` (`app.rs:2954-2960`). It first calls `bind_rewind_session` (`app.rs:2946-2950`), which resets the overlay with `change_session` (`state.rs:406`) when the foreground session is not the overlay's owner. `apply` runs the effects `update` returns in a plain `for` loop (`app.rs:11599`). The `AppEffect::Rewind` arm (`app.rs:12289`) only checks that the owner is still in front because it runs a task later. `start_branch` commits a `branch_request` refusal with `AppEffect::CommitLines` (`app.rs:7589-7591`). `branch_refusal` takes a `MutationOutcome` and is used only after a reply. `load_draft` (`app.rs:6977`) sets `draft` and emits `AppEffect::LoadDraft`. `return_prompt` (`app.rs:2996`) keeps a non-empty draft, but its notices speak of a removed prompt, so the fork does not use it.

## Hermes facts

At Hermes v0.22.4, `hermes_state_mutations.py:67-68` accepts `through_message_id` on operation `branch` when it is an int greater than 0. At Hermes v0.23.0, `canonicalRewindPicker.ts` has `storedRowId` (lines 19 to 20, the same row rule as REQ-3) and `forkFrom` (line 156). The oldest prompt calls `newSession` (line 166). Otherwise it sends operation `branch` with `through_message_id` (line 176). The picker file is absent at Hermes v0.22.4, so it is not part of the pin check.

## Where the prompt text comes from

`prompt_text` is `None` when a part is not text (`turns.rs:15`). `text_parts` is the joined text (`turns.rs:17`). `attachments` is true in that case (`turns.rs:19`). REQ-4 chooses which string is prefilled. The composer field is `draft`.

## What this page does not decide

[#561](https://github.com/infiquetra/talaria/issues/561) (`TRU-3`) adds `AppEffect::ClearSession { skip_detach: bool, cwd: PathBuf }`. REQ-7 does not change it: the composer text waits in `AppState.fork_prefill`. `return_prompt` (`app.rs:2996`) never overwrites a draft: with a draft in the composer it commits `draft_kept` (`crates/talaria-core/src/rewind/text.rs:326`) and loads nothing. The fork follows the same rule with its own sentence. `install_session` does not clear `AppState.draft`.
