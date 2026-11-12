# SYNC-2: Tell the operator once when the gateway is newer than tested or older than supported

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for REQ-1 through REQ-5 and the two doctor tests. This page maps the version the client already stores and the notice path the sentence uses. It does not restate the requirements.

## What exists today

`compare_hermes_version` in `crates/talaria-core/src/doctor.rs` takes `code_version`, `code_sha`, and a `Pin`. It returns `Pinned` when the sha is a prefix of the pin commit, before it looks at the version number. Otherwise it parses a three-part version and does not strip a leading `v` from `code_version`. `MIN_HERMES` is `(0, 22, 3)`. Below that the verdict is `TooOld`, and the doctor summary is `{v}; Talaria needs 0.22.3 or later`. `NewerThanPin` is a warning. Those pin verdicts stay. The floor cutoff moves to `OLDEST_SUPPORTED`, which [#554](https://github.com/infiquetra/talaria/issues/554) (`PROOF-1`) adds as `pub const OLDEST_SUPPORTED: (u64, u64, u64)` with value `(0, 23, 0)`. The new summary is `{v}; Talaria needs 0.23.0 or later`, with no leading `v` and not the tuple's debug form.

The pin in the tree this unit builds is Hermes v0.23.1, from [#544](https://github.com/infiquetra/talaria/issues/544) (`PIN-3`). This unit does not move it. A version below `(0, 23, 0)` that is not a commit-prefix match is `TooOld`, including a version that used to be `OlderThanPin`, `NewerThanPin`, or a same-release verdict only because it equalled the old pin tag. The function still returns `OlderThanPin` for a parsed version at or above the floor and below the pin tag. The table has no such row, and this unit does not add one.

`Endpoint::from_identify` in `crates/hermes-client/src/discovery.rs` copies `code_version` from the `identify` result. `code_version()` returns it. The websocket handshake calls `client.capabilities` with `server_requests: true` and then `runtime.describe`. `RuntimeDescribeResult` has no `code_version` field. `ConnectionService::open` in `crates/talaria-app/src/connection.rs` holds that `Endpoint`, then moves it into `LaunchDial`. `OpenConnection` does not carry the version until this unit adds `gateway_version: Option<String>`. `Dialed` and `LinkState` do not gain a field. `LinkState::Connected` is `Connected { generation }`.

`run_inner` calls `install`, and `app::run` calls `run_inner`. `install` must copy `gateway_version` onto `AppState` before it drops the rest of `OpenConnection`. `theme_command.rs` and `workflow_progress_settlement.rs` call `app::run` against the mock. `doctor_service.rs` sets the mock revision to the pin commit, so that doctor row stays `Pinned`. `link_event` handles `LinkEvent::State(LinkState::Connected { generation })` and does not read a gateway version today. A conversation notice is `Event::Notice { kind, text }`. The session reducer inserts a `NoticeCell` and returns no effect. `NoticeKind::Warning` is the existing kind for a caution. A reconnect keeps the same `OpenConnection`, so the stored version stays the opening one.

`version_verdicts` in `crates/talaria-core/tests/doctor_diagnose.rs` builds `pin()` from `TAG` and `COMMIT`. Before [#543](https://github.com/infiquetra/talaria/issues/543) (`PIN-2`) rewrites baked pin literals, `0.22.3` with `ffffffff` is `OlderThanPin`, `0.22.4` with `ffffffff`, with no sha, or with the first six characters of the pin commit is a same-release verdict, and `0.22.10` is `NewerThanPin`. Six characters are not a prefix match. Those rows move to `TooOld`. Rows [#543](https://github.com/infiquetra/talaria/issues/543) (`PIN-2`) already derives from the pin tag, at or above v0.23.1, stay, including that six-character row as `SameReleaseOtherCommit`. A prefix of at least seven characters stays `Pinned`. The details assertion at the end of that test is [#543](https://github.com/infiquetra/talaria/issues/543) (`PIN-2`)'s. It follows `TAG` and `COMMIT`. This unit does not edit it.

`hermes_contract::compat::CURRENT` does not exist until [#548](https://github.com/infiquetra/talaria/issues/548) (`SYNC-1`). It is `pub const CURRENT: CompatRow`. This unit only reads it, as `&hermes_contract::compat::CURRENT`.

The ensure-failure sentence `Update Hermes to v0.22.3 or later and restart its gateway.` in `discovery.rs`, and the copy in `connect.rs`, is the next step for Hermes exit code 3. This unit does not change it.

## Why the notice is not the doctor row

The doctor compares the gateway with the pin and can say newer-than-pin while the gateway is still inside the tested range. The notice compares with `newest_tested` and `floor` on `CompatRow`, which are strings. A gateway newer than the pin but not newer than `newest_tested` gets no notice. A gateway older than the pin but not older than the floor gets no notice. Both comparisons stay in the tree. They answer different questions.

## Edge cases

| Input | Notice | Doctor verdict |
| --- | --- | --- |
| Version inside the floor and the newest tested tag, inclusive | none | unchanged |
| Version above `newest_tested` | the newer sentence, once | `NewerThanPin` only when it is also above the pin |
| Version below the floor | the older sentence, once. The notice does not see the sha | `TooOld` when the sha is not a prefix of at least seven characters; `Pinned` when it is |
| `row.floor` or `row.newest_tested` is not three numbers | none, and `shown` stays unchanged | not involved |
| Missing version, or not three numbers | none | `Unknown`, as today |
| Second connect in the same process | none | not involved |
| `shown` already true | none, and `shown` stays true | not involved |
