# Unit specifications for Talaria v0.7.0

Status: `proposed`. Each file here is the reference specification for one GitHub issue in the
Talaria v0.7.0 issue set (the Objective `Talaria v0.7.0` on the Operations board). The issue is the
contract: what to build, the requirements, the tests to write first, the user acceptance scenarios,
the acceptance criteria and the stop conditions. The specification holds the depth: every Hermes
fact with its evidence at Hermes v0.22.4, request and result shapes, mock-server rules, edge cases
and detailed test notes. If an issue and its specification disagree, the issue wins and the
disagreement is reported on the issue.

The plan behind the set is the [roadmap](../07-roadmap-and-work-units.md). Capability issues (one
per phase slice) hold the run contract for their children and have no specification of their own;
neither do P6-08 (exploration).

## PH0: Talaria v0.7.0 phase 0: decide the direction and reset the repository for the Rust rewrite ([#191](https://github.com/infiquetra/talaria/issues/191))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P0-01: Record the operator's decisions on the rewrite: accept or amend the draft architecture decisions (unit P0-01) | operator decisions | [P0-01.md](P0-01.md) | [#194](https://github.com/infiquetra/talaria/issues/194) |
| P0-02: Tag the Python era and reset the repository into a Rust workspace (unit P0-02) | building | [P0-02.md](P0-02.md) | [#195](https://github.com/infiquetra/talaria/issues/195) |
| P0-03: Vendor the pinned Hermes contract and generate typed protocol bindings (unit P0-03) | building | [P0-03.md](P0-03.md) | [#196](https://github.com/infiquetra/talaria/issues/196) |
| P0-04: Write the patterns guide that every later unit copies (unit P0-04) | building | [P0-04.md](P0-04.md) | [#197](https://github.com/infiquetra/talaria/issues/197) |

## PH1A: Talaria v0.7.0 phase 1a: attach to the Hermes gateway and model a session ([#192](https://github.com/infiquetra/talaria/issues/192))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P1-01: Find the Hermes gateway, mint a ticket and attach (unit P1-01) | building | [P1-01.md](P1-01.md) | [#198](https://github.com/infiquetra/talaria/issues/198) |
| P1-02: Typed JSON-RPC channel with server requests and safe timeouts (unit P1-02) | building | [P1-02.md](P1-02.md) | [#199](https://github.com/infiquetra/talaria/issues/199) |
| P1-03: Liveness, reconnect and replay without losing or repeating events (unit P1-03) | building | [P1-03.md](P1-03.md) | [#200](https://github.com/infiquetra/talaria/issues/200) |
| P1-04: Redacting frame recorder, replay source and live fixtures (unit P1-04) | building | [P1-04.md](P1-04.md) | [#201](https://github.com/infiquetra/talaria/issues/201) |
| P1-05: Session reducer: one pure state machine for a session's transcript (unit P1-05) | building | [P1-05.md](P1-05.md) | [#202](https://github.com/infiquetra/talaria/issues/202) |

## PH1B: Talaria v0.7.0 phase 1b: the inline screen, the composer and the first usable binary ([#193](https://github.com/infiquetra/talaria/issues/193))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P1-06: Inline renderer: finished output in the terminal's own scrollback (unit P1-06) | building | [P1-06.md](P1-06.md) | [#203](https://github.com/infiquetra/talaria/issues/203) |
| P1-07: Streaming Markdown and highlighted code without dropped frames (unit P1-07) | building | [P1-07.md](P1-07.md) | [#204](https://github.com/infiquetra/talaria/issues/204) |
| P1-08: Composer: multi-line input, history, paste and configurable keys (unit P1-08) | building | [P1-08.md](P1-08.md) | [#205](https://github.com/infiquetra/talaria/issues/205) |
| P1-09: Answer the agent's approvals, questions and secret prompts (unit P1-09) | building | [P1-09.md](P1-09.md) | [#206](https://github.com/infiquetra/talaria/issues/206) |
| P1-10: Status line with prioritised segments and the external-command contract (unit P1-10) | building | [P1-10.md](P1-10.md) | [#207](https://github.com/infiquetra/talaria/issues/207) |
| P1-11: Performance harness that fails CI when a budget is exceeded (unit P1-11) | building | [P1-11.md](P1-11.md) | [#208](https://github.com/infiquetra/talaria/issues/208) |
| P1-12: The first usable talaria binary: new, continue, resume and detach (unit P1-12) | building | [P1-12.md](P1-12.md) | [#209](https://github.com/infiquetra/talaria/issues/209) |
| P1-13: Unattended live smoke test in Herdr (unit P1-13) | building | [P1-13.md](P1-13.md) | [#210](https://github.com/infiquetra/talaria/issues/210) |
| PH1-ACC: Phase 1 acceptance: the smoke suite on the candidate, then a day of real coding by the operator | acceptance | [PH1-ACC.md](PH1-ACC.md) | [#211](https://github.com/infiquetra/talaria/issues/211) |

## PH2A: Talaria v0.7.0 phase 2a: read and write code the way Claude Code does ([#213](https://github.com/infiquetra/talaria/issues/213))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P2-01: Tool-call cells and rendered diffs (unit P2-01) | building | [P2-01.md](P2-01.md) | [#221](https://github.com/infiquetra/talaria/issues/221) |
| P2-02: Transcript pager with search over the whole session (unit P2-02) | building | [P2-02.md](P2-02.md) | [#222](https://github.com/infiquetra/talaria/issues/222) |
| P2-03: Slash-command palette from the backend catalogue (unit P2-03) | building | [P2-03.md](P2-03.md) | [#223](https://github.com/infiquetra/talaria/issues/223) |
| P2-04: @ file mentions, attachments, image paste and large-paste collapse (unit P2-04) | building | [P2-04.md](P2-04.md) | [#224](https://github.com/infiquetra/talaria/issues/224) |
| P2-05: ! shell commands and /btw side questions (unit P2-05) | building | [P2-05.md](P2-05.md) | [#225](https://github.com/infiquetra/talaria/issues/225) |
| P2-11: Show the agent's task checklist (unit P2-11) | building | [P2-11.md](P2-11.md) | [#226](https://github.com/infiquetra/talaria/issues/226) |
| P2-12: Apply Hermes skins with light, dark and reduced-motion handling (unit P2-12) | building | [P2-12.md](P2-12.md) | [#227](https://github.com/infiquetra/talaria/issues/227) |
| P2-13: Comforts: external editor, history search, suspend, notifications and Herdr pane state (unit P2-13) | building | [P2-13.md](P2-13.md) | [#228](https://github.com/infiquetra/talaria/issues/228) |
| PH2A-ACC: Phase 2a acceptance: extend the smoke suite to every phase 2a feature and run it on the candidate | acceptance | [PH2A-ACC.md](PH2A-ACC.md) | [#229](https://github.com/infiquetra/talaria/issues/229) |

## PH2B: Talaria v0.7.0 phase 2b: control sessions, models, queues and modes ([#214](https://github.com/infiquetra/talaria/issues/214))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P2-06: Model, provider and effort picker that changes this session by default (unit P2-06) | building | [P2-06.md](P2-06.md) | [#230](https://github.com/infiquetra/talaria/issues/230) |
| P2-07: Session picker across profiles with rename, branch and export (unit P2-07) | building | [P2-07.md](P2-07.md) | [#231](https://github.com/infiquetra/talaria/issues/231) |
| P2-08: Queue and steer: see, edit, reorder and send queued prompts (unit P2-08) | building | [P2-08.md](P2-08.md) | [#232](https://github.com/infiquetra/talaria/issues/232) |
| P2-09: Rewind: checkpoints, diffs and restoring the conversation (unit P2-09) | building | [P2-09.md](P2-09.md) | [#233](https://github.com/infiquetra/talaria/issues/233) |
| P2-10: Context breakdown, compaction and usage (unit P2-10) | building | [P2-10.md](P2-10.md) | [#234](https://github.com/infiquetra/talaria/issues/234) |
| P2-14: Headless mode: talaria -p with text or JSON output and exit codes (unit P2-14) | building | [P2-14.md](P2-14.md) | [#235](https://github.com/infiquetra/talaria/issues/235) |
| P2-15: Clean display for Hermes's own /plan, /goal, /loop, /yolo and /fast (unit P2-15) | building | [P2-15.md](P2-15.md) | [#236](https://github.com/infiquetra/talaria/issues/236) |
| P2-16: Permission mode in the status line and /cd for the session's folder (unit P2-16) | building | [P2-16.md](P2-16.md) | [#237](https://github.com/infiquetra/talaria/issues/237) |
| PH2-ACC: Phase 2 acceptance: the smoke suite on the candidate, then a switch week from Claude Code to Talaria | acceptance | [PH2-ACC.md](PH2-ACC.md) | [#238](https://github.com/infiquetra/talaria/issues/238) |

## PH3: Talaria v0.7.0 phase 3: a fleet of agents working in the background ([#215](https://github.com/infiquetra/talaria/issues/215))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P3-01: Fleet registry: every session across served profiles and one needs-you queue (unit P3-01) | building | [P3-01.md](P3-01.md) | [#239](https://github.com/infiquetra/talaria/issues/239) |
| P3-02: Fleet view: peek, reply, stop and open any session (unit P3-02) | building | [P3-02.md](P3-02.md) | [#240](https://github.com/infiquetra/talaria/issues/240) |
| P3-03: Background launch, including a new session in a fresh git worktree (unit P3-03) | building | [P3-03.md](P3-03.md) | [#241](https://github.com/infiquetra/talaria/issues/241) |
| P3-04: Subagent tree with tail, steer and stop (unit P3-04) | building | [P3-04.md](P3-04.md) | [#242](https://github.com/infiquetra/talaria/issues/242) |
| P3-05: Background processes and /bg results (unit P3-05) | building | [P3-05.md](P3-05.md) | [#243](https://github.com/infiquetra/talaria/issues/243) |
| P3-06: Attention: bell, desktop notification and counters for needs-you, done and failed (unit P3-06) | building | [P3-06.md](P3-06.md) | [#244](https://github.com/infiquetra/talaria/issues/244) |
| P3-07: Review a session's worktree: stage, commit, push and open a draft pull request (unit P3-07) | building | [P3-07.md](P3-07.md) | [#245](https://github.com/infiquetra/talaria/issues/245) |
| P3-08: talaria attach, and open a session in a new multiplexer pane (unit P3-08) | building | [P3-08.md](P3-08.md) | [#246](https://github.com/infiquetra/talaria/issues/246) |
| P3-09: Resident agents: always-available named agents pinned in the fleet view (unit P3-09, only if the operator adopts it) | building | [P3-09.md](P3-09.md) | [#247](https://github.com/infiquetra/talaria/issues/247) |
| PH3-ACC: Phase 3 acceptance: three agents on three providers while the operator codes in a fourth | acceptance | [PH3-ACC.md](PH3-ACC.md) | [#248](https://github.com/infiquetra/talaria/issues/248) |

## PH4A: Talaria v0.7.0 phase 4a: settings, providers, model roles, profiles and a doctor ([#216](https://github.com/infiquetra/talaria/issues/216))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P4-00: Find out which configuration calls work on the gateway socket and which need the dashboard API (unit P4-00) | exploration | [P4-00.md](P4-00.md) | [#249](https://github.com/infiquetra/talaria/issues/249) |
| P4-01: Schema-driven settings: forms, validation and a diff before apply (unit P4-01) | building | [P4-01.md](P4-01.md) | [#250](https://github.com/infiquetra/talaria/issues/250) |
| P4-02: Providers, keys and sign-in (unit P4-02) | building | [P4-02.md](P4-02.md) | [#251](https://github.com/infiquetra/talaria/issues/251) |
| P4-03: Model roles: main, auxiliary, Mixture of Agents, fallbacks and delegation (unit P4-03) | building | [P4-03.md](P4-03.md) | [#252](https://github.com/infiquetra/talaria/issues/252) |
| P4-06: Profiles, including why a profile is not served (unit P4-06) | building | [P4-06.md](P4-06.md) | [#253](https://github.com/infiquetra/talaria/issues/253) |
| P4-10: Doctor: gateway health and versions, each problem with a fix (unit P4-10) | building | [P4-10.md](P4-10.md) | [#254](https://github.com/infiquetra/talaria/issues/254) |
| PH4A-ACC: Phase 4a acceptance: extend the smoke suite to settings, providers, model roles, profiles and the doctor, and run it | acceptance | [PH4A-ACC.md](PH4A-ACC.md) | [#255](https://github.com/infiquetra/talaria/issues/255) |

## PH4B: Talaria v0.7.0 phase 4b: MCP servers, skills, plugins, memory, cron, connectors and /configure ([#217](https://github.com/infiquetra/talaria/issues/217))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P4-04: MCP servers: catalogue, add, test, enable and sign in (unit P4-04) | building | [P4-04.md](P4-04.md) | [#256](https://github.com/infiquetra/talaria/issues/256) |
| P4-05: Skills, toolsets and plugins: browse, enable and install (unit P4-05) | building | [P4-05.md](P4-05.md) | [#257](https://github.com/infiquetra/talaria/issues/257) |
| P4-07: Memory provider settings, reset and curator (unit P4-07) | building | [P4-07.md](P4-07.md) | [#258](https://github.com/infiquetra/talaria/issues/258) |
| P4-08: Cron jobs: list, add, pause, resume, trigger and history (unit P4-08) | building | [P4-08.md](P4-08.md) | [#259](https://github.com/infiquetra/talaria/issues/259) |
| P4-11: Connectors and messaging platforms (unit P4-11) | building | [P4-11.md](P4-11.md) | [#260](https://github.com/infiquetra/talaria/issues/260) |
| P4-09: Conversational configuration with /configure (unit P4-09) | building | [P4-09.md](P4-09.md) | [#261](https://github.com/infiquetra/talaria/issues/261) |
| PH4-ACC: Phase 4 acceptance: configure a provider, an MCP server, a profile's model and a cron job without a YAML file | acceptance | [PH4-ACC.md](PH4-ACC.md) | [#262](https://github.com/infiquetra/talaria/issues/262) |

## PH5: Talaria v0.7.0 phase 5: workflows that fan work out across every configured model ([#218](https://github.com/infiquetra/talaria/issues/218))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P5-00: Measure the gateway with ten or more concurrent sessions before workflows depend on it (unit P5-00) | exploration | [P5-00.md](P5-00.md) | [#263](https://github.com/infiquetra/talaria/issues/263) |
| P5-01: Workflow script engine in embedded QuickJS (unit P5-01) | building | [P5-01.md](P5-01.md) | [#264](https://github.com/infiquetra/talaria/issues/264) |
| P5-02: Workflow journal and resume that never re-runs a finished agent (unit P5-02) | building | [P5-02.md](P5-02.md) | [#265](https://github.com/infiquetra/talaria/issues/265) |
| P5-03: Hermes binding for agent(): hidden sessions, explicit model and schema-checked results (unit P5-03) | building | [P5-03.md](P5-03.md) | [#266](https://github.com/infiquetra/talaria/issues/266) |
| P5-04: Staffing table, roster and per-provider scheduling within budgets (unit P5-04) | building | [P5-04.md](P5-04.md) | [#267](https://github.com/infiquetra/talaria/issues/267) |
| P5-05: Workflow progress view (unit P5-05) | building | [P5-05.md](P5-05.md) | [#268](https://github.com/infiquetra/talaria/issues/268) |
| P5-06: Workflow runner lifecycle: run, detach, resume, list and stop (unit P5-06) | building | [P5-06.md](P5-06.md) | [#269](https://github.com/infiquetra/talaria/issues/269) |
| P5-07: Worktree isolation for workflow agents (unit P5-07) | building | [P5-07.md](P5-07.md) | [#270](https://github.com/infiquetra/talaria/issues/270) |
| P5-08: Built-in workflows: review-branch, research, fix-failing-tests and ultracode (unit P5-08) | building | [P5-08.md](P5-08.md) | [#271](https://github.com/infiquetra/talaria/issues/271) |
| P5-09: Ultracode mode and the talaria-workflows Hermes skill (unit P5-09) | building | [P5-09.md](P5-09.md) | [#272](https://github.com/infiquetra/talaria/issues/272) |
| PH5-ACC: Phase 5 acceptance: a three-provider review workflow survives a kill and resumes without re-running finished agents | acceptance | [PH5-ACC.md](PH5-ACC.md) | [#273](https://github.com/infiquetra/talaria/issues/273) |

## PH6: Talaria v0.7.0 phase 6: later work, in the order the operator chooses ([#219](https://github.com/infiquetra/talaria/issues/219))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| P6-01: Remote gateways: native-app sign-in with a one-time proof key, behind a tunnel (unit P6-01) | building | [P6-01.md](P6-01.md) | [#274](https://github.com/infiquetra/talaria/issues/274) |
| P6-02: Agent Client Protocol adapter so non-Hermes agents appear in the fleet view (unit P6-02) | building | [P6-02.md](P6-02.md) | [#275](https://github.com/infiquetra/talaria/issues/275) |
| P6-03: Kanban view and compiling a workflow to kanban (unit P6-03) | building | [P6-03.md](P6-03.md) | [#276](https://github.com/infiquetra/talaria/issues/276) |
| P6-04: Vim editing mode in the composer (unit P6-04) | building | [P6-04.md](P6-04.md) | [#277](https://github.com/infiquetra/talaria/issues/277) |
| P6-05: Streaming JSON input and output for scripts (unit P6-05) | building | [P6-05.md](P6-05.md) | [#278](https://github.com/infiquetra/talaria/issues/278) |
| P6-06: Voice input and output (unit P6-06) | building | [P6-06.md](P6-06.md) | [#279](https://github.com/infiquetra/talaria/issues/279) |
| P6-07: Packaging: GitHub release binaries per platform and a Homebrew tap (unit P6-07) | building | [P6-07.md](P6-07.md) | [#280](https://github.com/infiquetra/talaria/issues/280) |
| P6-08: Propose the upstream Hermes contributions the client needs (unit P6-08) | exploration | none | [#281](https://github.com/infiquetra/talaria/issues/281) |

## REL: Talaria v0.7.0 release: integrated regression, documentation, migration notes and tag ([#220](https://github.com/infiquetra/talaria/issues/220))

| Unit | Kind | Specification | Issue |
|---|---|---|---|
| REL-01: Release candidate: integrated regression, replay suites and performance budgets on one commit (unit REL-01) | building | [REL-01.md](REL-01.md) | [#282](https://github.com/infiquetra/talaria/issues/282) |
| REL-02: User documentation: guide, keybinding and configuration reference, and migration from 0.6.5 (unit REL-02) | building | [REL-02.md](REL-02.md) | [#283](https://github.com/infiquetra/talaria/issues/283) |
| REL-03: Version, changelog, tag and release notes for v0.7.0 (unit REL-03) | building | [REL-03.md](REL-03.md) | [#284](https://github.com/infiquetra/talaria/issues/284) |
