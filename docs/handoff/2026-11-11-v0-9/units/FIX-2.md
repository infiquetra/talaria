# FIX-2: Interrupting an Ultracode prompt can overwrite the draft

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for REQ-1 through REQ-3 and the new composer test. This page maps the restore path. It does not restate the requirements.

## What exists today

`UltracodeService::intercept_submit` (`crates/talaria-app/src/ultracode.rs:115`) returns `Action::RunCommand` with `original_text` set to the submitted line (`ultracode.rs:126-131`). A one-shot keyword also pushes `notice::ONE_SHOT` (`ultracode.rs:123-125`).

`on_command_outcome` (`ultracode.rs:135`) returns `Action::SubmitCommand` when the reply is the `talaria-workflows` skill with a message and a display (`ultracode.rs:143-158`). Every other outcome in that match builds a failure string and returns a notice plus `Action::RestoreComposer(original_text)` (`ultracode.rs:184-187`).

`apply_ultracode` (`crates/talaria-app/src/app.rs:7076`) handles `Action::RestoreComposer` by calling `load_draft` (`app.rs:7097`). `load_draft` copies the text onto `state.draft` (`app.rs:6977-6978`). `AppMessage::UltracodeDone` calls `on_command_outcome` when the session is in the foreground (`app.rs:4204-4213`). The command itself is started from `AppEffect::Ultracode` (`app.rs:12837-12854`).

`failures_restore_composer_and_never_fall_back` (`crates/talaria-app/tests/ultracode_app.rs:215`) asserts that restore for six outcomes. It does not set `AppState.draft`. `session()` in that file is `SessionId::new("fixture-session")` (`ultracode_app.rs:10-12`).

[#576](https://github.com/infiquetra/talaria/issues/576) (`CREW-3`) is on `main` before this unit starts. It adds `answer_install`, `Action::InstallSkill`, `notice::offer`, and `AppMessage::UltracodeNotice`. This unit does not change them.

## Edge cases

| Composer when the command fails | Result |
| --- | --- |
| `state.draft.is_empty()` | `load_draft` runs with the original text |
| A line typed while the command was in flight | that line stays, and the failure notice is still shown |
| Successful skill reply | `Action::SubmitCommand`, unchanged |
