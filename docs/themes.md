# Themes

Talaria has five built-in themes and a 58-token public color vocabulary. Theme
preview is immediate, but persistence is always an explicit action.

## Built-in themes

| Name | Configuration name | Base |
| --- | --- | --- |
| Refined Default | `refined-default` | light |
| Dark Green Terminal | `dark-green-terminal` | dark |
| Neutral Dark | `neutral-dark` | dark |
| Accessible High Contrast | `accessible-high-contrast` | dark |
| Homebrew | `homebrew` | dark |

Refined Default is the fallback when no theme is configured or a configured theme cannot be
resolved. Accessible High Contrast uses the release's strongest contrast palette; the name does not
claim that every terminal emulator, font, or user setting has been independently certified.

## Homebrew

Homebrew is a restrained green-black theme for operators who live in green-tinted terminals but
find Dark Green Terminal's neon accents fatiguing. It is available everywhere a built-in theme is
— the `/theme` picker, `theme.name` configuration, and the theme registry — but it is never
selected at startup: a fresh process without configuration still opens on Refined Default.

Provenance: Homebrew was designed for Talaria v0.6.0, not sampled from any host terminal palette,
and it is not the Homebrew package manager — the shared name is a coincidence, and the palette
is Talaria-original. The same note rides the `/theme` picker row.
Its canvas, surface, and status fills are near-black greens (`#050905`, `#0A120D`, `#030604`);
body text is a soft green-grey (`#C9D9CE`); primary, accent, and transcript markers are muted
sages (`#6FA287`, `#4E9A6A`, `#5FA86F`) rather than neon. The flat chrome families
(canvas/surface/panel/text/border/selection/status/inspector) are the tokens a host terminal
palette could equally describe; the transcript, diff, and syntax families are Talaria-semantic and
stay Talaria-owned. Every body pairing holds the 4.5 text floor and every component pairing the
3.0 floor, measured the same way as the first four themes. Homebrew also exercises the groups
layer below: each transcript category carries its own subtle body tint while the shared body text
remains the one value an override replaces wholesale.

## Inheritance rules

Every theme — built-in, stored user theme, or imported — resolves through one order in
`ThemeRegistry.resolve`, so imported themes share the same semantics with no second
implementation:

1. shared defaults: the registry default's tokens (Refined Default for the built-in registry);
2. host palette: an explicitly passed host mapping, only for the flat chrome tokens where
   terminal colors meaningfully apply (canvas, surface, panel, text, borders, focus, selection,
   status fill/text/separator, inspector chrome) — transcript, diff, and syntax tokens are never
   inherited;
3. groups: the specification's sparse per-category values, one transcript category at a time
   (`operator`, `assistant`, `reasoning`, `activity`, `session`, `fault`), each naming any of its
   three roles by nickname (`text`, `marker`, `background`) or by canonical token name;
4. overrides: the specification's own tokens, which always win.

Each layer is sparse: an empty group, an unknown category or token, a null value, or a malformed
color falls through to the next layer instead of breaking the rest, and a malformed group color is
reported in the resolution notices. A category with no group text uses the theme's shared body
text; an unknown category falls back to the plain-surface default (shared body text on the default
surface).

Readability is a floor, not a suggestion. Category body text must hold 4.5 against its fill and
stripe markers 3.0, reusing the existing contrast machinery; a combination below the floor resolves
to the shared text, the default theme's marker, or black/white — visibly noticed — rather than
rendering. Inherited host values that break the text/canvas or selection pairings revert to the
built-in mapping with a notice; explicit Talaria overrides are never reverted. An unresolvable
host palette (absent, misshapen, or empty) degrades to the built-in mapping with a notice, never a
crash or a blank theme.

## Preview and select a theme

Enter `/theme` to open the theme picker. `Up` and `Down` preview the highlighted theme immediately.
`Enter` accepts it for the current Talaria process and closes the picker. `Escape` closes the picker
and restores the theme that was active when it opened. Browsing, previewing, accepting a session
theme, and quitting do not write a file.

`/theme select <name>` previews one installed theme immediately without opening the picker, for
example `/theme select neutral-dark`. Like the picker, it accepts the theme for the current
Talaria process only. An unknown name keeps the current theme and says so:

```text
theme 'not-installed' is not available — keeping 'refined-default'
```

A theme that cannot render also keeps the current theme with a notice naming the failure.

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
talaria theme import FILE [--name NAME] [--json]
```

Search the marketplace and fetch a theme with no manual download step:

```text
talaria theme search QUERY [--limit N] [--json]
talaria theme fetch REF [--name NAME] [--json]
```

`REF` is a `publisher/extension[/theme]` reference from the Open VSX registry, a direct
`http(s)` URL to a raw theme JSON file, a GitHub file-page URL (converted to the raw file
automatically), or an Open VSX extension page (resolved as its `publisher/extension`
reference). A gallery search page or any other web page is not fetchable: run `/theme search`
first and fetch the reference it lists. The user-selected source is accepted as given; there
is no additional trust policy. Fetched bytes are parsed by the same strict entry point as
local files and are never executed. Every bound still applies after conversion: payloads past
the size bound fail as `oversized` even when they started as a page URL — a page URL fetches
the page, so use the raw file when a page is refused.
The same search, fetch, and reload steps are available inside the app as `/theme search <query>`,
`/theme fetch <source> [--name <name>]`, and `/theme reload [name]`. A search lists bounded
entries with the reference to fetch, for example:

```text
marketplace themes:
Solar Flare (acme/solar) — fetch with /theme fetch acme/solar/1
```

`--json` writes one versioned machine-readable report to standard output. Its success and error
fields are defined in the
[Visual Studio Code theme import format](formats/vscode-theme-import.md).

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
the same explicit save commands as a built-in theme.

Every successful import records its source beside the library in
`<config-dir>/theme-sources.json`, mapping each slug to its file path or marketplace reference.
`/theme reload [name]` re-runs the import pipeline for that recorded source and applies the
re-resolved theme live, with no restart:

```text
theme 'solar-flare' reloaded: 40 source tokens, 18 fallbacks, 0 warnings
```

`reload` with no name re-imports the current session theme. A failed or invalid reload keeps
rendering the current theme with a notice and never partially applies. There is no file watcher:
editing a source file changes nothing until an explicit reload, and concurrent reloads serialize
behind one lock. The mapping, warning, and report rules for both sources are in
[Visual Studio Code theme import format](formats/vscode-theme-import.md); they are not duplicated
here.
