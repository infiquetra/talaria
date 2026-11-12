# FIX-8: Say when a configuration change needs a restart

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

The issue states REQ-1 through REQ-3, the one new test, and the stop conditions. This note maps those ids onto the current tree. It does not add a requirement and it does not restate a test. Line numbers are from Talaria `main` at the draft. REQ-3 says this issue states no Hermes fact, so there is no Hermes file to re-read.

## Where the sentence already exists

`ApplyEffect` (`crates/talaria-core/src/settings/effects.rs:2-16`) is `Now` ("applies now"), `NewSessions` ("new sessions"), `NeedsRestart` ("needs restart"), and `MayNeedRestart` ("may need a restart"). `APPLY_EFFECTS` (`effects.rs:18-29`) lists only `display.skin` and `approvals.mode`, both `Now`. `apply_effect` (`effects.rs:30-45`) returns `MayNeedRestart` with no evidence string for every other key. `NeedsRestart` is never returned. REQ-2 leaves this table as it is.

`readback` (`crates/talaria-core/src/settings/state.rs:770`) records saved keys, then sets the banner (`state.rs:804-834`). The first banner line is `Saved N change(s) for profile P.` or `Nothing was saved for profile P.` The loop at `state.rs:813-832` then adds a line for each non-empty group, in order: `Needs a gateway restart to take effect`, `May need a gateway restart to take effect`, `Applies to new sessions`. The format is `{prefix}: {text::limited(keys)}.` `text::limited` (`crates/talaria-core/src/settings/text.rs:49-55`) shows five keys and then `and N more`. `Now` is not in the loop. A save of `display.compact` therefore produces `May need a gateway restart to take effect: display.compact.` The row label is a different string, `apply_effect(...).label()` (`state.rs:1387`), drawn by the wide row (`crates/talaria-tui/src/settings.rs:227`) and prepended on a narrow detail (`settings.rs:255`). REQ-1 copies the banner sentence, not the row label.

## Where the notice drops it

`close` (`state.rs:848-879`) emits `CloseOverlay`. When `saved` is not empty it emits one `Notice`: `Settings saved for profile {}: {}.` (`state.rs:867-871`), with each value passed through `format_value` (`crates/talaria-core/src/settings/diff.rs:118`). A secret becomes `(secret)` (`state.rs:858-859`). It then clears `saved` (`state.rs:872`). It does not read `apply_effect`.

`readback` calls `close` only when `command_close` is set and at least one key was saved (`state.rs:842-843`). That flag is set when `/config key=value` arrives while the screen is closed or the profile differs (`state.rs:427-431`). While the screen is already open on that profile, the command sets `command_close` false (`state.rs:420-425`) and the banner stays on screen. A later `SettingsAction::Close` reaches `close` through `request_close` (`state.rs:1129-1143`) when `pending` is empty. REQ-1 is the append inside `close`, so both callers gain the sentence. The groups have to be built before `saved.clear()`.

A discarded edit has empty `saved`. `one_apply_at_a_time_and_discard_prompt` expects only `CloseOverlay` (`settings_state.rs:426`). A failed or refused write returns to `Reviewing` and does not call `close` (`state.rs:737-761`).

## What the new test drives

`snapshot` (`settings_state.rs:25`) has `display.compact`, `display.animations`, and `display.password`. It has no `display.skin`, so the skin run needs its own snapshot or `apply_command` reports an unknown setting (`state.rs:689-690`). `writing` (`settings_state.rs:91`) sends `display.compact=true` through `review` (`settings_state.rs:75`) while the screen is open. `open_load_browse_edit_review_apply_flow` (`settings_state.rs:151`) already asserts the banner sentence and does not close. `reveal_is_one_shot` (`settings_state.rs:522-532`) closes after saving `display.password` and forbids the secret value in the notice. An effect sentence that names the key and not the value keeps that assertion.

`every_entry_has_evidence_and_matches_the_schema` (`crates/talaria-core/tests/settings_effects.rs:3-24`) reads `fixtures/rest/config-schema.json` and requires `Now` for the two named keys and `MayNeedRestart` for every other schema key. `narrow_and_too_small` (`crates/talaria-tui/tests/settings_render.rs:332`) expects the row text `may need a restart`.

## Edges, by requirement

| Case | Result | Id |
| --- | --- | --- |
| `/config display.compact=true` while closed, save succeeds | One notice, saved sentence plus `May need a gateway restart to take effect: display.compact.` | REQ-1 |
| Save on the open screen, then close | The same notice text. The banner already had the sentence | REQ-1 |
| Saved key is `Now`, such as `display.skin` | Saved sentence, no `restart` | REQ-1 |
| Several keys share one effect | One sentence, keys joined by `text::limited` | REQ-1 |
| `saved` is empty, including a discarded edit | No saved notice | REQ-1 |
| Secret value | Value half stays `(secret)`. The effect sentence names the key | REQ-1 |
| `display.skin`, `approvals.mode`, every other key | Classes unchanged | REQ-2 |
| Hermes payload | This issue states none | REQ-3 |
