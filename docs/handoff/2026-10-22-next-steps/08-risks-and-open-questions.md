# 08 — Risks, pre-mortem, and the decisions the operator owns

Status: `proposed`

Research reports are cited by number; for example, u11 is research/u11-language-framework.md, the
language and framework evaluation.

## Pre-mortem: the most likely ways this plan fails

Each risk names the earliest signal that it is happening and what to do about it.

| # | Risk | Earliest signal | Mitigation |
|---|---|---|---|
| 1 | **The rewrite stalls before it is good enough to use daily.** The old client is abandoned and nothing replaces it — the classic second-system failure. | Phase 1 is not usable for a real coding session after its units are done. | Phase 1 is deliberately a walking skeleton the operator can drive daily. Until the new client passes the phase 2 daily-driver check, use Hermes's own terminal UI (`hermes --tui --native`) rather than patching the Python client. |
| 2 | **Hermes's protocol churns faster than agents can follow.** Hermes merged about 6,000 pull requests in the v0.22.0 cycle alone, and the contract file had 219 commits in five weeks. | The scheduled drift job fails on most weeks, or capability strings disappear. | Pin Hermes stable releases; generate types; gate features on `runtime.describe`; bump the pin as an ordinary pull request; push the contract fixes listed in section "Upstream contributions" so fewer things are hand-typed. |
| 3 | **The ticket bootstrap changes.** It is a private control-socket verb documented only in code. | The attach test in continuous integration fails after a Hermes bump. | Fall back to `hermes gateway ensure --json` plus whatever Hermes's own terminal UI does at that release (`ui-tui/scripts/gateway_bootstrap.py`); ask upstream for a documented `hermes gateway connect --json`. |
| 4 | **Moderate coding agents cannot build the terminal layer** in Rust. The bake-off showed the risk is real but contained: the Rust Sonnet builder needed six fixes in the terminal layer, the TypeScript builders none ([research/u16-stack-bakeoff.md](research/u16-stack-bakeoff.md) §3). | P1-06 fails its acceptance at Opus 5.5 high, or later units need terminal-library workarounds. | P1-06 is staffed at Opus 5.5 high and starts from [reference-slices/rust/](reference-slices/rust/) and Codex's `insert_history.rs`; only its terminal module writes escape sequences; the patterns guide names the ratatui interfaces not to use. The switching conditions in [05-architecture.md](05-architecture.md) §Stack name when to move to TypeScript and OpenTUI. |
| 5 | **Inline rendering misbehaves in multiplexers.** Scroll regions and synchronized output differ across terminal multiplexers such as Herdr (built for running coding agents in panes), tmux and Zellij. | Duplicated or missing lines in scrollback after resize, or while another pane scrolls. | Test matrix from phase 1 (Herdr, tmux, Zellij, Ghostty, iTerm2, Terminal.app); Codex keeps a separate strategy for Zellij; keep a full-screen fallback flag. |
| 6 | **Desktop parity swallows the project.** Desktop has 89 features and a 415,000-line renderer. | Phase 4 grows units for "nice" or "skip" features before the core is solid. | The phase gates in [07-roadmap-and-work-units.md](07-roadmap-and-work-units.md); the skip list in [03-hermes-today.md](03-hermes-today.md) §7; schema-driven settings instead of hand-built forms; conversational configuration for the long tail. |
| 7 | **Process creep returns.** Receipts, digest-bound evidence and per-release gate documents reappear. | Pull requests where documents outweigh code. | Architecture decision record (ADR) 0012 (draft): automated tests, performance budgets, one live drive per release. |
| 8 | **Workflows run up cost or stall on approvals.** | A workflow run's spend exceeds its estimate; agents sit in "waiting on you". | Money budgets enforced by the runner; a roster-aware per-provider scheduler; read-only toolsets by default for review and research workflows; approvals surfaced in the needs-you queue with notifications. |
| 9 | **Hermes builds the same client.** Its own terminal UI gained a subagent dock, an editable queue and native scrollback in one release. | A Hermes release closes most of the parity table. | Differentiate where a third-party client can: speed, the fleet view across profiles, workflows across providers, configuration, and the operator's own conventions. Contribute generally useful pieces upstream. If Hermes's own UI becomes good enough, stopping is a legitimate outcome. |
| 10 | **Secrets leak** through recordings, logs, crash reports or workflow journals. | A secret-shaped string in a fixture or journal. | Redaction before disk (method deny-set, URL credentials by position, route-aware HTTP bodies); a secret scan over fixtures in continuous integration; tickets never logged. |

## Decisions the operator owns

These are listed in the order they block work. Each has a recommendation; none should be decided by
an implementing agent.

1. **Rewrite, and in which stack.** Recommendation: rewrite in Rust with ratatui
   ([05-architecture.md](05-architecture.md) §Stack, backed by the bake-off in
   [research/u16-stack-bakeoff.md](research/u16-stack-bakeoff.md)). The honest alternative is
   TypeScript with OpenTUI. It is easier for moderate agents to build, but it is about 20 times
   heavier at runtime and about 300 ms slower to attach. Accepting the recommendation means
   accepting draft ADRs 0007 to 0012.
2. **Same repository or a new one.** Recommendation: the same repository. Tag the Python era
   (`python-final` on the v0.6.5 commit), remove `talaria/`, `tests/`, `scripts/acceptance/`,
   `docs/acceptance/` and the other process records from the main branch in one commit (they stay in
   history and under the tag), and start the new workspace at the root. Agents searching the
   repository today wade through 2,234 acceptance files; a clean tree is worth more than continuity of
   paths.
3. **Name and distribution.** `talaria` is taken on crates.io, npm and the Python Package Index, and
   another Hermes client on GitHub uses it. Recommendation: keep the command name `talaria`, publish
   binaries through GitHub releases and a Homebrew tap, and do not publish to a package registry under
   that name.
4. **Non-Hermes agents in the fleet view** (Claude Code, Codex, Gemini CLI through the Agent Client
   Protocol). Recommendation: not now. Model internal events on ACP's vocabulary so it stays cheap to
   add later.
5. **A Hermes gateway on another machine as a daily path.** If yes, the remote sign-in work (gated
   dashboard, the native-app PKCE sign-in, in which an authorization code is exchanged with a
   one-time proof key, and a tunnel) moves from phase 6 into phase 3.
6. **Appetite for upstream contributions.** Several gaps are cheapest to fix in Hermes (below). Each
   is a small, well-scoped pull request against a project that merges outside contributions at a high
   rate.
7. **The interim daily driver.** Recommendation: use `hermes --tui --native` until the new client
   passes the phase 2 check, and make no further changes to the Python client. Its approval path is
   broken against current Hermes, and the cheapest fix is the rewrite's phase 1. If the Python client
   stays installed in the meantime, reinstall it with `uv tool install --compile-bytecode` (or set
   `UV_COMPILE_BYTECODE=1`). That was measured to cut a cold launch from about 1.1 s to about 0.4 s
   with no code change. A shell that exports `PYTHONDONTWRITEBYTECODE=1` probably slows other Python
   tools, including the Hermes command line, in the same way; that has not been measured.
8. **What "an always-running agent" means.** The set reads it as the backend keeping agents alive with
   no client attached (R5, requirement 5 in [01-vision-and-requirements.md](01-vision-and-requirements.md)).
   If the operator also means a named agent that is always available to message, adopt Desktop's model,
   a profile plus one hidden persistent chat, as a pinned row in the fleet view. If so, a "Resident
   agents" unit is added to phase 3: create, rename, message and open such an agent, using
   `session.create` on that profile and hiding the session from the session picker.

## Upstream contributions worth proposing to Hermes

From [u04](research/u04-hermes-client-surfaces.md) §6.2 and [u15](research/u15-hermes-orchestration.md)
§4.10, most valuable first:

1. Declare the canonical events (`approval.request`, `approval.settled`, `clarify.request`,
   `clarify.settled`, `session.replay_gap`, `session.updated`, `session.tools_changed`,
   `message.user`) in the contract, and extend the completeness test to the gateway's session modules.
2. Type `session.mutate` as a discriminated union per operation.
3. Declare error codes and an `error.data.reason` enum per method.
4. Version the contract meaningfully (a content hash or a semantic version (SemVer) in `info.version`, surfaced by
   `runtime.describe`).
5. Document a supported bootstrap for same-user third-party clients (for example
   `hermes gateway connect --json` printing a URL, subprotocols and a fresh ticket).
6. Per-task `model`, `provider` and `reasoning_effort` in `delegate_task`, restricted to an operator
   allow-list.
7. An `output_schema` on `prompt.submit` with the one-correction-turn contract `delegate_task` already
   implements.
8. Replay open `sudo` and `secret` requests to a viewer that attaches later, as approvals already are.
9. Kanban read and write methods on the canonical socket.
10. A device-code sign-in flow for remote terminal clients.
11. Later, once Talaria's runner has settled the API: a backend workflow runner (`workflow.*` methods
    and events).

## Open questions nobody could answer from the evidence

- Does the gateway's legacy-method fallback cover everything the configuration screens need, or do
  some calls only work through dashboard REST with a gateway ticket? Test early in phase 4.
- How does the gateway behave with ten or more concurrent workflow sessions in one process? Measure in
  phase 5 before raising the default width.
- Are canonical `sudo` and `secret` requests lost for a viewer that attaches after they were sent?
  (A hypothesis from reading the code; see [u04](research/u04-hermes-client-surfaces.md) §3.1.)
- Why was one daily profile served by a standalone dashboard but not by the gateway? First read
  `parked_profiles` in the `identify` reply. If the profile is not listed there, it was never
  discovered rather than parked; find out whether that is a configuration choice or a bug. The fleet
  view must explain it either way.
