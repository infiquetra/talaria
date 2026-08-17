# Status-line command contract

Status: `active`
Authority: `contract`
Version: 1

Talaria can run an operator-supplied external command on an interval and show its output in the
status region — the same idea as a shell prompt's `git`/Kubernetes-context integration, applied to
a Talaria session. This file is the contract for anyone writing that command. It is frozen (KTD5):
the field set, the process behavior, and the environment rules below do not change without a
`version: 2` bump.

## Configuring a command

```toml
# ~/.talaria/config.toml or ./.talaria/config.toml
[status]
command = "git status --short"
interval_seconds = 10
```

`command` is a plain command-line string, split into an argument array with POSIX quoting rules —
**never passed to a shell**. Pipes, `&&`, globs, and `$VAR` expansion do not work; write a small
script and point `command` at it if you need those. Leaving `command` unset disables the runner
entirely: no child process is ever spawned, and the status region renders nothing.

`interval_seconds` sets the tick cadence. See `talaria/config.py` for the current default and
override chain (CLI flag, `TALARIA_STATUS_INTERVAL_SECONDS`, repo-local config, global config,
built-in default).

## What the command receives

On every tick, Talaria spawns the configured command directly (`exec`, no shell) and writes one
UTF-8 JSON document to its stdin, then closes stdin. A command that reads to EOF and exits promptly
is the well-behaved case; one that never reads stdin is unaffected (Talaria does not block waiting
for the read).

```json
{
  "version": 1,
  "mode": "replay",
  "connection": "connected",
  "session": { "id": "sess-1", "title": "a title" },
  "turn": "idle",
  "pending_prompts": 0,
  "subagents": { "active": 0, "terminal": 0 },
  "usage": { "input_tokens": 120, "output_tokens": 340 }
}
```

| field                | type                                                                              | meaning                                              |
| --------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `version`              | `int`                                                                              | Always `1` for this document shape. First field, so a consumer can branch before parsing anything else. |
| `mode`                 | `"replay"` \| `"live"`                                                            | Whether Talaria is replaying a recorded corpus or attached to a live gateway. |
| `connection`           | `"disconnected"` \| `"connecting"` \| `"connected"` \| `"reconnecting"` \| `"auth_failed"` | The attach state.                                     |
| `session`              | `{ "id": str, "title": str \| null }`                                              | The focused session's identifier and human-facing title. |
| `turn`                 | `"idle"` \| `"streaming"` \| `"waiting"` \| `"cancelled"`                          | `"waiting"` means a prompt is outstanding — this is the one field that tells an external consumer the session is blocked on the operator, not working. |
| `pending_prompts`      | `int`                                                                              | Count of outstanding prompts **in the focused session** — never the install-wide total. No prompt content is ever included here. |
| `subagents`            | `{ "active": int, "terminal": int }`                                              | Sub-agent counts by state, not the rows themselves.   |
| `usage`                | `{ "input_tokens": int, "output_tokens": int }` \| `null`                        | `null` until usage has been observed at least once.  |

### `pending_prompts` counts the focused session, and only the focused session

This is a clarification of what the field has always meant, not a change to it. Talaria v0.4 keeps a
fleet-wide *needs-you queue* — every outstanding prompt of every session on every connected gateway —
and that count is deliberately **not** folded in here. A consumer written against version 1 reads
this number as "the session I am looking at is blocked on me N times"; making it the whole install's
total would silently turn another session's approval into this session's, in a document whose field
set is frozen. The install-wide count lives on Talaria's own needs-you surface. If a fleet-scoped
number is ever wanted here it arrives as a new, additional field under a new `version` — never as a
meaning change to this one.

A prompt that belongs to a session Talaria is no longer showing does not count either, for the same
reason: the session on screen has nothing outstanding.

**This is the whole of what the command sees about Talaria's state.** No terminal-framework type, no
raw transcript, and no credential ever appears in this document — it is a small, deliberately
narrow projection.

## What the command should produce

Write plain text to stdout, one status row per line. Rows are rendered **as literal text** — ANSI
escape sequences are not interpreted, so color codes will show up as visible escape characters, not
as color. Keep output short: only the first 8 lines are shown, and output past 16 KiB is truncated.
Both truncations are marked visibly rather than silently cut.

Anything written to stderr is captured separately, capped at 4 KiB, and used **only** to annotate a
failure — it never appears among the rendered rows, so a command cannot make diagnostic chatter look
like a status line.

## Process contract

- **Working directory** is the directory Talaria was launched from, not `~/.talaria`. A command that
  runs `git -C "$PWD" rev-parse` (or the equivalent without a shell) resolves the operator's actual
  repository.
- **Timeout** is 2 seconds. A command that has not exited by then is killed, along with everything in
  its process group — a command that backgrounds a long-lived helper does not leave it running after
  the timeout fires.
- **At most one command runs at a time.** If a tick's command is still running when the next tick is
  due, that tick is skipped rather than starting a second, overlapping instance.
- Every failure — a nonzero exit, a timeout, empty output, invalid UTF-8, a command that does not
  exist — produces a categorical marker in the status region. None of these ever crash or block the
  rest of Talaria.

## Environment

The command's environment is **default-deny**, not a copy of Talaria's own environment. It receives:

- `PATH`, `HOME`, `SHELL`, `TERM`, `TMPDIR`
- `LANG` and every `LC_*` locale variable present in Talaria's environment
- Exactly five `TALARIA_*` variables, when Talaria itself has them set: `TALARIA_CONFIG_DIR`,
  `TALARIA_GATEWAY_URL` (with its query string removed — scheme, host, and path only), `TALARIA_PROFILE`,
  `TALARIA_LOG_LEVEL`, `TALARIA_STATUS_INTERVAL`. No other `TALARIA_*` variable is ever forwarded.
- Any additional variable named in the operator's `[environment] allowlist` config setting, if it
  is present in Talaria's own environment.

A variable whose name looks credential-shaped (matching the same pattern set Talaria's recorder uses
to redact frame logs — names like `*_token`, `*_secret`, `*password*`, `*_api_key`) is **never**
forwarded, regardless of whether it carries the `TALARIA_` prefix or appears in the operator
allowlist. This is a hard deny, not a suggestion.

It was written because the attach credential could ride a `TALARIA_GATEWAY_URL` query parameter,
which is why the query string is stripped wholesale rather than pattern-filtered. **That route was
removed on 2026-08-07** — Talaria now refuses an endpoint carrying a credential instead of reading
one from it — so the variable should no longer carry a credential at all. Both the stripping and the
deny stay: a variable Talaria refuses to *read* a credential from is still a variable an operator
may have put one in, and forwarding it to a child would leak it whether or not Talaria dialled with
it.
