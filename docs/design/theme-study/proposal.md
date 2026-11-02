# Talaria theme system design study

Talaria can offer control over each screen element while keeping the default interface quiet. **Recommend a versioned TOML theme, with a base palette, semantic roles, and optional element overrides. Ship Terminal, Ember, and Paper; leave Visual Studio Code theme import out of the first version.** Keep the existing Hermes skin path as the default. An operator decision on #475 is required before implementation cards are filed.

Offline study within the three-day timebox. Baseline: `3f6ea58ed8c3da9742b64a8e59a4d9c91e01a2bd`; branch: `study/475-theme-system`. The TestBackend prototype uses synthetic data. No theme loader, application integration, gateway access or terminal-mode changes are implemented.

## Decisions and scope

The [locked layout decision](https://github.com/infiquetra/talaria/issues/473#issuecomment-6569001361) is authoritative. The menu occupies one row. The right panel has 32 content columns and one divider. At 120x40 it docks; at 80x24 the captures show it explicitly opened as an overlay. Menu and panel remain independent, off by default. With either on, the opt-in full-screen workspace follows architecture decision record 0013; this study does not write or amend that record. Turning both off restores inline rendering and its native scrollback contract.

The panel shows attention, goal, turn, explicit verdict, checklist, and agents in that order. Pending is not success. Waiting agents come first, then running, queued and recent completions; unknown model or effort stays unknown. Overflow is explicit. The checklist appears once. Voice phase and read-aloud stay in the bottom status row. Themes change appearance, never panel width, docking thresholds, focus rules, action availability, key bindings, ordering, visibility preferences, or verdict semantics.

The [coordination note](https://github.com/infiquetra/talaria/issues/475#issuecomment-6570082299) requires the picker surfaces to join the catalogue. The reserved `picker` hierarchy includes filter bar, group tabs and headers, selected row, badges and detail pane. The final publication check of #474 is recorded below. These are appearance names, not an attempt to decide the picker's interaction design.

## What exists and what must change later

The current `Theme` groups Markdown, status, composer border/marker, reasoning and tools/diffs. Many views still own independent palettes, including syntax highlighting. The coverage table identifies them. A future build must migrate those palettes too; wrapping only today's `Theme` would miss substantial styling.

Source anchors: `crates/talaria-tui/src/theme/{mod,roles,cells}.rs`, `markdown/{theme,highlight}.rs`, and the view modules below; configuration and appearance are in `crates/talaria-app/src/{config,appearance}.rs` and `crates/talaria-core/src/appearance.rs`.

Hermes evidence is the vendored contract pinned by `crates/hermes-contract/PINNED`: v0.22.4, commit `4d04303818b61bf3aa09278af48356315515965b`. `SkinPayload` has string-valued color maps. No new gateway method or profile-file reader is proposed.

## Q1: tokens and complete fallback rules

Names are lower-case dotted paths owned by Talaria. A token identifies an element and state, not a terminal library widget. A token is a style record: optional `fg`, `bg`, `underline_color`, and explicit boolean `bold`, `dim`, `italic`, `underline`, `reverse`, `strikethrough`. Omission inherits that field; `false` removes an inherited attribute; `"default"` resets a color to the terminal default. No wildcard selectors, expressions, scripts, arbitrary escape sequences or unbounded style chains.

Each catalogue entry is independently overridable. Numbers identify rendered swatches; configuration uses dotted names. Every token appears in both sizes of every theme's atlas. There are no undocumented token-name fallbacks.

| Layer, low to high | Fallback and merge behavior |
|---|---|
| Built-in `terminal`, `dark`, or `light` base | Supplies every palette slot, role, element default, attribute and chrome setting. Terminal is the compatibility/default presentation; dark and light are complete explicit bases. |
| Hermes bridge, when enabled and eligible | Applies the existing skin key chains to the same existing surfaces, preserving today's unmapped defaults. New surfaces take their declared roles. No skin, follow off, 16 colors or colors off keeps today's bridge exclusion. |
| `TALARIA_ACCENT` compatibility alias, if present and valid | Proposed alias for the semantic accent foreground. Above the skin, below file-authored values. It never changes primary, warning or error. Invalid value produces one notice. |
| Selected file palette | Replaces only named palette slots. Role references are resolved after this replacement. |
| Selected file roles | Replaces only authored style fields. Inherited role references see the effective palette. |
| Element default, then selected file element override | Default role and attributes come from the catalogue; element override replaces only authored fields. Explicit skin-bound legacy element colors survive unless the file overrides that element or authors its bound role/palette slot. |
| Rendering constraints | Color depth, colors-off, supported attributes, single-cell glyph validation and reduced motion apply last. They cannot be overridden by a theme. |

Retain each field's source for `/theme` inspection: authored roles/palette slots supersede skin defaults without erasing unrelated skin distinctions. The bridge requires configuration permission and no file-level `follow_hermes_skin = false`. Polarity overlays precede skin base keys. Without a custom theme, preserve the existing contrast lift and dim removal.

| Element field | Exact fallback |
|---|---|
| `fg` | Explicit element value → explicitly authored bound role → bound palette slot → eligible skin-bound legacy value → catalogue semantic role → base role → terminal default. In this chain the explicit file role/palette wins over the skin as stated above. |
| `bg` | Explicit element value → explicitly authored role background → area background → terminal default. Selection/focus supply their own paired foreground and background. |
| `underline_color` | Explicit element value → explicitly authored role value → effective foreground. `"default"` explicitly resets it. It uses the same palette-reference rules and final depth conversion as foreground; colors-off removes it. |
| Attributes | Base element attributes from catalogue, patched by explicit role booleans, then element booleans. All unspecified attributes are false. `false` always clears. |
| Area background | `background` for transcript, composer, status, tool, diff and workspace; `surface` for menu, work panel, dialog, picker, code, queue, alerts, badges, lists and alternate views. |
| Unrecognized token/key/role | Reject the candidate file as a whole with its key and line/column; keep last good theme. Do not silently accept misspellings. |
| New token in a later compatible version | Receives its new catalogue default from a semantic role; old files remain complete without edits. |

The fourteen roles are background, surface, text, muted, disabled, border, primary, accent, success, warning, error, attention, selection and focus. Focus shares selection's default color pair. The twelve palette slots appear in the full example. These defaults apply to the opt-in catalogue. With no file or catalogue choice, preserve the existing resolver and independent view defaults exactly. The Terminal preview demonstrates an explicitly selected theme, not byte-for-byte compatibility of today's defaults.

Shared components use `list.*`, `dialog.*`, `picker.*` and `border.*`. Concrete owners use their named tokens where present. New independently styled elements require catalogue entries rather than hardcoded colors.

| Current owner or visual family | Proposed mapping |
|---|---|
| Markdown and history | `transcript.*`, `code.*`; includes user/system/command/banner, reasoning, links, images, quotes, headings, rules and tables |
| Composer, attachment, caret, completion | `composer.*`; caret changes only cells Talaria draws, not the terminal's cursor mode |
| Status, connectors and voice | `status.*`; ordinary and health tones retain distinct overrides |
| Queue preview | `queue.*` |
| Approval, clarification, secrets, destructive confirmation | `dialog.*`; masked text stays masked regardless of style |
| Model, effort, session, profile, route, role and command pickers | `picker.*`; `palette.origin` for the existing origin tag |
| Menu and dropdowns | `menu.*` |
| Right work panel and hosted checklist | `work.*`; the same task tokens style the inline checklist when panel is hidden |
| Workspace and shared borders | `workspace.*`, `border.*` |
| Tool cells, diff and review | `tool.*`, `diff.*`, `review.*`; syntax uses `code.syntax.*` |
| Notices, labels and badges | `alert.*`, `badge.*` |
| Fleet, agent dock/panel, background, shell/side output | `fleet.*`, `agents.*`, `background.*`, `side.*`, shared `list.*` |
| Automation, context, pager, rewind, workflow, scheduled jobs | `automation.*`, `context.*`, `pager.*`, `rewind.*`, `workflow.*`, `cron.*` |
| Provider, launch, configuration, diagnostic, mention, editor, settings views | `providers.*`, `launch.*`, `configure.*`, `doctor.*`, `mention.*`, `editor.*`, `settings.*`; remaining form fields use `dialog.*`, rows use `list.*` |

## Q2 and Q3: small files, full themes and disk location

Use **TOML with `schema = 1`**, matching existing configuration. `extends` accepts only built-in `terminal`, `dark`, or `light`; no file chains or URLs. Identifiers match `[a-z0-9][a-z0-9_-]{0,63}`, confined to the theme directory. Files are capped at 256 KiB. References accept `$palette.slot` only, without environment interpolation or recursion.

Store themes per user beside Talaria's `config.toml`, in `themes/<id>.toml` (portable default `~/.talaria/themes/<id>.toml`), shared across profiles. Propose `[theme] file = "quiet-amber"` with optional `[theme.profiles.<profile-id>] file = "..."`; quote profile keys. Selection: `/theme use <id>` session override → profile mapping → user file → existing skin behavior. Do not read or rewrite Hermes files. Ember/Paper retain their authored polarity; Terminal inherits detected defaults.

The minimal example changes one palette slot and one element attribute. The full example supplies twelve colors, fourteen roles and both chrome choices; catalogue defaults complete it. Any element can be added under `[elements."..."]`. Accept quoted `"default"`, `"#RRGGBB"`, `"ansi:0"`–`"ansi:15"`, or `"index:0"`–`"index:255"`. The new parser is strict; the existing skin parser stays separate and lenient.

## Q4: non-color style and accessibility

Yes: bold, dim, italic, underline, reverse and strikethrough are field-level settings. Styling never changes action availability or semantic symbols. Forbid invisible/blinking attributes. Unsupported attributes fall back to plain text; selection uses reverse without a reliable color pair.

Offer a small validated chrome vocabulary: `border = "rounded" | "single" | "double" | "ascii" | "none"`, `section_divider = "single" | "double" | "ascii" | "none"`. `none` removes the glyph but preserves structural spacing and focus indication. Border characters are controlled through these presets; arbitrary glyph sets are deferred. ASCII remains the width-safe fallback. No theme-controlled padding, sizes, animations, icons carrying state, key symbols, or multi-line dividers.

In the opt-in catalogue path, color off has final priority: nonempty `NO_COLOR`, then existing color-depth resolution. Strip foreground, background and underline color across all migrated palettes; keep supported attributes and symbols. The no-custom-theme path retains existing behavior, including rewind's inconsistent empty-`NO_COLOR` handling. Normalizing that compatibility path is a separate change requiring a decision. Terminal's ANSI (American National Standards Institute) palette theme uses terminal-default foreground/background and named 16-color accents. Terminal palette customization remains effective. For RGB (red, green and blue) themes, quantize to the existing 256-color converter; for explicit custom themes on 16 colors, choose the closest of the conventional 16-color reference palette, preserve semantic symbols and use reverse selection. Actual ANSI palette colors are user-controlled, so measured contrast cannot be guaranteed there. The skin-only bridge continues to be skipped at 16 colors.

For custom true-color themes, propose a contrast diagnostic of 4.5:1 for ordinary text and 3:1 for structural markers against known explicit backgrounds. Warn with token names without silently rewriting custom authored colors. These are proposed design targets, not a claim that every terminal or the existing skin floor meets them. Background/default colors that cannot be measured are reported as unknown. Explicit full backgrounds paint only Talaria-owned workspace/overlay cells; inline mode does not clear or recolor the terminal's scrollback. Existing unthemed scheduled-job and workflow selections already paint backgrounds, despite the broad wording in `docs/themes.md`; the future documentation must describe the actual narrower guarantee.

## Q5: Visual Studio Code import

**Out for the first version.** Editor colors cannot directly cover queued drafts, approval prompts, tools, voice or the work panel. Import adds mappings, fallback diagnostics and a second format; syntax scopes add another problem. Three built-ins and TOML meet the requested control. If later justified, use an offline converter that emits native TOML and lists unmapped keys, never runtime extension execution. This recommendation follows the local inventory.

## Q6: skin names, accent and current behavior

Keep skin names through existing `/skin` and resolved describe/event payloads. Hermes owns `config.yaml`; Talaria does not read it. Preserve the chains below. The documentation's `display.skin` wording differs from Talaria's `config.set` key `skin`; this study leaves that discrepancy unchanged.

The baseline has **no active `TALARIA_ACCENT` reader**. Propose a compatibility alias: one valid color for accent, after skin defaults and before file overrides, reported by `/theme`. With no variable or file, require unchanged output. Use the new `file` key; preserve the obsolete `[theme] name` notice.

| Existing bridge role | Hermes keys in first-parseable order | Existing surfaces retained |
|---|---|---|
| primary | `ui_primary`, `banner_title` | Markdown headings |
| accent | `ui_accent`, `banner_accent` | Inline code, link text, status accent |
| border | `input_rule`, `ui_border`, `banner_border` | Quote/rule/table/code label, composer border |
| prompt | `prompt`, `banner_text` | Composer marker |
| thinking | `ui_thinking`, `banner_dim` | Reasoning |
| tool | `ui_tool`, `ui_accent`, `banner_accent` | Running tool glyph |
| ok / error | `ui_ok` / `ui_error` | Tool success / failed glyph and error |
| muted | `banner_dim` | Tool metadata, muted glyph, line numbers, diff notes |
| diff added / removed | `diff_added_word`, `ui_ok` / `diff_removed_word`, `ui_error` | Added / removed diff text |
| diff hunk / file | `session_border` / `session_label` | Diff hunk / file heading |
| status text / muted | `status_bar_text` / `status_bar_dim` | Status text / muted and separators |
| status success | `status_bar_good`, `ui_ok` | Successful status |
| status warning | `status_bar_warn`, `ui_warn` | Warning status |
| status error | `status_bar_critical`, `status_bar_bad`, `ui_error` | Critical status |
| status attention | `status_bar_strong`, `ui_accent` | Attention status |

Preserve these distinct chains and the foreground-only bridge. Existing contrast floors are ordinary dark/light 1.45/1.18 and semantic dark/light 2.2/1.6. The generated-contract limitation with null skin colors is outside this study.

## Invalid files, reload and built-ins

Invalid startup files produce one notice naming the relative identifier and reason, then use the mode/skin fallback. Invalid reloads retain the last good theme. Validate everything before atomic application. `/theme reload` reads through application effects and the one event channel; `/theme use` is session-only. `/theme` reports file, base, skin participation, mode/depth/motion, sources and warnings. Start with manual reload. Defer a watcher; if added, debounce events without idle repainting.

Freeze a streaming cell's theme until it finishes. Native scrollback cannot be recolored; new content and chrome use the new theme, while pager/workspace redraws may recolor retained content. Resolve styles outside the domain core. Only the terminal module changes modes or writes escape sequences.

Built-ins: **Terminal** (terminal background and ANSI accents), **Ember** (dark warm, high contrast), **Paper** (light blue/brown accents). `dark`/`light` are inheritance bases. Preserve `/theme light|dark|auto`, `TALARIA_THEME`, configuration mode/colors/following, and motion precedence; theme selection never changes motion.

## Prototype evidence and limits

The prototype, `crates/talaria-tui/examples/theme_study.rs`, draws three themes and complete swatch atlases at both sizes. It uses resolved palette constants and catalogue defaults, not a TOML parser or inheritance implementation. The full example's overrides repeat those defaults. Tests check each swatch's colors/attributes and the divider, pending verdict, single checklist, overflow and bottom-row voice. They prove buffers, not application integration, depth conversion, skin compatibility, reload or real-terminal acceptance.

After the release gate has stopped, reproduce from the repository root with `cargo run -p talaria-tui --example theme_study`; it writes `docs/design/theme-study/screens.md`. Run `cargo test -p talaria-tui --example theme_study` for the focused checks. The operator's machine guard requires the designated shared Cargo target and absence of the release-candidate script before either command. No gateway is needed.

The remaining decision is whether to accept this hierarchy, the three palettes and the native-file-only recommendation. No build cards, PR, merge, release or deployment are part of this study.
