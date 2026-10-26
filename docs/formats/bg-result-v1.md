# Background launch result, version 1

`talaria --bg --output-format json` writes one `talaria.bg.v1` object followed by a newline, for
every exit except 2 (a usage error, which writes nothing on standard output). Warnings are also
printed on standard error.

```json
{"format":"talaria.bg.v1","status":"launched","session_id":"local-3f2a","stored_session_id":"local-3f2a","profile":"fixture","admission_id":"adm-1","admission_status":"queued","model":"minimax-m3.1-flash","cwd":"/home/me/proj/.worktrees/talaria-add-a-test","worktree":{"path":"/home/me/proj/.worktrees/talaria-add-a-test","branch":"talaria/add-a-test","base_ref":"origin/main","base_commit":"0123456789abcdef0123456789abcdef01234567","base_fetched":true},"warnings":[],"error":null}
```

Keys appear in exactly this order.

| Key | Value |
| --- | --- |
| `status` | `launched`, `unknown`, `not_started` or `interrupted`. |
| `session_id` | The live session id, used by `talaria fleet list` and the worktree record; `null` when no session is known. |
| `stored_session_id` | The id `talaria --resume` takes; falls back to `session_id`; `null` when no session is known. |
| `profile` | The profile the session was created in. |
| `admission_id`, `admission_status` | The admission Hermes returned (`queued`, `started`, `unknown` or `terminal`); `null` unless `launched`. |
| `model` | The model the session reported, or `null`. |
| `cwd` | The folder the session works in: the worktree, or the launch folder. |
| `worktree` | The tree this launch created, or a tree that was kept (`unknown`, or a tree the rollback guard kept); else `null`. |
| `warnings` | Warnings that did not fail the launch, such as a fetch that fell back to the copy fetched earlier. |
| `error` | `{"what":"...","next_step":"..."}` for every status but `launched`; else `null`. |

| Exit | `status` | Meaning |
| --- | --- | --- |
| 0 | `launched` | Hermes admitted the prompt; the session runs in the gateway after Talaria exits. |
| 1 | `unknown` | The prompt was sent and its reply was lost; it may be running. Check `talaria fleet list` before launching it again. The tree is kept. |
| 4 | `not_started` | Nothing was admitted; any tree this launch made was removed unless the text names it as kept. Retrying is safe. |
| 130 | `interrupted` | A signal arrived before the prompt was sent; nothing was started. A signal after that is ignored. |

The launch never sends a title or a finite turn: approvals wait for an answer in the fleet view
and fail closed after Hermes's `approvals.timeout`.

Version changes when a key is removed or its meaning changes. Adding a key keeps version 1;
readers should tolerate additional keys.
