# Workflow staffing and budgets

`staffing.toml` maps workflow tiers to Hermes models. Talaria reads it from
`$TALARIA_CONFIG_DIR/staffing.toml`, or `~/.talaria/staffing.toml` when that variable
is absent or empty.
A missing file means no tiers, five simultaneous agents and no budget.
The library never creates or rewrites your staffing file.

```toml
version = 1
[defaults]
tier = "survey"
[tiers.survey]
model = "minimax-m3.1-flash"
provider = "ollama-cloud"
# effort = "low"
# note = "A short description"
[limits]
max_in_flight = 10
[providers.ollama-cloud]
max_in_flight = 4
[budget]
tokens = 2000000
money_usd = 5.00
```

Only version 1 is accepted. Unknown keys fail with their key; syntax failures
include line and column. Tier names start with a lower-case ASCII letter,
contain only lower-case ASCII letters, digits, `_` or `-`, and have at most
32 characters. Every tier needs a nonblank model (at most 256 characters) and
provider. Optional effort is `none`, `minimal`, `low`, `medium`, `high`, `xhigh`,
`max` or `ultra`. Strings keep their spelling and whitespace. Widths are integers
from 1 through 64. Budgets are positive; dollars must also be finite. CLI budget
values override the corresponding file values. `StaffingTable::TEMPLATE` has
only the version and default global limit active, with the tier, provider and
budget examples commented out.

## Roster and routing

Each execution, including resume, reads `model.options` once before checking
every tier. Roster connections are bound to one profile and reused. Requests
send `{}` with a 15-second timeout; they do not ask Hermes to refresh provider
catalogues. Error 5098 tells the operator to restart an outdated gateway; Talaria
does not restart it. The cache limits ordinary subsequent reads to one per
30 seconds per profile. A failed refresh retains the preceding roster.

Missing providers, signed-out accounts, models not offered and unavailable
models are problems that prevent a run from starting. Rate limits are warnings:
the scheduler waits. Passed reset times no longer limit a route. Account limits
without a usable reset remain limited. A pool is limited only if all its accounts
are effectively limited, with the earliest known reset winning over unknown
resets. Exhausted account windows without pool accounts use the latest reset;
an unknown reset keeps the limit indefinite. Model windows can warn about low
quota but never exhaust the whole provider. Twenty percent or less remaining
in the tightest live window produces a warning.

A call naming a model and provider uses them. A model alone needs exactly one
signed-in provider offering it. A provider without a model is an error. Otherwise
the named tier, the default tier, or the roster's current model is used, in that
order. The current model is the profile's configured one unless the run was started with
`--session-model` and `--session-provider`. A call's effort overrides its tier's effort. Explicit models ignore an
accompanying tier, with one warning per tier per execution. Model-only and
profile-default resolution for a sibling profile require an explicit model and
provider or a tier, since that sibling's roster was not frozen at run start.

Resolution uses immutable table and roster snapshots. Limits never change the
resolved launch or its journal key. Resume prints tier drift before agents start.
Changing a used tier's model, provider or effort changes its resolved launch and
key, so finished calls on that changed launch run again. Editing a tier the script
never uses does not change finished calls' keys.

## Admission, retries and events

A lane is `(profile, provider)`. Provider caps apply independently to each
profile; the global cap bounds the whole execution. Fresh calls enter a FIFO
queue per lane; among lanes that can proceed, the earliest arrival wins. A
limited lane does not block other lanes. Reattached work always proceeds and
counts toward widths, even when the existing work exceeds those widths.
Cancelled queued calls release their place.

The first rate-limit failure in a run halves its global width, with a floor of
one. Every `rate_limit`, `upstream_rate_limit` or `overloaded` failure halves its
lane's width. The same journal settlement is applied once even if the retry hook
observes it before a journal fold. The lane forces a roster read before another
admission: a reported limit supplies the reset; otherwise the lane pauses for
60 seconds. Unknown reset times are checked every 60 seconds. Three consecutive
refresh failures produce a persistent warning. `billing` skips that lane's queued
and later calls for the rest of this execution and is never retried.

The journal backend drops the failed attempt's permit before consulting the gate.
An authorized retry writes a version-1 `call_finished` record with `retry: true`,
then starts the next attempt with fresh admission and new attempt identifiers.
There is at most one automatic retry per key per execution. Its intermediate
failure is never delivered to the script. A second failed attempt returns `null`.
Existing journal constructors and ordinary golden record bytes remain compatible;
`Record::CallRetryFinished` represents the retry finish in Rust.

Subscribers receive unbounded, ordered `TimedEvent` streams with UTC RFC 3339
`at` times. Events describe waits, admissions, releases, skips, width changes,
pauses, retries, budgets and roster reads. `status()` supplies widths, lanes and
spend for the progress view. These APIs contain no terminal presentation state.

## Spend and budgets

The scheduler folds the journal at start, on each permit release and on budget
reads at least 250 milliseconds apart. A turn's tokens use `total_tokens`, otherwise
input plus output when both are known. Missing token counts remain visible.

Cache reads are counted apart from the other tokens, and the token budget does not count
them. Hermes folds a turn's cache reads into the turn's token total, so a run that re-reads a
long context can be mostly cache reads, which cost a fraction of other tokens. Cache writes are
billed at or above the input rate and stay in the counted tokens. Hermes gives a settled turn's
tokens for that turn but its cache reads as the session's running total (`cache_read_tokens` in
the settled result), so Talaria journals that running total with each turn
(`session_cache_read_tokens` in the turn's usage) and takes the difference from the same
attempt's previous turn. A running total that went down started again, as after a gateway
restart, and counts as that turn's own. A turn's cache reads never exceed its tokens. Because
the journal holds what the split needs, a resumed run, `workflow list` and a replayed result
give the split the live run showed.

When a settled turn reports no cache read figure, the whole turn is counted toward the budget
and the output says so: "cache reads unknown for N turns, counted in full". Journals written
before cache reads were kept apart have no such figure, so every turn in them is unknown.
Spend therefore has these numbers: `tokens` (everything except cache reads, the budget's
measure), `cache_read_tokens` and `cache_unknown_turns`. In the `run_finished` JSON object,
`spend.tokens` used to include cache reads and no longer does; `cache_read_tokens` and
`cache_unknown_turns` are new beside it. `workflow list --json` rows carry the same three
numbers as `tokens_spent`, `cache_read_tokens` and `cache_unknown_turns`.

Hermes reports session cost cumulatively. Known `actual`, `estimated` and
`included` readings contribute only their increase within the attempt; a lower
reading starts a fresh counter. For example, $0.01 then $0.02 contributes $0.02,
while $0.03 then $0.01 contributes $0.04. Each attempt starts from zero.

Unknown costs, including `0.0` with status `unknown`, use input/output token
prices frozen in the original manifest. Prices are dollars per million tokens;
cache prices are retained but cannot be applied without cache token counts.
A sibling profile has no fallback prices. Unpriced turns are counted, never
silently called free. Output is `$x.xx`, `$x.xx plus n unpriced turns`, or
`unknown (n unpriced turns)`. Ollama Cloud has no catalogue price at the pinned
Hermes version. A money budget warns about that accounting gap without refusing
the provider. Estimates with token budgets show the cheapest-to-dearest known
range plus any unpriced tiers; an all-unpriced roster is `unknown`.

Without a token budget there is no range. The estimate then lists each tier's price per
million tokens. When no tiers are set (a script that names its models directly), there are no
tier prices to list, and it says so. It then says what decides the cost of a directly named
model: whether Hermes lists a price for that model on its provider. If Hermes lists no price
for any model on the configured providers, the estimate says that the cost of such agents
will be unknown. Setting tiers does not create a price Hermes does not have, so the text never
suggests it. The line printed after a run says why money is unknown in the same terms: Hermes
reported no cost and has no price for the models used, or no agent turn had finished.

Token spend also counts work in progress. Hermes streams a `session.usage` event
with each agent session's cumulative token count after every model call, with its cache
reads (`cache_read_tokens`) beside the total; the reading counts without them. An event
that does not give cache reads counts whole. The
runner adds each session's latest reading beyond what that session has already
settled, so a settled turn replaces its live share instead of adding to it, and a
lower reading never takes spend back. Money is counted only when a turn settles.

When counted spend reaches a budget:

- No new agent starts. The engine refuses the `agent()` call and journals an
  `agent_refused` engine event with the label, phase and reason; calls waiting at
  the admission gate end as stopped without reaching Hermes.
- Running agents are stopped through the same interrupt the operator's stop uses.
  A stop the gateway does not confirm is sent again on the runner's next check
  for as long as the turn is still running. Their turns settle, so their usage is still journaled, and each call finishes
  with a `stopped` outcome whose reason names what was spent. A turn that had
  already finished with an answer keeps it, and no agent starts a correction turn.
- The runner journals one log line when the budget is reached, and the run ends
  as `budget_exhausted` with refused and stopped counts in its summary when the
  script returns.

What the budget guarantees: no agent starts once counted spend reaches it, and
every agent still running is told to stop as soon as the runner counts it. What it
cannot guarantee: a hard ceiling. Spend can pass the budget by the model calls
already in flight when it is reached: Hermes reports a session's usage only after
each model call returns, and the runner cannot take back a call already running. A provider that streams
no usage leaves its agents counted only when their turns settle; the runner
journals `streamed no token usage` once for each such agent while a token budget
is set, and that agent is held only by the check made before it starts. After a
gateway restart a session's counter can start again from zero, so its live share
is counted again only once it passes what has settled. The money budget is
checked from settled turns only.

Retries stop when a budget is reached. Resume restores spend, frozen prices and
width reductions silently, without restoring billing pauses or cooldowns. Resuming
a `budget_exhausted` run with a larger budget reruns its stopped and refused
agents and reuses the completed ones. A resume without a budget flag keeps the
run's own budget, so a `budget_exhausted` run then only replays its stored result.

Snapshots omit provider endpoint URLs, environment-key names, warnings and account
identities or labels. Reports replace controls and bidirectional formatting
characters with `?`, including JSON report strings. The text report preserves the
published golden spacing.

## Examples

```sh
cargo run -p talaria-workflow --features hermes-live --example roster -- \
  --profile fixture --staffing /tmp/staffing.toml --json
cargo run -p talaria-workflow --features hermes-live --example staffed_run -- \
  --profile fixture --staffing /tmp/staffing.toml --script /tmp/fanout.js \
  --run-dir /tmp/workflow-runs --budget-tokens 2000000 --budget-usd 5
cargo run -p talaria-workflow --features hermes-live --example staffed_run -- \
  --staffing /tmp/staffing.toml --resume /tmp/workflow-runs/RUN_ID
```

`roster` exits 0 on a usable table, 2 on tier problems and 1 on read failures.
`staffed_run` uses the run-folder constructor, which creates a run-ID folder
under the supplied `--run-dir` parent. It prints scheduler events as JSON lines
and finishes with spend, money text and the actual run path. Warnings and drift
go to stderr. Both examples support `--help`; neither configures Hermes. The staffing retry hook and version-1 retry record are integrated into the
journal backend.
