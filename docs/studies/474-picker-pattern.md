# Picker pattern design study

Talaria needs a model picker and a provider setup dialog that remain easy to scan as the catalogue grows. This study recommends **Option A: search first, grouped results, details for the selected row**. It proposes presentation and navigation only. The operator must record a decision on #474 before build cards are filed.

Study date: 2026-11-02. Two-day timebox. Base: `3f6ea58ed8c3da9742b64a8e59a4d9c91e01a2bd`. Prototype branch: `study/474-picker-pattern`. The prototype is an offline example, separate from application rendering and input handling. No gateway was connected, queried or started.

## Evidence and size

The issue and its coordination comment (issuecomment-6570081988) are the authority. The screen sizes follow #473. This study leaves #470's investigation open and names surfaces for #475 below.

Source at the base revision:

- `crates/talaria-tui/src/model_picker.rs`: model rows, filtering, badges and confirmation rendering.
- `crates/talaria-core/src/models/`: provider groups, favourites, current selection, matching, effort requests and switching behavior. `catalog.rs:65-181` already groups favourites, authenticated providers and providers needing setup. The problem is the long, crowded presentation, rather than a complete absence of grouping.
- `crates/talaria-tui/src/providers/` and `docs/terminal-ui.md:143-184`: Keys, Accounts and Endpoints, secret editors, checks, save/read-back, removal confirmation and uncertain results.
- `crates/talaria-tui/src/model_roles/route_picker.rs`, with `crates/talaria-core/src/model_roles/picker.rs`: a provider-then-model route editor, not a session switch.
- `crates/hermes-contract/PINNED`: Hermes v0.22.4, commit `4d04303818b61bf3aa09278af48356315515965b`. Generated `ModelOptionProvider` and `ModelCapabilities` in `crates/hermes-contract/src/generated/schemas.rs:10339-10459` offer authentication, pricing, availability and reasoning flags. They do not supply a context window or an exact effort-level list.
- At that pinned Hermes revision, `hermes_cli/inventory.py:539-583` defaults `reasoning` to true when metadata is silent. It deliberately does not forward `supported_efforts`. A true flag therefore cannot prove exact effort support. No investigation of outgoing provider requests was performed here.

Static files were read as data only, without importing or executing Hermes. Values below count cached entries, not models returned by a running gateway and not unique models across providers.

| On-disk input | Observation | Meaning for the design |
| --- | --- | --- |
| `models_dev_cache.json` | 235 provider records; 8,940 model-map entries | The discovery catalogue is much larger than the configured picker; never expand it into the initial list automatically. |
| `provider_models_cache.json` | 19 provider caches; 418 model entries | A realistic picker needs search and visible scrolling, even with only a subset of providers. |
| `profiles/fixture/config.yaml` | One selected provider/model and one fallback provider; two distinct provider references | Selection and fallback configuration are not evidence of authentication or present availability. No credential values were inspected for this study. |
| Pinned Hermes `hermes_cli/models_catalog_static.py:330-369` | 39 built-in canonical provider rows before plugin additions and listing filters | The issue's 32-provider workload is a design target, not a fixed implementation count. Counts must be derived from each returned catalogue. |

Cached model-entry counts by provider:

| Provider cache | Entries | Provider cache | Entries |
| --- | ---: | --- | ---: |
| Alibaba | 15 | Alibaba China | 15 |
| Alibaba Coding Plan | 10 | Alibaba China Coding Plan | 10 |
| Alibaba Token Plan | 14 | Anthropic | 17 |
| Copilot | 25 | Copilot ACP (Agent Client Protocol) | 26 |
| DeepSeek | 2 | Gemini | 17 |
| Meta AI | 1 | Ollama Cloud | 24 |
| OpenAI API | 22 | OpenAI Codex | 15 |
| OpenCode Free | 7 | OpenCode Go | 50 |
| OpenCode Zen | 128 | Vertex | 11 |
| xAI OAuth (Open Authorization sign-in) | 9 | **Total** | **418** |

Configured-versus-unconfigured counts for the served fixture profile remain **unknown**. Caches can outlive credentials; a selected provider can be unauthenticated; credential presence does not prove it works. Reporting 19 configured and 13 unconfigured would invent a result. The prototype uses an explicitly synthetic 32-provider workload and illustrative readiness. Runtime counts, when implemented, come from Hermes's catalogue and setup inventories, with separate unknown states for failed reads. The broad discovery cache must not be treated as a list of authenticated routes.

## Two options and recommendation

**Option A — search-first grouped list (recommended).** A visible filter, a short row of scope chips, one results list with non-selectable group headers, a selected-row detail area and stable footer hints. At 80x24 the detail area is below the list. At 120x40 it is beside the list. There is one browse focus; typing always searches. Provider groups are labels, not dozens of tabs.

**Option B — navigation rail.** Models have a provider rail; Providers has a method rail. A second area holds results and a third shows details. This gives a useful provider inventory when someone knows the provider first. At 80 columns the rail takes 19 columns from the model names and adds a focus transition. For Models, typing searches the selected provider; selecting All searches across providers and prefixes provider identity on each result. For Providers, typing searches the selected method, with All as the default. Ctrl+P focuses the rail, arrows choose a scope, and Tab returns to results. Both rails scroll rather than squeezing every entry on screen.

Recommend A because the common task is to name a model and choose it immediately. It keeps long names readable at 80 columns, lets the same search discover setup providers, and avoids navigating a large provider rail before searching. It needs no model-use history or new catalogue data. B is viable if provider-first browsing proves more important during operator review, but costs horizontal space and more focus management.

Both are temporary full-screen picker overlays under the existing terminal ownership rule. They work with #473's optional workspace and menu off or on. Opening a picker temporarily owns the view; Escape restores the previous view and composer draft. No persistent sidebar or menu is added by this study.

## Answers to the six questions

### 1. How is the model list organized?

Use provider groups within one scrollable list, with a short Starred section first, configured provider groups next, and Needs setup provider stubs last. A starred route appears once: exclude its duplicate from the provider group while the Starred section is visible. Keep favourite order and Hermes provider/model order; do not silently rank by price or popularity. Identify a route by provider plus model, so the same model name on two providers remains distinct. The current route gets a separate marker from keyboard selection.

Scopes are `All`, `Starred`, `Configured`, `Set up`. All is the initial scope, selected at the current route when present. Configured means catalogue-authenticated, not a verified working credential. Needs setup rows do not expand into cached models. There are no collapsible groups initially: search and scopes already reduce the list, and collapse state adds another way to hide a match. Recent-first ordering is deferred because no authoritative recent-use list is available in this interface.

At 80x24, the outer border leaves 78x22 content cells: three rows for filter/scopes/count, ten for the results viewport, seven for a detail separator and six summary lines, and two footer rows. One result-viewport row reports scrolling. At 120x40, the body is split 60/40 between results and detail. Use side details only at widths of at least 112 and heights of at least 28. Below that, use bottom details. Below 60x16, show a compact list with details on demand; below 40x10, show a size message with Escape available. The two required sizes are rendered; smaller adaptations are proposed behavior, not demonstrated here.

### 2. How does search work with that organization?

Keep the existing whitespace-separated, case-insensitive matching over provider display name, provider slug and model identifier; every term must match. In All, typing `openai reasoner` finds matching routes across the catalogue; in another scope, it searches that scope only. Show the scope and the number of matching models/providers. Matching a provider name includes all its children; a model-only match keeps its provider header. A setup stub matches provider identity, but never claims that an unconfigured cached model is selectable.

No query language, fuzzy ranking or separate search mode is needed. Backspace edits; Ctrl+U clears. Keep the selected route by identity when it still matches; otherwise select the first selectable result. Headers are skipped. Scope changes preserve the filter, make the active scope visible, and offer `No matches in Starred; Tab for All` rather than implying the catalogue is empty. Refresh preserves filter and selection identity, not row position. The filter matches the full identifier even when its row is truncated.

### 3. What appears on a row, and what waits for details?

Rows show the model identifier, distinct star/current/selection markers, and one short status badge. Priority is unavailable or rate-limited, then controls not offered by the catalogue, then effort unknown; an optional fast-capability badge is secondary. The prototype shows the priority badge only. A provider name is in the group header, and is repeated on starred or cross-provider results. No row displays a price column or a context column at 80 columns.

Details show the full provider/model identity, availability reason, requested effort, support/evidence, applied state, and known pricing with units supplied by Hermes. Usage, limit reset and partial-catalogue counts belong with their provider details/header. Never invent context size: the current contract does not supply it. Show `Context: unknown`; omit other unavailable optional values or label them unknown. Price is an estimate only when supplied, not an assertion of plan eligibility. Keep the provider's `showing N of total` warning when the server returns a partial catalogue; search covers loaded entries, not undiscovered models.

Truncate the model label at a grapheme boundary with an ellipsis; reserve the status badge before cutting the label. The full identifier must be reachable with F2 in a scrollable detail view at both sizes. The six-line summary is not a substitute for that full detail view. The prototype includes long and double-width model labels to exercise cell layout. Only the prototype's static strings are rendered; an implementation must keep the existing literal-text sanitization boundary for gateway data.

### 4. How does effort stay truthful before and after #470?

Separate **requested setting**, **route support**, and **applied value**. A session value such as high is a request, not proof the provider received or accepted it. A model's self-description and the profile's saved value prove neither. The current route and a highlighted candidate must not borrow each other's effort evidence.

Before #470 reports, show `Requested: high`, `Support: unverified for this route`, `Applied: unknown`. For what the current picker calls a model without effort levels, use the badge `not offered` and detail `Catalogue: reasoning not advertised` when `reasoning=false`. That flag does not enumerate levels or prove that a previous high request was ignored. Keep the request visible as a retained session setting; disable effort edits on that row, as today. The model itself remains selectable. A missing capability flag, or a true flag whose source may be the default, shows `effort ?`, never `supports high`. The proposed `not offered` wording describes catalogue controls, not observed provider handling.

The same detail slots accommodate #470's result without changing layout:

| Finding for this provider/model route | Row badge | Detail wording | Effort control |
| --- | --- | --- | --- |
| Still unknown | `effort ?` | `Requested: high`; `Support: unverified`; `Applied: unknown` | Existing request ladder where offered; clearly requests only, never labelled supported levels. |
| Provider ignores the setting | `ignored` | `Requested: high (retained)`; `Handling: ignored on this route`; evidence source | Disable route-specific editing; do not erase a session/default setting needed by other routes. |
| Provider accepts only a coarse tier | `coarse` | `Requested: high`; `Mapping: high -> reasoning on` (only when established); `Applied: unknown` until a matching receipt | Offer the documented tier choices only when supplied authoritatively; no guessed thresholds. |
| Provider accepts exact levels | `exact` | `Requested: high`; `Supported: low, medium, high` (only when known); evidence source | Offer that route's known levels. Preserve distinctions such as reasoning cannot be turned off. |
| Provider rejects an unsupported value | `unsupported` | Name the requested value and refusal; retain the old selection/settings | No silent remap or fallback; explain before another request. |
| Metadata unavailable | `effort ?` | `Support: could not be read`; `Applied: unknown` | Do not turn a failed read into absent controls or exact support. |

These are possible findings, not conclusions about any named provider. A source finding can establish handling behavior; it does not establish the applied value of a particular live turn. Show `Applied: high` only with evidence for that same route and request, and distinguish request observed from provider acceptance. If no such receipt is available, retain `Applied: unknown` and show the known handling rule separately. A prior observation is labelled prior and is invalidated by route/settings changes. A deferred change reads `Requested: high; pending turn end`; an uncertain write reads `Outcome unknown; reloading`, never success.

When #470 reports, attach its cited route-specific handling/mapping to these fields. If this cannot be represented by the current contract, mark it unknown until a separately approved contract change supplies it; do not hardcode a provider-name heuristic or infer exact levels from the true reasoning flag. This study does not inspect provider traffic or implement a new effort protocol.

### 5. How does Providers use the pattern for 32 providers and setup?

Use the same frame, filter, group headers, selected row, bottom/side details and footer. Its short scopes are `All`, `Keys`, `Accounts`, `Endpoints`, so the existing direct commands retain an obvious destination. All is a provider overview; group catalogue-authenticated providers first, setup-needed providers next, and user-defined endpoints last, with Add an endpoint as an action row. Keys and Accounts list only methods actually offered for each provider. Endpoints lists endpoint identities and Add, not 32 copies of the same form. Merge inventories by provider identity, never by display name; partial inventory failures remain unknown. Providers with several methods get one row in All and show their methods in details.

The displayed provider count excludes custom endpoint/action rows and is computed dynamically. Key-method rows, account rows and model-provider rows can have different counts. Support the issue's 32-provider workload and the larger static registry without adding a tab per provider. Use the existing Hermes order within each group. Status text distinguishes `listed ready`, `key saved`, `needs setup`, `checking`, `unknown`, and `offline`. Listed ready means catalogue-authenticated; a saved key can be stale and does not prove present reachability. The mock status assignments are illustrative.

Enter opens the selected provider's method chooser, or opens its only method directly. `/providers keys`, `/providers accounts` and `/providers endpoints` open the matching method scope. Entry from a model setup stub retains that provider and returns to the model search/filter/selection when the user backs out; successful read-back can refresh availability, but must not silently switch models. After setup, return with that provider selected so choosing a model remains an explicit action.

Setup uses the existing cards, presented as named steps rather than unexplained list entries:

- **API key:** Enter opens an empty masked editor; paste; Enter checks and saves on success, then reads back. A rejected value remains for correction. If unreachable, a second explicit Enter saves unchecked; editing clears that confirmation. Escape cancels and clears the new secret, leaving earlier saved credentials intact. Credential values and previews stay out of summary/details; removal still names shared-credential effects before confirmation.
- **Account sign-in:** choose a supported method; start the device flow; show verification instructions and expiry; open/copy only on the card's explicit keys; wait; show approved/denied/expired/error distinctly. External sign-in remains instructions for a separate terminal. Escape cancels following the current method's semantics. No raw account token belongs in details or captures.
- **Endpoint:** Add or edit; name, base URL, API mode, model, optional context and masked key; Ctrl+T tests, Ctrl+S saves; preserve existing keep/clear-key semantics and the offline-save option. A resolved address is reviewed before save. Deletion needs confirmation. Endpoint saves do not change the profile's default model.

Browse letters always filter; `o` and `c` act only inside a card that offers open/copy. Tab changes scopes on the browse list and fields/methods inside the relevant form. One write runs through read-back before another starts. Disconnected changes are disabled; a lost reply triggers a read and an uncertain result, not an automatic repeat.

### 6. Does the model-roles route picker share the pattern?

Share the visual pattern later, while preserving its role-editing purpose and provider-then-model steps. The title names the role and route field; Enter stages a route choice, not a session model switch. Keep its current exclusion of the virtual Mixture of Agents route, authentication requirements, model-list loading/refusal states, and the warning for a stored model that is no longer listed. Its parent editor remains responsible for review/save and unsaved changes.

It can reuse filter presentation, headers, selection, full-identity details, status and footer styling. It should eventually use the same matching semantics where applicable, but no behavioural migration is made by this study. Do not add the session effort control, session-save scope or automatic default changes to route selection. The route editor can show effort metadata as information only when relevant and authoritative. Session picker and command palette can later reuse visual elements where useful; their recency, command actions and keys require their own review, not a blanket migration.

## Keyboard paths

| Task | Proposed path |
| --- | --- |
| Open models | `/model` then Enter; the filter is ready immediately. |
| Choose current result | Enter. |
| Find and choose | Type a provider/model fragment; arrows if necessary; Enter. No extra search-focus key. |
| Move | Up/Down, PageUp/PageDown, Home/End; skip group headers and keep selection visible. |
| Change scope | Tab / Shift+Tab on the browse list. Model Left/Right retains its existing requested-effort job where offered. |
| Read full identity/evidence | F2 opens a scrollable detail layer; Escape returns to the same row. It never confirms a change. |
| Star / saved scope | Existing Ctrl+F / Ctrl+G in Models. Title always distinguishes this session from this session and new sessions. |
| Open providers | `/providers` then Enter; type provider name; Enter for setup. A model setup stub reaches the same place directly. |
| Clear / cancel | Ctrl+U clears search. Escape closes the innermost card/details first, then clears a non-empty filter, then closes the overlay. With an empty filter, one Escape closes. |
| Reload | Existing Ctrl+R; preserve scope, query and route identity. |

All proposed new browse bindings, including F2 and model scope navigation, must be configurable and checked against the effective keymap before implementation. F2 avoids the terminal equivalence between Ctrl+I and Tab. Existing provider form bindings retain their context-specific jobs. No bare letter is reserved from search, and no global shortcut handles a secret editor's input. Switching still honors existing confirmation, deferred changes, and uncertain-outcome behavior. The offline renderer does not implement these keys.

## Empty, loading and error states

Use the same message area and recovery placement in both dialogs:

| State | Message and recovery |
| --- | --- |
| Initial loading | `Loading models…` or `Loading providers…`; footer keeps Escape; no invented rows or selectable placeholder. |
| Refresh | Retain rows and selection with `Refreshing…`; mark stale data when the refresh fails. |
| No matches | Name the query and scope; Ctrl+U clears, Tab changes scope, Escape clears then closes. No active Enter action. |
| No starred models | `No starred models. Tab for All; Ctrl+F stars a model.` |
| No configured providers | Show setup stubs, not an empty unexplained model picker. Enter opens setup. |
| Provider has no models | `No models listed for this provider`; reload or inspect setup; do not count discovery cache entries as selectable. |
| First load failed | `Could not load models/providers`; Ctrl+R retries, Escape closes. Details show no action. |
| Partial load failed | Label the failed inventory/category unknown; preserve other data, not a false unconfigured count. |
| Offline or lost write reply | Show offline or outcome unknown; keep selection/draft; disable new changes while offline; reload after reconnect. Never resend automatically. |
| Unavailable model | Keep its badge/reason visible; Enter explains without claiming a switch. |

Visible text and markers carry meaning in monochrome; colour is supplementary. Errors do not replace the only cancel/retry hints. A longer error or provider warning is available in full details rather than silently clipped away. Mouse use is optional and capture stays off by default.

## Element catalogue for #475

These are semantic element names for the theme study to map into its chosen naming convention, not an implemented theme API or a required colour palette. Both dialogs and the later route picker use the shared elements.

| Element | Required distinctions |
| --- | --- |
| Picker surface, border, title | Overlay background; border; title and profile/scope qualifier. |
| Filter input | Prompt, typed query, placeholder, cursor and focus indicator. |
| Scope chips | Active/inactive/focused chip; optional count; overflow indicator. No provider-per-tab styling required by A. |
| Group header | Provider/status name, count, separator; non-selectable. |
| Result row | Normal, selected, unavailable/disabled; match highlight; full text in details. Selection must remain distinguishable from current route. |
| Row markers | Current route, favourite/star and selected pointer; textual fallback for each. |
| Capability and effort badges | Fast capability; catalogue controls not offered; unverified; ignored; coarse; exact; unsupported. Never encode unknown as disabled or no support. |
| Availability badge | Unavailable, limited, setup-needed, listed ready, saved credential, unknown. |
| Detail pane | Divider/title, identity, field label/value, missing value, evidence/provenance, previous observation, warning. Bottom and side layouts share tokens. |
| Status/banner | Loading, refreshing, stale, informational, warning, error, pending and uncertain outcome. |
| Scroll indicator | More-above/below and position/count; not a progress indicator. |
| Footer hints | Key text, action text, disabled hint, context qualifier. |
| Setup card | Surface/border/title; method chooser; step name/number; instruction and waiting state. |
| Setup field | Label, placeholder, value, focused field, masked secret, validation error; secret values never appear as normal detail text. |
| Confirmation/action | Primary, cancel, destructive and disabled action; destructive warning; default cancel selection. |
| Option B only | Provider/method rail, selected item, category heading and divider; not required if A is adopted. |

## Prototype and verification

`crates/talaria-tui/examples/picker_study.rs` constructs Ratatui layouts and draws only to `ratatui::backend::TestBackend`. It renders both alternatives at both required sizes for both dialogs, plus recommended no-levels, empty, stale/error and masked-key states at 80x24. Its fixtures contain no credentials or local configuration. No application modules, transport, input loop, terminal modes or setup operations are invoked.

The focused example tests check dimensions and borders; visible action hints and effort truth at both sizes; preserved status under a long double-width label; no-levels wording; fixture counts; and retained rows on refresh failure. These are layout checks, not interaction or live-provider acceptance. Full details scrolling, keys, selection/search transitions, setup flows and the reduced-size variants remain design proposals. No production behavior changed, so the full workspace suite, full workspace lint and coverage run are outside this study's verification scope.

Verification results and the generated captures follow. The complete study and captures are stored together on the prototype branch. No PR, build card, merge, release or deployment is part of this delivery.

Verified on the final prototype source:

- `cargo fmt --check`: passed.
- `cargo test --locked -p talaria-tui --example picker_study`: 3 passed, 0 failed.
- `cargo clippy --locked -p talaria-tui --example picker_study -- -D warnings`: passed.
- `cargo run --locked --quiet -p talaria-tui --example picker_study`: generated all 12 captures below, including all eight required option/surface/size combinations.
- `sh tools/release/doc-scan.sh`: passed for the complete write-up. The same scanner was also run against a temporary tracked text copy of the prototype, because it normally scans only Markdown and text files. Additional checks covered machine addresses, identity literals, private repository names and attribution lines.

Cargo emitted existing manifest version-metadata warnings. A transient shared-build-directory failure occurred before tests ran; retrying without cleaning the shared directory passed. No gateway or live provider was used. The setup-card backdrop was inspected after regeneration: the result list and browse hints no longer show around the active card. The operator's design decision remains pending.

## Rendered screens

All screens below come directly from Ratatui TestBackend. Each block has exactly the named row count; trailing spaces are omitted. Names, route combinations, readiness, badges and setup state are synthetic; only the workload sizes are grounded in static counts. The two surfaces show independent illustrative states. Text captures omit colour and focus styling.

### Option A / Models / 80x24 / Browse

```text
┌Models / this session / profile: example──────────────────────────────────────┐
│Filter: type to search_                                                       │
│[All]  Starred  Configured  Set up     Tab changes scope                      │
│Synthetic: 418 models / 19 providers (cache-sized workload)                   │
│Starred                                                                       │
│  * Anthropic / reasoner-small                                        effort ?│
│OpenAI / configured                                                           │
│>   ● reasoner-large                                                  effort ?│
│    chat-fast                                                      not offered│
│    reasoner-lite                                                     effort ?│
│    example-model-04                                                  effort ?│
│    example-model-05                                                  effort ?│
│    example-model-06                                                  effort ?│
│More below / 9 rows in view / PgDn scrolls                                    │
│Details───────────────────────────────────────────────────────────────────────│
│Selected: OpenAI / reasoner-large                                             │
│Requested: high (session setting)                                             │
│Support: unverified for this route                                            │
│Applied: unknown; not a provider receipt                                      │
│Context: unknown / Price: unknown                                             │
│F2 full details / Left,Right request effort                                   │
│Up,Down move / Enter choose / Esc clear,close / F2 details                    │
│Ctrl+F star / Ctrl+G save scope / Ctrl+R reload / Ctrl+U clear                │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Option A / Models / 120x40 / Browse

```text
┌Models / this session / profile: example──────────────────────────────────────────────────────────────────────────────┐
│Filter: type to search_                                                                                               │
│[All]  Starred  Configured  Set up     Tab changes scope                                                              │
│Synthetic: 418 models / 19 providers (cache-sized workload)                                                           │
│Starred                                                                │Details                                       │
│  * Anthropic / reasoner-small                                 effort ?│Selected: OpenAI / reasoner-large             │
│OpenAI / configured                                                    │Requested: high (session setting)             │
│>   ● reasoner-large                                           effort ?│Support: unverified for this route            │
│    chat-fast                                               not offered│Applied: unknown; not a provider receipt      │
│    reasoner-lite                                              effort ?│Context: unknown / Price: unknown             │
│    example-model-04                                           effort ?│F2 full details / Left,Right request effort   │
│    example-model-05                                           effort ?│                                              │
│    example-model-06                                           effort ?│                                              │
│    example-model-07                                           effort ?│Full identity                                 │
│    example-model-08                                           effort ?│provider: openai                              │
│    example-model-09                                           effort ?│model: reasoner-large                         │
│    example-model-10                                           effort ?│                                              │
│    example-model-11                                           effort ?│Catalogue: reasoning advertised               │
│    example-model-12                                           effort ?│Exact levels: not supplied                    │
│    example-model-13                                           effort ?│Evidence: catalogue only                      │
│    example-model-14                                           effort ?│                                              │
│    example-model-15                                           effort ?│* starred / ● current                         │
│    example-model-16                                           effort ?│Unavailable rows explain why on Enter.        │
│    example-model-17                                           effort ?│Unknown values stay unknown.                  │
│    example-model-18                                           effort ?│                                              │
│    example-model-19                                           effort ?│                                              │
│    example-model-20                                           effort ?│                                              │
│    example-model-21                                           effort ?│                                              │
│    example-model-22                                           effort ?│                                              │
│OpenRouter / configured                                                │                                              │
│    chat-plus                                               unavailable│                                              │
│    organization/team/reasoner-with-a-long-model-name          effort ?│                                              │
│    multilingual-模型-extended                                 effort ?│                                              │
│Example provider 04 / configured                                       │                                              │
│    example-04-model-01                                        effort ?│                                              │
│    example-04-model-02                                        effort ?│                                              │
│More below / 32 rows in view / PgDn scrolls                            │                                              │
│Up,Down move / Enter choose / Esc clear,close / F2 details                                                            │
│Ctrl+F star / Ctrl+G save scope / Ctrl+R reload / Ctrl+U clear                                                        │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Option A / Providers / 80x24 / Browse

```text
┌Providers / profile: example──────────────────────────────────────────────────┐
│Filter: type to search_                                                       │
│[All]  Keys  Accounts  Endpoints      Tab changes scope                       │
│Synthetic: 32 providers / illustrative states                                 │
│Configured                                                                    │
│    Ollama Cloud                                                  listed ready│
│    Anthropic                                                     listed ready│
│    OpenAI Codex                                                  listed ready│
│Needs setup                                                                   │
│>   OpenAI                                                              no key│
│    DeepSeek                                                            no key│
│    Gemini                                                             sign in│
│    Example provider 07                                                 no key│
│More below / 9 rows in view / PgDn scrolls                                    │
│Details───────────────────────────────────────────────────────────────────────│
│Selected: OpenAI / needs setup                                                │
│Method: API key (other methods when offered)                                  │
│1 Enter to open a masked key field                                            │
│2 Check key / 3 Save / read back status                                       │
│Unreachable? Confirm save unchecked separately.                               │
│Enter set up / F2 details                                                     │
│Up,Down move / Enter set up / Esc clear,close / F2 details                    │
│Ctrl+R reload / Ctrl+U clear / Ctrl+D remove (confirmation)                   │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Option A / Providers / 120x40 / Browse

```text
┌Providers / profile: example──────────────────────────────────────────────────────────────────────────────────────────┐
│Filter: type to search_                                                                                               │
│[All]  Keys  Accounts  Endpoints      Tab changes scope                                                               │
│Synthetic: 32 providers / illustrative states                                                                         │
│Configured                                                             │Details                                       │
│    Ollama Cloud                                           listed ready│Selected: OpenAI / needs setup                │
│    Anthropic                                              listed ready│Method: API key (other methods when offered)  │
│    OpenAI Codex                                           listed ready│1 Enter to open a masked key field            │
│Needs setup                                                            │2 Check key / 3 Save / read back status       │
│>   OpenAI                                                       no key│Unreachable? Confirm save unchecked separatel…│
│    DeepSeek                                                     no key│Enter set up / F2 details                     │
│    Gemini                                                      sign in│                                              │
│    Example provider 07                                          no key│                                              │
│    Example provider 08                                          no key│Setup methods                                 │
│    Example provider 09                                          no key│[Key]  Account (if offered)                   │
│    Example provider 10                                          no key│Endpoint: separate form                       │
│    Example provider 11                                          no key│                                              │
│    Example provider 12                                          no key│Listed ready means catalogue auth.            │
│    Example provider 13                                          no key│It does not prove a key works now.            │
│    Example provider 14                                          no key│                                              │
│    Example provider 15                                          no key│No credential preview in this study.          │
│    Example provider 16                                          no key│Removal always asks first.                    │
│    Example provider 17                                          no key│                                              │
│    Example provider 18                                          no key│                                              │
│    Example provider 19                                          no key│                                              │
│    Example provider 20                                          no key│                                              │
│    Example provider 21                                          no key│                                              │
│    Example provider 22                                          no key│                                              │
│    Example provider 23                                          no key│                                              │
│    Example provider 24                                          no key│                                              │
│    Example provider 25                                          no key│                                              │
│    Example provider 26                                          no key│                                              │
│    Example provider 27                                          no key│                                              │
│    Example provider 28                                          no key│                                              │
│    Example provider 29                                          no key│                                              │
│    Example provider 30                                          no key│                                              │
│More below / 32 rows in view / PgDn scrolls                            │                                              │
│Up,Down move / Enter set up / Esc clear,close / F2 details                                                            │
│Ctrl+R reload / Ctrl+U clear / Ctrl+D remove (confirmation)                                                           │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Option B / Models / 80x24 / Browse

```text
┌Models / this session / profile: example──────────────────────────────────────┐
│Filter: type to search_                                                       │
│Provider rail / Ctrl+P focus rail / Tab return to results                     │
│Synthetic: 418 models / 19 providers (cache-sized workload)                   │
│Providers         │OpenAI / configured                                        │
│  All             │>   ● reasoner-large                               effort ?│
│> OpenAI          │    chat-fast                                   not offered│
│  Anthropic       │    reasoner-lite                                  effort ?│
│  OpenRouter      │    example-model-04                               effort ?│
│  Example 04      │    example-model-05                               effort ?│
│  +15 more        │    example-model-06                               effort ?│
│                  │    example-model-07                               effort ?│
│                  │    example-model-08                               effort ?│
│                  │More below / 9 rows in view / PgDn scrolls                 │
│                  │Details────────────────────────────────────────────────────│
│                  │Selected: OpenAI / reasoner-large                          │
│                  │Requested: high (session setting)                          │
│                  │Support: unverified for this route                         │
│                  │Applied: unknown; not a provider receipt                   │
│                  │Context: unknown / Price: unknown                          │
│                  │F2 full details / Left,Right request effort                │
│Up,Down move / Enter choose / Esc clear,close / F2 details                    │
│Ctrl+F star / Ctrl+G save scope / Ctrl+R reload / Ctrl+U clear                │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Option B / Models / 120x40 / Browse

```text
┌Models / this session / profile: example──────────────────────────────────────────────────────────────────────────────┐
│Filter: type to search_                                                                                               │
│Provider rail / Ctrl+P focus rail / Tab return to results                                                             │
│Synthetic: 418 models / 19 providers (cache-sized workload)                                                           │
│Providers         │OpenAI / configured                                        │Details                                │
│  All             │>   ● reasoner-large                               effort ?│Selected: OpenAI / reasoner-large      │
│> OpenAI          │    chat-fast                                   not offered│Requested: high (session setting)      │
│  Anthropic       │    reasoner-lite                                  effort ?│Support: unverified for this route     │
│  OpenRouter      │    example-model-04                               effort ?│Applied: unknown; not a provider recei…│
│  Example 04      │    example-model-05                               effort ?│Context: unknown / Price: unknown      │
│  +15 more        │    example-model-06                               effort ?│F2 full details / Left,Right request e…│
│                  │    example-model-07                               effort ?│                                       │
│                  │    example-model-08                               effort ?│                                       │
│                  │    example-model-09                               effort ?│Full identity                          │
│                  │    example-model-10                               effort ?│provider: openai                       │
│                  │    example-model-11                               effort ?│model: reasoner-large                  │
│                  │    example-model-12                               effort ?│                                       │
│                  │    example-model-13                               effort ?│Catalogue: reasoning advertised        │
│                  │    example-model-14                               effort ?│Exact levels: not supplied             │
│                  │    example-model-15                               effort ?│Evidence: catalogue only               │
│                  │    example-model-16                               effort ?│                                       │
│                  │    example-model-17                               effort ?│* starred / ● current                  │
│                  │    example-model-18                               effort ?│Unavailable rows explain why on Enter. │
│                  │    example-model-19                               effort ?│Unknown values stay unknown.           │
│                  │    example-model-20                               effort ?│                                       │
│                  │    example-model-21                               effort ?│                                       │
│                  │    example-model-22                               effort ?│                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │                                                           │                                       │
│                  │End of list / headers are skipped                          │                                       │
│Up,Down move / Enter choose / Esc clear,close / F2 details                                                            │
│Ctrl+F star / Ctrl+G save scope / Ctrl+R reload / Ctrl+U clear                                                        │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Option B / Providers / 80x24 / Browse

```text
┌Providers / profile: example──────────────────────────────────────────────────┐
│Filter: type to search_                                                       │
│Method rail / Ctrl+P focus rail / Tab return to results                       │
│Synthetic: 32 providers / illustrative states                                 │
│Methods           │Configured                                                 │
│> All             │    Ollama Cloud                               listed ready│
│  Keys            │    Anthropic                                  listed ready│
│  Accounts        │    OpenAI Codex                               listed ready│
│  Endpoints       │Needs setup                                                │
│                  │>   OpenAI                                           no key│
│Status            │    DeepSeek                                         no key│
│  Configured      │    Gemini                                          sign in│
│  Set up          │    Example provider 07                              no key│
│                  │More below / 9 rows in view / PgDn scrolls                 │
│                  │Details────────────────────────────────────────────────────│
│                  │Selected: OpenAI / needs setup                             │
│                  │Method: API key (other methods when offered)               │
│                  │1 Enter to open a masked key field                         │
│                  │2 Check key / 3 Save / read back status                    │
│                  │Unreachable? Confirm save unchecked separately.            │
│                  │Enter set up / F2 details                                  │
│Up,Down move / Enter set up / Esc clear,close / F2 details                    │
│Ctrl+R reload / Ctrl+U clear / Ctrl+D remove (confirmation)                   │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Option B / Providers / 120x40 / Browse

```text
┌Providers / profile: example──────────────────────────────────────────────────────────────────────────────────────────┐
│Filter: type to search_                                                                                               │
│Method rail / Ctrl+P focus rail / Tab return to results                                                               │
│Synthetic: 32 providers / illustrative states                                                                         │
│Methods           │Configured                                                 │Details                                │
│> All             │    Ollama Cloud                               listed ready│Selected: OpenAI / needs setup         │
│  Keys            │    Anthropic                                  listed ready│Method: API key (other methods when of…│
│  Accounts        │    OpenAI Codex                               listed ready│1 Enter to open a masked key field     │
│  Endpoints       │Needs setup                                                │2 Check key / 3 Save / read back status│
│                  │>   OpenAI                                           no key│Unreachable? Confirm save unchecked se…│
│Status            │    DeepSeek                                         no key│Enter set up / F2 details              │
│  Configured      │    Gemini                                          sign in│                                       │
│  Set up          │    Example provider 07                              no key│                                       │
│                  │    Example provider 08                              no key│Setup methods                          │
│                  │    Example provider 09                              no key│[Key]  Account (if offered)            │
│                  │    Example provider 10                              no key│Endpoint: separate form                │
│                  │    Example provider 11                              no key│                                       │
│                  │    Example provider 12                              no key│Listed ready means catalogue auth.     │
│                  │    Example provider 13                              no key│It does not prove a key works now.     │
│                  │    Example provider 14                              no key│                                       │
│                  │    Example provider 15                              no key│No credential preview in this study.   │
│                  │    Example provider 16                              no key│Removal always asks first.             │
│                  │    Example provider 17                              no key│                                       │
│                  │    Example provider 18                              no key│                                       │
│                  │    Example provider 19                              no key│                                       │
│                  │    Example provider 20                              no key│                                       │
│                  │    Example provider 21                              no key│                                       │
│                  │    Example provider 22                              no key│                                       │
│                  │    Example provider 23                              no key│                                       │
│                  │    Example provider 24                              no key│                                       │
│                  │    Example provider 25                              no key│                                       │
│                  │    Example provider 26                              no key│                                       │
│                  │    Example provider 27                              no key│                                       │
│                  │    Example provider 28                              no key│                                       │
│                  │    Example provider 29                              no key│                                       │
│                  │    Example provider 30                              no key│                                       │
│                  │More below / 32 rows in view / PgDn scrolls                │                                       │
│Up,Down move / Enter set up / Esc clear,close / F2 details                                                            │
│Ctrl+R reload / Ctrl+U clear / Ctrl+D remove (confirmation)                                                           │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Option A / Models / 80x24 / NoLevels

```text
┌Models / this session / profile: example──────────────────────────────────────┐
│Filter: type to search_                                                       │
│[All]  Starred  Configured  Set up     Tab changes scope                      │
│Synthetic: 418 models / 19 providers (cache-sized workload)                   │
│Starred                                                                       │
│  * Anthropic / reasoner-small                                        effort ?│
│OpenAI / configured                                                           │
│    ● reasoner-large                                                  effort ?│
│> chat-fast                                                        not offered│
│    reasoner-lite                                                     effort ?│
│    example-model-04                                                  effort ?│
│    example-model-05                                                  effort ?│
│    example-model-06                                                  effort ?│
│More below / 9 rows in view / PgDn scrolls                                    │
│Details───────────────────────────────────────────────────────────────────────│
│Selected: OpenAI / chat-fast                                                  │
│Requested: high (session setting)                                             │
│Catalogue: reasoning not advertised                                           │
│Applied: unknown; setting retained                                            │
│No effort edit here. Model remains selectable.                                │
│F2 full details / Esc back                                                    │
│Up,Down move / Enter choose / Esc clear,close / F2 details                    │
│Ctrl+F star / Ctrl+G save scope / Ctrl+R reload / Ctrl+U clear                │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Option A / Models / 80x24 / Empty

```text
┌Models / this session / profile: example──────────────────────────────────────┐
│Filter: zzzz                                                                  │
│[All]  Starred  Configured  Set up     Tab changes scope                      │
│0 matches / 418 loaded models (synthetic)                                     │
│                                                                              │
│No matches for "zzzz" in All.                                                 │
│                                                                              │
│Ctrl+U clears search. Tab changes scope.                                      │
│Esc clears search, then closes.                                               │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│Details───────────────────────────────────────────────────────────────────────│
│No action until a valid row is selected.                                      │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│No selection / Enter disabled / Esc clear,close                               │
│Ctrl+U clear / Tab scope / Ctrl+R reload                                      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Option A / Providers / 80x24 / Error

```text
┌Providers / profile: example──────────────────────────────────────────────────┐
│Filter: type to search_                                                       │
│[All]  Keys  Accounts  Endpoints      Tab changes scope                       │
│Synthetic: 32 providers / illustrative states                                 │
│! Could not reload. Showing stale results.                                    │
│Offline: changes disabled / Ctrl+R retry / Esc close                          │
│Configured                                                                    │
│    Ollama Cloud                                                  listed ready│
│    Anthropic                                                     listed ready│
│    OpenAI Codex                                                  listed ready│
│Needs setup                                                                   │
│>   OpenAI                                                              no key│
│    DeepSeek                                                            no key│
│More below / 7 rows in view / PgDn scrolls                                    │
│Details───────────────────────────────────────────────────────────────────────│
│Selected: OpenAI / needs setup                                                │
│Status: stale / offline                                                       │
│Setup changes disabled until reconnect                                        │
│Ctrl+R retry; no automatic write repeat                                       │
│Provider identity remains available                                           │
│F2 details / Esc close                                                        │
│Offline / changes disabled / Ctrl+R retry / Esc close                         │
│F2 details / Ctrl+U clear / Tab scope                                         │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Option A / Providers / 80x24 / Setup

```text
┌Providers / profile: example──────────────────────────────────────────────────┐
│Filter: type to search_                                                       │
│[All]  Keys  Accounts  Endpoints      Tab changes scope                       │
│Synthetic: 32 providers / illustrative states                                 │
│                                                                              │
│   ┌OpenAI / API key / step 1 of 3────────────────────────────────────────┐   │
│   │After pasting an illustrative value (not a real key):                 │   │
│   │Key: ••••••••_                                                        │   │
│   │                                                                      │   │
│   │Enter checks before saving.                                           │   │
│   │If unreachable: another Enter saves unchecked.                        │   │
│   │A rejected key remains here for correction.                           │   │
│   │                                                                      │   │
│   │Ctrl+U clear / Esc cancel and erase input                             │   │
│   │                                                                      │   │
│   │                                                                      │   │
│   │                                                                      │   │
│   └──────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```
