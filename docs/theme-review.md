# Talaria theme review

On November 24, 2026, all 15 built-in themes kept text and structural marks legible in
the screen checks below. Homebrew uses a green-on-black canvas with charcoal
panels and blue, cyan and purple accents. The theme guide
shows the same snippet and question in every palette. [Compare the themes](themes.md)

## Homebrew screenshot comparison

The three terminal screenshots supplied after the original review show several
distinct greens. Talaria uses their `#00ff00` for body text and `#009900` for the
composer wire. Against black these measure 15.30:1 and 5.56:1. Supporting text uses
`#0ca50c` so it also reads on the charcoal panels. Inline code uses the sampled blue
`#63a8f8`, tool names and provider headings use cyan `#7dd3fc`, and warnings use
gold `#c4a767`. Focused answers use mint `#4ade80` with a pointer and underline.

The conversation footer uses the reference goal label's purple `#b200b2` for the model
and effort. It measures 3.54:1 against black. The brighter purple `#d65cff` from
the transcript examples marks emphasis and shell operators and measures 6.83:1 on black.
The earlier Homebrew screen checks measured at least 5.05:1 for text in true colour,
4.94:1 in 256 colours and 5.25:1 in 16 colours. Those figures predate the footer change.

## Screen coverage

The review rendered 91 screen and state combinations with every theme through Talaria's
Rust renderers. It checked the text and cell positions across themes, including key hints,
selected answers, warning labels, search matches and workflow outcomes. Every frame ran
in true colour, 256 colours, 16 colours and with colour disabled. The Terminal theme also
ran with a light reference terminal. The configuration loading screen was present in
the first capture archive and missing from the repeatable test. The final run exercised
it through the configuration renderer at all four colour settings. The named colours
of a user's terminal and the appearance of dim or bold text depend on the emulator and font.

| Area | States reviewed |
| --- | --- |
| Chat | Bordered composer, completion, inline theme and effort pickers, mentions, queued draft, streaming Markdown, syntax, diffs, tools, working and recording indicators, status, side and background activity |
| Questions | Approval, masked entry, single choice, multiple choice, and a typed alternative |
| Navigation | Workspace, menu, dropdown, work panel, task list, transcript, history and transcript search, session directory, fleet list and peek, subagent dock and explorer |
| Work | Workflow picker, outline, selected agent and wide graph with mixed outcomes, kanban board and card detail, scheduled jobs, code review, rewind, and launch |
| Configuration | Settings browse, filter, edit, review, discard and help, configuration prompts, memory, diagnostics, model roles and route choice, model picker and no results, provider tabs and setup card, profiles and confirmation |
| Integrations | Capabilities and their four tabs, connector list, messaging, detail and edit, tool server catalogue and details, context usage |

In the running application, every theme went through selection, preview, cancellation,
Settings, the model picker and the transcript. The colour run sampled actual foregrounds
and panel backgrounds. A separate pass exercised the same navigation with colour disabled.
The custom-file check selected a temporary theme, changed the composer wire colour on
reload, and verified that an invalid edit preserved the last valid colour.

OpenCode Go rejected the original `muse-spark-1.3-contributor-free` identifier. With
operator approval, the review used `muse-spark-1.3-contributor` through Talaria.
Hermes reported `max` for both session and provider effort. All 15 themes went through
the live navigation sequence again with that model selected.

Muse asked “Which swatch?” through Hermes. The question pane opened with three
choices. Moving to Violet and pressing Enter produced `SWATCH=Violet` in chat.
In a fresh Homebrew session, Muse opened a multi-select question. Its focus marker
rendered in the expected `#39ff14`. Selecting Violet and Cyan with the arrow keys
and Space, then pressing Enter, sent both answers to Hermes. Muse replied `ACK [VC]`.

The final coverage run passed all 5,600 tests it ran and measured 83.72%
line coverage. The runner flagged an unrelated status-command shutdown test for a leak.
Its assertions passed, and the engineering journal records the warning for investigation.

## Independent review

Muse reviewed the theme comparison image with Hermes's vision tool and used a
local script to inspect the rendered cells across all 15 themes. It found the
workflow outcomes easy to distinguish because each colour came with a symbol and
a word, such as `✓ done`. In Homebrew, the same green marked both the draft and
the wire around it. An inline answer could also clear the text contrast threshold
and still look too much like its neighbour.

| Reviewed surface | Result of the original review |
| --- | --- |
| Homebrew composer | The draft keeps the reference green `#39ff14` on black at 15.49:1. The focused wire uses a softer green, `#6eae62` at 7.89:1. |
| Homebrew questions | The focused answer uses the reference green, a pointer, bold text and an underline. The other answers use the reference grey `#8a8f88` at 6.37:1. |
| Terminal questions and commands | The active row reverses the emulator's own text and background. The installed command picker displayed reverse video and an underline on the selected command. |
| Rosé Pine Dawn questions | The selected iris colour deepened from `#7a6196` to `#73598f`, raising its contrast on the cream background from 4.83:1 to 5.41:1. |

Muse checked the final 1,365 grids again. The lowest contrast it measured on any
run containing a letter or digit was 4.67:1 in Paper's empty model picker.
The rendering tests below also cover punctuation, focus markers and structural
lines. The review reflects one model's aesthetic judgement and the terminal
colour references used for the measurements.

## Measured contrast

These are the lowest foreground-to-background ratios among the visible text and
structural marks in the true-colour captures. The review used a matching native
background for inline output. The Terminal row uses conventional named colours on
black. The checks require 4.5:1 for text and 3:1 for structure at each measured depth.

| Theme | Text | Structure |
| --- | ---: | ---: |
| Homebrew (screenshot update) | 5.05:1 | 4.41:1 |
| Catppuccin Mocha | 4.85:1 | 3.22:1 |
| Catppuccin Latte | 4.81:1 | 3.20:1 |
| Tokyo Night | 5.04:1 | 3.25:1 |
| Dracula | 4.81:1 | 3.28:1 |
| Nord | 4.81:1 | 3.27:1 |
| Gruvbox Dark | 4.80:1 | 3.20:1 |
| Gruvbox Light | 4.81:1 | 3.55:1 |
| Rosé Pine | 5.25:1 | 3.23:1 |
| Rosé Pine Dawn | 4.81:1 | 3.52:1 |
| Solarized Dark | 4.80:1 | 3.21:1 |
| Solarized Light | 4.80:1 | 3.21:1 |
| Ember | 5.39:1 | 4.55:1 |
| Paper | 4.67:1 | 3.66:1 |
| Terminal | 5.01:1 | 5.01:1 |

The first pass found faint secondary text after 256-colour conversion, and selections
that disappeared when inline rows lost their fill. In the final captures, Homebrew's
active answer had a green marker and underline. Dracula's filled selection stood out in
dense tables. Paper and the light community variants kept dark text visible on their
secondary panels. The monochrome views kept their selection markers and navigation hints.

Run the cell checks with `cargo test -p talaria-tui --test integration theme_readability -- --nocapture`
to print the weakest measured pairs.
The engineering journal records two follow-ups from the live review, covering the long
theme diagnostic and the file path shown for bundled themes. [Queued work](engineering-journal/QUEUED.md)
