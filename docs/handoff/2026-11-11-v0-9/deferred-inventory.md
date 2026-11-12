# Deferred inventory: the 78 items no release owned

The product plan of 2026-11-08 listed work that was deferred with no release. This is that list, as triaged on 2026-11-11. The last column names the issue in the v0.8.1 to v1.0.0 set that takes an item up, where one does. The issue that closes this inventory gives every row one outcome.

| Id | Item | Area | Kind | Operator asked | Needs a Hermes change | Taken up by |
|---|---|---|---|---|---|---|
| I-003 | Resizing the inline view can leave stale rows or push live rows into scrollback | Workspace and scrollback | defect | no | not stated | FIX-1 |
| I-131 | Transcript and checklist extras left out of v0.7.0: message timestamps and model name, regular-expression and cross-session search, collapsing repeated Model Context Protocol (MCP) tool calls, tool-preview sizes, checklist editing and checklists for background sessions | Workspace and scrollback | feature | no | not stated |  |
| I-011 | Restyle the session picker and command palette on the shared frame | Pickers and themes | feature | no | not stated |  |
| I-013 | Reload theme files automatically when they change | Pickers and themes | feature | no | not stated |  |
| I-014 | Pager, side-cell and checklist theme tokens (#213 ruling C-6) | Pickers and themes | feature | no | not stated |  |
| I-022 | The model picker does not label the saved profile default apart from the session model | Pickers and themes | feature | no | not stated |  |
| I-138 | No command creates or scaffolds a theme file | Pickers and themes | feature | yes | not stated | FIX-9 |
| I-139 | When Hermes's skin-name check is inconclusive, /skin saves the name first and a failed restore can leave an unknown skin saved | Pickers and themes | defect | no | not stated | FIX-4 |
| I-141 | The model picker has word search and grouping but no fuzzy ranking, no featured-model ordering and no aliases | Pickers and themes | feature | no | not stated |  |
| I-024 | Command completion and unread catalogue fields | Composer, completion and attachments | docs or records | no | not stated |  |
| I-025 | A view listing composer keyboard shortcuts | Composer, completion and attachments | feature | no | not stated |  |
| I-027 | Interrupting an Ultracode prompt can overwrite the draft | Composer, completion and attachments | defect | no | not stated | FIX-2 |
| I-028 | PDF attachments are sent to file.attach although the recorded decision is to refuse them | Composer, completion and attachments | defect | no | not stated | FIX-3 |
| I-029 | Hunk-level edit, revert or apply in the review view | Composer, completion and attachments | feature | no | not stated |  |
| I-143 | /cd has no tab completion and no "/cd -", and there is no /add-dir | Composer, completion and attachments | feature | no | not stated |  |
| I-144 | Shell passthrough and side-question limits left out by #225 (! shell commands and /btw side questions): ! output not sent to the agent, no interactive shell, nothing over 30 seconds, unavailable headless | Composer, completion and attachments | feature | no | not stated |  |
| I-036 | Remember across restarts which sessions finished while Talaria was closed | Session lifecycle and remote | feature | no | not stated |  |
| I-149 | Talaria retries Hermes's restart refusal (runtime_draining) automatically only inside the workflow journal; prompts and slash commands show a message and wait | Session lifecycle and remote | feature | no | not stated |  |
| I-150 | No way to end a Hermes session for good (an end_reason) | Session lifecycle and remote | feature | no | not stated |  |
| I-151 | Worktree launch options left out by #241 (background launch in a fresh git worktree): other base branch, pull-request base, submodules, remote-backend isolation | Session lifecycle and remote | feature | no | not stated |  |
| I-152 | A queued item's "waiting 27m" age is Talaria's own observation, with no clock time, no origin label and no memory across restarts | Session lifecycle and remote | feature | no | yes |  |
| I-032 | Keep a partial reply when a credential fails mid-stream | Session lifecycle and remote | defect | no | not stated | FIX-6 |
| I-043 | Client-side voice needs installed helper programs; no platform preset | Voice | feature | no | not stated |  |
| I-044 | Remote audio is opt-in, and one documentation sentence reads stale | Voice | docs or records | no | not stated |  |
| I-045 | The under-100-millisecond stop-to-silence target has no recorded device measurement | Voice | test or release tooling | no | not stated |  |
| I-046 | Gateway audio capture can outlive a client detach | Voice | defect | no | yes |  |
| I-048 | No single written voice safety stance | Voice | decision | no | not stated |  |
| I-041 | Voice settings documented as "Deferred to a later release" | Voice | docs or records | no | not stated |  |
| I-056 | Ruling D-8 (a numbered ruling in issue #215) says five gaps were queued; they are not | Fleet, Herdr and multiplexers | docs or records | no | not stated |  |
| I-058 | Phase 3 non-goals: focus tracking, OSC 777 (a terminal notification escape sequence), sound effects, notification buttons, pull-request merge, close and comment, GitLab, two sessions per process, other multiplexers | Fleet, Herdr and multiplexers | decision | no | not stated |  |
| I-059 | A counter of background alerts skipped by the "watched" rule | Fleet, Herdr and multiplexers | feature | no | not stated |  |
| I-050 | Inline and workspace modes never run in Zellij despite ADR requirements | Fleet, Herdr and multiplexers | docs or records | no | not stated |  |
| I-062 | Live acceptance of runner pause, resume, stop and restart | Workflows and agents | test or release tooling | no | not stated |  |
| I-063 | Kanban adapter partly built; live compile probe outstanding | Workflows and agents | feature | no | not stated |  |
| I-064 | Worktree retention policy undecided; the live proof was recorded as missing | Workflows and agents | decision | no | not stated |  |
| I-065 | A workflow script can catch stack or memory exhaustion | Workflows and agents | feature | no | not stated |  |
| I-157 | A goal that Talaria first sees already finished gets no completion notice | Workflows and agents | defect | yes | not stated | FIX-5 |
| I-158 | Headless stream-json output (talaria -p) omits several record kinds, and four operator rulings from #278 have no recorded answer | Workflows and agents | decision | no | not stated |  |
| I-159 | The built-in review workflow cannot post findings to GitHub or apply fixes | Workflows and agents | feature | no | not stated |  |
| I-160 | Ultracode toggle is not saved across restarts; no session-wide waiver; no /effort ultracode | Workflows and agents | feature | no | not stated |  |
| I-071 | Features blocked on Hermes | Hermes contract and protocol | feature | no | yes |  |
| I-073 | Hermes-side requests drafted but never filed | Hermes contract and protocol | docs or records | no | yes |  |
| I-077 | DECISIONS.md says Hermes ignores omit_messages | Hermes contract and protocol | docs or records | no | no |  |
| I-078 | Temporary client-side workarounds waiting on Hermes | Hermes contract and protocol | docs or records | no | yes |  |
| I-163 | Prompts typed by another viewer of the same session never reach Talaria's stream | Hermes contract and protocol | defect | no | not stated |  |
| I-166 | Managed-key and recalled-plugin refusals are not named | Hermes contract and protocol | defect | no | no |  |
| I-167 | Safety-refused turns have no handling | Hermes contract and protocol | security | no | not stated |  |
| I-082 | Hub search cannot go past 50 results | Hub and skills | feature | no | yes |  |
| I-084 | Hub browse total is capped with no truncation flag | Hub and skills | feature | no | yes |  |
| I-085 | Uninstall a skill from /capabilities | Hub and skills | feature | no | not stated |  |
| I-086 | Toggle individual MCP server tools | Hub and skills | feature | no | yes |  |
| I-087 | Device-code sign-in for MCP servers | Hub and skills | feature | no | yes |  |
| I-088 | Interactive tools.configure form | Hub and skills | feature | no | not stated |  |
| I-091 | Editor for instruction files (AGENTS.md, .hermes.md) | Configuration and status line | feature | no | not stated |  |
| I-092 | Edit an existing cron job in place | Configuration and status line | feature | no | yes |  |
| I-093 | OAuth (sign-in standard) for memory providers | Configuration and status line | feature | no | not stated |  |
| I-096 | Frozen status-line doc names Python-era config paths | Configuration and status line | docs or records | no | not stated |  |
| I-097 | Status-line field for the agent's working folder | Configuration and status line | feature | yes | not stated | FIX-7 |
| I-098 | Restart needed for config changes; no in-app feedback | Configuration and status line | feature | yes | not stated | FIX-8 |
| I-099 | Phase 4 configuration non-goals (raw YAML, key delete, autosave, undo, profile rename and delete, cron cancel, QR, webhooks and others) | Configuration and status line | decision | no | not stated |  |
| I-101 | Status section in --bench-replay | Configuration and status line | test or release tooling | no | not stated |  |
| I-102 | Show which reasoning effort reached the provider | Configuration and status line | feature | no | yes |  |
| I-169 | Status command input lacks usage fields (context window, rate limit, spend, cache hit, latency) | Configuration and status line | feature | yes | not stated | ONE-3 |
| I-104 | Apple notarization of macOS archives | Testing and release infrastructure | test or release tooling | no | not stated |  |
| I-105 | Linux is shipped and merge-blocking in CI but never driven interactively | Testing and release infrastructure | test or release tooling | no | not stated |  |
| I-106 | A session.resume recording after the tool-diff turn | Testing and release infrastructure | test or release tooling | no | not stated |  |
| I-108 | Which CI checks are required on main is not recorded | Testing and release infrastructure | docs or records | no | not stated |  |
| I-109 | Performance claims and v0.8.0 acceptance evidence | Testing and release infrastructure | test or release tooling | no | not stated |  |
| I-110 | Operator visual sign-off on theme screens not recorded | Testing and release infrastructure | docs or records | no | not stated |  |
| I-172 | Deferred repository tooling (Cargo dependabot, branch coverage, review_tools, micro benchmarks, required benchmark check) | Testing and release infrastructure | test or release tooling | no | not stated |  |
| I-115 | Open items in the context library's own queue | Context library and docs | docs or records | no | not stated |  |
| I-119 | DECISIONS gaps and fired revisit triggers | Context library and docs | docs or records | no | not stated |  |
| I-120 | Handoff operator decisions 4 to 8 | Context library and docs | docs or records | no | not stated |  |
| I-121 | Python-era leftovers | Context library and docs | docs or records | no | not stated |  |
| I-173 | Python-era hands-on notes never traced in Rust | Context library and docs | docs or records | no | not stated |  |
| I-125 | Bound transcript growth | Security, architecture and process | feature | no | not stated |  |
| I-130 | Phase 2 and 6 non-goals and stale stub comments | Security, architecture and process | docs or records | no | not stated |  |
| I-175 | Screen-reader accessibility never revisited | Security, architecture and process | feature | no | not stated |  |
