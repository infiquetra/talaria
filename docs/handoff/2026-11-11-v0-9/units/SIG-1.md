# SIG-1: Notice store: keep, replace, expire and clear Hermes notices

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue says what the store does. This file is the map of the reducer arm, the payload fields, and the session object the store hangs on. It restates no requirement and no test. It refers to them by id.

## Where show and clear go today

`update` (`crates/talaria-core/src/session/reducer.rs:14`) returns immediately when the event session id is empty or is not this session (`reducer.rs:21-22`). Its comment says it reads no clock (`reducer.rs:13`). A known name with no arm falls through to the empty arm (`reducer.rs:523-532`). `notification.show` and `notification.clear` are known names (`crates/hermes-contract/src/events.rs:215-216`), so they hit that empty arm. `unknown` is false for a decoded notification. REQ-3 adds an arm beside `status.update` (`reducer.rs:495-511`).

The payload `session` reads is `raw["payload"]`. `session` starts at `reducer.rs:318`. `status.update` reads `payload["text"]` and `payload["kind"]` as strings (`reducer.rs:496-501`). REQ-3 reads the show fields the same way.

## Payload fields

`NotificationShowPayload` (`crates/hermes-contract/src/generated/schemas.rs:10566-10577`) has `text`, `level`, `kind`, and optional `detail`, `id`, `key`, and `ttl_ms`. `ttl_ms` is `Option<i64>`. `NotificationClearPayload` (`schemas.rs:10556-10558`) has `key` only. The doc comment (`schemas.rs:10564`) lists `kind` as `sticky`, `ttl`, or `agent`, and `level` as `info`, `warn`, `error`, or `success`. REQ-1 does not edit this file.

At Hermes v0.23.0, `agent/credits_tracker.py` sets `CREDITS_NOTICE_KIND` to `sticky` and `CREDITS_RESTORED_TTL_MS` to 8000. `gateway/slash_commands_login.py` publishes `notification.show` with `kind` `sticky` and `key` `login:` plus eight hex characters, and only for a local platform. REQ-5 says to re-verify those two facts.

## Why this is not StatusNote

`StatusNote` (`crates/talaria-core/src/session/state.rs:47-51`) has `kind` and `text` only. `SessionState::status` (`state.rs:119`) holds one note. A Hermes notice also has a key, a level, a detail, and a time to live. REQ-1 adds a separate store. `SessionState::notice` (`state.rs:362`) appends a transcript cell. REQ-3 does not call it.

`SessionState::empty` (`state.rs:151`) is the only constructor `from_snapshot` uses (`snapshot.rs:12`). The doc comment (`state.rs:189-192`) says that constructor serves both `session.create` and `session.resume`. A snapshot carries no notice rows. REQ-1 returns a new empty store and does not mutate the state the caller already holds. `configure/state.rs:669` stores that new state in place of the live one, so the previous notices are not copied. A resume of the same session id uses that constructor too (`state.rs:189-192`). `session_id` is not assigned after `empty`. The keyless slot of one state that stays live is a different fact: REQ-3 replaces it only with a new keyless show, and a clear never touches it.

The expiry stamp is not one of `NoticeRow`'s seven fields. REQ-1 keeps it inside `NoticeStore`. REQ-4 stamps a row on the first `expire` call. `ttl_ms` of 8000 stays present at `now_ms` 0 and 7999 and is gone at 8000. REQ-3 clears the stamp on a keyed replacement and on a keyless replacement. REQ-2's `apply_show` stores the row it is given. The text rules stay in the reducer.

## Tests already in the tree

`known_but_unmodelled_events_are_silent` (`crates/talaria-core/tests/session_rules.rs:1187`) applies six names and asserts no cells. It does not apply `notification.show`. `repeated_status_update_is_recorded_once` is `session_rules.rs:832`. `unknown_event_is_noted_once_per_connection` is `session_rules.rs:1143`. `ev` is `crates/talaria-core/tests/support/events.rs:61`. `update` is public (`reducer.rs:14`, re-exported from `session/mod.rs:25`). `reduce` is `support/drive.rs:11` and returns only the final state (`drive.rs:25-30`). Tests 4, 5 and 6 call `update`. `snapshot_json::base` is `support/snapshot_json.rs:22`.
