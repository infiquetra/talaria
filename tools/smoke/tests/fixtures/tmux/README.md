# Recorded `tmux` output for the phase 2a suite

Recorded from the real `tmux` command line, version `tmux 3.7c` (the text of
`version.txt`), on macOS, against a throwaway server started with an isolated socket
path and killed afterwards (`kill-server`; no other session was touched). Compare against
the version in hand before trusting these.

After recording, the socket path was replaced by `<tmp>`. The empty file is a call that
prints nothing and exits 0. `load-buffer`, `paste-buffer` and a successful `kill-session`
also print nothing on success (verified on the same server), so no recorded output exists
for them and no file is kept.

| File | The call | Exit | How it was made |
|---|---|---|---|
| `version.txt` | `tmux -V` | 0 | recorded |
| `has_session_ok.txt` | `tmux has-session -t <session>` against an existing session | 0 | recorded; **empty** |
| `has_session_missing.txt` | `tmux has-session -t talaria-smoke-absent` with the server up and the session missing | 1 | recorded (stderr) |
| `has_session_no_server.txt` | `tmux has-session -t <session>` with no server on the socket | 1 | recorded (stderr); socket scrubbed to `<tmp>` |
| `kill_session_missing.txt` | `tmux kill-session -t talaria-smoke-absent` with the server up and the session missing | 1 | recorded (stderr) |

No fixture holds a path, a home directory, a login name or a profile name other than
`fixture`; `fixtures_public_safe2a.rs` checks this.
