---
title: Talaria v0.1 prototype implementation plan — doc review
type: docs
status: complete
date: 2026-08-02
target: docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md
classification: plan
reviewed_revision: working-tree-untracked
reviewed_head: 064967b32c5975ba77c277b8a68448cc1e3690fa
target_sha256_before: 2642675aecdac4703b5cb9091be448e233014b6910e397a06e0fff6569e4ad44
target_sha256_after: 7fbf7d0270fde1f1399da9658a514130a3ce438fc60cf0c9ce03c9a3577c43e6
hermes_evidence_revision: 7f4d15515
external_panel: codex/gpt-5.6-sol (max effort, read-only); ollama-cloud/kimi-k3 (max effort)
ready_for_work: true
---

# Doc review — Talaria v0.1 prototype implementation plan

## Verdict

**Ready for `/work`.** All twenty-one findings are dispositioned; the operator settled the seven
that needed a decision on 2026-08-02 and the rest were applied.

The review found the plan structurally sound — near-complete requirement mapping, replay-first
ordering that genuinely spends nothing on transport before the framework verdict, a named falsifier
for every planning obligation — but built on external claims that did not all survive checking. The
credential design rested on two citations that answered a different question; correcting that
exposed three further decisions the plan called settled (the status contract, the frame-log
equivalence relation, the compatibility baseline) that were classifications rather than schemas.

Twenty-four fix groups were applied across two review rounds, then a third pass applied the
operator's decisions. The P0 is closed on verified source: gate selection follows the bind host, so
v0.1 targets loopback `?token=` and defers the gated path, which turned out to be reachable after
all through an RFC 8252 native-app flow. Planning obligations PC2, PC4, and PC10 all close, and no
open question in the plan blocks a unit.

**One correction to this review's own record.** It initially flagged a mismatch between the plan's
Hermes pin and a workspace checkout of the fork. That was wrong: `7f4d15515` is exactly the revision
installed at `~/.hermes/hermes-agent` (Hermes Agent v0.19.1), confirmed by ancestry check. The pin
was always correct and the checkout being read was not the operator's runtime.

## Applied fixes

| Fix | Priority | What changed |
| --- | -------: | ------------ |
| KTD11, PC10, U7, R1 row, Risks row, Sources | P0 | The Bearer-header claim is replaced with the verified query-parameter mechanism, its three credential forms, and an explicit statement of what is now undecided. The risk row is marked **fired**. |
| DECISIONS.md journal entry | P0 | The mirrored claim is amended with a `Status.` correction and a `Learning.` entry on why two agreeing citations were not corroboration. |
| KTD8 | P1 | Terminal sub-agent states corrected from three to the source's five (`error` and `timeout` were missing); the seven-member enum is now named. |
| Problem Frame | P1 | The inherited DR15 `/work` block, which the plan had dropped, is carried forward as an explicit stop condition. |
| R1 evidence cell | P1 | The claim that "AE10 install check greps process listing" is withdrawn — origin AE10 contains no such check — and the missing obligation is named. |
| KTD5 | P1 | The `TALARIA_*` environment wildcard is made non-overridable against credential-shaped names, and `TALARIA_GATEWAY_URL` is forwarded only in redacted form. The KTD now admits what it has not frozen. |
| KTD6 | P1 | The unresolved three-way conflict with `docs/formats/frame-log.md` is recorded as a U2 precondition. |
| KTD10 | P1 | Request field corrected to `start_line`, and the two semantics the source does fix (omitted arguments mean the visible screen; valid lines are `[0, total_lines)`, clamped) are recorded. |
| Open Questions | P1 | Four blocking questions added: gateway mode, corpus provenance, view-model sequencing, and viewport-field derivation. |
| U7, U8, U9 file lists | P2 | The user-interface integration files each unit's own test scenarios require are added; U7 also gains the journal file it needs to close the third standing P0. |
| U2 approach | P2 | "All five respond methods" corrected to the four deny-set methods, with a note on why `approval.respond` is excluded. |
| Traceability rows | P2 | R9 gains U8; R36 and R40 gain U6 in its requirements list; U3's goal no longer over-claims R15; R33 gains bandit. |
| Execution spec | P2 | `U4` gains the journal file it edits; `U7` gains its true `U3` dependency and its interface files. Revalidated clean with receipts. |
| Four output filenames | P2 | Literal `2026-08-XX` placeholders replaced with concrete dates, one of which conflicted with the execution spec. |
| U8 file list | P3 | The "if not folded into projection" hedge is resolved to the decision KTD10 already made. |
| U7 verification | P3 | Closes QUEUED.md's third standing P0, which the plan delivered but never closed. |
| U3, U6 requirements | P3 | Flows F3 and F5 — the only two the plan never named — are named where their units already cover them. |

### Second round — fixes to the fixes

The second engine reviewed the *corrected* document and found seven defects introduced or left open
by the first round. These were applied on top.

| Fix | Priority | What changed |
| --- | -------: | ------------ |
| KTD5 environment rule | P0 | The first round forwarded `TALARIA_GATEWAY_URL` "in `redactUrl` form" — a filter the same review had just documented as blind to `ticket` and `internal`. In gated mode that forwards a live credential into a subprocess environment on a ten-second timer. Now the **entire query string is dropped**; the rule no longer depends on pattern coverage. |
| U7 test scenarios | P1 | The first round corrected KTD11, PC10, R1, the risk row, the sources and U7's approach — but missed U7's own first test scenario, which still asserted "attach with header credential succeeds… asserting no token in the URL": both the refuted mechanism and the opposite of what the settled design requires. Rewritten, and acquisition-chain scenarios added (precedence order, `0600` rejection, prompt non-echo, rotation source). |
| KTD6 paragraph 1, PC4 row | P1 | KTD6 still closed PC4 with "no contract change is needed" in one paragraph while mandating a possible version bump in the next. PC4 is now `PARTIAL` and the assertion is scoped to the relation rather than the format. |
| U2 dependencies | P1 | KTD6 says "U2 must not begin until the authority is reconciled" while U2 depended only on U1. The reconciliation is now named as a dependency and flagged as unassigned. |
| PC2 row | P1 | Still claimed "full v1 contract frozen" against a KTD5 that now lists what it has not frozen. Now `PARTIAL`, including the note that process-group semantics is load-bearing for R36's teardown promise. |
| KTD5 pattern source, U6 dependencies | P2 | The deny cited the TypeScript pattern constant, with no dependency from U6 on U2's Python port — two copies of one security boundary, free to drift. Now cites the Python module and declares the U6→U2 edge. |
| R9 row, U5 corpus line | P2 | The R9 note claimed U8 owns "the only path where an operator-typed credential is handled live", which KTD11's own interactive prompt falsifies; both paths are now named. U5's corpus identity changed from a path to an opaque label, propagating the open question's own directive. |

## Findings and their disposition

All twenty-one are closed. Seven required an operator decision (marked **operator**); the rest were
applied directly. Dispositions landed 2026-08-02 in the same commit as this review.

| ID | Priority | Location | Finding | Disposition |
| -- | -------: | -------- | ------- | ----------- |
| D1 | P0 | KTD11, PC10, U7 | The replacement credential mechanism was undecided. | **operator** — v0.1 targets **loopback `?token=` only**. Gate selection follows the bind host (`should_require_auth`, `web_server.py:437-460`); `--insecure` is accepted but ignored since `hermes-0day`; the default bind is loopback. Gated mode *is* reachable by a dial-don't-launch client via RFC 8252 (`dashboard_auth/routes.py:289`, `:841`, `:799`, `:894`) — deferred to QUEUED.md. The credential is minted **per dial including reconnect** through a `CredentialProvider`, so the deferred path is a later class rather than a reconnect rewrite. PC10 closes |
| D2 | P1 | U2, U5 | No unit produced the gate corpus. | **operator** — captured with the **existing TypeScript recorder** against the local Hermes during ordinary use, which keeps Python off a socket before the framework verdict. `~/.hermes/sessions` was checked and cannot supply it (two LLM request dumps, not gateway frames). Recorded as U2's corpus-provenance step and a U5 dependency |
| D3 | P1 | KTD5 | Forwarded `TALARIA_*` set not enumerated. | Fixed — exactly five variables named in KTD5; credential deny outranks the list and the allowlist |
| D4 | P1 | KTD5, PC2, U6 | Status contract called frozen but not a schema. | **operator** — split: value types and the full process contract (stderr capped and separate, launch-directory cwd, stdin closed after payload, `start_new_session=True` process-group kill) are **frozen now**; the field *set* is v1 with additions as `version: 2`. PC2 closes; `tests/status/test_process_contract.py` added to U6 |
| D5 | P1 | KTD6, PC4, U2 | Format authority, KTD6, and the reference recorder made three incompatible claims. | **operator** — resolved to **parsed-value equality**; `docs/formats/frame-log.md` amended in this commit, no version bump (the byte-identity guarantee was never true of its own reference implementation). `endpoint` normalization now field by field. PC4 closes; U2 unblocked |
| D6 | P1 | U3, U5 | View-model shape assigned to U5's evidence but U3 ships first. | Fixed — **U3 decides** (immutable snapshots with explicit change markers, chosen for AE2 determinism), U5 measures the re-render cost as ADR-0002's evidence |
| D7 | P1 | KTD3, KTD13, U7 | `FrameSource` undeclared; RPC ids had no connection epoch; AE16's backpressure threshold unset. | Fixed — async `AsyncIterator` in `talaria/transport/source.py` with idempotent `close()`; correlation key is `(epoch, id)` so a late reply from a dead socket cannot turn an `unknown` into a false success; backpressure pauses at 1,000 frames or 8 MiB |
| D8 | P1 | KTD9, U10 | Compatibility baseline had no fixture or result schema. | Fixed — request fixture plus a **top-level key set and value kinds** signature per method; nested structure deliberately out of scope |
| D9 | P1 | KTD14, U5 | R38 asks for bounded memory *as history grows*; the gate took one reading. | **operator** — v0.1 accepts unbounded transcript growth and says so; the gate **publishes a growth curve** (`ru_maxrss` every 5,000 frames plus fitted slope). Eviction becomes a milestone-3 question the measurement answers |
| D10 | P1 | KTD10, PC9 | `viewport_rows` / `cursor_row` had no derivation. | Fixed — `viewport_rows` is the rendered region height, served truthfully; `cursor_row` is **`null`**, since the only caret belongs to the composer and a synthesised row would be a confident wrong answer |
| D11 | P1 | Problem Frame | DR15 review-panel block. | **operator override, recorded** — a receipt gap, not a review gap, and unsatisfiable: the reconciliation itself records that no mechanical verifier exists. Journal entry added; U1 may begin |
| D12 | P2 | KTD14 | Two gate thresholds had no measurement method. | Fixed — `resource.getrusage(RUSAGE_SELF).ru_maxrss` sampled every 5,000 frames; render ticks counted in the coalescing flush callback over a fixed 60-second window |
| D13 | P2 | U1, PC7, U10 | CI was Linux/Node-only against a macOS arm64 matrix; no lock artifact. | **operator** — macOS arm64 is the **required** job (free for public repos), Linux optional, CPython 3.12 and 3.13; `uv.lock` committed and `textual`/`websockets` bounded in `pyproject.toml` |
| D14 | P2 | KTD11 | "Re-read on every reconnect" undefined for a prompt-sourced credential. | Fixed by D1 — the provider re-reads environment and file per dial; a prompt-sourced credential is cached in memory and **never re-prompts**, which would block reconnect on operator presence |
| D15 | P2 | U5 | Corpus identity would have put a local path in a public document. | Fixed — opaque label, sha256, frame count |
| D16 | P1 | KTD5, KTD7, KTD11, U1, U7 | Nothing owned the configuration surface or the default entry point. | **operator** — **KTD15**: two-level `~/.talaria` (repo-local `./.talaria` overrides), `config.toml` + `credentials` at `0600` + `recordings/`, five-level precedence, `talaria/config.py` the only reader. `talaria/cli.py` and `talaria/config.py` added to U1 in the plan and the execution spec |
| D17 | P1 | PC10, U2, KTD6 | The endpoint falsifier was unsatisfiable as scoped. | **operator** — the security property wins: the Python redactor is a **strict superset**, adding `ticket` and `internal`, and KTD6's relation becomes "equal except for an enumerated set of Python-side denials," still mechanically checkable. Pinned by a test so it cannot be mistaken for drift |
| D18 | P2 | U9, KTD4 | Paste-collapse threshold had no value or unit. | Fixed — **KTD16**: 6 lines or 512 bytes, whichever trips first; both bounds needed, configurable under KTD15 |
| D19 | P2 | Traceability R1, R2, R3 | Three evidence cells cited artifacts no unit scheduled. | Fixed — all three assigned to **U10** and written into its requirements, scenarios, and verification |
| D20 | P3 | U5, KTD4, diagram | Replay-mode Enter and a dotted `ReplaySource → recorder` edge floated unowned. | Fixed — Enter submits and echoes nothing in replay (a local echo is indistinguishable from a sent message); the diagram edge is removed and the design states replay is read-only with respect to the corpus |
| D21 | P3 | U5, U6 | U5 numbered before U6 but depends on it. | No action — identifiers are never renumbered once assigned |

### Original finding text

| ID | Priority | Location | Finding | Status |
| -- | -------: | -------- | ------- | ------ |
| D1 | P0 | KTD11, PC10, U7 | The replacement credential mechanism is undecided. The false claim is corrected, but which gateway mode v0.1 targets — and how a dial-don't-launch client mints a gated ticket — needs an operator decision plus a live attach. | open, blocks U7 |
| D2 | P1 | U2, U5 | No unit produces the gate corpus that U2 and U5 both consume. | open, blocks U2/U5 |
| D3 | P1 | KTD5 | The forwarded `TALARIA_*` set is still not enumerated; only the credential deny is now closed. | partially fixed |
| D4 | P1 | KTD5, PC2, U6 | The status contract is called frozen but is not a schema: `connection` and `pending_prompts` have no types, and stderr, working directory, stdin closure, and process-group/descendant termination are unspecified. | open |
| D5 | P1 | KTD6, PC4, U2 | The format authority guarantees byte-identity, KTD6 rejects it, and the reference recorder re-serializes. All three cannot hold. | annotated, reconciliation owed |
| D6 | P1 | U3, U5, Open Questions | The view-model shape is assigned to U5's evidence but U3 must ship the projection first. | open |
| D7 | P1 | KTD3, KTD13, U7 | `FrameSource` has no declared module, no sync/async decision, and no close/cancel semantics; RPC identifiers have no connection epoch, so a late reply from a dead socket can satisfy a reused identifier after reconnect; AE16 measures a backpressure threshold the plan never sets. | open |
| D8 | P1 | KTD9, U10 | The compatibility baseline classifies methods but supplies no request fixture or result schema, so U10's "one response shape drifted" test has nothing to compare against. | open |
| D9 | P1 | KTD14, U5 | Origin R38 requires memory bounded *as history grows*; KTD14 bounds mounted widgets and takes one RSS reading over one fixed corpus while the domain transcript accumulates without eviction. | open |
| D10 | P1 | KTD10, PC9 | `viewport_rows` and `cursor_row` have no defined derivation from a transcript projection that has no cursor. | partially fixed |
| D11 | P1 | Problem Frame | DR15 remains unresolved on its own terms: a completed review panel or a recorded operator override is still required. | carried forward |
| D12 | P2 | KTD14 | Two of four gate thresholds — RSS growth and render ticks per second — have no measurement method, in a gate whose own text rejects subjective criteria. | open |
| D13 | P2 | U1, PC7, U10 | The CI workflow is Linux and Node-only while PC7's matrix is macOS arm64; no dependency lock artifact or version bound is named for `websockets`, and Textual's version appears only in prose. | open |
| D14 | P2 | KTD11 | "The source is re-read on every reconnect" is undefined when the source was an interactive prompt — either it re-prompts mid-stream and can block reconnect, or it caches and the sentence is wrong. | open |
| D15 | P2 | U5 | U5 must record corpus identity including a path into a committed document; this repository's public-context rule forbids local paths. Use an opaque label plus hash and frame count. | recorded in Open Questions |
| D16 | P1 | KTD5, KTD7, KTD11, U1, U7 | **No unit owns the configuration surface or the live entry point.** Three decisions read from "Talaria config" — the status executable and interval, the operator allowlist, the `0600` credential file — and nothing defines that file's location, format, key names, or its precedence against environment and flags. `talaria/cli.py` is absent from U1's files though U1's verification requires a runnable command; U2 adds `record` and U5 adds `replay`, but no unit ever adds the default run or KTD7's `--session`/`--resume` parsing. | open |
| D17 | P1 | PC10, U2, KTD6 | **The endpoint falsifier is unsatisfiable as scoped.** PC10 now requires that no URL-borne `ticket` or `internal` survive in the frame log, U2 is scoped to port the redactor "exactly" from a TypeScript reference the plan documents as blind to both, and KTD6 requires exact-`redactions` equivalence with that same reference. Closing the gap Python-side breaks the equivalence relation; not closing it fails PC10. No unit owns the choice. | open |
| D18 | P2 | U9, KTD4 | The paste-collapse threshold that switches KTD4's literal-insert behavior into an RPC round trip has no value, no unit (bytes, characters, or lines), and no configuration key anywhere in the plan. AE13's "several-hundred-line paste" does not pin it. | open |
| D19 | P2 | Traceability R1, R2, R3 | Three evidence cells cite artifacts no unit schedules: R1's process-surface assertion is assigned to "U7 or U10" — an undecided either/or inside a traceability table — while R2's "live startup acceptance in U10" and R3's "U10 live turn" appear in no unit's requirements, scenarios, or verification. | open |
| D20 | P3 | U5, KTD4, design diagram | Two replay-mode behaviors float unowned: what Enter does to composed text when no transport exists (a local echo would be indistinguishable from a sent message), and the diagram's dotted `ReplaySource → recorder` edge, which invites re-recording replayed corpora for no stated requirement. | open |
| D21 | P3 | U5, U6 | U5 is numbered before U6 but depends on it. Unit identifiers are never renumbered once assigned, so this is cosmetic and no action is proposed. | no action |

## External panel

Two engines were dispatched at the operator's request. Both were treated as advisory: every finding
was re-verified against this repository or the running Hermes checkout before adoption, and the
readiness verdict is the host's alone.

**codex / gpt-5.6-sol, max reasoning effort, read-only sandbox — completed, 13 findings.** It had
repository access and used it. Its C1 is the P0 in this review: it noticed that the plan's cited
Bearer-header helper takes a `Request`, not a `WebSocket`, and inferred the citation could not
govern the socket. That inference was checked line by line against the running checkout at the pin
and held. Nine further findings were confirmed on re-verification (C2 through C11 in its numbering,
appearing here as D2–D13 in various groupings); its C12 restated the DR15 block correctly. One
recommendation was declined: it proposed renumbering units topologically, which the plan contract
forbids once identifiers are assigned.

**ollama-cloud / kimi-k3, max reasoning effort — completed on the third dispatch, 13 findings.**
The first attempt timed out at a forty-minute socket read. The second, with streaming, returned
`finish_reason: length` after 92,657 characters of reasoning and **zero** characters of content: at
max effort the model exhausted a 24,000-token budget before beginning its answer. The third, with a
120,000-token budget, produced 101,339 characters of reasoning followed by a complete review. The
operational lesson generalizes — for this engine, `reasoning_effort: max` on a document of this size
needs an output budget several times larger than the expected answer, or the answer never starts.

Because it landed after the first round of fixes, it reviewed the **corrected** document, which made
it a check on this review's own work rather than a second opinion on the original. That turned out to
matter: seven of its thirteen findings were defects in the applied fixes, including the P0-severity
observation that the first round closed the frame-log boundary against URL-borne credentials while
leaving the status-child environment forwarding a credential through a filter the same review had
just documented as blind to it. All seven were verified and applied; they are the "fixes to the
fixes" table above. Its remaining findings became D16–D20. It had no repository access, so every
finding it raised is document-internal by construction — which is precisely why it caught
self-contradiction the repository-reading engine did not.

Neither engine's findings were adopted on its say-so. Codex's source claims were re-read line by line
against the running Hermes checkout at the pin; kimi's were checked against the plan text and, where
they touched the recorder or the format authority, against those files.

## Residual risk

The corrected credential mechanism is verified by source reading at `7f4d15515`, not by a live
attach. The same class of error that produced the P0 — reading the right file and the wrong
function — is only fully closed by connecting to a running gateway, which this review did not do.

The reconciliation catalogue from the Hermes turn controller remains unread; rules it surfaces could
still reshape normalization, which is why the plan schedules that read as U3's first task.

Textual's behavior under input methods, bracketed paste, and terminal restoration is unverified by
anyone — that is what the U5 gate exists to settle, and no reviewer can settle it from documents.

## Review result contract

- Target: `docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md`
- Classification: plan
- Reviewed revision: working tree, untracked, at repository `HEAD` `064967b`
- Readiness: **ready for `/work`**
- Blocking findings: none. All twenty-one dispositioned; PC2, PC4, and PC10 close; no open question blocks a unit
- Applied fixes: twenty-four groups across two review rounds, plus a third pass applying the operator's seven decisions; the plan and the execution spec both revalidate (`OK: talaria-v0-1-prototype (10 units)`)
- Review artifact: this file
- Override rationale: DR15's review-panel block is overridden by the operator and recorded in `docs/engineering-journal/DECISIONS.md` — it is a receipt gap against a verifier that does not exist, and the substantive review obligation was discharged by this review plus a two-engine external panel
- Next step: `/work` at U1
