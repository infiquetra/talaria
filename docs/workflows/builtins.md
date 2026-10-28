# Built-in workflows

Talaria ships four ordinary JavaScript workflows. Run one by name, for example
`talaria workflow run review-branch`. On the first named run Talaria installs all
four into the configured workflows folder. Your installed, edited copy wins.
A path containing `/` or `\`, ending in `.js`, or naming an existing file stays
a script path. Invalid and unknown names exit 2.

```sh
talaria workflow builtins list
talaria workflow builtins list --json --workflows-dir ./workflows
talaria workflow builtins show review-branch
talaria workflow builtins install
talaria workflow builtins install --force --json
talaria workflow builtins check review-branch --staffing ./staffing.toml
talaria workflow builtins check ultracode --args '{"designers":2}'
```

`list` reports phases, role defaults, toolsets, worktree use, the default agent
bound and whether each installed copy matches the embedded source. `show` prints
the exact embedded source. `install` keeps modified files unless `--force` is
supplied, then saves their old content as `<name>.js.bak`. It always keeps target
symbolic links and non-file entries. An explicitly linked workflows folder is
supported. Files are installed atomically with mode 0600; a newly created leaf
folder has mode 0700 on Unix. Only `list` and `install` accept `--workflows-dir`.
The default folder is the runner's `<configuration folder>/workflows`.

`check` reads only staffing configuration and prints one entry per possible role
slot, including `review.reviewers[0]` for a child review. It exits 0 if all tiers
exist, otherwise 2; it does not check gateway availability, credentials or model
availability. All builtins subcommands work without contacting Hermes. The default
staffing file is `staffing.toml` under `TALARIA_CONFIG_DIR`, or under `~/.talaria`.

Every workflow validates arguments before starting agents. Unknown keys at every
level are errors. Each role can be overridden through `args.tiers`; list roles
accept one to five tier names and cycle through them. Tier names match
`^[a-z][a-z0-9_-]{0,31}$`. Roles select tiers, never fixed models or providers.
Results have a stable set of keys even for empty, failed-agent or blocked paths.
Agent and repository material is capped and fenced as evidence, not instructions.
Replay uses the recorded results and starts no backend calls.

## review-branch

Collects a committed diff, reviews it through independent lenses, tries to refute
each selected finding, then synthesizes a ranked report. Phases: Collect, Review,
Verify, Synthesize. It does not call Hermes's `/review` command.

| Argument | Default and limits |
| --- | --- |
| `base` | Collector resolves `origin/HEAD`'s target, then `main`, then `master` |
| `head` | `HEAD` |
| `lenses` | `correctness`, `security`, `simplicity`; 1–5 distinct choices, also `performance`, `tests` |
| `maxFindings` | 12; integer 1–20 |
| `maxDiffChars` | 60000; integer 2000–200000 |
| `focus` | Optional nonblank string, at most 500 characters |
| `tiers` | Overrides for the roles below |

Refs match `^(?!-)[A-Za-z0-9._/@^~-]{1,200}$`. The collector pins merge base,
reviewed head and its own checkout commit. When the checkout differs from the
reviewed head it supplies file contents from that head; reviewers use this
context rather than stale files in their worktree.

| Role | Default tiers | Toolsets | Worktree |
| --- | --- | --- | --- |
| collector | survey | file, terminal | yes |
| reviewers | reviewer-1, reviewer-2, reviewer-3 | file | yes |
| verifier | judge | file | yes |
| synthesizer | judge | todo | no |

One reviewer runs per lens. Findings are deduplicated within each lens, numbered
once, then ordered by severity, lens and number. Up to `maxFindings` are verified;
the rest are `not_verified`. A failed verifier gives `uncertain`, and failed
reviewers appear in `reviewers_failed`. A failed collector or all reviewers
failing ends the run with an error. The result includes `status` (`reviewed` or
`nothing-to-review`), refs, commits, `files_reviewed`, `truncated`, `lenses`,
`confirmed`, `uncertain`, `refuted`, `not_verified`, `reviewers_failed`, `summary`,
`ranked` and `themes`. Unknown synthesis ids are discarded and omitted ids appended.
Default maximum: **17 agents**; `2 + lenses.length + maxFindings`, at most **27**.

## research

Plans queries, sweeps enabled source kinds, reads the most cited sources and
cross-checks the synthesizer's citations. Phases: Plan, Sweep, Read, Synthesize.

| Argument | Default and limits |
| --- | --- |
| `question` | Required nonblank string, at most 2000 characters |
| `web`, `code`, `docs` | true; at least one must be true |
| `maxReads` | 6; integer 1–10 |
| `tiers` | Overrides for the roles below |

| Role | Default tier | Toolsets | Worktree |
| --- | --- | --- | --- |
| planner | survey | todo | no |
| web-searcher | survey | web | no |
| code-searcher | survey | file | yes |
| web-reader | survey | web | no |
| code-reader | survey | file | yes |
| synthesizer | judge | todo | no |

Documentation uses the web roles. A missing web backend can yield unavailable
searches without failing session creation; a suppressed web toolset can fail the
call. Both are handled. `unavailable` lists only requested kinds for which every
searcher was unavailable. Locations are trimmed; URLs lose fragments and one
trailing slash. Sources merge by exact normalized location and rank by distinct
searcher count, kind and location. Claims survive only when every cited source
was actually read and accessible.

The result contains `status` (`answered` or `no-sources`), `question`, `answer`,
`claims`, `removed_claims` with reasons, `open_questions`, `sources_read`,
`sources_unread` and `unavailable`. No accessible reads means no synthesizer;
a failed synthesizer after accessible reads fails the run.
Default maximum: **13 agents**; `2 + searchers + maxReads`, up to five searchers
and **17** agents overall.

## fix-failing-tests

Finds failures, fixes each independently, and verifies valid commits in separate
worktrees. Phases: Find, Fix, Verify. Each fixer starts from the same baseline;
verified fixes can still leave other baseline tests failing.

| Argument | Default and limits |
| --- | --- |
| `testCommand` | Optional command; otherwise finder selects the project's suite command |
| `maxFixes` | 5; integer 1–10 |
| `tiers` | finder: survey, fixer: builder, verifier: survey |

All three roles use `file, terminal` in a worktree. `testCommand` must be a single
line of 1–500 characters, without NUL. An explicit command is passed verbatim in
every prompt. Finder-reported commands are fenced evidence. Failures are
deduplicated and sorted by id, with at most `maxFixes` attempted.

A fixer commits on its own current branch. A verifier brings its own branch to
the checked commit with `git merge --ff-only`, preserving the branch name needed
for resume. Verification requires a successful fast-forward, a passing target,
a completed suite and no new failing ids outside the baseline. A nonzero suite
exit with no named failures is unverified. Only a valid commit starts a verifier.

The result includes `status` (`fixed-all`, `partial`, `none-fixed`, `no-failures`,
`unparsed-failure`), `test_command`, `exit_code`, `excerpt`, `failing_total`,
`baseline`, per-test `attempted` records, `not_attempted` and `next_steps`.
Nothing is merged or pushed. Review each local branch before accepting it.
Default maximum: **11 agents**; `1 + 2 * maxFixes`, at most **21**.

## ultracode

Surveys code, compares independent designs, chooses a plan, implements it tests
first, then calls `review-branch` as a child workflow. Phases: Understand, Design,
Choose, Implement, Review. Nesting is one level deep.

| Argument | Default and limits |
| --- | --- |
| `task` | Required nonblank string, at most 8000 characters |
| `designers` | 3; integer 1–5 |
| `testCommand` | Optional single-line hint, same validation as above |
| `review` | Review arguments above, except `base` and `head` are forbidden |
| `tiers` | Overrides for the roles below |

| Role | Default tiers | Toolsets | Worktree |
| --- | --- | --- | --- |
| scout | survey | file | yes |
| designers | judge | file | yes |
| chooser | judge | todo | no |
| implementer | builder | file, terminal | yes |

Review arguments are checked before any agent starts. Failed scouts and choosers
have logged fallbacks; all designers failing fails the run. A single surviving
proposal bypasses the chooser. The implementer records its base commit, writes
and runs failing tests before code, runs the suite, and commits on its own branch.
A partial implementation with valid distinct commits is still reviewed.

The result contains `status` (`implemented` when review was attempted, otherwise
`blocked`), `task`, `design`, proposal titles, `implementation`, `review`,
`review_error` and `next_steps`. Missing child scripts produce the installation
hint `run talaria workflow builtins install`. A review failure preserves the
reviewable implementation. Nothing is merged or pushed.
Default maximum: **23 agents**; `3 + designers + child's bound`, at most **35**.

## Staffing and safety

This staffing excerpt defines the six default tiers. Replace the example model
and provider with models served by your Hermes configuration; use three provider
choices for the three reviewer tiers when comparing independent providers.

```toml
[tiers.survey]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
[tiers.builder]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
[tiers.judge]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
[tiers.reviewer-1]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
[tiers.reviewer-2]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
[tiers.reviewer-3]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
[limits]
max_in_flight = 4
```

The scheduler controls width; scripts do not set it. Lower configured concurrency
queues parallel reviewers. These built-ins make finite turns through the normal
Hermes binding. Hermes v0.22.4 has no read-only file toolset: `file` includes write
and patch tools. Every file-capable agent starts in its own worktree, and prompts
forbid writes outside that folder and changes to git configuration, other branches,
tags and remotes. Worktrees separate relative edits; they do **not** enforce
confinement against absolute or `..` paths.

Finite turns cannot ask for interactive approval. Dangerous commands are denied
by default only when there is no process/session yolo, approval-mode bypass or
saved pattern approval; `approvals.single_query_mode` can also enable approval.
Plain `git push`, `git commit` and `git merge --ff-only` are not dangerous-command
patterns in the pinned Hermes release. Prompts forbid pushing; only fixers and
the ultracode implementer may commit, on their current local branch. No built-in
merges into the operator's checkout or pushes. Review commits yourself before
accepting them. Secret-shaped diff text may be masked in stored Hermes results.
