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
| 2 | compatibility check on-screen output | **pass** |
| 3 | authentication-failure branch | **pass** |
| 4 | absent-capability branch | **pass** |
| 5 | F1 end to end, isolated session | **pass** |
| 6 | F7, gateway survives Talaria's exit | **pass** |

All six branches have live evidence. Steps 2 and 4 were **initially and wrongly** written up as
"partial" and "not reachable"; both were failures to test, corrected below and recorded here rather
than quietly overwritten, because the way each was gotten wrong is the reusable part.

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

## Step 2 — pass, after a wrong first answer

The compatibility check runs at startup on every launch and reports:

```
gateway compatibility: 0 blocking, 13 unverified at runtime (evidence-only, R34), baseline 7f4d15515
```

Per-probe verdicts, verbatim, against the live gateway:

```
session.most_recent: present, top-level response shape matches 7f4d15515
spawn_tree.list:     present, top-level response shape matches 7f4d15515
agents.list:         present, top-level response shape matches 7f4d15515
delegation.status:   present, top-level response shape matches 7f4d15515
commands.catalog:    present, top-level response shape matches 7f4d15515
```

Five probes made, five present, zero blocking.

**`spawn_tree.list` is not refused.** The checklist asks whether it is; against the live gateway it is
`present` with a matching response shape. The refusal it was written against is fixture behavior.

**What was first reported, and why it was wrong.** This step was initially written up as
"Talaria renders no compatibility panel at startup — the probe results exist only on the wire", on the
evidence of a blank transcript. `talaria/ui/app.py` guards the render with `if report.blocking:` — the
check runs every time and prints **only when it finds a gap**. On a fully compatible gateway, silence
*is* the verdict. Reporting "no panel" turned a passing check into a missing feature, and it came from
reading a screen instead of reading the code that draws it.

The thirteen evidence-only methods this run also enumerated by name — `session.create`,
`session.resume`, `prompt.submit`, `session.interrupt`, `subagent.interrupt`, `slash.exec`,
`command.dispatch`, `paste.collapse`, `approval.respond`, `clarify.respond`, `secret.respond`,
`sudo.respond`, `terminal.read.respond` — which is row 6's thirteen, now pinned rather than counted.

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

## Step 4 — pass, against the live gateway, no stub

A profile the gateway does not recognize returns a real 404 on the admin path:

```
GET /api/model/options                          -> 200
GET /api/model/options?profile=no-such-profile  -> 404
```

Driven through Talaria's own admin client against the live gateway:

```
happy path      : model_options() -> 8 providers
ABSENT-CAP path : reason='absent_capability'
                  message=this gateway does not serve /api/model/options; it predates the admin model API
repr(client)    : AdminClient(origin='http://127.0.0.1:9119/')
```

This is exactly the checklist's "a gateway **/profile** missing the capability", reached with no
fixture, no proxy and no stub.

**A defect this exposed.** The message is wrong for the case that most easily produces it. The gateway
*does* serve `/api/model/options`; the 404 came from the profile not existing. An operator who mistypes
a profile name is told their gateway predates the admin model API. `absent_capability` conflates
"endpoint absent" with "profile absent", and the second is far more likely. Worth its own queued item.

**What was first reported, and why it was wrong.** This step was initially written up as "not
reachable", on the reasoning that no local endpoint serves the WebSocket transport while 404-ing the
admin API. Two errors stacked. The endpoint survey used a plain HTTP `GET` against `/api/ws` and read
404 as "no WebSocket here", which a WebSocket route returns to any non-upgrade request — that probe
could not have distinguished a missing route from a present one. And only *one* admin path with *no*
parameters was ever tried; getting a 200 from it was treated as proof that no 404 was obtainable. The
happy path was confirmed and the absence of a result was written up as a finding.

The generalizable rule, since this is the second time today the same shape appeared: an "unreachable"
or "does not exist" conclusion is a *claim*, and it needs the same standard of evidence as a positive
one. Reaching for a stub to make a branch go green is the visible failure; quietly concluding the branch
cannot be reached is the same error with no artifact to review.

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

## What this does and does not settle

**Row 19.** All six branches now have live evidence, observed by the operator, against a real gateway.
Whether that clears the row is U7's call on a re-grade, not this document's.

**Row 6 gains exactly one method.** The check enumerated the thirteen evidence-only methods by name.
Of those, four now have live evidence — `session.create`, `prompt.submit`, `slash.exec`, and
`session.resume`, which this run added. Nine remain with none, so **row 6 does not clear**. This is
the falsifiability control holding: a run where every row improved would be the failure mode.

**Two defects to file separately**, neither of which blocks the acceptance evidence:

1. `absent_capability` misattributes a profile-not-found 404 as a too-old gateway (see step 4).
2. The model picker is a numbered list rather than a picker — already queued as P0 in
   `docs/engineering-journal/QUEUED.md`.

## Environment note

The Hermes dashboard was restarted during step 3 with the operator's explicit consent, and was
confirmed serving afterwards. Only the dashboard was touched; the long-running profile gateways were
not.
