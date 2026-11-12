# FLT-3: Two Talaria processes on one remote gateway stay signed in

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`refresh_cached` (`crates/hermes-client/src/remote/native_auth.rs:45`) takes `TokenStore::refresh_lock` (`native_auth.rs:51`). That lock is a process-local `tokio::sync::Mutex` kept in a `OnceLock` map keyed by the token path (`token_store.rs:15` and `:21`). Two `TokenStore` values in one process share it. Two processes do not. After the lock, the function loads the file and returns it when `now` is more than 60 seconds before `expires_at` (`native_auth.rs:56`). The refresh route is `POST /auth/native/refresh` (`native_auth.rs:61`).

`save` writes a temporary file and `persist`s it onto the token path (`token_store.rs:93` and `:104`). `persist` replaces the inode. An advisory lock on the old inode does not cover the new file.

`std::fs::File::lock` is an exclusive lock, released when the file is closed. It is stable since Rust 1.89.0. `rust-toolchain.toml` pins channel `1.98.1`, so the method is in the pinned standard library. No extra crate is required.

`redact_url` (`crates/hermes-client/src/redact.rs:614`) returns the URL with credentials withheld, or `[redacted]` when the input is not an absolute URL. `upgrade_error` (`remote/connect.rs:157`) includes the base for HTTP 404 (`connect.rs:160`) and omits it for 403 (`connect.rs:159`) and for 502, 503, and 504 (`connect.rs:161`). `RefreshError::SessionExpired` displays `sign-in expired; run talaria remote login <name>` (`remote/mod.rs:115`). `RefusedAfterRefresh` is also built at `ws_ticket.rs:42` and `remote/mod.rs:339`. Both must pass `endpoint`. A 401 retry holds the same token the gateway refused, so the refresh call is made as today. `ws_ticket_retries_once_after_refresh` keeps its refresh counts; only its expected sentence changes.

The mock counts refresh calls (`mock_gated.rs:642`). With `rotating: true`, a second use of the same refresh token is HTTP 401 `session_expired` (`mock_gated.rs:644`). `live_remote_attach_and_round_trip` (`live_remote.rs:44`) is one ignored process. It reads `TALARIA_LIVE_REMOTE` and skips when that variable is unset (`live_remote.rs:45`).

So the lock is taken on a separate lock file, `<name>.json.lock` in the same folder (`TokenStore::lock_path`), which is created once with mode 0600 and never replaced, written or deleted. `save` keeps its temporary file and `persist`. Nothing in the tree lists the `remote-tokens` folder (searched for `read_dir` under `crates/hermes-client/src/remote` and `crates/talaria-cli/src/remote.rs`), so the extra file is not mistaken for a token.
