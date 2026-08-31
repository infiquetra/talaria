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
theme selected in the running picker is a final, in-memory scope above the repository file.

Configuration is restart-to-apply. Talaria does not watch configuration files and does not reload
external edits in this release. `/theme` changes the current session immediately, and `/bar` changes
the current status-bar segment set immediately; neither action persists unless its own documented
save route exists.

## Complete default shape

This example is valid TOML and contains every table and default. `status.command` is commented
because its default is disabled, represented by omitting the key rather than by a TOML null value.

```toml
[theme]
name = "refined-default"

[ui]
reduced_motion = false

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

[profiles.endpoints]
# work = "ws://127.0.0.1:9119/api/ws"
```

`tests/test_config.py` parses the fenced example above and asserts its values against the runtime
defaults.

## Schema

| Path | Type and default | Contract |
| --- | --- | --- |
| `theme.name` | string, `"refined-default"` | Selects a theme. The four built-in slugs are accepted at startup. An unknown or non-string value visibly falls back to Refined Default. There is no environment or command-line alias. See [Themes](themes.md) for scopes and the current imported-theme persistence limitation. |
| `ui.reduced_motion` | boolean, `false` | Makes nonessential progress frames static and routed scrolling immediate. A non-boolean value visibly falls back to `false`. There is no environment or command-line alias. |
| `status.command` | optional string, omitted/disabled | Runs as a fixed argument vector without a shell in the existing multi-row `StatusRegion`. An empty, non-string, or unparseable value disables only that region and produces a startup notice. `TALARIA_STATUS_COMMAND` is its environment alias. |
| `status.interval_seconds` | integer, `5` | Status-command cadence, inclusive range 1–3600. An invalid value visibly falls back to 5. `TALARIA_STATUS_INTERVAL_SECONDS` is its environment alias. |
| `status.segments` | array of strings, `['cwd', 'git_branch', 'agent_model', 'context', 'task_progress', 'connection', 'version']` | Sets display order and visibility for the true-bottom bar. Known names keep their first occurrence; unknown and duplicate names are skipped with notices. If none remain, only `connection` renders. No environment alias. |
| `status.cwd_max_columns` | integer, `24` | Inclusive range 8–48. Invalid values visibly use 24. No environment alias. |
| `status.git_branch_max_columns` | integer, `18` | Inclusive range 8–40. Invalid values visibly use 18. No environment alias. |
| `status.agent_model_max_columns` | integer, `24` | Inclusive range 10–48. Invalid values visibly use 24. No environment alias. |
| `environment.allowlist` | array of strings, empty | Environment-variable names the optional status command may receive. Its child environment is default-deny; credential-like names remain subject to the status security boundary. No environment alias. |
| `composer.paste_collapse_lines` | integer, `6` | Collapses a paste meeting this line threshold. Zero or a negative value disables this half of the threshold. `TALARIA_COMPOSER_PASTE_COLLAPSE_LINES` is its environment alias. |
| `composer.paste_collapse_bytes` | integer, `512` | Collapses a paste meeting this byte threshold. Zero or a negative value disables this half of the threshold. `TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES` is its environment alias. |
| `profiles.endpoints` | table of string URLs, empty | Maps a Hermes profile name to the gateway endpoint Talaria should dial. Blank or non-string values are ignored. The map has no environment alias. |

The responsive widths and segment forms are fixed product behavior; changing a maximum does not move
a breakpoint. See [Terminal UI](terminal-ui.md#responsive-status-bar) for that table.

## Validation and compatibility

Talaria deep-merges each configured table onto the defaults, so files written before 0.5.0 do not
need migration. A missing table or key takes the new default. Invalid optional values use the
documented fallback and add a visible startup notice. Syntactically invalid TOML is different: it is
a launch error that names the offending file.

## What Talaria writes

Talaria writes only the top-level `[theme]` table, and only after `/theme save`, `/theme save user`,
or `/theme save repository`. The narrow writer changes `theme.name`, leaves every other key and
comment untouched, verifies the parsed document changed in exactly that way, and replaces the file
atomically. It is not a general configuration serializer.

No command writes the status, user-interface, environment, composer, or profile tables. `/bar`
toggles a known segment in memory for the running process and never writes. Inspector width and open
state, and diff mode/navigation state, are also process-local and have no configuration rows.
