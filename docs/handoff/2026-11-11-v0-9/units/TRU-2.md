# TRU-2: A foreground alert that the shared gate drops still silences the next one

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the two clocks that exist today and for the edge cases around a dropped alert. The requirements, the tests, and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`AlertGate::admit` (`crates/talaria-core/src/alert_gate.rs:29`) returns `None` before it stores `self.last` when the new alert is inside the window and no more urgent than the last one (`alert_gate.rs:30-36`). The type comment says a dropped alert does not move the window (`alert_gate.rs:16`). `urgency` there is needs-you, then failed, then done (`alert_gate.rs:8-12`).

The window constant is `GATE_WINDOW` (`crates/talaria-app/src/fleet_attention.rs:14`), two seconds. `AttentionHub::new` (`fleet_attention.rs:148`) builds `AlertGate::new(GATE_WINDOW)` (`fleet_attention.rs:155`). `on_foreground_alert` (`fleet_attention.rs:208-209`) is `self.gate.admit(alert, at)`.

`AttentionTracker` (`crates/talaria-core/src/attention.rs:161`) stores `last_alert` (`attention.rs:166`). `observe` (`attention.rs:186`) applies the same two-second check with its own `urgency` (`attention.rs:251-255` and `attention.rs:303-308`). On an alert it would return, it records first:

```rust
self.last_alert = Some((at, kind));
```

That assignment is `attention.rs:257`. The returned `Alert` (`attention.rs:128-139`) has no time field. `AttentionOutput` today is `alert` and `pane` (`attention.rs:152-157`). There is no `seat` field on this tree. `SEAT-1` adds `AttentionOutput.seat: Option<SeatState>` and `seat_state` before this unit starts. REQ-4 says to keep both.

The join is inside `update` (`crates/talaria-app/src/app.rs:3282`). For each foreground activity it calls `observe` (`app.rs:4515`). A pane change becomes `AppEffect::ReportPane` (`app.rs:4516-4518`). An alert is then offered to `state.hub.on_foreground_alert(alert, state.attention_at)` (`app.rs:4519-4522`). Only `Some` becomes `AppEffect::Alert` (`app.rs:4524`). The tracker has already stored `last_alert`. A hub drop does not clear it.

`state.attention_at` is the instant `activities_between` stamps onto those activities (`app.rs:4509-4513`). `apply` sets `attention_at` to `Instant::now()` once per message (`app.rs:11552`). On this path the activity time and the gate time are the same value. REQ-3 records that value, not a second call to `Instant::now()`.

`last_alert` is private. The only write is line 257. `crates/talaria-app` and `crates/talaria-core/tests` are separate crates, so the caller cannot assign the field. REQ-1 is the public method that writes it. No other unit's ledger row names that method.

The test that locks the tracker window is `rate_limit_keeps_the_more_urgent_alert` (`crates/talaria-core/tests/attention.rs:336`). It calls `observe` only. Its assertions include three needs inside the window ringing once, a need past two seconds ringing again, a need beating an earlier done, and a done never beating an earlier need (`attention.rs:348-398`). Those assertions read `out.alert`. They stay true only if a time was recorded after each accepted alert. That test is the caller in its own file, which is why the issue adds `record_admitted` there and changes no assertion.

`needs_you_alerts_once_per_prompt_key` (`attention.rs:92`) refuses a repeated key because the key is remembered (`attention.rs:220-227`), before the window check. A new key in that test is three seconds later, past the window. It does not need `record_admitted` to keep passing.

`cross_source_rate_limit_and_urgency` (`crates/talaria-core/tests/alert_gate.rs:30`) drives `AlertGate` alone. The drop block is lines 65-76. This unit does not change that file.

`foreground_and_fleet_alerts_share_one_gate` (`crates/talaria-app/tests/attention_fleet.rs:545`) admits a foreground alert, then a fleet alert one second later, and expects one kept alert. It does not call `observe`.

`accepted_title_updates_attention_label` (`app.rs:15673`) calls `observe` for one done alert and checks the text. It does not send a second alert inside two seconds.

## Edge cases

| What happens | `observe` | Gate | `last_alert` after the caller runs |
| --- | --- | --- | --- |
| First alert, gate returns it | `Some` | `Some` | recorded at the gate's instant |
| Same or lower urgency inside two seconds of a recorded alert | `None`, gate not called | unchanged | unchanged |
| `observe` returns an alert and the gate drops it | `Some` | `None` | not recorded |
| Needs-you inside two seconds of a recorded done | `Some` (urgency 2 is not less than or equal to 0) | `Some` if the gate's last is done | recorded |
| Same prompt key again | `None` from the remembered key | not called | unchanged |
| Restored need, short done turn, interrupted turn | `None` before the window check | not called | unchanged |

Test 2 locks the third row, then the fourth row on a dropped done. Test 1 locks the second and fourth rows for a caller that records every alert it accepted. The gate test locks the gate column of the third row. A new test that only repeated the gate test would pass before this change, so the issue does not add one.

## Why the time moves to the caller

Settled decision 1 on this issue keeps both two-second checks and changes only when the time is recorded. Deleting the tracker's check would make `rate_limit_keeps_the_more_urgent_alert` fail its assertions even after the caller recorded. Leaving the assignment at line 257 leaves the bug: the next same-urgency alert is judged from the dropped alert's time.

The gate does not know about `last_alert`, and the tracker does not know what the gate dropped. The caller in `update` is the only place that sees both results. Recording only on `Some` makes a dropped foreground alert leave the tracker's window where it was. A fleet alert that moved the gate still does not by itself write `last_alert`. The next foreground alert can be returned by `observe` and dropped by the gate, which is the third row above.
