# Themes

Talaria draws in the colours of the Hermes skin the profile already uses. It has no theme
catalogue of its own: with no skin it draws exactly as before skins were supported.

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

Talaria never paints the terminal's background or its default text colour: body text stays in
the terminal's own colour, and skin colours that are too faint for the background are lifted to
a readable contrast the way Hermes's own terminal interface does it.

## Commands

- `/skin` shows the profile's skin and how Hermes resolved it.
- `/skin <name>` sets the profile's Hermes skin (`display.skin`), as Hermes's own `/skin` does,
  so other Hermes windows on the profile change too.
- `/theme` shows the skin, the background, the colour depth and the motion setting, each with
  where it came from. `/theme light`, `/theme dark` and `/theme auto` change the background for
  this session only; nothing is written to disk.

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
