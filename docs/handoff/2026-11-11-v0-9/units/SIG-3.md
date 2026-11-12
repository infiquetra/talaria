# SIG-3: Sign-in link and code stay readable and can be copied

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today and for the Hermes evidence. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`talaria_core::session::update` (`crates/talaria-core/src/session/reducer.rs:14`) is public, re-exported from `session/mod.rs`. A notification the reducer does not recognise is ignored (`reducer.rs:86`). [#578](https://github.com/infiquetra/talaria/issues/578) (`SIG-2`) adds the show arm that commits a `login:` text once. `SessionState::notice` (`state.rs:362`) pushes a `NoticeCell`. `NoticeKind::System` is `cell.rs:158`.

`InlineTerminal::copy_to_clipboard` (`terminal.rs:724`) writes `osc52` (`providers/clipboard.rs:3`). `Screen::copy_mcp_text` (`crates/talaria-cli/src/run.rs:3518`) calls it. [#571](https://github.com/infiquetra/talaria/issues/571) (`ACT-3`) reaches that writer through `AppEffect::CopyReply`. [#571](https://github.com/infiquetra/talaria/issues/571) (`ACT-3`) creates `crates/talaria-app/tests/copy_command.rs` and `copy_emits_clipboard_text_and_a_notice`, which asserts `/copy link` emits `Usage: /copy [N]`.

Hermes publishes the same three notices at v0.23.0 and at the pin v0.22.4: link, then code, then the warning that starts with `Do not share this code.` The key is `login:` plus eight hex characters. At v0.23.0 the link and code fields are `verification_uri_complete` and `user_code` (`anon_sign_in.py:317`). At v0.22.4 they are `claim_url` (fallback `verification_uri_complete`) and `claim_code` (`anon_sign_in.py:379-386`). This unit reads the stored notice text, not those fields.

## Names this unit uses

The store names are the ones in [#577](https://github.com/infiquetra/talaria/issues/577) (`SIG-1`)'s body. `NoticeRow` has public `text` and `key: Option<String>`. Each `apply_show` makes that row the newest, including a replacement, and oldest to newest is that show order. A clear is `notification.clear` with payload `key`. [#578](https://github.com/infiquetra/talaria/issues/578) (`SIG-2`) commits in the reducer, adds `sign_in_text_commits_once`, and draws `most_recent` without this unit's instruction line. [#571](https://github.com/infiquetra/talaria/issues/571) (`ACT-3`) provides `LocalAction::Copy` as a unit variant and `AppEffect::CopyReply { text: String }`. `local_effect` (`app.rs:5779`) takes the parsed argument as a `String`. `SessionState::notices(&self) -> &NoticeStore` is public. `sign_in_copy` walks only `login:` keys. A same-key re-show of a link starts a new pair and clears its code. Clearing the newest link leaves the older stored link as the pair. `None` is the result only when no link remains. `copy_with_wait` is defined at `anon_sign_in.py:107` on v0.22.4.

REQ-3 adds `sign_in_copy` because `most_recent` is only one row, and the link is no longer that row once the code and the warning have been shown.
