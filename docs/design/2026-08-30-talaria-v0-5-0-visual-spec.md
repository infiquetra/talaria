# Talaria v0.5.0 integrated visual specification

Date: 2026-08-30

Status: implementation specification

Contract: [#103](https://github.com/infiquetra/talaria/issues/103), [#104](https://github.com/infiquetra/talaria/issues/104), [#105](https://github.com/infiquetra/talaria/issues/105), [#106](https://github.com/infiquetra/talaria/issues/106), [#107](https://github.com/infiquetra/talaria/issues/107), [#108](https://github.com/infiquetra/talaria/issues/108), and [#109](https://github.com/infiquetra/talaria/issues/109)

This document is the integrated visual contract for the Talaria v0.5.0 implementation lanes. Normative words such as MUST, MUST NOT, SHOULD, and MAY have their ordinary requirements meaning.

## Decisions taken

This specification takes the following defensible choices from the sets left open by the approved issues:

1. The existing HelpBar remains a fixed, one-row key-hint strip immediately above the new status bar. The new BottomStatusBar is the only widget on the terminal's last row.
2. BottomStatusBar replaces the existing reserved NeedsYouBar row. Its task_progress segment carries the same queue summary and its palette drill-down remains available. The flexible StatusRegion stays inside the body and retains its timer, row cap, literal rendering, seam rows, and visible failure marker. The screen therefore keeps two fixed footer rows before and after this release instead of gaining a third.
3. Refined Default is a light theme. Dark Green Terminal, Neutral Dark, and Accessible High Contrast are dark themes.
4. The inspector docks only at 120 columns or wider. Its saved session width is 36 columns by default, clamps to 28–48 columns, and changes in four-column keyboard steps.
5. The diff viewer defaults to side-by-side at 112 columns or wider and falls back to unified at 111 columns or narrower. Opening a diff temporarily collapses a docked inspector so the threshold depends on the terminal, not on leftover content width.
6. Theme browsing previews immediately. Escape restores the pre-picker theme; Enter selects only for the current session. A separate explicit save action writes the user scope by default or the repository scope when deliberately chosen.
7. All four built-ins target Web Content Accessibility Guidelines (WCAG) 2.2 contrast thresholds, not only Accessible High Contrast. Accessible High Contrast has additional separation and its lowest measured text ratio is 6.22:1.
8. Focus is shown by color and a fixed-position glyph in space already allocated to the widget. Focus never mounts, unmounts, expands, contracts, or wraps a row.

## Existing interface inventory

### Current screen geometry

TalariaApp.compose currently mounts these surfaces in order:

~~~text
#body (height: 1fr)
  TranscriptPane
  AgentRows
  PromptRegion
  PaletteRegion
  StatusRegion
Composer
NeedsYouBar  (fixed height: 1)
HelpBar      (fixed height: 1; current last row)
~~~

The Composer is already height-aware: its border is always mounted, its container is auto-height with a 12-row maximum, its ChatTextArea has an 8-row maximum, and its notice reserves one row. NeedsYouBar and HelpBar are both explicitly one row, no-wrap, and ellipsized. Transcript kind markers already reserve a gutter column so adding a border never changes wrap width. These are existing invariants, not opportunities for redesign.

The v0.5.0 composition is:

~~~text
MainAndInspector (height: 1fr)
  #body
    TranscriptPane
    AgentRows
    PromptRegion
    PaletteRegion
    StatusRegion     unchanged flexible shell-command/seam output
  Inspector          docked only when eligible
Composer             existing height behavior retained
HelpBar              fixed adjacent row
BottomStatusBar      fixed height: 1; only true last row
~~~

Implementation may arrange MainAndInspector around the existing body rather than literally nesting it as drawn, but the resulting order and geometry are normative.

### Real Textual variable use

The current source uses the following Textual variables. Dynamic transcript rules are included; they do not appear as literal CSS rows in a simple search.

| Current variable | Current use |
|---|---|
| $accent | Composer and picker-dialog borders; active palette/dialog rows; focused interruptible agent rows and focused prompt-card tint |
| $warning | Composer notice; degraded palette notice; StatusRegion failure/truncation marker; prompt borders/activity; confirmation dialog border/title/refusal |
| $error | Blocked prompt text and transcript fault group |
| $primary | Transcript operator group border and 10% background tint |
| $success | Transcript assistant group border and 10% background tint |
| $secondary | Transcript reasoning group border and 10% background tint |
| $panel | Transcript session-record group border and 10% background tint |
| $surface | Picker and confirmation dialog backgrounds |
| $text | Palette rows, command panels, dialog bodies, and dialog rows |
| $text-muted | HelpBar, NeedsYouBar, StatusRegion body, agent header, condensed transcript notice, palette header/hints, prompt hints/activity, and dialog hints |
| $text-warning | Oversized-entry transcript fallback banner |

TranscriptPane currently maps its twelve entry kinds into six visual groups:

| Group | Entry kinds | Current source variable |
|---|---|---|
| operator | user | $primary |
| assistant | assistant | $success |
| reasoning | reasoning | $secondary |
| activity | tool, subagent | $accent |
| session | system, prompt, prompt-expired, cancelled | $panel |
| fault | error, protocol-error, unknown-event | $error |

The v0.5.0 token layer preserves those meanings while ending the accidental coupling between, for example, an active palette row and a tool transcript marker.

## Theme token vocabulary

### Naming and Textual bridge

- Talaria's canonical public names use the talaria.category.name form below.
- Every canonical token also exists as a Textual custom variable formed by replacing dots with hyphens and prefixing a dollar sign. For example, talaria.diff.added.background is $talaria-diff-added-background.
- Foundation tokens additionally populate Textual Theme fields and compatibility variables so existing widgets continue to work without literal colors.
- Each built-in Theme sets text_alpha to 1.0 and ansi to false. Refined Default sets dark to false; the other three set dark to true.
- Textual 8.2.8 derives text, colored-text, cursor, selection, and blurred-border colors unless variables override them. Talaria MUST supply the explicit overrides in this table so the runtime values equal the measured hex values.
- Runtime theme dictionaries contain opaque uppercase #RRGGBB values only. Import may accept supported alpha syntax, but must composite it before registration.
- UI code MUST consume these tokens. A literal color in widget CSS is a specification failure.

### Complete registry

| Talaria token | Semantic role | Textual Theme field or variable bridge |
|---|---|---|
| talaria.canvas | Application and transcript canvas | Theme.background; $background; $talaria-canvas |
| talaria.surface | Raised controls, dialogs, and code-block surface | Theme.surface; $surface; $talaria-surface |
| talaria.panel | Secondary grouping surface | Theme.panel; $panel; $talaria-panel |
| talaria.text | Primary body text | Theme.foreground; $foreground; explicit $text; $talaria-text |
| talaria.text.muted | Secondary, hint, and metadata text | explicit $text-muted and $foreground-muted; $talaria-text-muted |
| talaria.primary | Primary product action and operator identity | Theme.primary; $primary; explicit $text-primary; $talaria-primary |
| talaria.secondary | Secondary action and reasoning identity | Theme.secondary; $secondary; explicit $text-secondary; $talaria-secondary |
| talaria.accent | Active selection and transient activity | Theme.accent; $accent; explicit $text-accent; $talaria-accent |
| talaria.success | Successful or completed state | Theme.success; $success; explicit $text-success; $talaria-success |
| talaria.warning | Attention, pending, and degraded state | Theme.warning; $warning; explicit $text-warning; $talaria-warning |
| talaria.error | Error, failed, and destructive warning state | Theme.error; $error; explicit $text-error; $talaria-error |
| talaria.border | Default strong border | explicit $border; $talaria-border |
| talaria.border.muted | Inactive and low-emphasis border | $border-blurred and $block-cursor-blurred-background; $talaria-border-muted |
| talaria.focus | Focus ring, active caret, and focused cursor | $block-cursor-background and $input-cursor-background; $talaria-focus |
| talaria.selection.background | Selected text cell background | $input-selection-background and $screen-selection-background; $talaria-selection-background |
| talaria.selection.text | Text over selection and cursor fills | $input-selection-foreground, $screen-selection-foreground, $block-cursor-foreground, and $input-cursor-foreground; $talaria-selection-text |
| talaria.status.background | True-bottom status bar fill | $talaria-status-background |
| talaria.status.text | Primary status segment text | $talaria-status-text |
| talaria.status.muted | Secondary status text | $talaria-status-muted |
| talaria.status.separator | One-column segment separator | $talaria-status-separator |
| talaria.status.success | Connected and successful-state text in the bottom status bar | $talaria-status-success |
| talaria.status.warning | Connecting and reconnecting state text in the bottom status bar | $talaria-status-warning |
| talaria.status.error | Disconnected and authentication-failed state text in the bottom status bar | $talaria-status-error |
| talaria.status.attention | Queue-attention `!N` marker in the bottom status bar | $talaria-status-attention |
| talaria.inspector.background | Docked or overlay inspector fill | $talaria-inspector-background |
| talaria.inspector.border | Inspector boundary | $talaria-inspector-border |
| talaria.inspector.heading | Inspector section headings and selected file | $talaria-inspector-heading |
| talaria.transcript.operator | Operator gutter marker | $talaria-transcript-operator |
| talaria.transcript.operator.background | Operator entry tint | $talaria-transcript-operator-background |
| talaria.transcript.assistant | Assistant gutter marker | $talaria-transcript-assistant |
| talaria.transcript.assistant.background | Assistant entry tint | $talaria-transcript-assistant-background |
| talaria.transcript.reasoning | Reasoning gutter marker | $talaria-transcript-reasoning |
| talaria.transcript.reasoning.background | Reasoning entry tint | $talaria-transcript-reasoning-background |
| talaria.transcript.activity | Tool and subagent gutter marker | $talaria-transcript-activity |
| talaria.transcript.activity.background | Tool and subagent entry tint | $talaria-transcript-activity-background |
| talaria.transcript.session | System/prompt/cancellation gutter marker | $talaria-transcript-session |
| talaria.transcript.session.background | System/prompt/cancellation entry tint | $talaria-transcript-session-background |
| talaria.transcript.fault | Error/protocol/unknown gutter marker | $talaria-transcript-fault |
| talaria.transcript.fault.background | Error/protocol/unknown entry tint | $talaria-transcript-fault-background |
| talaria.diff.context | Unchanged diff text | $talaria-diff-context |
| talaria.diff.line-number | Diff line numbers and omitted-line counts | $talaria-diff-line-number |
| talaria.diff.added | Added-line glyph and default foreground | $talaria-diff-added |
| talaria.diff.added.background | Added-line fill | $talaria-diff-added-background |
| talaria.diff.removed | Removed-line glyph and default foreground | $talaria-diff-removed |
| talaria.diff.removed.background | Removed-line fill | $talaria-diff-removed-background |
| talaria.diff.hunk | Hunk header and navigation marker | $talaria-diff-hunk |
| talaria.diff.hunk.background | Hunk header fill | $talaria-diff-hunk-background |
| talaria.diff.intraline-added.background | Changed span inside an added line | $talaria-diff-intraline-added-background |
| talaria.diff.intraline-removed.background | Changed span inside a removed line | $talaria-diff-intraline-removed-background |
| talaria.syntax.comment | Comments and documentation punctuation | $talaria-syntax-comment |
| talaria.syntax.keyword | Language keywords and storage modifiers | $talaria-syntax-keyword |
| talaria.syntax.string | Strings and templates | $talaria-syntax-string |
| talaria.syntax.number | Numeric constants | $talaria-syntax-number |
| talaria.syntax.function | Function names and callable support symbols | $talaria-syntax-function |
| talaria.syntax.type | Types, classes, and storage types | $talaria-syntax-type |
| talaria.syntax.variable | Variables and parameters | $talaria-syntax-variable |
| talaria.syntax.operator | Operators and structural access punctuation | $talaria-syntax-operator |
| talaria.syntax.constant | Language constants and other constants | $talaria-syntax-constant |

For inactive block cursors, $block-cursor-blurred-foreground MUST use talaria.text. Textual style variables remain unchanged unless named above.

## Four built-in themes

### Exact token values

| Token | Refined Default | Dark Green Terminal | Neutral Dark | Accessible High Contrast |
|---|---|---|---|---|
| talaria.canvas | #F6F8FA | #07110B | #151719 | #000000 |
| talaria.surface | #FFFFFF | #0B1A10 | #1D2023 | #0A0A0A |
| talaria.panel | #EAEFF4 | #102418 | #25292D | #141414 |
| talaria.text | #1F2328 | #D6F5DC | #E6E9EC | #FFFFFF |
| talaria.text.muted | #57606A | #8FB99A | #A6ADB4 | #D6D6D6 |
| talaria.primary | #0969DA | #6EE7A0 | #8AB4F8 | #66B3FF |
| talaria.secondary | #6F42C1 | #9BD4A7 | #C4A7E7 | #D7A9FF |
| talaria.accent | #087F5B | #39FF88 | #9AB7D3 | #00FF85 |
| talaria.success | #1A7F37 | #6EE7A0 | #82C99A | #63FF90 |
| talaria.warning | #8A5A00 | #FFD166 | #E4C07A | #FFD75F |
| talaria.error | #CF222E | #FF7B72 | #F08C8C | #FF6B6B |
| talaria.border | #6E7781 | #4BAA6A | #7A838C | #FFFFFF |
| talaria.border.muted | #7D8590 | #337A4B | #707983 | #8A8A8A |
| talaria.focus | #0969DA | #39FF88 | #B3C7DB | #00FFFF |
| talaria.selection.background | #0969DA | #39FF88 | #B3C7DB | #FFFF00 |
| talaria.selection.text | #FFFFFF | #041008 | #101214 | #000000 |
| talaria.status.background | #24292F | #020703 | #0D0F10 | #000000 |
| talaria.status.text | #F6F8FA | #D6F5DC | #E6E9EC | #FFFFFF |
| talaria.status.muted | #C6CDD5 | #8FB99A | #A6ADB4 | #D6D6D6 |
| talaria.status.separator | #8C959F | #4BAA6A | #68717A | #FFFFFF |
| talaria.status.success | #3FB950 | #6EE7A0 | #82C99A | #63FF90 |
| talaria.status.warning | #D29922 | #FFD166 | #E4C07A | #FFD75F |
| talaria.status.error | #FF7B72 | #FF7B72 | #F08C8C | #FF6B6B |
| talaria.status.attention | #58A6FF | #39FF88 | #9AB7D3 | #00FF85 |
| talaria.inspector.background | #FFFFFF | #0B1A10 | #1D2023 | #0A0A0A |
| talaria.inspector.border | #6E7781 | #4BAA6A | #68717A | #FFFFFF |
| talaria.inspector.heading | #0969DA | #6EE7A0 | #B3C7DB | #00FFFF |
| talaria.transcript.operator | #0969DA | #7EE7A5 | #8AB4F8 | #66B3FF |
| talaria.transcript.operator.background | #EAF2FC | #0E2216 | #1A2430 | #00172B |
| talaria.transcript.assistant | #1A7F37 | #6EE7A0 | #82C99A | #63FF90 |
| talaria.transcript.assistant.background | #EAF7ED | #0D2617 | #1A2920 | #00210C |
| talaria.transcript.reasoning | #6F42C1 | #B4D6A0 | #C4A7E7 | #D7A9FF |
| talaria.transcript.reasoning.background | #F1EBFC | #162518 | #282133 | #1D0B2B |
| talaria.transcript.activity | #0B7285 | #72D6B1 | #8EC5C5 | #5CFFF1 |
| talaria.transcript.activity.background | #E7F5F7 | #0E241B | #1B2929 | #002522 |
| talaria.transcript.session | #57606A | #A8C5AE | #A6ADB4 | #E0E0E0 |
| talaria.transcript.session.background | #EEF1F4 | #142018 | #24272A | #1A1A1A |
| talaria.transcript.fault | #CF222E | #FF8A80 | #F08C8C | #FF7B7B |
| talaria.transcript.fault.background | #FDEDEF | #281313 | #321D1D | #2A0000 |
| talaria.diff.context | #1F2328 | #D6F5DC | #E6E9EC | #FFFFFF |
| talaria.diff.line-number | #57606A | #8FB99A | #A6ADB4 | #D6D6D6 |
| talaria.diff.added | #116329 | #8BF0A8 | #9AD5AA | #9BFFB5 |
| talaria.diff.added.background | #E6F4EA | #0E2A18 | #1B2B20 | #003313 |
| talaria.diff.removed | #A40E26 | #FF9A91 | #F2A0A0 | #FF9A9A |
| talaria.diff.removed.background | #FCE8E6 | #2A1412 | #321F1F | #3A0000 |
| talaria.diff.hunk | #0550AE | #8FD3FF | #9EC5F8 | #8FD3FF |
| talaria.diff.hunk.background | #EAF2FC | #102631 | #1B2735 | #002744 |
| talaria.diff.intraline-added.background | #BFE3C8 | #1B5A31 | #285137 | #005C24 |
| talaria.diff.intraline-removed.background | #F4C7C3 | #6B2520 | #643333 | #6E0000 |
| talaria.syntax.comment | #57606A | #8FB99A | #A6ADB4 | #D6D6D6 |
| talaria.syntax.keyword | #6F42C1 | #B6F09C | #C4A7E7 | #D7A9FF |
| talaria.syntax.string | #116329 | #8BE9A8 | #9AD5AA | #9BFFB5 |
| talaria.syntax.number | #953800 | #FFD166 | #E4C07A | #FFD75F |
| talaria.syntax.function | #0550AE | #7DD3FC | #8AB4F8 | #8FD3FF |
| talaria.syntax.type | #0B7285 | #A7E3C1 | #8EC5C5 | #5CFFF1 |
| talaria.syntax.variable | #1F2328 | #D6F5DC | #E6E9EC | #FFFFFF |
| talaria.syntax.operator | #6F42C1 | #B4D6A0 | #B9C0C7 | #F2F2F2 |
| talaria.syntax.constant | #A40E4C | #FFB86C | #F2B37C | #FFB86B |

### Application and persistence behavior

The theme picker is a mode in Talaria's existing PaletteRegion:

1. Invoking /theme captures the current theme as the restore point.
2. Moving the active row previews the highlighted theme immediately.
3. Escape closes the picker and restores the captured theme.
4. Enter closes the picker and records the selection in the in-memory session scope only.
5. /theme save writes that selection to the user [theme] table. Choosing the explicit repository target writes only ./.talaria/config.toml's [theme] table.
6. Merely browsing, previewing, pressing Enter, or quitting MUST NOT write a file.

Resolution order is built-in default, then user config, then repository config, then unsaved session selection. Each later scope overrides the earlier. An unknown named theme resolves to Refined Default and emits a visible startup notice. A partial imported theme fills each missing token from Refined Default and lists every filled canonical token in a visible import/startup warning.

## Contrast contract and measurements

Ratios use the [WCAG 2.2 relative-luminance and contrast formula](https://www.w3.org/TR/WCAG22/#contrast-minimum) on the opaque sRGB values above. Talaria treats all terminal text as normal-size text: the minimum is 4.5:1. Required boundaries, focus indicators, and other non-text components use the [3:1 non-text contrast threshold](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html). No terminal font-size exemption is assumed.

The tables below are exhaustive for allowed foreground/background combinations:

- Normal transcript prose uses talaria.text on its six group backgrounds. A thick group marker supplies the group token.
- Transcript code blocks use the opaque talaria.surface behind syntax colors; syntax colors do not sit directly on group tints.
- Diff syntax uses canvas, added-line, or removed-line backgrounds. Inside an intraline changed span, diff added/removed foreground replaces syntax foreground.
- The added, removed, and hunk fills are redundant decoration because each line also has a +, -, or @@ text marker. Their required colored foreground/component marker is nevertheless measured against each fill.
- Bottom-bar connection state and queue attention use talaria.status.success, talaria.status.warning, talaria.status.error, and talaria.status.attention only on talaria.status.background. The automated contrast test MUST cover all four pairs for every built-in theme.
- Introducing any other pair requires adding it to this table and to the automated contrast test before release.

### Text and glyph contrast measurements

| Pair | Refined Default | Dark Green Terminal | Neutral Dark | Accessible High Contrast |
|---|---:|---:|---:|---:|
| body / canvas | 14.84:1 | 16.41:1 | 14.75:1 | 21.00:1 |
| body / surface | 15.80:1 | 15.35:1 | 13.43:1 | 19.80:1 |
| body / panel | 13.65:1 | 13.94:1 | 12.02:1 | 18.42:1 |
| muted / canvas | 6.00:1 | 8.75:1 | 7.92:1 | 14.45:1 |
| muted / surface | 6.39:1 | 8.19:1 | 7.22:1 | 13.62:1 |
| muted / panel | 5.52:1 | 7.44:1 | 6.46:1 | 12.68:1 |
| primary / canvas | 4.88:1 | 12.43:1 | 8.53:1 | 9.46:1 |
| secondary / canvas | 6.12:1 | 11.31:1 | 8.58:1 | 11.01:1 |
| accent / canvas | 4.70:1 | 14.47:1 | 8.63:1 | 15.64:1 |
| success / canvas | 4.77:1 | 12.43:1 | 9.22:1 | 16.24:1 |
| warning / canvas | 5.57:1 | 13.31:1 | 10.37:1 | 15.14:1 |
| error / canvas | 5.03:1 | 7.61:1 | 7.56:1 | 7.57:1 |
| selection text / selection | 5.19:1 | 14.62:1 | 10.82:1 | 19.56:1 |
| cursor text / focus | 5.19:1 | 14.62:1 | 10.82:1 | 16.75:1 |
| status text / status | 13.76:1 | 17.36:1 | 15.77:1 | 21.00:1 |
| status muted / status | 9.14:1 | 9.26:1 | 8.47:1 | 14.45:1 |
| status success / status | 5.77:1 | 13.15:1 | 9.85:1 | 16.24:1 |
| status warning / status | 5.80:1 | 14.08:1 | 11.09:1 | 15.14:1 |
| status error / status | 5.81:1 | 8.05:1 | 8.08:1 | 7.57:1 |
| status attention / status | 5.80:1 | 15.31:1 | 9.23:1 | 15.64:1 |
| inspector body / inspector | 15.80:1 | 15.35:1 | 13.43:1 | 19.80:1 |
| inspector heading / inspector | 5.19:1 | 11.63:1 | 9.44:1 | 15.79:1 |
| transcript body / operator | 14.00:1 | 14.26:1 | 12.87:1 | 18.14:1 |
| transcript body / assistant | 14.31:1 | 13.74:1 | 12.47:1 | 17.17:1 |
| transcript body / reasoning | 13.56:1 | 13.68:1 | 12.71:1 | 18.50:1 |
| transcript body / activity | 14.14:1 | 13.95:1 | 12.35:1 | 16.31:1 |
| transcript body / session | 13.93:1 | 14.37:1 | 12.32:1 | 17.40:1 |
| transcript body / fault | 13.94:1 | 15.05:1 | 12.96:1 | 19.12:1 |
| diff context / canvas | 14.84:1 | 16.41:1 | 14.75:1 | 21.00:1 |
| diff line number / canvas | 6.00:1 | 8.75:1 | 7.92:1 | 14.45:1 |
| diff added / added line | 6.51:1 | 11.10:1 | 8.83:1 | 11.71:1 |
| diff removed / removed line | 6.68:1 | 8.52:1 | 7.63:1 | 8.76:1 |
| diff hunk / hunk line | 6.73:1 | 9.62:1 | 8.50:1 | 9.41:1 |
| diff added / intraline | 5.29:1 | 5.92:1 | 5.37:1 | 6.79:1 |
| diff removed / intraline | 5.17:1 | 5.38:1 | 5.00:1 | 6.22:1 |
| syntax comment / surface | 6.39:1 | 8.19:1 | 7.22:1 | 13.62:1 |
| syntax keyword / surface | 6.51:1 | 13.62:1 | 7.82:1 | 10.38:1 |
| syntax string / surface | 7.39:1 | 12.24:1 | 9.72:1 | 16.37:1 |
| syntax number / surface | 7.39:1 | 12.45:1 | 9.45:1 | 14.27:1 |
| syntax function / surface | 7.59:1 | 10.77:1 | 7.77:1 | 12.19:1 |
| syntax type / surface | 5.59:1 | 12.31:1 | 8.53:1 | 16.05:1 |
| syntax variable / surface | 15.80:1 | 15.35:1 | 13.43:1 | 19.80:1 |
| syntax operator / surface | 6.51:1 | 11.17:1 | 8.91:1 | 17.68:1 |
| syntax constant / surface | 7.65:1 | 10.54:1 | 8.97:1 | 11.62:1 |
| syntax comment / canvas | 6.00:1 | 8.75:1 | 7.92:1 | 14.45:1 |
| syntax keyword / canvas | 6.12:1 | 14.56:1 | 8.58:1 | 11.01:1 |
| syntax string / canvas | 6.94:1 | 13.09:1 | 10.67:1 | 17.36:1 |
| syntax number / canvas | 6.94:1 | 13.31:1 | 10.37:1 | 15.14:1 |
| syntax function / canvas | 7.13:1 | 11.51:1 | 8.53:1 | 12.93:1 |
| syntax type / canvas | 5.25:1 | 13.16:1 | 9.37:1 | 17.03:1 |
| syntax variable / canvas | 14.84:1 | 16.41:1 | 14.75:1 | 21.00:1 |
| syntax operator / canvas | 6.12:1 | 11.94:1 | 9.78:1 | 18.76:1 |
| syntax constant / canvas | 7.18:1 | 11.27:1 | 9.85:1 | 12.32:1 |
| syntax comment / added line | 5.63:1 | 7.02:1 | 6.55:1 | 9.74:1 |
| syntax keyword / added line | 5.73:1 | 11.69:1 | 7.10:1 | 7.42:1 |
| syntax string / added line | 6.51:1 | 10.50:1 | 8.83:1 | 11.71:1 |
| syntax number / added line | 6.50:1 | 10.68:1 | 8.58:1 | 10.21:1 |
| syntax function / added line | 6.69:1 | 9.24:1 | 7.05:1 | 8.72:1 |
| syntax type / added line | 4.92:1 | 10.57:1 | 7.74:1 | 11.48:1 |
| syntax variable / added line | 13.91:1 | 13.17:1 | 12.19:1 | 14.16:1 |
| syntax operator / added line | 5.73:1 | 9.59:1 | 8.09:1 | 12.65:1 |
| syntax constant / added line | 6.74:1 | 9.04:1 | 8.15:1 | 8.31:1 |
| syntax comment / removed line | 5.42:1 | 7.93:1 | 6.85:1 | 12.25:1 |
| syntax keyword / removed line | 5.53:1 | 13.20:1 | 7.42:1 | 9.33:1 |
| syntax string / removed line | 6.27:1 | 11.86:1 | 9.23:1 | 14.72:1 |
| syntax number / removed line | 6.27:1 | 12.06:1 | 8.97:1 | 12.83:1 |
| syntax function / removed line | 6.45:1 | 10.43:1 | 7.37:1 | 10.96:1 |
| syntax type / removed line | 4.74:1 | 11.93:1 | 8.10:1 | 14.43:1 |
| syntax variable / removed line | 13.41:1 | 14.87:1 | 12.75:1 | 17.80:1 |
| syntax operator / removed line | 5.53:1 | 10.82:1 | 8.46:1 | 15.90:1 |
| syntax constant / removed line | 6.49:1 | 10.21:1 | 8.52:1 | 10.44:1 |
| **Minimum** | **4.70:1** | **5.38:1** | **5.00:1** | **6.22:1** |

### Non-text component contrast measurements

| Pair | Refined Default | Dark Green Terminal | Neutral Dark | Accessible High Contrast |
|---|---:|---:|---:|---:|
| border / surface | 4.55:1 | 6.21:1 | 4.25:1 | 19.80:1 |
| muted border / canvas | 3.50:1 | 3.68:1 | 4.07:1 | 6.08:1 |
| focus / canvas | 4.88:1 | 14.47:1 | 10.36:1 | 16.75:1 |
| selection / canvas | 4.88:1 | 14.47:1 | 10.36:1 | 19.56:1 |
| status separator / status | 4.82:1 | 7.02:1 | 3.87:1 | 21.00:1 |
| inspector border / canvas | 4.27:1 | 6.63:1 | 3.62:1 | 21.00:1 |
| operator marker / operator fill | 4.60:1 | 11.01:1 | 7.44:1 | 8.17:1 |
| assistant marker / assistant fill | 4.60:1 | 10.41:1 | 7.79:1 | 13.28:1 |
| reasoning marker / reasoning fill | 5.59:1 | 9.96:1 | 7.40:1 | 9.70:1 |
| activity marker / activity fill | 5.00:1 | 9.30:1 | 7.84:1 | 13.22:1 |
| session marker / session fill | 5.64:1 | 9.01:1 | 6.62:1 | 13.18:1 |
| fault marker / fault fill | 4.73:1 | 7.71:1 | 6.65:1 | 7.62:1 |
| added marker / added line | 6.51:1 | 11.10:1 | 8.83:1 | 11.71:1 |
| removed marker / removed line | 6.68:1 | 8.52:1 | 7.63:1 | 8.76:1 |
| hunk marker / hunk line | 6.73:1 | 9.62:1 | 8.50:1 | 9.41:1 |
| added marker / intraline | 5.29:1 | 5.92:1 | 5.37:1 | 6.79:1 |
| removed marker / intraline | 5.17:1 | 5.38:1 | 5.00:1 | 6.22:1 |
| **Minimum** | **3.50:1** | **3.68:1** | **3.62:1** | **6.08:1** |

All measured text pairs meet or exceed 4.5:1. All required non-text pairs meet or exceed 3:1. Accessible High Contrast therefore exceeds WCAG AA for every normative text and component pair; its minimums are 6.22:1 and 6.08:1 respectively.

## Representative terminal mockups

These are literal monospace renderings. Labels in square brackets are annotations, not extra runtime rows unless the text is shown inside a widget. Color names below each rendering describe the token used where plain Markdown cannot show terminal color.

### Main session view

Wide session at 142 columns, with the inspector closed:

~~~text
┌─ Talaria · live · default/session-7 ───────────────────────────────────────────────────────────────────────────────────────────────────────┐
│┃ > You  [operator]                                                                                                                         │
│┃   Please inspect the current change and explain the failing check.                                                                        │
│┃ A Talaria  [assistant]                                                                                                                    │
│┃   The status interval is negative, so startup must reject it visibly and retain the documented default.                                   │
│┃ · Reasoning  [reasoning]                                                                                                                  │
│┃   I am checking the config boundary before changing the rendering path.                                                                   │
│┃ $ read talaria/config.py  [activity]                                                                                                      │
│┃ — session resumed · profile default  [session]                                                                                            │
│┃ ! gateway warning: optional seam unavailable  [fault]                                                                                     │
│                                                                                                                                            │
│sub-agents: 1 · [>] running · test narrow layout                                                                                            │
│status: branch clean                                                                                                                        │
├─ compose [*] caret here ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Ask Talaria…                                                                                                                               │
├─ HelpBar · ctrl+g sub-agents · ctrl+c interrupt · / commands · f5 follow ──────────────────────────────────────────────────────────────────┤
│orch-design-codex│git: orch/tal…│Muse/Spark 1.2│ctx 25%│task 3/7 !1│[ok] up│v0.5.0                                                          │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
~~~

The six transcript fills use the corresponding transcript background tokens. Their always-reserved one-column thick gutter uses the matching foreground marker token. Body text stays talaria.text. The outer screen line is a mockup boundary, not a requested app border.

### Bottom status bar with all seven segments

The final terminal row at 144 columns or wider uses one-cell separators and never wraps:

~~~text
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│cwd: …/orch-design-codex│git: orch/talaria-v0…│agent: Muse · Spark 1.2│context: 32k/128k 25%│tasks: 3/7 !1│[ok] connected│v0.5.0              │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
~~~

Left to right the seven annotated segments are [1] cwd, [2] git_branch, [3] agent_model, [4] context, [5] task_progress, [6] connection, and [7] version. The runtime row has no outer border; it has talaria.status.background across every cell. Ordinary primary segment content uses talaria.status.text, secondary labels use talaria.status.muted, and each │ uses talaria.status.separator. The connection indicator and state text use talaria.status.success, talaria.status.warning, or talaria.status.error as specified in the Connection states table. The task_progress segment's literal !N marker uses talaria.status.attention.

### Right inspector expanded

Docked inspector at 132 columns with its 36-column default width:

~~~text
┌─ session ────────────────────────────────────────────────────────────────────────────────────┬─ Inspector [docked 36] ───────────┐
│┃ A I found two changed files. Select one to open its read-only diff.                         │TASKS                              │
│                                                                                              │ > [>] tests   running             │
│                                                                                              │   [ok] docs   completed           │
│                                                                                              │                                   │
│                                                                                              │CONTEXT                            │
│                                                                                              │ session  session-7                │
│                                                                                              │ profile  default                  │
│                                                                                              │ model    Muse Spark 1.2           │
│                                                                                              │                                   │
│                                                                                              │CHANGED FILES                      │
│                                                                                              │ > M talaria/config.py             │
│                                                                                              │   A tests/ui/test_status_bar.py   │
│                                                                                              │                                   │
│                                                                                              │OPERATION DETAILS                  │
│                                                                                              │ read · talaria/config.py          │
│                                                                                              │ 247 lines · completed             │
├─ compose [ ] caret elsewhere ────────────────────────────────────────────────────────────────┴───────────────────────────────────┤
│ Ask Talaria…                                                                                                                     │
├─ HelpBar · ctrl+b inspector · / commands ────────────────────────────────────────────────────────────────────────────────────────┤
│…/orch-design-codex│git: orch/tal…│Muse/Spark 1.2│ctx 25%│task 3/7 !1│[ok] up│v0.5.0                                              │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
~~~

The inspector boundary uses talaria.inspector.border; its fill uses talaria.inspector.background. Headings and the selected file use talaria.inspector.heading plus the fixed > focus gutter. Empty sections render the literal [none available from this session] sentence rather than disappearing, wrapping as needed so the complete sentence remains visible at every inspector width.

### Diff viewer, side-by-side

At 112 columns or wider the viewer opens side-by-side. The inspector is temporarily collapsed.

~~~text
┌─ diff · 1/2 talaria/config.py · side-by-side · read only ────────────────────────────────────────────────────────────┐
│ base · old                                               │ working tree · new                                        │
│  42 │  "status": {                                       │  42 │  "status": {                                        │
│  43-│      "interval_seconds": 5,                        │  43+│      "interval_seconds": 10,                        │
│  44 │  },                                                │  44 │  },                                                 │
│                                                          │                                                           │
│@@ -91,3 +91,4 @@ load_config                             │@@ -91,3 +91,4 @@ load_config                              │
│  93 │  merged = deepcopy(DEFAULTS)                       │  93 │  merged = deepcopy(DEFAULTS)                        │
│     │                                                    │  94+│  validate_intervals(merged)                         │
│  94 │  return Config(merged)                             │  95 │  return Config(merged)                              │
│                                                                                                                      │
│[read only] n/p hunk · N/P file · u unified · f files · esc close                                                     │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
~~~

The changed spans 5 and 10 use the intraline removed/added backgrounds; there are no brackets around them at runtime. Hunk foreground/background tokens apply to the complete @@ header row. Every removed row also has -, every added row has +, and every hunk has @@, so meaning never depends on color.

### Diff viewer, unified

Unified is always available and is automatic at 111 columns or narrower:

~~~text
┌─ diff · 1/2 talaria/config.py · unified · read only ─────────────────────────────────────────────────┐
│ old new                                                                                              │
│  42  42   "status": {                                                                                │
│  43      -    "interval_seconds": 5,                                                                 │
│      43  +    "interval_seconds": 10,                                                                │
│  44  44   },                                                                                         │
│@@ -91,3 +91,4 @@ load_config                                                                         │
│  93  93   merged = deepcopy(DEFAULTS)                                                                │
│      94  +validate_intervals(merged)                                                                 │
│  94  95   return Config(merged)                                                                      │
│                                                                                                      │
│[read only] n/p hunk · N/P file · s side-by-side · f files · esc close                                │
└──────────────────────────────────────────────────────────────────────────────────────────────────────┘
~~~

The differing numeric spans use intraline backgrounds while - and + remain visible. Syntax foregrounds continue outside those spans.

## Responsive states

All widths are terminal cell columns reported by Textual after the resize event. Threshold comparisons are inclusive as written. Resize handling is synchronous with the next layout pass; no debounce animation or intermediate wrapped row is permitted.

### Combined breakpoint table

| Terminal width | Status-bar default result | Inspector | Diff viewer |
|---:|---|---|---|
| 144 and wider | All seven, full forms within caps | Docked when user-expanded | Side-by-side preference honored |
| 120–143 | All seven, compact forms | Docked when user-expanded | Side-by-side preference honored |
| 112–119 | Drop version; compact remaining six | Auto-collapsed; toggle opens overlay | Side-by-side preference honored |
| 96–111 | Drop version; compact remaining six | Auto-collapsed; overlay available | Forced unified |
| 80–95 | Also drop cwd | Auto-collapsed; overlay available | Forced unified |
| 64–79 | Also drop git_branch | Auto-collapsed; overlay available | Forced unified |
| 48–63 | Also drop context | Auto-collapsed; overlay available | Forced unified |
| 32–47 | Also drop agent_model | Auto-collapsed; overlay available | Forced unified |
| 20–31 | Keep task_progress and connection compact while they fit | Auto-collapsed; overlay is terminal width minus two | Forced unified |
| Fewer than 20 | Drop task_progress; minimum connection form, hard-clipped only as a last resort | Full-terminal overlay only | Forced unified |

### Status-bar truncation and drop contract

Default config order is cwd, git_branch, agent_model, context, task_progress, connection, version. Configuration may reorder these names or omit a name to hide it. Priority does not change with display order.

| Segment | Full form and maximum | Compact form and maximum | Minimum form | Drop priority |
|---|---|---|---|---:|
| cwd | cwd: /path/to/repository, 24 | repository basename, 18 | …/repo | 10 |
| git_branch | git: branch-name, 18 | git: branch…, 14 | git:… | 20 |
| agent_model | agent: Provider · Model, 24 | Provider/Model, 18 | agt… | 50 |
| context | context: 32k/128k 25%, 22 | ctx 25%, 8 | ctx… | 40 |
| task_progress | tasks: 3/7 !1, 20 | task 3/7 !1, 12 | !1 or 3/7 | 80 |
| connection | [ok] connected, 16 | [ok] up, 8 | [ok], [..], [~], [x], or [!] | 100; never dropped |
| version | v0.5.0, 8 | v0.5.0, 8 | v… | 0 |

Lower numeric priority drops first: version, cwd, git_branch, context, agent_model, task_progress, then connection. Connection is never deliberately dropped.

Within a width band, the algorithm is deterministic:

1. Remove config-hidden and breakpoint-dropped segments.
2. Render full or compact forms prescribed by the band.
3. If the configured values still overflow, shorten the lowest-priority surviving segment from full to compact to minimum. Repeat in ascending priority order.
4. Use a middle ellipsis for paths, branches, and model names so both identity ends survive. Use a trailing ellipsis for other prose, but never remove the connection glyph, queue attention count, or context percentage.
5. Only after every lower-priority segment is at minimum may the lowest-priority segment drop. Repeat until the row fits.
6. Remove separators adjacent to a dropped segment. Render exactly one row with text-wrap disabled and ellipsis as the final terminal-capability safeguard.

In the 20–31-column band, `task_progress` and `connection` start in compact form. The normal overflow loop shortens `task_progress` only when their actual content does not fit; below 20 columns the breakpoint drops it and keeps the minimum connection form.

At default caps, seven full forms plus six one-cell separators fit in 144 columns. The optional status-bar integer keys are cwd_max_columns (default 24, valid 8–48), git_branch_max_columns (default 18, valid 8–40), and agent_model_max_columns (default 24, valid 10–48). Invalid values visibly fall back to their defaults. Layout thresholds remain fixed even when caps are customized.

An unknown segment name is omitted and reported in a startup notice. A non-list segments value falls back to the complete default list with a notice. If no configured name is valid, connection is rendered alone and the notice names the fallback. Existing status.interval_seconds accepts 1–3600 seconds; an invalid value visibly falls back to 5 seconds. A malformed status.command visibly reports the configuration problem in StatusRegion instead of silently disabling it.

### Inspector behavior

- ctrl+b and /inspector both toggle the panel. ctrl+b is primary and works without opening the palette.
- At 120 columns and wider, expanded means right-docked. Default width is 36, minimum 28, maximum 48. While focus is inside the inspector, Shift+Left and Shift+Right resize by four columns and clamp at the bounds.
- Width and manual collapsed/expanded state live only for the current process. Restart restores width 36 and expanded.
- Crossing from 120 to 119 auto-collapses without changing the user's manual preference. Returning to 120 restores a panel that was open before auto-collapse; a manually closed panel stays closed.
- Below 120, toggling opens a right overlay and does not reflow or resize the transcript. At 32–119 columns the overlay width is min(saved width, terminal width minus 2). Below 32 it occupies the terminal width. Escape closes it and restores focus to the previously focused widget.
- An overlay carries [overlay] in its existing border title. It does not add a notice row.
- Opening a diff collapses a docked inspector for the lifetime of the diff screen and restores it on close. The inspector overlay may still be invoked from the diff's file-list command.
- Tasks, Context, Changed files, and Operation details always keep their heading. Missing data reads [none available from this session]. The empty-state row wraps to two rows at widths 28 and 36 and fits on one row at width 48, so the complete sentence is always reachable. No section makes a gateway request, scans the filesystem, or starts a poll.

### Diff behavior

- Side-by-side requires 112 columns: two 54-column panes, two outer edge cells, one center divider, and one scrollbar/reserve cell.
- At 111 or fewer, the same diff state re-renders unified. The selected file, selected hunk, vertical anchor, and side-by-side preference survive the fallback. Resizing back to 112 restores side-by-side if that was the user's preference.
- Pressing s below 112 leaves unified active and replaces text in the existing header with side-by-side needs 112 columns; unified active. It MUST NOT add a row.
- u selects unified at any width. n/p navigate next/previous hunk; N/P navigate next/previous file; f opens the existing changed-file projection; Escape returns to the prior surface.
- Long source lines do not wrap. They clip with a visible ellipsis and allow bounded horizontal movement. A wrapped diff line would destroy line correspondence and is forbidden.
- The viewer is read-only in labels, commands, keymap, and implementation. No edit, stage, revert, discard, apply, or write action is present.

### Wide responsive mockup

At 132 columns, the inspector docks and all status segments use compact forms:

~~~text
┌─ main session [96 including edge] ───────────────────────────────────────────────────────────┬─ Inspector [36] ──────────────────┐
│transcript remains independently scrollable                                                   │TASKS · 2                          │
│body reflows once at the dock boundary                                                        │FILES · 2                          │
├─ compose height unchanged ───────────────────────────────────────────────────────────────────┴───────────────────────────────────┤
│HelpBar · ctrl+b inspector · / commands                                                                                           │
│orch-design-codex│git: orch/tal…│Muse/Spark 1.2│ctx 25%│task 3/7 !1│[ok] up│v0.5.0                                                │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
~~~

### Narrow responsive mockup

At 78 columns, the inspector is collapsed, the diff viewer is unified, and version, cwd, and git_branch have dropped:

~~~text
┌─ Talaria · live ───────────────────────────────────────────────────────────┐
│┃ A Transcript owns the width; Inspector overlays this content.             │
│┃ $ Diff uses unified mode automatically.                                   │
├─ compose [*] caret here ───────────────────────────────────────────────────┤
│ Ask Talaria…                                                               │
├─ HelpBar · ctrl+b inspector · / commands · f5 follow ──────────────────────┤
│Muse/Spark 1.2│ctx 25%│task 3/7 !1│[ok] up                                  │
└────────────────────────────────────────────────────────────────────────────┘
~~~

## Visual Studio Code color-theme mapping

The importer implements a bounded subset of the [Visual Studio Code workbench color keys](https://code.visualstudio.com/api/references/theme-color) and [TextMate tokenColors rules](https://code.visualstudio.com/api/extension-guides/color-theme#syntax-colors). The tables below are the allowlist. Nothing outside them is inferred.

### Input and resolution rules

1. The root must be a strict JSON object. name, type, and $schema are recognized metadata. colors must be an object when present and tokenColors must be an array when present.
2. Supported source colors are #RGB, #RGBA, #RRGGBB, and #RRGGBBAA, case-insensitive. Output is normalized to opaque uppercase #RRGGBB.
3. Alpha colors are composited in sRGB against the destination's normative background from the contrast table. Background tokens composite against their enclosing canvas or panel. If that background has not mapped yet, its Refined Default value is used. The import report names every alpha composite.
4. For a Talaria token with multiple candidate keys, the first present valid key in the listed order wins. A later candidate is not a warning; it is a documented fallback source.
5. Workbench colors map first, then tokenColors rules. tokenColors only writes syntax tokens.
6. Every Talaria token still missing after mapping is copied from Refined Default. Each copied token is reported by canonical name.
7. Invalid JSON, a wrong root shape, invalid colors/tokenColors types, or a file with neither a usable supported color nor a usable supported scope fails before any file is written. Re-import with the same chosen name deterministically replaces the stored user theme only after full validation.

### Supported workbench colors

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

### Supported tokenColors scopes

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

### Deliberately unsupported input

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

### Tokens with no Visual Studio Code source

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

Any other token falls back only when its listed source is absent or invalid. The successful import report states source-mapped count, fallback count, unsupported count, and every affected path/token. Successful imports with warnings exit successfully but visibly print the report; malformed imports write nothing and exit unsuccessfully.

Example report shape:

~~~text
Imported solar-example as user theme solar-example: 40 source tokens, 18 fallbacks, 3 warnings.
warning: root.include is unsupported; external theme files are not read
warning: colors.editorCursor.foreground is unsupported
warning: tokenColors[7].settings.fontStyle is unsupported
fallback: talaria.secondary <- Refined Default #6F42C1
fallback: talaria.transcript.operator <- Refined Default #0969DA
… 16 more fallback tokens
~~~

## Non-color signaling

Every status meaning has an ASCII glyph and a word or number in addition to color. Bracket forms are deliberately ASCII and fixed-width; implementation MUST NOT substitute ambiguous-width emoji.

### Connection states

| Runtime state | Required visible form | Color token |
|---|---|---|
| connected | [ok] connected; compact [ok] up | talaria.status.success |
| connecting | [..] connecting | talaria.status.warning |
| reconnecting | [~] reconnecting | talaria.status.warning |
| disconnected | [x] disconnected | talaria.status.error |
| authentication failed | [!] authentication failed | talaria.status.error |

The full sentence appears wherever space permits and in the connection detail. A minimum status segment retains at least [ok], [..], [~], [x], or [!].

### Agent states

| Runtime state | Required visible form | Color token |
|---|---|---|
| queued | [..] queued | text-muted |
| running | [>] running | accent |
| completed | [ok] completed | success |
| error | [!] error | error |
| failed | [x] failed | error |
| interrupted | [-] interrupted | warning |
| timeout | [t] timeout | warning |

AgentRows currently recognizes exactly these seven normalized states. A row may append detail after the required form but may not replace the form with color.

### Queue and other attention states

| State | Required visible form |
|---|---|
| Nothing needs attention | [ok] needs-you: none; compact task 3/7 |
| One or more waiting items | [!] needs-you: N; compact task 3/7 !N |
| Waiting item cannot be answered | [x] blocked followed by the existing reason |
| Possible duplicate sighting | [?] possibly duplicate |
| Prompt waiting for gateway | [..] waiting followed by the operation name |
| Status command failed | [x] status followed by exit/timeout/config reason |
| Status output truncated | [!] status truncated followed by the existing row-limit marker |

In the bottom bar's task_progress segment, the literal !N queue-attention marker uses talaria.status.attention; the glyph and count remain present when color is disabled. The task_progress segment replaces only NeedsYouBar's one-line summary. /needs continues to show the current queue's full, literal detail. Below 20 columns or on real overflow task_progress may drop, but opening /needs must still expose these non-color forms.

### Selection and transcript identity

- Palette, dialog, agent, inspector, and file-list rows reserve a one-column prefix. Focused is > and unfocused is a space. Color and bold are secondary cues.
- Prompt controls keep their current focus-within tint and add the same reserved > marker on the focused row.
- Transcript first rows identify groups without color: > You, A Talaria, . Reasoning, $ Tool/Subagent, - Session, and ! Error. The labels occupy the entry's existing first row; they do not insert a heading row.
- Diff lines retain +, -, and @@. Read only remains in the viewer header and key-hint line.

## Height-invariant focus and caret

The Composer's round border is always present today and remains present. v0.5.0 changes only values drawn inside existing cells:

| Composer state | Existing border title | Border token |
|---|---|---|
| Composer or descendant owns focus | compose [*] caret here | talaria.focus |
| Focus is elsewhere | compose [ ] caret elsewhere | talaria.border.muted |

Both titles occupy the already-rendered top border row. ChatTextArea remains compact, the Composer notice remains exactly one row, and the outer Composer height is computed from the same content in both states. No focus transition may mount/unmount a widget, change padding, change border thickness, change max-height, or toggle a notice row.

For other focusable surfaces, the reserved > gutter changes from a space to > while the row remains mounted. A bordered surface changes border.muted to focus without changing border style or width.

The implementation test and real-terminal acceptance must compare:

1. Composer outer height before and after moving focus.
2. Top-left screen row of Composer before and after moving focus.
3. Transcript viewport height and its anchored entry's screen row.
4. Footer row numbers.

All four values MUST be identical. Any added or removed screen row is a release-blocking regression.

## Transcript treatment, motion, and scroll

### Transcript treatment

- Preserve the current twelve-kind-to-six-group mapping and the already-reserved one-column gutter.
- Use each dedicated transcript marker/background pair instead of borrowing primary, success, secondary, accent, panel, or error directly.
- Keep prose foreground talaria.text. Group identity comes from the fixed first-row label, gutter, and background together.
- Bold only the existing first-row speaker/kind label. Do not recolor Markdown body text; syntax tokens retain their own foregrounds on opaque talaria.surface code blocks.
- Do not insert blank spacer rows between entries. Entry background and gutter provide separation without changing scroll geometry.
- Condensed and oversized-entry notices retain literal text and use text.muted or warning. They remain one row where the current source requires one row.

### Reduced motion

The documented key is:

~~~toml
[ui]
reduced_motion = false
~~~

When true, all nonessential spinners render a static [..] working/waiting form, smooth-scroll easing becomes an immediate jump to the same destination, pulsing/blinking application styles are disabled, and palette/theme transitions repaint once. Protocol progress and elapsed-time text continue to update because those are information, not decoration. The terminal emulator's own hardware caret blink is outside Talaria's control and is not simulated by the app.

The value participates in the existing configuration precedence and applies on restart. A malformed non-boolean value produces a visible fallback notice and uses false.

### Stable scroll

- Before append, status repaint, theme preview, inspector resize, or terminal resize, capture the stable transcript entry identifier and source-line offset at the top visible content row.
- If the viewport was exactly at its maximum scroll offset before an append, it is pinned and follows the new bottom.
- If it was not pinned, restore the same entry and nearest source-line offset to the same screen row after layout. Do not infer pinned state from a one-row proximity.
- When resize changes wrapping, retain the same entry and source-line offset even if the exact wrapped fragment must move to the nearest possible row.
- StatusRegion row growth, agent-row changes, theme preview/cancel, inspector dock/undock, BottomStatusBar changes, and footer focus changes MUST NOT jump an unpinned reader.
- Manual f5 follow-bottom establishes pinned state. Manual scrolling away clears it.

## Visual acceptance checklist

Tester ownership follows issue #110's approved split:

- **[talaria-t1]** owns the theming, Visual Studio Code import, and interaction/readability-polish track.
- **[talaria-t2]** owns the status bar, inspector, and diff-viewer track.
- **[shared]** means talaria-t1 and talaria-t2 each execute the item independently in their own environment. Install verification, restart semantics, and ordinary failure paths are shared.

Every item runs against the installed v0.5.0 wheel's talaria binary in a controlled real pseudo-terminal. At least one run per tester is a live Hermes-backed session. The primary model route is OpenCode Muse Spark 1.2 Contributor Free. Ollama GLM 5.3 Flash is permitted only for primary unavailability, connection failure, model-not-found, or bounded-test incompletion; the receipt records the route and exact fallback reason. No Computer Use evidence satisfies an item.

Each owning tester records terminal program, TERM value, terminal dimensions, installed Talaria version/artifact identity, session profile, model route, fallback reason or none, and screenshot/output receipt for every assigned item.

1. [ ] **[shared] Installed artifact.** Launch the installed talaria executable, not a source-tree module, in a fresh pseudo-terminal. Pass: the receipt identifies the installed v0.5.0 artifact and the visible version segment reads v0.5.0.
2. [ ] **[shared] Live primary route.** Start a live Hermes-backed session on OpenCode Muse Spark 1.2 Contributor Free. Pass: transcript traffic is live and the agent_model segment and receipt name that route; any fallback has one allowed explicit reason.
3. [ ] **[talaria-t2] Main hierarchy.** View an active wide session with transcript, optional agent/status regions, Composer, HelpBar, and BottomStatusBar. Pass: the status bar is the last terminal row, HelpBar is directly above it, and StatusRegion remains in the flexible body.
4. [ ] **[talaria-t1] Refined Default.** Select Refined Default and inspect transcript, Composer, palette, dialog, status, and inspector. Pass: it is light, all text remains readable, and no surface uses an unthemed stock color.
5. [ ] **[talaria-t1] Dark Green Terminal.** Preview Dark Green Terminal across the same surfaces. Pass: the dark green palette appears immediately and no content, focus marker, or status glyph disappears.
6. [ ] **[talaria-t1] Neutral Dark.** Preview Neutral Dark across the same surfaces. Pass: the low-saturation dark palette appears on every named surface with stable geometry.
7. [ ] **[talaria-t1] Accessible High Contrast.** Preview Accessible High Contrast across the same surfaces and a diff. Pass: boundaries, focus, selection, transcript groups, status, diff marks, and text are visibly distinct; captured runtime colors match the specified tokens and measured minimums are at least 6.22:1 text and 6.08:1 non-text.
8. [ ] **[talaria-t1] Preview cancellation.** Highlight at least two themes and press Escape. Pass: previews are immediate, Escape restores the exact pre-picker theme, and no config file timestamp/content changes.
9. [ ] **[talaria-t1] Explicit save and precedence.** Select a theme, save user scope, restart, then add a repository choice and finally an unsaved session choice. Pass: resolution is default → user → repository → session and only explicit save changes the selected config [theme] table.
10. [ ] **[talaria-t1] Theme fallback notice.** Start once with an unknown theme and import/select a partial theme. Pass: Refined Default fills unknown/missing values and a visible notice lists the theme error and every filled token rather than silently dropping it.
11. [ ] **[talaria-t1] Visual Studio Code import.** Import a fixture containing supported colors, supported scopes, an unsupported colors key, an unsupported fontStyle, and missing Talaria extension tokens. Pass: supported values appear, every unsupported path warns, every fallback token is listed, and re-import is deterministic.
12. [ ] **[talaria-t2] All status segments.** Resize to 144 columns or wider with default config. Pass: cwd, git_branch, agent_model, context, task_progress, connection, and version all appear in that order on exactly one row with six separators.
13. [ ] **[talaria-t2] Status configuration.** Reorder the seven names, hide two, and restart. Pass: display follows config exactly; hidden segments are absent; an unknown name produces a visible notice without preventing recognized segments.
14. [ ] **[talaria-t2] Status responsive sequence.** Resize through 144, 143, 120, 119, 112, 111, 96, 95, 80, 79, 64, 63, 48, 47, 32, 31, 20, and 19 columns. Pass: forms compact, then segments drop in the specified bands/order; connection remains; the bar never wraps or changes height.
15. [ ] **[shared] Status failure visibility.** Start with malformed status.command, then invalid interval and bar-width integers. Pass: each problem produces a visible startup/StatusRegion notice and the documented default is used; the shell-command status contract still renders literal bounded output.
16. [ ] **[talaria-t2] Inspector dock and resize.** At 120 columns or wider toggle the inspector, focus it, and resize beyond each bound. Pass: it docks right, changes four columns per action, clamps at 28 and 48, and never changes data by resizing.
17. [ ] **[talaria-t2] Inspector content and empty states.** Exercise tasks, context, changed files, and operation details with seeded/live state, then a session lacking each value. Pass: all four headings remain, existing state renders accurately, and the complete [none available from this session] sentence appears at inspector widths 28, 36, and 48 without a new request or filesystem scan.
18. [ ] **[talaria-t2] Inspector responsive state.** With inspector open, resize 120→119→120; also manually close and repeat. Pass: auto-collapse/restore follows the saved manual preference, the narrow toggle opens an overlay without transcript reflow, and geometry resets after process restart.
19. [ ] **[talaria-t2] Side-by-side diff.** Open a changed file at 112 columns or wider. Pass: two aligned panes show base/working line numbers, syntax colors, +/-, hunk headers, intraline spans, file/hunk position, read only, and the inspector is temporarily collapsed.
20. [ ] **[talaria-t2] Unified fallback.** Resize the same diff 112→111→112 and press u/s. Pass: it becomes unified at 111 with selection and scroll anchor preserved, returns to side-by-side at 112 when preferred, and the below-threshold refusal reuses the header row.
21. [ ] **[talaria-t2] Diff navigation and boundary.** Navigate previous/next hunk and file and inspect palette/key hints. Pass: selection cycles deterministically; long lines clip rather than wrap; no edit, stage, revert, discard, apply, or other writing action is offered or reachable.
22. [ ] **[talaria-t1] Composer caret location.** Move focus among Composer, transcript, a prompt control, agent row, and inspector. Pass: compose [*] caret here changes to compose [ ] caret elsewhere, border focus changes, other rows show >, and Composer height, Composer top row, transcript height/anchor, and both footer row numbers never change.
23. [ ] **[talaria-t1] Connection non-color states.** In a controlled live session exercise connected, connecting, reconnecting, disconnected, and authentication-failed paths. Pass: [ok], [..], [~], [x], or [!] plus state text remains readable with color disabled or a monochrome capture.
24. [ ] **[talaria-t1] Agent and queue non-color states.** Exercise all seven agent states and empty, waiting, blocked, and possible-duplicate queue states. Pass: every row carries its specified ASCII glyph/text; task_progress shows !N attention; /needs retains complete detail even when the segment is dropped.
25. [ ] **[talaria-t1] Transcript identity without color.** Capture operator, assistant, reasoning, activity, session, and fault entries in monochrome. Pass: first-row labels and gutter shapes distinguish all six groups and no entry gained a spacer row.
26. [ ] **[talaria-t1] Reduced motion.** Restart with ui.reduced_motion true and exercise waiting, scrolling, theme browsing, and reconnection. Pass: spinners/pulses/easing are absent, static state text remains, and critical state updates continue.
27. [ ] **[talaria-t1] Stable unpinned scroll.** Scroll to a middle transcript entry, then append output, refresh StatusRegion, update agents/status, preview/cancel a theme, resize, and dock/undock the inspector. Pass: the same entry/source offset stays anchored and the viewport never jumps to bottom.
28. [ ] **[talaria-t1] Stable pinned scroll.** Press f5 follow-bottom, append output, resize, and update status. Pass: the viewport follows the new bottom predictably; scrolling one row away clears pinned state.
29. [ ] **[talaria-t2] Wide screenshot.** Capture at 132 columns with inspector docked and compact seven-segment status. Pass: it matches the wide responsive mockup's hierarchy, one-row footer, and 36-column inspector.
30. [ ] **[talaria-t2] Narrow screenshot.** Capture at 78 columns and a unified diff below 112. Pass: inspector is collapsed/overlay-only, the specified status segments remain, no footer wraps, and diff content is unified.
31. [ ] **[talaria-t1] Malformed Visual Studio Code import.** Import malformed JSON through the installed CLI using a fresh theme name. Pass: the command exits clearly and unsuccessfully, reports the malformed input, and creates no stored theme artifact.
32. [ ] **[talaria-t2] Session-only status toggle.** Run /bar for a configured segment, inspect the row and config file, then restart. Pass: the segment toggles immediately, no config file content or timestamp changes, and restart restores the configured segment set.
33. [ ] **[shared] Dead gateway credential.** In each tester's scratch config, exercise the known stale-credential path after a controlled gateway restart. Pass: Talaria reaches an honest visible authentication error without a hang or silent blank, and the capture never exposes the credential.
34. [ ] **[shared] Killed session.** Kill the live throwaway session each tester is driving. Pass: the installed client renders a bounded, honest terminal state, does not hang or silently blank, and preserves enough connection/session context to recover or exit deliberately.
35. [ ] **[shared] Restart-only configuration.** While Talaria is running, edit one scratch-config value in the tester's assigned track, observe the unchanged running interface, restart, and observe the new value. Pass: the edit has no live effect, applies after restart, and proves no external-file watcher reloaded it.
36. [ ] **[shared] Cross-tester evidence.** Compare talaria-t1 and talaria-t2 receipts. Pass: both independently pass shared items 1, 2, 15, and 33–35; each assigned-track item has an owning tester's verdict, capture, and screenshot; each tester has a live Hermes-backed receipt; and every model route/fallback is explicit.

## Open questions for the operator

These questions do not block implementation; the choices in this specification remain normative unless deliberately superseded:

1. Should a later release derive Talaria-specific transcript tokens from an imported Visual Studio Code palette instead of always using Refined Default fallbacks? v0.5.0 uses explicit fallback and warning.
2. Should the inspector start collapsed by default after v0.5.0 usability evidence? v0.5.0 starts expanded when the terminal is at least 120 columns because the feature otherwise has poor discoverability.
3. Should ctrl+b remain the long-term inspector binding if live terminal evidence finds a conflict? v0.5.0 uses ctrl+b plus /inspector as the redundant path.
