# Configuration

Talaria reads one user configuration and an optional repository configuration at process startup.
The schema is additive: an older configuration remains valid, and every omitted newer key receives
the default shown here.

## Locations and precedence

The user file is `~/.talaria/config.toml`. Set `TALARIA_CONFIG_DIR` to relocate the whole Talaria
configuration directory; this is a path control, not a value inside `config.toml`. A repository may
add `./.talaria/config.toml`.

General precedence is highest first:

1. an explicit command-line override where a command exposes one;
2. a supported `TALARIA_*` environment variable;
3. `./.talaria/config.toml`;
4. the user `config.toml`;
5. built-in defaults.

The theme and reduced-motion rows deliberately have no command-line or environment override. A
theme selected in the running picker applies live and persists to the user configuration automatically.

External configuration edits are restart-to-apply. Talaria does not watch configuration files and does
not reload external file edits at runtime. In the running application, explicit theme selection
(`/theme select <name>` or `Enter` in the picker) applies live and persists `theme.name` to user scope
immediately, `/theme reload [name]` refreshes local stored theme files live, `/bar` changes the
status-bar segment set immediately for the running process, and `/config` opens the configuration
view described below.

## Complete default shape

This example is valid TOML and contains every table and default. `status.command` is commented
because its default is disabled, represented by omitting the key rather than by a TOML null value.

```toml
[theme]
name = "refined-default"

[ui]
reduced_motion = false
inspector_width = 36
inspector_open_at_start = false
inspector_dock_min_columns = 120
diff_side_by_side_min_columns = 112
show_timestamps = false

[status]
# command = "git status --short"
interval_seconds = 5
segments = [
  "cwd",
  "git_branch",
  "agent_model",
  "context",
  "task_progress",
  "connection",
  "version",
]
cwd_max_columns = 24
git_branch_max_columns = 18
agent_model_max_columns = 24

[environment]
allowlist = []

[composer]
paste_collapse_lines = 6
paste_collapse_bytes = 512
attachment_max_mb = 16

[notifications]
transcript_line = true

[keys]
toggle_inspector = "ctrl+o"
interrupt = "ctrl+s"
agents = "ctrl+g"
commands = "f3"
models = "f11"
profiles = "f12"
config = "ctrl+k"
follow = "f5"
replay_pause = "f8"
replay_slower = "f9"
replay_faster = "f10"

[profiles.endpoints]
# work = "ws://127.0.0.1:9119/api/ws"

# [connections.home]
# url = "http://127.0.0.1:8765"
# auth = "loopback"
# label = "local dashboard"
```

`tests/test_config.py` parses the fenced example above and asserts its values against the runtime
defaults.

## Schema

| Path | Type and default | Contract |
| --- | --- | --- |
| `theme.name` | string, `"refined-default"` | Selects a theme. The five built-in slugs and canonical stored imported slugs discovered under `<TALARIA_CONFIG_DIR>/themes/` are accepted at startup. An unknown or non-string value visibly falls back to Refined Default. There is no environment or command-line alias. See [Themes](themes.md) for scopes and persistence. |
| `ui.reduced_motion` | boolean, `false` | Makes nonessential progress frames static and routed scrolling immediate. A non-boolean value visibly falls back to `false`. There is no environment or command-line alias. Live when the running process re-applies motion policy. |
| `ui.inspector_width` | integer, `36` | Inclusive range 28–48, today's hardcoded inspector width. An invalid value visibly falls back to 36. No environment alias. Live on inspector resize. |
| `ui.inspector_open_at_start` | boolean, `false` | Whether the inspector starts docked open. A non-boolean value visibly falls back to `false`. No environment alias. Live. |
| `ui.inspector_dock_min_columns` | integer, `120` | Positive integer. The column count at which the inspector docks instead of overlaying. An invalid value visibly falls back to 120. No environment alias. Live. |
| `ui.diff_side_by_side_min_columns` | integer, `112` | Positive integer. The column count at which diffs render side-by-side. An invalid value visibly falls back to 112. No environment alias. Live. |
| `ui.show_timestamps` | boolean, `false` | Shows transcript timestamps. A non-boolean value visibly falls back to `false`. No environment alias. Live. |
| `status.command` | optional string, omitted/disabled | Runs as a fixed argument vector without a shell in the existing multi-row `StatusRegion`. An empty, non-string, or unparseable value disables only that region and produces a startup notice. `TALARIA_STATUS_COMMAND` is its environment alias. |
| `status.interval_seconds` | integer, `5` | Status-command cadence, inclusive range 1–3600. An invalid value visibly falls back to 5. `TALARIA_STATUS_INTERVAL_SECONDS` is its environment alias. |
| `status.segments` | array of strings, `['cwd', 'git_branch', 'agent_model', 'context', 'task_progress', 'connection', 'version']` | Sets display order and visibility for the true-bottom bar. Known names keep their first occurrence; unknown names are identified after controls are rendered visibly, and duplicate names are skipped with notices. If none remain, only `connection` renders. No environment alias. |
| `status.cwd_max_columns` | integer, `24` | Inclusive range 8–48. Invalid values visibly use 24. No environment alias. |
| `status.git_branch_max_columns` | integer, `18` | Inclusive range 8–40. Invalid values visibly use 18. No environment alias. |
| `status.agent_model_max_columns` | integer, `24` | Inclusive range 10–48. Invalid values visibly use 24. No environment alias. |
| `environment.allowlist` | array of strings, empty | Environment-variable names the optional status command may receive. Its child environment is default-deny; credential-like names remain subject to the status security boundary. No environment alias. |
| `composer.paste_collapse_lines` | integer, `6` | Collapses a paste meeting this line threshold. Zero or a negative value disables this half of the threshold. `TALARIA_COMPOSER_PASTE_COLLAPSE_LINES` is its environment alias. Live. |
| `composer.paste_collapse_bytes` | integer, `512` | Collapses a paste meeting this byte threshold. Zero or a negative value disables this half of the threshold. `TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES` is its environment alias. Live. |
| `composer.attachment_max_mb` | integer, `16` | Composer attachment size cap, Talaria's analogue of the Desktop attachment cap. A non-positive or non-integer value visibly falls back to 16. No environment alias. Live. |
| `notifications.transcript_line` | boolean, `true` | Whether `notification.show` renders a transcript line. A non-boolean value visibly falls back to `true`. No environment alias. Live. |
| `keys.toggle_inspector` | string, `"ctrl+o"` | Chord toggling the session inspector. `Ctrl+B` was the previous default; Herdr captures it when nested, so it stays documented as replaced rather than bound. `TALARIA_KEYS_TOGGLE_INSPECTOR` is its environment alias. Restart. |
| `keys.interrupt` | string, `"ctrl+s"` | Chord cancelling the in-flight turn. `Ctrl+C` left this action; pressed out of habit it reaches the text area's copy binding or the framework's quit hint, never the turn. `TALARIA_KEYS_INTERRUPT` is its environment alias. Restart. |
| `keys.agents` | string, `"ctrl+g"` | Chord toggling sub-agent rows. `TALARIA_KEYS_AGENTS` is its environment alias. Restart. |
| `keys.commands` | string, `"f3"` | Chord opening the command palette. `TALARIA_KEYS_COMMANDS` is its environment alias. Restart. |
| `keys.models` | string, `"f11"` | Chord opening the model picker. `TALARIA_KEYS_MODELS` is its environment alias. Restart. |
| `keys.profiles` | string, `"f12"` | Chord opening the profile picker. `TALARIA_KEYS_PROFILES` is its environment alias. Restart. |
| `keys.config` | string, `"ctrl+k"` | Chord opening the settings workspace. `TALARIA_KEYS_CONFIG` is its environment alias. Restart. |
| `keys.follow` | string, `"f5"` | Chord toggling follow. `TALARIA_KEYS_FOLLOW` is its environment alias. Restart. |
| `keys.replay_pause` | string, `"f8"` | Chord pausing replay. `TALARIA_KEYS_REPLAY_PAUSE` is its environment alias. Restart. |
| `keys.replay_slower` | string, `"f9"` | Chord slowing replay. `TALARIA_KEYS_REPLAY_SLOWER` is its environment alias. Restart. |
| `keys.replay_faster` | string, `"f10"` | Chord speeding replay. `TALARIA_KEYS_REPLAY_FASTER` is its environment alias. Restart. |
| `profiles.endpoints` | table of string URLs, empty | Compatibility alias mapping a Hermes profile name to the gateway endpoint Talaria should dial. Blank or non-string values are ignored. The map has no environment alias and is not a UI write target — edit it by hand. Restart. |
| `connections.<id>.url` | string dashboard base | One inventory entry's dashboard URL (`http(s)://…` or `ws://…/api/ws`). A URL carrying userinfo credentials is dropped on load and refused on write. No environment alias. Restart. |
| `connections.<id>.auth` | `"loopback"` or `"gated"` | Authentication mode for that connection. An unknown mode visibly falls back to `loopback`. Credentials themselves live in the 0600 credentials file, never in `config.toml`. Restart. |
| `connections.<id>.label` | optional string | Display label for the connection. Restart. |

The responsive widths and segment forms are fixed product behavior; changing a maximum does not move
a breakpoint. See [Terminal UI](terminal-ui.md#responsive-status-bar) for that table.

## Starter status configuration

Copy `docs/examples/status_bar_starter.py` somewhere durable (for example `~/.talaria/`) and point
`status.command` at it. Only the command needs setting; every other status key keeps its default
by deep-merge:

```toml
[status]
command = "python3 /home/operator/.talaria/status_bar_starter.py"
```

Replace `/home/operator` with the real absolute path. The command string is split with POSIX
quoting and exec'd directly — there is no shell, so `~` and environment variables do not expand
and a relative path resolves against the launch directory, not the configuration file. An empty,
non-string, or unparseable command disables only the status region with a startup notice; a
missing executable or a failing script keeps the rest of the interface usable and shows its
categorical failure marker in the region instead of rows.

## Status script input fields

Each tick writes one JSON document to the script's stdin. The field set below is verified against
`StatusPayload.to_json_dict()` (`talaria/domain/projection.py`), the single source the payload
encoder copies verbatim — anything not listed here is not in the input, however familiar its
name sounds from other tools.

| Field | Status |
| --- | --- |
| `version` (integer, always `1`) | always present |
| `mode`, `connection`, `turn`, `pending_prompts` | always present |
| `session` (`{id, title\|null}`) | always present |
| `subagents` (`{active, terminal}`) | always present |
| `usage` (`{input_tokens, output_tokens}`) | present, **null unless both counters have been observed** |
| context window (used, maximum, percent) | **absent — unavailable, label it, never fabricate it** |
| rate limits | **absent — unavailable, label it, never fabricate it** |
| spending / cost | **absent — unavailable, label it, never fabricate it** |

The three absences are real gaps in what the gateway path delivers to the script, not misreadings.
A `session.context_breakdown` call exists on the gateway but is not part of this input; wiring it
in is future work, not something a status script can reach today. The starter script above renders
each absence as a labelled `unavailable` row — copy that pattern rather than guessing numbers.

The child environment is default-deny: `PATH`, `HOME`, `SHELL`, `TERM`, `TMPDIR`, `LANG`/`LC_*`,
five `TALARIA_*` names (with the gateway URL's query string stripped), plus exactly the
`environment.allowlist` names that are not credential-shaped. Credential-like names never forward,
even allowlisted.

## When status changes apply

- Editing the status *script file* takes effect on the next tick: the runner spawns the script
fresh every tick, so there is nothing to restart and nothing cached. (`tests/status/test_runner.py`
pins this: `test_script_edits_take_effect_on_the_next_tick_without_restart`.)
- Changing status *configuration* — the command, interval, segments, column limits, or allowlist —
needs a restart. Configuration resolves once at startup and Talaria does not watch the files.
- `/bar` toggles the segment set live for the running process; it changes no file.
- This section promises nothing beyond status behavior: whether other configuration reloads live
is shared with the configuration views and decided there.

## The `/config` view

The `/config` command opens the configuration view: one screen showing exactly four settings —
the allowlist — each with its effective value and the precedence layer that supplied it
(default, user file, repository file, environment, or session). No credential, connection
setting, environment allowlist, column limit, or Hermes agent identity appears there, and none
can be written from it.

| Row | Control | Mode |
| --- | --- | --- |
| `theme.name` | opens the theme picker (closing the view — the picker is the palette's theme mode) | live: applies and persists through the theme flow |
| `status.command` | text; empty is allowed and labelled "no status script" | restart |
| `status.interval_seconds` | integer, the 1–3600 bound shown; invalid input is rejected inline and nothing is written | restart |
| `status.segments` | ordered multi-select over the seven known names (space toggles, shift+↑↓ reorders) | restart |

**Apply** writes only the changed keys to the user configuration file, through the same
byte-preserving targeted rewrite theme selection uses, generalized to whole status assignments
including the multi-line `segments` array. A second explicit action, **save to repository**,
writes the changed keys to the repository scope, mirroring `/theme save repository`. Cancel and
Escape write nothing. Where a hand-formatted file defeats the targeted rewrite — an inline
`status` table, a comment inside a replaced value, a dotted-and-table mix — the view refuses
and says to edit the file by hand rather than reformatting it.

A row whose value comes from the repository file is read-only for a user-scope apply, with that
reason shown: the repository file beats the user file, so a user-file write would be shadowed.
A row whose value comes from a `TALARIA_*` environment variable is read-only entirely, for the
same reason one level up. An empty `status.command` saves as the explicit `command = ""` — the
documented contract disables the region for an empty value, and one mechanism for that outcome
is enough.

After a save, each written row reads "saved: X · effective now: Y · takes effect on restart":
the status keys resolve once at startup, so the saved value differs from the effective one until
the next start. The segments row's effective value is the running bar's set — startup resolution
plus any `/bar` session toggles — and its source reads `session` while the two differ; `/bar`
remains session-only and is never written unless applied here.

## Validation and compatibility

Talaria deep-merges each configured table onto the defaults, so files written before 0.5.0 do not
need migration. A missing table or key takes the new default. After precedence resolves, the
`theme`, `ui`, `status`, `keys`, `composer.attachment_max_mb`, `notifications`, and `connections`
tables are normalized: an invalid value in those tables uses its documented fallback and adds a
visible startup notice. For `keys`, empty, non-string, and unrecognized chord names fall back to
their defaults; `ctrl+q` is reserved for quitting and falls back; and assigning two actions the
same chord resets both to their defaults.

A malformed `composer` paste-collapse threshold remains raw in the loaded configuration, then is
silently replaced by its default when the paste threshold is built. Blank or non-string
`profiles.endpoints` rows are silently dropped.
For an enabled status command, only a list of strings forwards as `environment.allowlist`.
Any other shape — `42`, `true`, `false`, `0`, `0.0`, a string such as `"FOO"`, a mapping, or a
nested list — falls back to the empty default with no notice: it never raises and never forwards
character fragments. Syntactically invalid TOML
is different: it is a launch error that names the offending file.

## What Talaria writes

Talaria writes through the same narrow, byte-preserving targeted rewrite for every DEFAULTS
table and for one `[connections.<id>]` entry at a time. Comments, layout, CRLF, and dotted
keys survive. A missing table is appended. Unsupported hand-formatted shapes — an inline
table, a comment inside a replaced value, a dotted-and-table mix — refuse and say to edit
the file by hand rather than reformatting. The parsed document is verified to differ from
the original in exactly the requested keys before anything is written.

`theme.name` persists to the user configuration immediately upon explicit theme selection
(`/theme select <name>` or `Enter` in the `/theme` picker) or explicit save (`/theme save [user]`),
and to repository configuration upon `/theme save repository`. Status keys persist when the
`/config` view's apply action writes the changed keys to the user file, or its save-to-repository
action writes them to the repository scope. The settings workspace uses the same writer for
the D12 Talaria-owned keys and for connection inventory entries.

`profiles.endpoints` is not a UI write target: a whole-map rewrite would re-render every
entry. Operators edit that alias by hand; the workspace manages `[connections.*]` instead.
A connection URL that carries `user:pass@` is refused at write time and dropped on load, so
the secret never lands in the file. Credential material (`token`, `access_token`,
`refresh_token`) belongs in `~/.talaria/credentials` at mode `0600`, never beside the URL.

`/bar` toggles a known segment in memory for the running process and never writes.

## Live versus restart-only

A saved Talaria key is live only when the running process can apply it without restart:

- **Live:** `theme.name`, `ui.reduced_motion`, `ui.inspector_*`, `ui.diff_side_by_side_min_columns`,
  `ui.show_timestamps`, `composer.*` thresholds and attachment cap, `status.segments` (Apply
  reuses the `/bar` path), `notifications.transcript_line`.
- **Restart:** every `keys.*` chord, `status.command`, `status.interval_seconds`,
  `environment.allowlist`, `profiles.endpoints`, and every `connections.*` field.

A restart-only row still saves immediately; the Save summary reads
`saved: X · effective now: Y · takes effect on restart` until the next start.

## Host administration

Hermes update checks and receipts, local-model status/catalog/hardware/jobs/search, and
gateway-migration plans are shown as read-only status or plan rows. The installed workspace
offers no Hermes-update, local-model mutation, or gateway-migration action. Those writes
remain host-side; Talaria does not expose them.
