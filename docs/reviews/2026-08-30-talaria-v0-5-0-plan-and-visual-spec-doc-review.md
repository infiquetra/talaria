---
title: Talaria v0.5.0 run plan and visual specification — integrated document review
type: review
status: blocked
date: 2026-08-30
origin: https://github.com/infiquetra/talaria/issues/103
---

# Talaria v0.5.0 run plan and visual specification — integrated document review

## Review-result contract

| Field | Value |
| --- | --- |
| Target paths | `docs/plans/2026-08-30-talaria-v0-5-0-run-plan.md` and `docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`, reviewed together as one pair |
| Reviewed revision | `16287bf3c1dc2d2f743df96321ba0a410402ca7f` (branch `orch/talaria-v0-5-0-docreview-claude`, clean working tree) |
| Blocked status | **BLOCKED** — 17 blocking findings remain (2 at `P0`, 15 at `P1`) |
| Finding counts | `P0` 2, `P1` 15, `P2` 4, `P3` 3 — 24 total |
| Applied fixes | **None.** Every blocking finding is a choice between two answers the approved issues both authorise; the doc-review skill classifies choosing between authorised options as an unsafe edit, and the operator assigned the repairs to the two authoring sessions. |
| Review artifact | `docs/reviews/2026-08-30-talaria-v0-5-0-plan-and-visual-spec-doc-review.md` (this file) |
| Linked issues | Parent [infiquetra/talaria#103](https://github.com/infiquetra/talaria/issues/103); children [#104](https://github.com/infiquetra/talaria/issues/104)–[#111](https://github.com/infiquetra/talaria/issues/111) |
| Rubric review | Issue phase, three core rubrics applied (`acceptance_criteria_clarity`, `devils_advocate_issue`, `spec_fidelity`) plus two conditional extras (`context_completeness`, `prerequisite_mapping`) |
| Override rationale | Not applicable — no override requested or granted |

## Readiness summary

**These two documents cannot yet drive implementation together, because on sixteen separate
decisions they give two different answers and each answer carries its own tests.**

Read alone, each document is unusually strong. The plan is decision-complete about ownership,
integration order, the serialized lease, and evidence provenance; the visual specification is
decision-complete about pixels, tokens, thresholds, and contrast, and every one of its 336 published
contrast numbers recomputes exactly from its own hexadecimal values. Neither author invented scope
the approved issues do not authorise.

Read as a pair — which is how four concurrent lanes will read them — they disagree about the theme
token names, the Visual Studio Code import mapping, the theme picker's widget and command name, the
inspector's width bound and collapse threshold, the diff viewer's mode threshold and key bindings,
the status bar's entire responsive algorithm and its drop priorities, the composed widget tree's
identifiers, and how the acceptance work is split between the two testers. A worker following one
document builds something a worker following the other document's tests will fail.

Separately from those collisions, one genuine product decision is unresolved by both documents and by
every issue body; it is described in its own section below and it is the only finding here that
cannot be repaired by choosing a side.

## What I verified, and what came back clean

**Verification found no defect in the specification's arithmetic, in the plan's citations, or in
either document's fidelity to the approved issue graph.**

- **Contrast is measured, not asserted.** I reparsed the specification's 54-token value table and
  recomputed all 84 published pairs across all four themes with the Web Content Accessibility
  Guidelines 2.2 relative-luminance formula. All 336 cells match the published figures to within
  0.055, none falls below its stated floor, and the four per-theme minimum rows (4.70 / 5.38 / 5.00 /
  6.22 for text, 3.50 / 3.68 / 3.62 / 6.08 for non-text) reproduce exactly. Every claimed pair maps
  to two real tokens; none is orphaned. This is the strongest part of either document.
- **The specification's inventory of the current interface is accurate.** Its "Real Textual variable
  use" table lists eleven variables. The real source under `talaria/ui/` uses exactly those eleven
  and no twelfth. Its twelve-kind-to-six-group transcript mapping matches the source. Its claim that
  the screen keeps two fixed footer rows before and after the release matches
  `talaria/ui/app.py:1515-1539`, which composes `NeedsYouBar` then `HelpBar`.
- **The specification's seven agent states are the real ones.** `talaria/domain/models.py:31-37`
  declares exactly `completed`, `error`, `failed`, `interrupted`, `queued`, `running`, `timeout`.
- **The plan's repository citations are real.** Every file it cites by path exists, every line range
  falls inside its file, and every test file it names to update exists. The three engineering-journal
  anchors land on the right entries: the caret-indication deferral at
  `docs/engineering-journal/QUEUED.md:1293`, the silent `status.command` failure at `:1355`, the
  integer range validation at `:1152`, and the "caret marker lives in the status region" decision at
  `docs/engineering-journal/DECISIONS.md:1187`.
- **The plan is right that Pygments must be declared.** `pyproject.toml` declares only Textual and
  websockets; Pygments 2.20.0 reaches the tree transitively through `uv.lock`. Production code
  depending on it undeclared is exactly the hazard the plan names.
- **The seven Code Review lenses are stated correctly and completely.** All seven map onto real
  entries in the canonical roster at `plugins/saga/references/lens-roster.json`. The four always-on
  lenses are all present — Architecture is `architecture-maintainability`, plus `correctness`,
  `security`, and `testing` — and the three the plan adds are real conditionals: API/config contract
  is `api-contract`, Terminal usability/accessibility is `accessibility-human-usability`, and
  Documentation is `documentation-clarity`. Omitting any always-on lens is a defect under that
  roster, and none is omitted. A caller-supplied selection counts as approval under the roster's own
  `caller_or_orchestrate_selection_is_approval` rule, so a plan-declared set of seven is legitimate.
- **The primary and fallback model routing rule is stated correctly and completely in both
  documents.** Both name OpenCode Muse Spark 1.2 Contributor Free as primary and Ollama GLM 5.3 Flash
  as the sole fallback; both reproduce all four permitted triggers from issue #110 — primary
  unavailability, connection failure, model-not-found, and bounded-test incompletion — and both
  require the route and the exact fallback reason in every receipt, with silent substitution failing
  the leg.
- **No unauthorised scope in either document.** Every addition traces to issue text: the
  specification's three status-bar width-cap keys to #106's "the new bar's integer keys", its
  `ctrl+b` binding to #107's "keyboard toggle", its narrow overlay to #107's "the toggle overlays or
  defers per planning", the plan's `/bar` command to #106's "palette toggles", and the plan's
  Pygments declaration to #108's "syntax highlighting".
- **The one structural decision that most needed agreement, agrees.** Both documents place `HelpBar`
  as the documented adjacent row, absorb `NeedsYouBar`'s queue summary into the `task_progress`
  segment, keep `StatusRegion` inside the body, and make the new bar the only true last row. Issue
  #106 left that open for planning; both authors closed it the same way.

## Findings by priority

| Key | Priority | Where | Finding | Status |
| --- | --- | --- | --- | --- |
| D1 | `P0` | Both — token vocabulary | Two incompatible theme-token vocabularies for the same layer | Open — blocking |
| D2 | `P0` | Specification — non-color signaling vs contrast contract | Bottom-bar semantic colouring is undecided and one reading fails the release's own contrast floor | Open — blocking, unresolved product decision |
| D3 | `P1` | Both — Visual Studio Code mapping | The two mapping tables assign different source keys to the same tokens | Open — blocking |
| D4 | `P1` | Both — `tokenColors` resolution | Last-rule-wins versus longest-prefix-wins | Open — blocking |
| D5 | `P1` | Both — colour syntax | Plan stores alpha values the specification forbids at runtime | Open — blocking |
| D6 | `P1` | Both — fallback-only tokens | Seven fallback-only tokens versus fourteen | Open — blocking |
| D7 | `P1` | Both — theme picker surface | Modal `PickerDialog` versus a mode inside `PaletteRegion` | Open — blocking |
| D8 | `P1` | Both — theme command | `/themes` with a three-outcome scope dialog versus `/theme` plus `/theme save` | Open — blocking |
| D9 | `P1` | Both — status-bar responsive model | Two configurable thresholds versus nine fixed bands, and three schema keys missing from the plan | Open — blocking |
| D10 | `P1` | Both — segment drop priorities | Different priority numbers and a different drop order | Open — blocking |
| D11 | `P1` | Both — inspector width | Maximum 60 columns versus maximum 48 | Open — blocking |
| D12 | `P1` | Both — inspector narrow contract | Auto-collapse at 90 versus 120, and no overlay versus an overlay | Open — blocking |
| D13 | `P1` | Both — diff mode threshold | Side-by-side at 120 columns versus 112 | Open — blocking |
| D14 | `P1` | Both — diff key bindings | `n`/`p` and `j`/`k` versus `n`/`p` and `N`/`P` | Open — blocking |
| D15 | `P1` | Both — canonical compose tree | Two different "canonical" trees with different widget names and identifiers | Open — blocking |
| D16 | `P1` | Specification — acceptance checklist preamble | Both testers run all 31 items, contradicting the approved track split in #110 | Open — blocking |
| D17 | `P1` | Specification — acceptance checklist | Five flows named in #110's own coverage list have no checklist item | Open — blocking |
| D18 | `P2` | Specification — checklist items 7 and 25 | Two checklist items name no method a tester could execute | Open — optional |
| D19 | `P2` | Plan — repository grounding | The supersession of the issues' `tests/config` command is not carried into closeout | Open — optional |
| D20 | `P2` | Plan — Saga Code Review lenses | The per-child review lens set is unstated and lens names are prose rather than roster identifiers | Open — optional |
| D21 | `P2` | Specification — example import report | The example reports seventeen fallbacks while the fallback-only table lists fourteen | Open — optional |
| D22 | `P3` | Plan — command and key ownership | Gateway ownership of `/status` is asserted without a probe | Open — optional |
| D23 | `P3` | Plan — integration order | Issue #103's "all eight children closed" criterion has no owning unit | Open — optional |
| D24 | `P3` | Specification — combined breakpoint table | The 120–143 band uses compact forms although full forms fit at 138 columns | Open — optional |

## Blocking findings

Each finding names the document, the section, what is wrong, and what would fix it. Where one
document's answer is better supported by the repository or by the approved issue text, that is said
plainly; where the two answers are equally authorised, the choice is the authors'.

### D1 (`P0`) — Two incompatible theme-token vocabularies

**Where.** Plan, "Theme token vocabulary and Visual Studio Code mapping" (lines 189–202) against
specification, "Complete registry" (lines 107–162).

**What is wrong.** The two documents define different token name sets for the same layer, and neither
is a superset of the other. The plan defines 29 flat lower-case-with-underscore names in five
groups (`background`, `surface`, `panel`, `text`, `text_muted`, `primary`, `secondary`, `accent`,
`success`, `warning`, `error`, `focus`, `queue_attention`, six `transcript_*`, five `diff_*`, five
`syntax_*`). The specification defines 54 dotted names of the form `talaria.category.name`, bridged to
Textual as `$talaria-...`. The plan defines `queue_attention` and `diff_changed`, which the
specification does not define at all. The specification defines `talaria.border`,
`talaria.border.muted`, both `talaria.selection.*` tokens, four `talaria.status.*` tokens, three
`talaria.inspector.*` tokens, twelve transcript tokens rather than six, `talaria.diff.context`,
`talaria.diff.line-number`, both `talaria.diff.hunk*` tokens, and nine syntax tokens rather than five
— none of which the plan defines.

This is the foundation unit. Unit U1 builds `ThemeSpec` from the plan's list; the specification's
exact-value table, its entire contrast contract, and its import mapping are keyed to the other list.
Units U2, U5, and U6 all consume whichever one U1 built. Choosing wrong here is not one lane's rework.

A second, checkable defect sits inside the plan's list: it provides no bridge for `$text-warning`,
which the real source uses at `talaria/ui/transcript.py:653` for the oversized-entry fallback banner.
The specification does bridge it, through `talaria.warning`. Implemented from the plan's list alone,
that banner keeps Textual's auto-derived colour and acceptance checklist item 4 — "no surface uses an
unthemed stock color" — cannot be honestly passed.

**What would fix it.** Adopt the specification's 54-token registry as the single normative
vocabulary, and replace the plan's token table with a pointer to it rather than a second copy. The
specification's set is the one that carries measured values, covers all eleven Textual variables the
current source actually uses, and reaches every surface the release adds. Then settle the two
plan-only names explicitly: decide whether `queue_attention` becomes a real token (see D2) and
whether `diff_changed` is dropped or mapped onto `talaria.diff.hunk`.

### D2 (`P0`) — Bottom-bar semantic colouring is undecided, and one reading fails the release's own contrast floor

This is the unresolved product decision. It has its own section below, "The one unresolved product
decision", because unlike every other finding here it cannot be repaired by choosing between the two
documents.

### D3 (`P1`) — The two Visual Studio Code mapping tables assign different source keys to the same tokens

**Where.** Plan, the Visual Studio Code source table (lines 219–242) against specification, "Supported
workbench colors" (lines 581–613).

**What is wrong.** For the same semantic token the two tables read different keys out of the imported
file, so the same input file produces two different themes:

| Talaria concept | Plan reads | Specification reads |
| --- | --- | --- |
| raised surface | `colors.sideBar.background` | `editorWidget.background`, then `input.background` |
| primary | `colors.activityBar.foreground` | `textLink.foreground` |
| secondary | `colors.statusBar.foreground` | nothing — always a Refined Default fallback |
| accent | `colors.button.background` | `activityBarBadge.background`, then `button.background` |
| error | `colors.editorError.foreground` | `errorForeground`, then `editorError.foreground` |
| inspector fill | not mapped | `sideBar.background` — the key the plan gives to `surface` |
| added-line fill | not mapped | `diffEditor.insertedLineBackground` |
| `diff_changed` | `colors.gitDecoration.modifiedResourceForeground` | the token does not exist |

The plan also has no notion of multiple candidate keys with a precedence order, which the
specification requires ("the first present valid key in the listed order wins"). Issue #105's
acceptance criteria require asserting exact mapped values and exact warning counts, so the fixture
test's expected values differ under the two tables. Worse, the plan instructs unit U2 to copy its own
table verbatim into `docs/formats/vscode-theme-import.md`, and issue #105 requires the documentation
child to link that published table rather than re-derive it — so the wrong table would become the
published contract.

**What would fix it.** Adopt the specification's workbench-colours table as normative, including its
multi-candidate precedence rule, and replace the plan's table with a pointer to the specification
section. Then delete the plan's instruction to copy its own table into `docs/formats/`, replacing it
with an instruction to publish the specification's.

### D4 (`P1`) — The `tokenColors` resolution algorithms contradict each other

**Where.** Plan, line 244, against specification, "Supported tokenColors scopes" (line 619).

**What is wrong.** The plan says "the last valid matching rule in document order wins". The
specification says "the longest supported scope prefix wins; a later rule wins ties", and gives the
worked consequence: `constant.numeric` maps to the number token rather than the broader constant
token. For a real theme file carrying a broad `constant` rule after a narrow `constant.numeric` rule,
the two algorithms produce different values for `syntax.number`. Issue #105 requires asserting exact
mapped values, so a fixture written against one algorithm fails under the other.

**What would fix it.** Adopt the specification's longest-prefix-wins rule and delete the plan's
sentence. The specification's rule is the one that matches how a reader expects a more specific scope
to beat a broader one, and it is the one whose consequence is already worked out in prose.

### D5 (`P1`) — The plan would store alpha colour values the specification forbids at runtime

**Where.** Plan, lines 245–247, against specification, "Naming and Textual bridge" (line 102) and
"Input and resolution rules" (line 573).

**What is wrong.** The plan accepts `#RGBA` and `#RRGGBBAA` input and never says what becomes of the
alpha channel, so the natural implementation stores the eight-digit value. The specification requires
that "runtime theme dictionaries contain opaque uppercase `#RRGGBB` values only", that import
composite alpha in sRGB against the destination's normative background, that background tokens
composite against their enclosing canvas or panel, that an unmapped background fall back to Refined
Default's value for the composite, and that the report name every alpha composite performed. The plan
requires none of that and its stored output would violate the specification's runtime invariant.

**What would fix it.** Add the specification's compositing rule and its report requirement to the
plan's unit U2 approach, or replace the plan's colour-syntax paragraph with a pointer to the
specification's "Input and resolution rules".

### D6 (`P1`) — The fallback-only token count is seven in one document and fourteen in the other

**Where.** Plan, lines 249–251, against specification, "Tokens with no Visual Studio Code source"
(lines 653–672).

**What is wrong.** The plan says the tokens with no Visual Studio Code source are `queue_attention`
plus six `transcript_*` tokens — seven. The specification says fourteen: `talaria.secondary`,
`talaria.status.muted`, and twelve transcript tokens (six foreground and six background). Issue #105's
acceptance criterion is that "extension-token fallback is listed; counts asserted", so a test will be
written against one of these two numbers.

**What would fix it.** This resolves automatically once D1 is settled in the specification's favour;
adopt fourteen and delete the plan's sentence. Note that `talaria.secondary` becoming
fallback-only is a real consequence of D3 — the plan gave it `colors.statusBar.foreground`, which the
specification assigns to `talaria.status.text` instead.

### D7 (`P1`) — The theme picker is a modal dialog in one document and a palette mode in the other

**Where.** Plan, "Repository grounding" (line 82) and unit U1 approach (lines 394–396), against
specification, "Application and persistence behavior" (line 229).

**What is wrong.** The plan reuses `PickerDialog`, which is a `ModalScreen` at `talaria/ui/dialog.py:72`
with documented exclusive key ownership and layered Escape behaviour, and extends it with a
highlighted-choice message. The specification says "the theme picker is a mode in Talaria's existing
PaletteRegion", which is a `Vertical` at `talaria/ui/palette.py:186` living inside the body container.
These are different widgets with different key ownership, different geometry, and different focus
behaviour; one implementation cannot satisfy both.

Issue #104 reads for the specification: "extend the existing command palette (Talaria's own palette
region) with a theme picker". The plan's choice reuses more of the repository's existing
modal-key-ownership machinery, which is a real engineering argument, but it is arguing against the
issue's own words.

**What would fix it.** Take the specification's reading, because it is the one issue #104 names, and
rewrite the plan's unit U1 approach and its "Repository grounding" row accordingly. If the plan's
modal argument is strong enough to override the issue text, that override needs to be stated as a
deliberate departure with its reason, not left as a silent disagreement.

### D8 (`P1`) — The theme command name and the meaning of Enter contradict

**Where.** Plan, "Command and key ownership" (line 146) and KTD2 (lines 318–321), against
specification, "Application and persistence behavior" (lines 231–236).

**What is wrong.** Two disagreements in one interaction. First the name: the plan registers `/themes`,
the specification uses `/theme` and `/theme save`. Second the semantics of Enter: the plan says Enter
opens a second small dialog offering three explicit outcomes — session only, save user, save
repository — while the specification says Enter closes the picker and records a session-only
selection, with a separate `/theme save` performing the write. Both satisfy issue #104's requirement
that browsing never writes, so neither is unauthorised; they are simply two different products.

On the name, the repository has a documented convention that favours the plan. `talaria/domain/commands.py:376-386`
records that Talaria's local commands are deliberately plural to avoid shadowing the gateway's
singular: `/models` exists because the gateway owns `/model` — probed live on 2026-08-06 against 114
catalogue names — and `/profiles` because the gateway owns `/profile`. Nobody has probed whether the
gateway owns `/theme`. Choosing the singular reverses a convention that exists precisely to prevent
shadowing a working gateway command.

**What would fix it.** Take `/themes` for the name, on the repository's own documented convention, and
choose one of the two Enter models deliberately. Whichever is chosen, the plan's KTD2, the plan's
command table, the specification's numbered persistence steps, and acceptance checklist items 8 and 9
must all be rewritten to describe the same interaction.

### D9 (`P1`) — The status bar's responsive model is two incompatible algorithms, and three schema keys exist in only one document

**Where.** Plan, "Consolidated configuration schema" (lines 169–170) and unit U3 approach (lines
489–492), against specification, "Combined breakpoint table" (lines 474–485) and "Status-bar
truncation and drop contract" (line 512).

**What is wrong.** The plan makes responsiveness configurable through two integer keys:
`status.compact_at_columns` (default 100, valid 60–240) and `status.minimal_at_columns` (default 72,
valid 40–160, strictly below the first). The specification makes it fixed, through nine hard bands at
144, 120, 112, 96, 80, 64, 48, 32, and 20 columns, each band prescribing which segments compact and
which drop. At 120 columns the plan renders full forms and the specification renders compact forms; at
90 columns the plan renders all seven in compact form while the specification has already dropped
version and current working directory. Both sides carry tests: the plan's unit U3 asserts
"100/72-column transitions", and acceptance checklist item 14 walks eighteen specific widths derived
from the specification's bands.

The schema disagreement runs in both directions. The specification introduces three integer keys the
plan's table — which the plan calls "the only schema ledger for the run" — does not contain:
`cwd_max_columns` (default 24, valid 8–48), `git_branch_max_columns` (default 18, valid 8–40), and
`agent_model_max_columns` (default 24, valid 10–48). Acceptance checklist item 15 requires invalid
"bar-width integers" to produce visible notices, so a tester will look for keys the plan's schema does
not define. Meanwhile the plan's two threshold keys appear nowhere in the specification.

**What would fix it.** Choose one model and make the plan's schema table complete under it. If the
specification's fixed bands win — they are the more defensible choice, because they make the
eighteen-width acceptance walk deterministic and they are what the checklist already tests — then
delete `status.compact_at_columns` and `status.minimal_at_columns` from the plan's schema and add the
three width-cap keys with their defaults and ranges. If the plan's configurable thresholds win, the
specification's band table and checklist item 14 must both be rewritten.

### D10 (`P1`) — The segment drop priorities differ in both their numbers and their order

**Where.** Plan, unit U3 approach (lines 481–482), against specification, "Status-bar truncation and
drop contract" (lines 491–501).

**What is wrong.** The plan assigns connection 100, task progress 90, context 80, agent/model 70,
current working directory 60, Git branch 50, version 10 — giving a drop order of version, Git branch,
current working directory, agent/model, context, task progress. The specification assigns current
working directory 10, Git branch 20, context 40, agent/model 50, task progress 80, connection 100,
version 0 — giving a drop order of version, current working directory, Git branch, context,
agent/model, task progress. The two orders are not the same: the plan drops Git branch before the
working directory and agent/model before context, and the specification does the reverse of both.
Acceptance checklist item 14 asserts "segments drop in the specified bands/order".

**What would fix it.** Adopt one priority table verbatim and delete the other. The specification's is
the one the checklist tests and the one that reads more naturally against its own combined breakpoint
table, whose 80–95 band drops the working directory before the 64–79 band drops the Git branch.

### D11 (`P1`) — The inspector's maximum width is 60 columns in one document and 48 in the other

**Where.** Plan, configuration schema row for #107 (line 173) and unit U4 approach (line 548), against
specification, "Decisions taken" item 4 (line 18) and "Inspector behavior" (line 519).

**What is wrong.** The plan clamps the panel to 28–60 columns in four-column steps. The specification
clamps it to 28–48. Acceptance checklist item 16 asserts the panel "clamps at 28 and 48", and the
plan's unit U4 test asserts its own resize bounds. A tester following the checklist against an
implementation built from the plan sees the panel keep growing past 48 and fails the item.

**What would fix it.** Pick one maximum and write it in both places. The specification's 48 is better
supported: it is the number the checklist already tests, and it keeps the transcript at 84 columns or
more at the specification's 132-column docked example, whereas 60 would leave 72.

### D12 (`P1`) — The inspector's narrow contract differs in both its threshold and its behaviour

**Where.** Plan, configuration schema row for #107 (line 173) and unit U4 approach (lines 550–551),
against specification, "Decisions taken" item 4 and "Inspector behavior" (lines 519–524).

**What is wrong.** Two disagreements. The threshold: the plan auto-collapses below 90 columns and
tests "89/90-column transitions"; the specification docks only at 120 columns or wider and describes
"crossing from 120 to 119 auto-collapses". That is a thirty-column band — every terminal from 90 to
119 columns behaves differently under the two documents, and acceptance checklist item 18 exercises
120 → 119 → 120.

The behaviour: the plan says that when narrow, toggling "reports that the panel will reopen when
space returns **rather than overlaying content**". The specification says "below 120, toggling opens a
right overlay and does not reflow or resize the transcript", and then specifies the overlay's width at
32–119 columns and below 32, its `[overlay]` border title, and its Escape-restores-focus behaviour.
Acceptance checklist item 18 requires "the narrow toggle opens an overlay without transcript reflow" —
an implementation built from the plan fails that item outright, because it deliberately has no
overlay.

Issue #107 authorised either: "on narrow terminals it auto-collapses and the toggle overlays or defers
per planning". So this is two authorised answers colliding, not scope creep.

**What would fix it.** Choose one threshold and one narrow behaviour, and rewrite the loser's section
plus its tests. The specification's pairing is more coherent — a 120-column dock threshold that
matches the diff viewer's neighbourhood, with an overlay that keeps the feature reachable when
docking is impossible — and it is the pairing the acceptance checklist already encodes.

### D13 (`P1`) — The diff viewer's side-by-side threshold is 120 columns in one document and 112 in the other

**Where.** Plan, configuration schema row for #108 (line 174) and unit U5 approach (line 585), against
specification, "Decisions taken" item 5 (line 19) and "Diff behavior" (line 529).

**What is wrong.** The plan makes side-by-side effective at 120 columns or wider, forces unified below
120, and tests "119/120-column effective mode". The specification requires 112 columns and shows the
arithmetic: two 54-column panes, two outer edge cells, one centre divider, and one scrollbar reserve
cell — which sums to exactly 112. Acceptance checklist items 19 and 20 test 112 and 111.

**What would fix it.** Adopt 112 and correct the plan's schema row, its unit U5 approach, and its unit
U5 test list. The specification's number is the one with a published derivation and the one the
checklist tests; the plan's 120 is stated without a width budget.

### D14 (`P1`) — The diff viewer's key bindings contradict

**Where.** Plan, "Command and key ownership" (line 149), against specification, "Diff behavior" (line
532) and the two diff mockups' key-hint rows (lines 440 and 462).

**What is wrong.** The plan binds `n`/`p` to next and previous **file**, and `j`/`k` to next and
previous **hunk**. The specification binds `n`/`p` to next and previous **hunk**, `N`/`P` to next and
previous **file**, and has no `j`/`k` at all. The same two keys mean opposite things in the two
documents, and the specification's mockups render its own binding into an on-screen hint row that a
tester will read during acceptance checklist item 21.

**What would fix it.** Adopt the specification's set, because it is the one already drawn into the
viewer's own visible key-hint row, and correct the plan's command and key ownership table. Note in
passing that `j`/`k` are printable keys, and `talaria/ui/dialog.py:278-300` records a repository
decision that printable keys belong to the filter in `PickerDialog` — the diff screen is a different
surface so the plan's choice is not unsafe, but the specification's set avoids the question entirely.

### D15 (`P1`) — Two documents each claim to hold the one canonical compose tree, and they disagree

**Where.** Plan, "Canonical final compose order" (lines 116–129), against specification, "Current
screen geometry" (lines 46–58).

**What is wrong.** The plan's tree names a `Horizontal #workspace` containing a `Vertical #main-pane`,
an `InspectorPanel`, and a `StatusBar`. The specification's tree names a `MainAndInspector` containing
`#body`, an `Inspector`, and a `BottomStatusBar`. The vertical ordering agrees, which is the important
half; the identifiers and class names do not, which matters because the plan's own shared-surface
lease says "the final tree, command table, and schema table above are the merge specification; a
textual conflict is resolved field by field against them". Two merge specifications is one too many.

The container identifier is not cosmetic. The real source composes `Vertical(id="body")` at
`talaria/ui/app.py:1515`, its stylesheet carries `#body { height: 1fr; }`, and two existing tests query
that identifier directly — `tests/ui/test_status_region.py:124` and `tests/ui/test_needs_you.py:114`.
The plan's rename to `#main-pane` breaks both, and the plan does not list either test as needing that
update.

**What would fix it.** Adopt the specification's names, which keep `#body` and therefore keep both
existing tests green, and rewrite the plan's canonical tree to match. If the rename is wanted anyway,
the plan must add those two test files to unit U3's file list with the reason.

### D16 (`P1`) — The acceptance checklist requires both testers to run every item, contradicting the approved split

**Where.** Specification, "Visual acceptance checklist" preamble (line 795) and item 31, against issue
#110's "Split" paragraph and the plan's execution-capacity table (lines 283–284).

**What is wrong.** The specification says "both tester roles, talaria-t1 and talaria-t2, execute every
numbered item", and item 31 reinforces it: "both independently pass items 1–30". Issue #110 says the
opposite — "talaria-t1 owns the theming/import/polish track; talaria-t2 owns the status
bar/inspector/diff track; both run install verification, restart, and failure paths independently" —
and the plan's capacity table encodes exactly that split. The specification's reading roughly doubles
the acceptance workload and contradicts the approved contract rather than another plan.

**What would fix it.** Rewrite the specification's preamble and item 31 to the approved split: mark
each of items 1–30 with its owning tester, keep the install-verification, restart, and failure-path
items as shared, and make item 31 compare the shared items across testers rather than all thirty. This
is the one blocking finding where a document contradicts an issue rather than the other document, so
the issue wins without further discussion.

### D17 (`P1`) — Five flows named in issue #110's own coverage list have no checklist item

**Where.** Specification, "Visual acceptance checklist" (items 1–31), against issue #110's "Flow
coverage" list and the plan's "Per-child mapping to #110 real-terminal flows" (lines 806–819).

**What is wrong.** The checklist is the artifact a tester actually executes, and issue #110's
acceptance criterion is that "every flow in the coverage list has a captured-output artifact, a
screenshot, and a verdict in the evidence document". Five flows that #110 or the plan names have no
item to execute:

1. **A dead gateway credential.** #110 lists it among the ordinary failure paths; the plan's #110 row
   names it. Item 15 covers malformed configuration but not a dead credential.
2. **A killed session.** #110 lists it; the plan's #110 row names it. No item covers it.
3. **A malformed Visual Studio Code import.** The plan's #105 row requires "repeat malformed import"
   with "malformed input exits clearly with no artifact", and #105's acceptance criterion requires
   rejection with no stored artifact. Item 11 exercises only the happy path with warnings.
4. **The `/bar` session toggle.** The plan's #106 row requires observing that the toggle "is immediate
   and unsaved". No item covers it, so a shipped command goes unproved.
5. **The restart-semantics proof.** #110 lists "restart and config-reload-on-restart semantics", and
   the plan describes the exact flow at lines 817–819: edit a scratch configuration while Talaria runs,
   observe no change, restart, observe the change. That flow is the only evidence that requirement R7's
   "no external-file watcher exists" holds. Several items restart, but none edits configuration while
   the process runs.

**What would fix it.** Add five checklist items, each with the same pass-condition shape the existing
thirty-one use, and assign each to its owning tester under D16's split. The dead-credential item
should reuse the known failure mode recorded in the repository: an authentication error after a
gateway restart is the token in `~/.talaria/credentials` going stale.

## The one unresolved product decision

**The bottom status bar's semantic colouring is settled nowhere — not in either document, not in any
issue — and the reading the specification states most explicitly fails the release's own contrast
floor in the default theme.**

The specification says two incompatible things about the same cells. Its "Connection states" table
(lines 694–700) assigns colour tokens to the connection segment's visible forms: `success` for
connected, `warning` for connecting and reconnecting, `error` for disconnected and authentication
failed — and those forms are the status bar's own, because the table names the compact `[ok] up` that
the bar mockup renders. Its bottom-bar mockup caption (line 391), meanwhile, says "primary segment
content uses `talaria.status.text`, secondary labels use `talaria.status.muted`", which assigns no
semantic colour at all. The queue-attention state has no colour column anywhere, so the `!1` marker's
colour is simply unstated; the plan's separate `queue_attention` token (see D1) is the only place that
question is even acknowledged, and the specification does not define that token.

The two readings are not equivalent, and one of them measurably fails. Refined Default is a light
theme — canvas `#F6F8FA` — whose status bar is deliberately inverted to `#24292F`. The semantic
colours are tuned against the light canvas, so on the dark bar they collapse. Recomputed from the
specification's own hexadecimal values with the same formula that reproduced all 336 published cells:

| Foreground on `talaria.status.background` | Refined Default | Dark Green Terminal | Neutral Dark | Accessible High Contrast |
| --- | ---: | ---: | ---: | ---: |
| `talaria.success` `#1A7F37` | **2.88:1** | 13.15:1 | 9.85:1 | 16.24:1 |
| `talaria.warning` `#8A5A00` | **2.47:1** | 14.08:1 | 11.09:1 | 15.14:1 |
| `talaria.error` `#CF222E` | **2.74:1** | 8.05:1 | 8.08:1 | 7.57:1 |
| `talaria.accent` `#087F5B` | **2.93:1** | 15.31:1 | 9.23:1 | 15.64:1 |
| `talaria.primary` `#0969DA` | **2.82:1** | 13.15:1 | 9.12:1 | 9.46:1 |

The specification's own floor is 4.5:1 for all terminal text, with no font-size exemption. Every
Refined Default figure above is below it, by a wide margin at 2.47:1. The other three themes pass
comfortably, so this is specifically a light-theme-with-inverted-bar problem.

The specification's contrast contract also declares these pairs out of bounds. Line 244 says the
tables "are exhaustive for allowed foreground/background combinations" and line 250 says "introducing
any other pair requires adding it to this table and to the automated contrast test before release".
None of the five pairs above appears in either table. So the specification simultaneously mandates
these pairs in one section and forbids them in another.

No issue resolves it. Issue #109 requires that "every color-only status signal has a redundant
non-color signal" and that the contrast audit passes for all four themes — which is what makes this
blocking rather than cosmetic, because both halves of that criterion are in play. Issue #106 says
"per-segment color customization beyond the theme tokens" is out of scope, which forecloses the escape
of giving the bar its own private colours without a token.

**Three defensible resolutions, none of which either document has taken.** The authors should pick
one, and it is a product call rather than an editorial one:

1. **The bar carries no semantic colour.** Connection and queue state are signalled by the ASCII forms
   alone — `[ok]`, `[..]`, `[~]`, `[x]`, `[!]`, `!N` — rendered in `talaria.status.text` and
   `talaria.status.muted`, both of which already measure 8.47:1 or better in every theme. Cheapest,
   fully compliant, and consistent with the release's own "non-color signaling" thesis; the cost is
   that a glance no longer distinguishes connected from disconnected by colour.
2. **Add four bar-scoped semantic tokens** — `talaria.status.success`, `.warning`, `.error`, and
   `.attention` — with values chosen against `talaria.status.background` per theme, add their four
   rows to the contrast table and to the automated contrast test, and give Refined Default lighter
   values than its canvas-tuned ones. Keeps the colour channel; costs four tokens across four themes
   and four new measured rows.
3. **Make Refined Default's status background light** so the canvas-tuned semantic colours apply
   directly. Cheapest in tokens, but it discards the inverted-bar look the mockups draw and would
   require remeasuring `status text / status`, `status muted / status`, and `status separator / status`
   for that theme.

Whichever is chosen, the specification's "Connection states" colour column, its bottom-bar mockup
caption, and its contrast tables must end up saying the same thing, and unit U6's contrast helper must
cover whatever pairs survive.

## Optional improvements

These will not be repaired before implementation. They are recorded so the run owner can decide
whether any is cheap enough to fold into a lane it already touches.

### D18 (`P2`) — Two checklist items name no method a tester could execute

Specification, acceptance checklist items 7 and 25. Item 7 asks the tester to confirm that "measured
minimums are at least 6.22:1 text and 6.08:1 non-text" — a human cannot measure a contrast ratio by
eye from a terminal. Item 25 asks for a monochrome capture of six transcript groups without saying how
monochrome is produced. Fix: for item 7, name the unit U6 automated contrast test's output as the
receipt for the numeric half and keep the visual half a visual judgement; for item 25, name the method,
such as launching under a `TERM` value without colour support or converting the screenshot to
greyscale.

### D19 (`P2`) — The `tests/config` supersession is not carried into closeout

Plan, "Repository grounding" (line 89). The plan correctly refuses to create a parallel `tests/config/`
directory, and its unit verification commands correctly use `tests/test_config.py`. But issues #104
and #106 both name `uv run pytest tests/config` inside their own Verification blocks, and that
directory does not exist in the tree — anyone closing those issues by running the issue's own command
gets an error and cannot honestly tick the box. Fix: add one line to unit U8 recording that the issues'
`tests/config` path is superseded by `tests/test_config.py`, so the closeout has a runnable command.

### D20 (`P2`) — The per-child review lens set is unstated, and lens names are prose rather than roster identifiers

Plan, "Saga Code Review lenses". The plan says "each child receives focused review before integration,
and the assembled candidate receives one review roster with exactly these seven lenses" — but never
says which lenses the per-child reviews run. A worker must invent whether a child gets the four
always-on lenses or all seven. Separately, the plan names the lenses in prose ("Architecture",
"API/config contract", "Terminal usability/accessibility", "Documentation") while the approval widget
presents the roster's own identifiers. Fix: state the per-child set explicitly, and add the roster
identifiers in parentheses — `architecture-maintainability`, `correctness`, `security`, `testing`,
`api-contract`, `accessibility-human-usability`, `documentation-clarity`.

### D21 (`P2`) — The example import report's fallback count reads as contradicting the fallback-only table

Specification, "Example report shape" (line 679) against "Tokens with no Visual Studio Code source"
(line 655). The example reports "37 source tokens, 17 fallbacks", which sums to the registry's 54 and
is internally correct — the extra three are tokens whose listed source happened to be absent in that
fixture, which line 674 does allow. But issue #105 requires counts to be asserted, and a reader
skimming for the assertable number sees both 14 and 17. Fix: one clause in the example, such as
"17 fallbacks (the 14 always-fallback tokens plus 3 whose sources this file omits)".

### D22 (`P3`) — Gateway ownership of `/status` is asserted without a probe

Plan, "Command and key ownership" (line 151): "Hermes already owns `/status`, and Talaria must not
shadow a working gateway command". The repository's precedent for that class of claim is a live probe —
`talaria/domain/commands.py:376` records `/model` being verified against 114 catalogue names on
2026-08-06. No such probe is cited for `/status`. The conclusion is safe either way, because `/bar`
avoids the collision regardless, so this costs nothing to leave. Fix: label it an assumption, or probe
it once during the run and record the result.

### D23 (`P3`) — Issue #103's "all eight children closed" criterion has no owning unit

Plan, "Integration order and the unmerged candidate", step 8. Parent #103's first acceptance criterion
is that all eight child issues are closed, verified with `gh issue list --repo infiquetra/talaria
--state open`. Unit U8 covers documentation, version, tag, and release, but issue closure appears in no
unit. Fix: add issue closure to unit U8's approach, or state that it is operator workflow outside the
plan's scope.

### D24 (`P3`) — The 120–143 band compacts although full forms fit at 138 columns and wider

Specification, "Combined breakpoint table" (line 476) against "Status-bar truncation and drop contract"
(line 512). At default caps the seven full forms total 132 cells and six separators add 6, so the full
row fits in 138 columns — yet the table prescribes compact forms for everything below 144. Fixed bands
are a deliberate determinism win and the acceptance walk depends on them, so this is defensible as
written. Fix, if wanted: one sentence saying the band boundary is set at 144 for determinism rather
than at the 138-column fitting boundary.

## Rubric review notes

**The issue-phase rubrics find no defect that the pairwise contradictions do not already cover.**

`spec_fidelity` — clean. Both documents cite parent #103 and the children they serve, both inherit the
binding constraints (ADR-0002's import boundary, ADR-0005's presentation layer, ADR-0006's bounded
rendering, restart-to-apply, read-only diffs, no settings application, Linux deferred), and I found no
requirement smuggled past a stated non-goal. The one term-of-art drift is D1's token vocabulary, filed
there.

`acceptance_criteria_clarity` — strong on both sides. Every one of the specification's 31 checklist
items carries an explicit pass condition with a nameable artifact, which is the rubric's strongest
form; the plan's per-unit verification blocks name exact commands. The two weak items are D18.

`devils_advocate_issue` — clean on scope. Neither document bundles adjacent work, neither pre-decides
implementation at a level the lanes should own, and both keep the release to the eight approved
children. The failure-mode discussion is unusually good: the plan's KTD10 candidate-invalidation rule
and its failure-routing step 7 answer "what if the change is wrong" concretely.

`context_completeness` (conditional, fired — non-trivial repository with established conventions) —
this is the plan's strongest dimension. It names files, cites precedents by line, names the test files
to update, and states which patterns to reuse. Verified: every file and test path it names exists.

`prerequisite_mapping` (conditional, fired — eight in-flight issues in one repository) — clean. The
plan reproduces the parent's dependency graph without adding edges, separates logical dependency from
integration order explicitly, and names the rebase points for the two soft dependencies.

## Residual risk from limited evidence

**Two claims in these documents could not be verified from the repository and remain assumptions.**

Whether the Hermes gateway owns the `/theme` or `/status` command names is unprobed (D22, and it bears
on D8). Settling it needs a live gateway session, which this review did not have.

Whether `F11` and `ctrl+b` are actually delivered to Talaria by the operator's terminal on macOS is
unprobed by either document. The plan is honest about this — it describes `F11` as "an alias whose
desktop delivery still requires #110's real-terminal evidence" — and the specification lists the
`ctrl+b` question among its non-blocking open questions. Both are correctly deferred to acceptance;
neither is a finding. Note only that `ctrl+b` is bound nowhere in the current tree, so nothing in the
repository contradicts either choice today.
