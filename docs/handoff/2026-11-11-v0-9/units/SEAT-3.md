# SEAT-3: Report the seat state to the terminal, behind the existing report_state setting

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the call sites and the quotes. The requirements, tests, and acceptance criteria are only in the issue. This page refers to them by id.

## What exists today

`update` (`crates/talaria-app/src/app.rs:3282`) is the apply path. Inside it, a session change and the activity loop push `AppEffect::ReportPane`.

Session change, `app.rs:4492-4496`:

```4492:4496:crates/talaria-app/src/app.rs
            if let Some(pane) = out.pane {
                effects.push(AppEffect::ReportPane(pane));
            } else {
                effects.push(AppEffect::ReportPane(state.attention.pane()));
            }
```

Activity loop, `app.rs:4515-4518`:

```4515:4518:crates/talaria-app/src/app.rs
        let out = state.attention.observe(activity);
        if let Some(pane) = out.pane {
            effects.push(AppEffect::ReportPane(pane));
        }
```

`AttentionOutput` (`crates/talaria-core/src/attention.rs:152`) has `alert` and `pane` today. `pane` is set only when the pane state or its message changed. There is no `seat` field yet. [#533](https://github.com/infiquetra/talaria/issues/533) (`SEAT-1`) adds `seat: Option<SeatState>`, `Some` only when the seat state changed. A new tracker is `Idle`, so the first session is not a change. The session-change block still pushes `ReportSeat` with that state. Done to idle is a change. An idle-to-idle switch in the activity loop is not. `AttentionTracker::observe` is at `attention.rs:186`. `pane` is at `attention.rs:284`. `PaneState` is `Idle`, `Working`, and `Blocked` (`attention.rs:11`). There is no `Done` or `Error` on `PaneState`.

`NeedSummary` (`attention.rs:53`) is `Approval { command, description }`, `Question(String)`, `Sudo`, `Secret`, `VaultCode`, and `VaultUnlock`. An open approval prompt becomes `NeedSummary::Approval` in `comforts/mod.rs:31`. [#533](https://github.com/infiquetra/talaria/issues/533) (`SEAT-1`) encodes that need as `BlockKind::Permission`.

The session-change branch pushes `ReportPane` in both arms (`app.rs:4492-4496`). REQ-1 pushes `ReportSeat` after that if/else with the tracker's seat state whether or not `out.seat` is `Some`, when `seat_reporting` is true, and not from inside the `else` arm. The startup call at `app.rs:14743` stays a pane report only.

The executor match:

```13263:13269:crates/talaria-app/src/app.rs
            AppEffect::ReportPane(pane) => screen.report_pane(
                &pane,
                state
                    .opened
                    .as_ref()
                    .map(|o| (o.session_id.as_str(), state.profile.as_str())),
            ),
```

Startup also reports once:

```14743:14743:crates/talaria-app/src/app.rs
    screen.report_pane(&state.attention.pane(), None);
```

`Screen::report_pane` (`app.rs:605`) has an empty default body. `TuiScreen` (`crates/talaria-cli/src/run.rs:291`) implements it at line 4742: it stores the report and, if `self.reporter` is `Some`, calls `HerdrReporter::report`. The reporter is built at `run.rs:5187` with `config.herdr.report_state`. `AppState::new` (`app.rs:1046`) does not receive that config, and `run_inner` (`app.rs:14677`) has no config argument. `work_panel` (`app.rs:539`) is the pattern for a screen accessor. REQ-4 adds `seat_reporting` on `Screen` and copies it once in `run_inner`. `report_pane` on `TuiScreen` is at `run.rs:4742`, inside the impl whose header is line 3429. [#534](https://github.com/infiquetra/talaria/issues/534) (`SEAT-2`) provides `InlineTerminal::report_seat(&mut self, state: SeatState) -> io::Result<()>`. It writes nothing when the probe was not answered, so REQ-4's caller does not check the probe. `TuiScreen`'s fields are private and its terminal is `InlineTerminal<Stdout>` (`run.rs:291`), so a test cannot pass a `Vec<u8>`. REQ-4's method takes `SeatState` by value, matching `AppEffect::ReportSeat(SeatState)`.

`HerdrReporter::from_env_with_notices` (`crates/talaria-app/src/comforts/herdr.rs:45`) returns `None` when `enabled` is false or `HERDR_ENV` is not `1` (`herdr.rs:60`). `report` (`herdr.rs:90`) maps the three `PaneState` values to `idle`, `working`, and `blocked`, then runs `pane report-agent`. It writes no terminal bytes.

`HerdrConfig.report_state` (`crates/talaria-app/src/config.rs:209`) defaults to true (`config.rs:297`). The user file is `<HOME>/.talaria/config.toml` (`config.rs:1036`, read from `HOME` at `run.rs:4855`).

`docs/configuration.md:414` says: "Inside Herdr, `[herdr] report_state` reports blocked, working or idle." The sample at line 354 is `report_state = true` inside the single toml block. `config_doc.rs::doc_default_shape_loads_cleanly` requires that block to stay the only toml block and to load as `Config::default()`. The table row at line 869 is `report_state`, boolean, default true.

`docs/engineering-journal/DECISIONS.md` is newest-first. The top section is dated 2026-11-04 and uses `**Context.**` and `**Decision.**`.

No Rust file writes the bytes of Operating System Command 7501. A search of `7501` under `crates/**/*.rs` found no escape sequence.

## Tests that already cover this path

`crates/talaria-app/tests/attention_replay.rs`:

- `approval_fixture_drives_states_and_one_alert` replays `fixtures/frames/approval.jsonl` through `AttentionTracker` and expects pane states `Working`, `Blocked`, `Working`, `Idle`.
- `has_pane` matches `AppEffect::ReportPane`.
- `local_masked_answer_closes_need_through_update` calls `update` with `AppMessage::PromptDone` and a sudo `MaskedRequestInfo`, then `has_pane(..., PaneState::Blocked)`.

No file under `crates/talaria-cli/tests/` mentions `ReportPane` or `report_state`. This issue adds no test there.

`reduce` (`app.rs:6073`) applies a session event only when `state.opened` is set. `PromptDone` (`app.rs:4086`) stores `masked` when it is `Some`, then calls `reduce` for each event. `attention_snapshot` (`app.rs:4665`) reads the opened session. An approval opens only if the session is already open and the events include `approval.request` (`fixtures/frames/approval.jsonl` has one at the line whose `type` is `approval.request`). A second `PromptDone` with empty events, empty notices, and `masked: None` does not change that snapshot.

Other `Screen` implementations, which a default method body leaves untouched: `Editor` in `app.rs:10187`, `Probe` in `crates/talaria-app/tests/session_flow.rs:298`, `Probe` in `crates/talaria-app/tests/support/directory_drive.rs:104`, and `Probe` in `crates/talaria-cli/tests/live_session_picker.rs:109`.

## Why the tests fail first

`AppEffect::ReportSeat` and `Screen::report_seat` are not in the tree. The three new tests name them, so the test commit fails to compile. After the names exist, `approval_opening_reports_blocked_permission` fails if the blocked effect is missing, `unchanged_seat_state_reports_nothing` fails if the second update still pushes `ReportSeat`, and `report_state_false_pushes_no_report_seat` fails if a false setting still pushes one.

## Checks

The project check is `tools/prepush.sh`, provided by SEAT-0 and on `main` before this issue starts. The issue's verification block runs the unit tests and then that script.

## Docs and the journal

REQ-6 replaces the sentence at `configuration.md:414` and adds one sentence under the `herdr` heading. It does not add a second toml fence and does not change line 354. REQ-7 inserts two sections above `## 2026-11-04`.
