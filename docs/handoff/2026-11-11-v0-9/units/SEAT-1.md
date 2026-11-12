# SEAT-1: Seat state in the domain core: idle, working, blocked with a kind, done and error

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today and for why the seat state is separate from the pane report. The requirements, the tests and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`talaria-core` does not import a terminal library. `crates/talaria-core/tests/layering.rs::domain_crates_never_reach_a_terminal_library` walks the workspace graph and fails if a domain crate can reach `ratatui` or `crossterm` (`layering.rs:344-356`). REQ-9 and AC-4 keep that true.

The foreground tracker is `AttentionTracker` in `crates/talaria-core/src/attention.rs`. It is constructed with `AttentionTracker::new` (`attention.rs:173`) and holds a config, the open needs, a running-turn instant, and the session label (`attention.rs:161-168`). It does not remember how the last turn ended. A new tracker therefore cannot tell "never finished" from "finished normally".

`observe` (`attention.rs:186`) is the only mutator. It matches `Activity` (`attention.rs:75-111`):

- `TurnStarted` records the start instant if one is not already stored (`attention.rs:189-191`).
- `TurnEnded` runs only when a start instant exists (`attention.rs:193`). `TurnOutcome::Failed` selects `AlertKind::Failed`. `TurnOutcome::Complete` selects `AlertKind::Done` only when the turn lasted at least `done_after`. `Interrupted`, and a complete turn shorter than that, produce no alert (`attention.rs:194-201`). That is why REQ-5 separates the seat state from the done alert, and why test 6 asserts `alert` is `None` on a one-second complete turn.
- `NeedOpened` appends the need if its key is new (`attention.rs:217-218`).
- `NeedClosed` drops that key.
- `SessionChanged` clears needs and the running turn only when the session id differs (`attention.rs:235-237`). A same-id change updates the label only. REQ-6 follows that split.

The value returned today is `AttentionOutput` with `alert` and `pane` (`attention.rs:152-157`). `pane` is `Some` only when `pane()` changed (`attention.rs:276-279`). No other file builds an `AttentionOutput` by hand, so adding `seat` does not force edits outside `attention.rs`.

`pane()` (`attention.rs:284-298`) uses the last open need, not the first:

```rust
match self.needs.last() {
    Some((_, summary)) => PaneReport { state: PaneState::Blocked, ... },
    None => PaneReport { state: if self.started.is_some() { Working } else { Idle }, ... },
}
```

Settled decision 1 says the seat kind comes from the first open need. REQ-3 therefore reads `needs` from the front. `pane()` stays on `needs.last()`. Test 5 is the case that would pass incorrectly if someone copied `pane()`.

`PaneState` is only `Idle`, `Working` and `Blocked` (`attention.rs:11-18`). There is no done or error on that enum. `AlertKind` already has `Done` and `Failed` (`attention.rs:22-28`). Those alert kinds are not the seat state.

`NeedSummary` has six shapes (`attention.rs:53-71`): `Approval`, `Question`, `Sudo`, `Secret`, `VaultCode`, `VaultUnlock`. Test 4 maps those six onto `BlockKind`.

`crates/talaria-core/src/lib.rs` lists `pub mod attention` at line 6. It does not yet list `seat_status`. REQ-1 adds that module and a crate-root re-export.

`crates/talaria-core/tests/attention.rs` builds activities with helpers `all_channels`, `prompt_key`, `approval` and `opened` (`attention.rs` test file lines 15-46). The new file should follow that shape. `interrupted_unseen_start_and_stray_end` (`attention.rs` test file line 186) and `turn_end_with_open_need_stays_blocked` (line 428) already pin the alert and pane behaviour that REQ-8 must leave alone.

## Why the output field is `seat`

`pane` means "here is the new pane report". `seat` means "here is the new seat state". Both are `Option` and both are `None` when nothing changed. REQ-8 names the field `seat` so a later caller can match on it the same way it matches on `pane`. The existing match arms for alerts and pane reports do not gain a new meaning.

`Blocked(None)` is on the enum because a later writer must be able to say "blocked, and this terminal did not list the kind" without a second type. `seat_state` never returns that value (REQ-3). The tests assert `Some(kind)` only.

## Edge cases the tests pin

| Situation | `seat_state` | Why |
| --- | --- | --- |
| New tracker | `Idle` | No turn has finished (settled decision 1). Test 1. |
| `TurnStarted` | `Working` | REQ-4. Test 2. |
| Need open during a turn | `Blocked(Some(_))` | Blocked wins. Test 3. |
| Six need shapes | the kinds in test 4 | REQ-3. |
| Two needs, question first | `Question` until that need closes | First, not last. Test 5. |
| `Complete` shorter than `done_after` | `Done`, no done alert | REQ-5. Test 6. |
| `Failed` | `Error`, failed alert still emitted | REQ-5 and REQ-8. Test 7. |
| `Interrupted` | `Idle` | Settled decision 1. Test 8. |
| Different session id | `Idle` | REQ-6. Test 9. |
| Same session id, new label | unchanged | REQ-6. Test 9. |
| `TurnEnded` with no running turn | unchanged | REQ-7. Test 10. |
| Each of three started turns ends while a need is open (`Complete`, then `Failed`, then `Interrupted`) | stays blocked; after the need closes, the last end (`Interrupted`) applies | REQ-3 and REQ-7. Test 11. |

A keystroke is not an `Activity` (`attention.rs:75-111`). Nothing in `observe` can move `Done` because the operator typed. That is the whole of "typing does not change it".
