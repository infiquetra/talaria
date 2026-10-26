# Fleet list, version 1

`talaria fleet list --json` writes one `talaria.fleet.v1` snapshot object followed by a newline.
`talaria fleet list --watch --json` writes the snapshot as its first line, then one object per
line for each change. Every object carries `"format": "talaria.fleet.v1"` and a `kind`. Errors
are plain text on standard error; standard output then holds nothing.

## The snapshot

```json
{"format":"talaria.fleet.v1","kind":"snapshot","as_of":1792690000.5,
 "scope":{"host":true,"profile":null,"why":null},
 "profiles":[{"name":"fixture","status":"listed","truncated":false,"reason":null}],
 "rows":[{"profile":"fixture","session_id":"local-3f2a","title":"","model":"minimax-m3.1-flash",
          "source":"tui","state":"needs_you","state_since":1792690010.0,"outcome":null,
          "message_count":4,"foreground":false,"stale":false,"gone":false,"unlisted":false}],
 "needs_you":[{"profile":"fixture","session_id":"local-3f2a","slot":"prompt","prompt_id":"a1b2",
               "kind":"approval","summary":"rm -rf ./talaria-fleet-probe",
               "choices":["once","deny","session","always"],
               "seq":1,"since":1792690010.0,"exact":false,"execution_generation":null}],
 "diagnostics":[]}
```

| Key | Value |
| --- | --- |
| `as_of` | Seconds since 1970 when the snapshot was taken. |
| `scope` | `{"host":true,"profile":null,"why":null}`, or `{"host":false,"profile":"<name>","why":"the gateway refused a host-scoped ticket"}` when only the foreground profile is in view. |
| `profiles` | Served profiles in served order, then parked ones. |
| `rows` | Sessions in display order: needs you, working, failed, done, idle. |
| `needs_you` | Open needs, oldest first (`seq` rises and never changes for one need). |
| `diagnostics` | At most 20 distinct notes about replies Talaria could not use. Text output never prints them. |

Field values:

| Field | Values |
| --- | --- |
| profile `status` | `loading`, `listed`, `failed`, `parked`, `out_of_scope`. `reason` is set for `failed` and `parked`, is `"outside the profile-only view"` for `out_of_scope`, else `null`. `truncated` is true only for a `listed` profile whose list returned 200 sessions, Hermes's limit; a working session beyond them has no row. |
| row `state` | `needs_you`, `working`, `failed`, `done`, `idle`. |
| row `outcome` | `completed`, `interrupted`, `failed`, `unknown`, `null`. Only `done` and `failed` rows hold one. |
| row `stale` | The last read of the row's profile failed, or the link dropped; the state is the last one known. |
| row `gone` | The session left the list while its finished turn was not acknowledged. |
| row `unlisted` | A waiting session the list does not show (hidden, or past the 200-row limit). |
| need `slot` | `prompt`, `masked`, `waiting`. `prompt_id` is the prompt id for `prompt`, the request id for `masked`, `null` for `waiting`. |
| need `kind` | `approval`, `question`, `sudo`, `secret`, `vault_code`, `vault_unlock`, `waiting`. |
| need `summary` | The command (approval) or the question (question), raw text; `null` otherwise. |
| need `exact` | True only while a Talaria view attached to the session reports the need; such a need has its `execution_generation`. A need with `exact: false` is never answered as it stands. |

A one-shot listing has no history, so no row is `done` or `failed` in it.

## Watch lines

After the first snapshot, `--watch --json` writes only these kinds:

| `kind` | Fields |
| --- | --- |
| `row` | `row`: the whole row object, when a row appears or any of its fields changes. |
| `row_removed` | `profile`, `session_id`. |
| `need_opened` | `need`: the whole need object. |
| `need_closed` | `profile`, `session_id`, `slot`, `prompt_id`. |
| `attention` | `alert` (`needs_you`, `done`, `failed`), `profile`, `session_id`. |
| `profile` | `profile`: one element of `profiles`, when its status changes. |
| `link` | `link` (`connected`, `reconnecting`, `failed`) and `reason` (a string or `null`). |

The needs open when the watch starts are in the first snapshot; no `need_opened` line repeats them.
When a waiting placeholder learns its prompt, the placeholder's `need_closed` and the prompt's
`need_opened` both appear, and the prompt keeps the placeholder's `seq`.

## Exit codes

0 success (also when a profile's reads fail: it shows as not listed); 1 no gateway for the Hermes
home, or no attach within 15 seconds; 2 a usage error, or `--profile` names a profile that is not
served or is parked; 130 Ctrl+C during `--watch`.
