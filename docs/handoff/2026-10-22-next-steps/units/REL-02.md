# REL-02: User documentation: guide, keybinding and configuration reference, and migration from 0.6.5 (unit REL-02)

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says which four documents to write or finish, which tests to write first, and which scenarios a tester runs. This document holds what a builder would otherwise guess: the shape of every document, the facts about the Python client 0.6.5 that the migration page must state (with where they were read), the few Hermes facts the pages repeat (with `path:line` at Hermes v0.22.4), the exact formats the tests parse, the expected outcome for every 0.6.5 setting, the drafted notice counts and how they were derived, and the design reasoning. Where this document says "expected" or "drafted", the merged code is the authority: the tests exist to find the places where the page and the code disagree, and the page is the thing that gets corrected.

## 1. What this unit is for, in plain terms

Talaria 0.7.0 is a rewrite in Rust of the Python client 0.6.5. Three kinds of reader arrive at the release:

1. A new user who has Hermes running and wants to start a session. They need one page that gets them from install to a first answer, and tells them what to do when the first run fails.
2. A daily user who wants to change a key or a setting. They need two reference pages that are true: every default key, and every setting in `~/.talaria/config.toml`.
3. A 0.6.5 user who has a config file, a status-line script and maybe a chosen theme. They need to know, setting by setting, what still works, what warns, what moved, and what now has no effect. They also need to know how to stay on 0.6.5 for a while and how to go back.

The roadmap keeps release ceremony small (non-goal in [01-vision-and-requirements.md](../01-vision-and-requirements.md#non-goals)), so this unit adds four pages, two fixtures and one test file that ties the pages to the code. The 0.6.5 `docs/configuration.md` was itself kept honest by a test that parsed its example; this unit keeps that practice.

## 2. The documents

| File | New or edited | Reader | Source of truth |
|---|---|---|---|
| `docs/guide.md` | new | new and daily user | the merged binary: `talaria --help`, each subcommand's `--help`, the slash palette |
| `docs/keybindings.md` | new | daily user | `KeyMap::defaults()` and `Action::ALL` in `crates/talaria-tui/src/keys.rs` |
| `docs/configuration.md` | edited (replaced earlier by `{{P1-08}}`) | daily user, 0.6.5 user | the loader in `crates/talaria-app/src/config.rs` and its tests |
| `docs/migrating-from-0.6.5.md` | new | 0.6.5 user | the 0.6.5 documents at tag `v0.6.5`, the loader and the key map |
| `README.md`, `docs/00-index.md`, `llms.txt` | edited (links only) | everyone | the four pages |
| `docs/formats/status-line.md` | **not edited** | script authors | frozen by `{{PH1B}}` decision C-8; amending it is the operator's |

### 2.1 `docs/guide.md` outline (headings in this order)

1. `## What Talaria is` (three sentences: a terminal client for the Hermes agent backend; it holds no agent logic; Hermes does the work).
2. `## Install` (only paths that exist on the candidate commit; section 5).
3. `## Before you start` (a running Hermes gateway; what a profile is; that Talaria never installs or starts Hermes for you except through the `hermes gateway ensure --json` call the client makes unless `--no-ensure` is given; confirm against `{{P1-01}}` and `{{P1-12}}` in the merged code).
4. `## Your first session` (`talaria`, ask a question, `/quit`, `talaria -c`, `talaria --resume <id>`).
5. `## Working in a session` (composer keys by pointer to `keybindings.md`; approvals and questions; tool cells and diffs; queue and steer; rewind; model picker; `@` mentions; `!` shell; the pager).
6. `## Many sessions at once` (fleet view, background launch, worktree launch, attach, notifications).
7. `## Settings` (settings screens, `/configure`, the doctor; what is stored in Hermes and what in `~/.talaria/config.toml`).
8. `## Workflows` (`talaria workflow run|resume|list|stop`, the built-ins).
9. `## Scripts and automation` (`talaria -p`, exit codes, JSON output, the status-line command).
10. `## When something goes wrong` (a table: what you see, what it means, what to do; at least the six situations of section 6).
11. `## Where things live` (config file, Hermes home, session files; `~` forms only).

Owners of what the guide describes (read their merged code and `--help`; the binary's help is the source of every command and flag): `{{P1-12}}` the session commands and errors; `{{P2-14}}` `-p`; `{{P2-03}}` the slash palette; `{{P2-16}}` permission modes; `{{P3-02}}` the fleet view; `{{P3-03}}` background and worktree launch; `{{P3-06}}` notifications; `{{P4-01}}` settings and `/configure`; `{{P4-10}}` the doctor; `{{P5-06}}` the workflow subcommands (there is no drafted body for it in this set yet; the merged binary decides).

Every command in a fenced `bash` block is either copied from the running binary's help or run once by hand against the operator's throwaway `fixture` profile; commands that need a gateway or cost money are marked in prose. Output goes in `text` blocks, never `bash`, because test 4 reads every `bash` line that starts with `talaria` on all four pages. After the install section the guide's blocks use the bare command `talaria` (the install section puts the binary on the path).

If a feature an outline heading names is not in the merged binary (for example `talaria --help` lists no `workflow` subcommand because `{{P5-06}}` did not merge), the heading stays (test 1 checks it), the section says in one sentence that this build does not include the feature, and it shows no command for it. The pull request names each such section.

### 2.2 `docs/keybindings.md` shape

One table for each key context that has at least one action in `Action::ALL`, in the order the contexts first appear there, and no table for any other context. Every action in `Action::ALL` is configurable through `[keys]` (`{{P1-08}}` section 3.3 starts from the defaults of every action and lets the file rebind each), so "configurable context" and "context with an action" are the same thing. The heading is `## <Context> keys`, where `<Context>` is the `KeyContext` variant's `Debug` name split before each inner capital, first word capitalised, the rest lower case: `Composer` gives `## Composer keys`, `QueueSelection` gives `## Queue selection keys`, `Fleet` gives `## Fleet keys`. A context with no action in `Action::ALL` (a variant left with no actions, or one whose unit did not merge) gets no table; test 5 fails if one is written.

Columns: `Action | Default keys | What it does | Config name`. One row per action of the context, including actions unbound by default.

- `Action`: a short plain name.
- `Default keys`: the default chords as `KeyChord::label()` prints them (`Shift+Enter`, `Ctrl+C`, `Esc`), separated by a comma and a space; `none` when the action is unbound by default.
- `What it does`: one clause.
- `Config name`: the `[keys]` name in backticks (`Action::config_name()`), or `fixed` for a key that cannot be changed (the test skips those rows).

Drafted contents, for orientation only (the test decides): `{{P1-08}}` gives twenty composer actions; `{{P2-11}}` adds `toggle_tasks`, `{{P2-16}}` `approvals_toggle`, `{{P3-06}}` `attention_next`, `{{P3-04}}` `agents_panel`, `{{P3-02}}` `fleet_view_open`, all in the composer context; `{{P2-03}}`, `{{P2-08}}`, `{{P3-02}}`, `{{P3-03}}`, `{{P3-04}}`, `{{P3-07}}`, `{{P4-01}}` and `{{P4-10}}` add contexts (completion menu, queue selection, fleet, agents, review, settings, doctor).

A section with the level-two heading `## Keys that do not reach Talaria` names F1, F2, Ctrl+B and Ctrl+S in words and says why: macOS takes F1 and F2, Herdr takes Ctrl+B when nested, Ctrl+S is the terminal flow-control stop key (see [02-current-state.md](../02-current-state.md#what-to-carry-into-the-rewrite)). A file may still bind them and gets a warning. The same section says Shift+Enter needs a terminal that supports the keyboard enhancement protocol (Kitty's); otherwise Alt+Enter or Ctrl+J inserts a newline, and the composer's hint shows Alt+Enter (`{{P1-08}}` unit specification 3.2 and 4.4).

This page is the reference for chords. `docs/configuration.md` names the settings and points here.

### 2.3 `docs/configuration.md`

`{{P1-08}}` replaced this file with a page whose only fenced `toml` block is the default shape, guarded by `config_doc.rs::doc_default_shape_loads_cleanly` (exactly one `toml` block; it loads with no notices) and `keymap_doc.rs::doc_keys_equal_defaults` (that block's `[keys]` through `KeyMap::from_entries` equals `KeyMap::defaults()`). Its "under 200 lines" rule was `{{P1-08}}`'s merge check (`wc -l` in its acceptance criteria), not a test; later units added actions to the `toml` block. This unit keeps both tests green, keeps exactly one `toml` block, writes no chord in quotes (`{{P1-08}}`'s `git grep` for a quoted F1, F2, Ctrl+B or Ctrl+S), and aims to stay under 300 lines by leaving chord tables to `keybindings.md`. The 300 lines are an aim, not a check: when completeness and the aim conflict, completeness wins and the pull request says why the page is longer. What the page must contain at the end:

- the file location and that it is read once at start (restart to apply);
- one Markdown table per table of the file (`[keys]`, `[status]`, `[environment]`, and any table a later unit added), each with the columns `Key | Type | Default | Range | Example`, one row per key; for `[keys]` a short rule (a string or a list; `[]` unbinds) and a pointer to `keybindings.md` instead of a row per action;
- what happens on a mistake (one plain notice per problem, a default for that key, the program still starts), with each notice kind in one line;
- which 0.6.5 features are gone, in one sentence, pointing to `migrating-from-0.6.5.md`;
- environment variables in two groups:
  1. Read by the binary itself: take the list from the merged `talaria-cli` and `talaria-app` sources (`std::env::var` outside tests). Drafted: `HERMES_HOME` (where Hermes lives; `{{P1-01}}` discovery); `HOME` (for `~/.talaria/config.toml` and the default Hermes home). Test-only switches such as `TALARIA_LIVE_REQUIRE` are not documented for users.
  2. Seen only by the status command (`{{P1-10}}` REQ-11): `PATH`, `HOME`, `SHELL`, `TERM`, `TMPDIR`, `LANG`, `LC_*`; the five names `TALARIA_CONFIG_DIR`, `TALARIA_GATEWAY_URL` (scheme, host, port and path only), `TALARIA_PROFILE`, `TALARIA_LOG_LEVEL`, `TALARIA_STATUS_INTERVAL` when set; names in `environment.allowlist` when present. A name the credential check flags never forwards, and these four never forward in any letter case: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `AWS_ACCESS_KEY_ID`, `GITHUB_PAT`.

  The page says plainly: no `TALARIA_*` variable changes a setting; `TALARIA_CONFIG_DIR` does not move the configuration file (it is only passed through to the status command, as 0.6.5 did); there is no repository-level file (`{{P1-08}}` non-goals).

### 2.4 `docs/migrating-from-0.6.5.md` outline

1. `## Who this is for` and one paragraph: the client is rewritten in Rust; same command name `talaria`; the Python client's package directory `talaria/` is identical at tags `v0.6.5` and `python-final` (`{{P0-02}}` REQ-1 puts `python-final` on its start commit and checks that `git diff v0.6.5 <start> -- talaria` is empty; that check covers the package directory only, so the page says "the Python client's code", not "the repository"); a reader who wants the Python client installs `v0.6.5`. The builder runs `git diff --quiet v0.6.5 python-final -- talaria` (must exit 0, else stop) and compares `git rev-parse v0.6.5^{commit} python-final^{commit}`: two hashes give "two tags on different commits"; one hash gives "the same commit".
2. `## Staying on 0.6.5 for now` (section 4).
3. `## Installing 0.7.0 next to 0.6.5` (section 5.3).
4. `## Your config file` (section 7: the table, then the notices block, then the count sentence).
5. `## Your status-line command` (section 8).
6. `## Your theme` (section 9).
7. `## What no longer exists` (section 10).
8. `## Going back to 0.6.5` (section 5.3).

## 3. Facts about 0.6.5 the migration page states (read at tag `v0.6.5`; re-read before relying on them)

All from the public repository at the 0.6.5 tag.

| Fact | Where it was read |
|---|---|
| The user file is `~/.talaria/config.toml`; `TALARIA_CONFIG_DIR` relocated the whole directory; a repository file `./.talaria/config.toml` also existed; precedence was command line, `TALARIA_*` environment, repository file, user file, defaults. | `docs/configuration.md` "Locations and precedence" |
| The default file has tables `[theme]`, `[ui]`, `[status]`, `[environment]`, `[composer]`, `[notifications]`, `[keys]`, an empty `[profiles.endpoints]` with one commented example, and a commented `[connections.home]` example. `status.command` is a comment because "disabled" is the absence of the key. | `docs/configuration.md` "Complete default shape" |
| 0.6.5 default keys: `toggle_inspector` ctrl+o, `interrupt` ctrl+s, `agents` ctrl+g, `commands` f3, `models` f11, `profiles` f12, `config` ctrl+k, `follow` f5, `replay_pause` f8, `replay_slower` f9, `replay_faster` f10. | same block, `[keys]` |
| The status command is the `[status]` table, key `command`, split with POSIX quoting and never passed to a shell; `interval_seconds` default 5; one JSON document (input version 1) on standard input; plain text rows or a version 2 JSON document out; rows shown as literal text, so ANSI colour codes appear as visible characters; a 2 second timeout that kills the process group; a default-deny environment with five `TALARIA_*` names. | `docs/formats/status-line.md` |
| Themes: five built in (`refined-default`, `dark-green-terminal`, `neutral-dark`, `accessible-high-contrast`, `homebrew`), selected by `theme.name`; imported themes lived in a `themes/` folder under the config directory; a theme chosen in the picker was written back to the file. | `docs/themes.md`, `docs/configuration.md` |
| The package installs a command named `talaria` (`[project.scripts]`, `talaria = "talaria.cli:main"`) and is installed with `uv tool install git+https://github.com/infiquetra/talaria@<tag>`. It is not on the Python Package Index under this project. | `pyproject.toml`, `docs/install.md` |
| 0.6.5 supported macOS for interactive use; Linux ran only in continuous integration. | `docs/install.md` |
| The status starter script is `docs/examples/status_bar_starter.py` at the tag. | tag contents |

The merged `docs/formats/status-line.md` is byte for byte the 0.6.5 file (checked on 2026-10-22: `git diff v0.6.5 origin/main -- docs/formats/status-line.md` is empty). It still names a repository config file, `talaria/config.py` and `TALARIA_STATUS_INTERVAL_SECONDS`. `{{PH1B}}` decision C-8 freezes it and queues the correction for the operator; its acceptance criterion 10 checks the file is unchanged.

A naming discrepancy to carry honestly: the roadmap text for `{{P1-08}}` says "the `[status_line]` table", but 0.6.5 and the `{{P1-08}}` issue use `[status]`. The page states what the merged loader accepts; if it accepts only another name, the row is `moved` and the question is reported on the issue.

## 4. The interim advice (decision 7 of `08-risks-and-open-questions.md`)

Decision 7, in plain words: until the new client passes the phase 2 check the operator uses Hermes's own terminal interface (`hermes --tui --native`), makes no further changes to the Python client, and, if the Python client stays installed, reinstalls it with bytecode compilation. By release the phase 2 check has passed, so the page presents this as advice for a reader who stays on 0.6.5 on purpose (for example until they have moved their status script).

### 4.1 Hermes facts (verified at Hermes v0.22.4)

| Fact | Evidence |
|---|---|
| `--tui` exists: "Launch the modern TUI instead of the classic REPL". | [`hermes_cli/_parser.py:208-209`](https://github.com/NousResearch/hermes-agent/blob/v0.22.4/hermes_cli/_parser.py#L208-L209) |
| `--native` (alias `--tui-native`) exists: "With --tui: use native terminal scrollback and disable mouse tracking". | `hermes_cli/_parser.py:210-211`; the chat sub-parser repeats both at lines 319 to 321 |
| The entry point treats `--tui`, `--native`, `--tui-native` and `HERMES_TUI=1` as TUI requests. | [`hermes_cli/main.py:297`](https://github.com/NousResearch/hermes-agent/blob/v0.22.4/hermes_cli/main.py#L297) |

The page says "at the Hermes version this release was tested with" and does not promise the flags for later versions.

### 4.2 uv facts (verified with uv 0.13.4 on the drafter's machine, `uv tool install --help`)

- `uv tool install --compile-bytecode` exists, with `UV_COMPILE_BYTECODE=1` as its alternative.
- `uv tool uninstall <name>` exists and removes only tools uv installed.
- `UV_TOOL_DIR` and `UV_TOOL_BIN_DIR` choose where tools and their executables go (UAT-7 uses them so a test never touches the reader's real install).

### 4.3 Speed and the approval problem

- The handoff measured a cold launch of 0.6.5 at about 1.1 s without bytecode compilation and about 0.4 s with it, on one machine ([research/u02-talaria-performance.md](../research/u02-talaria-performance.md), "Bytecode at install"). The page says "measured once, on one machine" and gives both figures or none.
- The claim that a shell exporting `PYTHONDONTWRITEBYTECODE=1` slows other Python tools was never measured (decision 7 says "probably"). The page leaves it out or says "not measured".
- The handoff reports that 0.6.5's approval path is broken against current Hermes. The drafter did not re-verify this. The builder either reproduces it once (a live approval with the 0.6.5 client on the `fixture` profile) and states what was observed, or writes "the project's handoff reports that approvals in 0.6.5 do not work reliably with current Hermes; not re-checked for this page". No third option.

## 5. Install, the command collision and going back

### 5.1 What exists depends on `{{P6-07}}`

`{{P6-07}}` (binaries per platform and a Homebrew tap) and this unit both start after `{{PH5-ACC}}`, so either may merge first; `{{P6-07}}` is a deferred phase 6 unit the operator schedules. The merged tree alone decides whether `{{P6-07}}`'s work is present; issue state is not read. Present means both: a workflow file under `.github/workflows/` that triggers on a pushed `v*` tag and uploads release binaries (look for `on: push: tags` and a release-upload step), and install documentation from `{{P6-07}}` naming the Homebrew tap. Neither present: absent. Exactly one present: stop and ask the lead (issue stop condition). `{{P6-07}}`'s body names its files: it keeps `{{P0-02}}`'s `.github/workflows/release.yml` with its `v*` tag trigger and adds a `publish` job that attaches four `talaria-<version>-<target>.tar.gz` archives and `SHA256SUMS`, built through the new `.github/workflows/package-build.yml`; its install documentation is the new `docs/install.md`, which names the tap `infiquetra/homebrew-talaria` and the command `brew install infiquetra/talaria/talaria`. Before `{{P6-07}}` merges, `release.yml` already triggers on `v*` tags but attaches no files, so that file alone is not present. The content test above decides, not these names. The guide's install section always describes building from source: `cargo build --release` (toolchain pinned by `rust-toolchain.toml`), the binary at `target/release/talaria`, then either `install -m 755 target/release/talaria ~/.local/bin/` (a directory the reader has on the path) or `export PATH="$PWD/target/release:$PATH"`. It describes the GitHub release and Homebrew paths only when `{{P6-07}}` is present, using the commands its merged documentation gives. Otherwise it says nothing about them, and the pull request says so.

### 5.2 Decision 3 guard

The command name stays `talaria`. Nothing is published to crates.io, npm or the Python Package Index under that name, and the pages never tell a reader to install from one. The name is taken on all three, so `cargo install talaria` or `pip install talaria` gets someone else's software. The guide says this in one sentence that starts `Do not`, names crates.io, npm and the Python Package Index, and is written on one line (no hard wrap); test 10 checks it exists, because test 3 and the acceptance guard accept a registry command only on a line containing `Do not`. The guard runs on the four user pages, `docs/00-index.md`, `README.md` and `llms.txt` only; historical documents such as this specification are out of its scope.

### 5.3 The command collision and going back

A machine with 0.6.5 has a `talaria` executable in uv's tool bin directory (`uv tool dir --bin`). A 0.7.0 binary placed elsewhere may shadow it or be shadowed by it, depending on the order of `PATH`. The migration page:

- shows how to see which one runs: `command -v talaria` and `talaria --version`;
- installing 0.7.0: `uv tool uninstall talaria` removes the Python client, or the reader keeps it and puts the 0.7.0 directory first on the path;
- going back, step 1, by install method: a source-built copy is deleted (`rm ~/.local/bin/talaria`, or whichever directory the reader used) or its directory is removed from `PATH`; a packaged install is removed with the uninstall command `{{P6-07}}`'s merged documentation gives (only if present). uv cannot remove either;
- going back, step 2: `uv tool install --compile-bytecode git+https://github.com/infiquetra/talaria@v0.6.5`;
- going back, step 3: `command -v talaria` names uv's tool bin directory and `talaria --version` names 0.6.5;
- never tells the reader to delete `~/.talaria`: both versions read the same file (0.6.5 ignores the 0.7.0 `[keys]` names it does not know; check this once and say what happens, or leave it out).

## 6. The messages the guide's "When something goes wrong" table must quote

The builder copies each first line from the merged binary by triggering it, never from this list. Drafted texts (check the merged ones):

| Situation | Owner | Drafted first line | Exit |
|---|---|---|---|
| No gateway reachable | `{{P1-12}}` | `talaria: no Hermes gateway is running for this home` (then a `Next step:` line naming `hermes gateway ensure --json`) | 1 |
| A profile the gateway does not serve | `{{P1-12}}` | `talaria: the gateway never discovered a profile named <name>` | 1 |
| `-c` in a folder with no earlier session (gateway running) | `{{P1-12}}` REQ-9 | `talaria: no earlier session found for this folder.` | 1 |
| A config file with a syntax error | `{{P1-08}}` 4.3 | `Talaria could not read ~/.talaria/config.toml (line L, column C): MESSAGE. Using the default settings.` | starts normally |
| An unknown slash command | `{{P2-03}}` | the refusal its merged palette shows | none |
| One doctor row | `{{P4-10}}` | one row of its merged output and its fix | none |

The unknown slash command is triggered with `/nosuchcommand` (UAT-5; `{{P2-03}}` REQ-11 drafts `Hermes does not know /<name>. Type / to see the commands you can use.`). The doctor row is triggered with `talaria doctor --profile nosuchprofile` with the gateway running (UAT-6; `{{P4-10}}` REQ-1 and REQ-17: `talaria doctor [--profile NAME]` prints a header, rows with severity, name and summary, and a `Fix:` line under each problem, and exits non-zero when a check failed). The table quotes the first row whose severity is not `ok` and its `Fix:` line.

"First line": for a message printed to the terminal after Talaria exits, its first line. For text drawn inside the screen (the config notice, the slash refusal), the first non-empty line of that message as `herdr pane read` shows it, with border and box-drawing characters and surrounding spaces removed. A name the reader supplies is written `<name>` in the table and compared after substitution.

UAT-6 requires exit 1 for the first three; a different code is a defect against `{{P1-12}}`, not a page edit.

## 7. The migration table: format, notices and expected outcomes

### 7.1 Format (the test parses this)

Under the heading `## Your config file`, the first Markdown table has exactly these columns:

`0.6.5 setting | Outcome | 0.7.0 setting | What to do`

- **Setting.** A setting of a fixture is a dotted path to a value that is not a table, or to a table that is empty. The settings of the two fixtures together are the rows' universe. In the defaults fixture `profiles.endpoints` is an empty table and therefore a setting; `status.command` and `connections.fixture.label` come from the custom fixture.
- Column 1: a dotted path in backticks (`` `keys.interrupt` ``), or `` `table.*` `` for every setting under one top-level table when all behave the same; the test expands it to that table's settings in both fixtures.
- Column 2: exactly one of these four, in plain text:
  - `kept`: the setting works as it did and produces no notice.
  - `kept with a warning`: allowed only for `keys.` rows, because only there can the test see the effect. The value takes effect (the action's chords from `KeyMap::chords` equal the file's chords, compared by label) and exactly one notice contains the action's name. A warning row in any other table fails test 7; if one is ever needed, the issue gains an effect check first.
  - `moved`: the path in column 3 is understood; the 0.6.5 path is not. Test 7 writes two files with the same value: one with the old path (exactly one notice, containing the old path's last segment) and one with the column-3 path (no notice). None is expected today; the `[status]` naming note in section 3 keeps one possible.
  - `notice`: the setting has no effect and produces exactly one notice, which contains the first or the last segment of the path (an unknown table's notice names the table: `Ignoring [ui] ...`).
- Column 3: the 0.7.0 path for `moved`, otherwise a dash.
- Column 4: plain words telling the reader what to do.

### 7.2 Expected outcomes (from the sibling drafts; the merged code decides)

| 0.6.5 setting | Expected outcome | Column 4 (draft) | Why |
|---|---|---|---|
| `theme.name` | notice | Delete the table; set the Hermes skin instead. | Themes come from the Hermes skin (`{{P2-12}}`); unknown tables are reported. |
| `ui.*` | notice | Delete the table. | No `[ui]` table in the 0.7.0 loader as drafted. |
| `status.command` | kept | Nothing; check the script against the section below. | `{{P1-08}}` keeps `[status]` and `command`. |
| `status.interval_seconds` | kept | Nothing. | Range 1 to 3600, default 5. |
| `status.segments`, `status.cwd_max_columns`, `status.git_branch_max_columns`, `status.agent_model_max_columns` | notice | Delete the line. | The 0.7.0 status line drops segments by priority as width shrinks (`{{P1-10}}`). |
| `environment.allowlist` | kept | Nothing. | `{{P1-08}}` keeps `[environment]`. |
| `composer.*` | notice | Delete the table. | Paste handling is built in (`{{P1-08}}`, `{{P2-04}}`). |
| `notifications.transcript_line` | notice | Delete the table. | Replaced by bell and desktop notification (`{{P3-06}}`). |
| `keys.interrupt` | kept with a warning | Delete the line: Ctrl+S may not reach Talaria, and while the line is there Esc no longer stops a turn. | The action exists in 0.7.0 with default Esc. An explicit entry replaces the defaults, so the file's Ctrl+S becomes the only interrupt key and gets one `risky_chord` notice from `KeyMap::from_entries` (`{{P1-08}}` REQ-3, unit specification 3.3). |
| `keys.toggle_inspector`, `keys.agents`, `keys.commands`, `keys.models`, `keys.profiles`, `keys.config`, `keys.follow`, `keys.replay_pause`, `keys.replay_slower`, `keys.replay_faster` | notice | Delete the line; see `keybindings.md` for the 0.7.0 keys. | `unknown_action` from `KeyMap::from_entries`. |
| `profiles.endpoints` | notice | Delete the table and its entries; one gateway per Hermes home now. | The per-profile gateway map is left behind ([02-current-state.md](../02-current-state.md#what-to-leave-behind)). |
| `connections.*` | notice | Delete the table. | Remote gateways are phase 6 (`{{P6-01}}`). |

If the merged code differs, the table follows the code. Two tables, `status` and `keys`, mix outcomes, which is why those rows are per setting.

### 7.3 The notices a reader sees, and the block

`{{P1-08}}` splits the work in two. `load_file` reports file-level problems and passes `[keys]` through raw (`LoadedConfig.config.keys`); it does not check action names or chords. `KeyMap::from_entries` checks those and reports `unknown_action`, `bad_chord`, `all_invalid`, `risky_chord`, `conflict` and `default_lost`. `{{PH1B}}` decision C-3 makes `{{P1-12}}` run both at start-up, before the first frame, and show their notices. So the notices a reader sees are both lists.

The pipeline the tests use:

```rust
let loaded = talaria_app::config::load_file(&path);
let (_map, key_notices) = talaria_tui::keys::KeyMap::from_entries(&loaded.config.keys);
let seen: Vec<String> = loaded.notices.into_iter().chain(key_notices).collect();
```

The builder reads the merged start-up code first. If it uses another order, the test uses that order and names the start-up function in a comment. If a public function returns the combined list, the test calls it instead. If the binary adds a display prefix or wraps lines, that affects only the screen: the page's block holds the notice texts, and UAT-3 and UAT-4 remove the prefix and join wrapped lines before comparing.

Order inside the loader (`{{P1-08}}` unit specification 4.3): tables `keys`, `status`, `environment`, then unknown top-level names sorted by name; inside a table, key-name order. Order inside the key map (3.3): entries in key-name order, then conflict notices.

Drafted expectations, derived from those rules (the tests compare with the real pipeline; a different merged count is not an error, only a different page):

- Defaults fixture: loader 9 (the four unknown `[status]` keys `agent_model_max_columns`, `cwd_max_columns`, `git_branch_max_columns`, `segments`; then `Ignoring [composer]`, `[notifications]`, `[profiles]`, `[theme]`, `[ui]`) plus key map 11 (ten `unknown_action` and one `risky_chord` for `interrupt`) = **20 notices**. A later unit that adds a table such as `[ui]` or an action named like a 0.6.5 key changes this.
- Custom fixture (section 7.4): loader 2 (`Ignoring [connections] ...`, `Ignoring [theme] ...`) plus key map 2 (`unknown_action` for `commands`, `risky_chord` for `interrupt`) = 4 lines.

The page states the defaults count in one fixed sentence, on its own line, exactly once on the page, after the notices block: `A file holding every 0.6.5 default shows 20 notices.` (the number from the real pipeline). Test 9 finds it with the regular expression `^A file holding every 0\.6\.5 default shows (\d+) notices\.$` and fails if there are zero or several matches, so other prose may mention notices freely. Notices never show the file's absolute path (they use the literal `~/.talaria/config.toml`), so the block can be exact. On failure, tests 8 and 9 print the real list so the developer can copy it into the page.

### 7.4 The fixtures (normalised public text, not verbatim copies)

`fixtures/config/config-0.6.5-defaults.toml`: from the "Complete default shape" block of 0.6.5's `docs/configuration.md` at tag `v0.6.5`, keep every table header and active assignment, including the empty `[profiles.endpoints]` header, and remove every comment line. The removed lines are the commented `status.command` (a command), the commented `work` endpoint and the commented `[connections.home]` example (two loopback addresses). The parsed values equal 0.6.5's defaults. The fixture holds no comment at all; the pull request says where the values came from.

`fixtures/config/config-0.6.5-custom.toml`: a small file a real user might have, and the source of the optional settings the defaults omit:

```toml
[theme]
name = "homebrew"

[status]
command = "./status-demo.sh"
interval_seconds = 10

[environment]
allowlist = ["EDITOR"]

[keys]
interrupt = "ctrl+s"
commands = "f3"

[connections.fixture]
label = "local dashboard"
```

The status runner starts a command without a shell, so `command` is a program and its arguments. The connection has no address on purpose. No path, name, host or profile name other than `default` and `fixture`.

## 8. The status-line command during migration

What the page tells a script author, and where each statement comes from:

1. What stays (the `[status]` rows of the table; `{{P1-10}}` REQ-8 to REQ-12): `command` split with POSIX quoting and started without a shell, in the launch folder; `interval_seconds` 1 to 3600, default 5; one version 1 JSON document on standard input with the same fields as 0.6.5 (`version`, `mode`, `connection`, `session`, `turn`, `pending_prompts`, `subagents`, `usage`); plain text rows or a version 2 JSON document out; at most 8 rows; a 2 second timeout and a kill of the whole process group; a default-deny environment (section 2.3).
2. The differences, from `{{P1-10}}`'s merged code and tests, not from a diff of the format document (that diff is empty, section 3). Candidates to confirm: an empty or badly quoted command is refused with a notice and the command is off; failure markers such as `status: command timed out` appear in the status segment next to the last good text; ANSI escape bytes are shown as a visible stand-in (`␛`) instead of raw; the built-in line gains context-percentage and cost segments (`{{P1-10}}` REQ-2) and drops segments by priority, so `status.segments` and the `*_max_columns` keys have no effect; `TALARIA_STATUS_INTERVAL_SECONDS` and the repository config file no longer exist.
3. The format document link, with one sentence: its lines about a repository config file and `TALARIA_STATUS_INTERVAL_SECONDS` describe 0.6.5; amending the document is queued for the operator (`{{PH1B}}` C-8). The pull request lists those lines so the operator can find them. If the operator has amended the document by the time this unit runs, the sentence is dropped.
4. The 0.6.5 starter script is at `docs/examples/status_bar_starter.py` at tag `v0.6.5`. The page says how to fetch it (`git show v0.6.5:docs/examples/status_bar_starter.py` in a clone). It does not copy the script.

## 9. Themes during migration

- 0.6.5 had five built-in themes, a theme picker, a stored-theme folder and a Visual Studio Code theme importer (`docs/themes.md` at the tag). 0.7.0 does not read `theme.name` or the stored-theme folder (expected; table row `notice`). The importer and marketplace are left behind ([02-current-state.md](../02-current-state.md#what-to-leave-behind)).
- 0.7.0 uses the Hermes skin (`{{P2-12}}`). Verified Hermes facts at v0.22.4:
  - The setting is `display.skin` in the Hermes configuration, default `"default"`: [`hermes_cli/config_defaults.py:914`](https://github.com/NousResearch/hermes-agent/blob/v0.22.4/hermes_cli/config_defaults.py#L914) (inside the `display` table that starts at line 833).
  - Hermes has a `/skin [name]` command, "Show or change the display skin/theme", flagged `cli_only=True`: [`hermes_cli/commands.py:229-230`](https://github.com/NousResearch/hermes-agent/blob/v0.22.4/hermes_cli/commands.py#L229-L230). `cli_only` suggests the gateway does not offer it; check whether Talaria's merged slash palette lists it before the guide tells a reader to type it. If not, the page says to change `display.skin` in Hermes (or in Talaria's settings screen if `{{P4-01}}` exposes it).
  - The gateway tells every client the active skin: the `gateway.ready` payload has a `skin` property (`GatewayReadyPayload`, `apps/shared/src/gateway-contract.openrpc.json:18764`), and the notification `skin.changed` carries a `SkinPayload` with `name`, `description`, `colors`, `light_colors` and `dark_colors` (contract, `x-notifications`, line 49077).
- The page maps no 0.6.5 theme name to a skin and says so. It must not promise that a skin looks like a 0.6.5 theme. It mentions light and dark detection and reduced motion only as far as `{{P2-12}}` implemented them.

## 10. What no longer exists (check each against the merged tree)

From [02-current-state.md](../02-current-state.md#what-to-leave-behind) and the roadmap's P0-02 remove list: the theme picker, marketplace and Visual Studio Code theme importer; the per-profile gateway map `[profiles.endpoints]`; scraping the dashboard session token; replay pacing as a user feature (the recorder survives as a developer tool); the repository-level `./.talaria/config.toml` and `TALARIA_*` setting overrides (`{{P1-08}}` non-goals; check whether a later unit added them back); the acceptance-receipt scripts and documents; the Python package. These seven items are the list (the issue's REQ-12 refers here). The page carries each one that is true of the merged tree, with a one-clause "use this instead"; an item a later unit brought back is left out and named in the pull request.

## 11. Edge cases

| Case | Expected handling |
|---|---|
| No `~/.talaria/config.toml` | Nothing to migrate; the client starts on defaults. |
| A syntax error | One notice with line and column; defaults for the whole file. Troubleshooting row. |
| `interrupt = "ctrl+s"` | Kept with a warning; Esc stops interrupting while the line is there; the page says to delete the line. |
| An empty `[keys]` entry list (`yank = []`) | Unbinds the action (a 0.7.0 feature; not a 0.6.5 case, but the configuration page says it). |
| Both versions installed | `PATH` order decides; `command -v talaria` shows which. |
| A status script prints ANSI colour codes | 0.6.5 showed them literally; 0.7.0 shows a stand-in character (check `{{P1-10}}`'s merged code and say what it does). |
| Linux | 0.6.5 supported macOS only for interactive use; state the platforms 0.7.0 supports per `{{REL-01}}`'s evidence. |
| The page mentions a command the binary rejects | Test 4 fails; the page is corrected. |

### 11.1 The scenario preamble

The scenarios run from the repository root of the candidate, in a Herdr pane:

```bash
REPO=$(git rev-parse --show-toplevel); T="$REPO/target/release/talaria"
export HERMES_HOME="${HERMES_HOME:-$HOME/.hermes}"   # keep the real Hermes home before HOME changes
fresh() { X=$(mktemp -d); W=$(mktemp -d); mkdir "$X/.talaria"; cd "$W"; }
```

`$T` is absolute and always quoted. Every scenario calls `fresh` first, so it has its own scratch home (`$X`, with an empty `.talaria`) and working folder (`$W`), and every launch, UAT-6's four commands included, sets `HOME="$X"` so the tester's real `~/.talaria/config.toml` can never add notices or move an error off the first line. Exporting `HERMES_HOME` first keeps Hermes discovery pointed at the real gateway after `HOME` changes. Fixture paths use `$REPO`. UAT-6 keeps the gateway running for all four commands and isolates only the first with its own empty `HERMES_HOME`; the operator confirms no profile is named `nosuchprofile`.

UAT-2's newline: `{{P1-08}}` shows Alt+Enter in the hint when the terminal did not negotiate the keyboard enhancement protocol, and Shift+Enter needs it. The tester records which chord the hint shows and uses that one; both are documented, so either passes when it behaves as the page says.

### 11.2 When a scenario's drafted key or text differs from the merged program

The scenarios name keys and messages from the sibling drafts so a tester knows what to expect. They are not the contract; the page and the merged binary are. The rule:

- Keys: UAT-2 presses the chords the page's `## Composer keys` rows list for the config names `newline`, `cancel` and `interrupt` (the first chord of each row; for `newline`, the one the composer's hint shows). If the merged defaults are other chords, the tester uses those and records them. If one of the three config names does not exist, that is a defect against `{{P1-08}}` (the scenario is FAIL, reported, not adapted).
- Texts: UAT-5 and UAT-6 compare the screen with the guide's table rows, and the rows with the binary. A merged text that differs from the drafted one is not a failure: the page quotes the merged text and the evidence records both. It is a defect against the owner only when the merged text breaks the owner's own rule: a notice that shows an absolute path or lacks line and column (`{{P1-08}}` 4.3), an exit code other than 1 for the three `{{P1-12}}` refusals, or a refusal that sends anything to the model (`{{P2-03}}`).
- Who edits: the builder edits the page, never the scenario. A scenario step that cannot be run as written for any other reason goes to the lead as a question, and the scenario stays NOT RUN until answered.

UAT-7 uses a stand-in shell script that prints `talaria 0.7.0` to show command resolution with a competing executable without installing anything real; it then follows the page's going-back step for a source-built copy.

## 12. Test notes

- `crates/talaria-cli/tests/docs.rs` is one integration test file. It reads files relative to `env!("CARGO_MANIFEST_DIR")/../..` and runs the binary through `env!("CARGO_BIN_EXE_talaria")`, only with `--help`, `--version` and subcommand `--help`. No network, no gateway, no real home directory. Files it writes go into a fresh subdirectory of `std::env::temp_dir()` named from the test name and the process id, removed at the end; the `tempfile` crate is used only if `talaria-cli` already has it as a dev-dependency.
- Dependencies: the `toml` crate is already in the workspace through `talaria-app` (`{{P1-08}}` reads `toml` 1.1.8); add it to `talaria-cli`'s `[dev-dependencies]` at the workspace version. If `talaria-cli` does not already depend on `talaria-app` and `talaria-tui`, add them as path dev-dependencies. Nothing else. Do not add a Markdown crate; a 30-line table parser (split on `|`, trim, strip backticks) is enough.
- Public items used (`{{P1-08}}` names; use the merged ones): `talaria_app::config::load_file`, `LoadedConfig { config, notices }`, `Config.keys`; `talaria_tui::keys::{KeyMap::defaults, KeyMap::from_entries, KeyMap::chords, Action::ALL, Action::config_name, Action::context, KeyChord::label, KeyContext}`. If one is not public, the builder stops; a one-word `pub` the owner would accept is the only allowed edit, listed in the pull request.
- Test 2: each file's relative links resolve against that file's own directory (`README.md` and `llms.txt` against the root, the pages against `docs/`); links starting `http://`, `https://` or `mailto:` are skipped; an anchor is compared with the target's headings after GitHub's rule: lower case; remove every character that is not a letter, digit, space, hyphen or underscore (so backticks, parentheses, dots and colons go); spaces become hyphens; a repeated heading gets `-1`, `-2`. A broken link in `README.md`, `docs/00-index.md` or `llms.txt` that this unit did not add is fixed here (link-only edit) and listed in the pull request.
- Test 3 scans the four pages, the two fixtures and `llms.txt`, line by line, and fails on:
  1. a home or temporary directory prefix of macOS or Linux (as regular expressions, bracketed so this document does not match itself: `/[U]sers/`, `/[h]ome/`, `/[p]rivate/`, `/[t]mp/`, `/var/[f]olders/`);
  2. an attribution line (regular expressions `Co-authored-b[y]`, `Generated wit[h]`);
  3. `unattended` set to true in TOML or JSON spelling;
  4. an email address (regular expression `[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}`; the `talaria@v0.6.5` form does not match);
  5. a profile name: the word after `--profile`, `-P` if the binary has it, or `profile named`, other than `default`, `fixture` or `<name>`;
  6. advice to delete the configuration directory: a line holding `rm` and `.talaria`;
  7. `://` anywhere in a fixture;
  8. a line holding `npm install`, `cargo install`, `pip install` or `uv tool install` followed by the word `talaria` (not `talaria@`, not a `git+` URL) unless that line contains `Do not`.

  Host names, people's names and tokens cannot be scanned reliably; the reviewer reads for them.
- Test 4 covers all four pages. For each line inside a fenced block whose info string is `bash` and whose first word is `talaria`: path = []; for each following word not starting with `-`, run `talaria <path> --help` and, if the word appears as a subcommand in its `Commands:` section (clap's layout), push it and continue, else stop (the word is a positional argument such as `review` in `talaria workflow run review`). Then run `talaria <path> --help` and assert each `--long-flag` on the line (up to `=`) appears in it. A line with `<placeholders>` is parsed the same way. Lines whose first word is anything else (`cargo`, `uv`, `command`, `git`) are skipped.
- Test 5: group `Action::ALL` by `context()`. For each group, find the heading of section 2.2 and the first table after it, and assert: the set of `Config name` cells (rows marked `fixed` skipped) equals the set of `config_name()` of the group's actions, each exactly once (so an unbound action without a row fails); for each row, the chords from splitting `Default keys` on `,`, trimming and lower-casing (`none` gives the empty set) equal `KeyMap::defaults().chords(action)` mapped through `label()` and lower-cased. Then assert no `## ... keys` heading exists for a context outside the groups (the heading `## Keys that do not reach Talaria` does not end in ` keys` and is not matched), and no `Default keys` cell on the page holds `f1`, `f2`, `ctrl+b` or `ctrl+s`.
- Tests 6 and 7: read both fixtures with `toml::from_str::<toml::Table>`, collect settings as in 7.1, build each one-setting file by inserting the value at its path into an empty `toml::Table` and serialising it, write it to the temporary directory, and run the pipeline of 7.3. The `moved` and `kept with a warning` procedures are in 7.1.
- Test 9: the fixed sentence and regular expression of 7.3.
- Test 10 (`required_statements_are_present`) asserts:
  1. `docs/guide.md` has exactly one line starting `Do not` that contains `crates.io`, `npm` and `Python Package Index`.
  2. `docs/keybindings.md` has the heading `## Keys that do not reach Talaria`, and the text under it (to the next level-two heading) contains `F1`, `F2`, `Ctrl+B`, `Ctrl+S`, `Shift+Enter`, `Alt+Enter` and `Ctrl+J`.
  3. Each of `README.md`, `docs/00-index.md` and `llms.txt` has a link whose target ends in each of `guide.md`, `keybindings.md`, `configuration.md` and `migrating-from-0.6.5.md`.
  4. The migration page's `## Your status-line command` section contains `git show v0.6.5:docs/examples/status_bar_starter.py`, `TALARIA_STATUS_INTERVAL_SECONDS`, a link ending in `formats/status-line.md`, `version 1` and `2 second`.
  5. Intent item 5: some line of the migration page contains `approval` and one of `reported` or `observed`; and every line of the four pages that contains `PYTHONDONTWRITEBYTECODE` also contains `not measured` (the claim may be left out entirely). So the approvals claim is always present, labelled or replaced by a live observation, and the bytecode-variable claim is optional but never bare.
  6. The migration page's `## Your theme` section contains `display.skin`.

  What test 10 cannot check stays review work: that the status differences match `{{P1-10}}`'s tests, that no page promises a skin looks like a 0.6.5 theme, and the `/skin` rule (issue AC-15).
- Red reasons: before the pages exist each test fails with a message naming the missing file (`docs/guide.md does not exist`), never a panic from unwrapping.

## 13. Design reasoning

- Why a table test instead of prose review: "your theme is ignored" is a claim about code. The cheapest way to keep it true through six phases is to run the claim.
- Why four outcomes: a loader that reports every unknown name has `kept`, `moved` and `notice`. `keys.interrupt` showed a fourth case the first draft missed: a setting that still works and also warns. Calling it `notice` would tell a reader that a working setting does nothing; calling it `kept` would hide the warning. A fifth word (`ignored`) would mean silent behaviour, which `{{P1-08}}` forbids.
- Why the pipeline and not the loader alone: the reader sees both lists at start-up, and eleven of the twenty drafted default notices (the unknown actions and the risky chord) come from the key map. A loader-only check would make the page's count disagree with the screen.
- Why normalised fixtures: a verbatim copy carries two addresses and a command in comments; they would break the public-safety rule and add nothing the tests read.
- Why the format document stays frozen: `{{PH1B}}` C-8 gave the decision to the operator, and its acceptance criterion checks the file. The migration page can say the truth without editing a shared contract.
- Why the guide is not generated: a generated guide reads like a reference and fails the first-run reader. The tests check commands and keys, not prose.
- Why no `talaria migrate-config`: it would be a product feature owned by no unit, it would write to the reader's configuration file, and the notices already say what to change.
