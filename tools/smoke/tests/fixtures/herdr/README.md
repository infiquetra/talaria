# Recorded `herdr` output

Recorded from the real `herdr` command line, version `herdr 0.9.4` (the text of `version.txt`), on
macOS, with one throwaway tab that was created, exercised and closed again (the tab was confirmed
gone from `herdr tab list` afterwards). Compare against the version in hand before trusting these.

After recording, every identifier and every path was replaced by a placeholder: the workspace is
`w1`, the tab `w1:t5`, the first pane `w1:p1G`, the pane made by a split `w1:p2H`, every folder is
`<scratch>`, terminal ids are `term_x`, process ids are made up. The shell prompt of the recording
machine was replaced by `$ ` in the two pane text files, and the labels of the operator's other
tabs were dropped from the tab lists (one neutral tab named `Terminal` stays so the list is not
empty after a close).

| File | The call | How it was made |
|---|---|---|
| `tab_create.json` | `tab create --workspace <w> --cwd <dir> --label <label> --no-focus` | recorded, placeholders applied; `number` was set to 5 to match the tab id |
| `pane_split.json` | `pane split <pane> --direction right --no-focus --cwd <dir>` | recorded, placeholders applied |
| `pane_run_ok.txt` | `pane run <pane> <text>` | recorded; **empty**: the call prints nothing and exits 0 |
| `send_text_ok.txt` | `pane send-text <pane> <text>` | recorded; **empty**: it prints nothing and exits 0 |
| `send_keys_ok.txt` | `pane send-keys <pane> <key>...` | recorded; **empty**: it prints nothing and exits 0 |
| `pane_read_visible.txt` | `pane read <pane> --source visible` | recorded; prompt replaced by `$ ` |
| `pane_read_recent_unwrapped.txt` | `pane read <pane> --source recent-unwrapped --lines 3000` | recorded right after the visible read; prompt replaced by `$ ` (plain text, not JSON; this source had no trailing newline) |
| `pane_process_info_shell.json` | `pane process-info --pane <pane>` | recorded at a quiet prompt, process ids made up |
| `pane_process_info_talaria.json` | the same call while the binary runs | **derived**: the shell file with its one foreground entry replaced by an entry whose `cmdline` is `<talaria> --profile fixture` (argv, argv0 and name to match) |
| `tab_list.json` | `tab list --workspace <w>` | recorded while the throwaway tab (with two panes) was open; the operator's other tabs reduced to one neutral `Terminal` tab |
| `tab_list_without.json` | `tab list --workspace <w>` | recorded after `tab close`; the same reduction; the smoke tab is gone |
| `tab_close_ok.json` | `tab close <tab>` | recorded |
| `version.txt` | `--version` | recorded |
| `error_invalid_key.json` | `pane send-keys <pane> bogus-key` | recorded (exit 1, JSON on stderr) |
| `error_tab_not_found.json` | `tab close <tab>` a second time | recorded (exit 1, JSON on stderr) |
| `error_pane_not_found.json` | `pane read <pane>` after the tab was closed | recorded (exit 1, JSON on stderr) |

That is sixteen files. `herdr pane wait-output` is never called by the smoke test, so no timeout
fixture exists. No fixture holds a path, a home directory, a login name or a profile name other than
`fixture`; `fixtures_public_safe.rs` checks this.
