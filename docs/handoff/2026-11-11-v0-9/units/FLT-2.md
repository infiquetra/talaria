# FLT-2: Remove old Herdr support from Talaria: the old state reporter and the Herdr-pane fleet backend

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`HerdrReporter::report` (`crates/talaria-app/src/comforts/herdr.rs:90`) builds `herdr pane report-agent` with `--state` (`herdr.rs:97`). `release` (`herdr.rs:141`) builds `herdr pane release-agent`. `TuiScreen` stores `Option<HerdrReporter>` (`crates/talaria-cli/src/run.rs:309`) and constructs it from `config.herdr.report_state` (`run.rs:5187`). `report_pane` (`run.rs:4742`) calls `reporter.report`.

`HerdrFleetBackend` (`crates/talaria-app/src/herdr/service.rs:35`) polls panes and calls `FleetReporter::observe_herdr` (`fleet/service.rs:336`, from `herdr/service.rs:234` and `:321`). `FleetRegistry::observe_herdr` (`crates/talaria-core/src/fleet/herdr.rs:148`) feeds `FleetInput::Herdr` (`registry.rs:191`), whose arm is `registry.rs:1024`. `project` (`herdr.rs:80`) builds `HerdrProjection` and stores `FleetBackend::Herdr` (`herdr.rs:118`). `FleetBackend` is `external.rs:6`. Match arms are `fleet_view/state.rs:1208` and `:1264`, `fleet_view/detail.rs:29` and `:41`, `fleet_attention.rs:290`, `fleet_cmd.rs:410`, `:422` and `:579`, and `app.rs:6467`, `:14123` and `:14197`. `AppEffect::Herdr` is `app.rs:2150`. The in-file test `terminal_actions_keep_their_owner_alongside_acp_sessions` (`app.rs:16446`) asserts that route. `list_once_with_herdr` keeps its name. `docs/configuration.md` lines 416-422 include the `talaria herdr panes` sentence, which stays, and lines 782-784 name a Herdr reply path, which goes. `live_comforts.rs` reads `herdr.log` from about line 312 through the `release-agent` count. `fake_herdr.sh` is `tests/support/fake_herdr.sh`.

`list_panes` is re-exported from `herdr/mod.rs:7` and used by `talaria herdr panes` (`crates/talaria-cli/src/herdr_cmd.rs:8`). `[herdr] enabled` is parsed at `config.rs:957` and defaults false (`herdr_config.rs:6`). `report_state` defaults true (`config.rs:297`).

`docs/configuration.md:414` says the setting reports blocked, working or idle. Lines 415-422 say `enabled` adds live terminal agents and Enter focuses a Herdr row. The sample block is line 353. The table row is line 869.

`SEAT-3` adds the status-record writer beside this reporter and rewrites line 414 so the setting switches both. This unit runs after that sentence is on `main`.

`app.rs:993` holds `herdr_service`. `list_once_with_herdr` is `fleet/service.rs:446`. `commit_fleet_transcript` is `voice/fleet.rs:43`. `herdr_cmd.rs:94` calls `observe_herdr` in process. `herdr_cmd.rs:144` runs `fleet list --json` and expects both a `herdr` row and a `hermes` row. `voice_target.rs:115` is another `FleetBackend::Herdr`. `voice_multi_backend.rs` starts `HerdrService` through `tests/support/herdr_harness.rs`.
