# Model roles

The Model roles page shows and changes every model a Hermes profile uses: the main model, the
fallback chain, the helper models Hermes uses for side tasks, the subagent route and the Mixture
of Agents presets. It writes the profile's own defaults in Hermes, through the same dashboard
routes Hermes Desktop uses, so nothing needs a YAML edit. It never changes a running session's
model: use `/model` for that.

## Opening the page

Type `/models` in the composer, or choose the "Model roles" row on the `/config` settings page.
`/models` takes no argument; with one, Talaria answers `Usage: /models` and opens nothing.

The page edits the profile Talaria is attached to; its title reads `Model roles - profile
<name>`. The tab row `1 Main  2 Fallbacks  3 Helpers  4 Subagents  5 Mixture of Agents` names
the five sections, and a section Hermes could not read adds ` (stale)` to its tab.

Every section loads on its own, so one failure leaves the others usable. The model list behind
the picker comes from Hermes and takes about a second; until it arrives the picker says
`loading models...`.

Without a connection to the gateway the page says `Model roles need the Hermes gateway, and
Talaria is not connected.`; Ctrl+R tries again and Esc closes.

## Main

Main shows the profile's main model and provider, its context window, what the model can do
(tools, images, reasoning, maximum output, family), and the profile's default reasoning effort.

- Enter opens the picker: choose a provider, then a model.
- `c` sets a context-window override; `0` or empty means auto-detect.
- `e` sets the default effort (`inherit`, or `none` to `ultra`).

Hermes may ask before it switches to an expensive model. Talaria shows Hermes's message and
`Use this model anyway? y/n`; `y` sends the change again with your confirmation and `n` keeps
it pending.

After a main-model change, Hermes lists the helpers that are still pinned to another provider.
Talaria then offers to make them follow the new main model. That is a separate change to
Helpers, with its own review.

When Talaria's own saved default model is set (from `/model`), Main says that Talaria starts new
sessions with it, which overrides the profile's main model.

## Fallbacks

Fallbacks lists the chain Hermes tries, in order, when the main model fails:
`1. <model> (via <provider>)`. Markers show an entry's own endpoint (`[endpoint]`), its own key
setting (`[key set]`), an entry equal to the main model (`same as main model`), and an entry
Hermes will skip because an earlier one is the same (`duplicate: Hermes skips it`).

- `a` adds an entry through the picker; `x` or Delete removes one.
- `K` and `J` (or Alt+Up and Alt+Down) move an entry up or down.

Talaria keeps every key of an entry, its endpoint and key settings included, even when you move
or change it. An entry written as a plain `provider/model` string is ignored by Hermes; Talaria
shows it marked `(ignored by Hermes until saved)`, and saving turns it into a real entry, which
the review says. Entries under the older `fallback_model` key are shown below a rule, read-only;
change them with the `hermes fallback` command.

## Helpers

Helpers are the auxiliary models Hermes uses for side tasks such as image analysis, context
compaction and session titles. The twelve built-in slots are listed in Hermes's order, from
`Vision` to `Curator`, followed by any slots plugins add.

A slot either `follows main model` or names its own `<model> (via <provider>)`, with its own
effort or `effort: inherited`.

- Enter picks a model for the slot.
- `e` sets the slot's effort.
- `i` puts the slot back to following the main model, with inherited effort.

A slot pinned to a provider other than the main one is marked `!`. `R` makes every marked slot
follow the main model again; only those slots change, never the rest.

Hermes may ask before pinning an expensive model to a slot. If you answer `n`, or a slot fails,
Talaria stops and says which slots were saved and which were not.

## Subagents

Subagents shows the route the profile's delegated tasks use. The route either follows the main
model or names its own provider and model, and it may use a direct endpoint. The section also
shows the subagents' effort and their fallback chain.

- Enter picks a route.
- `e` sets the effort.
- `i` goes back to inheriting. Leaving a direct endpoint also clears its address, key and API
  mode, so a later endpoint never inherits them.
- `m` cycles the fallback mode: `inherits the main chain`, `no fallback`, or `own chain`. An own
  chain is edited like Fallbacks.

Other subagent limits (concurrency, depth, timeouts) are on the `/config` settings page.

## Mixture of Agents

Mixture of Agents presets name several reference models that advise and one aggregator model
that acts. The left pane lists the presets (`*` marks the default, `(off)` a disabled one); the
right pane shows the selected preset. Left and Right move between them.

The detail pane covers four things:

- the references and the aggregator;
- how often the advisors run: once per user turn, every tool step, or every N tool steps;
- what happens when an advisor fails;
- the advisor timeout and the temperatures.

The active preset and the privacy filter are shown read-only.

- `a` in the preset list adds a preset; `D` makes the selected one the default.
- Space turns a preset or a reference on or off.
- `x` removes a preset (after `y/n`) or a reference. A profile keeps at least one preset.

Talaria checks a preset the way Hermes does, with Hermes's wording. An incomplete preset is
refused before anything is sent, for example `preset 'name' reference 1: provider and model are
required`.

When the profile has no presets of its own, Hermes shows its example preset. Saving writes that
example into the profile, which the review says, and enabled presets then appear as models.

## Keys

| Key | Action |
|---|---|
| `1` to `5`, Tab, Shift+Tab | Change section |
| Up, Down, Home, End, PageUp, PageDown | Move the selection |
| Left, Right | Move between the preset list and the detail pane (Mixture of Agents) |
| Enter | Open the picker on a route row, or edit the selected detail |
| `e` | Effort |
| `c` | Context-window override (Main) |
| `i` | Back to inherit (Helpers, Subagents) |
| `a` | Add an entry, a preset or a reference |
| `x`, Delete | Remove the selected entry, preset or reference |
| `K`, `J`, Alt+Up, Alt+Down | Move a fallback entry |
| Space | Turn a preset or reference on or off |
| `D` | Make the selected preset the default |
| `m` | Cycle the subagent fallback mode |
| `R` | Make the marked helpers follow the main model |
| Ctrl+S | Review the section's changes and apply them |
| `u` | Discard the section's changes |
| Ctrl+R | Reload every section |
| Esc | Close the page |
| `?` | List the keys |

In the picker, typing filters the list, Enter chooses, Backspace on an empty filter goes back to
the providers, Esc cancels, and Ctrl+R refreshes the model list. A provider that is not set up is
dimmed and says `not set up: add it under Providers`. A stored model missing from the list is
shown with `(not in the model list)`, never replaced.

Only one section holds unapplied changes at a time. Changing section, reloading or closing with
changes pending asks `a` apply, `d` discard, or Esc stay.

## When changes apply

Ctrl+S shows a review of exactly the keys that will change, before and after. Enter applies it.
Talaria first reads the section again. If Hermes's values changed since the page opened, it says
`Changed outside Talaria since you opened this page`, shows the new values and writes nothing.
After a write it reads the values back, so the page always shows what Hermes holds.

A main-model change applies to new sessions; running sessions keep their model. The other
sections also say `Applies to new sessions.`, with the note `Sessions already running may keep
their current setting.`, because Hermes does not report whether a running session picks them up.

Secrets are never shown. A key stored in an entry shows as `[key set]` and is sent back
unchanged.

## Errors

- `Hermes did not answer. Ctrl+R to retry.`: the section keeps its last values and its tab is
  marked `(stale)`.
- `Talaria could not sign in to the dashboard API.`: run the doctor.
- `This Hermes has no <route>.`: this Hermes version lacks the route, and the section is
  read-only.
- `Hermes refused this change:` or `Hermes found a problem:`, followed by Hermes's reason.
- `Hermes needs a restart before it can change models.`: run the doctor to restart the gateway.
  Talaria never restarts it from this page.
- `The change was not sent:`: nothing was written, and your changes stay pending.
- `The result of the last change is unknown. These are the values Hermes holds now.`: the
  connection dropped after the request was sent. Talaria read the section again instead of
  assuming success.
