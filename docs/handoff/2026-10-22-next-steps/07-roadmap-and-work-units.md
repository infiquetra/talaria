# 07 — Roadmap and work units

Status: `proposed`

This is the executable plan. It is written so that a coding agent of moderate capability can pick up
one unit, read only what the unit names, and finish it with tests that prove it done. Phases 0 and 1
are specified in full, because everything rests on them and they carry the protocol risk. Later phases
are specified to the level of goal, Hermes calls and acceptance; **re-plan each later phase at its
start** with a planning agent, using the code that exists by then.

This plan assumes the Rust stack chosen in [05-architecture.md](05-architecture.md) §Stack and in
architecture decision record (ADR) 0010 ([ADR-0010](../../../platform-specs/04-architecture/adrs/0010-rust-and-ratatui.md)).
If a switching condition there fires, phases 0 and 1 are re-planned: the unit boundaries, interfaces
and tests carry over, but the file paths and commands do not.

Research reports are cited by number; for example, u11 is
[research/u11-language-framework.md](research/u11-language-framework.md), the language and framework
evaluation.

## Rules for every unit

1. **One unit, one branch, one pull request.** Branch name `p<phase>-<nn>-<slug>`, for example
   `p1-04-recorder`. Conventional commit messages (`feat(client): ...`). No attribution lines of any
   kind in commits, pull requests or code.
2. **Read the unit's "Read first" list before writing code.** Verify every Hermes fact against the
   Hermes revision pinned in `crates/hermes-contract/PINNED` (set in P0-03), not against this document.
3. **Touch only the files the unit owns.** If you must touch another file, say so in the pull request
   and keep the change minimal.
4. **Tests first where the unit names tests.** A test must fail before the change and pass after it;
   say in the pull request that you saw it fail.
5. **The project check must pass:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test --workspace`, plus the unit's own acceptance commands.
6. **Coding rules** (from P0-04's patterns guide): no `unsafe`; no `Rc<RefCell<…>>` webs; application
   state is owned data passed through channels; every source of change sends a message into the single
   application event channel; domain crates never import the terminal library.
7. **Record non-obvious learnings** in `docs/engineering-journal/LEARNINGS.md` in the same pull
   request, with evidence and a one-line rule. Do not write planning codes into source comments.
8. **Model tier.** Each unit names a suggested tier. The tiers, lowest first, are: Haiku 5.5 xhigh
   (mechanical work that existing tests prove); Sonnet 5.5 high (a settled unit, including its
   tests); Opus 5.5 medium (a unit that spans subsystems); Opus 5.5 high (a unit retried after a
   failed review, or one that sets conventions). A unit that fails review once is retried at the next
   tier in this list. At the top tier, a second failure goes to the operator. With other providers,
   use the nearest equivalent and say which one in the pull request.

**Every phase ends with an operator check.** Phases do not overlap; the next phase starts when the
operator says the check passed.

```text
P0 decide and reset ──► P1 walking skeleton ──► P2 coding parity ──► P3 fleet
                                                                     │
                     P6 later ◄── P5 workflows ◄── P4 configuration ◄┘
```

---

## Phase 0 — Decide and reset

### P0-01 — Operator decisions
- **Who:** the operator. No agent work.
- **Decide:** accept or amend ADR drafts 0007 to 0012 ([adr-drafts/](adr-drafts/)); the repository
  question and the name question in [08-risks-and-open-questions.md](08-risks-and-open-questions.md).
- **Done when:** the drafts are moved into `platform-specs/04-architecture/adrs/` with status
  `accepted`; ADR-0001 (standalone client; its transport half), ADR-0003 (re-encode Hermes behaviour;
  its protocol half), ADR-0004 (Python), ADR-0005 (Textual) and ADR-0006 (bounded block rendering) are
  marked superseded; the decision is recorded in `docs/engineering-journal/DECISIONS.md`; and the
  operator has agreed to replace the required status checks on `main` before the P0-02 pull request
  merges (P0-02 step 6).

### P0-02 — Reset the repository and create the workspace
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P0-01.
- **Read first:** this document; [05-architecture.md](05-architecture.md) §Layers; the current
  `README.md`, `AGENTS.md`, `CLAUDE.md`, `.github/workflows/`.
- **Steps:**
  1. Tag the current `main` as `python-final` (annotated tag: "Last commit of the Python client,
     v0.6.5").
  2. In one commit, sort every tracked file into one of three groups.
     - **Remove:** `talaria/`, `tests/`, `scripts/`, `src/`, `package.json`, `package-lock.json`,
       `tsconfig.json`, `vitest.config.ts`, `pyproject.toml`, `uv.lock`, `.python-version`,
       `docs/acceptance/`, `docs/plans/`, `docs/reviews/`, `docs/code-reviews/`,
       `docs/work-sessions/`, `docs/brainstorms/`, `docs/ideation/`, `docs/evidence/`, `docs/design/`,
       `docs/releases/` (move the release history into `CHANGELOG.md` if it is not already there),
       `GEMINI.md`, `IDEA.md`, `.prettierignore`, `.github/copilot-instructions.md`,
       `.github/workflows/validate.yml`, `docs/examples/`, `docs/install.md`, `docs/themes.md`,
       `docs/terminal-ui.md`.
     - **Rewrite short:** `README.md`, `AGENTS.md`, `CLAUDE.md`, `llms.txt`, `docs/00-index.md`,
       `docs/README.md`, `.github/workflows/release.yml` (see steps 4 to 6).
     - **Keep unchanged:** `.gitattributes`, `.gitignore` (add `/target`),
       `.github/{CODEOWNERS,ISSUE_TEMPLATE,PULL_REQUEST_TEMPLATE.md,dependabot.yml}`,
       `docs/configuration.md` (it is the reference for the current `~/.talaria/config.toml` until
       P1-08 replaces it), `docs/public-safe-summary.md`, `LICENSE`, `CHANGELOG.md`,
       `platform-specs/`, `docs/analysis/`, `docs/engineering-journal/`, `docs/formats/`,
       `docs/handoff/`.

     Leave the untracked, git-ignored `corpora/` directory alone; it is not part of the repository.
  3. Create the Cargo workspace at the repository root:

     ```text
     Cargo.toml                 workspace; edition 2024; shared lints
     rust-toolchain.toml        pinned stable toolchain
     crates/
       hermes-contract/         generated types (P0-03)
       hermes-client/           discovery, tickets, WebSocket, JSON-RPC, replay (P1-01..03)
       talaria-core/            domain state and reducers; no terminal imports (P1-05)
       talaria-app/             application services (P1-12)
       talaria-tui/             rendering, input, overlays (P1-06..10)
       talaria-workflow/        workflow runner (phase 5; empty crate for now)
       talaria-cli/             the `talaria` binary
     tools/
       contract-gen/            contract preprocessing and generation (P0-03)
       bench/                   performance harness (P1-11)
     fixtures/frames/           recorded frame logs for replay tests (P1-04)
     docs/agents/patterns.md    the patterns guide (P0-04)
     ```
  4. Replace `README.md`, `AGENTS.md`, `CLAUDE.md`, `llms.txt`, `docs/00-index.md` and `docs/README.md`
     with short versions for the new workspace: purpose, the project check, the rules above, links to
     this handoff and to the ADRs. Keep the public-safety and no-attribution rules.
  5. Continuous integration: one workflow running `cargo fmt --check`, `cargo clippy --all-targets --
     -D warnings`, `cargo test --workspace` on macOS and Linux. Delete the Python and npm steps of
     `.github/workflows/release.yml` in the same commit.
  6. **Operator action, not agent work:** before this pull request merges, replace the required
     status checks on `main` (`python-check (3.12)`, `python-check (3.13)`) with the new workflow's
     job names (for example `rust-check (macos-14)` and `rust-check (ubuntu-latest)`). Without this
     the pull request stays blocked, because deleting `pyproject.toml` means the old checks never
     report. Name the jobs so that no matrix key leaks into a job name.
- **Done when:** `cargo build --workspace` and the CI workflow pass on the pull request; `git ls-files
  | wc -l` is under 300; the tag exists; the operator has replaced the required status checks
  (step 6).

### P0-03 — Vendor and generate the Hermes contract
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P0-02.
- **Read first:** [03-hermes-today.md](03-hermes-today.md) §3; [research/u11-language-framework.md](research/u11-language-framework.md)
  §3.4 (what broke `cargo typify` and how it was fixed); [research/u04-hermes-client-surfaces.md](research/u04-hermes-client-surfaces.md)
  §4; in Hermes: `apps/shared/src/gateway-contract.openrpc.json`, `apps/shared/src/gateway-events.ts`,
  `scripts/gen_gateway_contracts.py`.
- **Owns:** `tools/contract-gen/`, `crates/hermes-contract/`.
- **Steps:**
  1. Pin the newest Hermes stable release at the time the unit starts (v0.22.4 on 2026-10-21; find it
     with `gh api repos/NousResearch/hermes-agent/releases/latest --jq .tag_name`). The pin must be
     v0.22.3 or later, the first stable release with the session authority. `crates/hermes-contract/PINNED`
     holds `tag=<tag>` and `commit=<sha>`; `crates/hermes-contract/vendor/gateway-contract.openrpc.json`
     is that tag's file, byte for byte. Precondition the operator sets up before any live test in
     phase 1: the local Hermes is at the pinned tag and its gateway has been restarted (agents do not
     run `hermes update` or restart the gateway). A live test skips with a plain message when the
     `code_sha` that `identify` reports differs from `PINNED`.
  2. `tools/contract-gen` (a small Rust or Python script; Python is acceptable here because it runs
     only at generation time) lifts `components.schemas` into a JSON Schema document, strips every
     `additionalProperties: false`, drops the defaults that the generator rejects (the language and
     framework report, u11 §3.4, found three: `DisplayLease`, `MessageCompletePayload`,
     `ProviderUsageWindow`), and runs
     `cargo typify` to produce `src/generated/schemas.rs`.
  3. It also emits `src/generated/methods.rs`: one module per transport (`legacy` from `methods`,
     `canonical` from `x-canonical-methods`), each with a `Params`/`Result` type pair per method and a
     `const NAME: &str`; and `src/generated/events.rs`: one enum over the 79 `x-notifications` (tagged
     by event `type`) and one over the 16 `x-server-requests`, each with an `Unknown(serde_json::Value)`
     variant. It also emits `src/generated/native.rs`: a `NativeMethod` marker trait implemented for
     every canonical method, and for every legacy method that the gateway serves on the native socket
     (owner-implemented, or allowed by `tui_gateway/ws_legacy_fallback.py` in Hermes); where a name is
     in both catalogues, only the canonical type implements it. The gateway refuses the other legacy
     `session.*` verbs and the process-global verbs on this socket with error -32601.
  4. `src/canonical_events.rs`, hand-written: `approval.request`, `approval.settled`,
     `clarify.request`, `clarify.settled`, `session.replay_gap`, each citing
     `apps/shared/src/gateway-events.ts` at the pinned commit in a doc comment.
  5. Known contract errors are corrected in one place, `src/overrides.rs`, each with a comment and a
     test. The first one: `prompt.submit` on the canonical socket **requires** `submission_id` (or
     `input_id`) although the schema marks both optional; every builder in the stack bake-off hit this
     (report: [research/u16-stack-bakeoff.md](research/u16-stack-bakeoff.md)).
  6. A CI job `contract-drift` (weekly and on demand) regenerates against the newest Hermes stable
     release and fails if the generated code changes in a way that breaks the build.
- **Done when:** the generated crate compiles; a round-trip test decodes a real `gateway.ready`,
  `message.delta`, `tool.complete` and `approval.request` frame taken from `fixtures/` (until P1-04
  records fresh ones, copy them from the scrubbed bake-off recordings in
  [reference-slices/rust/tests/fixtures/](reference-slices/rust/tests/fixtures/)); an unknown field in
  any payload does not fail decoding (test it).

### P0-04 — The patterns guide
- **Tier:** Opus 5.5 high (it sets the conventions every later unit copies).
- **Depends on:** P0-02. Read [reference-slices/rust/](reference-slices/rust/) first.
- **Owns:** `docs/agents/patterns.md`.
- **Content:** six worked patterns with code: an application event enum and the single channel; a
  pure reducer `update(&mut State, Event) -> Vec<Effect>`; an effect executor that turns effects into
  client calls; a render function from state to a frame with a snapshot test; a transport message
  round-trip test from a recorded frame; a new overlay. Pin the versions of ratatui, crossterm and
  tokio and name APIs not to use — at least ratatui's `Viewport::Inline` resizing after start-up and
  `Terminal::insert_before` for history (why: [research/u16-stack-bakeoff.md](research/u16-stack-bakeoff.md)
  §3). State that only `talaria-tui`'s terminal module writes escape sequences. Keep it under 400
  lines.
- **Done when:** a fresh agent given only the guide can add a trivial overlay with a snapshot test
  without asking questions (try it with a Haiku-tier agent and record the result in the journal).

---

## Phase 1 — Walking skeleton

**Goal:** a client the operator can code with every day, with the right foundations. Exit check at
the end of the phase.

### P1-01 — Find the gateway and connect
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P0-03.
- **Read first:** [03-hermes-today.md](03-hermes-today.md) §2; in Hermes:
  `ui-tui/scripts/gateway_bootstrap.py`, `hermes_cli/gateway_client.py` (`connect_gateway`,
  `gateway_ws_target`), `gateway/control_socket.py` (`identify`, `_session_ticket`),
  `gateway/runtime_bootstrap.py` (`TicketStore.mint`), `gateway/run_api.py` (the `/api/ws` upgrade),
  `apps/shared/src/json-rpc-gateway.ts` (connect timeout, handshake order); the attach code in
  [reference-slices/rust/src/net.rs](reference-slices/rust/src/net.rs), which worked live.
- **Owns:** `crates/hermes-client/src/{discovery.rs,ticket.rs,connect.rs}`, `crates/hermes-client/tests/connect.rs`.
- **Provides:**
  - `discover(home) -> Result<Endpoint, DiscoveryError>`: connects to `<home>/gateway.sock` (or the
    path in `gateway.sock.path`), checks the peer is the same user, sends `identify`, parses
    `api_origin`, `instance_id`, `served_profiles`, `capabilities`, `code_version`.
  - `mint_ticket(home, &Endpoint, profile_id: &ProfileId, purpose: Purpose, scope: Scope) ->
    Result<Ticket, TicketError>`: a new socket connection per request; `Ticket` does not implement
    `Debug` or `Display` with its value. `Purpose` is `Interactive | NativeHttp` (others later).
    `Scope::Host` can be constructed only together with `Purpose::Interactive`. `ProfileId` is the
    absolute path of the profile home, exactly as `identify` lists it in `served_profiles`. `discover`
    also returns a name-to-id map (the last path component, or `default` for the Hermes home) for
    `--profile <name>`.
  - `connect(&Endpoint, Ticket) -> Result<Connection, ConnectError>`: WebSocket to
    `<api_origin as ws>/api/ws` with subprotocols `hermes-gateway-v1` and
    `hermes-gateway-ticket.<ticket>`, no `Origin` header, 15 s connect timeout; checks the echoed
    subprotocol.
  - `handshake(&mut Connection) -> Result<Handshake, HandshakeError>`: waits for `gateway.ready`,
    sends `client.capabilities {server_requests: true}`, calls `runtime.describe`.
  - When no gateway answers: run `hermes gateway ensure --json` once (never with `--idle-exit`) and
    retry discovery; map its exit codes (0 ready, 3 incompatible, 4 authorization, 5 deadline,
    6 draining, 7 conflict) to errors with a plain-language next step.
- **Done when:** tests against an in-process mock (a Unix socket plus a WebSocket server) cover:
  success; stale `instance_id`; unknown profile ("bootstrap binding rejected"); expired ticket; wrong
  echoed subprotocol; no `gateway.ready` within 15 s. An ignored live test
  (`cargo test -p hermes-client --test live_attach -- --ignored`) passes against a running gateway and
  prints the timings of each step without the ticket.

### P1-02 — The JSON-RPC channel
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-01.
- **Read first:** `apps/shared/src/json-rpc-channel.ts` and `json-rpc-gateway.ts` in Hermes (the
  behaviour to match); [05-architecture.md](05-architecture.md) §Protocol rules.
- **Owns:** `crates/hermes-client/src/{channel.rs,rpc.rs}`, tests.
- **Provides:** `Client::call::<M: NativeMethod>(params) -> Result<M::Result, CallError>` with typed
  methods from `hermes-contract` (`NativeMethod` is emitted in P0-03); a stream of `Inbound` items (`Event`,
  `ServerRequest` with a responder handle); per-call timeouts (default 120 s; `approval.respond` 300 s); a per-connection
  generation number on every inbound item; `CallError::Unknown` when the connection drops before a
  reply (never treat it as success).
- **Done when:** tests cover concurrent calls with out-of-order replies; an error reply with `code` and
  `data.reason`; a server request answered through its responder; a reply arriving after the
  connection was replaced is discarded; a dropped connection yields `Unknown` for pending calls; a compile-fail test (trybuild or a doc
  test) shows that `legacy::session_compress` does not satisfy `NativeMethod`.

### P1-03 — Liveness, reconnect and replay
- **Tier:** Opus 5.5 medium (it spans the client and the session model).
- **Depends on:** P1-02.
- **Read first:** [research/u04-hermes-client-surfaces.md](research/u04-hermes-client-surfaces.md) §3;
  `apps/shared/src/json-rpc-gateway.ts` (replay after reconnect, the replay gap, heartbeat),
  `apps/shared/src/reconnect-backoff.ts`.
- **Owns:** `crates/hermes-client/src/{liveness.rs,replay.rs}`, tests.
- **Provides:** ping every 15 s with a 45 s deadline; reconnect with backoff from 1 s to 30 s (a fresh
  ticket each time); for every subscribed session, `session.events.since` from its last sequence
  number, delivering missed events in order; when the reply says `snapshot_required` (or `truncated`),
  a `ReplayGap` event that makes the session service call `session.resume`; detection of a backend
  restart through `replay_epoch`.
- **Done when:** a mock-server test kills the socket mid-stream and the consumer sees every event
  exactly once and in order; a truncated replay produces exactly one `ReplayGap`; an epoch change
  forces a resume.

### P1-04 — Recorder and replay source
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-02.
- **Read first:** `docs/formats/frame-log.md` (Talaria's language-neutral format); the redaction rules
  in [02-current-state.md](02-current-state.md) §What to carry; `python-final:talaria/recorder/redact.py`
  for the deny-set.
- **Owns:** `crates/hermes-client/src/{record.rs,replay_source.rs}`, `tools/record-fixture/`,
  `fixtures/frames/`, `docs/formats/frame-log.md` (bump to version 3).
- **Provides:** a `Recorder` that `hermes-client::Connection` accepts as an option (P1-12 exposes it
  as `talaria --record <path>`) writes every inbound and outbound frame as JSON Lines with a
  monotonic timestamp and connection generation, **redacted before writing** (ticket subprotocols
  never recorded; secret-bearing methods per the deny-set; URL credentials by position).
  Outbound response frames to server requests are redacted by correlation: the recorder remembers the
  `method` of each inbound server request by its `id`, and when the outbound response for a `sudo`,
  `secret`, `vault.*` or `clarify` request is written, its whole `result` is withheld. A test records a
  `sudo` exchange and asserts the answer never reaches disk. `ReplaySource` drives the same pipeline
  from a file with no socket, at recorded speed or instantly.
  A fixture-recording script runs a fixed set of prompts against a live gateway on a free model and
  writes `fixtures/frames/*.jsonl`: a plain answer, a tool call with a diff, an approval, a
  clarification, an interrupted turn, a subagent batch, a queued second prompt.
  - **Precondition the operator sets up** (agents do not create or edit Hermes profiles, and do not
    start or restart the gateway): a throwaway profile named `fixture`, whose `config.yaml` sets
    `approvals.mode: manual`, served by the running gateway (or a separate throwaway Hermes home with
    its own gateway, if the operator prefers), and a scratch project under `$TMPDIR`. Under the default mode ("smart") a guardian
    model may auto-approve a flagged command, so no approval card would appear; under `manual`
    every flagged command (`rm -rf .pytest_cache` matches the recursive-delete pattern) raises
    `approval.request`. The recording script skips with a plain message that states this
    precondition when it is absent.
  - Before writing, the script rewrites every absolute path under `$HOME`, and every profile id and
    home, to the placeholders `<home>` and `<profile>`.
- **Done when:** each fixture, read back through `ReplaySource`, yields the same ordered sequence of
  decoded inbound frames that the recorder wrote (a round-trip test); after replay, each fixture ends
  with the `message.complete` status the recording script expected; a scan over `fixtures/` finds no
  secret-shaped string, no occurrence of `$HOME` or the operator's username, and no profile name
  other than `fixture`; `docs/formats/frame-log.md` describes version 3.

### P1-05 — The session reducer
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P0-03, P0-04, P1-04 (fixtures).
- **Read first:** [05-architecture.md](05-architecture.md) §Protocol rules;
  `python-final:docs/analysis/2026-08-02-hermes-reconciliation-rules.md` and
  [research/u03-talaria-history.md](research/u03-talaria-history.md) §3.1 (re-check each rule
  against the pinned Hermes; several are obsolete).
- **Owns:** `crates/talaria-core/src/session/`, tests.
- **Provides:** `SessionState` with a list of cells (user, assistant, reasoning, tool, notice, prompt)
  and a live tail; `update(&mut SessionState, Event) -> Vec<Effect>` handling `message.start`,
  `message.delta` (null text is a segment boundary), `message.interim`, `message.complete` (outcome
  from its status), `reasoning.delta`, `tool.start`, `tool.complete` (`is_error`, summary, optional
  diff), `todo.updated`, `status.update`, `session.usage` (field-wise merge), `approval.request` and
  `.settled`, `clarify.request` and `.settled`, `error`. Prompts are keyed by (`session_id`,
  `prompt_id`, `execution_generation`) as carried on `approval.request` / `clarify.request`; a
  response always echoes all three. The interrupt latch. Unknown events surfaced once per connection.
  A cell becomes "final" when nothing can change it again; the renderer commits final cells to
  scrollback. Also `SessionState::from_snapshot(CanonicalSnapshot) -> SessionState`, which builds
  final cells from `display_messages` (or from `messages` when absent), restores open approval and
  clarification prompts from `pending`, and sets the replay cursor from `last_sequence` and
  `replay_epoch`; a user cell is created locally on submit and from the snapshot on resume.
- **Done when:** a replay test per fixture asserts the final cell list (snapshot); targeted tests cover
  the interrupt latch, a late `message.complete`, usage merging, and a duplicate approval keyed by
  (`session_id`, `prompt_id`, `execution_generation`); a snapshot fixture (a recorded `session.resume`
  result) produces the same cell list as replaying that session's events.

### P1-06 — Inline renderer
- **Tier:** Opus 5.5 high. This is the hardest building unit in the plan. The bake-off put six of the
  Rust builder's eleven fixes here, so it is not handed to a moderate tier.
- **Depends on:** P0-04, P1-05.
- **Read first:** [ADR-0009](../../../platform-specs/04-architecture/adrs/0009-finished-output-lives-in-native-scrollback.md);
  [reference-slices/rust/src/tui.rs](reference-slices/rust/src/tui.rs), which worked in the bake-off;
  [research/u16-stack-bakeoff.md](research/u16-stack-bakeoff.md) §3; OpenAI Codex
  `codex-rs/tui/src/insert_history.rs` and `custom_terminal.rs` (Apache-2.0; if you copy code, keep
  the licence notice in `NOTICE` and the file header).
- **Known traps (from the bake-off):**
  - Do not resize ratatui's `Viewport::Inline` after start-up. It asks the terminal for the cursor
    position, and crossterm cannot read the answer while the key reader runs. Use a fixed viewport
    that this module moves, or query the cursor once before the key reader starts.
  - Do not commit history with `Terminal::insert_before`. Its scroll-up sequence (`CSI S`) may not
    reach native scrollback. Use newline scrolling inside a scroll region anchored at the top, as
    Codex and the reference slice do.
  - Create crossterm's event stream only when standard input is a terminal; it panics otherwise.
- **Switching condition:** if this unit fails its acceptance twice at this tier, stop and raise the
  stack question with the operator ([05-architecture.md](05-architecture.md) §Stack).
- **Owns:** `crates/talaria-tui/src/{terminal.rs,history.rs,live.rs}`, tests.
- **Provides:** terminal setup and restore (raw mode, bracketed paste, keyboard enhancement flags when
  supported, no mouse capture); an inline viewport of variable height at the bottom; `commit(cells)`
  writes finished cells above the viewport into native scrollback; the live region redraws only when
  something visible changed, wrapped in synchronized output when the terminal reported support at
  startup; resize recomputes the viewport without duplicating or losing committed lines; a full-screen
  overlay API that enters the alternate screen and restores the inline view on close.
- **Done when:** `vt100`-emulator tests prove: committed lines appear in scrollback exactly once
  across a resize; the first line is not lost when the viewport starts at the top of the screen (a
  bake-off finding); closing an overlay restores the inline view; the live region does not grow
  unbounded. A manual check in Herdr (a terminal multiplexer, like tmux), in tmux and in one plain terminal is
  recorded in the pull request.

### P1-07 — Streaming Markdown and code
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-06.
- **Read first:** Codex `codex-rs/tui/src/markdown_stream.rs` and `markdown_render.rs`; [the language and framework report,
  research/u11-language-framework.md](research/u11-language-framework.md) §3.3 (Markdown cost).
- **Owns:** `crates/talaria-tui/src/markdown/`, tests, a benchmark.
- **Provides:** incremental Markdown with pulldown-cmark: finished blocks render once and are
  committed; only the trailing block re-renders per delta; fenced code highlighted with syntect;
  tables, lists, quotes, headings, links; wrapping to the terminal width; untrusted text rendered
  literally (control and bidirectional-override characters replaced by visible stand-ins).
- **Done when:** snapshot tests for each construct; a benchmark streams a 64 KB reply in 1,000 deltas
  with no frame over 16 ms on the reference machine.

### P1-08 — The composer
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-06.
- **Read first:** Codex `codex-rs/tui/src/bottom_pane/textarea.rs` and `paste_burst.rs`;
  [02-current-state.md](02-current-state.md) §What to carry (terminal lessons); the current
  `docs/configuration.md`.
- **Owns:** `crates/talaria-tui/src/composer/`, `crates/talaria-tui/src/keys.rs`,
  `crates/talaria-app/src/config.rs`, tests.
- **Provides:** multi-line editing; Enter submits, Shift+Enter (and Alt+Enter) inserts a newline;
  history with Up and Down; bracketed paste (and paste-burst detection where unavailable); Esc
  interrupts a running turn; Ctrl+C clears a draft, then interrupts, then (on an empty idle composer)
  asks to exit; every binding configurable in `~/.talaria/config.toml`; no default binding on F1, F2,
  Ctrl+B or Ctrl+S. `config.rs` loads `~/.talaria/config.toml`, keeping the `[status_line]` table of
  `docs/formats/status-line.md` and the `[keys]` table defined here; unknown keys warn once; a parse
  error starts with defaults and one plain-language notice.
- **Done when:** key-sequence tests for each behaviour; a paste of 10,000 characters arrives as one
  event.

### P1-09 — Questions from the agent
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-05, P1-08.
- **Read first:** in Hermes, at the pinned commit: `gateway/session_pending_controls.py`,
  `gateway/session_server_requests.py`, `gateway/session_clarify_batch.py` and
  `apps/shared/src/gateway-events.ts`; `x-server-requests` in the pinned contract.
- **Owns:** `crates/talaria-tui/src/prompts/`, `crates/talaria-app/src/prompts.rs`, tests.
- **Provides:**
  - Approval cards in the live region (command, reason, numbered choices once / session / always /
    deny as the event offers, Enter and Esc). Every answer is keyed by (`session_id`, `prompt_id`,
    `execution_generation`) as carried on the request event, and the response echoes all three. If the
    operator types a note, Talaria sends `approval.respond` first and then submits the note as an
    ordinary queued `prompt.submit` to the same session; Hermes has no note field.
  - Clarification, single: a `clarify.request` event answered with `clarify.respond {session_id,
    execution_generation, prompt_id, answer}`. Clarification, batch (one to five questions,
    multi-select): only when Talaria sets `clarify_batch: true` on `prompt.submit`; it arrives as the
    `clarify` server request, each answer is locked with `clarify.lock`, and the batch is settled by
    the response frame. Phase 1 ships the single form; the batch form is optional.
  - Masked input for `sudo` and `secret` server requests. A prompt that settled elsewhere disappears.
  - Every other request in `x-server-requests` is answered at once, never left to time out. The
    `vault.code` and `vault.unlock_prompt` requests get the masked-input card. These get a JSON-RPC
    error that says this client does not support the request, with one notice per connection:
    `vault.save_login`, `terminal.read`, `setup_beat`, `theme.pick`, `theme.apply`, `preview.act`,
    `preview.read`, `window.read`, `tour` and `display.install.sudo`. While the batch form is not built,
    a `clarify` server request gets the same error.
- **Done when:** replaying the approval and clarification fixtures shows the card and sends the right
  `approval.respond` / `clarify.respond` (asserted against the recorded outbound frame); a mock-server
  test sends each of the 16 request kinds, every one of the twelve not shown as a card or masked
  prompt gets a reply within 100 ms, and every card or masked prompt appears within 100 ms. The live
  approval test is part of P1-12's live check.

### P1-10 — Status line
- **Tier:** Haiku 5.5 xhigh (the contract exists; tests prove it).
- **Depends on:** P1-06.
- **Read first:** `python-final:docs/formats/status-line.md`.
- **Owns:** `crates/talaria-tui/src/status.rs`, tests.
- **Provides:** one line with model and effort, working folder and git branch, context percentage,
  session cost, needs-you count and working count, dropping segments by priority as width shrinks; an
  optional external command fed the documented JSON on standard input, with an allow-listed
  environment, refreshed only when its inputs change or on its interval.
- **Done when:** snapshot tests at 60, 100 and 160 columns; the external-command contract tests pass.

### P1-11 — Performance harness and budgets
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-07, P1-08.
- **Read first:** [research/u02-talaria-performance.md](research/u02-talaria-performance.md) (method);
  [05-architecture.md](05-architecture.md) §Performance budgets.
- **Owns:** `tools/bench/`, a CI job `bench`, and a `talaria-cli` bench entry point
  (`talaria --bench-replay <fixture>`) that P1-11 adds as a thin wiring of `ReplaySource` and
  `talaria-tui`; P1-12 keeps it.
- **Provides:** a pseudo-terminal harness at 160×48 that answers terminal capability queries and, driving
  `talaria --bench-replay <fixture>`, measures launch to first frame, keystroke-to-frame latency while a fixture streams at 1× and 10×, CPU,
  bytes written per frame, idle repaints over 30 s, and resident memory; compares each to its budget
  and fails CI when one is exceeded.
- **Done when:** the job runs on every pull request (Linux) and on demand (macOS) and prints a table.

### P1-12 — The first usable binary
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-01 to P1-11.
- **Owns:** `crates/talaria-app/src/{connection.rs,sessions.rs}`, `crates/talaria-cli/`.
- **Provides:** `talaria` (a new session in the current folder on the profile's default model),
  `talaria -c` (continue the most recent session in this folder), `talaria --resume [id]`,
  `--model`, `--provider`, `--effort`, `--profile`; `session.create` with `cwd`; `/quit` detaches and
  leaves the session running; a plain-language error with the next command to run when no gateway is
  reachable or the profile is not served by the gateway.
- **Done when:** the bench budgets pass; a 30-minute live session in Herdr on a free model covers ask,
  edit with an approval, interrupt, quit, and resume. The edit with an approval includes the live
  approval test from P1-09: answering an approval for `rm -rf .pytest_cache` in a throwaway project.
  It needs the precondition from P1-04 (a throwaway profile `fixture` with `approvals.mode: manual`,
  set up by the operator); the test skips with a plain message that states the precondition when it
  is absent.

### P1-13 — Live smoke test in Herdr
- **Tier:** Sonnet 5.5 high.
- **Depends on:** P1-12.
- **Owns:** `tools/smoke/`.
- **Provides:** a script that opens a Herdr tab, starts `talaria` in one pane against the running
  gateway on a cheap model (default `minimax-m3.1-flash` on `ollama-cloud`, overridable), types a
  fixed set of prompts with `herdr pane run`, reads the pane with `herdr pane read`, and checks the
  result: a plain answer, an edit that needs an approval, an interrupt, quit and resume. The script
  must close its tab when it finishes, whether it passed or failed. The approval step needs the
  precondition from P1-04; the script skips that step with a plain message when it is absent.
- **Done when:** it runs unattended in under five minutes and fails with the captured pane text when
  any step fails.

### Phase 1 operator check
Use the build for a day of real coding inside Herdr. It passes when: approvals and clarifications
work; streaming feels instant while typing; the terminal's own search and copy work on the transcript;
quitting and resuming loses nothing; nothing on screen is stale or garbled.

---

## Phase 2 — Coding parity

**Goal:** every row marked phase 2 in the parity table ([04-prior-art.md](04-prior-art.md)
§Claude Code parity checklist). Each unit below names the Hermes calls; acceptance is a fixture replay
test plus a live check. Before using any legacy method name on the gateway socket, check it against
`NativeMethod` (P0-03).

| Unit | Goal | Hermes calls | Tier |
|---|---|---|---|
| P2-01 Tool cells and diffs | One line per tool call (name, argument summary, outcome, duration), with expandable output; diffs rendered with line numbers and colour | `tool.start`, `tool.generating`, `tool.complete`, `tool.progress` when present | Sonnet high |
| P2-02 Transcript pager | Full-screen pager over the whole session with search, tool output expanded, jump to bottom | `session.resume` snapshot for history beyond what was streamed | Sonnet high |
| P2-03 Slash palette | Commands from the backend catalogue with descriptions and provenance; local commands clearly marked; refusals shown in plain words | `commands.catalog`, `complete.slash`, `slash.exec`, `command.dispatch`, `skills.changed` | Sonnet high |
| P2-04 Mentions and attachments | `@` path completion; file and image attachments; clipboard image paste; large-paste collapse | `complete.path`, `file.attach`, `image.attach_bytes`, `clipboard.paste`, `paste.collapse` | Sonnet high |
| P2-05 Shell and side questions | `!command` and `/btw` | `shell.exec`, `prompt.btw`, `btw.complete` | Haiku xhigh |
| P2-06 Model picker | Providers with authentication state, favourites, subscription usage, pricing; model; effort; "this session" by default, "default for new sessions" as an explicit choice | `model.options`, `session.mutate` (model, reasoning) | Sonnet high |
| P2-07 Session picker | Fast list across profiles with title, age, model and cost; rename, branch, export | `session.list`, `session.resume`, `session.mutate`, `session.export` (v0.22.3 or later; check it is in the pinned contract) | Sonnet high |
| P2-08 Queue and steer | "Queued (n)" indicator; list, edit, reorder and remove queued prompts; send-now; steer the running turn | `prompt.submit`, `prompt.replace_queued`, `prompt.cancel`, `prompt.steer_queued`, `session.steer` | Sonnet high |
| P2-09 Rewind | Checkpoint list, diff, restore; rewind the conversation; `/diff` for the session | `rollback.list/diff/restore`, `rollback.session_diff`, `session.mutate` rewind | Sonnet high |
| P2-10 Context, compaction, cost | `/context` breakdown, compaction, `/usage` | `session.context_breakdown` (owner-served), `session.mutate {operation: "compress"}`, the `session.usage` event and `session.stats`, `usage.bars` | Haiku xhigh |
| P2-11 Task checklist | Show the agent's todo list | `todo.updated` | Haiku xhigh |
| P2-12 Themes | Apply Hermes skins; light and dark detection; reduced motion | `gateway.ready` skin, `skin.changed` | Haiku xhigh |
| P2-13 Comforts | External editor (Ctrl+G), history search (Ctrl+R), suspend (Ctrl+Z), bell and OSC 9 desktop notification (a terminal escape sequence that asks the terminal to show a system notification) when done or waiting, Herdr pane state reporting | — | Sonnet high |
| P2-14 Headless mode | `talaria -p "prompt"` with text or JSON output and exit codes | `session.create`, `prompt.submit`, `prompt.receipt` | Sonnet high |
| P2-15 Hermes commands | Clean display for Hermes's own `/plan`, `/goal`, `/loop`, `/yolo`, `/fast` and other pass-through commands | `slash.exec`, `session.control.read`, `session.control.update` | Haiku xhigh |
| P2-16 Permission mode and folder | Show the approval mode (manual, smart, off) in the status line; cycle it with a configurable key (Shift+Tab by default, as in Claude Code), and say whether the change applies to this session or to the profile; `/cd <path>` changes the session's working folder | `config.set` (`approvals.mode`), `session.mutate {operation: "workspace"}` (what Desktop sends; the legacy `session.cwd.set` is refused on the gateway socket) | Sonnet high |

**Phase 2 operator check ("switch day"):** the operator uses Talaria instead of Claude Code for a
week of Hermes-backed coding. Every phase 1 and phase 2 parity row works; anything that does not is
either fixed or explicitly accepted as a gap.

---

## Phase 3 — The fleet: many agents in the background

| Unit | Goal | Hermes calls | Tier |
|---|---|---|---|
| P3-01 Fleet registry | Rows for every session across served profiles (host-scope ticket); states working / needs you / idle / done / failed; one needs-you queue with stable item identity | `session.list`, `sessions.changed`, per-session events for subscribed sessions | Opus medium |
| P3-02 Fleet view | Overlay grouped by state; peek panel with the latest output or the open question; reply (queued, not interrupting); stop; open as foreground; filter | `prompt.submit`, `session.interrupt`, `approval.respond` | Sonnet high |
| P3-03 Background launch | `talaria --bg "prompt"`, `n` in the fleet view, and `w` = new session in a git worktree branched from freshly fetched `origin` | `session.create` (cwd), `prompt.submit` | Sonnet high |
| P3-04 Subagents | Tree under each session with tail, detail, steer and stop; pause new spawns | `subagent.*` events, `subagent.list/tail/steer/interrupt`, `delegation.status/pause` (Desktop never calls steer or interrupt: test them live first) | Sonnet high |
| P3-05 Background work | Background processes with live output and kill; `/bg` results | `process.list/kill/stop`, `agent.terminal.output`, `terminal.close`, `prompt.background`, `background.complete` | Sonnet high |
| P3-06 Attention | Bell and desktop notification on needs-you, done and failed; status-line counters; answer approvals from the peek panel | — | Haiku xhigh |
| P3-07 Review | Changed files for a session's worktree; stage, commit, push, open a draft pull request; pull-request status label on fleet rows | Local `git` and `gh` | Sonnet high |
| P3-08 Attach anywhere | `talaria attach <session>`; inside Herdr or tmux, "open in new pane" | — | Haiku xhigh |

**Phase 3 operator check:** three sessions on three different providers working on three tasks while
the operator codes in a fourth; an approval answered from the peek panel; a worktree session ending in
a draft pull request.

---

## Phase 4 — Configure Hermes from Talaria

| Unit | Goal | Hermes calls | Tier |
|---|---|---|---|
| P4-01 Settings framework | Forms generated from the config schema; read, edit, validate, diff before apply; "applies now" or "needs restart"; secrets masked with a one-shot reveal | `GET /api/config/schema`, `GET /api/config/defaults`, `config.get`, `config.set`, `PUT /api/config` (deep-merge: send only changed keys); REST authenticated with a `native-http` ticket | Opus medium |
| P4-02 Providers, keys, sign-in | Add a key, sign in with OAuth, add a custom endpoint, disconnect | `model.options`, `model.save_key`, `model.disconnect`, `/api/env`, `/api/providers/oauth/*`, `/api/providers/custom-endpoints` | Sonnet high |
| P4-03 Model roles | Main model, auxiliary models, Mixture of Agents presets, fallbacks, the delegation model | `/api/model/*`, `config.set` | Sonnet high |
| P4-04 MCP servers | Model Context Protocol (MCP) servers: catalogue, add, test, enable, OAuth and device-code sign-in | `mcp.catalog`, `mcp.servers.*`, `reload.mcp` | Sonnet high |
| P4-05 Skills, toolsets, plugins | Browse, enable, install | `skills.manage`, `tools.list`, `tools.configure`, `plugins.manage`, `/api/skills/*`, `/api/tools/toolsets/*` | Sonnet high |
| P4-06 Profiles | List with served-by-gateway state and the reason when not served (from `identify.parked_profiles`); create; describe; configure | `profiles.*`, control-socket `identify` | Sonnet high |
| P4-07 Memory | Provider settings, reset, curator | `/api/memory/*`, `/api/curator/*` | Haiku xhigh |
| P4-08 Cron | List, add, pause, resume, trigger, history | `cron.manage`, `cron.changed` | Sonnet high |
| P4-09 Conversational configuration | `/configure <request>`: a short-lived Hermes session with a configuration skill proposes changes; Talaria shows the key diff; the operator approves; Talaria applies them | `session.create`, `config.set`, the typed calls above | Opus medium |
| P4-10 Doctor | Gateway health, served profiles, pinned versus running Hermes, capability strings, each with a fix; restart the gateway after a confirmation (`POST /api/gateway/restart`) | control-socket `identify`, `runtime.describe` | Haiku xhigh |

**Phase 4 operator check:** add a provider key, add an MCP server, change a profile's model and create
a cron job without leaving Talaria or opening a YAML file.

---

## Phase 5 — Workflows

Design in [06-workflows.md](06-workflows.md).

| Unit | Goal | Tier |
|---|---|---|
| P5-01 Script engine | Embed QuickJS (`rquickjs` with futures); expose `agent`, `parallel`, `pipeline`, `phase`, `log`, `args`, `budget`, `workflow`; make `Date.now`, `Math.random` and argument-less `new Date` throw; no filesystem or network; a mock backend for tests | Opus medium |
| P5-02 Journal and resume | Append-only journal; call keys; idempotent `request_id` and `submission_id`; re-attach through `prompt.receipt`; unknown outcomes surfaced, never re-run | Opus medium |
| P5-03 Hermes binding | `agent()` → hidden session with explicit model, provider, effort, toolsets, skills and folder → durable prompt → receipt → schema validation → one correction turn; the submit never sets `unattended` (in Hermes it auto-approves every dangerous command in that turn), and a test asserts the outbound frame has no `unattended` key | Sonnet high |
| P5-04 Staffing and scheduling | Staffing table; roster from `model.options` at run start; per-provider scheduling from limits and usage; global cap 10 halving on rate limits; money and token budgets | Sonnet high |
| P5-05 Progress view | Tree from the journal plus live events: run, phase, agent, subagents; open, steer, stop, restart, pause | Sonnet high |
| P5-06 Runner lifecycle | `talaria workflow run [--detach]`, `resume`, `list`, `stop`; a runs directory the terminal UI discovers | Sonnet high |
| P5-07 Worktree isolation | `isolation: 'worktree'` per agent | Haiku xhigh |
| P5-08 Built-in workflows | `review-branch`, `research`, `fix-failing-tests`, `ultracode`, each with a fixture-backed test | Sonnet high |
| P5-09 Ultracode mode | The `talaria-workflows` Hermes skill; `talaria workflow roster --json`; an ultracode toggle per session | Opus medium |

**Phase 5 operator check:** `review-branch` with three reviewers on three providers; kill Talaria
and the runner mid-run; resume; finished agents are not re-run; the cost shown matches the backend's
usage.

---

## Phase 6 — Later, in the order the operator chooses

- Remote gateways: the gated dashboard with the native-app PKCE sign-in (an authorization code exchanged with a one-time proof key), behind a tunnel (moves to
  phase 3 if the operator needs it daily).
- An Agent Client Protocol adapter so non-Hermes agents appear in the fleet view.
- A kanban view and "compile this workflow to kanban" for unattended static graphs.
- Vim editing mode; streaming JSON input and output for scripts; voice.
- Packaging: GitHub releases built by CI per platform, a Homebrew tap.
- Upstream contributions as they become relevant (list in
  [08-risks-and-open-questions.md](08-risks-and-open-questions.md)).
