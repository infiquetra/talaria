# Hermes configuration surface: socket or dashboard REST, call by call

Exploration #249 (unit P4-00 of the v0.7.0 roadmap, under the phase 4a parent #216) asked which
configuration calls Talaria can make over the gateway WebSocket it already holds, and which need the
dashboard's REST routes on the same gateway. This document is the finding. The call matrix it rests
on is [`fixtures/configuration-surface/matrix.json`](../../fixtures/configuration-surface/matrix.json),
and the probe that produced it is `crates/hermes-client/tests/live_config_surface.rs`.

## Summary

1. The socket does not cover configuration: canonical `config.get` and `config.set` accept only a
   short fixed list of presentation and session keys, and refuse every schema key such as
   `display.skin` with `invalid_params` (rows C06, C08), so the handoff's reading "reads with
   `config.get`, writes with `config.set`" is refuted for a schema-driven settings form.
2. The dashboard REST routes on the same loopback listener do cover it: `GET /api/config` returns
   the whole configuration in 4 ms, `PUT /api/config` with one changed key wrote it, the next read
   showed it, and a second `PUT` restored the file byte for byte (rows C15, C17, C19).
3. Every REST route the phase 4a children need answered a same-user client holding a single-use
   `native-http` ticket for `fixture`, and the five ticket rules (reused, `Origin`, 31 s old, absent,
   other profile) all refused exactly as the source predicts (rows C16, C21 to C24).
4. `GET /api/config` returns secrets unmasked (one credential-shaped leaf on `fixture` came back in
   plain text, row C15), so #250 (`P4-01`) must mask secrets itself.
5. Only provider listings are slow: socket `model.options` (130 to 143 ms), REST
   `/api/model/options` (127 ms) and `/api/providers/oauth` (58 ms) exceed the 50 ms frame budget and
   need a loading state; every other call answered in 16 ms or less.

## Hermes release

Tag `v0.22.4`, commit `4d04303818b61bf3aa09278af48356315515965b`, read from
`crates/hermes-contract/PINNED`. The live gateway reported the same commit through `identify`
(`live_preconditions()` skips on any other), and the static readings were re-checked against a
checkout at that commit. The gateway's `api_origin` is discovered on every run; at the time of these
runs it was `http://127.0.0.1:60222`.

## How the evidence was gathered

All runs were on 2026-10-26 against the isolated test gateway, profile `fixture`, through the
three ignored tests in `live_config_surface.rs`. SHA-256 digests of the `fixture` profile's
`config.yaml` and `.env` were taken before and after every run, and were unchanged by every run.

| Run | Result |
|---|---|
| `probe_reads_on_fixture` | 43 rows, 0 refuted, 225 requests, `PASSED` |
| `probe_ticket_rules` | 5 rows, 0 refuted, 14 requests, `PASSED` |
| `probe_writes_round_trip_on_fixture` with `TALARIA_PROBE_WRITES=1` | C10 and C19 `write-restored`, `applies-now`, 20 requests, `PASSED` |
| The same write test without the opt-in | `SKIPPED (write opt-in not set)`, `requests sent: 0`, exit 0 |
| The read test with an empty `HERMES_HOME` | `SKIPPED (no Hermes gateway is running for this home)`, exit 0; with `TALARIA_LIVE_REQUIRE=1`, exit 101 |

Departures from the issue, each approved before it happened:

- **The REST ticket is minted by the probe itself.** The `Ticket` returned by #198 (`P1-01`)'s
  `mint_ticket` keeps its secret behind a crate-private accessor (`crates/hermes-client/src/ticket.rs`,
  `Ticket::expose`), so an integration test cannot put it in an `x-hermes-gateway-ticket` header. The
  build lead approved the test-only route: `config_probe::mint_native_http` sends the same
  control-socket `session-ticket` request that `mint_ticket` sends, with purpose `native-http` and
  scope `profile`, after the same checks (the home and the socket are private to this user, the
  socket's peer is this user) and the same reply checks (protocol 1, same instance, same profile,
  well-formed ticket). The library is untouched. Socket rows still use `mint_ticket`.
- **The write run was run by the builder seat, not the operator.** The issue reserves it for the
  operator. The build lead's assignment for this issue directed the builder seat to run it on
  `fixture` under the lead's ruling D-3, and to make `display.show_cost` explicit first. Before the
  baseline digest, the builder added `show_cost: false` under `display:` in the `fixture` profile's
  `config.yaml` (`display.skin` was already explicit). That line remains in the file.
- **No cold-mount timing.** The gateway was already running and warm, so the first REST request after
  a gateway start (S1) was not timed. No row recorded a `warm-up:` note, so no warm-up differed from
  the timed runs.

## Static readings S1 to S15

| # | Verdict | Evidence |
|---|---|---|
| S1 | Confirmed in part | Every REST row reached the same loopback listener named by `identify`'s `api_origin` (C54 lists `api_origin`). The cold mount was not timed (see above). |
| S2 | Confirmed | C21 (reused), C22 (`Origin`), C23 (31 s old) and C24 (absent) all gave 401 `Unauthorized`, each followed by `gateway-state: ready`; C16 gave 403 `profile_scope_mismatch`. One nuance from the source: with no ticket header the ticket code is inert and the 401 comes from the dashboard's own auth middleware (`hermes_cli/web_server.py:456-476`). |
| S3 | Not tested | C46 is static. The source re-check holds: `native_http.py:79-82` refuses any non-GET on `/api/profiles` with a `native-http` ticket. |
| S4 | Confirmed | C11 and C13 answered with no ticket, `/api/model/info` (C34), `/api/status` (C52) and `/api/health` (C53) too. The schema reply's keys are `category_order, fields`; no field carries `default` or a restart flag. Field entries carry two attributes the reading does not list, `clearable` and `searchable`. The probe's JSON parser sorts keys, so the claim that `model` is the first field was not checked live. |
| S5 | Confirmed | C15 and C17 returned the configuration; C19's `PUT` replied with keys `ok` only, the next `include_defaults=false` read showed the new value, and the restore left the file identical (digest unchanged). Comment survival could not be observed: the `fixture` file has no comments. The source also shows a `preserve_language` query parameter on the `PUT` (`config_env.py:119`). |
| S6 | Confirmed | C43 `profiles.describe` and C44 `profiles.configure` (legacy only) answered `ok` on the canonical socket; C59 gave -32601 with reason `unknown_method`. |
| S7 | Confirmed | C01 to C05 `ok`; `key=full` returned only `approvals, display, voice` inside `config` (the two paste thresholds appear only when the file sets them); C06 and C07 gave 4001 `invalid_params`. |
| S8 | Confirmed | C08 `invalid_params` with the file unchanged; C09 `use_session_mutation_model`; C10 `config.set key=skin` wrote, read back and restored. The `skin.changed` broadcast was not seen: the source publishes it only to sessions with subscribers, and the probe opens none. |
| S9 | Confirmed | C47 and C49 gave 4001 `profile_mismatch` (never 4064); C48 on a host-scope `interactive` ticket was routed to `default` and its home matched the served `default` profile. |
| S10 | Confirmed | `model.options` (legacy only) answered on the canonical socket (C25). The vendored contract has 257 `methods` and 48 `x-canonical-methods`. |
| S11 | Holds, one correction | Desktop's route list (`apps/desktop/electron/connection-config.ts:651-710`) names six model routes rather than a wildcard, and does not include `GET /api/config/defaults`. |
| S12 | Not tested | C56 to C58 are static. The source re-check holds. |
| S13 | Confirmed | C15 found 25 credential-shaped leaves on `fixture`: 24 empty and 1 plain. Hermes returned that one value unmasked. C18 `GET /api/config/raw` replied with keys `path, yaml`. Both bodies were dropped unread apart from this classification. |
| S14 | Confirmed, with one side effect | C27 gave 4002 and C28 gave 4005, both with no `data.reason`; C44 `ok`. The source re-check shows that `model.disconnect` takes the auth store's lock before answering, which opens and stamps the Hermes home's `auth.lock` (`hermes_cli/auth.py:674`, `:687`) without changing credentials; that file's modification time moved during the read run. |
| S15 | Confirmed | C60 404 `no_such_api_endpoint` with a ticket, C61 401 without; C50 403 `profile_scope_mismatch` for an unserved profile. The 404 catch-all is GET only (`web_server_dashboard.py:234-240`). |

Two source facts the readings did not state, found in the re-check:

- **Listing models can write `config.yaml`.** Socket `model.options` and `GET /api/model/options`
  build their payload through `list_authenticated_providers`, which for a profile with a
  `custom_providers` entry and a stale catalog saves the discovered models into the profile's
  `config.yaml` (`hermes_cli/model_switch_providers.py:1168`, the comment at
  `hermes_cli/web_routers/models.py:203-204` says so). `fixture` has no `custom_providers`, and its
  digest did not change. A read in Talaria's provider screen can therefore change the file on another
  profile.
- **`GET /api/model/info` can make an outbound request** to the configured `base_url` to learn the
  context length, capped at 5 s (`models.py:60-87`). It answered in 4 ms here.

## Schema facts

From C11 and C12 (identical): `fields` holds 908 entries keyed by dotted name. `category_order` is
`general, agent, terminal, display, delegation, memory, compression, security, browser, voice, tts,
stt, logging, discord, auxiliary`. The union of attribute names across all entries is `category,
clearable, description, options, searchable, type`. No entry carries `default`, and none carries a
restart flag. Defaults come only from `GET /api/config/defaults` (C13), whose 100 top-level names
start with `_config_version`.

- `stt.provider` is absent from the schema while `tts.provider`, `memory.provider` and
  `terminal.backend` are present. The source says why: `stt.provider` is defined in the overrides of
  `hermes_cli/web_server_config.py:127`, but the schema is built from `DEFAULT_CONFIG`, and
  `DEFAULT_CONFIG["stt"]` deliberately seeds no `provider` (`hermes_cli/config_defaults.py:1175`:
  unset means autodetect). A schema-driven form therefore cannot show the speech-to-text provider
  unless Talaria adds the field itself.
- Two dotted-name segments are 40 characters long: `allow_data_training_tiers_noninteractive` and
  `voice_channel_inactivity_timeout_seconds`. A privacy scan that refuses runs of 40 or more token
  characters must allow snake-case names, or allow these two by name.

## Per-call recommendation

"Loading state" means the warm median exceeded the architecture's 50 ms worst-frame budget.
"Typed" means the pinned contract describes the call; REST routes and legacy-only methods are not
typed and the child hand-types the shape.

| Need | Call | Transport and ticket | Median | Loading state | Typed | Deciding criterion |
|---|---|---|---|---|---|---|
| Read the whole configuration | `GET /api/config?profile=fixture` (C15) | REST, fresh `native-http` | 4 ms | no | no | 2: the socket returns only the presentation subset |
| Read only the explicit values | `GET /api/config?include_defaults=false&profile=fixture` (C17) | REST, fresh `native-http` | 3 ms | no | no | 2 |
| Write one setting | `PUT /api/config` with only the changed key (C19) | REST, fresh `native-http` | 10 ms | no | no | 4: one writer for every schema key |
| Write a session or display word | `config.set` (C10) | socket, `interactive` | 18 ms | no | yes | 4: only for S8's keys, which have session effects |
| Schema | `GET /api/config/schema` (C11) | REST, no ticket | 14 ms | no | no | 1 |
| Defaults | `GET /api/config/defaults` (C13) | REST, no ticket | 3 ms | no | no | 1 |
| Change detection | `config.get key=mtime` (C02) | socket, `interactive` | 2 ms | no | yes | preference rule: the socket answers fully |
| Model options | `model.options` (C25, C26) | socket, `interactive` | 130 to 143 ms | yes | legacy only | preference rule |
| Save or remove a key | `model.save_key`, `model.disconnect` (C27, C28) | socket, `interactive` | 1 to 2 ms | no | legacy only | 1 |
| Environment variables | `GET /api/env?profile=fixture` (C29) | REST, fresh `native-http` | 15 ms | no | no | 1: no socket method |
| OAuth providers | `GET /api/providers/oauth` (C30) | REST, fresh `native-http` | 58 ms | yes | no | 1 |
| Custom endpoints | `GET /api/providers/custom-endpoints` (C31) | REST, fresh `native-http` | 3 ms | no | no | 1 |
| Model info and roles | `GET /api/model/info`, `GET /api/model/auxiliary`, `GET /api/model/moa` (C35, C37, C38) | REST, fresh `native-http` | 3 to 4 ms | no | no | 1 |
| Profiles | `profiles.list` (C41) | socket, `interactive` | 4 ms | no | yes | preference rule |
| Profile detail | `profiles.describe` (C43) | socket, `interactive` (legacy fallback) | 10 ms | no | legacy only | 1 |
| Another profile's values | `config.get` with `profile` on a host-scope ticket (C48) | socket, `interactive` host | 4 ms | no | yes | 3 |
| Health and version | `identify` (C54), `runtime.describe` (C51), `GET /api/health` (C53), `GET /api/status` (C52) | control, socket, REST without ticket | 1, 4, 3, 41 ms | no | `identify` and `runtime.describe` only | see #254 |

The medians exclude the ticket mint. Minting a REST ticket took 0 to 2 ms more (median per row,
`mint_ms` in the matrix), so a ticketed REST call costs its median plus about 2 ms. All timings are
on `fixture`; other profiles were not timed.

## Security observations

- All five ticket rules held (C16, C21 to C24). None refuted.
- `GET /api/config` returns secrets in plain text (C15), and `GET /api/config/raw` returns the file
  text and its absolute path (C18).
- `GET /api/status` needs no ticket. On this gateway its reply includes the keys `hermes_home`,
  `config_path`, `env_path` and `gateway_pid` (C52). The source shows they appear only when
  `auth_required` is false (`hermes_cli/web_routers/status.py:482-579`). A client must not show or
  log those values.
- `profiles.describe` returns the whole soul text and the MCP server names (C43's keys include
  `soul` and `mcp_servers`); the probe recorded keys only.
- `model.disconnect` touches `auth.lock` (S14 above), and listing models can persist discovered
  models (above).

## Proposed edits per child

The phase planner edits the child bodies; these are the edits this finding supports.

### #250 (`P4-01`) settings framework

- Read the whole configuration with `GET /api/config?profile=<p>` (C15, 4 ms). Use
  `include_defaults=false` (C17) to know which keys the file sets explicitly.
- Write one setting with `PUT /api/config` carrying only the changed key, as
  `{"config": {...}, "profile": "<p>"}` (C19, 10 ms, `applies-now` on the next read). A deep merge
  cannot delete a key, and saving strips a value equal to its default unless the key was explicit,
  so "reset to default" cannot be a `PUT`; label it as not available, or leave it to a raw edit.
- Use socket `config.set` only for S8's keys (session controls and display words), because those
  have effects on live sessions (`skin` broadcasts `skin.changed`). Never `config.set key=model`.
- Schema from `GET /api/config/schema` and defaults from `GET /api/config/defaults`, both without a
  ticket (C11 14 ms, C13 3 ms). The schema has no `default` and no restart flag, so restart labels
  come from a table Talaria owns. Expect `clearable` and `searchable` attributes as well as `type`,
  `description`, `category` and `options`. `stt.provider` is missing from the schema at this release.
  Test 1's reviewed allow-list for long names must include
  `allow_data_training_tiers_noninteractive` and `voice_channel_inactivity_timeout_seconds`.
- Mask secrets in Talaria: the route returns them in plain text. Hermes's own rule (single key
  segment, `_is_secret_config_key`) is ported in `config_probe::is_credential_key` and can be reused.
- Own a small REST client: one TCP connection per request with `Connection: close`, one fresh
  `native-http` ticket per request (single use, 30 s), no `Origin` header, `Content-Length` and
  chunked bodies. The ticket's secret is crate-private in `hermes-client`, so the client must live in
  that crate. The #250 (`P4-01`) specification already places `NativeHttpMinter` and the REST client
  in `crates/hermes-client/src/rest.rs`, where the minter can read the secret from `mint_ticket`'s
  `Ticket`; keep it there. The probe's own test-only mint is not a product route.
- Change detection can stay on the socket: `config.get key=mtime` (C02, 2 ms) returns `mtime` and
  `mcp_rev`.

### #251 (`P4-02`) providers, keys, sign-in

- `model.options` over the socket: 130 to 143 ms, needs a loading state; REST `/api/model/options`
  returns the same top-level keys (`model, provider, providers`) in 127 ms, so the socket is
  preferred.
- `model.save_key` and `model.disconnect` are reachable over the socket through the legacy fallback
  (C27 4002, C28 4005 on an unknown slug). Their errors carry no `data.reason`. Expect
  `model.disconnect` to touch `auth.lock` even when nothing is cleared.
- `GET /api/env?profile=<p>` returns one object keyed by variable name (464 entries on `fixture`),
  each with `advanced, category, channel_managed, custom, description, is_password, is_set, provider,
  provider_label, provider_primary, provider_profiles, redacted_value, tools, url`. The names include
  ones the operator chose; treat them as private and never log them.
- `GET /api/providers/oauth` (58 ms, loading state) and `/api/providers/custom-endpoints` answered
  without `?profile=`. The open point on the issue asked how a profile-scope ticket behaves there:
  both answered `ok` for `fixture`. The source accepts `profile` on both, so a screen for another
  profile should pass it.
- Never call `POST /api/env/reveal` except as a deliberate one-shot reveal the operator asked for.
- Listing models can write `config.yaml` on a profile with `custom_providers`; the screen should not
  list on every redraw.

### #252 (`P4-03`) model roles

- To change a profile's default model without touching live sessions, use
  `PUT /api/profiles/fixture/model` (C40, static; the path segment is the profile name) for the main model, or `POST /api/model/set` (C39,
  static) for the main or an auxiliary slot: its handler says it applies to new sessions only. Both
  are reachable with a `native-http` ticket for that profile. `PUT /api/config` with `model` (the C19
  route) would also write the key; this exploration did not compare it with the model routes, so
  prefer the routes made for it.
- Never `config.set key=model` (C09 `use_session_mutation_model`); per-session changes stay
  `session.mutate`.
- Read the roles with `GET /api/model/info`, `GET /api/model/auxiliary` and `GET /api/model/moa` (3 to 4 ms). The `presets` object of
  `GET /api/model/moa` is keyed by preset names the operator chose.

### #253 (`P4-06`) profiles

- List with socket `profiles.list` (C41, 4 ms): it returned the same set as `identify`'s served
  profiles and no parked one. `GET /api/profiles` agrees (C42) but each row holds an absolute path.
- Join served state from `identify` (`profiles` and `parked`).
- `profiles.describe` works over the socket through the legacy fallback (C43). Its reply includes the
  soul text; show it only on request.
- Creation was not probed (C45, C46 static). The source says socket `profiles.create` falls through
  to the legacy handler with an `interactive` ticket, and `POST /api/profiles` refuses a
  `native-http` ticket. Recommend `profiles.create` over the socket, confirmed by the operator before
  the child is built.
- For another served profile's values use a host-scope `interactive` ticket and `profile` (C48).
  An unserved name gives `profile_mismatch` on the socket (C49) and 403 on REST (C50).

### #254 (`P4-10`) doctor

- Version: `identify` gives `code_version` and `code_sha` (C54); `GET /api/health` gives `version`,
  `displayVersion` and `commit` without a ticket (C53, 3 ms). `runtime.describe` gives no version
  (C51's keys are `authority_epoch, capabilities, instance_id, profile_id, session_create, skin`).
- Served state: `identify` (`profiles`, `parked`, `state`) and `GET /api/status` (`gateway_state`,
  `profiles`, `parked_profiles`, 41 ms, no ticket). The probe did not compare their values; the
  doctor should check that the two agree and say which one it trusts.
- Restart through `POST /api/gateway/restart` (C56, static): it spawns `hermes gateway restart
  --interrupt`, and no handler checks the ticket purpose. Keep it behind a confirmation and the
  operator's own action.

## Phase 4b (#217)

These children were not probed; this is what they should expect.

- Any setting that is a schema key reads and writes through the same REST pair as #250 (`P4-01`);
  none can use socket `config.set`.
- MCP server settings live under `mcp_servers.<name>`, and their `env` maps can hold secrets the
  route returns in plain text, so the masking rule applies.
- Some socket methods the authority answers are not in `x-canonical-methods` (for example
  `plugins.list` and `setup.status`, S10); a child that uses one types it from the legacy contract.
- A screen that lists models or providers needs a loading state and must not list on every redraw.
- Anything that restarts or reloads the gateway is operator-run.

## Operator questions

1. Do comments in a profile's `config.yaml` survive a `PUT /api/config`? Not observable here: the
   `fixture` file has no comments. Not blocking.
2. Should Talaria create profiles over socket `profiles.create` (legacy fallback, no new ticket
   purpose), or should #253 (`P4-06`) add the `native-profile-create` purpose for
   `POST /api/profiles`? Not blocking this merge.
3. `show_cost: false` was added under `display:` in the `fixture` profile's `config.yaml` for the
   write run. Keep it (it makes the write run repeatable) or remove it?
4. A cold-mount timing of the first REST request needs a freshly started gateway, with how it was
   started (supervised or not). Wanted?

## How to re-run the probe

Start the pinned gateway with the `fixture` profile served, then from a clone:

```bash
TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_reads_on_fixture
TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_ticket_rules
# operator only, after making display.skin and display.show_cost explicit in fixture's config.yaml:
TALARIA_LIVE_REQUIRE=1 TALARIA_PROBE_WRITES=1 cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_writes_round_trip_on_fixture
```

Each test merges its rows into `config-surface/matrix.json` under Cargo's per-target temporary
folder, seeded from `fixtures/configuration-surface/static-rows.json`. Copy that file over
`fixtures/configuration-surface/matrix.json`, read the diff, and run
`cargo test -p hermes-client --test config_surface_matrix`. On a new Hermes release, re-check the
static readings at the new tag first; a `REFUTED` line is a finding, not a test failure, except on
the three control rows C59 to C61.
