# Workflow staffing and budgets

`staffing.toml` maps workflow tiers to Hermes models. Talaria reads it from
`$TALARIA_CONFIG_DIR/staffing.toml`, or `~/.talaria/staffing.toml` when that variable
is absent or empty. A missing file means no tiers, ten simultaneous agents and
no budget. The library never creates or rewrites your staffing file.

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
the named tier, the default tier, or the profile's current model is used, in that
order. A call's effort overrides its tier's effort. Explicit models ignore an
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
reads at least 250 milliseconds apart. Total tokens use `total_tokens`, otherwise
input plus output when both are known. Missing token counts remain visible.

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

Budgets stop new admissions and retries; running work finishes. Queued calls are
skipped when a budget is reached. Resume restores spend, frozen prices and width
reductions silently, without restoring billing pauses or cooldowns.

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
