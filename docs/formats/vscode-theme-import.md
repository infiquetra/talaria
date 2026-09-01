# Visual Studio Code color-theme import

Talaria imports one strict Visual Studio Code color-theme JSON file into one
restart-scoped user theme. This document is the public allowlist. Nothing outside
these tables is inferred, and the source file is never watched after import.

The mapping and resolution rules below are copied verbatim from the v0.5.0 visual
specification so later documentation can link to one stable format contract.

## Input and resolution rules

1. The root must be a strict JSON object. name, type, and $schema are recognized metadata. colors must be an object when present and tokenColors must be an array when present.
2. Supported source colors are #RGB, #RGBA, #RRGGBB, and #RRGGBBAA, case-insensitive. Output is normalized to opaque uppercase #RRGGBB.
3. Alpha colors are composited in sRGB against the destination's normative background from the contrast table. Background tokens composite against their enclosing canvas or panel. If that background has not mapped yet, its Refined Default value is used. The import report names every alpha composite.
4. For a Talaria token with multiple candidate keys, the first present valid key in the listed order wins. A later candidate is not a warning; it is a documented fallback source.
5. Workbench colors map first, then tokenColors rules. tokenColors only writes syntax tokens.
6. Every Talaria token still missing after mapping is copied from Refined Default. Each copied token is reported by canonical name.
7. Invalid JSON, a wrong root shape, invalid colors/tokenColors types, or a file with neither a usable supported color nor a usable supported scope fails before any file is written. Re-import with the same chosen name deterministically replaces the stored user theme only after full validation.

## Supported workbench colors

| Talaria token | Visual Studio Code colors key, in precedence order | Notes |
|---|---|---|
| talaria.canvas | editor.background | Required source for an imported canvas; otherwise fallback |
| talaria.surface | editorWidget.background; input.background | Raised/control surface |
| talaria.panel | panel.background | Secondary panel surface |
| talaria.text | editor.foreground | Also supplies Textual foreground |
| talaria.text.muted | descriptionForeground; input.placeholderForeground | Metadata/hints |
| talaria.primary | textLink.foreground | Primary action/link |
| talaria.accent | activityBarBadge.background; button.background | Active selection/activity |
| talaria.success | testing.iconPassed; gitDecoration.addedResourceForeground | Successful state |
| talaria.warning | editorWarning.foreground; notificationsWarningIcon.foreground | Pending/degraded state |
| talaria.error | errorForeground; editorError.foreground | Error state |
| talaria.border | contrastBorder; widget.border | Strong boundary |
| talaria.border.muted | editorGroup.border | Inactive boundary |
| talaria.focus | focusBorder | Focus/caret color |
| talaria.selection.background | editor.selectionBackground | Selection fill |
| talaria.selection.text | editor.selectionForeground | Selection/cursor text |
| talaria.status.background | statusBar.background | Bottom bar fill |
| talaria.status.text | statusBar.foreground | Bottom bar primary text |
| talaria.status.separator | statusBar.border | Segment separator |
| talaria.inspector.background | sideBar.background | Inspector fill |
| talaria.inspector.border | sideBar.border | Inspector edge |
| talaria.inspector.heading | sideBarTitle.foreground; sideBarSectionHeader.foreground | Headings/selection |
| talaria.diff.context | editor.foreground | Unchanged line text |
| talaria.diff.line-number | editorLineNumber.foreground | Both modes |
| talaria.diff.added | gitDecoration.addedResourceForeground | Added glyph/default foreground |
| talaria.diff.added.background | diffEditor.insertedLineBackground | Added line fill |
| talaria.diff.removed | gitDecoration.deletedResourceForeground | Removed glyph/default foreground |
| talaria.diff.removed.background | diffEditor.removedLineBackground | Removed line fill |
| talaria.diff.hunk | editorInfo.foreground | Hunk text |
| talaria.diff.hunk.background | editor.lineHighlightBackground | Hunk row fill |
| talaria.diff.intraline-added.background | diffEditor.insertedTextBackground | Changed added span |
| talaria.diff.intraline-removed.background | diffEditor.removedTextBackground | Changed removed span |

## Supported tokenColors scopes

Only settings.foreground is consumed. scope may be one string, a comma-separated string, or an array of strings. A simple scope matches itself and descendants, so comment matches comment.line. Compound selectors, exclusions, priorities, and wildcards are unsupported.

When scopes overlap, the longest supported scope prefix wins; a later rule wins ties. This makes constant.numeric map to number rather than the broader constant token. One rule naming several supported scopes applies its foreground to every mapped Talaria token.

| Talaria token | Supported TextMate scope prefixes |
|---|---|
| talaria.syntax.comment | comment; punctuation.definition.comment |
| talaria.syntax.keyword | keyword; keyword.control; storage.modifier |
| talaria.syntax.string | string; string.quoted; string.template |
| talaria.syntax.number | constant.numeric |
| talaria.syntax.function | entity.name.function; support.function |
| talaria.syntax.type | entity.name.type; entity.name.class; support.type; storage.type |
| talaria.syntax.variable | variable; variable.other; variable.parameter |
| talaria.syntax.operator | keyword.operator; punctuation.separator; punctuation.accessor |
| talaria.syntax.constant | constant; constant.language; support.constant |

## Deliberately unsupported input

The importer MUST issue one stable, path-qualified warning for every occurrence of the following:

| Unsupported input | Required warning behavior |
|---|---|
| Any colors member not listed in the workbench table | Report colors.KEY as unsupported; do not approximate it |
| include | Report that external theme chaining is not read; do not open another file |
| semanticHighlighting and semanticTokenColors | Report semantic-token theming as unsupported |
| tokenColors supplied as a path string or external .tmTheme reference | Report external TextMate files as unsupported |
| tokenColors rule with an unscoped default | Report the rule index; do not treat it as editor foreground |
| TextMate compound/exclusion/wildcard selector | Report the rule index and selector |
| tokenColors settings.background | Report and ignore; diff/surface backgrounds remain semantic tokens |
| Non-empty tokenColors settings.fontStyle | Report and ignore; bold/italic/underline are not color-token imports |
| iconThemes, productIconTheme, file icon colors, terminal ANSI palette, bracket-pair colors, minimap, overview ruler, charts, debug colors, and git state keys not explicitly listed | Report each encountered root/key path as unsupported |
| Invalid color literal, non-string color value, or unsupported color syntax | Report the exact path as invalid; if no usable mapping remains, fail without writing |
| Unknown root property | Report the root property as unsupported |

$schema, name, and type are metadata and do not warn. type may set the imported theme's Textual dark flag when its value is exactly light, dark, or hc; other values warn and use Refined Default's light/dark behavior.

## Tokens with no Visual Studio Code source

These eighteen Talaria extension tokens have no entry in either supported mapping and therefore always come from Refined Default during import:

| Fallback-only token | Reason |
|---|---|
| talaria.secondary | No bounded workbench key has the same semantic role |
| talaria.status.muted | Visual Studio Code exposes status foreground but no secondary status text role |
| talaria.status.success | Visual Studio Code has no bounded per-state status-bar colour role |
| talaria.status.warning | Visual Studio Code has no bounded per-state status-bar colour role |
| talaria.status.error | Visual Studio Code has no bounded per-state status-bar colour role |
| talaria.status.attention | Visual Studio Code has no bounded per-state status-bar colour role |
| talaria.transcript.operator | Talaria-specific transcript channel |
| talaria.transcript.operator.background | Talaria-specific transcript channel |
| talaria.transcript.assistant | Talaria-specific transcript channel |
| talaria.transcript.assistant.background | Talaria-specific transcript channel |
| talaria.transcript.reasoning | Talaria-specific transcript channel |
| talaria.transcript.reasoning.background | Talaria-specific transcript channel |
| talaria.transcript.activity | Talaria-specific transcript channel |
| talaria.transcript.activity.background | Talaria-specific transcript channel |
| talaria.transcript.session | Talaria-specific transcript channel |
| talaria.transcript.session.background | Talaria-specific transcript channel |
| talaria.transcript.fault | Talaria-specific transcript channel |
| talaria.transcript.fault.background | Talaria-specific transcript channel |

Any other token falls back only when its listed source is absent or invalid. The successful import report states source-mapped count, fallback count, unsupported count, and every affected path/token. Successful imports with warnings exit successfully but visibly print the report. A failed import never writes a stored theme; its command-line report is described below.

## Command-line reports and exit status

The default report remains prose: informational lines use standard output and
warnings use standard error. `talaria theme import FILE --json` instead writes
one JSON object to standard output and writes nothing to standard error.

On success, `schema_version` is `talaria-theme-import-report-v1`. The object
also carries `slug`, `target_path`, `source_token_count`, `fallback_count`,
`warning_count`, and ordered `composites`, `fallbacks`, and `warnings` arrays.
Each composite record carries `severity`, `path`, `token`, `source`,
`background`, and the flattened `value`. Each fallback and warning record also
has an explicit `severity`, so consumers never infer routing from English text.

On failure, `schema_version` is `talaria-theme-import-error-v1`. The object
carries the stable `kind` and a human-readable `message`; the exit status still
uses the table below. The `kind` vocabulary is `unreadable`, `empty`,
`malformed`, `wrong-root`, `reserved-slug`, `invalid-slug`, and `unwritable`.
These seven values distinguish the documented causes even when several share
one exit status.

| Exit status | Meaning |
|---|---|
| 0 | Import completed; warnings, if any, are present in the report |
| 2 | Command-line usage error |
| 3 | Source file unreadable, empty, malformed, or structurally unsupported |
| 4 | Requested storage slug invalid or reserved |
| 5 | Validated theme could not be written |

## Stored user-theme format

The selected storage name uses this fixed precedence: command-line `--name`,
top-level `name`, then source-file stem. It must already be a lowercase
hyphenated slug with no separators or traversal. Built-in theme slugs are
reserved.

Talaria writes `<config>/themes/<slug>.json` only after parsing and reporting
complete successfully. The stored object contains `schema_version` with the
value `talaria-theme-v1`, `dark`, `name`, `slug`, and the complete `tokens`
mapping. The normative schema is
[`stored-theme.schema.json`](stored-theme.schema.json). Keys are sorted, values
are opaque uppercase `#RRGGBB`, and the file has exactly one trailing newline.
A later import of the same slug atomically replaces that path. If the path is a
symlink, Talaria replaces the link itself and never writes through to its target.
Imported themes are read only when a new Talaria process constructs its theme registry;
source-file changes are not watched.

During v0.5.0, the reader treats a stored theme without `schema_version` as
version one. The field becomes required in v0.6.0. A file carrying any other
version, or any other invalid stored document, is skipped with a visible
notice; it cannot prevent valid themes or the application from loading.
