# Workflow script API

A workflow is plain JavaScript with a literal metadata header and an async body:

```js
export const meta = {
  name: 'review',
  description: 'Review and summarize a change',
  whenToUse: 'Before merging a change',
  phases: [{ title: 'Review', detail: 'Check the change', model: 'reviewer' }],
};
phase('Review');
return await parallel([
  () => agent('Review correctness', { effort: 'high' }),
  () => agent('Review simplicity'),
]);
```

The first statement after whitespace and comments must be `export const meta = {...}`.
`name` is required: 1–64 ASCII letters, digits, dots, underscores or hyphens, without a leading
dot. `description` is a required non-blank string. `whenToUse` is an optional string. `phases`
is an optional array of objects with unique, non-blank `title` strings and optional string
`detail` and `model` fields. Unknown keys warn; duplicate keys fail.

The header is parsed without executing JavaScript. Objects, arrays, strings, finite JSON numbers,
booleans and null are allowed, with comments and trailing commas. Single, double and backtick
quotes work; template interpolation, calls, identifiers as values, spreads and computed keys do
not. A semicolon or a valid automatic statement boundary must follow the object; continuing its
expression on the next line is refused. The body supports top-level `await` and `return`, runs
in strict mode, and keeps the source's line numbers. TypeScript and module declarations are
syntax errors.

## Agents

`agent(prompt, opts?)` returns a promise for the backend's final string or JSON object. A failed
or skipped agent returns `null`. The prompt must be a non-blank string. Options may be absent,
`undefined`, `null`, or a plain object. Unknown keys and null option values are errors;
undefined option values are absent. Non-blank strings retain their original whitespace.

| Option | Value |
|---|---|
| `label` | Non-blank string, at most 120 Unicode scalar values. Defaults to the first non-blank prompt line, trimmed and cut at 60 characters with `…`. |
| `phase` | Non-blank string; overrides the instance's current phase. |
| `schema` | Plain JSON schema with root `type: 'object'`, object `properties`, and optional `required` array naming only existing properties. |
| `tier` | Non-blank staffing tier. |
| `model` | Non-blank model; overrides the tier's model downstream. |
| `provider` | Non-blank provider. |
| `effort` | Exactly `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, or `ultra`. |
| `profile` | Non-blank profile. |
| `toolsets`, `skills` | Non-empty arrays of distinct, non-blank strings. Duplicates compare exactly. |
| `isolation` | Exactly `'worktree'`. |
| `maxTurns` | Integer from 1 to 10,000. Omitted means the profile's turn-limit policy, which may be finite. |
| `stallMs` | Positive integer; accepted and ignored. Its first use logs an explanation. |

An `agentType` option is refused: Hermes has no agent types; use `tier`, `toolsets` or `skills`.
The engine forwards model and staffing choices to its backend; it does not resolve them.

Schema checks are structural only. Every nested value must be plain JSON data: no functions,
symbols, BigInts, undefined values, array holes, accessors, `toJSON`, special objects, cycles or
non-finite numbers. Accepted schemas are forwarded unchanged. The engine performs no deeper
contradiction analysis. The live backend owns output validation and one correction turn; the
mock returns its configured value directly.

Await every agent whose result matters. When the root body returns or throws, pending work is
abandoned: its future is dropped, its finish event says `abandoned`, and the root outcome stands.
A late abort from abandoned work does not replace that outcome.

## Combinators and narration

`parallel(thunks)` validates an array of functions before invoking any of them. It invokes every
thunk immediately, in index order, and waits for all results, returning them in index order.
A throwing or rejecting thunk yields `null`. An empty array returns `[]`.

`pipeline(items, ...stages)` requires an array and at least one function stage. Each item's
stages run independently, with no barrier between items. A stage receives
`(previousResult, originalItem, index)`; the first receives the original item as its previous
result. A thrown or rejected error drops that item to `null` and skips its remaining stages.
A returned `null` passes to the next stage. Results preserve item order.

`phase(title)` converts to a string, trims it, and requires a non-empty title. It changes only the
calling instance's current phase and emits a phase event. Undeclared phase titles are allowed.
`log(message)` converts to a string and emits a log event, cutting at 4,000 Unicode scalar values
with `…`. Use `log()` for narration; `console` is absent.

`args` is the run's JSON input, or `undefined` if omitted.

## Budgets

`budget.total`, `budget.spent()` and `budget.remaining()` read token usage from the backend.
The total is a number or `null`; remaining is clamped at zero or `Infinity` without a total.
`budget.money.total`, `.spent()` and `.remaining()` work the same way in US dollars.

Once either configured total is reached, new live agents reject with `BudgetExhaustedError`.
Already paid replayed calls bypass this check. The engine does no accounting itself. Negative
or non-finite money snapshots fail the run as backend defects. Reading live usage to control a
loop can change its number of iterations after resume.

## Child workflows

`workflow(nameOrRef, args?)` accepts a non-empty saved name or an object containing only
`{scriptPath: 'relative/path.js'}`. The resolver returns the child's source. Each call creates
an independent instance with its own arguments, current phase and workflow path. Concurrent
children keep these bindings across `await`. Names and paths never alias. Children share the
run's backend, budget, agent count, cancellation, limits and replay log. Children cannot start
grandchildren: nesting is one level only.

Child arguments and results use strict JSON conversion: `toJSON` is honored, undefined object
properties are omitted, root undefined becomes null, and array holes become null. Functions,
symbols, BigInts, cycles, non-finite numbers and explicit undefined array elements fail.
The same conversion applies to the root result. Conversion errors in child arguments are
`WorkflowOptionError`; invalid child results are `WorkflowScriptError`.

Child parse, import and syntax errors reject with `WorkflowScriptError` naming the child and
its original line. Missing or unreadable children give `WorkflowNotFoundError`. Uncaught child
error objects keep the child's source position and workflow path, including when rethrown by
the parent. Events identify their instance by `workflow_path`; consumers must group by this
path because parent and sibling events may interleave.

## Determinism, identity and replay

`now()` returns the configured run-start timestamp in milliseconds for the entire run.
`random(key)` requires a non-empty string and returns a deterministic number in `[0, 1)`.
It hashes the seed as eight big-endian bytes, the instance path joined with `/`, a zero byte,
and the UTF-8 key; the first 53 bits divided by 2^53 give the result. Resume must restore the
same timestamp, seed and input.

Each agent receives `CallId { digest, occurrence }`. The digest is SHA-256 of canonical JSON
containing version 1, workflow path, prompt, schema and present execution options. Keys are
sorted and absent options omitted. Labels, phases and `stallMs` do not affect identity.
Occurrences start at zero and count earlier started calls with the same digest. Rejected
calls consume neither an occurrence nor a slot. Child path components include a hash of their
converted arguments. Backends must keep labels and phases out of their computation.

The engine delivers one backend outcome, drains runnable JavaScript, then delivers the next.
Outcomes ready together use invocation order. Delivered agent finishes carry increasing
`completion_seq` values. A replay log supplies completed outcomes in that order, matched by
the full call ID; those calls use no backend and bypass the budget check but still count
toward the agent limit. New live calls start immediately, with their outcomes held while
ordered replay remains.

If the next logged call was never invoked when JavaScript becomes idle, `ReplayDiverged`
reports its log index. Remaining entries become a call-ID cache and held live results are
released. After divergence, identical calls with different stored results can be assigned
differently; live budget-dependent control flow can also diverge.

## Sandbox, errors and endings

Scripts have standard ECMAScript built-ins and the documented host API. No filesystem,
network, timers or module access exists. Absent globals include `require`, `process`, `fetch`,
`XMLHttpRequest`, `WebSocket`, `setTimeout`, `setInterval`, `setImmediate`, `std`, `os`, `Deno`,
`Bun`, `crypto`, `performance`, `Intl`, `WeakRef`, `FinalizationRegistry`, `navigator`, `console`.
`eval` and `Function` can compute inside the same sandbox.

`Date.now()`, `Date()`, argument-less `new Date()` and `Math.random()` throw
`DeterminismError` with advice to use `args`, `now()` or `random(key)`. Dates with explicit
arguments, `Date.UTC` and `Date.parse` work. Constructor and reflection paths preserve the guard.

Dynamic `import()` and `import.meta` are refused before the run starts. The token scanner
ignores strings, comments, regex literals and template text, but scans template expressions.
Property calls such as `x.import()` are allowed. An object method named `import` is a deliberate
false positive; write `{'import'() {}}`. A loader backstop makes imports constructed at runtime
an uncatchable `ImportNotAllowed` run failure. Static imports and exports are syntax errors.

| Limit | Default | Result |
|---|---|---|
| Heap | 256 MiB | Run fails with `out of memory`. |
| Stack | 1 MiB | Run fails with `stack overflow`. |
| Runnable JavaScript per turn | 5 seconds | Run fails with advice about waiting for an agent or child workflow. |
| Agents per run, children and replays included | 1,000 | Catchable `AgentLimitError`. |
| Items per `parallel` or `pipeline` | 4,096 | Catchable `ItemLimitError`. |

All limits are configurable through `EngineConfig`. Backend and resolver waits do not count
against a JavaScript turn; an endless chain of already resolved promises does count.
The embedded QuickJS release exposes native heap and stack exhaustion as catchable exceptions.
Uncaught exhaustion fails the run as shown above; making caught exhaustion end the run requires
an upstream runtime hook. This limitation is tracked for the engine integration review.

Catchable API errors are `DeterminismError`, `WorkflowOptionError`, `SchemaError`,
`AgentLimitError`, `ItemLimitError`, `BudgetExhaustedError`, `NestedWorkflowError`,
`WorkflowNotFoundError` and `WorkflowScriptError`, alongside JavaScript's own errors.
Header refusals are `MetaMissing`, `MetaNotLiteral`, `MetaField` and `ImportNotAllowed`,
returned by `parse_script` before any event is emitted. Positions use 1-based original lines
and Unicode scalar columns when QuickJS supplies a column.

The root outcome is `Completed`, `Failed` with a located `ScriptFailure`, or `Aborted`.
Cancellation and backend aborts end the run outside JavaScript; neither `try/catch` nor
combinators can swallow them. Every agent start has one finish, every child start one finish,
then one final `Finished` event. A dropped event receiver does not stop execution.

## Mock and dry run

Enable the `mock` Cargo feature for `MockBackend`, its chaining builder, and the dry-run adapter.
JSON rules match prompt content, exact prompt, label, phase, tier, model or occurrence. All supplied
match keys must agree; the first matching rule wins. An optional default supplies unmatched calls.
Outcomes are `{"text":"..."}`, `{"object":...}`, `{"failed":"..."}`, `"skipped"` or
`{"abort":"..."}`. Optional non-negative integer `delay_ms` uses Tokio time. Unknown or malformed
fields are refused with their rule index. Completed mock calls can charge configured token and
money amounts. Inspection exposes arrival-order requests, completed calls with outcomes, current
and peak in-flight counts. Dropped futures release their count.

```sh
cargo run -p talaria-workflow --features mock --example dry_run -- \
  review.js --mock rules.json --args '{"lenses":["correctness"]}' --seed 7
```

`--mock` and one script path are required. Optional flags are `--args <json>`,
`--max-sync-ms <positive integer>` and `--seed <u64>`. Flags cannot repeat or use `=` syntax.
The dry run prints an event per JSON line and a final `result`, `failed`, `aborted` or `refused`
line. Exit codes are 0 completed, 1 failed, 2 refused, 3 aborted, 64 usage. Ctrl+C cancels.
Timestamp and default seed are zero for repeatable fixtures. Child names resolve to sibling
`name.js` files; script paths resolve relative to the root script directory.

## Differences from Claude Code

Talaria adds Hermes options, the expanded effort set, `budget.money`, `now()` and `random(key)`.
It refuses `agentType` and accepts `stallMs` as a logged no-op. The engine imposes no concurrency
width; a backend scheduler owns its cap. Schema checks are structural only, with one correction
turn in the live backend. Resume uses content plus occurrence with ordered replay instead of
invalidating every call after a changed prompt. Pending work is explicitly abandoned at root
completion. The scanner's `import` method false positive and the absence of `console` are
documented above. A transfer using the shared API keeps its JavaScript orchestration semantics.
