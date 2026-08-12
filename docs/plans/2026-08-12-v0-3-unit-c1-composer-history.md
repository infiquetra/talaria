---
title: v0.3 unit C1 — up-arrow history in the composer
type: plan
status: proposed
date: 2026-08-12
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: C1
---

# Unit C1 — up-arrow history in the composer

**In Talaria, pressing up-arrow in the composer recalls the previous submitted message, fully editable, without ever writing operator text to disk.** That is the one-sentence answer to the five questions a reviewer will check. History holds only messages that were actually submitted, including slash commands as they were typed, with a bounded in-memory list and a draft stash that protects a half-written message across any amount of arrowing. Up-arrow moves the caret inside a multi-line draft and only recalls history when the caret is at the top; history itself lives in the domain core as plain Python, beside the transcript, and the terminal framework never owns it. The seam with the slash-command palette (unit C2) is a single ordered predicate, not two features guessing about each other.

The operator's whole requirement is in [the hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md), note 12 (`:336-343`):

> the input box will need some work, helpful things like: up arrow to go to previous commands

That note also asks for the palette this plan's seam section is written to coexist with — "type '/' allows scrolling/filtering on all possible slash commands" (`:342-343`) — which is why a plan that says nothing about who owns which key is a defect.

## The requirement this unit serves

Every comparable terminal interface offers up-arrow recall, and its absence is the kind of omission that reads as unfinished rather than as a deliberate choice. The charter places this unit in spine C (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:131-136`) as additive work that reopens no gate, scheduled after spines A and B are merged and explicitly the release's shock absorber if spine A widens.

## Mechanism — verified by reading, at `main` = `d56eb09`

**The composer today is a multi-line `TextArea` with two hard-wired keys.** `talaria/ui/composer.py` defines `ChatTextArea` at `:53`, a subclass of Textual's `TextArea` with `language=None`, `soft_wrap=True`, `show_line_numbers=False` and `compact=True` (`:190-198`), and `Composer` at `:129` which wraps it with a one-row `Static` notice (`:202`, `classes="composer--notice"`). `ChatTextArea._on_key` at `:115-126` intercepts exactly two keys — `enter` which posts `Submitted` (`:62-68`, `self.text`), and `ctrl+j` / `shift+enter` which inserts `\n` (`:121-124`) — and delegates everything else to `super()._on_key(event)` at `:126`. The effect is that `enter` always submits and `ctrl+j` always breaks a line; no other binding is yet claimed in this widget.

**Submission is a message the app owns, not the composer.** `ChatTextArea.Submitted` carries the composed `text` as written, and the app handles it in `talaria/ui/app.py:2789` (`on_chat_text_area_submitted`). That handler resolves the text through `resolve_command(message.text, self.catalog)` (`app.py:2808`) — Talaria-local commands first, then gateway commands, then plain message — and in the plain-message branch spawns `self._submit_and_discard(message.text)` (`app.py:2845`) which calls `submit_live` (`app.py:1628-1681`). `submit_live` trims (`body = text.strip()` at `:1653`), calls `dispatcher.call(SUBMIT_METHOD, {"session_id": ..., "text": body})` (`:1657-1661`), and only on `confirmed` or `unknown` writes `record_submission(self.state, body, ...)` (`:1669-1671`) and clears the composer (`self.composer.clear()` at `:1678`, defined at `composer.py:236`). The composer itself is cleared only after delivery is known — "Only called once a submit is known to have been delivered" (`composer.py:236-242`) — and an abandoned draft (escape, quit, or simply not submitting) never reaches this path.

**Focus matters because a no-text region silently discards typing.** `talaria/ui/focus.py:37-46` defines `CaretReleased`, the event a region posts after Talaria takes a control away, and `talaria/ui/app.py:4213-4228` classifies the focused widget into `transcript`, `agents`, or `prompts` via `_NO_TEXT_REGION_IDS` and `_no_text_region()`. `on_paste` at `app.py:4437` and `on_key` at `app.py:4455` use that classification to notice a printable key or paste that "would otherwise be silently discarded" and write a discard notice instead. History's key handling must therefore be ordered with respect to both focus and that discard path, and with respect to whatever C2 adds for `Up`/`Down` inside a palette overlay.

**Text access is already framework-independent.** `Composer.text` at `composer.py:218-222` and `Composer.clear()` at `:236` hide the `TextArea` behind plain `str` operations, so a domain-owned history manager can read and write the composer's content without importing Textual. ADR-0002's boundary (`platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md:36-51`) — the domain core never imports the terminal framework — therefore constrains where the history list lives, not whether history is possible at all.

**No history list exists today.** `grep -rn "history"` over `talaria/` finds only session history — `seed_history` (`talaria/domain/state.py:578`), `WITHHELD_HISTORY_PREFIX` (`state.py:507`), and transcript history — not composer recall. The composer has no index, no stash, and no `Up`/`Down` handler.

## The design space

### What counts as history — and why not everything typed

Two earlier plans bound this choice. Unit B1 replaced R5 with R5' (`docs/plans/2026-08-11-v0-3-unit-b1-caret-status-row.md:209-211`) and made the composer the one place typing must reliably reach; any history design that writes operator text to disk without the operator saying so undercuts that guarantee in a public repository. And the charter's repository-care rule (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:325-327`) forbids operator profile names, machine names, hosts, or workspace identifiers in any committed file — the same caution applies with particular force to free-form operator text that history would preserve verbatim.

**History holds every line the operator pressed `enter` on, in the form it was submitted after `strip()`, capped in memory and never written to disk in v0.3.** That includes an ordinary prompt ("summarise the diff") and a slash command as typed (`/models 2`, `/quit`, `/resume` the same way `resolve_command` at `app.py:2808` distinguishes `LocalInvocation`, `GatewayInvocation`, and plain `text`). A recalled slash command is therefore editable and re-submittable like any other entry — the operator who pressed up to get `/models 2` can change it to `/models 3` before pressing `enter`, and nothing about the recall distinguishes the two. The trimming matches `submit_live`'s own `body = text.strip()` (`app.py:1653`), so history shows what the gateway saw, which is what the transcript will show after `record_submission` (`state.py:783`).

Abandoned drafts — text that was typed and then erased, or left in the composer when the app quit without pressing `enter` — never enter history. The rationale is the trust rule already in `composer.py:236-242`: a composer cleared only after delivery is known, never on a refused send, because "A composer cleared on a refused or failed send loses what the operator typed, which is the one thing a chat client must never do." History is the memory of what was sent, not a transcript of what was typed.

Failed or unknown deliveries *do* enter history. `submit_live` writes `record_submission` even for `unknown` (`app.py:1669-1671`) and marks the transcript with `delivery_of(outcome)` (`app.py:1606-1619`). A message that was handed to the transport and then lost is still a message the operator meant to send, and losing it from recall would be the same unreliability history exists to remove. An empty string after `strip()` is the only thing excluded.

**Size and lifetime.** One hundred entries, in memory only, newest at the end. One hundred covers a long session without growing without bound; in memory keeps the unit out of the privacy hazard entirely for v0.3. History survives focus moves and prompt-card mounts within a process — tabbing into the transcript or answering a card does not clear it — and is cleared only when the process exits. A `--resume` that lands a different session's server-side history (`state.py:578-583` — "The gateway owns history truth") does not restore the *client's* recall list from anywhere, because there is nowhere to restore it from without first putting operator text on disk.

### Up-arrow in a multi-line composer — three honest answers, one chosen

Every terminal interface with a multi-line input must answer this and they do not all answer it the same way. The three real positions are:

**1. Up always recalls, even mid-document — rejected.** This is what a single-line `Input` would do unconditionally, and it is defensible there because the caret has nowhere else to go. In `ChatTextArea` it breaks editing: the composer is configured for multi-line (`soft_wrap=True` at `composer.py:193`, `Ctrl+J` inserts `\n` at `:121-124`), and a draft that is already three lines cannot be navigated with `Up`/`Down` if both keys have been taken for history. The operator who typed a paragraph and wants to fix the first line would leave recall to do so, which defeats the editor.

**2. Up moves the caret inside the draft, and history is reached by a different key — rejected.** This preserves editing perfectly and is what some full editors do, but it makes the note 12 ask — "up arrow to go to previous commands" (`hands-on notes:338`) — false by construction. An operator who presses `Up` on a single-line draft and sees the caret do nothing has met a key that claims to do history and does not. The charter's theme is that Talaria confirms what it just did; a silent refusal to recall is the same class of defect B3 repaired for `F1`, `F2`, `F5`, and `end` (`docs/plans/2026-08-11-v0-3-unit-b3-keypress-feedback.md:110-135`).

**3. Hybrid — caret-aware — chosen.** When the composer's text contains a newline, `Up` recalls only when the caret is in the top row; `Down` recalls only when the caret is in the bottom row; otherwise the key moves the caret. When the composer is single-line (no `\n` in `self.text`, which is the common case for a prompt), `Up`/`Down` always navigate history. This is the shape `fish`, `zsh` with multi-line, and `prompt_toolkit` use, and it is the only one of the three where both editing a paragraph and recalling on a single line work without a second binding.

The hybrid rule also respects Textual's own caret tracking. `TextArea` exposes `cursor_location` (row, column) and already moves the caret on `Up`/`Down` inside `_on_key` when those keys are not intercepted. History intercepts only when the boundary condition above is met; otherwise it returns without consuming the event and `super()._on_key` does the caret move.

### The half-written message — why it must survive arrowing

Losing a half-written message is the failure mode that makes people distrust history. The operator types "explain why the gate re-ran at ", presses `Up` to check what they asked last time, decides not to send that either, presses `Down` — and the sentence they were composing is gone. Every mature shell saves that draft.

**History keeps a draft stash beside the list, not inside it.** When the operator is at the newest end of history (the `down` direction has reached the sentinel one past the last entry, which is defined to be "the draft you were writing"), the composer holds whatever they typed. The first `Up` from that sentinel copies the current composer text into the stash before overwriting the composer with `history[-1]`. Each further `Up`/`Down` moves the index; pressing `Down` past the last entry restores the stashed text verbatim, moves the caret to its prior position, and clears the navigation state. Editing a recalled entry and then moving does not overwrite the stash — the stash is taken once, on leaving the sentinel, and is held until the navigation is abandoned (by submitting, by editing and then submitting, or by pressing `Down` back to the sentinel).

**What abandoning navigation means.** Any of three actions abandons the walk: submitting (which appends to history and clears the stash), pressing `escape` or otherwise dismissing the composer without submitting (stash kept until next walk, never written to history), or simply editing the recalled text and then pressing `Up`/`Down` again — the edited-but-not-submitted text is not snapshotted on each move, because doing so would turn two successive `Up` presses into an unintended history entry and fill history with typos.

**The stash is not history.** It is never appended to the list; it is only shown while the sentinel is focused. This is the distinction shells draw between the edit buffer and the history file, and it keeps the size bound honest.

### Recalled text is editable — and it becomes a new entry

Whether the recalled text is editable and whether editing it amends the old entry or creates a new one are two halves of one decision, and both answers are load-bearing for trust.

Recalled text lands in the composer as ordinary editable text (`composer.text = history[index]`), with the caret at the end, and every editing operation that works on a fresh draft works on it. Submitting that text — whether as-is or after editing — appends a new entry to the end of the list. It does not replace the entry it was recalled from, and it does not de-duplicate against it. The entry recalled from `history[i]` remains at `i`; the submitted text, even if identical, becomes `history[n]` with a later timestamp.

The rejected alternative — amending in place, or replacing the recalled entry — is what makes a second `Up` after editing appear to "lose" the edit. It also makes the transcript and history diverge in a way the operator cannot see: the transcript (`record_submission` at `state.py:783`) always appends, never overwrites, so an amended history would name the same logical prompt with two different strings depending on where you looked.

Consecutive duplicates are kept. Deduplicating the most recent entry ("do not append if `body == history[-1]`") saves a handful of entries at the cost of lying about how many times the operator actually asked: pressing `enter` on "retry" three times produced one history entry, one `Up` rather than three, and a transcript that says three while recall says one.

### Where the state lives — the ADR-0002 line

ADR-0002's boundary (`platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md:36-51`) is that the domain core has no dependency on the terminal framework, in either direction of knowledge. The pipeline is `raw gateway frames -> normalized events -> domain state -> render projection`, and the presentation layer consumes view models and may request commands — it does not hold protocol truth.

**History lives in the domain core as plain Python; the composer widget owns only the caret and the stash cursor.** Concrete shape: a new module `talaria/domain/composer_history.py` holding a frozen dataclass — `entries: tuple[str, ...]` bounded at 100, `draft_stash: str | None`, `index: int | None` where `None` means "sitting on the sentinel (the live draft)" — with pure functions `push(state, text) -> state`, `move_up(state, composer_text, caret_at_top) -> (state, composer_text | None)`, `move_down(...) -> ...`, and `abandon(state) -> state`. No import of `textual` appears in that module; the enforcement check that fails the build if the domain imports the framework (`tests/domain/test_boundary.py`) therefore continues to pass. The app holds one `ComposerHistory` on `TalariaApp`, mutates it only through those pure functions on `Up`/`Down`/`Submitted`, and writes `composer.text =` for the result. The `TextArea` subclass `ChatTextArea` (`composer.py:53`) intercepts `Up`/`Down` only to compute the boundary condition; it never mutates the list.

A UI-owned history — a list on `Composer` or on `ChatTextArea` and key handling entirely inside `composer.py:115-126` — was weighed and rejected. It satisfies the letter of ADR-0002 (a widget may hold presentation state) but not its purpose: history would then exist only while the widget does, would need to be re-derived after every table-test mount, and would double-exist in tests that drive domain and UI together. The cheaper test layer this project relies on — domain tests with no screen — would not cover it, and the `record_submission` path that the transcript already trusts (`state.py:783`) would have to be duplicated to keep two truths aligned. Domain-owned and projection-consumed is the smaller surface.

**Rejected — a hybrid where the domain owns the list and the widget owns the index.** This splits one invariant — "the 100-entry list and the index into it are consistent" — across the framework boundary, so neither side can assert it alone. The hybrid buys nothing the domain-cursor does not already buy, because the caret-top check already forces the widget to participate.

## Key technical decisions

### KTD1 — what enters history, and what never does

Every string that passes `strip()` after `enter` and reaches either `submit_live` (`app.py:1653`) or `resolve_command` (`app.py:2808`) as a slash command, excluding only the empty string. That means an ordinary prompt ("explain the diff"), a slash command (`/sessions`, `/models 3`, `/profiles lab`), and a slash command with an argument all enter history in the exact bytes the operator submitted. An abandoned draft, a notice-bar message, and the `PLACEHOLDER` (`composer.py:50`) never do. Failed and `unknown` deliveries enter history — they were still the operator's intent, and `record_submission` (`state.py:783`) already treats them as transcript truth with a delivery marker rather than as non-events.

**Rejected — submit only successful messages.** This would tie recall to the gateway's reply, so a message that timed out disappears from recall even though the operator may want to retry it immediately — the worst moment to discover it is gone.

**Rejected — slash commands excluded from history.** This treats "talaria commands" and "prompts" as separate kinds at the recall layer, when `resolve_command` already treats them as one disjunction at dispatch, and it would make the most repeatable lines in a session (model switches, profile switches) the ones that cannot be recalled.

### KTD2 — `Up`/`Down` are hybrid on caret position, and history never traps the caret

When `Composer.text` contains no `\n`, `Up` steps one entry back in history and `Down` steps one forward, with `Down` from the newest entry restoring the stashed draft. When the text is multi-line, the key is consumed for history only when the caret is at the respective boundary row (`Up` only from the first row, `Down` only from the last); otherwise the key is left for `TextArea` to move the caret. The boundary is measured from `text_area.cursor_location` (Textual's `(row, column)`). One `Up` from the sentinel stashes `composer.text` verbatim; a later submission appends and clears both the index and the stash; abandoning the walk without submitting preserves the stash until the next walk.

**Rejected — unconditional recall.** Breaks multi-line editing, as described above.

**Rejected — a modifier chord for history (`Ctrl+Up`)** with plain `Up` always moving the caret. This preserves both behaviours at the cost of discoverability: the note 12 operator pressed `Up` expecting history, not `Ctrl+Up`. A chord may be added later as a *second* path, but it does not replace the hybrid.

### KTD3 — the draft stash is a first-leave snapshot, not a ring

The stash is taken once — on the first `Up` from the sentinel — and is thereafter read-only until the sentinel is re-reached or a submission clears it. Moves inside history do not overwrite it, and editing a recalled entry does not snapshot that edit on the next move. Pressing `Down` past the newest entry restores `stash` exactly, caret at the end, and sets `index = None` (back on the sentinel). Pressing `escape` while navigating, or focusing away from the composer (`focus.py:37`'s `CaretReleased` path does not discard the stash), abandons the walk but keeps the stash for the next walk, so a tab-away and back does not destroy a half-written message.

**Rejected — snapshotting on every move.** This fills history with fragments as the operator moves, and makes two `Up` presses and one `Down` return a different string than the one that was stashed.

**Rejected — clearing the stash on any focus loss.** This punishes the deliberate focus moves `focus.py:9-14` documents ("Both regions are `VerticalScroll`, which sets `can_focus = True`") by discarding the draft the moment the operator tabs to check the transcript — the exact cross-pane workflow history should support.

### KTD4 — recalled text is fully editable and submits as a new entry

A recalled entry populates `composer.text` and is indistinguishable from freshly typed text: cursor movement, selection, `Ctrl+J` newline, paste (`ChatTextArea._on_paste` at `composer.py:89`), and `clear()` (`:236`) all behave the same. Submitting it — `enter` posting `ChatTextArea.Submitted` (`:62-68`) — calls `push(history, submitted_text)` which appends, evicts the oldest when over 100, and resets navigation state. No de-duplication, no in-place amendment.

**Rejected — editing a recalled entry amends the original.** This makes the history list mutable in a way the append-only transcript is not, and the divergence is invisible.

**Rejected — recalled text re-submits without becoming editable first.** This would be a shortcut ("press `enter` immediately to resend"), and it is already covered: the recalled text is selected to the end, so `enter` on an untouched recall does resend, without needing a second mode.

### KTD5 — history is domain state, beside the transcript, with a bounded in-memory store

A new module `talaria/domain/composer_history.py` holds the frozen state and pure transitions, bounded at 100 entries, with no import of `textual`. `TalariaApp` holds one `ComposerHistory` instance (initialised empty), updates it on `Up`/`Down` and on `ChatTextArea.Submitted` (and on `LargePaste` collapse, which does not itself enter history — only the subsequent submit does), and writes `composer.text` for the projection. The transcript's own history path (`seed_history` at `state.py:578`, `record_submission` at `:783`) is untouched; composer recall and session transcript are different histories with different owners.

**No persistence in v0.3.** History does not survive a process exit, is not seeded by `--resume`'s `seed_history` (`state.py:578-583`), is not written to the recording (`record/recorder.ts` parity, `tests/recorder/test_equivalence.py`), and is not read from or written to any file under `<config_dir>`. This keeps operator text off disk entirely and satisfies the public-repository constraint by construction. Persistence is explicitly deferred (see `## What this unit does not do`) and when it arrives it must route through the same file-mode discipline as `<config_dir>/credentials` (`mode 0600`) and be gated by an explicit operator opt-in, not enabled by default — a decision whose authority is operator-only and which this plan does not pre-decide.

**Rejected — a widget-owned list on `Composer`.** Already addressed under "Where the state lives" above.

**Rejected — history derived from the domain transcript.** The transcript holds assistant messages, tool output, and system lines the operator never typed; deriving recall from it requires a second filter, and it makes `Up` recall something the operator did not write, which breaks the "previous commands" vocabulary of note 12 (`hands-on notes:338`).

## Risk this unit must clear

**Operator text reaching disk is a privacy defect in a public repository.** Talaria is public, and a history file containing free-form prompts written to `<config_dir>` or alongside recordings would be readable by any process that can read that directory and at risk of being pasted into a bug report or committed. The plan's mitigation is total in v0.3: history is in-memory only, there is no file, mode, path, or format to audit, and the deferred persistence (when it exists) must be opt-in and `0600`. A reviewer checks this by confirming that the new module touches no `pathlib`/`open`/`json` file path and that no test writes a history file.

**Losing the draft on any realistic sequence, not only the happy path.** The failure is not "down past the newest restores" but "down past the newest after tabbing to the transcript, answering a prompt, or inserting a newline." KTD3's stash rule — taken once, restored on sentinel, never overwritten mid-walk, and not cleared by focus moves — is pinned by AE3 across all three of those sequences.

**The shared key surface with C2.** Both C1 and C2 want composer keys. If each plan claims `Up`/`Down` unconditionally, one of them silently wins depending on import order, and the other ships broken. The seam below names a single ordered predicate and the review gate fails any change that adds a second `Up` handler without extending that predicate. AE8 checks the predicate from both sides.

**Large pastes and the collapse placeholder.** A paste at or above `PasteThreshold` (`Composer.paste_threshold` at `composer.py:174`, tripped at `:112`) inserts literally and then collapses to a placeholder (`collapse_paste` at `:259`), so the composer text after the round trip is no longer what was pasted. History records the *submitted* text — the placeholder line, not the original body — because that is what the transcript will show after `record_submission`. Recording the original body would make a recalled history entry differ from the transcript line for the same submit.

## Acceptance evidence

- **AE1.** Submitted messages enter history, slash commands included, empty lines excluded: submitting an ordinary prompt, then `/models 2`, then a blank `enter`, yields history length 2 with the two strings in submission order, newest last. Abandoning a draft (typing without `enter`) leaves history unchanged.
- **AE2.** `Up` recalls backwards and `Down` advances forwards in submission order: with history `[a, b, c]`, pressing `Up` three times from an empty composer shows `c`, then `b`, then `a`; pressing `Down` steps `b`, `c`, and then the stashed draft.
- **AE3.** The draft stash survives and restores: typing `draft half` then `Up` then `Down` past the newest restores `draft half` verbatim. The same holds when the `Up` is followed by tab into the transcript and back, by answering a prompt card, and by inserting a newline with `Ctrl+J` inside a multi-line draft before recalling — all three asserted.
- **AE4.** Multi-line boundary rule: with a two-line draft "line one\nline two" and caret in line two, `Up` moves the caret to line one and does not change the history index; with caret in line one, `Up` recalls `history[-1]`. With caret in line one of a multi-line draft, `Down` moves the caret down; with caret in the last line, `Down` (when navigating) advances history. Single-line drafts always recall.
- **AE5.** Recalled text is fully editable and submits as a new entry: recalling `a`, editing it to `a edited`, pressing `enter` appends `a edited` as the new last entry without changing the entry at its original position; a second `Up` still shows `a` at its original index, and the transcript shows the new entry as the one that was delivered.
- **AE6.** Failure and `unknown` are not a second-class history: a submit whose reply is `unknown` (`delivery_of` at `app.py:1606-1619`, `DELIVERY_NOTES` at `state.py` domain constants) appears in history and is recallable and re-submittable.
- **AE7.** Size bound, in-memory, no file: after 105 submissions history holds exactly 100 entries, the oldest five having been dropped; the process's file system shows no history file under any path, and `--resume` landing a new session does not merge the previous process's in-memory list.
- **AE8.** The seam with C2 is ordered and exclusive: while the palette introduced by C2 is open, `Up`/`Down` navigate the palette and do **not** move history; while it is closed, `Up`/`Down` navigate history under KTD2 and the palette is not triggered by recalling a slash command. Both asserted from the same test that opens the palette with `/` and then drives `Up`.
- **AE9.** The framework boundary is intact and history is not entangled with the caret row: the new `talaria/domain/composer_history.py` contains no import of `textual`; `tests/domain/test_boundary.py` remains green; the discard notice introduced by B1 (`app.py:4437-4475`, KTD2 latch) and the focus handlers (`app.py:4267-4271`) are untouched.
- **AE10.** The suite is green and the replay gate is unaffected: `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check` are clean; `talaria gate --corpus <recording>` runs green over the existing corpus, writing no history-derived transcript row, because history never writes to the transcript except through the existing `record_submission` path.

**Acceptance for a person, per the charter's evidence rule 2** (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:182-186`): driving the app, the operator types three prompts, presses `Up` twice to re-edit the second one, submits it, and sees it reappear as the next transcript entry; typing a half-written message, pressing `Up` to glance, and pressing `Down` back never loses what was half-written, even after tabbing away. That is operator-only and is not claimed on test evidence.

## Verification

```bash
uv sync --all-groups
uv run ruff check .
uv run mypy
uv run pytest tests/domain/ tests/ui/ -q
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched.

Operator-only verification — a hands-on drive against a live gateway on a real terminal — follows the checklist this plan's `## Acceptance evidence` will be rendered into, exactly as `docs/plans/2026-08-06-u6-row19-operator-checklist.md` renders a plan's acceptance into steps. No step claims a live gateway was used by an agent.

## What this unit does not do

- **It does not persist history to disk.** In-memory only for v0.3. Persistence, when it arrives, needs an opt-in, a `0600` file, a retention and redaction policy, and a `clear-history` command — all operator-granted and all out of this unit's scope.
- **It does not import or trigger the palette.** Typing `/` inserts the character; C2's palette decides whether that opens anything. Recalling a slash command from history does not open the palette, and the palette does not write to history until its selection is submitted.
- **It does not change where focus goes.** History never moves focus (`talaria/ui/focus.py:37-46`'s `CaretReleased` path is untouched), never takes the caret away, and never claims a key while the caret is not in `ChatTextArea` (`composer.py:53`). A printable key discarded in a no-text region remains B1's notice, not history.
- **It does not change the submit path.** `on_chat_text_area_submitted` (`app.py:2789`), `submit_live` (`app.py:1628`), `LargePaste` (`composer.py:70`), and `collapse_paste` (`:259`) are untouched; only the *text* they hand to the dispatcher happens to have come from history on a recalled-then-edited submit.
- **It does not add a second recall binding.** `Ctrl+P`/`Ctrl+N` or `Alt+Up` may be added later, but v0.3's contract is `Up`/`Down` under KTD2. A review that asks for a chord in this unit is asking to widen scope.
- **It does not search or filter history.** Incremental search (`Ctrl+R`) and substring filtering are a later additive unit; `Up`/`Down` walk the list linearly.
- **It does not reorder history on recall.** The list stays in submission order; a recalled entry does not jump to the front until it is re-submitted, which appends it anew at the end.
- **It does not touch the TypeScript tree under `src/`.** That tree is the reference recorder (`tests/recorder/test_equivalence.py`), not bootstrap (`AGENTS.md:21-22`). Nothing in this unit justifies touching it.

## The seam with unit C2, stated so it is reviewable

C2 is the filterable slash-command palette triggered by typing `/` in the composer, planned in parallel against the same widget. Both units want composer keys. The ordering is one predicate, evaluated before either unit's handler runs:

1. If the palette introduced by C2 is open, `Up`/`Down`/`Enter`/`Esc`/`Tab` belong to the palette. History does not see them, the composer text does not change from history, and a printable key that would otherwise trigger B1's discard notice is consumed by the palette's type-ahead filter instead.
2. If the palette is closed and the composer (`ChatTextArea` at `composer.py:53`) holds the caret, `Up`/`Down` belong to history under the caret-position predicate KTD2 describes. `Enter` remains `ChatTextArea.Submitted` (`:62`) and is history's `push` point; `/` inserts the character and C2 decides whether to open the palette on the new text.
3. If neither holds — caret in a no-text region (`app.py:4213-4228`) — `Up`/`Down` belong to neither unit. The key reaches scroll or B1's discard path (`app.py:4455-4474`) without re-dispatch.

History therefore **requires** of any cohabiting composer feature that it be overlay-aware: history is inert while an overlay that claims `Up`/`Down` is open, and the overlay is inert while the composer is not the thing being typed into. A plan that states "C1 owns `Up` unconditionally" is replaced by the ordered clause above, which a reviewer can check by opening the palette with `/` and pressing `Up` — the palette moves, history does not.

