---
title: v0.3 unit C2 — a filterable slash-command palette on `/`
type: plan
status: proposed
date: 2026-08-12
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: C2
---

# Unit C2 — a filterable slash-command palette on `/`

**In Talaria, typing `/` at the start of an empty composer opens a non-modal filtered palette that stays attached to the composer, filters the live slash-command inventory as the operator types, and inserts the chosen command name on selection without submitting it.** The operator asked for it in the simplest possible words (`docs/analysis/2026-08-10-v0-2-hands-on-notes.md:336-343`): "type '/' allows scrolling/filtering on all possible slash commands like most other TUI's like this". This plan turns that one sentence into a complete interaction specification, reuses the inventory Talaria already reads from the gateway, and states exactly where the palette diverges from the existing browse-only listing and where it shares its seam with the parallel history-recall work.

The plan follows the shape of the merged B1 plan (`docs/plans/2026-08-11-v0-3-unit-b1-caret-status-row.md`): mechanism verified by reading, design space with options weighed and rejected, numbered key technical decisions, risk, numbered acceptance evidence, verification, and an explicit boundary of what this unit does not do.

## The requirement, in the operator's own words

From `docs/analysis/2026-08-10-v0-2-hands-on-notes.md:336-343` (note 12):

> type "/" allows scrolling/filtering on all possible slash commands like most other TUI's like this.

Note 12 pairs this with up-arrow history (`:337-339`), which is unit C1 (planned in parallel by a different session). The planner for that note grouped both under "The composer needs the conventions every other terminal interface has" (`:335-336`). The requirement is therefore not a niche shortcut — it is the expected discovery surface for a command set the operator otherwise has to guess at. The browse-only palette that exists today requires pressing `F3` and shows an unfiltered listing; it satisfies "see what exists" but not "find it while you type". This unit closes that gap without replacing the browse listing.

## Mechanism — verified by reading, at `main` = `d56eb09`

All line numbers below were opened in this worktree before being cited. A citation not opened is not a citation.

**The gateway inventory is already read once and never guessed.** `talaria/ui/app.py:3438` (`async def load_catalog(self) -> CommandCatalog:`) begins with the docstring "Read the gateway's slash inventory once, and never guess at it." It calls the gateway method named `commands.catalog` (`talaria/domain/commands.py:118` and `talaria/domain/compat.py:118` both pin `method="commands.catalog"` with `classification="read-only"`, `evidence="tui_gateway/methods_tools.py:255-367"`). The reply is decoded by `decode_catalog` (`talaria/domain/commands.py:291`), whose first line is `entries: list[CommandEntry] = list(_local_entries())`. An unavailable reply is not rendered as empty; it is rendered as `unavailable_catalog` (`talaria/domain/commands.py:336` `def unavailable_catalog(reason: str) -> CommandCatalog:`) which still carries `entries=_local_entries()`. The docstring at `talaria/ui/app.py:3442` says why: "A call that fails leaves an *unavailable* catalogue rather than an empty one, and the difference is the whole of AE9's honesty clause". The fetch is started at mount (`talaria/ui/app.py:1150` area `self.fetch_catalog()`) and rendered through `await self.palette.apply(self.catalog)` (`talaria/ui/app.py:3486` via `render_catalog` at `talaria/ui/app.py:3438-3486`).

**The local inventory already shadows the gateway.** `talaria/domain/commands.py:350` defines `class LocalCommand:` ("One control Talaria performs itself, with no socket involved.") and `talaria/domain/commands.py:364` defines `TALARIA_LOCAL_COMMANDS` as a seven-element tuple: `/quit`, `/pause`, `/resume`, `/speed`, `/models`, `/profiles`, `/sessions`. The module docstring at `talaria/domain/commands.py:21-28` names this closed set and says the local set is "resolved *before* the catalogue on every lookup". `talaria/domain/commands.py:434` (`def _local_entries() -> tuple[CommandEntry, ...]:`) projects each local command as a `CommandEntry` with `availability="talaria-local"` and category `Talaria`. `talaria/domain/commands.py:291` seeds every decoded catalogue with that set, so even an unavailable catalogue shows them. `talaria/domain/commands.py:150` (`CLIENT_LOCAL_NAMES: frozenset[str] = frozenset({"/density", "/logs", "/mouse", "/sessions"})`) and `talaria/domain/commands.py:341` (`def _is_client_local(...)`) mark the Hermes client-local extras as `unsupported` rather than dispatchable. The fixed ordering in `talaria/ui/app.py:2789` (`def on_chat_text_area_submitted(self, message: ChatTextArea.Submitted) -> None:`) is: `LocalInvocation` first, then `UnsupportedInvocation`, then `GatewayInvocation` via `dispatch_command_live` (`talaria/ui/app.py:4008`), then plain message submit. The same ordering appears at `talaria/ui/app.py:3488` (`def perform_local_command(self, invocation: LocalInvocation) -> None:`).

**The existing listing is a foldable non-modal region, not a dialog.** `talaria/ui/palette.py:74` (`class PaletteRegion(Vertical):`) is named on purpose to avoid colliding with Textual's (the terminal framework) own `CommandPalette`. Its CSS at `talaria/ui/palette.py:82-89` is `display: none` unless `-showing`. `talaria/ui/palette.py:44` (`NOT_YET_FETCHED = "commands: not fetched yet"`) is the header when `catalog is None`; `talaria/ui/palette.py:66` (`return NOT_YET_FETCHED`) handles that case. `talaria/ui/palette.py:113` renders that string while `talaria/ui/app.py:1072` yields the region as `yield PaletteRegion(id="palette")` between `PromptRegion` and `StatusRegion` inside `Vertical(id="body")` (`talaria/ui/app.py:1067-1072`). `talaria/ui/app.py:2847` (`async def action_toggle_palette(self) -> None:`) merely toggles visibility; the listing costs no focus, holds no keys, and is read-only.

**The composer is the only widget that should accept typing.** `talaria/ui/composer.py:50` (`PLACEHOLDER: Final[str] = "Message  ·  Enter sends  ·  Ctrl+J newline"`) documents the two bindings, and `talaria/ui/composer.py:129-159` defines `class Composer(Vertical):` with `Composer > .composer--notice { height: 1; text-wrap: nowrap; text-overflow: ellipsis; }`. That CSS carries the explicit warning at `talaria/ui/composer.py:146-160` that the row is routinely too narrow and `text-overflow: ellipsis` only works with wrapping off. `talaria/ui/app.py:4213` (`_DISCARD_NOTICE_BY_REGION: Final[Mapping[str, str]] = {`) and `talaria/ui/app.py:4228` (`def _no_text_region(self) -> str | None:`) implement the B1 discard notice that fires when a printable key reaches `on_key` (`talaria/ui/app.py:4452`) or a paste reaches `on_paste` (`talaria/ui/app.py:4473`) while focus is in a no-text region. `talaria/domain/commands.py:143` (`Availability = Literal["dispatch", "talaria-local", "unsupported"]`) and `talaria/ui/palette.py:53` (`def format_entry(entry: CommandEntry) -> str:`) define how the marker column renders (`local`, `unsupported`, or blank). `platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md` forbids the domain from importing the terminal framework, so filtering and selection state must live in the presentation layer and consume `CommandCatalog` rather than driving it.

In short: the catalogue exists, it already merges gateway entries and the seven local entries, it degrades honestly, and the screen already has a non-modal place to show a list. The slash palette is a filtered mode of that surface, not a new data fetch and not a parallel listing.

## The design space — what was weighed and why only one survives

### 1. What opens the palette

**A `/` anywhere — rejected.** The composer accepts prose, pasted code, URLs (uniform resource locators), and file paths. A slash appears inside all of them. Opening a palette on any `/` means the operator's next Up/Down rebinds mid-sentence: the key that should move the caret inside `TextArea` (the terminal framework's multi-line editor) now moves a highlight. The hands-on note's own phrasing ("type '/' allows scrolling/filtering" at `docs/analysis/2026-08-10-v0-2-hands-on-notes.md:340`) is written about an empty composer, not about prose; reading it as "anywhere" invents the worse problem the note never asked for.

**A `/` only after whitespace or at column zero, at any time — rejected as still too eager.** This still fires on `/tmp/file` typed as a path argument to a message ("see /tmp/foo") and on inline `/command` typed to quote a command without running it. Those cases are rarer than a bare leading `/`, but they are real pastes that already live in the transcript ecology Talaria records.

**A `/` as the first non-whitespace character of an otherwise empty composer, or a `/` that becomes so after a single leading-`/` edit — chosen.** The predicate is: the composer holds the caret, its stripped text matches `^/[A-Za-z0-9_-]*` (the same character class `talaria/domain/commands.py:118`'s `_COMMAND_LINE` permits), and the slash is at column zero ignoring leading whitespace. An operator who types `/` into an empty box gets the palette immediately; one who types prose containing slashes never does. Leading whitespace is tolerated because operators often paste with a leading space, but `/ hello` (slash, space, word) is not a command prefix and the palette does not open on it. Editing away from the predicate (typing a space after the slash, deleting the slash, or moving the caret) closes the palette with no side effect.

This mirrors the dispatch predicate in `talaria/domain/commands.py:291` and the existing `parse_command_line` boundary ("A command is a slash, a name, and then either end-of-input or whitespace" at `talaria/domain/commands.py` near `:420`), so what the palette filters and what `on_chat_text_area_submitted` (`talaria/ui/app.py:2789`) will dispatch are the same language rather than two similar ones.

### 2. What closes it, and what happens to the `/` and the filter text

**Candidates:** pressing `Escape` (often called Esc), deleting the leading `/`, typing a space that breaks the command prefix, submitting the line, moving focus away, or selecting a row.

The palette closes on any of those, and the text policy is uniform: **the composer text is never cleared or rewritten on close except when a row is selected.** An `Escape` or a blur leaves `/foo` exactly as typed — the operator may have been drafting a command they decided not to run, and discarding their draft would be the same class of silent loss B1's R5' (`talaria/ui/app.py:4213` region notices) exists to prevent. A dismiss therefore does exactly one thing: it hides the palette and returns key routing to normal. A select inserts (KTD6) and then hides.

A space typed after a prefix also closes, because a space ends a command name in Hermes's own handler (the `_COMMAND_LINE` boundary above). The operator typed `/models ` with a trailing space; the command word is complete and the palette has nothing left to filter. Keeping it open would filter on the argument (`default`), which the gateway's slash worker does not filter by — the catalogue's argument shapes are per-command and not part of the name index.

### 3. Whether the palette is modal

The brief flags an anti-modal lineage: the original model picker was a foldable numbered list (`docs/engineering-journal/QUEUED.md:181` "The model picker is a numbered list, not a picker — KTD3's anti-modal decision needs reopening") precisely because `talaria/ui/palette.py:1-22` rejected a modal search box for a listing on the grounds that it "would put a second focus owner in front of the composer". That reasoning survived for the listing (`talaria/ui/palette.py:6-14` still says the rows are folded away and "the transcript is worth more rows than it is"). It was overturned for the pickers because operating needs keys: `docs/engineering-journal/DECISIONS.md:684` records that the picker became a modal dialog where "arrows move, `enter` selects, typing filters, `escape` backs out" and the foldable `PickerRegion` was removed.

**This unit does not overturn that distinction — it follows it.** A command listing is something the operator reads; a picker is something the operator operates. A slash palette is both: the operator keeps typing to filter it, which is exactly the behaviour a modal dialog would steal. Making the palette modal would mean the dialog owns the keyboard while it is open, so characters typed to filter would not reach the composer — the place the gateway will actually dispatch from — and the composer would have to mirror a second text buffer to keep them in sync. That is worse than keeping the composer as the sole focus owner and making the palette a filtered view that follows it.

So the palette is **not modal**. The composer retains focus at every instant, the palette never calls `focus()`, and no `Textual` `Screen` is pushed. Up/Down/Enter/Escape are intercepted at the application level only while the palette is open (KTD7), but they never leave the composer: the `TextArea` still holds the caret, and dismissing the palette restores its ordinary key routing with no focus move. Where this diverges from the pickers is deliberate and is the same divergence the original file already named: reading versus operating, and for the palette the operating is done by continuing to type in the same widget.

**Rejected — a modal overlay like the pickers.** It would give `Enter` unambiguously to "select" (the collision that motivated the picker modal), but for the slash palette `Enter` as "submit the composed line" and `Enter` as "select the highlighted row" are not colliding by accident — they are the same verb at two stages. The slash palette resolves the collision by not submitting on select at all (KTD6), which keeps `Enter` meaning "finish the line in the composer" whether the palette is open or not: with a palette open it inserts, without it submits. A modal would need a second buffer and a second `Enter` meaning, and would hide the transcript behind a layer for a command list that is at most fourteen rows (`talaria/ui/palette.py:85` `max-height: 14`). The cost — a second focus owner in front of the composer, the exact thing `talaria/ui/palette.py:1-22` warns against — is paid for nothing the non-modal cannot do.

**Rejected — a second focus owner that is not a Screen but still steals `Enter`.** Same collision, smaller surface, same cost. The composer would lose `Enter` as submit while the palette is open, which is the one key the `PLACEHOLDER` (`talaria/ui/composer.py:50`) promises will send the message.

### 4. What the inventory looks like before the gateway answers, or when it refuses

**Talaria asks the gateway once.** `talaria/ui/app.py:3438` runs at mount; `talaria/domain/compat.py:118` pins that method as `read-only` so probing is permitted, while `talaria/domain/compat.py:336` guards evidence-only methods from being probed at all (R34). The choice is therefore load-bearing: the palette has to be honest about which inventory it is showing.

**Before the reply arrives (`catalog is None`).** The palette opens. It shows the seven local entries (`talaria/domain/commands.py:364`) and the header `NOT_YET_FETCHED` (`talaria/ui/palette.py:44`) with the degraded line hidden. No rows appear beyond the local seven; there are no gateway rows to show and no failure to name. This is the same honesty the existing `header_line` (`talaria/ui/palette.py:59-71`) already implements: `if catalog is None: return NOT_YET_FETCHED`. The operator who opens the palette before the fetch lands gets a usable, if short, list rather than an empty box.

**When the gateway has no inventory to give (`catalog.available is False`).** The palette opens and shows the local seven plus the degraded line `CATALOG_FAILURE_PREFIX + failure` (`talaria/ui/palette.py:46` and `talaria/ui/palette.py:168-172` `if not catalog.available: return f"{CATALOG_FAILURE_PREFIX}{catalog.failure}"`), which is also the line the F3 toggle shows. The refusal names the symptom the operator can act on (connectivity, credential, gateway down), and the local set remains because it never needed the gateway — an operator whose gateway is refusing can still type `/quit` (`talaria/ui/app.py:3446` comment: "an operator whose gateway is refusing calls can still type `/quit`"). This matches `unavailable_catalog` (`talaria/domain/commands.py:336`) and the `available=False` branch of `header_line`.

**Rejected — hiding the palette until the fetch lands.** That would make the discovery surface unavailable during the only interval when the operator is most likely to try discovery (first launch, first empty composer). A palette that opens with local entries is strictly more useful than one that says nothing.

**Rejected — synthesising a catalogue when the gateway has none.** No fake entries. The warning case (`catalog.warning` at `talaria/ui/palette.py:173`) already has its own degraded line (`CATALOG_WARNING_PREFIX` at `talaria/ui/palette.py:50`), and that line is shown in the palette's degraded row rather than silently trimming.

### 5. How filtering works, and what happens when nothing matches

**Prefix, case-insensitive, against the command name — chosen.** The filter lowercases the trimmed composer text after the leading `/` and keeps entries whose `name.lower()` starts with that prefix. An empty prefix (bare `/`) shows every runnable entry (local plus gateway dispatchable), ordered by `category` then `name` — the same grouping the F3 listing uses, so the two surfaces do not disagree about where `/models` lives. Matching touches only the name, not the description: a command typed as `/models` is looked up by name, and the description is never part of what `resolve_command` (`talaria/domain/commands.py:520-559`) matches. Adding description-substring matching would surface `/quit` when the filter is "leave", which is helpful for search and unhelpful for insertion — the palette's job is to complete what the operator started typing, not to guess what they meant.

**Rejected — substring (contains) matching.** It ranks `/quit` under filter `it`, `/sessions` under `ess`, and any command containing `a` under `a`. For slash completion that almost-typing is noise. The same filter in the picker (`talaria/ui/dialog.py:291` area) is substring because the picker filters provider and model *descriptions* as well as names; the palette has only names and one-line descriptions (`talaria/ui/palette.py:53` `format_entry`), so the analogous helpful surface is prefix on name.

**Rejected — fuzzy matching.** Fuzzy ranking floats unrelated names to the top on transpositions (`/mdoesl` matching `/models`) and its scores are not stable across small edits, which makes arrow navigation surprising. For roughly a hundred entries (`talaria/domain/compat.py:118` response shape's `skill_count: int` plus local seven), a full scan is trivial and prefix is the interaction operators expect from every comparable palette.

**When nothing matches.** The palette stays open, shows the header and degraded line (if any), and renders a single muted row "no matching commands" in place of entries. It does not close itself: auto-closing on zero matches would flicker on every backspace and would steal the operator's draft. The empty state is still keyboard-navigable in that there is no selection to move, and `Enter` does nothing except keep the composer text — there is no command to insert, and inserting a non-command would be fabricating a dispatch.

### 6. What selection does

**Inserts the command name for the operator to complete — chosen.** Selecting a row (by `Enter` or by click) replaces the composer's slash prefix with the chosen entry's canonical `name` followed by a single trailing space, places the caret at the end, leaves focus in the composer, and closes the palette. The text stays unsent. The operator may now type arguments, paste, or press `Enter` again to dispatch. For a plain message the palette is already closed (no leading slash), so `Enter` submits as it always has (`talaria/ui/composer.py:129-159` and `talaria/ui/app.py:2789`).

**Rejected — submit immediately on selection.** A command that runs on a single keystroke is a different risk from one that is merely typed for you. Several catalogue commands are destructive or modal (`/quit`, `/sessions` which shadows the gateway's own listing at `talaria/domain/commands.py:364` docstring KTD6, and any future gateway command that ships with side effects). Requiring a second `Enter` is the same two-stage rule the pickers use (`/models` inside the picker never sends on highlight, only on `Enter`), and it is the rule that keeps `/quit` from exiting on a mis-tapped `Enter`.

The insertion uses the `canon` map (`talaria/domain/commands.py:212` `CommandCatalog.canon`) so an alias inserted is the canonical spelling the gateway will receive, not the alias the operator filtered on. The marker column (`talaria/ui/palette.py:53` and `talaria/domain/commands.py:168` `AVAILABILITY_MARKER`) is shown in the browse listing but not in the filtered palette's narrow window: the filtered palette shows only runnable rows (`dispatch` and `talaria-local`), and `unsupported` rows are omitted entirely (next decision), so there is nothing to mark. When the operator later browses the full listing with `F3`, all three availabilities appear with their markers as before.

## Key technical decisions

### KTD1 — the filtered palette reuses the existing `PaletteRegion`, it does not add a second listing

`talaria/ui/palette.py:74` (`class PaletteRegion(Vertical):`) and `talaria/ui/app.py:1072` (`yield PaletteRegion(id="palette")`) already reserve a foldable region with a header, a degraded row, and one row per entry (`talaria/ui/palette.py:155-165`). The slash palette is a filtered mode of that widget: when the composer holds a slash prefix (KTD2), `PaletteRegion.apply` is called with the already-held `catalog` (`talaria/ui/app.py:3486`) plus a prefix filter, and `showing` is driven by the predicate rather than only by `F3`. When there is no prefix, `F3` continues to behave as today. There is no second `Vertical`, no duplicate `format_entry`, and no parallel header line.

Why this and not a new `SlashPalette` region: the catalogue is one object (`talaria/domain/commands.py:200` `class CommandCatalog`) and showing it in two places that can disagree about counts, degraded state, or ordering is the inconsistency the charter's evidence rule 2 ("a gate measures the claim it was built to measure") exists to prevent. Reusing the widget keeps `CommandCatalog.canon`, the degraded lines, and the ordering in one place. The only new code on `PaletteRegion` is a second entry point `apply_filtered(catalog, prefix)` that renders the same rows through `format_entry` (`talaria/ui/palette.py:53`).

Rejected — a new region mounted just above `Composer`. It would duplicate the header (`talaria/ui/palette.py:59`), the degraded handling (`talaria/ui/palette.py:168`), and the `format_entry` contract, and it would put two listings on screen when `F3` and the slash trigger overlap. The existing region's `max-height: 14` (`talaria/ui/palette.py:85`) already clips at a useful height; a second region would need the same clip and the same reasoning.

### KTD2 — open predicate: first non-whitespace character is `/` and the suffix matches the command-name class

Open when all of:

- the composer owns the caret (`self.focused is self.composer.text_area` or an ancestor walk that includes it),
- `self.composer.text.lstrip().startswith("/")`,
- the stripped text matches `^/[A-Za-z][A-Za-z0-9_-]*$` or `^/$` (bare slash) ignoring the leading whitespace, i.e., the same character class `_COMMAND_LINE` (`talaria/domain/commands.py:420`-ish `re.compile(r"^/([A-Za-z][A-Za-z0-9_-]*)(?:\s+(.*))?$")`) allows for a name, without the trailing argument group.

Close when any fails: a space after the name, deletion of the slash, non-prefix text, or focus leaving the composer. The mutation is observed at the presentation layer, not in the domain: a handler on `ChatTextArea` text change (or on `app.on_key` after `TextArea._on_key` has run) recomputes the predicate and calls `palette.apply_filtered` or `palette.apply`. Because the domain owns the catalogue and the UI owns the text, the domain boundary of `platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md` is untouched — the plan does not make `talaria/domain/commands.py` import the terminal framework.

### KTD3 — not modal, styled as an overlay that does not take focus

The palette never calls `focus()`, never pushes a `Screen`, and never steals `Enter`. While open it draws above the status region and below `PromptRegion`, exactly where `PaletteRegion` already mounts (`talaria/ui/app.py:1067-1072`). `TextArea` keeps the caret; the warning-coloured notice row (`talaria/ui/composer.py:145` `Composer > .composer--notice`) stays available for B1's discard notice (`talaria/ui/app.py:4213`). This is the `talaria/ui/palette.py:1-22` lineage taken literally: a listing is read, and the slash filter is a mode of that listing.

### KTD4 — keyboard claims while the palette is open, and what that demands of neighbours

When the palette is open and theComposer holds the caret, the palette claims `Up`/`Down`, `Enter`, `Escape`, and `Tab` at the application layer before they reach `TextArea`. The handler order is application before widget (Textual (the terminal framework) delivers `Key` to the `App` when `priority=True` or via `on_key` bubbling), so the claim is checked in `TalariaApp.on_key` (`talaria/ui/app.py:4452` is the B1 precedent) before `ChatTextArea._on_key` (`talaria/ui/composer.py:115`) inserts or submits. Outside that predicate the keys are untouched.

Consequences the plan states as claims a reviewer can check against unit C1:

- **Up and Down belong to the palette while it is open.** C1 (up-arrow history recall) may not consume `Up` when `palette.showing and _slash_active`. When the palette is closed, C1 owns `Up` as specified by the other plan. `Down` is similarly claimed only while open; otherwise it stays with `TextArea`'s caret movement.
- **Enter while open inserts; while closed submits.** This is KTD6, but the claim matters here: C1 must not interpret `Enter` as history acceptance while the palette is open.
- **Escape while open closes the palette and consumes the event.** While closed, `Escape` remains with the prompt card (`talaria/ui/prompts.py:886` `action_decline`) and the pickers. The palette's `Escape` never reaches them.
- **Tab is consumed to keep focus in the composer while open.** B1's discard latch already uses `Tab` semantics to return focus (`talaria/ui/app.py:4228` ancestor walk). Allowing `Tab` to move focus to `TranscriptPane` while the palette is open would leave the palette showing for a composer that no longer holds the caret — a state the predicate immediately closes, but only after a frame of visual mismatch.

Implementation note: the claim is a narrow `if _slash_active:` gate at the top of `on_key`, not a global `BINDINGS` entry, so that `Chooser` tests can drive the palette without pretending to type through a modal.

### KTD5 — inventory shown before the fetch or on failure, and the local-vs-gateway distinction

- **Before the fetch (`catalog is None`):** header `NOT_YET_FETCHED` (`talaria/ui/palette.py:44`), no degraded line, rows are the seven local entries (`talaria/domain/commands.py:364`). Filter applies to those seven only.
- **On failure (`catalog.available is False`):** header counts are gateway `0` / local `7` / unsupported as decoded, degraded line `CATALOG_FAILURE_PREFIX + failure` (`talaria/ui/palette.py:46`), rows are local seven plus any gateway rows that arrived despite the failure (none, by construction of `unavailable_catalog` at `talaria/domain/commands.py:336`). Filtering includes whatever rows exist.
- **When available:** degraded line shows `warning` if present (`talaria/ui/palette.py:50` `CATALOG_WARNING_PREFIX`), otherwise hidden (`talaria/ui/palette.py:149-153` `set_class(bool(said), "-said")`). Filter includes local plus gateway `dispatch` rows; `unsupported` rows are omitted entirely from the filtered palette (they remain visible in the `F3` browse listing with marker `unsupported` at `talaria/domain/commands.py:168`).

Whether the palette distinguishes local from gateway: in the filtered palette, no marker, because every row is runnable and the distinction is not actionable while filtering. In the browse listing (`F3`), markers remain (`local` / `unsupported` / blank) at `talaria/ui/palette.py:53`. The palette's header always distinguishes: `header_line` (`talaria/ui/palette.py:59`) already counts by availability, and `apply_filtered` preserves it. So the operator who browses sees what runs where; the one who filters sees only what runs.

Rejected — showing `unsupported` as disabled rows in the filtered palette. Those rows advertise a command whose dispatch is defined to refuse (`talaria/ui/app.py:2789` `UnsupportedInvocation` branch). Surfacing them in a completion list is surfacing a dead end the operator cannot resolve by typing more.

### KTD6 — selection inserts the canonical name with a trailing space and never dispatches

`resolve_command` (`talaria/domain/commands.py:520`) already yields `GatewayInvocation` with `canon` and `wire_name`. The palette's select handler takes `entry.name`, canonicalises through `catalog.canonical(entry.name)` (`talaria/domain/commands.py:212`), writes `f"/{canonical} "` into `Composer.text` (`talaria/ui/composer.py:218`), places the caret at the end, closes the palette, and keeps focus in `ChatTextArea`. It does not call `perform_local_command` (`talaria/ui/app.py:3488`) or `dispatch_command_live` (`talaria/ui/app.py:4008`). A second `Enter` — the ordinary submit — then dispatches through the single path `on_chat_text_area_submitted` (`talaria/ui/app.py:2789`).

Click also selects by the same insert rule, with the same non-dispatch. `Textual` delivers `Click` to the mounted `Static` row; the palette maps click to the row's entry index and runs the insert.

Why trailing space: every gateway slash command that takes an argument does so after a space (`talaria/domain/commands.py:118`'s `_COMMAND_LINE` group 2), and `GatewayInvocation.slash_exec_command` (`talaria/domain/commands.py` near `:540`) recomposes as `f"/{wire_name} {arg}"` stripping the space when there is no arg. Giving the operator the slash, the canonical name, and one space puts the caret where the next character belongs and makes the round-trip test (`parse → compose` via `slash_exec_command`) stable.

### KTD7 — filtering is case-insensitive prefix on the name, with zero-match handling that keeps the palette open

Filter is `prefix = composer_trimmed[1:].lower()` (strip leading `/`, lower). Keep `entry` when `entry.name.lower().removeprefix("/").startswith(prefix)`. Ordering is `sorted(category, name.lower())` so `Talaria` locals group first and the browse order is stable. The filter runs on every text change while the predicate holds; it is O(number of entries) (at the pin about a hundred `CommandEntry` plus seven locals) and needs no debounce.

Empty prefix (`/` with no name) is not a distinct state — it keeps every runnable entry. A filter that yields no rows keeps the palette open with header, degraded line if any, and one muted row "no matching commands". No selection highlight exists in that state.

### KTD8 — the palette does not borrow the B1 notice row

The composer notice row (`talaria/ui/composer.py:146` `Composer > .composer--notice`) is height one with `text-overflow: ellipsis` (`talaria/ui/composer.py:159`) and its B1 content (`talaria/ui/app.py:4213` `press tab to return to the message box — …`) must survive at 80 columns (`talaria/ui/composer.py:146-160` truncation warning). The palette has its own degraded row (`talaria/ui/palette.py:98` `PaletteRegion > .palette--degraded`) and its own show/hide (`.-said`). The slash palette uses that row for degraded state and does not write into `Composer.notice`. So the transient typing-paused notice and the palette cannot overwrite each other, and the way-back clause B1 placed at the front of its message is not truncated by a palette that is open elsewhere. This is a stated seam with unit B1: B1 keeps the notice row unconditionally (`talaria/ui/composer.py:201`), and this unit does not suppress or reuse it.

## Risk this unit must clear

**A palette that guessed.** The catalogue is not synthesised, not cached across gateways, and not merged speculatively (`talaria/ui/app.py:3438` fetches once; this unit does not retry or refresh on focus). A degraded listing says so.

**A palette that stole `Enter` from the one widget that owns it.** The PLACEHOLDER (`talaria/ui/composer.py:50`) promises `Enter` sends; KTD3 and KTD4 keep that true by never moving focus and never submitting on select.

**A palette that made prose slashes expensive.** KTD2's first-column predicate is the guard. A palette that opened mid-sentence would make the composer the place where typing `/tmp/path` might filter instead of inserting, which is the hazard that killed the "anywhere `/`" option.

**A palette that made `F3` lie.** Reusing `PaletteRegion` means one catalogue, one `format_entry`, one `header_line`. A second listing would need to be kept in sync and would instead be two.

**The overlap with unit C1 on `Up`/`Down`/`Enter`.** Named in KTD4 as testable claims. If C1's plan claims `Up` unconditionally in the composer, the two plans conflict and one must give way; this plan gives way only by closing the palette first — while open, palette keys win, and history recall is not also triggered.

**Geometry.** No false layout invariant. This plan does not assert palette row counts survive focus changes beyond what `talaria/ui/palette.py:155-165` already asserts for the browse listing. A future geometry bug is not papered over with a tautology.

**What breaks, named rather than discovered.** `tests/transport/test_commands.py:770` asserts `/quit`'s `local` marker via `app.palette.row_texts` after `F3`; that test stays green because `F3` still marks locals. New tests for the filtered mode assert against `app.palette`'s filtered row set, not against a new widget.

## Acceptance evidence

- **AE1. The leading-slash predicate.** From an empty composer, typing `/` opens the palette; typing `x/`, typing ` /` then `a` without a leading `/`, typing prose containing `/tmp/foo`, and typing `/` with the caret not at column zero leave it closed. The probe is a present-vs-absent check on `app.palette.showing` after driving `TextArea` content through the same path `ChatTextArea._on_key` (`talaria/ui/composer.py:115`) takes, not a string search for "/" anywhere in the widget tree.

- **AE2. Filtering is live and prefix-only, case-insensitive.** With the composer containing `/mod`, the palette shows every entry whose name lowercased starts with `mod` (including `/models` and the gateway's `/model`), shows no entry that merely contains `mod` elsewhere, and re-filters within one frame when a character is appended or removed. A cross-case check (`/MOD` versus `/mod`) yields identical row sets. Bare `/` yields every runnable entry (local seven plus gateway dispatchable) with header counts matching `header_line` (`talaria/ui/palette.py:59`).

- **AE3. Nothing-match handling.** With the composer containing `/zzzzzzz`, the palette stays open, shows the header and degraded line if any, shows exactly one muted row whose text contains "no matching commands", and shows zero runnable rows. Pressing `Enter` in that state leaves `app.composer.text` equal to `/zzzzzzz` and does not dispatch.

- **AE4. The unavailable and not-yet-fetched states.** With `catalog is None` the palette opens on `/` and shows exactly seven rows (the seven at `talaria/domain/commands.py:364`), header `NOT_YET_FETCHED` (`talaria/ui/palette.py:44`), and no degraded line. With `catalog.available is False` it opens and shows the same seven rows plus the degraded line `CATALOG_FAILURE_PREFIX` (`talaria/ui/palette.py:46`) containing `catalog.failure`, and header counts reflecting the local seven. With `catalog.warning` present it shows the `CATALOG_WARNING_PREFIX` line (`talaria/ui/palette.py:50`).

- **AE5. The local vs gateway distinction.** While filtering, every displayed row is runnable (`dispatch` or `talaria-local`), and no `unsupported` row appears; while browsing with `F3`, all three availabilities appear with their `AVAILABILITY_MARKER` (`talaria/domain/commands.py:168`) text. The header counts remain tripartite in both modes.

- **AE6. Selection inserts, never submits, and closes the palette.** With the composer `/mod` filtered to show `/models`, pressing `Enter` replaces the composer text with `/models ` (canonical name plus trailing space), leaves `app.dispatcher` call count unchanged (no `SLASH_EXEC_METHOD` or `DISPATCH_METHOD` at `talaria/domain/commands.py:118` nor `talaria/domain/compat.py:118`), keeps focus in `ChatTextArea` (`app.focused is app.composer.text_area`), and leaves the palette closed. A click on the same row does the same. The inserted name is the catalogue's `canon` (`talaria/domain/commands.py:212`) when the filtered row was an alias.

- **AE7. Dismiss keeps the draft.** With the palette open on `/models`, pressing `Escape` leaves `app.composer.text == "/models"` byte-identical, leaves focus in the composer, and closes the palette; deleting the leading `/` does the same; sending focus to `TranscriptPane` closes it with no row inserted.

- **AE8. The palette never steals from B1's notice row.** With focus in `TranscriptPane#transcript` (`talaria/ui/app.py:1069`) and a printable key arriving at `TalariaApp.on_key` (`talaria/ui/app.py:4452`), `app.composer.notice` equals `_DISCARD_NOTICE_BY_REGION["transcript"]` (`talaria/ui/app.py:4213`) and the palette's degraded row text is unchanged. Opening the palette does not clear that notice, and the discard latch (`talaria/ui/app.py:4228` `_no_text_region`) is not cleared by palette show/hide.

- **AE9. Key seam with C1 and with the composer.** With the palette open, `Up`/`Down` move a highlight inside `PaletteRegion` and do not move the `TextArea` caret or trigger history recall; `Enter` inserts rather than submits or recalls; `Escape` closes the palette and consumes the event so no prompt card or picker receives it. With the palette closed, the same keys revert to their existing owners: `Up` is available for C1's history, `Enter` submits (`talaria/ui/app.py:2789` `on_chat_text_area_submitted`), and `TextArea` arrow handling (`talaria/ui/composer.py:115`) is untouched. All three are asserted with focus explicitly in the composer before the keypress.

- **AE10. The browse listing is unchanged.** Togglling with `F3` (`talaria/ui/app.py:2847`) still renders every entry through `format_entry` (`talaria/ui/palette.py:53`) with `AVAILABILITY_MARKER`, still caps at `max-height: 14` (`talaria/ui/palette.py:85`), and still applies the catalogue via `await palette.apply(self.catalog)` (`talaria/ui/app.py:3486`). The `test_the_talaria_local_four_are_marked_local_in_the_listing` shape (`tests/transport/test_commands.py:770` area) still passes.

- **AE11 — operator-only: driving the app, `/` in an empty composer opens a list that filters as you type, arrow keys move, `Enter` completes the word so you can add arguments, and `Escape` puts you back where you started with your draft intact.** A person can also paste `/sessions`, see it filtered, select it, see `/sessions ` in the composer, and decide not to submit. This is not claimed on the suite's evidence.

- **AE12. The replay gate runs green over the existing gate corpus, with `content_is_complete` and `interface_shows_everything` true. The corpus is named by digest and frame count, never by path. The palette writes no transcript row and the gate drives no slash prefix, for the same reason B1 and B3 gave — the change has no surface under the gate.**

- **AE13. The project check is clean.** `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check`. `npm run check` is not required — nothing under `src/` is touched.

## Verification

```bash
uv sync --all-groups
uv run pytest tests/ui/ tests/domain/ tests/transport/ -q
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched (that tree is the TypeScript (a superset of JavaScript) reference recorder `tests/recorder/test_equivalence.py` asserts the Python recorder is equivalent to).

A live gateway check is not part of this unit's verification: every claim above is against the already-read baseline (`talaria/domain/compat.py:118`) or a stub gateway, per the orchestration charter's live-testing rule ("operator-only" at `docs/plans/2026-08-11-v0-3-orchestration-charter.md:377`).

## What this unit does not do

- **It does not re-decide the picker shape.** The model and profile pickers stay modal dialogs (`talaria/ui/dialog.py`). This palette is non-modal for the reason `talaria/ui/palette.py:1-22` originally gave for a listing, and the decision log at `docs/engineering-journal/DECISIONS.md:684` that overturned KTD3 for pickers is not revisited.

- **It does not add a new gateway method or a refresh.** The catalogue is still read once (`talaria/ui/app.py:3438`). Refreshing on reconnect or on demand is a separate capability and would need its own probe of `commands.catalog` (`talaria/domain/compat.py:118`) plus a stale-row policy; none is added here.

- **It does not change where local commands live.** The seven remain in `talaria/domain/commands.py:364` (`TALARIA_LOCAL_COMMANDS`), resolved before the catalogue at `talaria/ui/app.py:2789`. A new local command is a row in that tuple, not a branch in the palette.

- **It does not change dispatch.** `dispatch_command_live` (`talaria/ui/app.py:4008`) still tries `slash.exec` then `command.dispatch` (`talaria/domain/commands.py:118` pair), and `on_chat_text_area_submitted` (`talaria/ui/app.py:2789`) still refuses `unsupported` before sending. The palette never dispatches.

- **It does not change the composer's submit-versus-newline contract.** `Enter` submits and `Ctrl+J` inserts a newline (`talaria/ui/composer.py:115`, `PLACEHOLDER` at `talaria/ui/composer.py:50`) are untouched. The palette claims `Enter` only while open and as insertion, not submission.

- **It does not claim cross-platform key interception.** `F3`'s palette toggle, `F1`'s jump, and `F5`'s follow are not re-bound here. If a desktop eats a function key, this palette is still reachable by typing `/` — that is the point of C2 being a typed entry point rather than a function-key one.

- **It does not borrow the status or notice rows for listing state.** `StatusRegion` is unconditionally mounted (`talaria/ui/app.py:1073`) and the composer notice (`talaria/ui/composer.py:146`) remains B1's transient surface. The palette's degraded row (`talaria/ui/palette.py:98`) is the only place the palette says anything when there is nothing to say.

- **It does not implement history recall.** C1 (up-arrow history) is planned in parallel; this plan states the seam (KTD4) rather than guessing C1's shape. The only commitment this plan makes to history is that it does not consume `Up` while the palette is open — a claim the reviewer can check against the other plan.

