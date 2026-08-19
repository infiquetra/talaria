# Queued Work - talaria

> Future work with priority, rough effort, and worth-it-when triggers.

## P0

### Eleven public exports have no production caller — v0.4 accumulated review

**Priority.** P3 — none can hurt anyone; all of them read as live to a future maintainer, which is the
cost.

**Evidence.** A mechanized sweep of all 320 names exported by `__all__` across `talaria/` found 12
with zero production call sites. One was a real defect and is fixed (`truncation_note` — evictions
were silent). The other eleven fall in three groups:

| Group | Names | Why it is not urgent |
| --- | --- | --- |
| Superseded | `decode_identity` | `decode_selection` replaced it; the old 3-part split is tested in isolation only |
| Test-only helpers living in production modules | `apply_frames`, `route_frames`, `stale_for`, `probe_set_is_permitted`, `unprobed_methods` | each has real test callers; the smell is that they are exported, not that they exist |
| Declared and never read | `EXPIRE_EVENT_KINDS`, `GATEWAY_STATUS_WORDS`, `RecordOutcome`, `read_credential_document` | referenced by nothing at all, not even a test |

`sync_queue` is a twelfth zero-caller export and is **correct as-is** — its own docstring says it is
"never called by a reduction here" and explains why.

**Worth it when.** The next change that touches one of these modules for another reason. Deleting
public API surface across five modules is its own change with its own review, and doing it as a
by-product of a review pass would put unrelated churn in a remediation commit.

**The sweep itself is worth keeping.** The script that found this is throwaway, but the check is not:
"which production call site reaches this?" has now found four dead paths across U6, U7 and U8B plus
this one. Consider making it a gate rather than a habit.


### The settled-item bound became live in U8B and nothing measures what it costs

**Priority.** P3 — recorded because U8B is what activated it, not because it is urgent.

**What.** `SETTLED_ITEM_LIMIT` (`talaria/domain/state.py:3938`) caps `settled_items` at 256 newest,
and `settle_queue_item` has kept only that many since it was written. Until 2026-08-19 nothing wrote
to it, so the bound was inert. U8B gave it a production caller, so eviction is now reachable: after
256 further latches, an older tombstone falls off, and if the gateway is *still* listing the approval
that tombstone named — which is exactly the ambiguous-outcome case the latch exists for — the item is
offered again.

**Why it is not blocking.** Re-offering after 256 latches is strictly better than the shipped
behaviour it replaced, which was re-offering immediately, every poll, forever. U8B narrows the window
rather than opening one. The failure needs a single session to answer 256 prompts while one
ambiguously-answered approval stays outstanding on the gateway.

**Worth it when.** A live run produces enough answered prompts for the bound to be reachable, or the
first report of an approval re-appearing after it was answered. The measurement to take first is how
many latches a real working session accumulates in a day — if that is two orders of magnitude below
256, this stays closed.

**Rejected without measuring.** Raising the cap. The bound exists for R3's fixed-size discipline and
guessing a bigger number trades a known bound for an unknown one.


### ~~Pin a runtime alias while a prompt still references it~~ — GATE FAILED 2026-08-18, superseded

**Not taken, and the measurement is why.** The operator gated this on quantifying the new bound —
four slots plus outstanding prompts per row — before any code moved. Twelve ordinary
land-approve-switch-away cycles against ONE durable row:

```
outstanding prompts   : 12
distinct session ids  : 12
row runtime_ids kept  : ('rt-8', 'rt-9', 'rt-10', 'rt-11')
after age-out at 1000x APPROVAL_STALE_AFTER: 11 prompts remain
=> pinned aliases would be : 11
```

Eleven permanent pins after twelve cycles, growing linearly with no ceiling. Pinning would have moved
U3's unbounded growth out of the alias window and into the pin set, which is the one thing the
four-slot bound exists to prevent.

**The root is one level lower than this entry assumed.** A non-focused session's approval has no
clearing path at all: there is no gateway `approval.expire` event (the catalogue carries
`secret.expire`, `sudo.expire`, `clarify.expire` and `terminal.read.expire` only), `age_out_approvals`
is focus-scoped at `talaria/domain/state.py:1454`, answering requires focus because
`respond_to_prompt` refuses a wrong-session answer, `focus_session` deliberately retains `prompts`
per its own docstring, and nothing drops prompts on row retirement or eviction. At a thousand times
the stale threshold the age-out removed exactly one prompt: the focused one.

**Superseded by the lifecycle fix, and then re-asked and declined on the new number.** The split
landed — `age_out_approvals`'s removal is unconditional, its presentation effects stay focus-scoped
(DECISIONS.md, 2026-08-18) — and the same probe was re-run against it:

```
after 12 land/approve/switch-away cycles on ONE durable row:
  outstanding prompts   : 12      (transient peak, unchanged)
  row runtime_ids kept  : 4
  after age-out at 1000x stale: 0 prompts remain
  => pinned aliases would be  : 0
```

**The bound is now statable, which was this entry's own re-ask condition: steady state zero extra
pins, transient at most eight extra aliases (twelve outstanding against a four-slot window) for at
most one 300-second stale window.** The re-ask condition is therefore met, and the operator declined
it anyway on 2026-08-18, for a reason worth keeping: the split converted pinning's purchase from
closing an unbounded leak into shaving a bounded transient, and that no longer justifies reopening
U3's reviewed fixed-size-row decision. Not "still unbounded" — bounded, and no longer worth the
price.

**What would change that.** A workload where many re-lands of one durable session fall inside a
single stale window, making the transient large rather than eight; or U8 answering the keyless-approval
question (see the plan's U8 section) such that the unplaceable fold can be deleted, at which point
pinning and fold-deletion are one purchase rather than two.

### ~~An approval cannot be answered from the keyboard on macOS~~ — CLOSED 2026-08-16

**Closed.** v0.3's card-owns-focus unit removed the mechanism this entry names: `talaria/ui/prompts.py:1172` now reads `if focus_new and card.action_widget is not None`, so a button-backed approval card takes the caret itself when the composer is empty — the `Input`-only restriction is gone, and with it the dependence on the eaten `F1`, whose binding v0.3 removed outright. Confirmed in use by the operator's hands-on drive of v0.3 in a real terminal against a live gateway (attested 2026-08-16, functionally fine). **Still open from the "Related, same root" paragraph below:** the mis-aimed mouse. The v0.3 diagnosis refuted this entry's mixed-height suspect — Talaria maps clicks by entry id, never by line offset, so the mechanism is below Talaria — and the hand-driven capture that would separate the remaining candidates was cut from v0.3 by decision D13 and has not happened.

**Author.** v0.2 hands-on drive, 2026-08-10
**Priority.** P0
**Effort.** Small for the unblock, Medium to do properly
**Worth it when.** Before anything else in v0.3 — this is the answerability spine, shipped one release ago, not working for the only person who uses it.
**Context.** Full write-up and the surrounding nineteen notes: [docs/analysis/2026-08-10-v0-2-hands-on-notes.md](../analysis/2026-08-10-v0-2-hands-on-notes.md).

A prompt card never takes focus when it mounts unless it is input-backed — `talaria/ui/prompts.py:1171` reads `if focus_new and isinstance(card.action_widget, Input)` — and the comment above it states the consequence plainly: every other kind "is reachable exclusively through the jump". An approval card is button-backed. The jump is `F1`, and `F1` does not arrive: it was pressed repeatedly against a live approval and nothing moved. The card meanwhile prints `enter select · esc decline`; both were tested with focus in the composer and neither does anything. The card was answerable only with the mouse.

`F1` is not mis-bound. `talaria/ui/app.py:770` binds it to `jump_to_prompt` with `priority=True`, the identical form used by `F8`, `F9` and `F10` on the three lines below — and `F8`/`F9` were driven successfully in the same session, so function keys as a class reach the application. `F2` is separately confirmed eaten by macOS Mission Control. What claims `F1` is not yet established, and it decides the fix: an alternate binding on a surviving key is nearly free, whereas "the desktop owns our whole hotkey row" is a redesign.

**Related, same root.** The mouse is the only working path to the card, and the mouse is also mis-aimed — a double-click landed several rows above the clicked line, which is separately why the terminal's own select-and-copy never reaches through the Talaria pane. Suspect: the mixed-height widget layout v0.2 introduced. Undiagnosed.

### ~~Sort the rest of the v0.2 hands-on findings into v0.3~~ — CLOSED 2026-08-16

**Closed.** The triage happened at v0.3 planning, 2026-08-11: the orchestration charter and the decision register under `docs/plans/` are the sort, the "Talaria confirms what it just did" theme this entry flagged became v0.3's release theme, and the `F4` half-description it names was corrected forward in v0.3's changelog. Closed now rather than then only because nobody swept this file after the release.

**Author.** v0.2 hands-on drive, 2026-08-10
**Priority.** P0 to triage, the items themselves range P1–P3
**Effort.** Small to triage
**Worth it when.** At the start of v0.3 planning, before scope is chosen.
**Context.** [docs/analysis/2026-08-10-v0-2-hands-on-notes.md](../analysis/2026-08-10-v0-2-hands-on-notes.md) closes with a sorted candidate list — what passed, seven defects, four undiagnosed items, six design questions, and three findings that are not Talaria's to fix. It is deliberately not scheduled.

Two things in it are worth carrying into planning rather than triaging item by item. First, a theme: four findings that look unrelated — a status row nobody can interpret, a number without its scope, a card advertising keys that do nothing, and a keypress indistinguishable from a dead one — are all one problem, which is that Talaria does not confirm what it just did. That is a tighter release theme than "readability" and absorbs most of it. Second, one defect is in the shipped release notes themselves: both `docs/releases/v0.2.0.md:21` and `CHANGELOG.md:26` say `F4` "sweeps the answerable set", omitting that it first interrupts the in-flight turn (`app.py:776` → `action_interrupt` → the sweep at `:1666`). The omitted half is the destructive one.

### ~~The v0.2.0 release has no wheel and no source distribution attached~~ — CLOSED 2026-08-11

**Author.** v0.2.0 release, 2026-08-11
**Closed.** 2026-08-11, same day, on the operator's explicit go-ahead.
**Context.** [The v0.3 handoff](../plans/2026-08-11-v0-3-session-handoff.md), *Loose ends*; mechanism in [LEARNINGS.md](LEARNINGS.md) under 2026-08-11.

The release was created by hand with `gh release create` about a minute before the tag-triggered `release.yml` reached its final step, which then failed with "a release with the same tag name already exists". Steps 1–15 of that run passed on the tagged tree, so the release was fully validated and merely undelivered.

Repaired by deleting the GitHub release — never the tag, which stayed at `465649e4` throughout — and re-running the failed workflow run. `v0.2.0` now carries `talaria-0.2.0-py3-none-any.whl` and `talaria-0.2.0.tar.gz`, built by continuous integration from the tagged tree, matching how `v0.1.0` got its assets. The recreated release was verified byte-for-byte against a backup taken before the delete: identical body hash, title, and prerelease flag.

**The rule that outlives it:** push the tag and let `release.yml` publish; never create the release by hand. After any release, check the assets rather than the release page — a validated-but-undelivered release looks entirely normal from the outside.

### Decide which Validate jobs are required status checks on `main`

**Author.** v0.2.0 release, 2026-08-11
**Priority.** P1
**Effort.** Small
**Worth it when.** Before the next release, since a release is when the gap costs the most.
**Context.** [LEARNINGS.md](LEARNINGS.md) under 2026-08-11.

Branch protection requires exactly `python-check (3.12)` and `python-check (3.13)`. The Node `check` job and both `install` jobs run but cannot block a merge. `check` failed on a Prettier violation in ADR-0006 and merged red twice — with the block-markdown work (`05ecaa6`) and with the v0.2.0 release merge (`d925891`) — before it was noticed and fixed in `5211a8c`.

The argument for adding them: a job that cannot block will drift again, and it already did, straight through a release. The argument against: `check` covers the TypeScript reference recorder, whose failure mode is narrow. Either answer is defensible. **What is not defensible is leaving it undecided**, which is the state that produced two red merges. Audit with `gh api repos/infiquetra/talaria/branches/main/protection`.

**Re-verified 2026-08-16.** Protection still requires exactly `python-check (3.12)` and `python-check (3.13)`; the decision is still unmade.

### ~~ADR-0006 is `proposed` and its own acceptance condition has been met~~ — CLOSED 2026-08-16

**Closed.** The disagreement this entry warned about is gone: the record was flipped to `accepted` during the v0.3 run (commit `c32424c`, "flip ADR-0006 to accepted — U6 replay gate passed 24/24"), and the file reads `Status: accepted`, verified 2026-08-16 by reading it.

**Author.** v0.2.0 release, 2026-08-11
**Priority.** P2
**Effort.** Trivial to flip, small if the answer is to hold it open with a reason
**Worth it when.** Next time anyone touches the transcript pane, at the latest.

[ADR-0006](../../platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md) states its condition in its own opening note: `proposed` rather than `accepted` "until that gate runs green under the restated claim". The gate ran green, 24 of 24, confirmed across three runs ending at `2e96324` and published in [the gate results](../analysis/2026-08-09-block-markdown-gate-results.md). Either flip the status or record why it is being held open. A record whose stated condition is satisfied while its status disagrees is worse than either state alone, because a reader cannot tell which one is stale.

### ~~The shipped `F4` description omits the destructive half~~ — CLOSED 2026-08-16

**Closed.** The decision this entry insisted on was made and shipped with v0.3: correct forward, leave the shipped notes as published. The correction of record is the v0.3 changelog's Fixed entry and the v0.3 release notes' "One correction to the v0.2.0 notes" section; `docs/releases/v0.2.0.md` deliberately stays as it was.

**Author.** v0.2 hands-on drive, 2026-08-10
**Priority.** P2
**Effort.** Trivial either way; the decision is the work
**Worth it when.** Whenever v0.3's changelog entry is written, at the latest.

Both `docs/releases/v0.2.0.md:21` and `CHANGELOG.md:26` say `F4` "sweeps the answerable set". It first interrupts the in-flight turn — `app.py:776` binds it to `action_interrupt`, which sweeps at `:1666` only after the interrupt is confirmed. An operator following the release notes would stop a running turn expecting to answer a prompt; this nearly happened during the hands-on drive, from an instruction that quoted the notes faithfully.

Both files are published. Decide deliberately between correcting them in place, which edits a shipped release's notes, and correcting forward in v0.3's changelog. No position is taken here beyond insisting the choice be made rather than forgotten.

### ~~A blocking prompt cannot be answered without guessing how many times to press `tab`~~ — CLOSED 2026-08-09

**Closed.** The v0.2 answerability spine (pull request #45, merge `529928c`) shipped the F1 jump
(U1), per-card hint lines (U2), and the card-level focus tint (U3); all three were verified live in
the U8 acceptance run — legs 1 and 2 of
[docs/plans/2026-08-09-u8-live-acceptance-results.md](../plans/2026-08-09-u8-live-acceptance-results.md).
The "two smaller things" below also shipped: a confirmed `F4` now sweeps outstanding prompts
(leg 4), and every card names its operating keys.

**Priority.** P0 — an approval that cannot be answered in time is an approval that expires, and the
same invisible focus put a value meant for a hidden credential field into the visible message box.
Found 2026-08-07 driving F2 through F6 live for row 6; full write-up in
[docs/plans/2026-08-07-row6-live-evidence-results.md](../plans/2026-08-07-row6-live-evidence-results.md).

**Evidence, all from one session.** An approval expired while its answer was being aimed — the reply
came back "the gateway had no approval waiting — nothing was resolved" — and it happened twice, once
for a single approval and once for `deny all` across three. One `tab` from the sudo card put focus on
the composer, and the typed answer appeared in plain text where a chat message goes; it was a canary,
but a real sudo password would have been one `enter` from the transcript. Two other typed answers
were absorbed by whatever held focus and went nowhere. The tab distance to a control varied between
**3 and 7** in the same session, because it depends on what else is on screen.

**Why the existing styling does not solve it.** The focus styles exist —
`AgentRow.-interruptible:focus` sets a 20% accent background, `Button` takes reverse video — and they
are only legible once you already know which row to look at. Locating a control took an ANSI-level
dump of the screen.

**Worth it when.** Before F2 is claimed to work for anybody but its author. This is the same shape as
the picker defect: the machine does the right thing when driven correctly and nothing tells the
operator what correct is.

**Two smaller things from the same run**, neither blocking and both worth their own entry if they are
not fixed alongside: an outstanding blocking prompt survives `F4`, so the next submission queues
behind a card the operator may think they cancelled; and nothing on a prompt card names the keys that
reach it, unlike the picker dialog's hint line, which names every key that does something.

### ~~`--resume` reattaches to the session and throws away its entire conversation~~ — CLOSED 2026-08-09

**Closed.** U4 of the v0.2 answerability spine (pull request #45, merge `529928c`) renders the
`session.resume` reply's history as committed transcript entries, names an omitted history via
`messages_omitted`/`message_count`, and was verified live in the U8 acceptance run — leg 6 of
[docs/plans/2026-08-09-u8-live-acceptance-results.md](../plans/2026-08-09-u8-live-acceptance-results.md)
rendered all sixteen prior messages before any live event.

**Priority.** P1 — the flag works and the feature does not. Found 2026-08-07 by the reply-side pass
that closed row 6, then confirmed on screen rather than left as a code reading.

**Evidence, measured.** `talaria --resume` was run against a session that had just held a real
exchange. The gateway's `session.resume` reply carried `message_count = 3`, a `messages` array of
three entries, and `messages_omitted = False` — it withheld nothing. **Talaria rendered an empty
transcript.** Nothing follows the reply on the wire that could carry the history either: the frames
after it are `sessions.changed`, `session.info` and `session.reclaimed`, so the reply's `messages`
array is the history, not a summary of one delivered elsewhere.

**Mechanism.** `TalariaApp._land_session` (`talaria/ui/app.py`) reads exactly one field out of the
reply, `session_id`, and returns. `messages` and `message_count` appear nowhere in the package outside
the compatibility baseline — grep finds two hits, both in `talaria/domain/compat.py`. The transcript
is built from streamed events, and a resume streams no events for messages that already happened.

**Why nothing caught it.** Row 19's acceptance run graded `--resume` **pass** on 2026-08-07, and that
grade is correct for what it measured: which session each startup path lands in, settling KTD7's
precedence chain. Nobody asked whether the conversation appeared. A startup path that lands correctly
and shows nothing passes a landing test.

**The smaller case, worth fixing in the same change.** `messages_omitted` is a `bool` the gateway sets
on all three `session.resume` success paths (`methods_session.py:466`, `:551`, `:712`). When it is
true the gateway sends `messages: []` while `message_count` still reports the real length — "there are
*n* messages and I am not sending them". Talaria reads neither, so once the history renders at all,
a withheld history would render as a complete short one unless the flag is read too. The key was not
in `talaria/domain/compat.py` until this pass added it.

**Effort.** Medium. The history has to be projected into the same transcript state the event stream
feeds, which is where the design question sits — not in the reading of the reply.

**Worth it when.** Before `--resume` or `--session <id>` is recommended to anybody. A resumed session
is exactly where a reader trusts the transcript to be the whole conversation, and right now it is
reliably empty.

### ~~`absent_capability` blames the gateway's version for a mistyped profile name~~ — CLOSED 2026-08-17

**Closed by.** Two units, one rule. v0.4's U4 fixed the reported HTTP case: a 404 carrying a
`profile` parameter is re-probed with a bare GET, and a route that answers 2xx or 405 to the bare
path reclassifies the failure as `unknown_profile`
(`_disambiguate_absent_capability`, `talaria/transport/admin.py`). v0.4's U5 generalized the rule to
the JSON-RPC probe path it had never been applied to: a `-32601` on a request that carried parameters
is re-asked bare before `missing` may be returned, and a bare re-ask that is answered reclassifies
the verdict as `parameter-invalid` (`_reask_bare`, `talaria/transport/compat_check.py`). The same
sentence governs a replayed recording, so a corpus holding only a parameterized failure reproduces
`parameter-invalid` rather than inventing an absence
(`classify_recorded_reply`, `talaria/domain/compat.py`). Pinned by
`test_a_parameterized_failure_is_reasked_bare_before_absence` and
`test_a_recorded_parameterized_method_not_found_is_not_read_as_absence`, both verified to fail with
the re-ask removed. The generalizable rule is in [LEARNINGS.md](LEARNINGS.md), "Absence needs two
calls, not one, wherever one status code does two jobs".

**Priority when open.** P1 — the message actively misdiagnoses, and it fires on the most likely operator error.
Found 2026-08-07 while exercising row 19's absent-capability branch against a live gateway.

**Evidence.** `GET /api/model/options` returns 200; `GET /api/model/options?profile=no-such-profile`
returns **404**. Talaria's admin client maps any 404 on an admin path to `absent_capability` with the
message "this gateway does not serve /api/model/options; it predates the admin model API"
(`talaria/transport/admin.py`, the 404 branch of `_http_error`). Reproduced live: the gateway plainly
does serve the endpoint, because the unparameterized request to it succeeded moments earlier.

**Mechanism.** The 404 is doing two jobs. Hermes returns it both for "this route does not exist" (a
genuinely old gateway) and for "this route exists but that profile does not". The client cannot tell
them apart from the status code alone, and it guessed the rarer one. A version skew is a once-a-release
event; a mistyped or stale profile name is an everyday one.

**Why it matters more than a wording nit.** The message names a cause the operator cannot act on and
hides the one they can. Told the gateway predates the API, the reasonable next move is to upgrade
Hermes — which will not help, because the profile name is still wrong.

**Effort.** Small. The two cases are distinguishable without guessing: a request carrying a `profile`
parameter that 404s should first re-ask without it (or consult `GET /api/profiles`, which the client
already calls) and report "no such profile" when the bare path answers. Only a 404 on the bare path is
evidence of an absent capability.

**Worth it when.** Before the profile picker is used by anyone but its author — U4 makes profile-scoped
requests routine, which makes this the everyday failure rather than a corner case.

### ~~The model picker is a numbered list, not a picker — KTD3's anti-modal decision needs reopening~~ — CLOSED 2026-08-07

**Closed by.** The picker redesign. The operator chose the modal shape; KTD3 is overturned and the
reasoning is recorded in [DECISIONS.md](DECISIONS.md), "The picker is a modal dialog, overturning
KTD3 — a listing is read, a picker is operated". `talaria/ui/dialog.py` is the dialog,
`talaria/domain/selection.py` the pure selection model behind it, and the foldable `PickerRegion` is
removed rather than kept alongside. Arrows move, `enter` selects, typing filters, `escape` clears the
filter then pops a stage then closes; `right` and `left` are second names for select and back. 58 new
tests; the selection rules are asserted without a terminal, the key routing with one.

The operator's first round of feedback landed in the same change: the dialog now opens on the model
in use, tracked by Talaria itself because the gateway does not publish it (see
[LEARNINGS.md](LEARNINGS.md), "The picker marked the wrong model after a switch"), and each row shows
the number `/models <n>` actually takes rather than its position on the stage.

**Two things this closure did not do**, both smaller than the original item and neither blocking:
the dialog is built from the listing held at the moment it opens and a refetch arriving while it is
up does not restock it (a selection against a stale listing is still refused on the epoch check, so
this is a display staleness, not a wrong dial); and `ctrl+p` remains Textual's, see the P2 below.

**Priority.** ~~P0 — the picker is the headline feature of the 2026-08-06 work and it does not do the
thing its name promises. Raised by the operator on 2026-08-07, on first live use.~~

**What was observed.** `/models` renders a read-only list and selection requires typing `/models <n>`.
`talaria/ui/picker.py` has **no key handling at all** — no highlight, no cursor, no navigation — exactly
like `talaria/ui/palette.py`, which it was modelled on. `F6` toggles the region and that is the whole of
the interaction. The operator's words: "I expected it to work like the Hermes TUI. `/models` would open
up a dialog picker of some sort, not just a list I now need to pick a number."

**Why it shipped this way.** KTD3 of the 2026-08-06 plan chose a foldable region over a modal overlay,
citing `talaria/ui/palette.py:1-22`'s rejection of a modal search box on the grounds that it "would put
a second focus owner in front of the composer." That reasoning is sound as far as it goes and it was
applied consistently. What it did not weigh is that the command *listing* is something you read, while a
*picker* is something you operate — the same answer does not serve both.

**The actual obstacle, so nobody re-derives it.** The composer owns `Enter` as "send message"; a picker
wants `Enter` as "select the highlighted row". KTD3 avoided the collision by giving the picker no focus
at all, and the numbered list is the price of that dodge. Hermes does not face this because its picker
is modal — while it is open, nothing else owns the keyboard.

**Two shapes were put to the operator.** A modal dialog (arrows move, `Enter` selects, `Esc` cancels,
typing filters — overturns KTD3 and needs its own recorded decision), or a navigable region where
`Enter` selects only when the composer is empty (keeps KTD3, but the conditional `Enter` is a rule the
operator has to learn). The operator deferred the choice to finish the row-19 acceptance run first, so
**the shape is still open** — do not treat the modal as decided.

**Effort.** Medium. `PickerRegion` already renders providers, models, the current marking and the
unauthenticated/warning states; what is missing is a selection model and key routing. The modal shape
additionally needs a focus owner and a restore-focus-on-close path.

**Worth it when.** Before anyone calls the picker done. Row 19's acceptance run is being completed
against the shipped interface deliberately, with the operator's agreement, so this does not block the
gate — but it does block claiming the feature is finished.

### ~~TranscriptPane.reconcile desynchronizes from the projection~~ — CLOSED 2026-08-03

**Priority.** P0 — this is what the repaired validation gate failed on, 2026-08-03. It blocked ADR-0005 and the milestone-1 merge.

`TranscriptPane.apply` finds the common prefix between the last snapshot and the new one, drops the changed tail, mounts what is new, and stores the divergence point as a floor the next scan starts from. Storing that floor was the defect: the floor could land *inside* the provisional streaming block, above lines that were about to move, and nothing ever looked at them again.

Measured on the recorded 5,773-frame corpus at the settled checkpoint, after forced flushes: **274 lines rendered against 275 projected**, first misalignment at index 251, and one line of real conversation text rendered nowhere at all — neither mounted nor condensed. The operator silently loses a line.

**The mechanism recorded here on 2026-08-03 was wrong, and the correction is the useful part of this entry.** It said a *transient notice line appears mid-transcript and later disappears*. No line ever disappears: the domain transcript is strictly append-only and entry text is immutable, which a replay of the stress corpus confirms — 15 below-floor incidents, all of one class, zero committed lines ever changed. What actually moves is the **provisional streaming block**, which the projection places *after* the committed lines. Committing an entry while a turn is still streaming pushes every provisional line down by the length of that entry. Two consecutive snapshots then agree on a provisional line whenever the streaming text did not change between them — which multi-line streaming makes constant, since each delta only rewrites the last line — and the floor advanced on that coincidence. First occurrence in the stress corpus is frame 31, where the floor reached line 1 while **zero** entries had been committed: the floor was entirely inside the streaming block.

Both remedies proposed here followed from the wrong mechanism and neither would have worked. Reconciling the full window each tick costs O(transcript) per 50ms tick, which is the cost KTD14 exists to bound. Making notice lines non-transient fixes nothing, because the notice lines were never transient — they are ordinary committed entries, and it is the streaming block that moves past them.

**Closed by.** `TranscriptView` now publishes `committed_lines`, the boundary a consumer cannot compute for itself, and the pane clamps its stored floor to it: `self._stable = min(stable, view.committed_lines)`. Truncation still uses the true divergence point, so a streaming delta churns one widget rather than the whole block. Pinned at unit size by `tests/ui/test_transcript_bounds.py::test_an_entry_committed_mid_stream_shifts_the_provisional_block_correctly` (the symptom) and `::test_the_stable_floor_never_advances_into_the_provisional_block` (the invariant); both verified to fail against the pre-fix implementation.

**That fix alone did not turn the gate green, and the reason is worth keeping.** With the floor corrected the pane began re-deriving the provisional block — real work it had been skipping — and two further defects surfaced on checks that had been *passing* while the bug suppressed them. `condensed_count` was a cumulative eviction tally doubling as the window's start index, and reached **7,493 on a transcript of 4,454 lines**, so the pane rendered a wrong slice of a correct projection; it is now derived from an explicitly tracked position. And the mount cap was enforced *after* the mount, so a tick that re-derived the whole block transiently held **667 widgets against KTD14's ceiling of 600**; the pane now condenses from the top before mounting, and the bound is `mount_cap + 1` at every instant. The gate passed on the third run with all thirteen checks green.


### ~~Run the Textual validation gate~~ — CLOSED 2026-08-03, verdict **pass**

**Author.** Reconsidered language and TUI framework analysis
**Priority.** P0
**Effort.** Medium
**Worth it when.** Before adding product behavior beyond the current TypeScript bootstrap shell.
**Context.** Drive one bounded Textual projection from a framework-neutral Python reducer and the existing frame-log contract. Prove coalesced streaming, bounded transcript mounting, scroll anchoring, deterministic `run_test()` and `Pilot` behavior, selected pseudo-terminal behavior, framework-independent domain state, strict typing and linting, and clean `uv tool install` launch.
**Amended 2026-08-02 by ADR-0004.** The fallback is no longer Go with Bubble Tea. The language is settled as Python, so a framework failure selects a different Python presentation layer — which is why identifying one is now a prerequisite rather than a contingency. Choose the vertical slice from the Hermes terminal UI feature inventory rather than from a generic renderer stress list, so the gate produces a prototype instead of a harness.

**Closed by.** Unit U5 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), on the third run of the day. All thirteen threshold checks passed; measurements, corpus identities, the full three-verdict sequence and explicit non-claims are in [Textual validation gate results](../analysis/2026-08-03-textual-validation-gate-results.md), with the machine-readable record at [`evidence/2026-08-03-textual-validation-gate.json`](../analysis/evidence/2026-08-03-textual-validation-gate.json). Re-runnable as `uv run talaria gate --corpus <recording> --deltas 50000`, which exits non-zero on a fail verdict. Headline numbers: 501 mounted line widgets against a ceiling of 600 (peak, not steady state — the pane condenses before it mounts), 44.3 MB resident growth against 300 MB, 15.3 coalescing flushes per second under sustained streaming against 25, and zero content loss across 24 checkpoints. The framework choice is [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md), `accepted`.

**What the gate did not measure, so it is not mistaken for closed.** Real-terminal behaviour (tmux, a live emulator, IME composition) was not exercised — the run is headless, and those belong to U10's acceptance. The fallback assessment of `prompt_toolkit` stands unretired.
**Refs.** [Reconsidered language and TUI framework analysis](../analysis/2026-08-02-language-and-tui-framework-analysis-reconsideration.md), [ADR-0004](../../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md), [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md)

### ~~Identify and assess a Python fallback presentation layer~~ — CLOSED 2026-08-02

**Author.** ADR-0004
**Priority.** P0
**Effort.** Small
**Worth it when.** Before the Textual gate runs, so a failure has somewhere to go.
**Context.** Every analysis in the chain evaluated Textual as the only Python candidate; the others were all in other languages. Settling the language on Python therefore leaves the fallback set unevaluated. If Textual fails on transcript cost or pseudo-terminal correctness, nobody has assessed what replaces it. Name at least one alternative and check it against the same gate criteria — enough to know it exists and is plausible, not a full comparative analysis.
**Closed by.** Unit U4 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), discharging PC8/KTD12. `prompt_toolkit` is assessed against the gate criteria to plausibility depth (with `urwid` recorded as secondary candidate) in [Python fallback presentation layer](../analysis/2026-08-02-python-fallback-presentation-layer.md), dated before the U5 gate verdict. Verdict: plausible on all five assessed criteria — bounded transcript strategy, streaming coalescing, multi-line editing/bracketed paste, headless test story, install cleanliness.
**Refs.** [ADR-0004](../../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md), [Python fallback presentation layer](../analysis/2026-08-02-python-fallback-presentation-layer.md)

### ~~Prove the Hermes transport seam~~ — CLOSED 2026-08-03

**Author.** Project bootstrap
**Priority.** P0
**Effort.** Medium
**Worth it when.** The prototype shell is ready for the first real integration slice.
**Context.** Talaria needs capability discovery, session lifecycle, prompt streaming, cancellation, and approval handling before UI work can be judged against real Hermes behavior.

**Closed by.** Unit U7 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md): `talaria/transport/attach.py`, `talaria/transport/rpc.py`, `talaria/transport/credentials.py`, and `LiveSource` in `talaria/transport/source.py`, with the live-submit and interrupt wiring in `talaria/ui/app.py` and `talaria/ui/composer.py`. Evidence is `tests/transport/` (78 tests) plus `tests/ui/test_live_wiring.py`.

**What is proved, and by what.** Each of these runs against a stub gateway that is a *real* WebSocket server on loopback, dialled with the real `websockets` client — not a hand-written double, because a double would prove that Talaria calls a function with a string rather than that a server which only reads the URL query can authenticate the connection.

- **Attach over the authenticated path.** The credential arrives as the `?token=` URL query parameter and nowhere else, matching the pin's `ws.query_params`-only check (`hermes_cli/web_server.py:14443-14524`).
- **Credential acquisition, per dial.** Environment, then a `?token=` already on `TALARIA_GATEWAY_URL`, then `<config_dir>/credentials` (refused unless `0600` or stricter), then a non-echoing prompt — proved against a real pseudo-terminal, and confirmed to fail if the prompt is swapped for `input()`.
- **Prompt streaming.** Identical ordered frames from a file and from a socket produce identical domain state after *every* frame, not merely at the end (AE16 / R31).
- **Cancellation.** `session.interrupt` marks the turn cancelled only on a confirmed reply; an interrupt whose outcome was lost with the transport changes nothing and says so (R4, AE8).
- **Reconnect.** Distinct visible states, a credential re-read on every dial (a rotated token is picked up), no duplicate transcript entries, outstanding prompts re-keyed once, and sub-agent terminal states preserved against a re-announced start (R35, F6).
- **Lost outcomes.** An RPC interrupted by a disconnect resolves to `unknown`, and a reply from a stale connection epoch is counted and discarded rather than resolving a reused request id into a false success (KTD13).
- **Recording.** Both directions now pass the U2 redaction boundary before any frame-log write.

**What is deliberately NOT closed by this, so it is not mistaken for done.**

- **No live attach against a real Hermes gateway was performed.** Every assertion above is against the stub. The plan's verification clause asks for a smoke attach against a harness-launched local gateway; that belongs to U10's acceptance run and is queued below.
- **Capability discovery and session lifecycle** — the two other clauses of this item's original context — are R34 and R2, owned by U10 and U3/U10 respectively. This item closes the *transport seam*; it does not close the startup path that chooses a session, which is why a bare `talaria` run still exits rather than dialling.
- **Approval and the four blocking bridges** are U8.

**Refs.** [Project direction](../analysis/2026-08-01-hermes-tui-project-direction.md), [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md) unit U7

### ~~Smoke-attach the transport against a real local Hermes gateway~~ — CLOSED 2026-08-16, by operator attestation

**Closed.** Live attaches against a real gateway have happened repeatedly since this was queued: the recorded 2026-08-04 live run (R3's corpus), the 2026-08-07 live credential verification both ways, the v0.2 hands-on drive, and the operator's own hands-on drive of v0.3 against a live gateway, attested 2026-08-16 as functionally fine. On that attestation the operator retired this as scheduled work. **What this closure does not claim:** the itemized captures below — the auth-failure close shape, a live frame log diffed against the AE3 sweep, the real catalogue's entry count and categories, a real `paste.collapse` file, what a real slash worker prints — were never individually recorded. They stand as open observations for whenever one of those seams misbehaves, not as gates.

**Author.** v0.1 milestone-2, unit U7
**Priority.** P0
**Effort.** Small
**Worth it when.** U10's acceptance run, which already launches a dedicated loopback gateway instance with an injected token and a scratch session.
**Context.** U7's whole evidence base is a stub server that reads the URL query the way the pin says Hermes does. That settles the *client* side, and it cannot settle one thing: whether a real Hermes accepts what Talaria sends. Two specific unknowns are worth naming rather than assuming away.

1. **How an authentication failure reaches the client is unverified.** `gateway_ws` calls `await ws.close(code=4401)` *before* accepting the upgrade (`hermes_cli/web_server.py:15615-15617`), and whether the client observes that as a WebSocket close with code 4401 or as an HTTP handshake rejection is decided by the ASGI server, not by Hermes. `classify_dial_error` handles both shapes deliberately; a live run would say which one actually occurs, and whether the HTTP status is 401 or 403.
2. **The frame log recorded live has never been checked against a real corpus.** The redaction assertions are against frames this repository wrote.

**Do.** Attach, submit one prompt, interrupt it, drop the socket, confirm the reconnect, and diff the resulting frame log's redaction markers against the AE3 sweep.

**Extended by U9, 2026-08-03.** U9's own verification clause — "a live isolated run dispatches one real catalogue command of each available shape and one collapse round-trip" — is unmet for the same reason and belongs on this run rather than on a second one. **No Hermes gateway was attached at any point in the U9 session.** Everything U9 asserts is against `tests/transport/conftest.py`'s loopback stub, whose reply bodies are transcribed from the pinned handlers; that settles Talaria's side of each contract and settles nothing about what a real gateway sends. Five specific unknowns:

1. **Which result shape each real command actually returns is unverified.** The stub returns the shape the test asked for. A live run would say what `/model`, `/status`, a real skill command and a real bundle answer with, and whether any of them carries a field combination the three-destination routing reads differently from intended.
2. **Whether a real `commands.catalog` decodes cleanly is unverified.** The stub serves eight entries in the documented shape. A real one carries the full registry plus quick commands plus a skills scan, and its `canon` map is the thing alias resolution depends on.
3. **Three client-local entries have never been seen in a real catalogue.** The name-plus-category rule is derived from the pinned source, not observed. One live `commands.catalog` settles whether `/density`, `/logs` and `/mouse` arrive under category `TUI` as the source says. `/sessions` is **not** among them and is not an open question: the registry defines `CommandDef("sessions", …, "Session")` (`hermes_cli/commands.py:180` at `7f4d15515`), so the catalogue builder's dedup guard drops the `TUI` extra of that name and serves it as an ordinary dispatchable command. Talaria treats it as one.
4. **`paste.collapse` has never written a real file.** The placeholder text Talaria inserts is the gateway's, so its exact shape — and whether the path in it is one an operator can open — is unobserved.
5. **The `slash.exec` / `command.dispatch` split has never been exercised against a real worker.** The pinned source is unambiguous about which handler serves what, and Talaria now calls `slash.exec` first and falls back exactly as Hermes's own client does. What a stub cannot show is what a real slash worker *prints* — whether a command's output arrives as one block, whether a warning accompanies it, and how long the first call takes while the worker subprocess is spawned on demand (`methods_tools.py:1177-1194`).

**Do, additionally.** With the session from the smoke-attach: fetch the catalogue and record its entry count and category names; dispatch one command of each shape the catalogue actually offers; paste 400 lines and confirm the placeholder and the file it names.

### R1's environment clause is unmet, and no change to Talaria can meet it

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure)
**Priority.** ~~P0~~ — **closed 2026-08-07**, see the closing block at the end of this entry. It was P0 because it was a security requirement recorded as met nowhere, with a mitigation that was an operator procedure not written down outside this journal and the verdict document. The mitigation is now in `README.md` and the grading question is answered; the underlying platform fact is unchanged and is not what the P0 was about.
**Effort.** Small (a documentation decision), or Medium (removing the environment credential source entirely).
**Worth it when.** Before Talaria is used by anyone other than its author on a shared machine.

**Context.** R1 asks that a running Talaria's command line **and environment** carry no credential. The argv half holds and is now measured across every shipped entry point that can hold a credential — the bare launcher, `talaria record`, and `talaria refresh-credential` — each launched as a real subprocess holding a live credential, showing no token, no `?token=` URL and no endpoint in `ps -ww` / `/proc/<pid>/cmdline`, with every environment entry carrying the credential one the process was launched with, and a classification guard that fails a new subcommand until someone declares whether it belongs on this list (`tests/transport/test_process_surface.py`). **This sentence previously read "The argv half holds and is measured" citing only the bare launcher.** That was an overclaim: the cited test built the launcher alone (`parse_args([])`), so it said nothing about `talaria record`, which at the time took its credential as a `?token=` on its own command line — the exact leak this file's environment half describes, just reached through argv instead. The 2026-08-05 credential-and-bridge-drift remediation plan (`docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md`, units U2–U3) closed both the leak and the narrow probe; this paragraph is corrected rather than left to quietly become true on its own. The environment half cannot hold when the operator uses KTD11's highest-precedence source, `HERMES_DASHBOARD_SESSION_TOKEN`: the kernel snapshots the environment block at `exec` and `/proc/<pid>/environ` serves that snapshot for the life of the process, so `os.environ.pop` changes nothing a reader can see; macOS exposes the same to the owning user through `ps -E`.

The mitigation exists and is measured: KTD11's third level, a `0600` file at `<config_dir>/credentials`, leaves the process environment clean (`::test_the_credential_file_route_keeps_the_environment_clean`).

**Do.** Decide one of two things and write it into the README rather than only here. Either (a) document the credential file as the supported route whenever the process surface matters, and keep the environment variable as a convenience with a stated caveat; or (b) drop the environment variable from the precedence chain, accepting that it is the variable Hermes's own dashboard publishes and that dropping it costs every operator a setup step.

**Decided 2026-08-06 — option (b) — and this entry stays open on a narrower residual.** Unit U3 of the model-picker plan deleted `TOKEN_ENV_VAR`, the `_resolve` branch that read `HERMES_DASHBOARD_SESSION_TOKEN`, and the `"environment"` label from `CredentialSource`; `README.md` now documents the three surviving routes and names the credential file as the one with no environment footprint. The rationale, and the exact limit on how far the daily-driver verdict's row 13 may be re-graded on it, are in `DECISIONS.md` under "`HERMES_DASHBOARD_SESSION_TOKEN` leaves the credential chain, and row 13 may be re-graded exactly one step — not to *met*". **The entry is not retired**, because the decision closed the *question* and not the *requirement*: route 1 — a `token` on `TALARIA_GATEWAY_URL` — survives by explicit design and is an environment variable carrying a credential, and an inherited variable stays readable from `/proc/<pid>/environ` and `ps -E` regardless of what Talaria reads. Removing route 1 needs Hermes to hand a client its session token by some route other than the endpoint URL, which is work in Hermes and out of this repository's scope. Re-scope this entry to that, rather than closing it: the P0 label now attaches to the residual, not to the undecided question.

**Route 1 was removed on 2026-08-07, and the sentence above about it needing work in Hermes was wrong when written.** Hermes had had such a route since 2026-08-04 and this repository was already using it: `talaria refresh-credential` fetches the dashboard index unauthenticated and reads the injected session token out of the page. Nothing in Hermes changed. `resolve_endpoint` now **refuses** a configured endpoint carrying a credential — in `TALARIA_GATEWAY_URL`, in the credential file's `url` key, or on the `talaria record` command line — instead of stripping it, and `LoopbackTokenProvider` has no environment injection point at all. Two routes remain: the `0600` credential file and the interactive prompt. Verified against a live gateway both ways on 2026-08-07: an environment-free configuration (both halves from the file) recorded a `gateway.ready` frame under `env -i`, and an exported credential-bearing endpoint was refused with the value appearing zero times in stderr. Full account and the two false premises in `DECISIONS.md` under "The endpoint URL stops being a credential source, and the decision that kept it is reopened because both its premises measured false".

**What is left of this entry, and it is one thing.** An inherited `HERMES_DASHBOARD_SESSION_TOKEN` stays readable from `/proc/<pid>/environ` and `ps -E` for the life of the process. No code change reaches it: the kernel snapshots the block at `exec`, and re-execing with a scrubbed environment narrows nothing, because the same-user reader who can read Talaria's environment can equally read the launching shell's. The mitigation is an operator procedure — do not export a gateway credential — and it is now documented in `README.md` as the only thing left to do.

**Do, and it is a grading decision rather than code.** Decide whether row 13 grades *Talaria* or *the machine Talaria runs on*. Every other row in that table grades Talaria; this one currently grades a variable Hermes publishes and the operator's shell exports. Re-scoping it with a named, falsifiable condition — the way `terminal.read.respond` was re-scoped out of row 6 — is available and is the operator's call. The alternative is that row 13 never reaches *met*, which is a defensible answer but should be a chosen one.

**Closed 2026-08-07 by that decision: row 13 grades Talaria.** The operator's reasoning was that every other row in the table grades Talaria and this one should too. Row 13 is re-titled from "the environment carries no credential" to "Talaria places no credential in its environment", graded `met`, and the inherited variable is scoped out of it — named, not dropped, with the falsifier and the ruled-out reading in the verdict document under §The inherited variable is out of scope for row 13. That was the last condition on the v0.1 daily-driver gate, so the verdict moved to **READY** in the same edit. The rationale and the alternatives rejected are in `DECISIONS.md` under "Row 13 grades Talaria, not the machine Talaria runs on".

**What did not change, and is the thing to check before quoting this entry.** R1's environment clause is still not met when an operator exports a credential. The inherited variable is still readable from `/proc/<pid>/environ` and `ps -E` for the life of the process. Nothing about the kernel, the platform, or Talaria's reach over its own inherited environment moved — only what row 13 is asking about. The falsifier is four existing tests, checked on every suite run: if any member of `CredentialSource` names an environment variable, if `LoopbackTokenProvider` regains an injection point, if a configured endpoint is read for a credential rather than refused, or if a running Talaria's environment carries a credential name it was not launched with, the exclusion is wrong and the row reopens.

**The operator procedure this entry existed to force into writing is written, and stands.** `README.md` documents the `0600` credential file as the supported route and `talaria refresh-credential` as the way to write it. The one remaining operator job is one line: do not export a gateway credential.

**The cost of option (b) fell sharply on 2026-08-04.** The argument for keeping the environment variable was that the file route costs "every operator a setup step" — and that step was the awkward part, because the dashboard mints its session token at server start and keeps it in memory only, so the file had to be rewritten by hand after every dashboard restart, with the token passing through a shell prompt and into shell history on the way. `talaria refresh-credential` (`talaria/transport/refresh.py`) now writes that file from the page the dashboard already serves its own web UI, at `0600`, preserving the file's other keys, without the value ever reaching a terminal. The recurring step is now one command that prints nothing secret. That does not decide the question — it removes the practical objection to the answer that is better for R1.

**Also relevant:** the environment variable is no longer the only route that survives a restart unattended, which was the other implicit argument for it.

**Do not.** Widen R1's wording so the argv half satisfies it. The failing half is asserted by a test that asserts the *failure*, so if Talaria ever does scrub its inherited environment that test goes red and somebody has to remove it on purpose.

### ~~R2, R3 and the F1/F7 live demonstrations are unmet — the whole live acceptance run~~ — CLOSED 2026-08-16, by operator attestation

**Closed.** The operator has driven the interface by hand in a real terminal against a live Hermes gateway — the v0.2 hands-on drive (2026-08-10) and again on v0.3, attested 2026-08-16: functionally fine, with interface feedback deliberately held for the theming-and-readability work. On that attestation the remaining steps are retired as scheduled work. **What was never produced:** step 1's three-startup-paths capture and step 3's compatibility-check output were not recorded, and `docs/plans/2026-08-06-u6-row19-operator-checklist.md` was never run as written. The v0.1 daily-driver verdict is a dated artifact and is not re-graded by this closure.

**Author.** v0.1 milestone-2, unit U10
**Priority.** P0 — this is the gap that makes the daily-driver verdict *not ready*.
**Effort.** Medium.
**Worth it when.** Immediately; it is the first thing that should happen after this branch lands.

**Context.** U7's queued smoke-attach item above names the transport half. U10's own verification clause names three more, and none of them happened: **R2** (a real attach resolving KTD7's precedence chain against a running gateway and landing in the expected session), **R3** (one prompt streamed to completion through the live path, its transcript compared against replay from the same recorded frames), and the origin's **F1** and **F7** flows demonstrated live in an isolated session.

What U10 *did* build is the path they would exercise. `talaria/cli.py:build_live_app` assembles the live shell, and `TalariaApp.open_session` resolves the selection into `session.create`, or `session.most_recent` followed by `session.resume`, or a direct `session.resume` — all proved against the loopback stub in `tests/transport/test_session_startup.py`, including the order (compatibility probes before the open) and the parameters as the *server* received them. Nothing in it has been answered by Hermes.

**Do.** Run the smoke-attach above and, in the same isolated session: (1) launch bare, with `--resume`, and with `--session <id>`, and record which session each lands in; (2) submit one prompt and let it stream to completion with `--record` on, then run `talaria replay` over that recording and diff the two transcripts; (3) record the compatibility check's real output — how many of the five read-only probes come back `present`, and whether `spawn_tree.list` refuses the fixture. Then update the evidence table and the verdict in `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`.

**Recording this is now possible from the client, which it was not when this item was written.** `LiveSource` accepted a recorder and `build_live_app` never passed one, and `talaria record` draws no interface — so step (2)'s "with the recorder on" named something the shipped client could not do. U10's closeout added `talaria --record [PATH]`. Use it; the frame log's header carries the credential-stripped endpoint, and every frame passes the U2 redaction boundary before it is written.

**Step (2) — R3 — is done, 2026-08-04.** A Hermes dashboard on loopback, `talaria --record`, a prompt submitted from the composer, the reply streamed to completion. Replaying the same recording rebuilds a transcript byte-identical to the live screen: three rows, three lines, `interface_shows_everything` true. Corpus cited by digest and frame count rather than path (R29): `talaria-live-v1-32f-5f477fa24fa5`, sha256 `5f477fa24fa50b391d73eee6f455190000281980a8db33c17f4130208d997549`, frame-log v1 recorded 2026-08-04T19:37:35.709Z.

The comparison could not have passed before that run, and the reason was a defect it exposed: a replay could not reconstruct the operator's own line at all, because it is written locally at submit time and `ingest` discarded every outbound frame. Fixed — see the DECISIONS entry "A replay reads one outbound frame". The first attempt at this comparison *did* report zero differences, from a scrolled screen capture whose compared rows happened to exclude that very line; use a turn short enough to fit entirely in view.

**What is still open in this item.** Step (1), the three startup paths — bare has been exercised many times and lands in a session, but neither `--resume` nor `--session <id>` has been run against Hermes, so KTD7's precedence chain is unverified live. Step (3), the compatibility check's real output, is unrecorded. F1 and F7 have not been demonstrated.

**The remaining steps are now a written checklist, and that is all that changed on 2026-08-06.** Unit U6 of the model-picker plan turned them into `docs/plans/2026-08-06-u6-row19-operator-checklist.md` — six steps, each naming what to run, what to capture verbatim, and what to hand back (digest and frame count per recording, no operator profile inventory). It is marked `status: ready-for-operator` and none of it has been run: the recording corpus has gained nothing since 2026-08-04, its newest frame log's header reading `2026-08-04T19:43:17.075Z`. The verdict document *has* now been updated — re-graded 2026-08-06 — and row 19 stays **unmet** on exactly this. Do not read the checklist's existence as progress against this item; the item closes when somebody runs it.

### ~~The install job and the CI matrix are declared but have never run~~ — CLOSED 2026-08-16, long since stale

**Closed.** The branch was pushed with v0.1.0 and `validate.yml` has run on every pull request since — the 2026-08-11 branch-protection entry above attests the install jobs run (they merely cannot block), and every release since v0.1.0 built its wheel in continuous integration. The verdict document's matrix rows were corrected as this entry asked: `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md:588` records the rows moving off *declared, not observed* once the push happened. Verified 2026-08-16.

**Author.** v0.1 milestone-2, unit U10
**Priority.** P1
**Effort.** Small — it is a push.
**Worth it when.** As soon as this branch is pushed.

**Context.** `.github/workflows/validate.yml` gained an `install` job that installs the wheel with `uv tool install` into a fresh prefix and runs the console script under `env -i`. AE10 asks that this pass in CI. It has not, because this work is uncommitted. The same is true of the two existing Python legs for this change. The equivalent was run by hand on macOS and passed; the verdict document's platform matrix marks the CI rows *declared, not observed* and must be corrected once they have run.

**Do.** Push, read the run, and correct the matrix rows in the verdict document from the actual result — including a failure, if that is what happens.

### The platform matrix is one operating system and two terminal hosts

**Author.** v0.1 milestone-2, unit U10
**Priority.** P2
**Effort.** Medium.
**Worth it when.** Before claiming support for anything the matrix does not list.

**Context.** R39 says the recorded matrix lists exactly what was exercised. What was exercised is macOS arm64, Python 3.12.11 and 3.13, a bare pseudo-terminal at 100×30, a tmux 3.7b pane at 100×30, and a clean `uv tool install`. Not exercised, and therefore claimed nowhere: Linux as a daily driver (the CI leg is informational and runs the test suite, not the interface), Windows, any real terminal emulator, screen, mosh, any remote session, any narrow terminal under a real emulator, and Python 3.14.

**Do.** Add one real terminal emulator and one Linux desktop run to the matrix before the next verdict revision, and re-run the pseudo-terminal teardown tests at a narrow width (80×24 and below) — U8's height-zero failure family lived exactly there.

**Updated 2026-08-16.** The real-terminal-emulator half is now exercised: the operator has driven v0.2 and v0.3 by hand in a real terminal on macOS. The Linux half is deliberately parked on the operator's ruling of 2026-08-16 — "not concerned about Linux right now" — so no Linux run is scheduled, and the claims rule stands unchanged: nothing is claimed for platforms the matrix does not list.

### Audit the egress surfaces as a set, rather than the redactor's call sites

**Author.** v0.1 milestone-2, unit U7 — proposed by the milestone-1 review agent
**Priority.** P0
**Effort.** Medium
**Worth it when.** Before U8 lands, because U8's four blocking-prompt bridges and U10's live acceptance run each add several more surfaces of exactly these kinds.
**Context.** The same defect has now appeared three times in three different places, and each time it was found by someone tripping over it rather than by looking. Milestone-1's P0 was two call sites and disk. U7's `describe_dial_error` was *zero* call sites and the screen — the operator's session token rendered into a composer notice by a wrong-scheme endpoint typo, and re-leaked on every reconnect. The common root is that `redact_url` is a function invoked where somebody remembered, not a boundary that everything crosses.

Enumerating the redactor's call sites is the wrong direction: it can only find places already thought about. Enumerate the places data *leaves* the process and ask which of them crosses the boundary. From the incidents so far, the kinds worth sweeping are:

- exception stringification (`str(exc)` anywhere a dial target, URL, or credential can be in scope — `talaria/transport/source.py:385` and `:541` are the same shape as the fixed leak, safe today only because those exceptions happen not to carry the endpoint)
- `repr()` and `__str__` on transport objects and anything reachable from them
- retained failure state (`LiveSource.last_failure` and anything like it)
- operator-visible text that formats a dial target — notices, status lines, diagnostics
- log lines and the status payload

**Do.** Build the list of surfaces first and write it down, then check each one, then decide which need the boundary applied. Record the negatives too — `credentials.py`'s `tomllib` parse error was checked across four malformed files and is genuinely safe, and that should not need re-checking. The rule to test against is both halves: the credential is absent **and** the diagnostic survives. A redaction that eats the message it was protecting has already happened twice in this module.

**What U8 added, and what it swept.** The four blocking-prompt bridges are the first path on which Talaria sends an operator-typed *credential*, so this item's "before U8 lands" trigger has now fired. U8 closed the surfaces it created rather than the standing list: the respond value is proven absent from the recorded frame log's raw bytes (over a real socket, not by unit test), from the status document, from every transcript entry, from the composer notice, from `LiveSource.last_failure`, and from the rendered screen via `App.export_screenshot`; the terminal-read failure text goes through `scrub_urls` and is asserted on both halves. It also found a case of the "redaction ate the message" failure this item warns about twice — the failure line was clipped at `SYSTEM_LINE_CLIP` because the constant repeated the exception's own first clause. **Still unaudited:** the list as a *set*, including `repr()` on transport objects generally and log lines.

**Already covered by U7, so start after these.** `describe_dial_error` now scrubs through `scrub_urls` plus a literal pass over the credential in use, and `LiveSource` scrubs `last_failure` on the disconnect and close-error paths (`source.py:385`, `:541`), so the three exception-stringification sites known at the time are closed and pinned by tests that assert both halves. `AttachTarget.safe_url` is the choke point for the endpoint and is credential-free twice over — stripped at construction, then routed through `redact_url` so userinfo is withheld too. The outcome object is swept by `test_the_attach_outcome_carries_no_credential_anywhere`, with one documented exclusion: the live `GatewayConnection`, because `websockets` retains `connection.request.path` including the query and no client library can avoid that. **What remains unaudited** is the rest of the list as a *set* — `repr()` on transport objects generally, log lines, the status payload, and every new surface U8's bridges and U10's acceptance run introduce.

## P1

### `pageup` and `home` never scroll — they only stop the view following

**Author.** v0.3 unit B3 document review, 2026-08-11
**Priority.** P1
**Effort.** Small
**Worth it when.** With spine A's mis-aimed-input work, or sooner. The operator's stated way of reading back while a turn streams is to scroll up; the key advertised for that does not move the view, which makes reading back during a live turn a mouse-only operation — the same shape as the approval card that could only be answered with the mouse.

**Evidence.** `talaria/ui/app.py:4314-4315` handles `pageup` and `home` by calling `self.transcript.hold_anchor()`, and that method sets `self.follow = False` and nothing else (`talaria/ui/transcript.py:1714-1715`). Its sibling in the same handler is the contrast that proves the gap: `end` reaches `follow_bottom()`, which calls `scroll_end` (`transcript.py:1717-1719`). The transcript pane declares no page-key bindings of its own, and the handler's own comment at `app.py:4311-4313` states why the keys arrive at the application at all — the composer holds focus, not the transcript — so the framework never scrolls the pane. The only test that presses `pageup`, `test_end_and_pageup_toggle_the_anchor` (`tests/ui/test_transcript_bounds.py:159-169`), asserts only that the flag flipped, so the missing scroll is invisible to the suite.

**Why this was not folded into unit B3.** B3 makes silent keypresses distinguishable from effective ones, and by that standard `pageup` qualifies — pressed at the bottom of a paused replay it does nothing observable, permanently, since no new content will arrive to reveal that the anchor is held. It is excluded deliberately: adding a notice reading "scroll position held" would confirm a keypress whose advertised effect never happens, dressing a broken control as a working one. The repair is to make the key scroll, not to announce that it did not.

**Worth checking at the same time.** Whether `home` should scroll to the top of the transcript rather than merely release the anchor, and whether `F5` is alive at all while scrolled up — an open item on the operator's own checklist that touches the same follow state.

### ~~A replay-gate test counts a legitimately-lagging pane as content loss when the runner is loaded~~ — CLOSED 2026-08-12, on the second attempt

**Author.** v0.3 unit B5 implementation, 2026-08-11
**Closed 2026-08-12, reopened the same day, closed again the same day.** The first closure was wrong
and is recorded below rather than erased, because the way it was wrong is the reusable part. The
second closure carries two separate proofs; see *How this was actually closed* at the end.
**Priority.** P1
**Effort.** Small to medium
**Worth it when.** Now. It has failed three times, the third on a documentation-only pull request, and
the branch it was declared fixed on changed nothing about it. It is worse than an ordinary flaky test
because `content_loss_failures == 0` is also the replay gate's own pass criterion
(`talaria/replay/gate.py:1677`, `:1684`, `:1702`, `:1752`) — the instrument the project uses to decide
whether a release is honest can be failed by runner load.

**Evidence.** `tests/replay/test_gate.py:1381`, `test_reachability_coverage_rides_the_poll_not_the_checkpoint_schedule`, first arm: `assert measurement.content_loss_failures == 0`. Failed `python-check-linux (3.13)` on pull request 64 with `assert 1 == 0`. The same test passes on `main` and passes three runs out of three on the pull request's own branch, at 4.6 seconds each. The change under review cannot reach it: the gate constructs `TalariaApp` at `talaria/replay/gate.py:1217`, `:1433` and `:1454` and passes no startup selection in any of the three, so `begin_live_startup` short-circuits (`talaria/ui/app.py:3053`) and neither `open_session` nor `_land_session` ever runs under the gate — a `grep` for either name across `gate.py` returns nothing.

**Mechanism.** The first arm asserts that a *correct* pane records zero content-loss failures, and it separates correct from broken purely by wall-clock: `CATCHUP_GRACE_SECONDS` is monkeypatched to 0.2, chosen — in the test's own comment — as "comfortably above the pane's 50 ms coalescing flush so a correct, legitimately-lagging pane cannot fail the first arm (the run-9 lesson)". Four times the flush interval is comfortable on an unloaded machine. On a shared runner it is not, and a pane that is behind for ordinary scheduling reasons is counted as having lost content. The test already carries a scar from this exact failure; the margin was widened rather than the dependence removed.

**Repair.** Restored `assert measurement.content_loss_failures == 0`. The previous repair at `097101b` replaced it with `assert content_is_complete(state, transcript_view(state))`; that expression does catch aggregate projection loss — `content_is_complete` renders each entry in isolation at `gate.py:1041` and checks aggregate order, so a dropped or reordered aggregate is caught even though each isolated render is correct — but it drops the two pane-involving branches at `gate.py:1375` (`apply_in_flight` stuck marker, which would have silently excused every mid-stream ownership sample) and `gate.py:1382` (`interface_shows_everything`, the only genuine pane-versus-projection check in the settled chain). The test is named `test_reachability_coverage_rides_the_poll_not_the_checkpoint_schedule` and its correctness arm existed to prove the pane kept up; keeping only the domain branch does not replace it. The poll-versus-checkpoint property is still asserted via `content_loss_checkpoints == 1`, `ownership_checkpoints == 0`, `reachability_checkpoints >= 1` (checkpoint schedule disabled via `RSS_SAMPLE_EVERY = 10**9`). The second arm — stale snapshot counted once per consumed fingerprint via monkeypatched `_snapshot_covers` — is unchanged.

**Why that repair did not fix this, and why it looked like it had.** It restored the assertion the
previous attempt had weakened, which was right, and then claimed the restored assertion was no longer
wall-clock-dependent, which was wrong. The claimed reason was that `gate.py:1367-1371` forces a flush
before the settled check, so the settle is deterministic. The settle is deterministic; the assertion
is not, because `content_loss_failures` does not count only settled failures. It also counts the
mid-stream reachability check at `gate.py:1268`, which is gated on `now - prev_progress_at >=
CATCHUP_GRACE_SECONDS` at `:1261` — wall-clock, unchanged, and the original mechanism. The commit
message read `make two timing-dependent tests deterministic`; the diff to `tests/replay/test_gate.py`
was a removed comment and an added comment, with no behavioural change to this test at all. The other
test in that pull request, `tests/ui/test_transcript_bounds.py`, was genuinely fixed.

**Third occurrence, and the measurement that settles which branch fires.** Failed
`python-check-linux (3.13)` on pull request 70 with `assert 1 == 0` at `content_loss_failures=1`,
`reachability_checkpoints=9`, on a commit whose entire diff is two files under `docs/`. Instrumenting
all five `failures += 1` sites in `gate.py` and sweeping this arm's grace on an unloaded machine:
every failure comes from `midstream:reachability` (`:1268`) and none from any settled branch, at 0.02 s
46 failures, at 0.05 s and above none. So the 0.2 s in the test is roughly a 4x margin over the local
edge, and a loaded runner still crossed it — one failure in nine checks.

**The repair is to stop deciding correctness by elapsed time.** Either drive the sampler's clock deterministically so the grace window is measured in controlled ticks rather than wall-clock seconds, or make the first arm assert against an observed pane state rather than against a deadline. Widening the grace again buys another few months and re-teaches the same lesson.

**A concrete candidate, not yet decided.** Age the progress fingerprint in *applied frames* rather
than in seconds: if the pane has had N further applies since the fingerprint was taken and still does
not cover it, the pane is genuinely behind, and that is a property of the stream rather than of the
runner. This changes what the release gate counts as a failure, so it wants a decision rather than a
patch.

**That candidate was wrong, and the reason generalizes.** Applied frames are the wrong unit because
the pane's catch-up is governed by a *time-based* coalescing timer, so at replay speed many frames
elapse inside one flush interval and a frame-indexed grace expires before the pane has had a single
chance to render. The stress corpus replays roughly 460 frames per second against a 50-millisecond
flush, so about 23 frames pass between flushes; any frame-based grace below that fails a correct pane
by a different route. The constant's own comment in `gate.py` had already recorded this objection —
found independently, then discovered to agree.

**How this was actually closed.** The grace is now counted in `app.render_ticks`, the coalescing
flushes that *actually re-rendered* the interface, with the wall clock retained only as a ceiling for
a pane that has stopped rendering altogether. `_render_tick` returns early when nothing is dirty and
otherwise projects current state, and ingestion sets the dirty flag, so one completed re-render after
a fingerprint is taken already guarantees the pane's snapshot covers it. The threshold is twenty, a
wide margin over a structural guarantee rather than a bet on a clock. Recorded as a decision in
`DECISIONS.md`, dated 2026-08-12, because it changes what four gate pass criteria count as a failure.

**The two proofs, kept separate on purpose.** This entry was closed once already on a repair that
proved neither, so both are stated with their numbers.

- *Teeth.* With `TranscriptPane.apply` mutated to stop recording what it applied — a genuinely lagging
  pane — the repaired check reports eight failures and the assertion goes red. The guard is live.
- *Stability.* Both gates run at the same load in the same session, 96 processes on 24 cores. The
  repaired gate: 30 runs, zero failures. The pre-repair gate: 35 runs, **three failures**, every one
  of them `AssertionError: assert 1 == 0` on `content_loss_failures` at `tests/replay/test_gate.py`,
  with `reachability_checkpoints` at 30 and 32 — the same assertion, on the same counter, as all three
  CI occurrences. This is the first time the failure has been reproduced off a CI runner.

**A measurement that lied first, and the shape it lied in.** The first before/after run reported both
arms clean. Its pre-repair arm was installed with `git stash push`, the shell's command proxy rewrote
that into a form git rejected, the script did not check the exit status, and fifteen more runs of the
*repaired* code were printed under the label `OLD`. Its `No stash entries found` line was reporting the
absence of the thing the run depended on, and read as reassurance. The rerun installs the old arm with
`git show HEAD:<path>` and refuses to run the arm at all unless `git diff --name-only` comes back
empty. Same family as the rest of this file: a failed step whose failure is shaped like success.

### A partial reply was not committed when the credential failed mid-stream — seen once, not reproduced

**Author.** v0.3 orchestration, 2026-08-12
**Priority.** P1 to *diagnose*, because the guarantee it touches is data preservation rather than
tidiness. Not yet P1 to fix, because it is not yet established that there is anything to fix.
**Effort.** Unknown — the first task is reproduction, not repair.
**Worth it when.** On the second occurrence, immediately, and this entry exists so the second
occurrence is recognised rather than re-diagnosed from nothing. Also worth a bounded attempt before
the release ships, because the failing assertion is R6: a credential that stops working mid-stream
must not throw away the text the operator already received.

**Evidence.** `tests/transport/test_source.py::test_auth_failed_mid_stream_commits_the_partial_reply`
failed `python-check (3.13)` — the macOS job, runner path `/Users/runner/...` — on pull request 71 at
commit `513da81`, with `AssertionError: the typed auth_failed cause did not reach the domain commit`
and `assert [] == ['partial reply']`. The transcript held no assistant entry at all where it should
have held the partial reply. That branch changes two files under `docs/engineering-journal/` and one
comment in `tests/replay/test_gate.py`, so it cannot reach this code. The test passes on `main`
locally, and 25 runs of the test plus 8 runs of its whole file, all under deliberate CPU load, did not
reproduce it.

**Mechanism — not known, and one plausible theory already ruled out.** The obvious shape is the one
the other two flakes in this file have: the test waits on a proxy signal rather than on the thing it
asserts. It settles on `app.state.connection == "auth_failed"` and then asserts the transcript
commit, and the commit is gated on the typed *cause*, not on the status — `set_connection` in
`talaria/domain/state.py` runs `_commit_partial_streams` only inside `if cause is not None`. So a
cause-less call that set the status first would satisfy the predicate before the commit existed.

That theory does not survive checking. The transport reaches `auth_failed` only through calls that
carry the cause (`talaria/transport/source.py:544`, `:852`), and the comment at `:626-628` states it
outright: the domain already committed on whichever typed cause put the source in `auth_failed` to
begin with. Whatever this is, it is not that. Do not re-run that reasoning; start somewhere else.

**Where to start.** Reproduce first — this needs a real failing instance before anything is changed.
A GitHub-hosted macOS runner is more contended than a developer machine, so the next attempt should
raise concurrency rather than raw load. If it reproduces, the question to answer first is whether
`streaming_text` was still populated at the moment the causal `set_connection` ran, which separates a
test race from a genuine loss of the operator's text. Resist repairing the test until that is known:
the assertion is currently the only thing standing between a real R6 regression and silence.

### ~~A replay-pause test asserts how fast the runner is, and fails on Linux when it is fast enough~~ — CLOSED 2026-08-12

**Author.** v0.3 orchestration, 2026-08-11
**Closed.** 2026-08-12 — fixed on branch `fix/timing-dependent-tests` alongside the sibling 200 ms-grace flake; see below.
**Priority.** P1

**Evidence.** `tests/ui/test_transcript_bounds.py:313`, `test_the_projection_and_the_domain_transcript_agree_at_every_pause_point`. Failed `python-check-linux (3.12)` on pull request 61 with `assert 2 >= 3`, on a branch whose entire diff is three files under `docs/`. `python-check-linux (3.13)` passed on the identical commit, so the failure is not deterministic.

**Second occurrence — the trigger that fired the Worth-it-when.** Failed `python-check-linux (3.12)` on pull request 65 at commit `750a34c` with the same `assert 2 >= 3`, while `python-check-linux (3.13)` passed on the identical commit. The branch's only change was twelve added lines in `tests/ui/test_agent_rows.py` (the B3 collapsed-flip pin). The first occurrence was pull request 61; this second failure is the one that queued this repair.

**Mechanism.** The test resumes and pauses a replay up to twelve times, breaking early once `app.replay_complete` is set, then asserts `checked >= 3`. That final assertion is not a property of the code under test — it is a claim that the replay takes at least three resume-pause cycles to drain, which depends on how the runner schedules `pilot.pause()` against the replay source. The guarantee the test exists to prove, `content_is_complete` at every pause point, held on both samples it managed to take. Only the did-I-sample-enough guard failed.

**Repair.** The test now controls its sample count: it ingests the stress corpus in deterministic batches of `len(records) // 6` via `app.ingest(record)` and `await app.render_snapshot()`, asserting `content_is_complete` after each. `checked` is `ceil(len(records)/batch_size)` — for the 321-frame stress fixture that is a 53-frame batch and **seven** checks, not six, because integer division floors and the remainder takes a short final batch — impossible to fail due to runner speed. Rejected: feeding more frames (probabilistic mitigation, still flaky, costs wall-clock) and adding a stepping API to `ReplayControls` (`talaria/replay/controls.py:87`) — adds production surface purely for a test when the existing `ingest` path already provides a deterministic drive with no new API. The sibling ingest-driven tests directly above in the same file (`app.ingest(record)` loop asserting `seen == len(frames)`) are the model; this test's unique value — invariant holds at every mid-stream sample — survives because each batch is a genuine mid-stream domain state.

**The fix is not simply lowering the threshold.** Three samples is a real requirement — checking a repeated invariant once is the thing this test was written to stop. The sound repair makes the sample count a property the test controls rather than one it observes: feed enough frames that the replay cannot drain in two cycles, or drive the source deterministically so each resume yields a bounded number of frames. Lowering `3` to `2` keeps the flake and weakens the check at the same time.

### ~~Make the macOS checks required status checks on `main`~~ — CLOSED 2026-08-03

**Author.** v0.1 scaffold code review, 2026-08-02
**Priority.** P1
**Effort.** Small
**Worth it when.** Before the next unattended run merges anything — this is the only remaining gap between a red check and `main`.
**Context.** `.github/workflows/validate.yml` runs the `python-check` job on macOS arm64 with no `continue-on-error`, so a failure fails the workflow run. That is not the same as blocking a merge, and nothing in a workflow file can be. Verified 2026-08-02: `gh api repos/infiquetra/talaria/branches/main/protection` returns HTTP 404 "Branch not protected" and `gh api repos/infiquetra/talaria/rulesets` returns `[]` — `main` has neither branch protection nor a ruleset, so a pull request with a red check can still be merged.

An earlier version of the workflow carried a `required: true` matrix key, which reads as though it configured this; it only negated `continue-on-error`. That key has been removed, and job names are now free of incidental matrix values so the check names (`python-check (3.12)`, `python-check (3.13)`) stay stable if protection is configured against them.

**Left to the operator deliberately.** Repository governance is not something an unattended run should change on the operator's behalf, and requiring a check name that does not match exactly would deadlock every merge — including the ones the run was authorized to make. Until it is configured, the gate is enforced behaviorally: this run does not merge without observing the required legs green.

**Closed 2026-08-03 on explicit operator instruction**, immediately after PR #11 merged. Classic branch protection on `main` requires exactly `python-check (3.12)` and `python-check (3.13)`, with `enforce_admins: true`, force pushes and deletions denied.

Three choices inside "set the protection" that the instruction did not spell out, recorded so they can be reversed knowingly:

- **`enforce_admins: true`.** The gap this item exists to close is an unattended run merging a red check, and such a run holds the operator's own admin rights. With admins exempt the protection would not constrain the only actor it was queued for. The cost is that the operator has no bypass either; lifting it is one API call.
- **Only the two macOS legs are required.** `python-check-linux` is informational by ADR-0005, and `check` covers the TypeScript bootstrap under `src/`, which is superseded and slated for removal — requiring a check that later stops being reported blocks every merge permanently, which is the deadlock this entry already warned about.
- **`strict: false`.** A branch does not have to be up to date with `main` before merging. This closes the stated gap (red check cannot merge) without forcing a rebase on every merge. It leaves the semantic-merge-conflict hole open, which is a separate concern from this item.

**Verified, not assumed.** `git push origin main` with an empty commit was rejected: `remote: - 2 of 2 required status checks are expected` / `! [remote rejected] main -> main (protected branch hook declined)`. Reading the configuration back would not have proved the hook fires.

### ~~`test_mounted_widgets_stay_under_the_cap` fails intermittently under load~~ — CLOSED 2026-08-03

**Author.** v0.1 milestone-2, unit U7 — found while running the project check, and fixed rather than queued
**Priority.** P1
**Context.** `tests/ui/test_transcript_bounds.py::test_mounted_widgets_stay_under_the_cap_while_content_stays_reachable` failed intermittently on `pane.rendered_lines == view.lines[pane.condensed_count:]` — **one line of skew**, `'line 38.3' != 'line 38.4'` at index 30. The file already documented the symptom: the `_drain` helper's docstring says the accounting assertions "fail roughly one run in three, and only under whole-suite load", and `_drain` forces a flush specifically to suppress it. Measured with U7 present: 2 failures in 12 paired runs, and 2 failures in 9 whole-suite runs.

**It was not a test bug.** `TalariaApp.render_snapshot` has two kinds of caller — the coalescing timer, which Textual runs on the message pump and never re-enters, and forced flushes (`drain`, and the gate's checkpoints) which run on the *calling* task. Those two were never serialized against each other, and `TranscriptPane.apply` is a read-modify-write over its own window bookkeeping spanning several awaits. Two passes over different projections interleave and leave the pane holding a window the projection does not have. The U5 gate calls `drain` at every checkpoint, so this was live in the gate too.

**Closed by.** An `asyncio.Lock` around `render_snapshot`, uncontended in the ordinary path because Textual's timer never re-enters its own callback. Pinned by `tests/ui/test_transcript_bounds.py::test_two_renders_can_never_reconcile_the_pane_at_the_same_time`, which measures overlap depth: 2 against the unfixed code, 1 with the lock. Rate after the fix: 12 of 12 paired runs clean.

**Author.** v0.1 scaffold code review, 2026-08-02 (rated P2 advisory by the review; carried at P1 here because the trigger is the next milestone)
**Priority.** P1
**Effort.** Small
**Worth it when.** Before U6's status runner executes `status.command` — that is what turns this from a precedence question into an execution path.
**Context.** KTD15 ranks a repo-local `./.talaria/config.toml` above the operator's global `~/.talaria/config.toml`. Nothing in the scaffold executes anything, so there is no vulnerability today. But KTD5 specifies `status.command` is executed on an interval, and the precedence chain is being locked in now: an operator who clones an untrusted repository, `cd`s into it, and runs `talaria` would execute a command supplied by that repository's contents.

The review named two viable resolutions: require a repo-local config file to be explicitly trusted before it is honored, or exclude command-valued keys from the repo-local level while leaving the rest of KTD15's order intact. Either is a change to a key technical decision the plan settled, which is why it is recorded for decision rather than made unilaterally mid-run.
**Refs.** [v0.1 plan KTD15 and KTD5](../plans/2026-08-02-talaria-v0-1-prototype-plan.md)

### Let the operator decline a blocking prompt without waiting for it to expire

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)
**Priority.** P1
**Effort.** Small
**Worth it when.** Someone runs Talaria against a real session and hits a sudo prompt they do not want to answer.
**Context.** U8's controls can answer a prompt but cannot decline one. The shipping terminal UI can: `answerSudo('')` and `answerSecret('')` clear the overlay locally when the operator submits nothing, and `useInputHandlers.ts:119` / `:128` send `{password: ''}` / `{value: ''}` on escape, which releases the gateway's blocking wait immediately instead of making the tool sit out its full 120-second (sudo) or configured (secret) timeout. Talaria's operator has to let it expire.

The reason it is not in U8 is that "an empty value" and "no answer" are the same thing on this wire, and the deliberate choice — decline — deserves a distinct affordance rather than an accidental one, which is a keybinding decision rather than a transport one. Note the interaction with U8's own decision that a cleared prompt is not re-offered: a declined prompt must clear like an answered one, not restore.
**Refs.** [DECISIONS.md](DECISIONS.md) "A prompt is cleared before its answer is sent", `ui-tui/src/app/useMainApp.ts:936-975` at Hermes `7f4d15515`

### Build the stable screen model

**Author.** Project bootstrap
**Priority.** P1
**Effort.** Medium
**Worth it when.** Transport fixtures exist and event/state transitions can be tested independently of a terminal.
**Context.** Stable rendering and reduced flicker depend on state-driven rendering rather than direct callback-driven writes.

### ~~Read Hermes's turn controller and complete the reconciliation-rule catalogue~~ — CLOSED 2026-08-02

**Author.** ADR-0003
**Priority.** P1
**Effort.** Small
**Worth it when.** Before the normalization layer is written, since a rule discovered afterwards is a defect found in production.
**Context.** The 2026-08-02 read covered `ui-tui/src/app/createGatewayEventHandler.ts` and found its reusable content is a set of short rules rather than portable machinery. That handler delegates to `ui-tui/src/app/turnController.ts` (1,092 lines) at more than twenty call sites, and only its call surface has been read. The rule catalogue ADR-0003 depends on is incomplete until the controller is read at a pinned revision.
**Closed by.** Unit U3 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), discharging R37. `turnController.ts` is now read in full at `7f4d15515` and the catalogue is [Hermes reconciliation-rule catalogue](../analysis/2026-08-02-hermes-reconciliation-rules.md): 38 rules, each with an explicit verdict (re-encode / re-encode with a change / drop) and a named test under `tests/domain/`. `tests/domain/test_reconciliation.py::test_every_catalogued_rule_names_a_test_that_exists` parses the document and fails if any rule names a test that does not exist, so the catalogue cannot rot quietly — which is the failure mode ADR-0003 names.

**What the controller read settled.** ADR-0003 left open whether the controller's engine would recover part of the reuse argument. It does not: its density is streaming *presentation* — segment assembly, tool-shelf coalescing, reasoning pulse timers, notice TTL machinery, markdown and diff de-duplication — none of which transfers to a plain-text client. Nine of the thirty-eight rules come only from the controller, and each is one to four lines. The catalogue recovers; the reuse argument does not.

**Two gaps found in Hermes rather than rules taken from it**, both recorded as catalogue entries RR-27 and RR-28: the gateway emits `.expire` for all four blocking bridges but the shipping terminal UI handles only two of them, and `approval.request` carries no `request_id` at all, so R8's keyed registry needs a synthesized key for approvals.
**Refs.** [ADR-0003](../../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md), [Hermes reconciliation-rule catalogue](../analysis/2026-08-02-hermes-reconciliation-rules.md), [LEARNINGS](LEARNINGS.md)

### Add the sub-agent monitor

**Author.** Project bootstrap
**Priority.** P1
**Effort.** Medium
**Worth it when.** The gateway advertises enough delegation or spawn-tree state to make the pane meaningful.
**Context.** Sub-agent visibility is a primary UX goal.

### Attach to a remote (gated) gateway

**Author.** v0.1 prototype plan doc review, 2026-08-02
**Priority.** P1
**Effort.** Medium
**Worth it when.** The operator wants Talaria pointed at a Hermes running on another host. v0.1 is loopback-only by decision, not by limitation.
**Context.** Whether the gateway's auth gate is active is decided entirely by its bind host: `should_require_auth` (`hermes_cli/web_server.py:437-460` at `7f4d15515`) returns true for anything that is not `localhost`, `127.0.0.1`, or `::1`, and RFC1918 addresses count as public. The legacy `--insecure` escape hatch is accepted but **ignored** since the June 2026 `hermes-0day` campaign, so a remote bind cannot be un-gated — the gated path must be implemented.

It is fully reachable for a client that dials a gateway it did not launch, which the v0.1 plan initially doubted. The complete RFC 8252 native-app flow, verified at the pin: `GET /auth/native/authorize` (`hermes_cli/dashboard_auth/routes.py:289`) runs PKCE against a loopback redirect; `POST /auth/native/token` (`:841`) exchanges the code for `{access_token, refresh_token, token_type: "Bearer"}` explicitly intended for OS-keychain storage; `POST /api/auth/ws-ticket` (`:799`) turns that session into `{ticket, ttl_seconds: 30}`; the ticket goes on the `/api/ws` upgrade URL as `?ticket=`; `POST /auth/native/refresh` (`:894`) rotates. Tickets are single-use with a 30-second TTL (`hermes_cli/dashboard_auth/ws_tickets.py:42`).

**The transport seam for this now exists, in code — updated 2026-08-03 by unit U7.** `talaria/transport/credentials.py` defines `CredentialProvider.acquire()`, and `LiveSource._dial` calls it on **every** dial including every reconnect; `tests/transport/test_reconnect.py::test_the_provider_is_invoked_once_per_dial_and_again_on_reconnect` fails if a credential is cached across a reconnect, and `::test_a_credential_rotated_between_attach_and_reconnect_is_picked_up` fails if the re-read does not reach the wire. `redact_url` already denies `ticket` and `internal` by name (KTD6's Python-only superset) and `AttachTarget` strips all three credential forms from an endpoint, so the recording and hygiene boundaries are ready too.

So this work is a new `GatedTicketProvider` returning `("ticket", <single-use value>)`, plus keychain storage and the PKCE loopback listener. Nothing in `LiveSource`, `RpcCorrelator`, or the UI should need to change — and if it does, that is the signal the seam was drawn in the wrong place, which is worth reporting rather than working around.
**Refs.** [v0.1 plan KTD11](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)

### ~~A Talaria-authored delivery note is longer than the transcript's own line clip~~ — CLOSED 2026-08-04

**Closed by.** Splitting the one 120-character clip into two bounds by destination — `DETAIL_LINE_CLIP` (120, for a sub-agent row's single screen line) and `TRANSCRIPT_LINE_CLIP` (2000, for the scrolling transcript). The 146-character deny-all line and the 143-character single-answer line now land whole, and `tests/ui/test_prompts.py` asserts the deny-all sentence by equality rather than as a prefix.

**This entry asked the right question and the answer turned out to be simpler than either option it weighed.** It framed the choice as "which half survives the clip" and proposed moving the note to its own transcript entry. Neither was needed: the clip itself was misapplied. It had been re-encoded from Hermes's `gateway.stderr` bound, which exists so a runaway line cannot own a **one-row activity region** — a constraint Talaria's scrolling transcript does not have. The closing note the entry itself flagged as "worth checking at the same time" — *whether `record_local_note` should clip at all* — is what the fix acted on.

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), third adversarial round
**Priority.** P1 — raised from P2 in the fourth round.
**Effort.** Small
**Worth it when.** Before a live run leans on the transcript as the record of what happened to an unconfirmed answer — or sooner, if an operator reports being told to "send it again" with the sentence cut off.

**Context.** `record_local_note` bounds an entry at `SYSTEM_LINE_CLIP` (120 characters) and marks the cut with an ellipsis, which is correct for text the gateway wrote. `DELIVERY_NOTES["not_sent"]` is **121 characters on its own**, so every line that prefixes it loses its tail — `"…send it again when the connection is bac…"`. Measured: `"2 approvals not denied — " + DELIVERY_NOTES["not_sent"]` is 146 characters, and the single-answer path's `"sudo not answered — …"` is 143.

**Correction to this entry's own wording, fourth round.** It described the defect as "pre-existing and not introduced by the deny-all fix". That is misleading in a public journal: U8 is *entirely uncommitted*, so nothing in it is pre-existing relative to `main`, and "pre-existing" reads as "inherited from a shipped release". What is true is narrower — it predates this **round**, not this unit. The deny-all path made it visible by putting the same note on a second path; both paths arrived in U8.

The clip is not the wrong behaviour — a bound on transcript entries is deliberate — but *which half survives* it should be a decision, not an accident. `record_submission` already solves this for a submitted message by writing the note as its own transcript entry rather than as a suffix, and that is the obvious shape here too.

**Not fixed now** because the change touches the wording shared with the submit path and the combined-line assertions in four existing tests, which is a wider blast radius than the defects each round was scoped to. The deny-all line was arranged so the operative clause ("delivery unconfirmed", "not sent") lands well inside the bound and only the explanatory tail is cut.

**Partly mitigated, fourth round, on a different surface.** The composer notice bar is one row with no wrap, and a long note used to stop mid-clause with nothing marking the cut. It now carries `text-wrap: nowrap` plus `text-overflow: ellipsis`, so the *screen* marks its own truncation the way the transcript already did. That is the marker, not the length — the notes are still longer than either surface.

**Worth checking at the same time.** Whether `record_local_note` should clip at all for a Talaria-authored constant, or whether the clip belongs only on the paths that embed gateway text (`outcome.notice` does).

### A card whose control is on screen and working is titled "answer below — scroll"

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P1
**Effort.** Small — the mechanism is identified precisely.
**Worth it when.** Before the reachability marker is presented to anyone as a reliable cue. It is currently right in the dangerous direction and wrong in the safe one.

**Context.** The `answer below — scroll` border title is applied, after an ordinary terminal resize with no operator scroll, to a card whose buttons are drawn on screen and answer a click. Reproduced against the real app: an approval plus a `secret.request` plus a `sudo.request` queued, mounted at 120x40, then `pilot.resize_terminal(60, 20)`. From one `export_screenshot()` the card's title reads `answer below — scroll` while `"once"` is present on the same screen, and `pilot.click("#choice-0")` sends `("approval.respond", {"session_id": "s1", "choice": "once"})`. No card on that screen says `waiting for you`, so the operator's only cue is the false one — and the action it names, scrolling down, moves the working buttons off screen.

It is stale rather than transient: twelve consecutive settle cycles leave the title unchanged, and one direct call to `mark_unreachable_controls()` against the final geometry flips it back.

**Mechanism.** `reveal_actions` (`talaria/ui/prompts.py`) scrolls with `scroll_to_widget(...)` and then schedules the marking with a **single** `call_after_refresh(self.mark_unreachable_controls)`. One deferral is not enough — the marking reads the region's geometry before the scroll has committed, records the pre-scroll answer, and nothing recomputes afterwards. The comment above that line says the deferral exists precisely so the check runs against the post-scroll arrangement; measured, it does not.

**Severity, stated fairly.** This is the *inverse* of the defect the fix was written for, and the inverse is the safer direction: the control is present and clickable, the operator is merely told something untrue about it. A sweep for the original direction — 26 terminal heights at width 120, four approvals plus a clarify mounting into a full region, every scroll position at 80x30, and five other arrangements — found **zero** cards left looking live with an unreachable control. The multi-card fix achieved its safety goal; this is the bill for it.

**Not the same as the hand-scrolled item below.** That one's premise is that resize and re-wrap recompute correctly and only hand-scrolling escapes them. There is no operator scroll in this reproduction: the resize path itself yields the stale label.

### The resize trigger for the reachability marker is unpinned, and it is the only trigger a command-less prompt has

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P1
**Effort.** Small — one test.
**Worth it when.** Immediately. This is the round-5 hole entered from the other side, and the code is correct today only by luck of nobody having touched it.

**Context.** Round 5 asked whether the `CommandPanel.Rewrapped` channel was dead weight and pinned it on both halves. It did not ask the mirror question. Deleting `self.call_after_refresh(self.reveal_actions)` from `PromptRegion.on_resize` leaves **all 651 tests green**, and the call is load-bearing: six clarify prompts (no command body, so no `CommandPanel` exists and no `Rewrapped` message is ever posted) mounted at 120x40 and resized to 60x12 give, unmutated, five cards correctly marked `answer below — scroll`; with the trigger deleted, **zero** cards marked and the same five controls still unreachable — the original defect restored in full, suite green.

Every round-5 test that resizes uses approval cards, whose command body posts `Rewrapped` on the same resize and masks the missing trigger. The uncovered case is any prompt *without* a command — clarify, secret, sudo — in a region that resizes.

**Suggested framing.** One test: several clarify cards, resize the terminal narrower, assert every card whose control leaves `scrollable_content_region` is retitled. Closing this also closes the mount-path item below, which is the same hole from the other direction. This is the repository's own "a guard nothing can exercise is a guard nobody can trust" rule applied to round 5's own new code.

### ~~`test_a_card_mounting_into_a_full_region_is_still_recomputed` fails intermittently across full runs~~ — CLOSED 2026-08-03

**Closed by.** Reading the chain rather than re-running until it broke. The marking is **two chained `call_after_refresh` calls** deep — `CommandPanel.Rewrapped` → `reveal_actions` (`talaria/ui/prompts.py:802`) → `mark_unreachable_controls` (`:852`) — while the test's `settle()` helper pumps exactly **two** refresh cycles. That is a margin with no slack, and a machine slow enough to lose one cycle samples the border title before the second deferral lands. The hypothesis recorded below was the right one; this is its confirmation, plus the specific arithmetic that makes it true.

The test now waits for the marking with a bounded budget instead of sampling at a fixed refresh depth. **That is not the "obvious change" this item declined to make.** Polling *the same assertion* with a deadline still fails when the marking never lands — which is exactly what the defect this test exists for produces — and that was verified rather than assumed: deleting `call_after_refresh(self.reveal_actions)` from the `Rewrapped` handler still fails the test with the wait in place. What the wait removes is only the assertion that the marking arrives within a particular number of refresh cycles, which was never the behaviour under test.

**Not reproducible locally, and that stayed true to the end** — 20 runs of the test alone and 6 full-suite runs under six busy loops, all green, all reporting zero extra cycles needed. Both observed failures were on GitHub runners. So the fix rests on the mechanism being legible in the code and on the mutation still killing the test, not on a red run turning green.

**The two P1 items below are untouched by this.** They are about cards that are *never* recomputed, which is a product defect; this was about when a test looked.

### The original entry


**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification
**Priority.** P1 — not because the product symptom is severe, but because a suite that fails intermittently cannot be used as a gate, and this unit's definition of done is a green `uv run pytest`.
**Effort.** Small to make the test deterministic; medium to answer the question underneath it.
**Worth it when.** Before anyone treats a single green `pytest` run as evidence, and before the next unit inherits the habit of re-running until it passes.

**Reproduction.** `uv run pytest -q` from the repository root, repeated. Measured on the U10 closeout tree, macOS 26.5.2 arm64, Python 3.12.11: **thirteen consecutive full runs, twelve green and one with `tests/ui/test_prompts.py::test_a_card_mounting_into_a_full_region_is_still_recomputed` failed** — eight runs during closeout (one red) and five more afterwards during commit verification (all green), so the observed post-closeout rate is roughly one in thirteen and a handful of green runs says very little. The adversarial review measured two failures in five on the pre-closeout tree, with the assertion `assert 'waiting for you' == 'answer below — scroll'` — the border-title assertion at the end of that test, not the geometry assertion before it. It is **not** reproducible in isolation: 20 consecutive runs of that test alone, with four busy loops saturating the machine, all passed. Something about the full-suite ordering is required.

**What is and is not known.** The failing assertion says the card's control is off screen (that assertion passes) and the card is still titled "waiting for you". The marking is reached through two chained deferrals — `CommandPanel.Rewrapped` → `call_after_refresh(reveal_actions)` → `call_after_refresh(mark_unreachable_controls)` — while the test's `settle()` helper awaits two `pilot.pause()` cycles. **Hypothesis, not a measurement:** the test observes the arrangement one refresh before the marking lands. If that is right this is a test-synchronization defect and the product is correct. If it is wrong, it is a third face of the two items below and the product intermittently never marks at all.

**Deliberately not "fixed" at closeout.** The obvious change — poll for the title with a deadline instead of asserting it once — makes the red run green without settling which of those two it is. A red test whose cause is unknown is worth more than a green one that was adjusted until it passed. Resolve the hypothesis first: instrument `mark_unreachable_controls` to record when it runs, run the full suite until it fails, and read whether it ran at all.

### A control-only card mounting into a full prompt region is never recomputed

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round — found while fixing the multi-card reveal, queued rather than fixed. Re-priced from P2 to P1 by the round's verification, which disproved the mitigation this item originally claimed.
**Priority.** P1
**Effort.** Small
**Worth it when.** A session queues a clarify, a secret or a sudo behind enough other prompts to fill the region — or before the "answer below — scroll" marker is relied on as a guarantee rather than as a best effort.

**Context.** `PromptRegion` recomputes which cards can reach their control on two triggers: its own `Resize`, and `CommandPanel.Rewrapped`. Instrumented over a run, a card mounting into a region that has already reached `max-height: 70%` fires **only** `Rewrapped` — which is what pins that channel — but `Rewrapped` comes from the command body, and only approvals have one. A clarify, secret or sudo mounting into a full region therefore fires **neither** trigger: measured, `on_resize` and `on_command_panel_rewrapped` both stayed silent, and the new card kept the default `waiting for you` title with its input below the fold.

**This item first said the case was "partly masked" because `PromptRegion.apply` calls `card.focus_answer()` and Textual scrolls a focused widget into view. That mitigation was measured and does not happen — raise the priority accordingly.** Four approvals at 120x40, then a `sudo.request`: `app.focused` is the sudo card's `Input` at region `(2, 49, 115, 1)`, the region's `scrollable_content_region` is `(0, 10, 119, 25)`, and `contains_region` is **False**. Pressing six keys leaves the input's `value` six characters long while the same screenshot contains neither the words `sudo password` nor a single password-mask glyph anywhere.

So the operator types a password into a focused control that draws nothing, with no visual confirmation of any kind. Nothing leaks — R9 holds, the value never reaches the transcript, the screen or a log — but "type your password blind" is a materially worse failure than the stale-title one this item was originally filed under, and it is the reason this is P1 rather than P2.

**Suggested framing.** Schedule the recomputation from `apply` whenever it mounted or removed a card. Note that doing so makes the `Rewrapped` channel redundant for the mount case — check, by deleting it and running `test_a_card_mounting_into_a_full_region_is_still_recomputed`, whether it still has a case of its own before keeping both.

## P2

### A streaming table's block phase hitches past ~340 rows — demote open tables early, or append rows incrementally

**Author.** RA5, 2026-08-09 — the one red check the v0.2 gate loop recorded instead of fixed
**Priority.** P2
**Effort.** Medium (early demotion) / Large (incremental row-append)

**What is true.** A still-open, block-rendered markdown table re-renders wholesale on every
streamed append — an open construct's reparse window is the whole construct — so per-delta cost
grows with the table: past the 50 ms coalescing interval at ~340 rows, 54–59 ms plateau, one
observed 190 ms outlier, until the two-condition trigger demotes it at ~500 rows. After demotion
every append is bounded (≤ 44 ms measured; the fence's ring steady state is 4–9 ms). The gate
enforces the steady-state phase and records `block_phase_peak_ms` verbatim (RA5); a user
streaming a 400-row table today feels the hitch.

**Fix shapes, in preference order.** (1) A streaming-tail-only trigger: an open table beyond
~300 rows demotes before the crossing — one RA4-excluded demotion, then ring appends; needs the
two-sided ownership proof's expected-set derivation (`_ktd2_selects_block`) to learn the tail
trigger, and the commit handoff to re-promote a demoted-early table whose committed body is
under the committed trigger (one bounded update at commit). Rejected tonight only for review
surface, not design doubt. (2) Incremental row-append inside `_BoundedMarkdownTableContent`
(RA2's own widget): no claim or behavior change, deepest surgery. An adaptive measured-cost
trigger was rejected outright: demotion would depend on machine load, and replay determinism
compares structure across replays.

**Worth it when.** A real corpus streams tables past ~300 rows, or the next gate hardening pass
wants the block phase enforced rather than recorded.

### Committing a monster line-rendered entry still mounts every row in one apply before condensation folds it

**Author.** the tail-cap fix, 2026-08-09 — the sibling transient the cap did not close
**Priority.** P2
**Effort.** Small to Medium

**What is true.** The live tails are now bounded everywhere — pre-capped at build (`_build_unit`'s
`max_rows`), patched incrementally on growth, folded by `_condense`'s budget walk. But a committed
entry that arrives *without* a matching tail handoff (a resumed session's history, or a commit whose
final body differs from the tail's applied text) goes through `_prepare_committed_entry` →
`_prepare_unit` with `max_rows=None`: a 10,000-line fallen-back entry mounts 10,001 widgets in that
one `apply()`, and `_condense` folds them back to the cap in the same call. Net state is bounded;
the transient is not — it is the same one-frame spike family RA4 excludes for the tail demotion,
but on the commit path, where nothing measures it today (the stress corpus's largest entries are
far below the trigger; the workloads never commit).

**The fix shape.** Pass the retention the budget walk would compute — or simply `mount_cap` — as
`max_rows` for line-rendered committed builds too. The straddle fold's retention convention
(`retain = end - new_top`, mounted lines shrink from the left) already tolerates a pre-folded unit,
which is what makes this small; the care is in `_reconcile_committed`'s batch path, where several
entries build against one budget, not one.

**Narrowed on 2026-08-10 (CR3 re-review fix).** Markdown-kind entries whose final body trips the
fallback trigger now build pre-capped at `mount_cap` (`_prepare_committed_entry` rechecks both
demotion conditions and passes `max_rows`), which closes the transient for exactly the shapes the
trigger can see — the 10,000-line fence included. Still open: non-markdown kinds (a monster
`system` entry has no trigger check and still mounts every row), and the batch-budget nuance above.

**Worth it when.** A real corpus (resume with monster history, or a commit-differs-from-tail
stream) shows the spike, or the gate grows a workload that commits — measure first, the family's
last two fixes both moved on evidence.

### Textual's own command palette is reachable on `ctrl+p` and nothing in Talaria put it there

**Author.** the picker redesign, 2026-08-07 — found by a dialog test, not by looking for it
**Priority.** P2
**Effort.** Small

**What is true.** `App.ENABLE_COMMAND_PALETTE` defaults to `True` and `TalariaApp` never sets it, so
`ctrl+p` opens Textual's built-in palette over whatever is on screen. This is **not** new: it has
been reachable since the first Textual commit and predates the picker work entirely. It surfaced now
because binding `ctrl+p` to "move up" in the picker dialog silently did nothing —
`tests/ui/test_dialog.py::test_ctrl_p_is_not_a_movement_key_because_textual_owns_it` pins the
collision so nobody re-tries it.

**Why it is worth fixing rather than tolerating.** Talaria has its own command listing (R23/AE9, F3
and `/commands`), built to show the *gateway's* catalogue with availability markers. Textual's
palette shows Textual's commands — change theme, save screenshot, toggle dark mode — which are
framework affordances an operator of a Hermes client has no reason to see, and which are not part of
any requirement. Two palettes on two keybindings, one of which nobody chose, is the kind of thing an
operator finds by accident and then has to be told to ignore.

**The fix, and the reason it is not already done here.** `ENABLE_COMMAND_PALETTE = False` on
`TalariaApp`, which also frees `ctrl+p` for emacs-style movement in the dialog. Left out of the
picker change deliberately: it changes a global app affordance rather than the picker, and it wants
its own before/after check that nothing in the test suite was relying on that palette.

**Worth it when.** Anyone touches app-level key bindings, or the dialog wants `ctrl+p`/`ctrl+n`.

### R28's equivalence proof leaves the repository with the TypeScript tree

**Author.** Conformance audit of R1–R40, batch 2, 2026-08-05 — recorded as DRIFT-02 in [the audit's finding register](../analysis/2026-08-05-conformance-audit-drift-findings.md). Found by the independent static pass. Filing this entry *is* that finding's resolution: the defect was a scheduled consequence with no record attached.
**Priority.** P2
**Effort.** Small if the choice is made deliberately; the cost is entirely in making it under time pressure instead
**Worth it when.** The moment anyone proposes removing `src/`. This entry exists to be found then.

**Context.** R28 requires that the TypeScript and Python recorders produce contract-equivalent frame logs. The test proving it runs the **real** TypeScript recorder as a subprocess: `tests/recorder/ts_bridge/run_ts_recorder.mjs` imports `FrameRecorder` directly from `src/record/recorder.js`. That is a direct file import, so deleting `src/` breaks the proof immediately rather than degrading it.

**The existing records anticipate the wrong casualty.** `CLAUDE.md`, this file, and `DECISIONS.md` all reason about `src/` removal in terms of the Node `check` job that runs `npm run check` — correctly predicting that Prettier and that job leave with the tree. R28's harness does not live there. It is a pytest test in the `python-check` job that spawns `tsx`, so it leaves too, and nothing said so.

**Severity is low today and moderate at removal time.** The failure is loud — the bridge import breaks and the test errors — so this cannot leak a credential or pass a false proof. The risk is that the decision gets made by deleting a red test under deadline, which retires R28's evidence without a decision record.

**The choice to make, when the time comes.** Either vendor a frozen copy of the TypeScript reference recorder purely as a test fixture, keeping the equivalence executable; or accept that R28 becomes historical, and record in `DECISIONS.md` that the equivalence relation was proven at a named commit and is no longer re-verified. Either is defensible. Making the choice implicitly, by deleting a failing test, is not.

### A replay cannot reconstruct Talaria's own delivery notes, so an unacknowledged submit replays as a clean one

**Author.** post-v0.1, second operator session against a live gateway
**Priority.** P2
**Effort.** Medium — the correlation is easy; amending an entry the append-only transcript has already written is not.
**Worth it when.** A replay is used to diagnose a *failed* session rather than to demonstrate a working one. Until then the gap only understates trouble in recordings that had none.

**Context.** `record_submission` writes the operator's line and, when the `prompt.submit` call did not resolve cleanly, a second `system` entry naming what is known — `not sent`, `delivery unconfirmed — the connection dropped`, and the two others in `DELIVERY_NOTES`. Those notes are authored locally from an observed RPC outcome and cross no wire, so they are in no frame log.

Replay now reconstructs the operator's line from the recorded outbound `prompt.submit` (see DECISIONS, "A replay reads one outbound frame") and deliberately claims nothing about delivery — `record_replayed_submission` takes no `DeliveryState`, because at the moment the request is ingested the answer is a frame that has not arrived yet. The consequence is that a recording of a session whose submit went unacknowledged replays as though it were fine: the operator's line appears, the warning under it does not.

**Suggested framing.** The evidence is in the recording — the response frame carries the request `id`. What is missing is a way to *amend* a transcript entry after later frames arrive, which the append-only transcript has no mechanism for and should probably not grow one for casually. Consider instead deferring the write: hold the submitted text until its response frame resolves, then write the line and any note together. Note what that costs — the operator's line would appear a beat late in a replay, where today it appears exactly where it was sent.

### `compare_shape` compares the top level only, so a wholly restructured payload reads as "present"

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification
**Priority.** P2
**Effort.** Medium — the comparison is easy; deciding how deep to go and what an *acceptable* nested change looks like is the work.
**Worth it when.** A real gateway has been attached (R2) and the check has been run against at least one Hermes upgrade, so the false-positive cost of going deeper is knowable rather than guessed.

**Context.** The startup compatibility check grades a method `present` when the response's own key set and each value's *kind* match U3's pin. Nothing below that is looked at. Measured: a gateway answering `commands.catalog` with `{"pairs": [{"name": "/status", "desc": "renamed field layout"}]}`, `agents.list` with `{"processes": ["not-an-object-at-all"]}` and `spawn_tree.list` with `{"entries": [{"totally": "different", "from": "the pin"}]}` produced `ready: True`, `0 blocking`, and `commands.catalog: present, top-level response shape matches 7f4d15515`.

This is U3's deliberate v0.1 scope and `talaria/domain/compat.py:343` says so. It is queued rather than left implicit because the word an operator reads is `present`, which sounds like a broader statement than the one being made — every entry in `commands.catalog` could have been restructured and the check would say the catalogue was fine, while the parser that consumes it fell over. The wording now says "top-level response shape" in the code, the report line and the verdict document; the *coverage* is unchanged.

**Suggested framing.** One level deeper for the three methods whose payload Talaria actually parses element by element (`commands.catalog`, `agents.list`, `spawn_tree.list`): compare the kind of the first element of each recorded list. Do not recurse generally — the maintenance cost is what U3 rejected, and that judgement still holds.

### ~~R1's Linux half has never executed — the `/proc` branch is unmeasured~~ — CLOSED 2026-08-03

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification
**Priority.** P2
**Effort.** Small, and it needs a Linux machine or a Linux CI runner.
**Worth it when.** The CI matrix runs the process-surface tests on Linux, or anybody proposes to run Talaria on Linux as a daily driver.

**Closed by.** Pushing the U10 branch, which is the first time this repository's CI had anything to run. On the pull request that carries the daily-driver verdict (GitHub Actions run `30865814553`), `python-check-linux` executed **all five** process-surface tests on `ubuntu-latest` under Python 3.12 and 3.13, and all five passed — so the `/proc/<pid>/cmdline` and `/proc/<pid>/environ` branches have now run against a real process on the platform whose kernel behaviour the R1 argument rests on. Read as this item asked: the job's own output was inspected line by line rather than trusted from a green tick, which matters because the job is `continue-on-error: true`. Its progress line was `tests/transport/test_process_surface.py .....`, five dots and no skip. The same run also executed all fourteen pseudo-terminal teardown tests on Linux with no skips; the seven Linux skips are all the TypeScript equivalence bridge, which needs `node_modules`.

**What this does not close.** Nobody has driven the interface on Linux, and no real terminal emulator has been used on either platform. R1's environment clause remains **unmet** on Linux exactly as on macOS — the measurement confirmed the mechanism rather than removing it. See the item above.

**Context.** `tests/transport/test_process_surface.py::read_surface` has two branches: `/proc/<pid>/cmdline` and `/proc/<pid>/environ` on Linux, `ps -ww` / `ps -Eww` on macOS. Only the macOS branch has ever run. The Linux branch is marked `# pragma: no cover - exercised on Linux only` and is exactly that — never exercised. Every R1 measurement in this build is a macOS measurement.

That matters more than a coverage gap usually would, because the *argument* for R1's unmet half is a Linux argument: the kernel snapshots the environment block at `exec` and serves it from `/proc/<pid>/environ` for the life of the process, which is why scrubbing cannot work. The claim is right, and it is reasoned from documentation rather than measured here.

**Suggested framing.** The existing `python-check-linux` job runs the suite on Ubuntu with `continue-on-error: true`. Read its output for these five tests specifically before trusting anything in the R1 section on Linux; a `continue-on-error` job that is never read is not evidence.


### Command entry is a listing, not completion — three catalogue fields go unread

**Author.** v0.1 milestone-2, unit U9 (slash commands and paste collapse)
**Priority.** P2
**Effort.** Medium
**Worth it when.** Talaria is being used as a daily driver against a gateway whose catalogue runs to ninety-odd commands, at which point reading the list stops being a substitute for finding one.
**Context.** U9's plan entry asks for a "minimal entry affordance", and `talaria/ui/palette.py` is exactly that: a foldable region on F3 listing every command with its availability marker. It is a listing. Typing still means typing the whole name, and three fields the gateway already sends are decoded and then unused:

- **`canon`** *is* used, for alias resolution — this one is wired.
- **`sub`** maps each command to its tab-completable subcommands (`tui_gateway/methods_tools.py:352`). Nothing reads it, so `/goal ` offers nothing after the space.
- **`skills`** carries a per-skill `{usage, origin}` pair, which the gateway assembles specifically so a client can rank the listing — its own comment says "every consumer that renders the catalog also wants to rank it" (`:337-341`). The listing is in catalogue order instead.

The gateway also publishes `complete.command`, `complete.path` and `complete.at` (`tui_gateway/methods_complete.py`), none of which Talaria calls.

**Deliberately deferred, not overlooked.** A completion overlay is a second focus owner in front of the composer, and the composer is the widget the interface is built around — U9's own decision to make the sub-agent interrupt a row action rather than a key binding turned on the same concern. Getting that wrong costs more than the typing it saves.

**Do.** Filter the listing as the composer's text changes, rank skills by `usage`, and read `sub` for a second-word completion, before reaching for an overlay or for the `complete.*` methods.

### A deny-all that succeeds can re-offer a control the gateway already resolved

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Before an operator is expected to trust that a card which reappears is a live question. Also whenever the deny-all-while-in-flight decision is revisited — this is one of its costs.

**Read this beside the decision it comes from:** *"Deny-all reports what it decided and what it cannot know as two clauses, not one total"* in `DECISIONS.md`, whose rejected alternatives include refusing deny-all while an answer is in flight. This item is the price of that rejection and should be weighed with it rather than on its own.

**Context.** `approval.respond` with `all: true` resolves the whole session queue, including an entry whose own `approval.respond` is still travelling. When that in-flight call comes back `not_sent`, `_record_prompt_outcome` takes the restore branch and `restore_prompt` puts the card back — offering `once` for an approval the gateway denied a moment earlier. Pressing it sends a second `approval.respond` into a queue that should be empty; the reply's `resolved: 0` is caught and reported (`GATEWAY_HAD_NO_APPROVAL`), so the outcome is a confusing screen rather than a wrong grant.

**PLAUSIBLE, not confirmed.** Read from two code paths and **not reproduced**. Reproducing it needs a `not_sent` outcome on the single-answer call while a deny-all succeeds on the same session, which the `HoldingDispatcher` can arrange.

**Likely shape of the fix.** `restore_prompt` already declines to restore a prompt whose id is in `flushed_prompt_ids`; a deny-all that resolves the queue could latch every id it swept, which is the same latch an expiry uses and would need no new mechanism.

### The `feed()` fixture's clock is nine days behind the age-out's

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Before any prompt test is given a real `coalesce_interval`, or the next time a prompt test flakes on an empty screen.

**Context.** `tests/ui/test_prompts.py`'s `feed()` stamps frames at `1_785_000_000 + seq`, roughly nine days behind wall clock, while the live age-out reads `time.time()`. Every approval fed that way is already stale by nine days. Nothing fails today only because `live_app` parks the coalesce timer at 3600 s, so the tick that would withdraw them never runs — roughly thirty prompt tests are one `coalesce_interval=` argument away from silently asserting against an empty screen. The two round-5 age-out tests avoid it by ingesting a `FrameRecord` stamped from the real clock.

**Product behaviour is unaffected, and this was checked rather than assumed.** `LiveSource` stamps frames from its own clock, defaulting to `time.time` (`talaria/transport/source.py:37,181,517`), so a live approval's `opened_at` and the age-out's `now` come from the same clock.

**Suggested framing.** Either stamp `feed()` from the real clock, or make `live_app` refuse a real coalesce interval unless the test opts in explicitly — the second is the stronger version, because it makes the trap loud at the moment a test walks into it.

### A hand-scrolled prompt region carries a stale reachability marker

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Alongside the item above — the two share a fix.

**Context.** `mark_unreachable_controls` runs after a resize or a re-wrap and not after a scroll, so an operator who scrolls the region by hand can leave a card marked `answer below — scroll` whose control is now on screen, or leave the ordinary `waiting for you` on a card whose control has just scrolled off. The direction of the error is a wrong label rather than the original silent inertness, and the scrolled state is one the operator caused and can undo — which is why this is separated from the defect it comes from. `ScrollableContainer` exposes `scroll_y` as a reactive, so a `watch_scroll_y` that chains to `super()` is the obvious hook; it needs a throttle, because it would otherwise run on every row of a drag.

### Two queued approvals with realistic commands: the second one is unreadable

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round
**Priority.** P2
**Effort.** Medium — this is a layout redesign, not a scroll fix.
**Worth it when.** A real session queues two approvals with long commands, or before an operator is expected to *read* rather than merely dismiss a queue.

**Context.** At 80x24 with two long commands, the second card is cut by `PromptRegion`'s `max-height: 70%`. Measured: `deny all` appears once rather than twice, and `max_scroll_y == 9`.

**Severity is below the resize defect fixed this round, and the reason is what makes it a P2 rather than a P1.** The *first* card's `deny all` is visible, and one press denies the whole queue — so the operator is never trapped without a safe action. The harm is being unable to read the second command before denying it, and for a denial that is the survivable direction.

**Worth fixing at the same time.** The overflow marker reads `… +N more lines (whole command in transcript)`. The second clause is true of the transcript *data* and false of the transcript *pane* at the moment the decision is live — the pane is scrolled to the bottom and the arrival entry has usually left it.

### Honour the clarify bridge's multi-select hint

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)
**Priority.** P2
**Effort.** Small
**Worth it when.** A real session raises a clarify that sets the flag, or checkbox-style answers appear in a corpus.
**Context.** `clarify.request` carries `multi_select: true` when the tool wants several choices, and the gateway emits the field *only* when it is true so that single-select payloads keep their pre-multi-select shape (`tui_gateway/server.py:5506-5519`). Its own comment says renderers without checkbox support "ignore the extra field and stay single-select (a single answer still parses as a one-element list on the tool side)", so Talaria's single-choice control is a supported degradation rather than a defect. It is recorded here because the degradation is currently *silent*: the operator is not told that the question wanted more than one answer.
**Refs.** `tui_gateway/server.py:5506-5519` at Hermes `7f4d15515`

### Send `approval.respond`'s `all` flag, or establish that the choice string carries it

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)
**Priority.** P2
**Effort.** Small
**Worth it when.** An operator reports that picking "always" did not stop the next identical approval.
**Context.** `approval.respond` accepts an `all` parameter, passed to `resolve_gateway_approval(..., resolve_all=params.get("all", False))` (`tui_gateway/methods_prompt.py:887-905`). The shipping terminal UI never sends it — it sends `{choice, session_id}` and nothing else (`useMainApp.ts:928`) — so the `session`/`always` semantics must ride the choice string, and Talaria matches that exactly. What is *not* established is whether the flag does something the string does not; the read stopped at the call site rather than following `resolve_gateway_approval` into `tools/approval.py`. Cheap to settle, and the failure it would produce is an approval the operator believes they answered permanently.
**Refs.** `tui_gateway/methods_prompt.py:887-905`, `ui-tui/src/app/useMainApp.ts:928` at Hermes `7f4d15515`

### Bound the domain transcript, not just the mounted widget count

**Author.** v0.1 unit U3 (reconciliation catalogue rule RR-21), 2026-08-02
**Priority.** P2
**Effort.** Medium
**Worth it when.** The U5 gate publishes its memory growth curve, or a real session runs long enough that resident memory becomes visible.
**Context.** KTD14 bounds *mounted widgets* (default 500) and explicitly leaves the domain transcript accumulating without eviction. U3 widened that gap on purpose in two places, and both are catalogued rather than hidden.

Hermes truncates its reasoning buffer at 80,000 characters by discarding all but the last 60,000 (`ui-tui/src/app/turnController.ts:778-780` at `7f4d15515`) and gates reasoning capture on a display setting. Talaria does neither, because R6 puts reasoning *presentation* out of scope while requiring that its *content* is never dropped, and both of Hermes's behaviours drop content. `tests/domain/test_transcript_state.py::test_reasoning_is_committed_at_turn_end_and_never_truncated` pins that with a 100,000-character block.

Talaria also keeps sub-agent rows past the end of their turn, where Hermes drops them at `idle()` and archives the fan-out to disk. Talaria cannot archive: R17 forbids authoring sub-agent state, so `spawn_tree.save` is not in its vocabulary (catalogue rule RR-32). Rows are cleared by the next `message.start`, so the bound is one turn's fan-out rather than a session's.

Neither is a leak today at prototype scale, and eviction interacts with replay determinism (AE2) and scrollback in ways that need the projection to exist first — which is exactly why the plan deferred it. The input to the decision is the growth *slope* the U5 gate records, not the endpoint.
**Refs.** [Hermes reconciliation-rule catalogue](../analysis/2026-08-02-hermes-reconciliation-rules.md) rules RR-21 and RR-32, [v0.1 plan KTD14](../plans/2026-08-02-talaria-v0-1-prototype-plan.md)

### Range-validate the integer configuration settings

**Author.** v0.1 scaffold code review (delta re-review), 2026-08-03
**Priority.** P2
**Effort.** Small
**Worth it when.** U6 builds the status runner and U5 the composer — those units know what a valid bound is, and this module does not.
**Context.** `talaria/config.py` type-checks integer settings but does not bound them. Verified 2026-08-03: `TALARIA_STATUS_INTERVAL_SECONDS=-5` resolves to `-5`, and `TALARIA_COMPOSER_PASTE_COLLAPSE_LINES=0` resolves to `0`. KTD16 defines the paste thresholds as "6 or more lines, or 512 or more bytes", so a threshold of `0` collapses every paste including a one-line one; a negative interval hands U6's status runner a negative sleep.

Deliberately not fixed in the scaffold. The bound is a semantic property of the consuming unit, and inventing minimums in the config loader mid-run would be this session guessing at values the plan does not specify. The class predates the scaffold's coercion rewrite — the old code accepted these values too — but the rewrite is the natural place bounds will land.

**Half-resolved by U9, 2026-08-03 — the paste half only.** The consuming unit arrived and answered its own question: `PasteThreshold` treats a non-positive bound as "this half is off", re-encoding the shipping client's `pasteCollapseLines > 0 && …` guard (`ui-tui/src/app/useComposerState.ts:277-280` at `7f4d15515`). So `TALARIA_COMPOSER_PASTE_COLLAPSE_LINES=0` no longer collapses every one-word paste; it disables the line bound and leaves the byte bound working. `talaria/cli.py:_build_paste_threshold` also falls back to the KTD16 defaults for a non-integer value rather than raising, because a malformed paste setting should not stop the client from starting. Pinned by `tests/domain/test_commands.py::test_a_non_positive_line_bound_switches_that_half_off` and `::test_both_bounds_off_collapses_nothing`, both watched to fail with the `> 0` clause removed.

**Reach closed by U10, 2026-08-03.** `talaria/cli.py:build_live_app` now assembles the live shell — transport, credential provider, status runner, startup selection — and passes `_build_paste_threshold(cfg)` into it, so a bare `talaria` run reaches the configured thresholds in the one mode where a paste is ever collapsed. Pinned by `tests/test_cli.py::test_the_configured_paste_thresholds_reach_the_live_app`, which compares a configured launcher against a default one so it asserts the configuration was *read* rather than that a threshold exists, and was watched to fail with the `paste_threshold=` argument removed from `build_live_app`. **Reach is not the same as live proof:** no paste has been collapsed by a Hermes gateway, because none has been attached — see the daily-driver verdict, `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`.

**Still open: `status.interval_seconds`.** `TALARIA_STATUS_INTERVAL_SECONDS=-5` still resolves to `-5` and still hands U6's status runner a negative sleep. There is no equivalent Hermes reading of a negative interval to re-encode, so the bound is a decision the status runner has to make, and it was out of U9's scope. Nothing in this update changes that path.

### Add MoA progress and fallback rendering

**Author.** Project bootstrap
**Priority.** P2
**Effort.** Medium
**Worth it when.** The first transport path is proven and MoA event capability differences are captured in fixtures.

### Add a deterministic Kanban adapter

**Author.** Project bootstrap
**Priority.** P2
**Effort.** Medium to large
**Worth it when.** The board contract and ownership boundary are clear enough to avoid taking accidental responsibility for dispatcher internals.
**Constraint, verified against Hermes `7f4d15515`.** The adapter cannot sit behind the terminal gateway: `tui_gateway/` registers no `@method("kanban.*")` at all. It does push board _notifications_ — a 5-second poll of `kanban_notify_subs` emitting completed/blocked/gave_up/crashed/timed_out events — so "tell me when a task finishes" is already available there, while "query the board" is not. A queryable adapter has to reach `GatewayRunner` or the API server instead. See [LEARNINGS](LEARNINGS.md).

### ~~`test_the_status_command_runs_and_renders_under_replay` flakes under load~~ — EXPLAINED and fixed 2026-08-03

**Priority.** P3 — reported as an unexplained CI flake; root cause found by reproducing it locally under CPU load.

Not a race in the KTD5 overlap guard. `TalariaApp` starts a background status loop as soon as it runs (`talaria/ui/app.py:154`), and that loop ticks *before* its first sleep — so the app has a tick in flight almost immediately. The test then fired a second tick explicitly and asserted `outcome == "ok"`. When the app's own tick was still awaiting its Python subprocess, the guard correctly returned `overlapped_skip`, which is the guard doing exactly what R21 specifies.

It is load-sensitive because the window is the duration of an interpreter spawn: slow under CI load, near-instant on an idle developer machine. Reproduced deliberately by giving the status child a 0.6s sleep, which makes the first attempt `overlapped_skip` **every** time; the fixed test retries and reaches `ok` after 11 attempts.

**The family, not just the instance.** This is the third load-sensitive assertion in this suite — an earlier one asserted an overlap count against a 0.3s timeout that left 0.1s for an interpreter spawn. Tests that spawn real subprocesses and assert exact outcomes are betting on machine speed. Worth a sweep if another appears.

**Still unexplained, and deliberately not claimed as fixed:** the CI failure at `3231803` was `test_overlap_at_most_one_child_ever` reporting zero successful invocations of three — a *different* test with its own runner and no app involved. Same family (load-sensitive, spawns real interpreters), but the mechanism above does not explain it, and it has not recurred.

### Status child can escape the process group with setsid, and stdout EOF is not child exit

**Priority.** P2 — found by adversarial review of the status runner, 2026-08-03; not fixed in milestone 1.

Two residual defects in `talaria/status/runner.py`, both bounded in blast radius but real:

- A descendant that calls `setsid()` leaves the group, so `_kill_process_group` does not reach it. It survives the tick and, if it inherited the pipes, holds them open — which costs a second timeout budget before the runner gives up. Measured at 4.01s against a configured `timeout_seconds` of 2.0.
- Output is read until stdout reaches EOF, not until the child exits. A command that backgrounds anything inheriting stdout (`worker & echo ok`) therefore reports `timeout` and discards output the direct child already produced correctly.

Both need the same thing to fix properly: wait on child exit and treat the streams as separately terminable, rather than treating stdout EOF as the completion signal. Deferred because it changes the completion semantics of every tick and milestone 1 is closing; the memory, orphan and descriptor-leak defects found alongside these were fixed because they were unbounded.

### A bearer capability carried in a URL path is recorded verbatim

**Priority.** P2 — found by external review of the redaction boundary, 2026-08-03; deliberately not fixed.

`redact_url` withholds credentials from a URL's userinfo and its query string. It does not touch the path, so the concrete Chrome DevTools Protocol form — `ws://127.0.0.1:9222/devtools/browser/<GUID>`, where the GUID alone drives the browser — reaches the frame log intact. It is reachable: at Hermes `7f4d15515`, `browser.manage` returns the operator's configured CDP override on an ordinary status call (`tui_gateway/server.py:13405` → `methods_tools.py:1349`), and that override may be set to the concrete WS form.

Not fixed because both obvious rules are bad. A Hermes-shaped path rule (`/devtools/browser/<segment>`) protects exactly one known shape and will not generalize to the next capability-bearing path — and it is worse than doing nothing, because it creates the appearance that paths are handled, which is the same staleness failure the deny-set was already bitten by. A generic "high-entropy path segment" heuristic would redact ordinary URLs — commit SHAs, content hashes, UUID resource ids — and the corpus exists to be studied; over-redaction is a different failure, not the safe direction (the same reasoning that keeps `max_tokens` out of the key-name net).

**Correction, 2026-08-03.** This entry originally justified the deferral on the grounds that the form is "loopback-only". That is false, and the error mattered: it made the risk look structural when it is merely default. Loopback is the *default* (`hermes_cli/browser_connect.py:21`, `DEFAULT_BROWSER_CDP_URL = "http://127.0.0.1:9222"`) and nothing constrains the override to it. Hermes documents the opposite to operators directly — `hermes_cli/tips.py:306` at the pin reads *"BROWSER_CDP_URL connects browser tools to any running Chromium-family browser — accepts WebSocket, HTTP, or host:port"* — and `_resolve_browser_cdp_url` is deliberately structured to avoid blocking on an unreachable host, which is code written for the remote case. Remote CDP is configurable today.

**A third rule exists and is the leading candidate:** withhold the path only for `ws`/`wss` URLs whose host is not loopback. It carries no Hermes-specific shape, and it costs nothing on the study data, because commit SHAs, content hashes and UUID resource ids live in `http`/`https` document URLs rather than in remote WebSocket paths.

**What actually blocks it is the redactor rule, not the harness.** An earlier version of this entry priced the blocker as "teaching the KTD6 comparator a new kind of authorized divergence", which overstates it — and a queued item whose stated blocker is larger than the real one gets mispriced and deferred again, the same failure as the dead revisit trigger above. The harness now has frame-body divergence authorization (`_frames_equivalent` plus `PYTHON_ONLY_FRAME_REDACTION_REASONS`), so a path allowance is roughly ten lines mirroring the existing query-key allowance in `_compare_endpoint`. One genuine difference in kind survives: the query allowance is set membership over an enumerable list of key names, while a path allowance must be a *predicate* on the URL (`scheme in {ws, wss} and host not loopback`). That is three lines and deterministic.

The real dependency is sequencing: the comparator has to encode an expectation about a redactor rule that does not exist yet. Defining the rule is the remote-attach work's job, and building the check for it first is designing against nothing.

**Revisit when** — the original trigger here was "remote CDP becoming supported", which describes a state that had already arrived and so could never fire. Two triggers that can:

1. **Attach to a remote (gated) gateway** (P1, above) is implemented. That work makes non-loopback hosts routine and is the natural place to extend the comparator.
2. A non-loopback host appears in a recorded URL. Mechanically checkable against any corpus rather than a standing intention to notice something:
   `grep -oE '"wss?://[^"/]+' <corpus> | grep -vE '127\.0\.0\.1|localhost|\[::1\]'`

This is a check someone runs, not an alarm that fires by itself. Instrumenting the redaction boundary to count non-loopback hosts would make it self-firing, and was not done because adding a counter to the security boundary for a P2 is a poor trade — but that is the honest limitation of trigger 2, and trigger 1 is the one to rely on.


### The withdrawn-approval hedge does not retire when the screen shows the agent working

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Alongside any other work on the clearing rule. The line is stale rather than false, which is why it is not P1.

**Context.** `_clear_withdrawal_on_progress` (`talaria/domain/state.py`) clears the withdrawn count only when `_turn_progress` — `(turn, turn_index, streaming_text, segments)` — moves. `_on_tool_start`, `_on_tool_complete`, `_on_status_update` and `_on_session_info` all only call `_append`, which touches none of those four fields.

Reproduced with `APPROVAL_STALE_AFTER` patched to 0.05s: after a withdrawal, feeding `tool.complete {name: "bash", summary: "denied by timeout"}` — exactly the event the default-configuration case produces, the gateway's own fail-closed timeout coming back through the tool — leaves `withdrawn_approvals` at 1, and one screenshot contains both `bash ✓ denied by timeout` and `1 approval withdrawn — whether the agent is still blocked is unknown`. Unchanged after `tool.start`, `status.update` and `session.info` too. The hedge survives an unbounded number of tool calls and only ends on a `message.delta` or a turn-phase change.

This contradicts the clearing rule's own docstring: *"The moment the agent produces a token or the turn changes phase, it is no longer unknown — it is observed."* A tool completing is such an observation.

**Severity, stated fairly.** The sentence is a hedge about Talaria's knowledge, so a stale one is a screen contradicting itself rather than a screen lying about the session being busy — and it never reinstates `working…`, which is the failure R8 actually forbids.

**Suggested framing.** Widen `_turn_progress` to include tool activity, but do it narrowly and pin both directions: the case that must *not* clear is the bad one, where the gateway still holds the approval and the agent is blocked inside a tool call producing nothing. A tool *completing* is progress; a tool *starting* may not be.

### ~~`focus_session` does not clear `withdrawn_approvals`~~ — CLOSED 2026-08-17

**Closed.** Fixed in the v0.2 answerability spine and verified again while v0.4's unit U4 was written: `focus_session` (`talaria/domain/state.py`) now sets `withdrawn_approvals=0` in its own `replace(...)` call, and its docstring states why — the count is about approvals *this* session had withdrawn from under it, so carrying it into the next session makes that session's screen hedge about a withdrawal that never happened there. Pinned by `tests/domain/test_prompt_registry.py::test_a_switch_clears_the_withdrawal_count_it_cannot_speak_for`, which fails when the reset is removed, and now also by `tests/ui/test_focus_churn.py::test_a_withdrawal_hedge_does_not_follow_a_switch` at the operator-visible level. Note for anyone re-checking this by mutation: there are **two** places the count is cleared, in different functions and for different reasons — `focus_session`'s reset, which this entry is about, and `_clear_withdrawal_on_progress`, which ends the hedge when the agent is observed working and is pinned by `tests/ui/test_prompts.py`. Mutating the second and concluding this citation is wrong is a mistake a review of this file has already made once. The paragraphs below describe the pre-repair tree and are kept as the record of what the defect was.

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P2
**Effort.** Trivial — one field, one assertion.
**Worth it when.** Before anything calls `focus_session`. There is still no production caller, which is the only reason this is latent.

**Context.** `focus_session`'s docstring says it exists to stop "session A's state bleeding into session B", and it clears `prompts`, `answering`, `approvals_seen`, `flushed_prompt_ids`, `turn`, `segments` and `streaming_text`. Round 5 added `withdrawn_approvals` and did not add it to the reset. Measured: `focus_session(SessionState(withdrawn_approvals=3, approvals_seen=5), "s2")` returns `approvals_seen=0` and **`withdrawn_approvals=3`**.

A screen consequence was attempted and could not be built: `focus_session` sets `turn="idle"`, and any later event that makes the turn stream also trips the clearing rule. So this is a genuinely broken invariant on a reset function, latent rather than visible, and unpinned. It belongs beside the existing `focus_session` item rather than separately from it.

### The terminal-read arrival line is the residual self-contamination, and it is measured

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Before a real session does many reads, or whenever the terminal-read decision is next opened.

**Context.** The decision to keep `terminal_read prompt awaiting an answer: …` in the transcript was originally justified with the claim that it "does not compound". Two independent lenses measured that it does — the correction is now in `DECISIONS.md` beside the decision. Six sequential reads in a session whose only real content is one `message.delta` grow the served payload from 172 to 512 bytes, and the sixth answer is six copies of the arrival line plus one line of actual content.

No individual line grows — that was round 3's defect. But the buffer accumulates one self-generated line per read, without bound in the number of reads, which is the same property the decision's rejected-alternatives paragraph uses to turn *outcome* lines down. Net accounting: the fifth round removed between zero and one line per **failed** read and left one line per **every** read, so the residual is strictly larger than what was removed.

**Suggested framing.** Move the arrival record to a side channel the read projection does not serve, keeping the audit property without the feedback. Dropping it entirely is the alternative already rejected, on the grounds that an agent reading the operator's screen should not be invisible in the operator's own record.

### Talaria inherits the age of a gateway it does not own, and cannot say so when that gateway fails

**Author.** First operator-supervised live run, 2026-08-04.
**Priority.** P2 — deferred deliberately. The TUI's own defects come first; this one has a known operator workaround.
**Effort.** Small for the diagnosis half; Medium-and-architectural for the launch half.
**Worth it when.** A second operator hits it, or the first time someone who did not read this entry has to debug a failed handshake.

**What happened.** Four of the five Hermes dashboards on this machine refused Talaria's WebSocket handshake with HTTP 500. The cause was not in Talaria and not in Hermes's current source: the four dashboards were **launchd processes started nine days before**, holding a stale `hermes_constants` module in memory, while the `tui_gateway/server.py` they lazily import had since gained a `DEFAULT_INDICATOR_STYLE` constant. The import failed inside a long-lived process against code that was correct on disk. `launchctl kickstart -k` on each fixed all four.

**Why Hermes's own terminal UI never sees this.** It does not dial a gateway — it **spawns one**, `python -m tui_gateway.entry` over stdio, fresh on every launch (`ui-tui/src/gatewayClient.ts:356`). A stale gateway is not a state it can reach. Talaria dials a dashboard it did not start, so it inherits however old that process happens to be. That is not an oversight; it is [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) working as decided — *"It owns its own lifetime and dials a Hermes gateway it did not launch."*

**Two separable pieces of work, and they should not be conflated.**

*The diagnosis half is a plain bug and does not touch any ADR.* Talaria rendered `handshake rejected with HTTP 500 (server rejected Web…` and nothing else. Finding the cause took launchd inspection, a dashboard stderr log, and `git log -S` against Hermes. A client that says which endpoint it dialled, how old that process is, and that a restart is the usual remedy would have replaced all of it. Note the message was *also* truncated — that specific cut is fixed, but the message was unactionable at any length.

*The launch half is an architecture decision and needs a superseding ADR, not a convenience feature.* Options, roughly in increasing distance from ADR-0001: **(a)** ensure a dashboard is running and start `hermes dashboard` if none answers — launches a supported service, keeps the WebSocket transport, brushes the "did not launch" clause; **(b)** spawn `tui_gateway.entry` over stdio the way the native TUI does — removes this failure class outright, but couples Talaria to a Hermes *internal module path*, which is exactly what this repository's guidance says to avoid in favour of transport interfaces and capability discovery. Neither should be smuggled in as a fix for the incident above.

**Recommendation on file.** Do the diagnosis half whenever the connection path is next touched; open the launch half as its own ADR conversation. The incident is an argument for better error reporting, and only weakly an argument for changing who owns the gateway's lifetime.

### Nothing on screen says where the caret is when it is not in the composer

**Author.** First operator-supervised live run, 2026-08-04.
**Priority.** P2
**Effort.** Small, but it reopens a settled layout decision.
**Worth it when.** The next time the composer's border or the focus styling is touched, or the first time an operator reports typing into a dead interface after the `CaretReleased` fix has shipped.

**Context.** Talaria silently stopped accepting typed text mid-session. The root cause — the caret landing on a scroll region that discards keys — is fixed (`talaria/ui/focus.py`, and the decision beside it in `DECISIONS.md`). This item is the *second* half of what made that defect so hard to see, and it survives the fix.

When the caret is not in the composer, the interface looks exactly the same as when it is. The composer shows its placeholder whenever it is empty, focused or not, so the one widget an operator would check reports nothing. `AgentRow.-interruptible:focus` has a background tint, but the two scroll regions and the composer have no focus styling at all, deliberately: `talaria/ui/composer.py:181-189` records that a `&:focus` border made the composer one row taller while focused, so the whole interface above it jumped by two rows every time the caret moved — including on a mouse press on a prompt button. The fix was to stop depending on focus for styling entirely.

So the requirement is narrow and real: **an indication of where the caret is that does not change any widget's height.** Candidates that satisfy it — a border *colour* change with the border always present, a caret glyph or colour shift in the placeholder row, or a marker in the status line. What must not come back is anything that adds or removes a row.

**Why it is still worth doing with the bug fixed.** The `CaretReleased` rule covers the three transitions known today, and its own `DECISIONS.md` entry names the condition under which a fourth appears. A focus indicator is the thing that would let an operator *see* the fourth one in the second it happens, rather than reporting it as "the app stopped responding" a session later.

### Block-level markdown: headings, fenced code, lists, tables, block quotes

**Author.** First operator-supervised live run, 2026-08-04.
**Priority.** P2 — **operator has asked for this explicitly**, to be taken up once the defects found in the core build are cleared.
**Effort.** Large, and it reopens U5's measured gate results.
**Worth it when.** The core-build defect list is empty. That is the operator's stated sequencing, recorded here so the next session does not have to re-derive it.

**What already exists.** The *inline* half shipped on 2026-08-04: `talaria/ui/markdown.py` renders emphasis (`**strong**`, `*emphasis*`) and backtick code spans on assistant and reasoning lines, and the decision beside it in `DECISIONS.md` explains why it stops there. This item is the rest: constructs whose unit is a **block** rather than a line.

**Why this is not simply "more of the same".** [R6](../brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md) puts markdown presentation out of scope for v0.1 while requiring that content is never dropped, so implementing this is a **requirement change**, not a bug fix. The obligation R6 does impose is enforced by `tests/domain/test_projection.py::test_every_transcript_entry_survives_into_the_line_buffer`, and it must keep passing.

**The architectural collision, concretely.** `TranscriptPane` is line-indexed by construction, and four separate mechanisms depend on one line meaning one widget:

- `_lines: tuple[str, ...]` with a stable-prefix diff (`_common_prefix`), so a streaming delta churns one widget rather than the whole block.
- `DEFAULT_MOUNT_CAP = 500` — KTD14's ceiling is stated in *widgets*, and `mounted_count` reads `len(self.children)` precisely so the pane cannot self-report.
- `_top` as an absolute line index, plus `CONDENSED_TEMPLATE`'s "N earlier lines condensed" and the `condensed_count + mounted == total` identity the gate checks.
- `_restore_anchor`, which subtracts the *measured height* of evicted widgets to hold a reader's scroll position.

A fenced code block or a table is one renderable spanning many lines. Every one of those four has to be restated in terms a variable-height widget can satisfy, and `interface_shows_everything` in `talaria/replay/gate.py` — which compares the pane's mounted lines against the projection window position by position — needs a replacement claim before it can be trusted again.

**The streaming problem, which is separate and harder.** A code fence arrives one delta at a time, so for as long as the closing fence is missing the correct rendering is genuinely ambiguous: render eagerly and the screen flickers between "literal text" and "code block" on every delta; wait for the closer and a truncated turn never renders at all. Hermes's own controller carries markdown and diff de-duplication machinery for exactly this class of problem — that machinery is worth reading before designing, not after.

**Suggested shape, not a decision.** Render blocks only on **committed** entries (the projection already publishes `committed_lines`, and committed entries are immutable), leaving the provisional streaming tail as inline-rendered lines. That confines variable-height widgets to the region that never changes, which is also the region the diff already skips. It does not solve the mount-cap accounting; it makes it tractable.

**Do not start by writing widgets.** Start by deciding what replaces "one line, one widget" as the bounded-rendering claim, and get that into an ADR. The rest follows from it.


## P3

### Three small findings from the U8 live acceptance run

**Author.** the U8 live acceptance drive, 2026-08-09 —
[docs/plans/2026-08-09-u8-live-acceptance-results.md](../plans/2026-08-09-u8-live-acceptance-results.md),
observations 2, 3 and 5. All three are cosmetic or documentation-sized; none blocked a leg.
**Priority.** P3 · **Effort.** Small each.

1. **`--session` help text should name the durable id.** The gateway refuses a runtime session id
   with code 4007 ("session not found"); only the durable `stored_session_id` resumes. The help
   string says "attach to an explicit session id" without saying which of the two ids that is.
2. **An empty prompt dock still takes focus.** After an interrupt sweep removed the only card, a
   `tab` stop still reported `caret: prompts` with nothing to operate. The dock should refuse
   focus (or be skipped) when it holds no card.
3. **Teach talaria the gateway's newer event kinds.** `platforms.changed` and
   `agent.terminal.output` render as "! unknown event type" notices — the designed unknown-event
   path, doing its job loudly on every occurrence. Worth teaching (or deliberately ignoring)
   both kinds once their payloads are understood.

### A malformed `status.command` turns the status line off without saying so

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification
**Priority.** P3
**Effort.** Small for a stderr line; medium for an in-interface notice, which is the version worth having.
**Worth it when.** A second configuration setting acquires the same silent-fallback behaviour, or an operator reports that their status line "just stopped working".

**Context.** `status.command = ["sh", "-c", "date"]` in `config.toml` — a TOML array, the obvious guess for an argv — used to raise `AttributeError` out of `shlex.split`. Once U10 put `_build_status_runner` on the bare-`talaria` launch path that became a full traceback and exit 1 with no interface at all: a whole client refusing to start over an optional status line. `parse_command` now returns `None` for any non-string, matching the policy `cli._build_paste_threshold` already documents for its own malformed input.

The client starts, and nothing tells the operator why their status line is missing. That is the right trade against crashing and the wrong end state.

**Suggested framing.** `build_live_app` and `run_replay` both already assemble configuration before the app exists. Collect the settings that fell back, hand them to `TalariaApp` as startup notes, and record them as local transcript entries at mount — the same surface the compatibility check's blocking rows use, which exists and is already read by tests.

### Four guards inside round 5's reachability code have no test that can fail

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P3
**Effort.** Small
**Worth it when.** Alongside the P1 resize-trigger item, which is the same rule applied to the same code.

**Context.** Each of these survives deletion with the entire 651-test suite green, and each has a docstring paragraph arguing it matters. The behaviour is correct in every case — it was verified separately — so these are test gaps, not defects.

- **Both-directions retitling.** Making `mark_unreachable_controls` one-directional (mark, never un-mark) leaves the suite green. Real behaviour verified by hand: at 120x40 both cards read `waiting for you`; at 60x20 the approval reads `answer below — scroll`; back at 120x40 it reads `waiting for you` again.
- **`scrollable_content_region` rather than `region`.** The docstring argues the outer region is wrong because it includes the border and the scrollbar column. Swapping it leaves the suite green.
- **The zero-height clause.** Dropping `and target.region.height > 0` — whose docstring says "a control laid out at zero height is mounted, focusable, and draws nothing" — leaves the suite green.
- **The withdrawn line's absence from an idle turn.** Adding it to the idle branch of `activity_line` leaves the suite green.

There is also a predicate asymmetry worth resolving while here: `reveal_actions` breaks on `target.is_mounted` alone, while `mark_unreachable_controls` additionally requires `target.region.height > 0`. A first card whose control laid out at zero height would consume the reveal and leave every card marked with none revealed. Reasoned from the source, not reproduced.

**Context for why this is P3 and not higher.** This repository's recorded rule is that a guard nothing can exercise is a guard nobody can trust — round 4 deleted its own unreachable latch on exactly that basis. These are the same shape in round 5's new code. They are P3 because the behaviour is right today; the risk is that the next edit breaks one silently.



### Replay can fabricate an age-out the recording never contained

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P3
**Effort.** Small
**Worth it when.** A recorded corpus is used as evidence of what a session showed, rather than as a rendering exercise.

**Context.** `_age_out_approvals` uses the recorded clock in replay, which is what keeps AE2's "replay it twice, get the same state" true. But a corpus where an approval sits outstanding across 400 recorded seconds will withdraw the card and inject a `prompt-expired` transcript entry the gateway never emitted. Determinism holds — the fabricated entry is identical on every replay — so the failure is not a flake; it is that the replayed transcript shows an event that did not happen. Reachability in a real corpus is unproven; the 400-second gap is a synthetic construction.

**Suggested framing.** Either mark locally synthesized entries distinctly in replay, or suppress the age-out entirely in replay on the grounds that a recording's approvals have already had whatever fate they had. The second is simpler and loses nothing, because the reason the age-out exists — a phantom approval disabling the correlation rule for the *next* one — is a live-session concern.

### The age-out sentence speaks about "the gateway" from a pinned constant

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P3
**Effort.** Small
**Worth it when.** A deployment is known to run a non-default `HERMES_APPROVAL_TIMEOUT`, or the gateway starts publishing its configuration.

**Context.** `APPROVAL_AGED_OUT` reads "the gateway's default wait is 5 minutes and it announces no approval timeout, so it has probably stopped waiting". Both facts are read from Hermes at `7f4d15515` (`tools/approval.py:2648-2657`, `tui_gateway/server.py:2981-2998`), not from the connected gateway. The hedge — "probably" — keeps the sentence from being false, but it reads as a statement about *this* deployment when it is a statement about the pinned source. The same gap is what makes `withdrawn_approvals` necessary at all; see the post-withdrawal decision in `DECISIONS.md`.

**Suggested framing.** Say whose default it is ("Hermes's default wait is 5 minutes"), or probe the seam at startup and name the number when it is knowable and its absence when it is not — the pattern AGENTS.md already asks for on gateway capabilities.

### `answer_terminal_read`'s unavailable-projection path leaves the prompt registered

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P3 — **latent**; the code is correct today and the invariant it depends on is not enforced.
**Effort.** Small
**Worth it when.** Before anything else can make `transcript_view_for_read()` return `None`.

**Context.** `talaria/ui/app.py:1201-1215` discards the request id from `_answering` and writes a note, but leaves the prompt registered. `_answer_unattended_prompts`'s docstring now depends on the opposite: "this dispatches on sight, so the bound is that every outcome settles the prompt". Proven **unreachable as a loop today** — the only trigger is `transcript_view_for_read()` returning `None`, which happens only during teardown or before the first render, and both of those stop the render tick that would re-dispatch.

**Frame it as making the code honour the invariant its docstring depends on**, not as fixing a loop. The one-line change is to settle the prompt on that path as every other outcome does; the reason it needs a moment's thought is deciding whether a read that could not be answered should leave any trace in the registry at all.

### ~~`focus_session` disarms the in-flight bookkeeping the prompt registry depends on~~ — CLOSED 2026-08-17

**Closed.** Both halves are repaired, and the function has had production callers since v0.2 — every landing reaches it through `land_session`, which the `/sessions` switcher, `--resume` and startup all go through. What the current contract does (`talaria/domain/state.py`, `focus_session`): `prompts` and `flushed_prompt_ids` **survive** a switch, so neither an outstanding control is orphaned nor a closed one resurrected; `answering` is emptied, which is safe only because a switch is refused outright while an answer is on the wire (`switch_refusal`), so the tuple is already empty by the time the clear runs — the invariant is stated in the code beside the clear; and `approvals_seen` survives too, which is what keeps a synthesized approval id from colliding with the tombstone of an earlier visit to the same session. Pinned by the 53 tests of `tests/domain/test_prompt_registry.py`, and kept green under the v0.4 registry by `tests/ui/test_focus_churn.py`, which drives three sessions with outstanding prompts through rapid switches and answers each one after its session is refocused. The paragraphs below describe the pre-repair tree.

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round
**Priority.** P3 — **latent**, and the priority is low only because of that. Fix it before anything calls the function.
**Effort.** Small
**Worth it when.** Before the first production caller. `grep -rn "focus_session" talaria/` finds exactly two hits — the definition and the `__all__` entry — so there is no caller today.

**Context.** `focus_session` (`talaria/domain/state.py:237-260`) clears `prompts`, `answering` and `flushed_prompt_ids`. The last two are precisely what the third round added to make the in-flight answer window safe:

- clearing `answering` re-enters the shipped uncorrelated-approval defect through another door — `outstanding_approvals` searches `answering` so an approval whose answer is travelling still counts, and a focus switch inside that round trip makes it stop counting;
- clearing `flushed_prompt_ids` removes the latch that stops `restore_prompt` resurrecting a prompt the gateway has already closed, and `prompt_view` (`talaria/domain/projection.py:321`) applies no session filter, so a resurrected foreign-session prompt renders.

Reproduced at the domain level.

**Its own docstring is currently false**, and that is worth fixing whichever way this goes: it says "the caller here is reconnect, not a UI control", describing a caller that does not exist. Reconnect does not call it today.

**Decide, don't patch.** Either the reconnect path should call it (in which case the in-flight sets need a considered policy — probably settle rather than drop), or the function should not survive to v0.1 with no caller.

### The terminal-read bridge serves un-defanged bytes, and the screen it claims to describe is defanged

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round — found while fixing the bidi/zero-width defect, queued rather than fixed.
**Priority.** P3
**Effort.** Small, once the boundary is decided.
**Worth it when.** An agent is observed acting on terminal-read output rather than treating it as prose, or the defang set changes again.

**Context.** `defang` now replaces bidirectional-override and zero-width characters as well as C0 controls, and it runs at *render* time — `literal_text` is the one door onto a widget. So the card, the command panel and the transcript pane are all clean, and the stored `TranscriptEntry.text` deliberately keeps the gateway's bytes as the audit record.

`terminal_read` (`talaria/domain/projection.py:446`) serves `TranscriptView.lines`, which are built straight from those stored entries. So the buffer the agent receives is **not** the buffer the operator sees whenever a defanged character is present: the agent gets `‮`, the screen shows `�`. The bridge's contract is "what is on screen".

**Not fixed now** because the fix has to choose a boundary and the choice is not obvious. Defanging in `transcript_view` would put the terminal layer's rule into `talaria.domain`, which ADR-0002 forbids; defanging in `answer_terminal_read` puts it on the UI side of the seam but then the projection's own tests describe a buffer nobody is served. Neither is a five-line change, and the divergence is currently cosmetic — an LLM reading JSON does not perform bidi reordering.

### Desktop-like configuration views

**Author.** Project bootstrap
**Priority.** P3
**Effort.** Large
**Worth it when.** The core session workflow is reliable and the gateway configuration methods are capability-described.

### Status runner: URL path is forwarded to the child, and a few outcomes are misreported

**Priority.** P3 — same review, 2026-08-03.

- `_strip_query` now drops the query, fragment and userinfo, but keeps the path. An operator-supplied `TALARIA_GATEWAY_URL` carrying a ticket in its path (`/attach/TICKET-ABC`) still reaches the status child. Talaria's own URLs put the credential in the query and the path is meaningful (`/api/ws`), so stripping the path outright is not obviously right — decide deliberately rather than by default.
- A nonexistent `launch_cwd` raises `FileNotFoundError` and is reported as `missing_executable`, telling the operator "command not found" about a command that is present.
- The timeout path sends SIGKILL with no preceding SIGTERM, so a child never gets a chance to flush or clean up temporary files.
- `is_suspicious_key` anchors its API-key pattern to the whole variable name, so `ANTHROPIC_API_KEY`, `AWS_ACCESS_KEY_ID`, `GITHUB_PAT` and similar are not "suspicious" to it. They are denied by default anyway, but `contract.py`'s claim that the credential-name deny outranks the operator allowlist is false for them — an operator who allowlists one gets it forwarded.


### Replay determinism exercises two speeds, not the speed range AE11 asks for

**Priority.** P3 — found by external review of the gate, 2026-08-03; description corrected, coverage not widened.

`run_gate` proves AE11 ("identical domain state at any speed") with three replays: 64x, 64x with a pause and resume halfway, and unbounded. That is **two** distinct speeds, since the paused run is also 64x. `MIN_SPEED` is never replayed, so the slow end of the range is unmeasured.

The check's published `description` claimed "1x-with-pause, 64x and unbounded", which overstated it — a treatment named in a check description but never run is the same class of defect as a threshold quietly loosened, and it was corrected in place. The results doc described the replays accurately; only the code's own string was wrong.

Not widened because a genuinely slow replay is wall-clock expensive: `speed` multiplies the corpus's recorded cadence, so replaying 53,516 frames at 1x takes as long as the original session did. A cheap partial improvement is a third speed between 1x and 64x over a truncated corpus, which would give the fit three points instead of two without a real-time replay.

**Revisit when.** The determinism check is next touched, or a timing-dependent reducer bug escapes to a user — that would make the unmeasured slow end the first place to look.

## Maybe

### ~~Package Talaria as an independently installable distribution~~ — CLOSED 2026-08-08

**Author.** Project bootstrap
**Priority.** Maybe
**Effort.** Medium
**Worth it when.** The client can launch against both local and remote Hermes instances with a stable compatibility story.

**Closed by v0.1.0**, which builds a wheel and a source distribution in continuous integration and attaches both to the GitHub Release. `uv tool install git+https://github.com/infiquetra/talaria@v0.1.0` was verified end to end into a clean prefix, run under a scrubbed environment, and reports its version.

**Its trigger was not met — the release went out anyway, with the gap stated instead.** Nobody has driven the interface against a remote Hermes, and the compatibility story is checked at the top level of each response only. Those are written into the release notes and the v0.1 verdict rather than treated as satisfied. What this entry actually asked for was a *distribution*, and that exists; what remains is publishing it to an index, which is the item below and a different question.

### Ask for the `talaria` name on the Python Package Index

**Author.** the v0.1.0 release, step S5 — deferred 2026-08-08
**Priority.** Maybe
**Effort.** Small to send; slow to resolve
**Worth it when.** The name is settled — no rename under consideration — **and** there is an actual intent to publish. Either one alone is not the trigger.

The research is done and both drafts are written in [docs/plans/2026-08-08-pypi-name-request.md](../plans/2026-08-08-pypi-name-request.md); neither was sent. The name belongs to an unrelated content management system, last uploaded **2010-06-19**, whose own metadata carries `Development Status :: 7 - Inactive` — but whose author is active and reachable, which is why the first step is a letter rather than a PEP 541 form. Reasoning and the reopen condition are in [DECISIONS.md](DECISIONS.md#the-talaria-name-on-the-python-package-index-is-not-asked-for-yet-because-the-project-cannot-yet-promise-it-will-keep-it).

**Two things a repository cannot supply:** the letter has to be sent by a person under their own name, and the PEP 541 form validates a PyPI username that is not derivable from here.
