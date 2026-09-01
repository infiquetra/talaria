# Themes

Talaria 0.5.0 has four built-in themes and a 58-token public color vocabulary. Theme
preview is immediate, but persistence is always an explicit action.

## Built-in themes

| Name | Configuration name | Base |
| --- | --- | --- |
| Refined Default | `refined-default` | light |
| Dark Green Terminal | `dark-green-terminal` | dark |
| Neutral Dark | `neutral-dark` | dark |
| Accessible High Contrast | `accessible-high-contrast` | dark |

Refined Default is the fallback when no theme is configured or a configured theme cannot be
resolved. Accessible High Contrast uses the release's strongest contrast palette; the name does not
claim that every terminal emulator, font, or user setting has been independently certified.

## Preview and select a theme

Enter `/theme` to open the theme picker. `Up` and `Down` preview the highlighted theme immediately.
`Enter` accepts it for the current Talaria process and closes the picker. `Escape` closes the picker
and restores the theme that was active when it opened. Browsing, previewing, accepting a session
theme, and quitting do not write a file.

Persistence is separate:

- `/theme save` saves the current theme in the user configuration. `/theme save user` is the
  explicit form of the same command.
- `/theme save repository` saves it in `./.talaria/config.toml` for the current repository.

Both commands change only the top-level `theme.name` setting, whether the existing TOML uses a
`[theme]` table, a dotted key, or an inline table. They preserve every other configuration key and
comment. See [Configuration](configuration.md) for paths, precedence, and the restart contract.

The four theme scopes resolve in this order, from lowest to highest precedence:

1. the built-in default, Refined Default;
2. the user configuration at `~/.talaria/config.toml`, or the directory selected by
   `TALARIA_CONFIG_DIR`;
3. the repository configuration at `./.talaria/config.toml`;
4. the unsaved selection held by the running process.

A later scope replaces the earlier selection. Configuration files are read at startup. Talaria does
not watch them, so an external edit or an explicit save affects the next process, while a picker
selection affects only the current one.

## Token vocabulary

Every runtime theme resolves all 58 canonical tokens below to an opaque uppercase `#RRGGBB` value.
UI code uses these semantic names instead of literal colors. The corresponding Textual custom
variable replaces dots with hyphens and adds `$`; for example,
`talaria.diff.added.background` becomes `$talaria-diff-added-background`.

| Tokens | Meaning |
| --- | --- |
| `talaria.canvas`, `talaria.surface`, `talaria.panel` | Application canvas, raised controls and code blocks, and secondary panels |
| `talaria.text`, `talaria.text.muted` | Primary prose and secondary metadata or hints |
| `talaria.primary`, `talaria.secondary`, `talaria.accent` | Primary action/operator identity, secondary action/reasoning identity, and active selection/activity |
| `talaria.success`, `talaria.warning`, `talaria.error` | Successful, pending/degraded, and failed/destructive states |
| `talaria.border`, `talaria.border.muted`, `talaria.focus` | Strong, inactive, and focused boundaries or carets |
| `talaria.selection.background`, `talaria.selection.text` | Selected cell fill and the text drawn over it |
| `talaria.status.background`, `talaria.status.text`, `talaria.status.muted`, `talaria.status.separator` | Bottom-bar fill, primary text, secondary text, and separators |
| `talaria.status.success`, `talaria.status.warning`, `talaria.status.error`, `talaria.status.attention` | Bottom-bar connection/success, pending/reconnect, failure, and queue-attention states |
| `talaria.inspector.background`, `talaria.inspector.border`, `talaria.inspector.heading` | Inspector fill, boundary, and headings/selected file |
| `talaria.transcript.operator`, `talaria.transcript.operator.background` | Operator gutter marker and entry tint |
| `talaria.transcript.assistant`, `talaria.transcript.assistant.background` | Assistant gutter marker and entry tint |
| `talaria.transcript.reasoning`, `talaria.transcript.reasoning.background` | Reasoning gutter marker and entry tint |
| `talaria.transcript.activity`, `talaria.transcript.activity.background` | Tool/subagent gutter marker and entry tint |
| `talaria.transcript.session`, `talaria.transcript.session.background` | System/prompt/cancellation gutter marker and entry tint |
| `talaria.transcript.fault`, `talaria.transcript.fault.background` | Error/protocol/unknown-event gutter marker and entry tint |
| `talaria.diff.context`, `talaria.diff.line-number` | Unchanged diff text and line numbers/omitted-line counts |
| `talaria.diff.added`, `talaria.diff.added.background` | Added-line glyph/foreground and fill |
| `talaria.diff.removed`, `talaria.diff.removed.background` | Removed-line glyph/foreground and fill |
| `talaria.diff.hunk`, `talaria.diff.hunk.background` | Hunk header/navigation marker and fill |
| `talaria.diff.intraline-added.background`, `talaria.diff.intraline-removed.background` | Changed spans within paired added and removed lines |
| `talaria.syntax.comment`, `talaria.syntax.keyword`, `talaria.syntax.string` | Comment, keyword, and string syntax classes |
| `talaria.syntax.number`, `talaria.syntax.function`, `talaria.syntax.type` | Numeric constant, callable, and type syntax classes |
| `talaria.syntax.variable`, `talaria.syntax.operator`, `talaria.syntax.constant` | Variable, operator/punctuation, and other constant syntax classes |

The exact values for every built-in theme and the contrast measurements are in the
[v0.5.0 visual specification](design/2026-08-30-talaria-v0-5-0-visual-spec.md#four-built-in-themes).

## Fallback behavior

An unknown configured name selects Refined Default and produces a visible startup notice. A partial
theme specification fills every missing canonical token from Refined Default and names those
fallbacks. Built-in themes are complete and cannot be replaced by an imported theme with the same
slug.

## Import a Visual Studio Code theme

Import a strict Visual Studio Code color-theme JSON file with:

```text
talaria theme import FILE [--name NAME]
```

The explicit name wins over a top-level theme name, which wins over the source filename. Talaria
requires the resulting name to already be a lowercase hyphenated slug. A name containing spaces,
uppercase letters, path separators, or traversal is rejected before anything is written:

```console
$ talaria theme import theme.json --name 'Solar Flare'
talaria: theme import failed: invalid theme slug: 'Solar Flare'
```

A valid import writes a canonical user theme under `<config-dir>/themes/<slug>.json`. Re-importing
the same slug replaces that file atomically. A malformed, empty, or unsupported input fails before
writing. The source file is never watched.

The importer supports a deliberately bounded set of workbench colors and syntax scopes, reports
unsupported inputs, composites supported alpha colors, and fills unmapped tokens from Refined
Default. The normative mapping and warning rules are in
[Visual Studio Code theme import format](formats/vscode-theme-import.md); they are not duplicated
here.

Imported themes become available to `/theme` only after a fresh process loads the stored theme
library. Once a canonical theme document exists under `<TALARIA_CONFIG_DIR>/themes/` (or the default
`~/.talaria/themes/`), its slug is accepted from `config.toml` at startup and can be persisted with
the same explicit save commands as a built-in theme. The stored library is read once per process;
there is no file watcher or external-file live reload.
