# Themes

Talaria follows the profile's Hermes skin by default. Choosing a theme file adds a palette,
semantic roles and element overrides. With no file selected, the existing appearance is unchanged.

## Theme files

Save a UTF-8 TOML file as `themes/<id>.toml` beside Talaria's `config.toml`. Files are shared
across profiles. The identifier starts with a lower-case letter or digit, followed by up to
63 lower-case letters, digits, underscores or dashes. Files cannot leave that folder, including
through symbolic links, and cannot exceed 256 KiB. The identifiers `terminal`, `dark`, `light`,
`ember` and `paper` are reserved for built-ins and cannot name user files.

This minimal theme changes the accent and the active composer frame:

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

Set `[theme] file = "quiet-amber"` in `config.toml` to select it at launch. A
`[theme.profiles."fixture"]` table with `file = "quiet-amber"` selects it for that profile.
Selection order is the session choice from `/theme use <id>`, the profile table, the global
`[theme] file`, then the existing Hermes skin behaviour. `/theme use` clears the session choice.
These commands never write configuration or Hermes files.

`schema = 1` is required. `extends` defaults to `terminal` and accepts only `terminal`, `dark`
or `light`, never another file or a URL. `name` is an optional display label. The twelve palette
slots are `background`, `surface`, `foreground`, `muted`, `disabled`, `border`, `primary`,
`accent`, `success`, `warning`, `error` and `attention`. Fourteen style roles use these colours:
`background`, `surface`, `text`, `muted`, `disabled`, `border`, `primary`, `accent`, `success`,
`warning`, `error`, `attention`, `selection` and `focus`.

A role table such as `[roles.accent]`, or an element table such as
`[elements."transcript.heading.1"]`, accepts `fg`, `bg`, `underline_color`, and the booleans
`bold`, `dim`, `italic`, `underline`, `reverse` and `strikethrough`. Omission inherits the field;
`false` clears an inherited attribute. Colours accept `"default"`, `"#RRGGBB"`, `"ansi:0"`
through `"ansi:15"`, and `"index:0"` through `"index:255"`. Roles and elements may reference
`"$palette.<slot>"`. Palette values are literal colours; references cannot chain. Unknown keys,
tokens, roles, slots, colour forms and values reject the whole file with the key, line and column.
There are no expressions, wildcards, scripts, environment substitutions or blinking attributes.

The [element catalogue](theme-tokens.md) lists all 323 names and their defaults. A selected file
styles the transcript, syntax highlighting, status, composer, tools, work panel, menus, pickers
and overlays. Element overrides can change one part of a view without changing its role peers.

The layers, from lowest to highest, are the built-in base, the eligible Hermes skin, the file's
palette, its roles, its element fields, and the terminal's rendering constraints. An authored
role or palette slot wins over skin values for its bound elements. Unrelated skin distinctions
remain. `follow_hermes_skin = false` in the file disables the bridge. The existing configuration
permission and colour-depth exclusions also apply.

`[chrome] border` accepts `rounded`, `single`, `double`, `ascii` or `none` and changes the
frames throughout Talaria. `none` replaces frame glyphs with spaces while preserving their
width. The active composer frame uses `composer.border.focused`. `section_divider` accepts
`single`, `double`, `ascii` or `none` for Talaria's section separators. Markdown rules and table
grids remain content and keep their glyphs. Themes never change motion, dimensions, keys or
action meanings.

The inline view does not paint the terminal's background or default body text colour. Without a
theme file, scheduled-job and workflow selections retain their existing dark grey backgrounds.
With a file selected, full-screen overlays and the workspace paint their whole owned area:
transcript, composer, status, tools, diffs and workspace use `background`; menus, work panels,
dialogs, pickers and alternate lists use `surface`. Inline surfaces omit backgrounds, including
selection backgrounds, so native scrollback keeps the terminal's background. The contrast
diagnostic uses a trusted startup background for inline text when available; otherwise it
reports the pair as unknown. Selection/focus pairs are measured against their paired
backgrounds. The diagnostic warns below 4.5:1 for text and 3:1
for structural marks, names the affected tokens, and never rewrites an authored colour. ANSI
palette colours and unknown terminal defaults cannot be measured reliably. Explicit colours
convert to the available depth: true colour, the nearest indexed colour, or the nearest of the
conventional sixteen colours. A non-empty `NO_COLOR` or `[theme] colors = "none"` removes all
foreground, background and underline colours while retaining attributes.

An invalid startup file gives one notice and uses the existing appearance. `/theme reload`
rereads the selected file manually; an invalid reload retains the last good theme. There is no
file watcher. New cells and chrome use a successful change. Streaming cells finish in their
starting theme, and finished native scrollback keeps its colours.

## Where the skin comes from

- At launch, from the `skin` of the `runtime.describe` result, which Hermes resolves for this
  connection's profile. The skin is applied before any history is written.
- Live, from `skin.changed` events (another window ran `/skin`, or the skin file changed). A
  replayed `skin.changed` after a reconnect is ignored; Talaria calls `runtime.describe` again
  after every reconnect instead, retrying once after a second if that call fails.
- A skin equal to the current one draws nothing. `{}` or no skin means Talaria's own colours.

A change recolours the composer, the status line, tool rows and cells that start after it. Lines already
in the terminal's scrollback keep their colours, and a reply that is streaming when the skin
changes finishes in the colours it started with.

## Light, dark and colour depth

Light or dark is decided once at start-up, first match wins: `/theme light|dark` for this
session, `TALARIA_THEME`, `[theme] mode`, the terminal's background colour (not when it reports
exactly black, which some terminals and tmux do when they do not know), its text colour,
`COLORFGBG`, `TERM_PROGRAM=Apple_Terminal`, then dark.

Colour depth: a non-empty `NO_COLOR` turns colour off, then `[theme] colors`, then `TERM=dumb`
(off), `COLORTERM` (`truecolor` or `24bit`), `TERM` containing `256color`, then 16 colours. On a
16-colour terminal, with `follow_hermes_skin = false`, or with colour off, the skin is not applied.
Colour off keeps bold, italic and underline.

On the default skin path, body text keeps the terminal's own colour and skin colours that are
too faint for the background are lifted to a readable contrast as in Hermes's terminal interface.
Theme files instead preserve their authored colours and report contrast warnings. Inline
surfaces do not paint backgrounds.

## Commands

- `/skin` shows the profile's skin and how Hermes resolved it.
- `/skin <name>` sets the profile's Hermes skin (`display.skin`), as Hermes's own `/skin` does,
  so other Hermes windows on the profile change too.
- `/theme` shows the skin, the background, the colour depth and the motion setting, each with
  where it came from. `/theme light`, `/theme dark` and `/theme auto` change the background for
  this session only; nothing is written to disk.
- `/theme use <id>` selects a file for this session. `/theme use` returns to the configured
  selection. `/theme reload` rereads the active file. When a file is selected, `/theme` also
  reports its identifier, base, skin participation, per-field sources and warnings. Without a
  selected file, the report keeps its existing five lines.

## Reduced motion

`TALARIA_REDUCED_MOTION` or `[ui] reduced_motion = true` selects reduced motion; `/theme` shows
which is in force. Nothing in Talaria blinks, in either mode: no source may name a blink
attribute or write a blinking escape sequence.

The rule every component that draws follows, under reduced motion:

- No glyph or colour changes on a timer.
- Elapsed-time numbers and real progress may still update, at most once per second.
- Scrolling and overlay changes jump straight to their end state.

Under full motion the same is recommended. Any future animation must be off under reduced motion
and must not draw while nothing changes.

## Known limit

A skin file that writes a colour with an unquoted `#` (for example `#FFD700` without quotes) is
read by YAML as a comment, so Hermes sends the colour as `null`. Talaria's own skin code reads a
`null` or other non-string colour leniently: it skips that colour, keeps the rest of the skin,
and lists the key under `Ignored skin values` in `/theme`.

That leniency starts at the skin Talaria reads from the describe result. The launch handshake
decodes `runtime.describe` through the generated contract types before that, and those types
reject a `null` colour. So in this release a profile whose skin has an unquoted `#` colour stops
Talaria at the handshake and does not launch; the fix is to quote the colour in the skin file.
Making the handshake decode skip such colours belongs to the contract types, and is left for a
later release.
