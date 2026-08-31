# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. The `talaria-t1` run used the frozen wheel; `talaria-t2` evidence remains pending.

## Status

**NOT SATISFIED — the installed artifact passed, but the T1 run could not produce valid item
receipts.** The live gateway created sessions on an unapproved default route, the approved primary
route switch did not complete, several deterministic legs lacked their required real-session corpus,
and the headless pseudo-terminal had no permitted non-automated screenshot path. Raw captures remain
in tester scratch and are not committed.

The source checklist is the **Visual acceptance checklist** in
`docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`. The machine-readable owner registry is
`docs/acceptance/v0.5.0/checklist-items.json`. A passing row requires an immutable receipt matching
`docs/acceptance/v0.5.0/receipt.schema.json`, a raw terminal capture, a real-terminal screenshot, and
a completed redaction review.

## Candidate provenance

| Field | Value |
| --- | --- |
| Integration branch | `orch/talaria-v0-5-0` |
| Full candidate commit | `d86979127f871a479eb104fc10c886b5c5480a8c` |
| Wheel filename | `talaria-0.5.0-py3-none-any.whl` |
| Wheel Secure Hash Algorithm 256-bit (SHA-256) digest | `a165ad24bd2a4baa7d11aec5d5f434e1451fd688661fed1fe8919ca0c65a1afb` |
| Installed version | `0.5.0` |
| `talaria-t1` install receipt | tester scratch `install-receipt.json` — valid |
| `talaria-t2` install receipt | `PENDING` |

The install probe must reject a source checkout, editable installation, or global executable. Both
testers must install the same candidate wheel digest into distinct fresh virtual environments and use
distinct scratch configuration directories through `TALARIA_CONFIG_DIR`.

## Live model route status

The primary route is **OpenCode Muse Spark 1.2 Contributor Free**. The only permitted fallback is
**Ollama GLM 5.3 Flash**, and only for primary unavailability, connection failure, model-not-found, or
bounded-test incompletion. Every live receipt records the requested route, observed route, route
status, fallback availability, and the exact fallback reason when applicable.

The final dispatch reported Ollama GLM 5.3 Flash available. T1 did not use it: the observed problem
was that the live gateway created the session on `gpt-5.5`, an unapproved third route, and its
session-scoped switch to the primary route did not complete before Talaria's bounded slash-command
fallback. No fallback reason was therefore applicable.

## Safety envelope

1. Every application drive uses the executable proven by that tester's install receipt.
2. Raw American National Standards Institute (ANSI) terminal bytes remain in tester scratch until
   credential and private-identifier review. Unsafe material is withheld, not committed.
3. Deterministic flows may use the shipped `talaria replay` against frame-log corpora. Every live leg
   uses a real Hermes-backed throwaway session.
4. No Computer Use, GUI automation, mocked acceptance, or simulated Talaria application satisfies a
   row.
5. A timeout, empty capture, missing screenshot, missing route, silent substitution, unavailable
   required fallback, hang, or blank terminal state fails or blocks the row visibly.
6. Any operator-reserved step stays `RESERVED`; it is never simulated or converted into a pass.

## Evidence matrix

Shared rows require independent receipts from both testers. A dash means the tester does not own that
row. Receipt, capture, and screenshot paths are added only after sanitization review.

| Item | Checklist item | Owner tester | `talaria-t1` verdict | `talaria-t2` verdict | Receipt / capture / screenshot | Observation |
| ---: | --- | --- | --- | --- | --- | --- |
| 1 | Installed artifact | both independently | `PASS` | `PENDING` | tester scratch `install-receipt.json` | Frozen wheel identity, version, bare launch, and complete 50,000-delta gate report were proven. |
| 2 | Live primary route | both independently | `FAIL` | `PENDING` | `raw/item-02*.ansi`; no item receipt or screenshot | Gateway connected, but created `gpt-5.5`; the approved Muse switch did not complete and no fallback was used. |
| 3 | Main hierarchy | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 4 | Refined Default | `talaria-t1` | `BLOCKED` | — | `raw/item-04.ansi`; no item receipt or screenshot | The drive opened the picker only at its final acceptance key, so it did not capture the promised preview round trip. |
| 5 | Dark Green Terminal | `talaria-t1` | `BLOCKED` | — | `raw/item-05.ansi`; no item receipt or screenshot | The drive did not move within the picker after it opened; the target theme was not exercised. |
| 6 | Neutral Dark | `talaria-t1` | `BLOCKED` | — | `raw/item-06.ansi`; no item receipt or screenshot | The drive did not move within the picker after it opened; the target theme was not exercised. |
| 7 | Accessible High Contrast | `talaria-t1` | `BLOCKED` | — | `raw/item-07.ansi`; no item receipt or screenshot | The theme was not selected and the real-session corpus had no completed diff for the contrast leg. |
| 8 | Preview cancellation | `talaria-t1` | `BLOCKED` | — | `raw/item-08.ansi`; no item receipt or screenshot | The picker never opened, so no preview or cancellation occurred. |
| 9 | Explicit save and precedence | `talaria-t1` | `BLOCKED` | — | `raw/item-09-*.ansi`; no item receipt or screenshot | All three drives exited cleanly, but neither user nor repository configuration changed; the scripted picker/save sequence did not execute its condition. |
| 10 | Theme fallback notice | `talaria-t1` | `BLOCKED` | — | `raw/item-10-*.ansi`; no item receipt or screenshot | The installed importer reported 19 warnings and 56 fallbacks, and startup visibly reported the unknown name and Refined Default fallback; a required screenshot was unavailable. |
| 11 | Visual Studio Code import | `talaria-t1` | `BLOCKED` | — | `raw/item-11-*.ansi`; no item receipt or screenshot | Two imports produced identical bytes; startup loaded `vscode-import-evidence` without a fallback notice. The held-diff and screenshot portions were unavailable. |
| 12 | All status segments | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 13 | Status configuration | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 14 | Status responsive sequence | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 15 | Status failure visibility | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 16 | Inspector dock and resize | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 17 | Inspector content and empty states | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 18 | Inspector responsive state | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 19 | Side-by-side diff | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 20 | Unified fallback | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 21 | Diff navigation and boundary | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 22 | Composer caret location | `talaria-t1` | `BLOCKED` | — | `raw/item-22.ansi`; no item receipt or screenshot | Focus labels for transcript, prompts, and inspector were visible, but the corpus lacked the required populated agent and task rows. |
| 23 | Connection non-color states | `talaria-t1` | `BLOCKED` | — | `raw/item-23.ansi`; no item receipt or screenshot | `[x] down` and `[ok] up` were visible, but the shared gateway was not stopped or restarted, so reconnecting and authentication failure were not exercised. |
| 24 | Agent and queue non-color states | `talaria-t1` | `BLOCKED` | — | `raw/item-24.ansi`; no item receipt or screenshot | No sanitized real-session corpus containing the seven required agent plateaus and four queue plateaus was available. |
| 25 | Transcript identity without color | `talaria-t1` | `BLOCKED` | — | `raw/item-25.ansi`; no item receipt or screenshot | The available real-session recording contained operator, assistant, session, and error content, but not all six required transcript kinds. |
| 26 | Reduced motion | `talaria-t1` | `BLOCKED` | — | `raw/item-26-*.ansi`; no item receipt or screenshot | Both restart values ran, but the corpus had no working agent or controlled gateway bounce from which to judge the motion difference. |
| 27 | Stable unpinned scroll | `talaria-t1` | `BLOCKED` | — | `raw/item-27.ansi`; no item receipt or screenshot | The real-session recording was shorter than one viewport and could not establish the required stable middle anchor. |
| 28 | Stable pinned scroll | `talaria-t1` | `BLOCKED` | — | `raw/item-28.ansi`; no item receipt or screenshot | The real-session recording was shorter than one viewport and could not establish bottom-follow and later manual-unpin behavior. |
| 29 | Wide screenshot | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 30 | Narrow screenshot | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 31 | Malformed Visual Studio Code import | `talaria-t1` | `BLOCKED` | — | `raw/item-31.ansi`; no item receipt or screenshot | Installed command exited 2 with the required strict-JSON error and created no theme; a required screenshot was unavailable. |
| 32 | Session-only status toggle | `talaria-t2` | — | `PENDING` | `PENDING` | `PENDING` |
| 33 | Dead gateway credential | both independently | `BLOCKED` | `PENDING` | no capture, item receipt, or screenshot | Producing a genuinely stale credential required a gateway restart or revocation; neither was authorized against the shared gateway. |
| 34 | Killed session | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |
| 35 | Restart-only configuration | both independently | `BLOCKED` | `PENDING` | `raw/item-35-*.ansi`; no item receipt or screenshot | A live config edit left the running Refined Default colors unchanged and the next launch loaded Neutral Dark; a required screenshot was unavailable. |
| 36 | Cross-tester evidence | both independently | `PENDING` | `PENDING` | `PENDING` | `PENDING` |

## Terminal and route summary

| Tester | Terminal program | `TERM` value | Dimensions exercised | Session profile | Live route observed | Fallback reason | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `talaria-t1` | Python `pty.fork` on macOS | `xterm-256color` | 31–180 columns by 36–60 rows | isolated acceptance profile against the supplied gateway | none approved; gateway exposed `gpt-5.5` | none; fallback not used | `NOT SATISFIED` |
| `talaria-t2` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` | `PENDING` |

## Honest halt record

Record any prerequisite or runtime halt here. State the observed cause first, then the affected item,
tester, receipt path, and next authority. Do not turn a missing prerequisite into a product defect or
improvise around it.

The installed-artifact leg issued a valid install receipt. Its gate report retained the unchanged
50 ms ceiling and failed `workload_latency_growing-one-column-table` at 65.836 ms; the install
decision excluded only that named check, recorded v0.5 samples of 54.45, 60.229, 68.861, and
65.836 ms with a 14.411 ms spread, and required every other check to pass.

The live-route leg then connected to the gateway but received a `gpt-5.5` session. That is not one
of the two approved receipt routes. `/model muse-spark-1.2-contributor-free --provider opencode-free`
did not return within the slash-command bound; Talaria's fallback `command.dispatch` returned
`not a quick/plugin/bundle/skill command: model`. No third model was accepted and no fallback was
silently substituted.

The pseudo-terminal driver preserves raw ANSI bytes but deliberately does not manufacture images.
This headless run had no permitted non-automated real-terminal screenshot path, so the receipt
validator correctly prevented every item except the separate install receipt from becoming a
passing immutable receipt.

Items 24–28 also lacked the specialized sanitized real Hermes frame-log corpora named by their event
scripts. Item 23 required control of the shared gateway lifecycle, and item 33 required a real stale
credential. Neither external mutation was taken without authority.

## Final verdict

**NOT SATISFIED.** T1 proved the frozen installed artifact and captured useful installed-binary
observations, including the Visual Studio Code importer fix, but the live primary route failed and
the required item receipts and screenshots do not exist. No blocked or failed row is treated as a
pass.
