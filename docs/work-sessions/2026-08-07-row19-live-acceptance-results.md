# Row 19 live acceptance run — results

**Checklist.** `docs/plans/2026-08-06-u6-row19-operator-checklist.md`
**Run.** 2026-08-06 (UTC 2026-08-07), operator present and watching the terminal.
**Build.** The merged tree at `19a5d26`, launched from the repository virtualenv. Note that `talaria`
on `PATH` resolves to a stale frozen scaffold and was deliberately not used.
**Gateway.** A live Hermes dashboard on the loopback transport — not a stub.

**Corpus.** `talaria-live-corpus-v1-107f-e40d9fd04ec5` — 7 recordings, 107 frames. Per R11 the corpus
is cited by digest and count only and is not committed.

## Outcome by step

| step | branch | result |
|---|---|---|
| 1 | `--resume` and `--session <id>` | **pass** |
| 2 | compatibility check on-screen output | **partial — see below** |
| 3 | authentication-failure branch | **pass** |
| 4 | absent-capability branch | **not reachable — see below** |
| 5 | F1 end to end, isolated session | **pass** |
| 6 | F7, gateway survives Talaria's exit | **pass** |

## Step 1 — both startup paths, run live for the first time

`--resume` fetched `session.most_recent` and resumed that session. `--session <id>` called
`session.resume` with the **supplied** id, not the most-recent one it had just fetched — which settles
the precedence chain of `KTD7 (2026-08-02 prototype plan)` (`--session` beats `--resume` beats new)
against a live gateway.

The stronger evidence came from a mistake. The first `--session` attempt used a `stored_session_id`
returned by `session.create`, which is not a resumable id. The gateway refused it —
`code 4007: session not found` — and Talaria **failed rather than falling back** to the most-recent
session it already held, surfacing the gateway's own code and message on screen. A silent fallback would
have been indistinguishable from success on a passing run; only the failing id could show its absence.

`session.resume` had no live evidence of any kind before this run. It now appears in 3 recordings.

## Step 2 — partial, and the reason matters

All five read-only startup probes answer against the live gateway: `commands.catalog`,
`session.most_recent`, `spawn_tree.list`, `agents.list`, `delegation.status`.

Two divergences from what the checklist expected:

**`spawn_tree.list` is not refused.** The checklist asks whether it is refused; against the live gateway
it returns `{"entries": []}`. The refusal it was written against is fixture behavior, not gateway
behavior.

**There is no on-screen compatibility output to capture.** The checklist asks for "the literal on-screen
text, not a paraphrase". Talaria renders no compatibility panel at startup — the probe results exist only
on the wire. The step cannot be completed as written against this build, and the request should be
re-scoped (either the interface surfaces the check, or the step asks for the frame evidence it actually
produces).

## Step 3 — authentication failure and recovery

Forced by restarting the Hermes dashboard, which invalidates the token in the credential file. Verbatim
on screen, including the ellipsis the notice bar renders:

```
authentication failed — the gateway rejected the credential ·
ws://127.0.0.1:9119/api/ws: handshake reject…
```

Recovered with `talaria refresh-credential`, which reported updating the credential file at mode 0600.
The file's digest changed, confirming a genuinely new token rather than a no-op. The next launch
completed the full startup sequence with 7 responses and 0 errors.

## Step 4 — not reachable, which is itself the finding

The branch needs an endpoint that serves the WebSocket transport but lacks the admin HTTP API. Every
listener on this machine was probed: **no endpoint serves `/api/ws` while returning 404 for
`/api/model/options`.** In this Hermes build the two capabilities ship together, so the absent-capability
branch is defensive code for a version skew that does not yet exist in the wild.

It was **not** manufactured with a stub — the checklist's preconditions require a live gateway, and a
fixture would have produced a green box proving nothing about the field. The branch remains covered by
U1's unit tests, which exercise 404 against a real loopback HTTP server.

## Step 6 — F7, by direct observation

The one branch no frame log can settle, because the log ends at the exit being tested.

| sample | gateway PID | started |
|---|---|---|
| before exit | 28663 | Thu Aug 6 09:56:55 |
| after exit | 28663 | Thu Aug 6 09:56:55 |

Method: `ps -o pid,lstart`. Talaria's exit was **confirmed** by its absence from the process table, not
assumed. Identical PID and start time prove the same process rather than a respawn; the gateway
continued serving.

A first attempt at this step was **discarded as invalid**: the quit was sent as text and Talaria never
exited, so "the gateway survived" was true of a run in which nothing had exited. The frame count
confirmed no quit reached the wire.

## What this does not settle

Row 6 gains exactly one method — `session.resume`. The remaining evidence-only methods are still
unexercised, so the row does not clear.

Step 2 cannot be completed as written and step 4 is not reachable against any current Hermes. Row 19
therefore should **not** be graded met on this run; the honest grade is that four of six branches now
have live evidence and two need the checklist itself amended.

## Environment note

The Hermes dashboard was restarted during step 3 with the operator's explicit consent, and was
confirmed serving afterwards. Only the dashboard was touched; the long-running profile gateways were
not.
