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

|Layer, low to high|Fallback and merge behavior|
|---|---|
|Built-in `terminal`, `dark`, or `light` base|Supplies every palette slot, role, element default, attribute and chrome setting. Terminal is the compatibility/default presentation; dark and light are complete explicit bases.|
|Hermes bridge, when enabled and eligible|Applies the existing skin key chains to the same existing surfaces, preserving today's unmapped defaults. New surfaces take their declared roles. No skin, follow off, 16 colors or colors off keeps today's bridge exclusion.|
|`TALARIA_ACCENT` compatibility alias, if present and valid|Proposed alias for the semantic accent foreground. Above the skin, below file-authored values. It never changes primary, warning or error. Invalid value produces one notice.|
|Selected file palette|Replaces only named palette slots. Role references are resolved after this replacement.|
|Selected file roles|Replaces only authored style fields. Inherited role references see the effective palette.|
|Element default, then selected file element override|Default role and attributes come from the catalogue; element override replaces only authored fields. Explicit skin-bound legacy element colors survive unless the file overrides that element or authors its bound role/palette slot.|
|Rendering constraints|Color depth, colors-off, supported attributes, single-cell glyph validation and reduced motion apply last. They cannot be overridden by a theme.|

Retain each field's source for `/theme` inspection: authored roles/palette slots supersede skin defaults without erasing unrelated skin distinctions. The bridge requires configuration permission and no file-level `follow_hermes_skin = false`. Polarity overlays precede skin base keys. Without a custom theme, preserve the existing contrast lift and dim removal.

|Element field|Exact fallback|
|---|---|
|`fg`|Explicit element value → explicitly authored bound role → bound palette slot → eligible skin-bound legacy value → catalogue semantic role → base role → terminal default. In this chain the explicit file role/palette wins over the skin as stated above.|
|`bg`|Explicit element value → explicitly authored role background → area background → terminal default. Selection/focus supply their own paired foreground and background.|
|`underline_color`|Explicit element value → explicitly authored role value → effective foreground. `"default"` explicitly resets it. It uses the same palette-reference rules and final depth conversion as foreground; colors-off removes it.|
|Attributes|Base element attributes from catalogue, patched by explicit role booleans, then element booleans. All unspecified attributes are false. `false` always clears.|
|Area background|`background` for transcript, composer, status, tool, diff and workspace; `surface` for menu, work panel, dialog, picker, code, queue, alerts, badges, lists and alternate views.|
|Unrecognized token/key/role|Reject the candidate file as a whole with its key and line/column; keep last good theme. Do not silently accept misspellings.|
|New token in a later compatible version|Receives its new catalogue default from a semantic role; old files remain complete without edits.|

The fourteen roles are background, surface, text, muted, disabled, border, primary, accent, success, warning, error, attention, selection and focus. Focus shares selection's default color pair. The twelve palette slots appear in the full example. These defaults apply to the opt-in catalogue. With no file or catalogue choice, preserve the existing resolver and independent view defaults exactly. The Terminal preview demonstrates an explicitly selected theme, not byte-for-byte compatibility of today's defaults.

Shared components use `list.*`, `dialog.*`, `picker.*` and `border.*`. Concrete owners use their named tokens where present. New independently styled elements require catalogue entries rather than hardcoded colors.

|Current owner or visual family|Proposed mapping|
|---|---|
|Markdown and history|`transcript.*`, `code.*`; includes user/system/command/banner, reasoning, links, images, quotes, headings, rules and tables|
|Composer, attachment, caret, completion|`composer.*`; caret changes only cells Talaria draws, not the terminal's cursor mode|
|Status, connectors and voice|`status.*`; ordinary and health tones retain distinct overrides|
|Queue preview|`queue.*`|
|Approval, clarification, secrets, destructive confirmation|`dialog.*`; masked text stays masked regardless of style|
|Model, effort, session, profile, route, role and command pickers|`picker.*`; `palette.origin` for the existing origin tag|
|Menu and dropdowns|`menu.*`|
|Right work panel and hosted checklist|`work.*`; the same task tokens style the inline checklist when panel is hidden|
|Workspace and shared borders|`workspace.*`, `border.*`|
|Tool cells, diff and review|`tool.*`, `diff.*`, `review.*`; syntax uses `code.syntax.*`|
|Notices, labels and badges|`alert.*`, `badge.*`|
|Fleet, agent dock/panel, background, shell/side output|`fleet.*`, `agents.*`, `background.*`, `side.*`, shared `list.*`|
|Automation, context, pager, rewind, workflow, scheduled jobs|`automation.*`, `context.*`, `pager.*`, `rewind.*`, `workflow.*`, `cron.*`|
|Provider, launch, configuration, diagnostic, mention, editor, settings views|`providers.*`, `launch.*`, `configure.*`, `doctor.*`, `mention.*`, `editor.*`, `settings.*`; remaining form fields use `dialog.*`, rows use `list.*`|

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

|Existing bridge role|Hermes keys in first-parseable order|Existing surfaces retained|
|---|---|---|
|primary|`ui_primary`, `banner_title`|Markdown headings|
|accent|`ui_accent`, `banner_accent`|Inline code, link text, status accent|
|border|`input_rule`, `ui_border`, `banner_border`|Quote/rule/table/code label, composer border|
|prompt|`prompt`, `banner_text`|Composer marker|
|thinking|`ui_thinking`, `banner_dim`|Reasoning|
|tool|`ui_tool`, `ui_accent`, `banner_accent`|Running tool glyph|
|ok / error|`ui_ok` / `ui_error`|Tool success / failed glyph and error|
|muted|`banner_dim`|Tool metadata, muted glyph, line numbers, diff notes|
|diff added / removed|`diff_added_word`, `ui_ok` / `diff_removed_word`, `ui_error`|Added / removed diff text|
|diff hunk / file|`session_border` / `session_label`|Diff hunk / file heading|
|status text / muted|`status_bar_text` / `status_bar_dim`|Status text / muted and separators|
|status success|`status_bar_good`, `ui_ok`|Successful status|
|status warning|`status_bar_warn`, `ui_warn`|Warning status|
|status error|`status_bar_critical`, `status_bar_bad`, `ui_error`|Critical status|
|status attention|`status_bar_strong`, `ui_accent`|Attention status|

Preserve these distinct chains and the foreground-only bridge. Existing contrast floors are ordinary dark/light 1.45/1.18 and semantic dark/light 2.2/1.6. The generated-contract limitation with null skin colors is outside this study.

## Invalid files, reload and built-ins

Invalid startup files produce one notice naming the relative identifier and reason, then use the mode/skin fallback. Invalid reloads retain the last good theme. Validate everything before atomic application. `/theme reload` reads through application effects and the one event channel; `/theme use` is session-only. `/theme` reports file, base, skin participation, mode/depth/motion, sources and warnings. Start with manual reload. Defer a watcher; if added, debounce events without idle repainting.

Freeze a streaming cell's theme until it finishes. Native scrollback cannot be recolored; new content and chrome use the new theme, while pager/workspace redraws may recolor retained content. Resolve styles outside the domain core. Only the terminal module changes modes or writes escape sequences.

Built-ins: **Terminal** (terminal background and ANSI accents), **Ember** (dark warm, high contrast), **Paper** (light blue/brown accents). `dark`/`light` are inheritance bases. Preserve `/theme light|dark|auto`, `TALARIA_THEME`, configuration mode/colors/following, and motion precedence; theme selection never changes motion.

## Prototype evidence and limits

The prototype, `crates/talaria-tui/examples/theme_study.rs`, draws three themes and complete swatch atlases at both sizes. It uses resolved palette constants and catalogue defaults, not a TOML parser or inheritance implementation. The full example's overrides repeat those defaults. Tests check each swatch's colors/attributes and the divider, pending verdict, single checklist, overflow and bottom-row voice. They prove buffers, not application integration, depth conversion, skin compatibility, reload or real-terminal acceptance.

After the release gate has stopped, reproduce from the repository root with `cargo run -p talaria-tui --example theme_study`; it writes `docs/design/theme-study/screens.md`. Run `cargo test -p talaria-tui --example theme_study` for the focused checks. The operator's machine guard requires the designated shared Cargo target and absence of the release-candidate script before either command. No gateway is needed.

The remaining decision is whether to accept this hierarchy, the three palettes and the native-file-only recommendation. No build cards, PR, merge, release or deployment are part of this study.


## Complete example theme files

### Minimal override

```toml
schema = 1
name = "Quiet amber"
extends = "dark"
follow_hermes_skin = true

[palette]
accent = "#ffb86b"

[elements."composer.border.focused"]
fg = "$palette.accent"
bold = true
```

### Full custom theme

```toml
schema = 1
name = "Ember"
extends = "dark"
follow_hermes_skin = false

[palette]
background = "#171411"
surface = "#25201b"
foreground = "#f5eadb"
muted = "#c4b5a2"
disabled = "#a19383"
border = "#9c846b"
primary = "#ffc078"
accent = "#ffb86b"
success = "#a8d08d"
warning = "#ffd166"
error = "#ff8b82"
attention = "#ffd166"

[roles.background]
fg = "$palette.foreground"
bg = "$palette.background"
[roles.surface]
fg = "$palette.foreground"
bg = "$palette.surface"
[roles.text]
fg = "$palette.foreground"
[roles.muted]
fg = "$palette.muted"
[roles.disabled]
fg = "$palette.disabled"
[roles.border]
fg = "$palette.border"
[roles.primary]
fg = "$palette.primary"
[roles.accent]
fg = "$palette.accent"
[roles.success]
fg = "$palette.success"
[roles.warning]
fg = "$palette.warning"
[roles.error]
fg = "$palette.error"
[roles.attention]
fg = "$palette.attention"
[roles.selection]
fg = "$palette.background"
bg = "$palette.accent"
[roles.focus]
fg = "$palette.background"
bg = "$palette.accent"

[chrome]
border = "rounded"
section_divider = "single"

# All element defaults are specified by the catalogue; none need repetition.
# These explicit examples have the same appearance as their inherited values.
[elements."work.verdict.pending"]
fg = "$palette.muted"
bold = false
dim = false
[elements."transcript.heading.1"]
fg = "$palette.primary"
bold = true
[elements."picker.row.selected"]
fg = "$palette.background"
bg = "$palette.accent"
reverse = false
```


## Complete token catalogue and inheritance table

Each entry is `swatch-number suffix`. Prepend the Area and a dot to obtain the exact token name: Area `transcript` plus `user.label` means `transcript.user.label`. Role and attributes apply to every entry in that row; backgrounds inherit the area as specified above. All 323 tokens are independently overridable. Full descriptions are in `docs/design/theme-study/tokens.txt`.

|Area|Role|Attributes|Swatch and suffix|
|---|---|---|---|
|agents|accent|-|0269 running|
|agents|disabled|-|0274 gone|
|agents|error|-|0272 failed|
|agents|muted|-|0270 queued; 0275 help|
|agents|primary|bold|0268 header|
|agents|success|-|0271 done|
|agents|text|-|0276 input|
|agents|warning|-|0273 interrupted|
|alert|accent|-|0201 info|
|alert|attention|bold|0205 attention|
|alert|error|bold|0204 error|
|alert|success|-|0202 success|
|alert|surface|-|0206 background|
|alert|warning|bold|0203 warning|
|automation|accent|-|0232 accent|
|automation|muted|-|0234 metadata|
|automation|primary|bold|0231 heading|
|automation|warning|-|0233 warning|
|background|error|-|0281 error|
|background|muted|-|0279 metadata|
|background|selection|-|0278 selected|
|background|success|-|0282 success|
|background|text|-|0277 text|
|background|warning|-|0280 warning|
|badge|attention|bold|0212 attention|
|badge|disabled|-|0213 disabled|
|badge|error|bold|0211 error|
|badge|success|bold|0209 success|
|badge|surface|-|0208 background|
|badge|text|-|0207 text|
|badge|warning|bold|0210 warning|
|border|accent|-|0161 focused|
|border|border|-|0160 normal; 0163 divider|
|border|disabled|-|0162 disabled|
|code|accent|-|0166 inline; 0173 syntax.function|
|code|error|underline|0179 syntax.invalid|
|code|muted|-|0167 label; 0168 line_number; 0176 syntax.punctuation|
|code|muted|italic|0172 syntax.comment|
|code|primary|-|0174 syntax.type|
|code|primary|bold|0169 syntax.keyword|
|code|success|-|0170 syntax.string|
|code|surface|-|0164 background|
|code|text|-|0165 text; 0175 syntax.operator; 0177 syntax.variable|
|code|warning|-|0171 syntax.number; 0178 syntax.constant|
|composer|accent|-|0035 border.focused; 0038 attachment|
|composer|accent|bold|0033 marker|
|composer|background|-|0030 background|
|composer|border|-|0034 border|
|composer|error|bold|0039 attachment.error|
|composer|focus|-|0037 cursor|
|composer|muted|-|0032 placeholder; 0040 paste; 0043 completion.description|
|composer|selection|-|0036 selection; 0042 completion.selected|
|composer|text|-|0031 text; 0041 completion.text|
|configure|selection|-|0316 selection|
|context|accent|-|0244 section.tool_definitions; 0248 section.subagent_definitions; 0250 section.conversation|
|context|error|-|0241 error|
|context|muted|-|0240 free; 0249 section.memory; 0251 section.other|
|context|primary|-|0243 section.system_prompt; 0246 section.skills|
|context|primary|bold|0238 header|
|context|success|-|0242 tick; 0247 section.mcp|
|context|text|-|0239 text|
|context|warning|-|0245 section.rules|
|cron|accent|-|0304 accent|
|cron|error|-|0306 error|
|cron|muted|-|0307 hint|
|cron|primary|bold|0303 heading|
|cron|selection|-|0308 selected|
|cron|surface|-|0309 input|
|cron|warning|-|0305 warning|
|dialog|accent|-|0074 button; 0079 key; 0082 approval.path|
|dialog|background|-|0080 backdrop|
|dialog|border|-|0067 border|
|dialog|disabled|-|0076 button.disabled|
|dialog|error|bold|0077 error; 0086 confirm.destructive|
|dialog|focus|bold|0075 button.focused|
|dialog|muted|-|0070 muted; 0072 placeholder; 0085 secret.mask|
|dialog|primary|bold|0068 title|
|dialog|selection|-|0073 selection; 0084 clarification.selected|
|dialog|surface|-|0066 background|
|dialog|text|-|0069 text; 0071 input; 0081 approval.command; 0083 clarification.option|
|dialog|warning|-|0078 warning|
|diff|accent|-|0184 hunk|
|diff|background|-|0180 background|
|diff|error|-|0182 removed|
|diff|error|bold|0189 removed.word|
|diff|muted|-|0186 note; 0187 line_number|
|diff|primary|bold|0185 file|
|diff|selection|-|0190 selection|
|diff|success|-|0181 added|
|diff|success|bold|0188 added.word|
|diff|text|-|0183 context|
|doctor|primary|bold|0318 header|
|doctor|selection|-|0317 selection|
|editor|selection|-|0321 selection|
|fleet|accent|-|0256 state.working; 0264 label.accent|
|fleet|attention|bold|0255 state.attention|
|fleet|error|-|0257 state.failed; 0261 label.error|
|fleet|muted|-|0259 state.idle; 0263 label.muted; 0265 secondary|
|fleet|primary|bold|0252 header; 0253 group|
|fleet|selection|-|0254 cursor|
|fleet|success|-|0258 state.done; 0260 label.success|
|fleet|text|-|0266 peek; 0267 input|
|fleet|warning|-|0262 label.warning|
|launch|error|-|0313 error|
|launch|muted|-|0315 secondary|
|launch|primary|bold|0314 marker|
|list|accent|-|0224 running|
|list|attention|bold|0223 attention|
|list|border|-|0225 divider; 0226 scrollbar|
|list|disabled|-|0219 disabled|
|list|error|-|0222 error|
|list|muted|-|0218 muted; 0227 empty|
|list|primary|bold|0215 header|
|list|selection|-|0217 selected|
|list|success|-|0220 success|
|list|surface|-|0214 background|
|list|text|-|0216 row|
|list|warning|-|0221 warning|
|mention|muted|-|0320 metadata|
|mention|selection|-|0319 selection|
|menu|border|-|0123 dropdown.border; 0128 dropdown.separator|
|menu|disabled|-|0121 item.disabled; 0126 dropdown.disabled|
|menu|focus|bold|0120 item.focused|
|menu|muted|-|0127 dropdown.key|
|menu|primary|bold|0118 brand|
|menu|selection|-|0125 dropdown.selected|
|menu|surface|-|0117 background; 0122 dropdown.background|
|menu|text|-|0119 item; 0124 dropdown.item|
|pager|accent|bold|0286 gutter_mark|
|pager|selection|-|0283 header; 0284 footer; 0285 search|
|palette|accent|-|0310 origin|
|picker|accent|-|0104 badge.provider|
|picker|accent|bold|0093 filter.match; 0101 row.match|
|picker|border|-|0088 border; 0109 detail.divider; 0115 scrollbar|
|picker|disabled|-|0099 row.disabled; 0107 badge.unavailable|
|picker|focus|bold|0095 group.tab.selected|
|picker|muted|-|0092 filter.placeholder; 0094 group.tab; 0100 row.description; 0102 badge; 0105 badge.effort; 0110 detail.label; 0112 detail.help; 0114 empty; 0116 footer|
|picker|primary|bold|0089 title; 0096 group.header|
|picker|selection|-|0098 row.selected|
|picker|success|bold|0103 badge.current|
|picker|surface|-|0087 background; 0090 filter.background; 0108 detail.background|
|picker|text|-|0091 filter.text; 0097 row.text; 0111 detail.value|
|picker|warning|-|0106 badge.unknown; 0113 detail.warning|
|profiles|attention|-|0312 attention|
|profiles|warning|-|0311 warning|
|providers|primary|bold|0322 title|
|queue|accent|-|0064 action|
|queue|muted|-|0063 index; 0065 overflow|
|queue|muted|bold|0060 label|
|queue|selection|-|0062 selected|
|queue|surface|-|0059 background|
|queue|text|-|0061 text|
|review|accent|-|0293 label.other|
|review|error|-|0291 label.error|
|review|muted|-|0294 label.muted|
|review|primary|bold|0287 header|
|review|selection|-|0288 selection|
|review|success|-|0290 label.success|
|review|text|-|0289 input|
|review|warning|-|0292 label.warning|
|rewind|accent|-|0298 accent|
|rewind|muted|-|0296 metadata|
|rewind|selection|-|0295 selected|
|rewind|warning|-|0297 warning|
|settings|selection|bold|0323 selection|
|side|error|-|0236 error|
|side|muted|-|0237 metadata|
|side|warning|-|0235 warning|
|status|accent|-|0053 voice.phase|
|status|accent|bold|0048 accent|
|status|attention|bold|0052 attention|
|status|background|-|0044 background|
|status|border|-|0047 separator|
|status|error|bold|0051 error|
|status|muted|-|0046 muted; 0054 voice.read_aloud; 0056 connection; 0057 context; 0058 cost|
|status|success|-|0049 success|
|status|text|-|0045 text; 0055 voice.target|
|status|warning|bold|0050 warning|
|tool|accent|-|0192 running|
|tool|background|-|0191 background|
|tool|error|-|0200 error|
|tool|error|bold|0194 failed|
|tool|muted|-|0195 muted; 0198 metadata|
|tool|primary|bold|0196 title|
|tool|success|-|0193 success|
|tool|text|-|0197 subject; 0199 output|
|transcript|accent|bold|0003 user.label|
|transcript|accent|underline|0020 link.text|
|transcript|background|-|0001 background|
|transcript|border|-|0023 quote.bar; 0025 rule; 0027 table.border|
|transcript|focus|bold|0012 search.current|
|transcript|muted|-|0006 system; 0007 timestamp; 0010 omission; 0021 link.destination; 0026 list.marker; 0230 notice|
|transcript|muted|bold|0009 reasoning.label|
|transcript|muted|italic|0008 reasoning; 0022 image.alt|
|transcript|muted|strikethrough|0019 strikethrough|
|transcript|primary|bold|0005 assistant.label; 0014 heading.1; 0015 heading.2; 0016 heading.3; 0028 table.header; 0228 banner|
|transcript|selection|-|0011 search.match; 0013 selection|
|transcript|text|-|0002 text; 0004 user.text; 0024 quote.text; 0029 table.cell|
|transcript|text|bold|0017 strong; 0229 command|
|transcript|text|italic|0018 emphasis|
|work|accent|-|0134 turn; 0147 agent.running|
|work|accent|bold|0142 task.running|
|work|attention|bold|0131 attention; 0146 agent.waiting|
|work|border|-|0139 divider|
|work|disabled|strikethrough|0144 task.cancelled|
|work|error|-|0150 agent.failed|
|work|error|bold|0137 verdict.failure|
|work|muted|-|0132 label; 0135 verdict.pending; 0141 task.pending; 0148 agent.queued; 0152 agent.metadata; 0154 overflow; 0155 help|
|work|primary|bold|0130 title; 0138 section|
|work|selection|-|0145 task.selected|
|work|success|-|0143 task.done; 0149 agent.done|
|work|success|bold|0136 verdict.success|
|work|surface|-|0129 background|
|work|text|-|0133 goal; 0140 task.text; 0151 agent.name|
|work|warning|-|0153 agent.unknown|
|workflow|muted|-|0301 metadata|
|workflow|selection|-|0300 selected|
|workflow|text|-|0299 text|
|workflow|warning|-|0302 warning|
|workspace|accent|-|0158 overlay.border|
|workspace|background|-|0156 background|
|workspace|border|-|0157 divider|
|workspace|muted|-|0159 empty|


## Offline rendered screens and style callouts

All frames below come from `ratatui::backend::TestBackend`, not a terminal or gateway. Coordinates are zero-based (column,row). Blank trailing cells are omitted from text, but buffers retain their full dimensions. Plain Markdown cannot display cell colors: the callouts and swatch identifiers preserve their exact mapping.

### Ember / dark warm high contrast

Role colors below are the actual preview colors. Element backgrounds inherit their area: background for transcript/composer/status/tool/diff; surface for menu/work/dialog/picker/lists/code/queue/alerts/badges and alternate views. Selection and focus use the accent background and background foreground, or terminal reverse in Terminal. Attributes come from the catalogue.

|Role|Foreground|Base background|
|---|---|---|
|background|#f5eadb|#171411|
|surface|#f5eadb|#25201b|
|text|#f5eadb|#171411|
|muted|#c4b5a2|#171411|
|disabled|#a19383|#171411|
|border|#9c846b|#171411|
|primary|#ffc078|#171411|
|accent|#ffb86b|#171411|
|success|#a8d08d|#171411|
|warning|#ffd166|#171411|
|error|#ff8b82|#171411|
|attention|#ffd166|#171411|
|selection|#171411|#ffb86b|
|focus|#171411|#ffb86b|

#### 80x24: menu on, panel opened on demand as overlay

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                               │Work / 1 needs you
 You: Keep the interface clean and light.      │Goal: keep the interface light
 Talaria                                       │Turn: running / compare themes
 A few overrides can change the whole theme.   │Verdict: pending
 Theme preview                                 │Tasks 1/3
 Reasoning: checking readable contrast.        │[x] Read inputs
 Read docs/themes.md                           │[>] Compare themes
 rust / sample.rs                              │[ ] Review screens
 let theme = palette.resolve();                │Agents
 --- sample.rs                                 │! review: needs you
 - old_color                                   │  model: unknown / effort: ?
 + semantic_role                               │> preview: rendering
 ok render: six example workspace frames       │  model: unknown / effort: ?
 ! Preview only; decision pending.             │- next: queued
                                               │ok inventory: done
                                               │+2 more
                                               │Alt+S details / Alt+A next
                                               │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                   │
╰──────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.overlay.border` (47,1); `work.attention` (48,1); `work.verdict.pending` (48,4); `queue.text` (0,19); `composer.border.focused` (0,20); `composer.marker` (2,21); `status.voice.phase` (42,23); styles resolve through the catalogue and role table above.

#### 80x24: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016
0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032
0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064
0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080
0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112
0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128
0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160
0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176
0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208
0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224
0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256
0257 0258 0259 0260 0261 0262 0263 0264 0265 0266 0267 0268 0269 0270 0271 0272
0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304
0305 0306 0307 0308 0309 0310 0311 0312 0313 0314 0315 0316 0317 0318 0319 0320
0321 0322 0323



```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 16), y = floor((n − 1) / 16). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.

#### 120x40: menu on, panel docked

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                                                                       │Work / 1 needs you
 You: Keep the interface clean and light.                                              │Goal: keep the interface light
 Talaria                                                                               │Turn: running / compare themes
 A few overrides can change the whole theme.                                           │Verdict: pending
 Theme preview                                                                         │Tasks 1/3
 Reasoning: checking readable contrast.                                                │[x] Read inputs
 Read docs/themes.md                                                                   │[>] Compare themes
 rust / sample.rs                                                                      │[ ] Review screens
 let theme = palette.resolve();                                                        │Agents
 --- sample.rs                                                                         │! review: needs you
 - old_color                                                                           │  model: unknown / effort: ?
 + semantic_role                                                                       │> preview: rendering
 ok render: six example workspace frames                                               │  model: unknown / effort: ?
 ! Preview only; decision pending.                                                     │- next: queued
                                                                                       │ok inventory: done
                                                                                       │+2 more
                                                                                       │Alt+S details / Alt+A next
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                                                           │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts: `menu.brand` (0,0); `menu.item.focused` (28,0); `transcript.text` (1,4); `transcript.reasoning` (1,6); `diff.removed` (1,11); `diff.added` (1,12); `workspace.divider` (87,1); `work.attention` (88,1); `work.verdict.pending` (88,4); `queue.text` (0,35); `composer.border.focused` (0,36); `composer.marker` (2,37); `status.voice.phase` (42,39); styles resolve through the catalogue and role table above.

#### 120x40: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024
0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072
0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120
0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168
0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216
0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256 0257 0258 0259 0260 0261 0262 0263 0264
0265 0266 0267 0268 0269 0270 0271 0272 0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304 0305 0306 0307 0308 0309 0310 0311 0312
0313 0314 0315 0316 0317 0318 0319 0320 0321 0322 0323


























```

Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo 24), y = floor((n − 1) / 24). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.

### Paper / light

|Role|Foreground|Base background|
|---|---|---|
|background|#25231f|#fffcf5|
|surface|#25231f|#eee8dc|
|text|#25231f|#fffcf5|
|muted|#665d50|#fffcf5|
|disabled|#766a58|#fffcf5|
|border|#897a65|#fffcf5|
|primary|#714000|#fffcf5|
|accent|#005a80|#fffcf5|
|success|#285c2a|#fffcf5|
|warning|#704600|#fffcf5|
|error|#a01824|#fffcf5|
|attention|#704600|#fffcf5|
|selection|#fffcf5|#005a80|
|focus|#fffcf5|#005a80|

#### 80x24: menu on, panel opened on demand as overlay

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                               │Work / 1 needs you
 You: Keep the interface clean and light.      │Goal: keep the interface light
 Talaria                                       │Turn: running / compare themes
 A few overrides can change the whole theme.   │Verdict: pending
 Theme preview                                 │Tasks 1/3
 Reasoning: checking readable contrast.        │[x] Read inputs
 Read docs/themes.md                           │[>] Compare themes
 rust / sample.rs                              │[ ] Review screens
 let theme = palette.resolve();                │Agents
 --- sample.rs                                 │! review: needs you
 - old_color                                   │  model: unknown / effort: ?
 + semantic_role                               │> preview: rendering
 ok render: six example workspace frames       │  model: unknown / effort: ?
 ! Preview only; decision pending.             │- next: queued
                                               │ok inventory: done
                                               │+2 more
                                               │Alt+S details / Alt+A next
                                               │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                   │
╰──────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts use the same token coordinates as Ember at this size, resolved through this theme's color table and the catalogue attributes.

#### 80x24: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016
0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032
0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064
0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080
0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112
0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128
0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160
0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176
0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208
0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224
0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256
0257 0258 0259 0260 0261 0262 0263 0264 0265 0266 0267 0268 0269 0270 0271 0272
0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304
0305 0306 0307 0308 0309 0310 0311 0312 0313 0314 0315 0316 0317 0318 0319 0320
0321 0322 0323



```

Swatch coordinates and attributes follow the same catalogue mapping as the Ember atlas at this size; colors use this theme's role table.

#### 120x40: menu on, panel docked

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                                                                       │Work / 1 needs you
 You: Keep the interface clean and light.                                              │Goal: keep the interface light
 Talaria                                                                               │Turn: running / compare themes
 A few overrides can change the whole theme.                                           │Verdict: pending
 Theme preview                                                                         │Tasks 1/3
 Reasoning: checking readable contrast.                                                │[x] Read inputs
 Read docs/themes.md                                                                   │[>] Compare themes
 rust / sample.rs                                                                      │[ ] Review screens
 let theme = palette.resolve();                                                        │Agents
 --- sample.rs                                                                         │! review: needs you
 - old_color                                                                           │  model: unknown / effort: ?
 + semantic_role                                                                       │> preview: rendering
 ok render: six example workspace frames                                               │  model: unknown / effort: ?
 ! Preview only; decision pending.                                                     │- next: queued
                                                                                       │ok inventory: done
                                                                                       │+2 more
                                                                                       │Alt+S details / Alt+A next
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
                                                                                       │
 Queued 1: compare the light variant
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Keep the defaults quiet.                                                                                           │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts use the same token coordinates as Ember at this size, resolved through this theme's color table and the catalogue attributes.

#### 120x40: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024
0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072
0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120
0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168
0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216
0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256 0257 0258 0259 0260 0261 0262 0263 0264
0265 0266 0267 0268 0269 0270 0271 0272 0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304 0305 0306 0307 0308 0309 0310 0311 0312
0313 0314 0315 0316 0317 0318 0319 0320 0321 0322 0323


























```

Swatch coordinates and attributes follow the same catalogue mapping as the Ember atlas at this size; colors use this theme's role table.

### Terminal / ANSI

|Role|Foreground|Base background|
|---|---|---|
|background|default|default|
|surface|default|default|
|text|default|default|
|muted|Gray|default|
|disabled|DarkGray|default|
|border|Gray|default|
|primary|Yellow|default|
|accent|Cyan|default|
|success|Green|default|
|warning|Yellow|default|
|error|Red|default|
|attention|Yellow|default|
|selection|default|default|
|focus|default|default|

#### 80x24: menu on, panel opened on demand as overlay

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                               |Work / 1 needs you
 You: Keep the interface clean and light.      |Goal: keep the interface light
 Talaria                                       |Turn: running / compare themes
 A few overrides can change the whole theme.   |Verdict: pending
 Theme preview                                 |Tasks 1/3
 Reasoning: checking readable contrast.        |[x] Read inputs
 Read docs/themes.md                           |[>] Compare themes
 rust / sample.rs                              |[ ] Review screens
 let theme = palette.resolve();                |Agents
 --- sample.rs                                 |! review: needs you
 - old_color                                   |  model: unknown / effort: ?
 + semantic_role                               |> preview: rendering
 ok render: six example workspace frames       |  model: unknown / effort: ?
 ! Preview only; decision pending.             |- next: queued
                                               |ok inventory: done
                                               |+2 more
                                               |Alt+S details / Alt+A next
                                               |
 Queued 1: compare the light variant
+------------------------------------------------------------------------------+
| > Keep the defaults quiet.                                                   |
+------------------------------------------------------------------------------+
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts use the same token coordinates as Ember at this size, resolved through this theme's color table and the catalogue attributes.

#### 80x24: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016
0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032
0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064
0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080
0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112
0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128
0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160
0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176
0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208
0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224
0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256
0257 0258 0259 0260 0261 0262 0263 0264 0265 0266 0267 0268 0269 0270 0271 0272
0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304
0305 0306 0307 0308 0309 0310 0311 0312 0313 0314 0315 0316 0317 0318 0319 0320
0321 0322 0323



```

Swatch coordinates and attributes follow the same catalogue mapping as the Ember atlas at this size; colors use this theme's role table.

#### 120x40: menu on, panel docked

```text
Talaria   Session   Model   View   Voice   Settings   Help
                                                                                       |Work / 1 needs you
 You: Keep the interface clean and light.                                              |Goal: keep the interface light
 Talaria                                                                               |Turn: running / compare themes
 A few overrides can change the whole theme.                                           |Verdict: pending
 Theme preview                                                                         |Tasks 1/3
 Reasoning: checking readable contrast.                                                |[x] Read inputs
 Read docs/themes.md                                                                   |[>] Compare themes
 rust / sample.rs                                                                      |[ ] Review screens
 let theme = palette.resolve();                                                        |Agents
 --- sample.rs                                                                         |! review: needs you
 - old_color                                                                           |  model: unknown / effort: ?
 + semantic_role                                                                       |> preview: rendering
 ok render: six example workspace frames                                               |  model: unknown / effort: ?
 ! Preview only; decision pending.                                                     |- next: queued
                                                                                       |ok inventory: done
                                                                                       |+2 more
                                                                                       |Alt+S details / Alt+A next
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
                                                                                       |
 Queued 1: compare the light variant
+----------------------------------------------------------------------------------------------------------------------+
| > Keep the defaults quiet.                                                                                           |
+----------------------------------------------------------------------------------------------------------------------+
 Model unknown / effort ? | 1 needs you | IDLE +read off
```

Callouts use the same token coordinates as Ember at this size, resolved through this theme's color table and the catalogue attributes.

#### 120x40: complete token swatch atlas

```text
0001 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024
0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048
0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072
0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0092 0093 0094 0095 0096
0097 0098 0099 0100 0101 0102 0103 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120
0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144
0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168
0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0181 0182 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192
0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216
0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 0239 0240
0241 0242 0243 0244 0245 0246 0247 0248 0249 0250 0251 0252 0253 0254 0255 0256 0257 0258 0259 0260 0261 0262 0263 0264
0265 0266 0267 0268 0269 0270 0271 0272 0273 0274 0275 0276 0277 0278 0279 0280 0281 0282 0283 0284 0285 0286 0287 0288
0289 0290 0291 0292 0293 0294 0295 0296 0297 0298 0299 0300 0301 0302 0303 0304 0305 0306 0307 0308 0309 0310 0311 0312
0313 0314 0315 0316 0317 0318 0319 0320 0321 0322 0323


























```

Swatch coordinates and attributes follow the same catalogue mapping as the Ember atlas at this size; colors use this theme's role table.



## Verification and publication boundary

The offline renderer completed and produced six workspace frames plus six complete token atlases. Both focused tests passed. `cargo fmt --check` passed. Both example files passed TOML syntax/reference checks, and all 323 token names are unique. Ember's explicit palette has a minimum measured contrast of 4.55:1 against its background and surface. All Cargo execution used the operator-designated shared target after checking that the release script was absent. The release script restarted before the optional targeted Clippy check, so its guard withheld that command. Full-workspace tests, full-workspace lint, coverage and live-terminal checks were not run for this offline study.

The final #474 comment check found only its start/coordination note, with no picker study report yet; the filter/group/row/badge/detail hierarchy remains reserved. No gateway was accessed. The prototype is unmerged, no PR is opened and no build card is filed. Application integration and real-terminal acceptance remain for work authorized after the operator records a decision.

Public-text verification: `sh tools/release/doc-scan.sh` passed over the tracked documents and a temporary scan repository containing every study/prototype file as text. Additional attribution, machine-path, address and repository-name checks passed.
