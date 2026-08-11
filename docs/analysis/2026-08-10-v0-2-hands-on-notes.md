# v0.2 hands-on notes — 2026-08-10

Live driving notes taken while walking Talaria v0.2 in the Talaria Testing tab, on
`main` at `d925891` (version 0.2.0) against a running Hermes dashboard.

Raw capture. Nothing here is a verdict yet — the point is to collect what the
operator actually saw, in their own words, and sort it afterwards. Uncommitted
until reviewed.

## Sorted for v0.3

Written after the walk finished, from the nineteen notes below. Nothing here is
scheduled — this is the candidate list, with defects kept apart from design
questions so the two are not argued about together.

### What passed, driven by hand for the first time

Every one of these had only ever been measured against a bare pseudo-terminal
before today.

- **Block markdown** — headings, bullets, ordered lists, syntax-highlighted fences
  and bordered tables, on two separate prompts (notes 4, 17). The operator's verdict
  on tables and code was "fantastic".
- **The bounded fallback** — a 600-line entry demoted out of block rendering into
  clipped rows behind a banner, with a condensed marker above and the 499/101
  retention split confirmed from the screen (notes 5, 9).
- **The session switcher** — filter, switch away, switch back, against nine real
  sessions (note 10).
- **`--resume` history rendering** — against a real transcript far richer than
  anything this walk produced synthetically (note 16).
- **Record and replay** — 854 frames captured and replayed with `not connected`
  visible in the status line, `F8` pausing and `F9` stepping the speed (notes 17,
  18).

### Defects — Talaria's own

1. **The approval card has no keyboard path on macOS** (notes 8, 9). Cards never
   auto-focus unless input-backed (`prompts.py:1171`) and are reachable only through
   the `F1` jump, which never arrives. The card meanwhile advertises
   `enter select · esc decline`; both were tested and neither works. This is the
   answerability spine — v0.2's first leg — unusable by keyboard on the only
   platform anyone drives.
2. **Mouse selection lands several rows off** (notes 2, 13), which also explains why
   the terminal's own select-and-copy never reaches through. Suspect: the
   mixed-height widget layout introduced by this release. Undiagnosed.
3. **`platforms.changed` floods the transcript** (notes 15, 17). 26 rows in one
   turn. One-line fix: add it to `_OBSERVED_ON_A_LIVE_GATEWAY` (`decode.py:110`),
   whose comment already names this session's job. Consider coalescing repeats.
4. **The fallback banner reports the retained count as "clipped"** (note 5), and the
   count *falls* as more is hidden — proven live when it went 499 → 494. Suggested
   wording: "showing the last 499 of 600 lines".
5. **The condensed marker and the banner are measured against different scopes**
   (note 5) — pane-wide versus entry-scoped — and neither answers "where does this
   entry start and how long is it". Fix as one change.
6. **The shipped release notes describe `F4` by half** (note 18), in both
   `docs/releases/v0.2.0.md:21` and `CHANGELOG.md:26`. It interrupts the in-flight
   turn *and then* sweeps. The omitted half is the destructive one.
7. **`F2` is eaten by macOS Mission Control** (note 19).

### Undiagnosed — needs evidence before it can be called anything

- **Duplicated rendered content**, sighted twice (notes 6, 14). One recorded
  reproduction attempt **failed** — deltas matched `message.complete`, headings
  appeared once. The next attempt needs a turn where the model speaks, calls a tool,
  then continues (note 17).
- **What specifically eats `F1`** (note 8) — macOS Help, brightness, or the host
  terminal. The binding is proven identical to keys that work, so this decides
  whether the fix is one alternate binding or something larger.
- **Whether `F5` is alive** (note 19). Pressed at the bottom of a paused replay,
  where its action is a legitimate no-op. Scroll up and press it again.
- **`F4` and `F10`** — never pressed.

### Design questions — not defects, and not to be argued as if they were

- **Should focus always return to the composer?** (note 4) The operator's proposal.
  Tension: `F1` and `F4` move focus deliberately, so an unconditional snap-back
  breaks the answerability spine. A narrower rule may work.
- **Should an answer carry notes or caveats?** (note 9) The operator's own open
  question of whether this belongs on an approval or only on an ask. Check first
  whether `approval.respond` can carry free text at all.
- **`--resume` resumes the gateway's most recent session, not yours** (note 16).
  Correct per `session.most_recent`, silently surprising on a gateway shared with
  automation. Cheapest fix is naming the resumed session on arrival.
- **The composer needs the usual conventions** (note 12): up-arrow history, and a
  filterable slash-command palette on `/`.
- **The approval card is cramped** (note 9); the transcript is visually busy and
  hard to differentiate (note 11) — the latter only visible against real transcripts.

### Not Talaria — no action here

- The 256k context window on `muse-spark-1.2-contributor` (note 1) — the gateway's
  model catalogue.
- Approvals not firing (notes 6, 7) — `approvals.mode: off` on the operator's Hermes
  profile. Flipped to `manual` for the test and **restored to `off`**, verified.
- An expired approval defaults to **deny** (note 15 follow-up) — the safe default.

### The theme worth carrying into v0.3

Four separate findings are one problem: **Talaria does not confirm what it just
did.** A status row nobody can interpret, a number without its scope, a card
advertising keys that do nothing, a keypress indistinguishable from a dead one. That
is a tighter release theme than "readability", and it absorbs most of note 11 as a
consequence rather than a separate project.

### The methodological finding

Every defect above survived a 24-of-24 replay gate, six external review rounds and
about 1,700 tests. None of them could have been caught by any of it, because all of
them live in the seam the gate explicitly does not cover: a **real terminal, on a
real desktop, driven by a real person**. Both v0.1 and v0.2 record "no run on either
platform has used a real terminal emulator" as a known limitation. Today is what
that limitation was worth — roughly two hours of hands-on driving produced seven
defects and four design questions that no amount of further automated verification
would have found.

## Session setup

- Working copy fast-forwarded from `cccf6c3` to `d925891`; `uv run talaria --version`
  reports `talaria 0.2.0`.
- The `talaria` on PATH (`~/.local/bin/talaria`) is a stale frozen scaffold — it does
  not recognise `--version`. Everything below was driven with `uv run talaria` from
  the repository.
- Hermes dashboard answering `200` on `127.0.0.1:8765`; credential file present at
  mode `0600`.

## Notes

<!-- Each entry: what was being exercised, then the operator's note verbatim. -->

### Note 1 — model switch to `muse-spark-1.2-contributor` reports a 256k context window

Exercising: step 1, `/models` picker.

> when I change the model to muse-spark-1.2-contributor, its only 256k context
> window. Should be 1M. Probably not a talaria bug, but its something to note.

Output as seen in the transcript:

```
— ⚠️  Context file AGENTS.md TRUNCATED: 76904 chars exceeds limit of 61440 — trim the file, pin a larger context_file_max_chars, or use a larger-context model!
— /model: ✓ Model switched: muse-spark-1.2-contributor
    Provider: Meta
    Context: 256,000 tokens
    Max output: 131,072 tokens
    Capabilities: reasoning, tools, vision, PDF, audio, structured output
    (session only — add --global to persist)
```

Operator's own read: probably not a Talaria bug.

### Note 2 — terminal select-to-copy does not reach through the interface

Exercising: step 1, general interaction.

> I don't seem to be able to copy/past text. Doesn't seem to allow the terminal
> being used to "come through". For example this is a herdr session, I should be
> able to to just select and copy. That doesn't work. ctrl+c does work though

Narrowed by the operator immediately after: **the failure is confined to the Talaria
pane.** Select-and-copy works normally in other herdr panes in the same session, so
this is Talaria's own terminal behaviour, not a herdr limitation.

### Note 3 — the provider stage showed only authenticated providers

Exercising: step 1, `/models` provider stage.

> picker only shows authenticated models. You said it would show all

Screenshot shows six provider rows, none carrying an `[unauthenticated]` marker:

```
models — choose a provider

  Mixture of Agents (moa)  (4 models)
  Anthropic (anthropic)  (12 models)
  OpenAI Codex (openai-codex)  (10 models) · profile default
  Google AI Studio (gemini)  (14 models)
  Ollama Cloud (ollama-cloud)  (21 models)
 *meta (meta)  (3 models) · switched here this session
```

Footer: `↑↓ move · →/enter select · ←/esc back · type to filter · ctrl+q quit`.

### Note 4 — block markdown renders correctly; the `caret:` line is the one complaint

Exercising: step 2a, the markdown feature-test prompt (sent into the pane over herdr).

> looks pretty good, only one note is regarding the "caret" text at the bottom

Everything the prompt asked for rendered as real blocks, confirmed from the
screenshot: a styled `Heading Level 2` and `Heading Level 3`, a bulleted list with
real bullet glyphs, a numbered list with coloured ordinals, a Python fence with
syntax highlighting (keywords, strings, f-string interpolation all distinct), and a
four-column table with drawn borders and a highlighted header row. This is the v0.2
headline claim working in a real terminal for the first time.

The complaint is the bottom row, which reads:

```
caret: transcript
```

Operator's follow-up when asked which of three objections it was:

> its the wording I think and more importantly... I as the user had no idea why it
> was there or even what caret corresponded to. to be frank, I think it might just
> be prudent for the focus to always go back to the input box. note that as possible
> improvement

### Note 5 — the fallback banner fired, and its wording misreports what happened

Exercising: step 2b, `print exactly 600 lines, each just its line number. nothing
else.` (sent into the pane over herdr).

> ok for 2b got this:

Banner as rendered, below a run of clipped line rows ending at `600`:

```
── entry too large to render as markdown (499 lines clipped at the viewport edge, full content still readable by the agent) ──
```

The mechanism worked: the entry demoted out of block rendering into bounded,
clipped line rows behind a one-row banner, exactly as the release notes claim.

### Note 6 — step 3's trigger failed: `echo` never raised an approval

Exercising: step 3, `run the shell command: echo hello-from-talaria` (sent over
herdr), intended to raise an approval prompt for `F1` to jump to.

> take a look, didn't ask for approval

Transcript as read back from the pane:

```
› run the shell command: echo hello-from-talaria
 You got it — running that echo now.

⏺ terminal echo hello-from-talaria
⏺ terminal ✓
 You got it — running that echo now.

 Done — exit 0:

   hello-from-talaria
```

The command ran straight through. No prompt, so nothing for `F1` to find.

### Note 7 — the approval never fires because this profile has approvals switched off

Exercising: step 3 retry, `run the shell command: chmod 777 /tmp/talaria-demo`.

> didn't ask either

The command ran unprompted, batched with others (`terminal chmod 777
/tmp/talaria-demo + 2 commands`), failed harmlessly with
`chmod: cannot access '/tmp/talaria-demo': No such file or directory`, and the model
offered to `touch` the path first.

### Note 8 — `F1` is intercepted by macOS before Talaria ever sees it

Exercising: step 3, pressing `F1` to jump to the newest unanswered prompt.

> I tried F1, and it actually appears to be a hotkey for macos usage. So we might
> want to do something different here that use a key
>
> This is what claude code does: [screenshot of a clickable `Jump to bottom (click) ↓`
> affordance]
>
> note the jump to bottom. That might be the better way to go about this. That said,
> this is not all that important right now. I can always just scroll. We have bigger
> fish to fry in v0.3 than a focus key. That said, I would like to see the "ask"
> dialogue, that is more important than focus jump.

Operator's own priority call, recorded as given: the focus key is **not** important
for v0.3; seeing the approval dialogue **is**.

### Note 9 — the approval card, seen for the first time outside a test

Exercising: step 3, after setting `approvals.mode` to `manual` on the
operator's Hermes profile. The running dashboard picked the change up live — **no
restart was needed**, and Talaria's credential stayed valid.

The card as rendered:

```
waiting for you — approval: world/other-writable permissions
╭─ waiting for you ───────────────────────────────────────────╮
│ approval: world/other-writable permissions                  │
│  chmod 777 /tmp/talaria-demo                                │
│   once    session   always    deny                          │
│ enter select · esc decline                                  │
╰─────────────────────────────────────────────────────────────╯
```

Operator's notes:

> 1. its pretty cramped and needs some UX consideration.
> 2. I can only select with the mouse, the cursor is not focused inside the approval
>    card
> 3. in onther systems someting I think that is always useful is to have a "notes"
>    option. Such that I can add notes to the choice, as I might agree with the
>    choice but with caveats. Though we would need to determine if that is
>    appropriate for approval, or just "ask"

### Note 10 — step 4 passed: the session switcher works

Exercising: step 4, `/sessions` — filter, switch away, switch back.

> 1. filter seems to be working
> 2. worked fine
> 3. worked fine
> in general the session switching worked pretty well actually.

All three sub-steps passed against a real gateway with nine real sessions. The
picker is keyboard-driven — footer `↑↓ move · →/enter select · ←/esc back · type to
filter · ctrl+q quit` — which is the direct contrast with the approval card in note
9: **dialogs are reachable by keyboard, prompt cards are not.**

### Note 11 — the transcript is visually busy and hard to differentiate

> the overal test box is a bit busy, multiple colors, etc... its sort of hard to
> follow, needs more spacing, formatting etc... take notes. I am sure we will have a
> whole release on readabilty and what not. that said some of it is fantastic. Tables
> are great, code is very readable. some of the coloring is working well. Just some
> parts are hard to differentiate. I only noticed when pulling up other sessions with
> more real content.

Worth recording precisely because of the last sentence: this only became visible
against **real transcripts from other sessions**, not against the synthetic prompts
this walk was built from. Verdict is mixed rather than negative — tables and code
blocks are called out as working well; the problem is separating one region from
another.

### Note 12 — the composer needs the conventions every other terminal interface has

> the input box will need some work, helpful things like: up arrow to go to previous
> commands, type "/" allows scrolling/filtering on all possible slash commands like
> most other TUI's like this.

Two specific asks: history recall on up-arrow, and a filterable slash-command
palette triggered by typing `/`.

### Note 13 — mouse selection lands on the wrong line

> mouse is a bit off look at this image
>
> I double clicked on this text: "write a binary search in: python, c and rust.
> compare and contrast the implementatins"
>
> note where the highlight is... is a number lines up from where I clicked.

The screenshot shows the highlight landing on `Running that chmod for you — one
moment.`, several rows **above** the line actually clicked.

### Note 14 — rendered code repeats after the comparison table

> the output from that adhoc test I asked, was pretty readable. Table was great.
> there is some work there, on the other parts... but worse is the code started to
> be duplicated after the table

The entry rendered Python, C and Rust implementations, then a comparison table, then
a `Takeaway` paragraph — and then began again with a `Python` heading and the same
Python code block, followed by `C`.

### Note 15 — `unknown event type: platforms.changed`, five times in red

> Also this is happening in sessions now.. maybe because of switching session? maybe
> not

```
! unknown event type: platforms.changed
! unknown event type: platforms.changed
! unknown event type: platforms.changed
! unknown event type: platforms.changed
! unknown event type: platforms.changed
```

### Note 16 — `--resume` rendered history correctly, and resumed the wrong session

Exercising: step 5. Talaria was quit with `ctrl+q` and relaunched as
`uv run talaria --resume`.

The history rendering half of the claim **worked**, and worked against far richer
content than anything this walk had produced synthetically: nested bullet lists,
tool-call rows, indented sub-bullets and a closing verdict paragraph all rendered as
proper blocks on the way back in.

The session it resumed was **not** the one the operator had been driving. It picked
up an unrelated automation session belonging to a different repository's continuous
integration work. (Its content is deliberately not reproduced here — this is a
public repository and that session carries private operational detail.)

### Note 17 — the recorded turn settles `platforms.changed` and fails to reproduce the duplication

Exercising: step 6. Talaria relaunched as `uv run talaria --record`, then asked to
run `uname -a` and write binary search in three languages with a comparison table —
a prompt built to reproduce **both** duplication shapes in one turn. Frame log:
`~/.talaria/recordings/2026-08-11T00-18-11-356Z.jsonl`, 854 frames.

Inbound event counts, read straight off the log:

| Event type | Count on the wire | Talaria's treatment |
| --- | --- | --- |
| `message.delta` | 580 | streamed |
| `sessions.changed` | 204 | known — silent |
| `platforms.changed` | 26 | **unknown — one red row each** |
| `reasoning.delta` | 9 | streamed |
| `thinking.delta` | 6 | streamed |

The comparison table rendered well again; the operator's praise for table rendering
holds against a second, wider table.

## Follow-ups

<!-- Anything that looks like a defect, a surprise, or a queue candidate. -->

- **Note 1 — where the 256k figure comes from.** The block quoting `Provider: Meta`
  and `Context: 256,000 tokens` is Hermes's own `/model` confirmation, not text
  Talaria composes, so the number is the gateway's model catalogue reporting it.
  Worth confirming against the provider's published figure before treating it as a
  defect anywhere. Not yet verified from a current source.
- **Note 2 — mechanism to check.** Talaria does not disable mouse tracking anywhere
  (`grep -rn 'mouse' talaria/` finds only scroll handling and prose), so it inherits
  Textual's default, which turns mouse reporting **on**. With reporting on, the
  terminal forwards mouse events to the application instead of performing its own
  select-to-copy. That is a hypothesis matching the symptom, not a confirmed
  diagnosis. Open questions: whether the host terminal offers a
  bypass-mouse-reporting modifier, and whether Talaria should expose a copy path of
  its own rather than relying on the terminal's.
- **Note 3 — Talaria hides nothing; the gateway sent six.** Checked in the source:
  `talaria/domain/models_catalog.py:335` reads `authenticated` straight off each
  gateway row, and the module's own header records that `authenticated: false` is a
  normal state carried through to the caller rather than filtered. The picker then
  renders such a row with an `[unauthenticated]` suffix
  (`talaria/ui/picker.py:97`), highlightable but refusing selection with a reason
  (`picker.py:398`, `picker.py:434`). So the six rows on screen are exactly what
  Hermes reported — Talaria dropped nothing. Open question for Hermes, not Talaria:
  does the gateway's model catalogue omit providers that are not authenticated, or
  are all six genuinely authenticated on this profile? Untested — needs the raw
  catalogue payload to settle.
- **Note 3 — arrow-key navigation confirmed live.** The footer reads
  `→/enter select · ←/esc back`, which is the v0.1 requirement that right selects
  like Enter and left goes back like Escape. Working in the real terminal.
- **Note 4 — the `caret:` row is deliberate, but its wording is developer-facing.**
  Checked in the source: `talaria/ui/status_region.py` reserves a dedicated
  fixed-height one-row slot for it, and `set_caret` writes `f"caret: {location}"`
  only when the caret is somewhere other than the composer, clearing the slot
  entirely when the composer holds it (`status_region.py:101`, and the docstring at
  `:88`). The requirement it serves is R5/KTD5 — name where the caret went. So
  `caret: transcript` on screen means focus was genuinely in the transcript at that
  moment, and the row is doing its job.
  What is worth challenging is the *wording*, not the mechanism: "caret" is a
  developer's word for it, and the bare `key: value` shape reads like debug output
  rather than an operator-facing status line. Candidate for a v0.3 wording pass.
  Answered: it is the wording, plus a deeper problem the wording only exposes — the
  operator did not know why the row was there or what "caret" referred to. A status
  row nobody can interpret is not a status row.
- **Note 4 — proposed improvement: focus always returns to the input box.** The
  operator's own suggestion, recorded as a v0.3 candidate rather than a decision.
  It would make the `caret:` slot unnecessary by construction, which is the cleanest
  possible fix to a row nobody can read.
  The tension to resolve before adopting it: focus moves away from the composer on
  purpose in v0.2. `F1` jumps to the newest unanswered prompt and `F4` sweeps the
  answerable set, and both work by putting focus *on* a prompt so the next keystroke
  answers it. An unconditional snap-back would break the answerability spine that is
  the other half of this release. A narrower version — return focus to the composer
  whenever nothing is waiting to be answered, keep it wherever it was put when
  something is — may get the ergonomics without the regression. Untested; needs a
  look at what actually moves focus today.
- **Note 5 — the banner's number is the count of rows KEPT, not the count dropped.**
  Traced in the source. `transcript.py:1206` builds the banner as
  `_fallback_banner(len(widgets))`, where `widgets` is the list built *after* the
  cap slice at `:1202`, so the number is the retained row count. `_banner_text`
  (`:619`) drops it into `FALLBACK_BANNER_TEMPLATE` (`:131`), whose text is
  `"{lines} lines clipped at the viewport edge"`. The template's author meant
  *horizontal* clipping — each retained row is rendered `no_wrap` and cut at the
  right edge rather than wrapping, which is also why the rows carry
  `transcript--nowrap`. The sentence is technically true under that reading.
  It does not survive contact with an operator. "499 lines clipped" reads as "499
  lines were removed", and the arithmetic makes the misreading worse rather than
  better: the cap is `DEFAULT_MOUNT_CAP = 500` (`:118`), a fallback unit retains
  `max_rows - 1` rows to leave room for its banner (`:1200`), so a 600-line entry
  keeps 499 and leaves 101 rows unmounted. Both numbers are plausible-looking, and
  the banner shows the one that is not the loss.

  | Figure | Value | Shown in the banner |
  | --- | --- | --- |
  | Lines the model produced | 600 | no |
  | Rows retained, each horizontally clipped | 499 | yes — as "clipped" |
  | Rows not mounted at all | 101 | no |

  Two fixes worth weighing, neither decided: reword so the count is unambiguous
  ("showing the last 499 of 600 lines"), or report the dropped count instead. The
  first is strictly more informative and costs one format string.
  **Resolved by scrolling to the top — the earlier concern was unfounded.** A marker
  row is there, reading
  `── 317 earlier lines condensed (still readable by the agent)`, with the first
  visible content row being `102`. Nothing is silently lost, and the retention
  arithmetic is confirmed from the screen rather than inferred: first visible row
  `102`, last row `600`, giving exactly the 499 retained rows predicted above.
- **Note 5 — the two numbers on screen are measured against different scopes.** This
  is the sharper version of the wording complaint. An operator looking at one
  600-line entry now sees `317` at the top and `499` at the bottom, and neither is
  600, neither is 101, and the two are not commensurable:
  `condensed_count` is `self._top + self._tail_top` (`transcript.py:730`) — folded
  rows ahead of the visible window across the **whole pane**, spanning every earlier
  entry and the tail's folded head, which is what makes the invariant
  `pane.rendered_lines == view.lines[pane.condensed_count:]` hold (`:746`). The
  banner's 499, by contrast, is scoped to **this entry's** retained rows.

  | On screen | Value | Scope | Reads as |
  | --- | --- | --- | --- |
  | Top marker | 317 | whole pane, all earlier entries | "this entry lost 317 lines" |
  | Bottom banner | 499 | this entry's retained rows | "this entry lost 499 lines" |

  Both are correct against their own definitions and both invite the same wrong
  reading. Neither surfaces the figure the operator actually wants, which is where
  this entry starts and how long it is. Worth fixing together rather than
  separately.
- **Note 6 — my prompt was the wrong trigger; Hermes was behaving correctly.** Not a
  Talaria defect and not a Hermes defect. Checked with `hermes approvals test`,
  which dry-runs the verdict without executing: `echo hi` returns `allow (exit 0)`,
  so no approval was ever going to be raised. Verdicts sampled while hunting for a
  usable trigger — `curl https://example.com`, `git push` and `sudo -n true` all
  return `allow`; `rm -rf /tmp/x` returns `ask-approval` on rule "delete in root
  path"; `chmod 777 /tmp/talaria-demo` also returns `ask-approval` and is harmless,
  so that is the trigger to use instead.
  Talaria's five prompt kinds, for picking future triggers
  (`talaria/domain/models.py:102`): `approval`, `clarify`, `secret`, `sudo`,
  `terminal_read`, mapped from the gateway events `approval.request`,
  `clarify.request`, `secret.request`, `sudo.request` and `terminal.read.request`
  (`talaria/domain/state.py:69`).
- **Note 6 — the banner count fell from 499 to 494, which confirms note 5 from the
  running system.** The same 600-line entry's banner now reads `494 lines clipped`
  where it read `499` before the echo turn arrived. If the number meant "lines
  removed" it could only ever rise as more got hidden. It fell, because it is the
  retained-row count and five more rows folded away as new content pushed the window
  down. This is stronger evidence than the source read in note 5: the misleading
  reading is not hypothetical, it inverts under normal use. It also confirms the
  partial-fold banner refresh is working as built.
- **Note 6 — an assistant line appears twice around the tool call.** "You got it —
  running that echo now." is rendered once before the `⏺ terminal` rows and again
  after them. Undiagnosed: this could be Hermes re-emitting the assistant message
  after the tool result, or Talaria committing the same entry twice across the tool
  boundary. Needs a frame log to settle — `talaria --record` on a session that runs
  a tool would capture it. Do not treat as a Talaria defect until the frames say so.
- **Note 7 — root cause found, and it is neither Talaria nor a bug.** The gateway
  Talaria dials runs as `hermes -p <profile> dashboard --host 127.0.0.1 --port
  8765 --no-open`, and `hermes -p <profile> config get approvals.mode` resolves
  to **`off`**. The global `~/.hermes/config.yaml` says `approvals: mode: manual`
  (line 517), so the profile overrides the global setting and this session was never
  going to prompt for anything. `hermes approvals test` reads the global policy,
  which is why its `ask-approval` verdict disagreed with what actually happened —
  the test tool and the running session were answering different questions.
  Consequence for this walk: **Talaria's approval card cannot be exercised against
  this gateway as configured.** Nothing about the prompt machinery has been driven by
  hand yet — not the card, not the hint line, not `Escape` sending an explicit
  `deny`. That whole leg of v0.2 remains unverified in a real terminal.
  To fix, one of: set `approvals.mode` to `manual` on that profile
  and restart the dashboard; or run a second gateway on another port under a profile
  that already prompts, and point Talaria at that. Unknown, and worth establishing
  before choosing: whether the running dashboard re-reads `approvals.mode` per
  session or only at start-up. A dashboard restart invalidates Talaria's stored
  credential, which `talaria refresh-credential` repairs.
- **Note 8 — CORRECTED by step 6: the collision is `F1` specifically, not function
  keys generally.** The original claim written here — that every Talaria hotkey is
  equally exposed to macOS interception — was **falsified** during the replay step.
  `F8` paused the replay and `F9` stepped the speed down twice (1x → 0.5x → 0.25x),
  both reaching the application without trouble. So function keys as a class are not
  being swallowed.
  This is not a Talaria binding fault either. `app.py:770` binds `F1` to
  `jump_to_prompt` with `priority=True`, the identical form used by the `F8`, `F9`
  and `F10` bindings on the three lines directly below it (`:771`–`:773`) that
  demonstrably work. The binding is registered exactly like the ones that fire.
  So something eats `F1` alone — macOS Help, a brightness mapping, or the host
  terminal claiming it. Which of those is untested and is the next thing to
  establish, because the fix depends on it: if `F1` is unreachable on this hardware
  no matter what, the answer is simply a second binding on a key that survives, not
  a redesign of the hotkey surface.
  Still untested: `F2` (fold sub-agents), `F4` (interrupt and sweep) and `F5`
  (re-follow). Each is one keypress to check and would map the surviving surface
  exactly.
  The measured gate could not have caught any of this: every gate run was against a
  bare pseudo-terminal with no window manager in the way.
  The operator's suggested direction is Claude Code's affordance: a visible, clickable
  `Jump to bottom (click) ↓` control rather than an undiscoverable hotkey. Worth
  noting that this solves two separate problems at once — the interception, and the
  discoverability complaint already logged against the `caret:` row in note 4.
  Deprioritized by the operator for v0.3; recorded, not scheduled.
- **Note 9.2 — the mouse-only card is a confirmed design gap, not a glitch. This is
  the most serious finding of the session.** Traced to one line.
  `talaria/ui/prompts.py:1171` reads
  `if focus_new and isinstance(card.action_widget, Input): card.focus_answer()`.
  Only an **input-backed** card gets mount-time auto-focus. An approval card is
  **button-backed**, so it never auto-focuses — and the comment directly above says
  so in as many words: "every other kind is reachable **exclusively through the
  jump**" (`:1170`).
  Chain the pieces together and the consequence is severe on this platform:

  1. An approval card never takes focus when it mounts, by design.
  2. It is reachable by keyboard **only** through the `F1` jump.
  3. `F1` is intercepted by macOS and never reaches Talaria (note 8).
  4. Therefore, on macOS, an approval card is **keyboard-unreachable** — mouse only,
     exactly as the operator found.

  The card's own hint line makes this worse rather than better. It advertises
  `enter select · esc decline`, but with focus sitting in the composer, Enter sends
  a message and Escape is not routed to `PromptCard.action_decline`
  (`prompts.py:886`), which is a card action. The interface is printing a promise it
  cannot keep in the state the operator is actually in.
  **Tested live, and it is the worse answer:**

  > escape does not work

  So there is no partial keyboard path. With focus in the composer, neither of the
  two keys the card advertises does anything: Enter does not select, Escape does not
  decline. The approval card on macOS can be answered **only** with the mouse.
  This means the answerability spine — the headline of v0.2's first leg — has no
  working keyboard path on the only platform anyone drives Talaria on. The measured
  gate could not have caught it: every run was against a bare pseudo-terminal with
  no window manager to intercept anything.
- **Note 9.1 — cramped card layout.** Recorded as a UX item without a diagnosis. The
  four choices (`once`, `session`, `always`, `deny`) sit on one row directly under
  the command with no separation, and the whole card is bordered inside a region
  that is itself bordered. Worth designing rather than patching.
- **Note 9.3 — a "notes" affordance on an answer.** The operator's own idea, and the
  operator already named the open question: whether it belongs on an *approval* (a
  yes/no with consequences) or only on an *ask*/clarify (a question expecting
  prose). Recorded as a v0.3 design question, not a defect. Worth checking what the
  Hermes wire can even carry — `approval.respond` may have nowhere to put free text,
  in which case the answer is decided for us.
- **Note 13 — the mouse offset confirms note 2's hypothesis and supersedes it.**
  Talaria *is* capturing mouse events — that is why the terminal's own
  select-and-copy never reaches through — and its own hit-testing then maps the
  click to the wrong row. So note 2 and note 13 are one defect seen from two sides,
  not two defects. Undiagnosed. The obvious suspect is the mixed-height widget
  layout this release introduced: block documents and line widgets now sit in the
  same scrollable pane at different heights, so any coordinate maths that assumes
  one row per widget would drift by exactly the accumulated difference — which
  matches "a number of lines up from where I clicked". Needs a reproduction at a
  known scroll offset to confirm.
- **Note 14 — this is the second sighting of duplicated rendered content.** Note 6
  recorded an assistant line rendered twice around a tool call; this is a heading
  and a fenced code block rendered again after a table. Two independent sightings
  raise this well above curiosity. Still undiagnosed, and still not attributable to
  Talaria without evidence — a long generation genuinely repeating itself is a real
  possibility for the second case, though much less so for the first. Settling it
  needs the same thing both times: a session run under `talaria --record`, so the
  frame log shows whether the duplicate arrived on the wire or was drawn twice.
  This is now the highest-value diagnostic to run next.
- **Note 15 — a one-line fix the code already anticipated.** Not a defect in the
  handling: `talaria/domain/decode.py:114` states the rule plainly — a type outside
  `KNOWN_EVENT_TYPES` is surfaced by name (R5) rather than dropped, because the
  gateway registers far more methods than any client uses, so unknown events are
  expected traffic. `platforms.changed` is simply not in the set.
  What makes this actionable rather than noise is the set it belongs in.
  `_OBSERVED_ON_A_LIVE_GATEWAY` (`decode.py:110`) holds `sessions.changed`,
  `session.title` and `session.reclaimed` — three types found by attaching to a live
  gateway on 2026-08-04 — and its comment names its own purpose: "the place any
  future live capture should add to". **This session is that future live capture.**
  Add `platforms.changed` to that frozenset.
  Separate and worth weighing: five identical red rows for one repeated event is
  loud. The design says surface unknown types rather than drop them, which is right,
  but repeated identical types could be coalesced into one row with a count.
  On the operator's own question — whether session switching caused it — unanswered.
  The event name suggests it fires on gateway-side platform changes rather than on
  anything Talaria did, but that is a guess, not a finding.
- **The expired-approval question from note 7 is answered, and the answer is the safe
  one.** The transcript records what happened when the card timed out:
  `Command was blocked — it timed out waiting for approval and wasn't executed:
  BLOCKED: Command timed out without user response. The user has NOT consented to
  this action.` So an unanswered approval defaults to **deny**, not approve. Nothing
  to fix; worth knowing.
- **Note 16 — the wrong-session resume is correct behaviour with a bad outcome.**
  Not a bug. `--resume` is documented as resuming the most recently used session via
  `session.most_recent`, and it did exactly that: the session list read during step 4
  showed the operator's own session last touched at 13:55 while several automation
  sessions had been touched at 18:37. The gateway's most-recent session genuinely was
  not the operator's.
  The design assumption underneath is the thing to revisit: `--resume` reads as "pick
  up where **I** left off", but `session.most_recent` means "whatever touched this
  gateway last", and on a gateway shared with automation those are different
  sessions most of the time. The failure is silent — nothing on screen says "this is
  not the session you were in", and the operator only knows because the content is
  visibly foreign.
  Worth weighing for v0.3, in rough order of cost: name the resumed session on
  arrival so the surprise is at least visible; prefer the most recent session *this
  client* attached to; or make `--resume` open the picker pre-filtered rather than
  choosing silently. The first is nearly free and fixes the silence, which is the
  actual harm.
  Also confirmed by this step, though not its purpose: quitting with `ctrl+q` and
  relaunching worked cleanly, and the credential survived the restart — no
  `refresh-credential` was needed, because the dashboard itself never restarted.
- **Note 17 — `platforms.changed` is confirmed as gateway chatter, faithfully
  surfaced.** 26 events arrived on the wire and roughly 26 rows were drawn, so
  Talaria is not duplicating anything — it is doing exactly what `decode.py:114`
  says it does. The fix stands as written in note 15: add `platforms.changed` to
  `_OBSERVED_ON_A_LIVE_GATEWAY`.
  The 204 `sessions.changed` events are the argument for that fix rather than a
  footnote to it. That type **is** in the known set, so it passed in silence. Had it
  not been, one ordinary turn would have drawn 204 red rows. The design is sound;
  the set is just incomplete, and every gap in it is a flood waiting for a live
  gateway to find.
- **Note 17 — the duplication did NOT reproduce, and the negative result is
  informative.** Reconstructing the assistant text from the 580 `message.delta`
  events gives 1,950 characters; the `message.complete` payload carries 1,948. The
  headings appear exactly once each — `## Python`, `## C`, `## Rust`,
  `## Comparison`. No repeat on the wire, and none on screen.
  So notes 6 and 14 remain undiagnosed, and this run does not clear Talaria. What it
  does is narrow the reproduction. This turn had one `message.start` and one
  `message.complete` — a single assistant message *after* the tool call. The note 6
  sighting had assistant text **before** the tool rows and again after, which needs
  a turn where the model speaks, calls a tool, then continues. That shape is what a
  future recorded reproduction has to force.
  Recorded as a genuine negative: one attempt, one failure to reproduce, no
  conclusion drawn from it beyond narrowing the next attempt.
### Note 18 — replay controls work, and `F4` is documented by half

Exercising: step 6, replay driven from
`~/.talaria/recordings/2026-08-11T00-18-11-356Z.jsonl`.

> f8 gave me a "paused" in the message box
>
> F9 moved it to 0.5x

Status line during replay, which also demonstrates the inert-control claim:

```
replay — not connected · paused · 0.25x
```

`F9` pressed twice stepped 1x → 0.5x → 0.25x, so the key repeats correctly. The
`not connected` clause is the visible proof that no socket is open in the process.

- **Note 18 — the shipped release notes describe `F4` by half, and the missing half
  is destructive.** Both `docs/releases/v0.2.0.md:21` and `CHANGELOG.md:26` say
  "`F4` sweeps the answerable set". That is true and incomplete. `app.py:776` binds
  `F4` to `action_interrupt`, whose docstring is "Stop the in-flight turn (R4)"
  (`:1436`); the sweep happens *after* a confirmed interrupt, at `:1666`, which
  calls `decline_outstanding_prompts`.
  So `F4` stops whatever the model is doing and *then* declines every outstanding
  prompt. A reader who takes the release note at face value and presses `F4` to
  answer prompts also kills their running turn. The documentation is not wrong; it
  omits the consequential half. Fix the sentence in both files.
  (Worth noting the source of this finding: these notes' own author repeated the
  release note's framing when scripting step 3, and would have told the operator to
  press `F4` to "sweep" mid-turn. The published sentence is genuinely misleading,
  not merely terse.)
### Note 19 — the function-key map, as far as it has been driven by hand

> so F2 is mapped to apples mission control and F5 does nothing

| Key | Talaria's binding | Result in a real terminal on macOS |
| --- | --- | --- |
| `F1` | `jump_to_prompt` | eaten before Talaria sees it |
| `F2` | `toggle_agents` | eaten — macOS Mission Control |
| `F4` | `interrupt` (and sweep) | untested |
| `F5` | `follow_bottom` | "does nothing" — cause unresolved, see below |
| `F8` | `toggle_pause` | works |
| `F9` | `slow_down` | works, repeats correctly |
| `F10` | `speed_up` | untested, but adjacent to two that work |

- **Note 19 — `F5` is ambiguous evidence, and that ambiguity is itself the finding.**
  `F5` is bound to `follow_bottom`, which re-follows the newest line. It was pressed
  in a **paused replay already sitting at the bottom**, where re-following is a
  legitimate no-op. So "does nothing" is exactly what a working `F5` and a
  swallowed `F5` both look like from the operator's chair. The observation cannot
  distinguish them.
  To resolve it: scroll well up, then press `F5`. If the view snaps to the bottom
  the key is alive.
- **The synthesis this session keeps arriving at: Talaria does not confirm what it
  did.** Four findings that looked unrelated are one theme.

  | Finding | The shape it takes |
  | --- | --- |
  | Note 4 — the `caret:` row | reports state in a word the operator cannot interpret |
  | Note 5 — the fallback banner | reports a number against an unstated scope |
  | Note 9 — the approval card | advertises two keys that do nothing in the state shown |
  | Note 19 — `F5` | a successful keypress is indistinguishable from a dead one |

  Each was written up as its own item, and each is cheap to fix on its own. But the
  v0.3 framing worth considering is the common one: **every control should say what
  it just did, and every number should say what it counts.** That is a smaller,
  more tractable release theme than "readability", and it subsumes most of note 11.
- **Note 2 — ambiguity to resolve.** "ctrl+c does work though" — unclear whether that
  means `ctrl+c` copies, or that `ctrl+c` still quits the interface. Ask before
  acting on it.
