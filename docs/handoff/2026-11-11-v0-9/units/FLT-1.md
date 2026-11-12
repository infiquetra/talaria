# FLT-1: Sub-agent and background-process counts in the fleet peek

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`PeekPanel` (`crates/talaria-tui/src/fleet_view/mod.rs:47`) holds `&SessionState`, an optional `PromptStack`, and `now`. `render` (`fleet_view/peek.rs:78`) draws the separator, then `header_line` (`peek.rs:281`), then the body. The header is glyph, title, profile, model, state word, and age (`peek.rs:292`). It has no counts.

`TuiScreen::sync_fleet` (`crates/talaria-cli/src/run.rs:3709`) builds the panel. `fleet_peek_is_foreground` (`run.rs:718`) is true when the peeked key's profile and session id are the foreground ones. The foreground panel uses `view.state` (`run.rs:3743`). A background peek uses `view.peek`'s `SessionState` (`run.rs:3751`).

`SubagentHub::counts` (`crates/talaria-app/src/subagents/hub.rs:1133`) returns `Option<SubagentCounts>`. `live` is `tree.rs:38`. The context view's `active_subagents` line says the number is for every session on the gateway (`context_view.rs:491`).

`BackgroundWork::summary` (`background/mod.rs:379`) returns `running`, `finished_recently`, and `bg_running` (`background/mod.rs:254`). The comment above the method says "Counts for later units" (`background/mod.rs:377`). `PeekService::on_link_event` applies `session::update` and drops the effects (`crates/talaria-app/src/peek.rs:436`), so a background peek's process summary does not follow later process events.

`peek_panel_tail_card_and_outcome` (`fleet_view_screens.rs:617`) builds three `PeekPanel` values, at lines 642, 677, and 713.

The count line is drawn only in that peek. The fleet list is unchanged. `PIN-3` leaves `PINNED` at Hermes v0.23.1. This unit does not edit that file. `/fleet` opens the overlay (`docs/configuration.md`). Space is `fleet_peek`.
