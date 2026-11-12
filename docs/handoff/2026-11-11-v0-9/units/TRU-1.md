# TRU-1: A Hermes skin with an empty colour stops Talaria at connect

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today, the Hermes lines the handshake is matching, and the edge cases around a null colour. The requirements, the tests, and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`GatewayReadyPayload.skin` is a required `SkinPayload` (`crates/hermes-contract/src/generated/schemas.rs:7805`). `RuntimeDescribeResult.skin` is `Option<SkinPayload>` (`schemas.rs:15101-15102`). `SkinPayload` documents colour maps as token to colour string (`schemas.rs:18004`). The three maps are `HashMap<String, String>`: `colors` at `schemas.rs:18020`, `dark_colors` at `schemas.rs:18025`, `light_colors` at `schemas.rs:18034`. `branding` is the same map type at `schemas.rs:18015` and is not a colour map. `banner_hero` and `banner_logo` are strings (`schemas.rs:18008-18010`).

`handshake` calls `handshake_with` (`crates/hermes-client/src/connect.rs:499-500`). `handshake_with` (`connect.rs:503`) decodes the ready frame here:

```rust
let ready: GatewayReadyPayload =
    serde_json::from_value(value["params"]["payload"].clone())
        .map_err(|_| HandshakeError::BadReady)?;
```

That is `connect.rs:512-514`. Later it calls `runtime.describe` with an empty parameter map (`connect.rs:550-556`). If `value["result"]` has no `session_create`, it returns `HandshakeError::BadDescribe` before decode (`connect.rs:569-572`). The describe decode is `connect.rs:574-575` and maps failure to `BadDescribe`. The operator text for `BadDescribe` is "the gateway sent a runtime.describe reply Talaria could not use." (`connect.rs:298-299`). The operator text for `BadReady` is "the gateway sent a gateway.ready frame Talaria could not use." (`connect.rs:288-289`).

`Client::call` delegates to `call_with` (`crates/hermes-client/src/rpc.rs:326-328`). `call_with` decodes with `serde_json::from_value` and, on failure, returns `CallError::BadResult` whose display is `"{method}: result did not decode"` (`rpc.rs:345-349` and `rpc.rs:218`). `call_raw` (`rpc.rs:356`) returns the result value and does not decode it. `CallOptions::default()` leaves the timeout empty, so `runtime.describe` uses `DEFAULT_CALL_TIMEOUT`, 120 seconds (`rpc.rs:35` and `rpc.rs:48-53`).

`ConnectionService::open` (`crates/talaria-app/src/connection.rs:182`) dials with `LaunchDial`. `LaunchDial::dial` calls `hermes_client::handshake` and stores `handshake.describe` (`connection.rs:118-120`). A missing stored describe becomes `LaunchFailure::from(HandshakeError::BadDescribe)` (`connection.rs:206-207`). `open_with_dial` (`connection.rs:134`) is the other open path. After `Link::start` it calls the typed marker (`connection.rs:160-165`) and on any error closes the link (`connection.rs:173-175`). The remote launch in `crates/talaria-cli/src/run.rs` is what calls `open_with_dial` (`run.rs:5039`). Local launch uses `open`. Headless launch opens that same connection at `crates/talaria-app/src/headless/service.rs:107`. `SessionService::with_client` is `headless/service.rs:136`. `opening_service.open` at `headless/service.rs:143` is `SessionService::open` (`sessions.rs:211`). When saved model defaults exist and `create_defaults` is absent, that method still uses the typed describe at `sessions.rs:231-236`. `OpenConnection` (`connection.rs:75-80`) holds `link`, `inbound`, `describe`, and `profile_name`. It has no `gateway_version` field today. [#549](https://github.com/infiquetra/talaria/issues/549) (`SYNC-2`) provides `OpenConnection.gateway_version: Option<String>`. This unit does not add, remove, or rename that field, and it does not wait on [#549](https://github.com/infiquetra/talaria/issues/549) (`SYNC-2`).

`Skin::from_value` (`crates/talaria-core/src/appearance.rs:59`) already skips every non-string colour. Its comment says an unquoted `#` arrives as null and a skipped value never fails the skin (`appearance.rs:57-58`). The three maps are read at `appearance.rs:83-85`. This function never runs when the handshake has already failed. `Skin::from_payload` serialises a `SkinPayload` and then calls `from_value` (`appearance.rs:48-51`), so a key removed before the typed decode is also absent from the skip list. The `/theme` line is built from that skip list (`crates/talaria-app/src/appearance.rs:628`).

`crates/hermes-contract/src/overrides.rs` is the hand-written seam. Its first line is "Protocol rules enforced by Hermes but omitted from its published schema." `prompt_submit_params` is at `overrides.rs:44`. The crate root exports the module (`crates/hermes-contract/src/lib.rs:7`). There is no skin helper today.

`docs/themes.md` heading `## Known limit` is at line 156. Lines 158-161 describe `Skin::from_value` and the `Ignored skin values` line. Lines 163-168 say the launch handshake decodes `runtime.describe` through the generated types, those types reject a null colour, and making the handshake skip such colours "belongs to the contract types, and is left for a later release." REQ-5 replaces only that second paragraph.

The scripted handshake helpers are `frame_ready` (`crates/hermes-client/tests/support/mod.rs:774`), `frame_capabilities` (`support/mod.rs:792`), and `frame_describe` (`support/mod.rs:801`). `frame_ready` sends `"skin": {"name": "default"}` and no colour map. `frame_describe` sends no `skin` key. `opened` is `crates/hermes-client/tests/connect.rs:2238`. `handshake_validates_ready_and_describe` is `connect.rs:2373`. Its `no skin` row removes the ready `skin` key (`connect.rs` test file lines 2419-2422) and expects `BadReady` (lines 2468-2469). Its `no session` row removes `session_create` (lines 2434-2437) and expects `BadDescribe` (line 2475).

`empty_skin_payload_means_no_skin` (`crates/talaria-core/tests/appearance.rs:32`) feeds `Skin::from_value` this object at lines 46-50:

```json
{"name": "mixed", "colors": {"ui_accent": null, "prompt": "#FFF8DC"}, "light_colors": {"ui_ok": 7}, "dark_colors": {}}
```

It expects the null key and the number key in `SkippedValue` (lines 55-67). That is the domain reader, not the handshake. REQ-4 and the "leave unchanged" list keep this test.

`crates/talaria-app/tests/connection.rs` does not exist. The only `open_with_dial` test today is inside `crates/talaria-app/src/connection.rs` at line 292, `supplied_dial_preserves_remote_upgrade_guidance`. Its dial is `Refused` and returns before describe (`connection.rs:296-301`). Test 2 cannot live in that function. It is a new file under `tests/` so the second commit touches no test file.

`jsonl_files` (`crates/hermes-client/tests/fixtures_replay.rs:99`) lists the `*.jsonl` files in `fixtures/frames`. `listed_fixtures` (`fixtures_replay.rs:115-120`) asserts there are 33. `listed_fixtures_exist` (`fixtures_replay.rs:201`) calls `listed_fixtures`. Test 1 therefore builds frames in memory. `socket_pair` (`crates/talaria-app/tests/support/mock_gateway.rs:661-700`) is the loopback test 2 inlines. It wraps the client `TcpStream` as `tokio_tungstenite::MaybeTlsStream::Plain` before `client_async` (`mock_gateway.rs:675-678`). The request string is `ws://127.0.0.1/` (`mock_gateway.rs:676`). `WsSink` and `WsStream` are the split halves of `WebSocketStream<MaybeTlsStream<TcpStream>>` (`connect.rs:28-31`). It is private, so test 2 does not call it. The server half from `accept_async` is a `WebSocketStream<TcpStream>`.

`ConnectionParts::for_test` is public and hidden from docs (`connect.rs:480`). `LaunchFailure` converts `CallError::BadResult` through the non-RPC arm (`crates/talaria-app/src/failure.rs:171-176`), which uses `CallError`'s display string.

## Hermes facts

Read at Hermes v0.23.0, in `hermes_cli/skin_engine.py`.

`_load_skin_from_yaml` (line 369) calls `yaml.safe_load` (line 374). An unquoted `#FFD700` is a comment, so the key's value is null, and the function still returns the dict when `name` is present (lines 375-376).

`_build_skin_config` (line 382) sets `colors=merged("colors")` (line 402). `merged` is `{**default.get(key, {}), **section(key)}` (lines 396-397). A null `prompt` in the file therefore overwrites the built-in string. `light_colors` and `dark_colors` use `section`, not `merged` (line 403).

`load_skin` (line 424) reads `_skins_dir() / f"{name}.yaml"` before the built-in table (lines 426-430). `_skins_dir` is `get_hermes_home() / "skins"` (lines 365-366). `init_skin_from_config` (line 470) reads `display.skin` and falls back to `default` (lines 473-474). The built-in names at line 63 are `default`, `ares`, `mono`, `slate`, `daylight`, `poseidon`, `sisyphus`, and `charizard`. UAT-3 uses `nullcolour`, which is none of those.

## Edge cases

| What is in the JSON | Ready frame | Describe result | `open_with_dial` |
| --- | --- | --- | --- |
| `colors.banner` is null, `colors.prompt` is `#FFD700`, plus a null in `light_colors` and in `dark_colors` | null keys removed, `prompt` kept, `Ok` | same | same, link stays open |
| `skin` key absent | `BadReady` (existing `no skin` row) | `skin` stays `None` | `from_value` gives `skin: None` |
| `skin` is JSON null | helper leaves it, `BadReady` | helper leaves it, `Option` decodes null as `None` | same as the describe column |
| a colour value is a number, a bool, or an object | left in place, `BadReady` | left in place, `BadDescribe` | `BadResult`, link closed |
| a `branding` value is null | left in place, decode fails | left in place, `BadDescribe` | `BadResult`, link closed |
| `session_create` absent | the ready payload has no such field | `BadDescribe` at `connect.rs:571-572`, before the helper | `from_value` fails, link closed |

Test 1 locks the first row on both handshake decodes. Test 2 locks the first row on `open_with_dial`. The existing `no skin` and `no session` rows lock two of the failure rows. The number row is already a failing decode. A new test for it would pass before the change, so the issue does not add one. The stop condition is the guard.

## Why the helper sits here

Settled decision 1 puts one helper in the hand-written override and uses it at both handshake decodes and on the open path. The generated file is the wrong place: [#544](https://github.com/infiquetra/talaria/issues/544) (`PIN-3`) regenerates `schemas.rs`, and a hand edit there would disappear. `prompt_submit_params` is the existing seam of that shape (`overrides.rs:44`), which is why REQ-1 names `skin_payload_lenient` beside it.

`Skin::from_value` is already lenient and runs too late. Calling it from the handshake would still require a successful `SkinPayload` decode first, which is the decode that fails. The helper therefore edits the JSON object before `serde_json::from_value`.

`open_with_dial` never sees the raw JSON if it keeps `Client::call`. `call_with` decodes every method (`rpc.rs:345`). Changing that would strip null colours out of unrelated replies. REQ-3 uses `call_raw` for `runtime.describe` only. `ConnectionService::open` needs no edit of its own, because its describe value is the one `handshake_with` already decoded.

Stripping the null before decode has a consequence for `/theme`. `Skin::from_payload` round-trips the typed value (`crates/talaria-core/src/appearance.rs:48-51`), so the removed key is not a `SkippedValue` and the typed launch handshake does not list it. The later raw read in `crates/talaria-app/src/appearance.rs:582` still reaches `Skin::from_value` (`crates/talaria-core/src/appearance.rs:460`) on `/theme` and on a reconnect refresh. This issue does not change that read. REQ-5 therefore keeps the paragraph that says `Skin::from_value` lists the key, and must not say the handshake lists it.
