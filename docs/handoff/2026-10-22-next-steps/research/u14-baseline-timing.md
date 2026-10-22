# u14-baseline-timing: startup cost of installed agent terminal UIs

Date: 2026-10-22. Machine: this Mac (Darwin 27.0, arm64, 24 CPUs, 128 GB RAM). This unit measures how long each agent terminal tool takes to start and to show a usable first screen, plus its memory and idle processor use. The output feeds the Talaria handoff about turning Talaria into a "Claude Code for Hermes" client. Talaria is the terminal client in `<talaria>` (version 0.6.5). Hermes is the agent backend installed at `<hermes>` (revision 22063fc09, reported by `hermes --version` as `local 22063fc099ec`).

## Plain-English answer

- **Process start (`--version`):** Talaria's `--version` takes a median of 262 ms. Claude Code and Codex are much faster (about 21 ms each), OpenCode takes 74 ms, and Qwen takes 97 ms. Gemini takes about 0.9 s, and Hermes is the slowest at a median of 2.6 s (range 2.1 to 4.3 s). These numbers measure process start only. They say nothing about when a screen is usable.
- **First usable screen:** The fastest were Talaria in replay mode (0.91 s median) and Claude Code in a trusted folder (1.35 s). Qwen Code took 1.59 s. Codex showed its composer in about 0.24 s, which is close to the measurement floor (see caveats). OpenCode took 4.15 s, and Hermes `--tui` took 4.99 s. Claude Code and Gemini CLI, run in this unit's scratch folder, stopped at a folder-trust question after 0.47 s and 2.03 s. That is a question, not a usable screen.
- **Memory:** Talaria replay holds 66 MB. Hermes `--tui` holds 92 MB in its front end plus 113 MB in a Python child, about 205 MB in total. Codex holds about 248 MB. Claude Code in a trusted folder holds about 488 MB, and its process tree about 1.4 GB.
- **Idle processor use (median of 3 runs):** Codex (0.00 percent), Hermes (0.40), Gemini (0.58), OpenCode (0.65), Claude Code at its trust dialog (0.74) and Talaria replay (0.93) each stayed under 1 percent of one core. Qwen idled at about 1.9 percent. Claude Code in a trusted folder idled at about 10 percent. That is probably from the hooks and tool servers it starts, but this unit did not check which process used the time.
- **Biggest confound:** The pane's own shell, in Herdr (a terminal multiplexer, like tmux), took 9 to 19 seconds to show a prompt in most runs, and the machine carried heavy background load throughout (see caveats). Shell start was excluded from every tool number, but it is a real cost of this environment.

## Environment and tool versions

| Tool | Version measured | Path | Notes |
|---|---|---|---|
| talaria | 0.6.5 | `~/.local/bin/talaria` | Live attach needs a credential, so only `--version` and `replay` were measured. |
| hermes | v0.22.2+1692.g22063fc (2026.9.24), stable, local 22063fc099ec | `~/.local/bin/hermes` | `hermes --tui` measured. |
| claude | 2.1.308 (Claude Code) | `~/.local/bin/claude` | |
| codex | codex-cli 0.163.0 | `~/.local/bin/codex` | |
| opencode | v2.1.2 | `~/.opencode/bin/opencode` | A `opencode serve --service` process (pid <pid>) was already running. |
| gemini | 0.57.0 at version timing, then 0.65.0 | `/opt/homebrew/bin/gemini` | Gemini updated itself during an early probe (see anomalies). |
| qwen | 0.25.1 | `~/.local/bin/qwen` | Qwen reported an update to 0.26.0 that installs "after you exit". Still 0.25.1 at the end of the unit. |
| pi | not installed | `~/.cmuxterm/cmux-cli-shims/.../pi` | The path is a cmux shim. It exits with status 127 and prints "Configured agent is unavailable." Not timed as a UI. |
| hyperfine | not installed | none | The `--version` timing used a Python script instead. |

Machine state during the runs: the one-minute load average before each timed run ranged from 9.3 to 31.3. The "rest of machine" processor use (all processes except the tool's own tree) ranged from about 670 to 1,960 percent, where 2,400 percent is all 24 cores. Other agent sessions and Hermes services were running. Numbers from this unit are therefore measurements under heavy background load, not on a quiet machine.

Tools present and timed: talaria, hermes, claude, codex, opencode, gemini, qwen. Missing: hyperfine (replaced by Python timing), pi (not a real agent on this machine). Talaria's UI was timed only in replay mode.

## Method

### 1. Process start (`--version`)

Script: `<scratch>/research/version_timing.py`. For each tool it runs `subprocess.run([tool, "--version"])` with output discarded and times it with `time.perf_counter()`. It does 2 untimed warm-up runs, then 10 timed runs. The 90th percentile uses the nearest-rank method on the 10 timed runs. Raw output: `research/version_timing.json`. Gemini was re-timed after its update with `research/version_retime_gemini.py` (raw: `research/version_timing_gemini_0.65.0.json`).

Command used: `python3 <scratch>/research/version_timing.py`

### 2. Time to first usable frame, and 3. resident memory, and 4. idle processor use

These were measured by driving each tool in its own Herdr pane. Herdr is the terminal multiplexer this session runs in (`HERDR_ENV=1`). The tool that controls it is the `herdr` CLI, used as the skill directs (`herdr --skill`).

- Tab: created one tab, `tui-timing` (tab `<tab>`, root pane `<pane>`), in the current workspace `<workspace>`. Other tabs (`Lead`, `Terminal`, `talaria-eval`) were not touched. The tab was closed at the end (`herdr tab close <tab>` returned `ok`).
- Each run: `herdr pane split <pane> --direction right --cwd <scratch dir> --no-focus` creates a fresh pane to the right of the root pane. Every timed run recorded 42 viewport rows (`viewport_rows` in each raw file). The split ratio was not set, so Herdr's default applies. Its value was not checked.
- Shell readiness: the harness polls `herdr pane process-info` and `herdr pane read` until the pane's shell is the only foreground process and the prompt text (`<user>:`) is visible. The time this takes is recorded as `shell_ready_s` and is excluded from the tool timing.
- Submission: `herdr pane run <pane> "<command>"`. The clock starts just before this call.
- Polling: every 100 ms (plus a Herdr CLI round trip, median 9 ms over 12 calls, in `research/shell_and_herdr_latency.json`), the harness reads the pane with `herdr pane read <pane> --source visible` and checks for a ready pattern. "First seen" is the first poll that matches. "Stable" is the second consecutive matching poll, so stable is about 0.1 s after first seen. The headline figure is stable.
- Dialogs: where a first-run, login or folder-trust dialog appears, the harness records the first frame as that dialog. It does not accept it.
- Memory: at 5 s after the stable frame, the harness reads resident set size (`ps -axo pid=,ppid=,rss=`) for the tool's process and for its whole child tree.
- Idle processor use: for 20 seconds, starting 5 s after the frame, the harness samples cumulative CPU time for the tool's process tree and for the rest of the machine once per second. The macOS `ps` manual describes `%cpu` as a decaying average over up to a minute of earlier time, which is too smoothed for a 20-second window, so it was not used. The harness uses cumulative CPU time (`ps -o time=`, 0.01 s resolution) instead. Each one-second delta, divided by the elapsed time, gives percent of one core.
- Exit: `ctrl+c` twice for Claude, Codex, OpenCode, Gemini and Hermes. `ctrl+d` then `ctrl+c` twice for Qwen. `ctrl+q` for Talaria, as its documentation says (`docs/terminal-ui.md`, line 224). Then `herdr pane close`. Where a tool ignored the keys, closing the pane ended it. The process checks made after the runs found no leftover processes from these panes.

Driver: `python3 <scratch>/research/timing_run.py <config> 3`. Harness code: `research/harness.py`. Summaries: `research/summarize.py` writes `research/summary.json`.

Launch commands and ready patterns (the scratch directory is `<scratch>`):

| Config | Command | Ready pattern (regex on visible text) | Dialog pattern |
|---|---|---|---|
| claude | `claude` | `for shortcuts` (not reached in scratch folder) | `Quick safety check` |
| codex | `codex` | `Ask Codex to do anything` | none |
| opencode | `opencode` | `Ask anything` | none |
| gemini | `gemini` | `Type your message` (not reached in scratch folder) | `Do you trust the files in this folder` |
| qwen | `qwen` | `Type your message or @path/to/file` | none |
| hermes_tui | `hermes --tui` | `─ ready │` or `› Try "` | none |
| talaria_replay | `talaria replay ~/.talaria/recordings/2026-08-04T16-02-24-964Z.jsonl` | `Enter sends` | none |
| claude_talaria_cwd (supplementary) | `cd <talaria> && claude` | `bypass permissions on` or `for shortcuts` | `Quick safety check` |
| gemini_talaria_cwd (supplementary) | `cd <talaria> && gemini` | `Type your message` | `Terms of Services and Privacy Notice`, `Use Enter to select`, `Do you trust the files in this folder` |

The two supplementary configs run in the Talaria repository, because the scratch folder is untrusted for Claude and Gemini. Claude records `hasTrustDialogAccepted: true` for the Talaria repository in `~/.claude.json`. Gemini's trusted-folder list (`~/.gemini/trustedFolders.json`) does not name the Talaria repository directly. It does name `~`, its parent, and whether that trust carries down was not checked. The frame Gemini showed there was a terms screen, not a trust question.

## Results

### 1. Process start, `--version` (10 timed runs after 2 warm-ups, milliseconds)

| Tool | Median | 90th percentile | Min | Max | Exit code |
|---|---|---|---|---|---|
| talaria 0.6.5 | 261.7 | 286.3 | 176.4 | 287.6 | 0 |
| hermes v0.22.2 | 2622.2 | 3873.4 | 2111.5 | 4315.6 | 0 |
| claude 2.1.308 | 21.1 | 21.5 | 20.1 | 21.6 | 0 |
| codex 0.163.0 | 20.7 | 21.1 | 19.8 | 41.2 | 0 |
| opencode 2.1.2 | 73.5 | 77.2 | 69.8 | 128.4 | 0 |
| gemini 0.57.0 (before its update) | 923.5 | 971.5 | 907.5 | 977.8 | 0 |
| gemini 0.65.0 (after its update) | 876.4 | 918.5 | 861.9 | 919.0 | 0 |
| qwen 0.25.1 | 96.9 | 132.1 | 71.4 | 135.1 | 0 |
| pi (cmux shim, not installed) | 22.3 | 23.9 | 20.7 | 24.2 | 127 |

### 2. Time to first usable frame (3 runs each, seconds, stable frame measured from command submission)

Sorted by median. The "first seen" column is the first poll that matched, before the stability check.

| Tool (folder) | First frame | Median stable (s) | Range (s) | Median first seen (s) |
|---|---|---|---|---|
| Talaria replay (scratch) | Replay composer | 0.914 | 0.687 to 1.311 | 0.803 |
| Claude Code (Talaria repo, trusted) | Composer | 1.353 | 1.164 to 2.385 | 1.242 |
| Qwen Code 0.25.1 (scratch) | Composer | 1.594 | 1.587 to 1.709 | 1.480 |
| Gemini CLI 0.65.0 (Talaria repo) | Terms of Service notice (first-run dialog) | 1.961 | 1.881 to 2.073 | 1.961 |
| Gemini CLI (scratch) | Folder-trust dialog | 2.029 | 2.022 to 2.222 | 2.029 |
| Codex 0.163.0 (scratch) | Composer | 0.240 | 0.239 to 0.242 | 0.127 |
| Claude Code 2.1.308 (scratch) | Folder-trust dialog | 0.465 | 0.461 to 0.906 | 0.465 |
| OpenCode 2.1.2 (scratch) | Composer | 4.153 | 2.251 to 5.101 | 4.041 |
| Hermes `--tui` (scratch) | Composer, status line "ready" | 4.992 | 4.042 to 6.655 | 4.880 |

Rows marked "dialog" are not usable screens. A folder-trust or terms dialog appears before the tool accepts input. The harness did not accept any dialog.

### 3. Pane shell readiness (excluded from the table above)

| Pane | Median (s) | Range (s) |
|---|---|---|
| Claude (scratch) | 9.32 | 8.67 to 9.35 |
| Codex (scratch) | 9.66 | 8.51 to 10.33 |
| OpenCode (scratch) | 9.49 | 9.33 to 9.78 |
| Gemini (scratch) | 9.07 | 8.97 to 9.97 |
| Qwen (scratch) | 9.54 | 9.48 to 10.28 |
| Hermes `--tui` (scratch) | 15.40 | 9.42 to 17.05 |
| Talaria replay (scratch) | 18.69 | 17.09 to 20.98 |
| Claude (Talaria repo) | 16.33 | 4.16 to 23.71 |
| Gemini (Talaria repo) | 1.89 | 1.89 to 1.99 |

A login bash shell outside Herdr, run as `bash -l -i -c exit`, took 1.80 to 3.73 s over 3 runs (`research/bash_startup_probe.json`). The shell reads the operator's shell profile, which evaluates `oh-my-posh init bash`. That init alone took 0.35 s. The gap between the Herdr pane (9 to 19 s typical) and a plain login shell (2 to 4 s) is not explained. Hypothesis, unverified: the Herdr pane start competes with the heavy background load, or the Herdr pane shell does more work than a plain login shell.

### 4. Resident memory and idle processor use (median of 3 runs)

RSS is resident set size, the memory a process holds in RAM. "Root" is the tool's own process. "Tree" includes its child processes. Idle CPU is the tool's tree averaged over 20 seconds, starting 5 s after the stable frame. "Rest of machine" is the average processor use of everything else on the Mac over the same window, in percent of one core, where 2,400 percent is all 24 cores.

| Tool (folder) | Root RSS (MB) | Tree RSS (MB) | Idle CPU, tree (%) | Rest of machine (%) |
|---|---|---|---|---|
| Talaria replay | 65.8 | 65.8 | 0.93 | 1,575.5 |
| Hermes `--tui` | 92.3 | 204.8 | 0.40 | 1,371.4 |
| Codex | 247.5 | 247.5 | 0.00 (range 0.00 to 0.34) | 814.8 |
| OpenCode | 212.4 | 212.4 | 0.65 (range 0.59 to 0.70) | 719.9 |
| Gemini (scratch, dialog) | 187.5 | 465.6 | 0.58 (range 0.57 to 1.34) | 790.5 |
| Gemini (Talaria repo, dialog) | 185.0 | 464.5 | 0.62 (range 0.58 to 1.39) | 914.8 |
| Claude (scratch, dialog) | 271.3 | 271.3 | 0.74 (range 0.74 to 0.78) | 765.5 |
| Qwen (scratch) | 263.5 | 507.1 | 1.92 (range 1.92 to 2.12) | 939.1 |
| Claude (Talaria repo) | 487.6 | 1,411.3 | 9.96 (range 4.33 to 11.11) | 1,699.2 |

### 5. Raw per-run numbers

All times in seconds unless stated. Shell = pane shell ready. First seen and stable are measured from command submission. Memory is in MB at 5 s after the stable frame. Load is the one-minute load average taken just before the run. Source: `research/raw/<config>_run<n>.json` and `research/summary.json`.

| Config | Run | First frame | Shell | First seen | Stable | Root RSS | Tree RSS | Idle CPU tree % | Rest of machine % | Load 1 min before |
|---|---|---|---|---|---|---|---|---|---|---|
| claude | 1 | dialog | 8.666 | 0.906 | 0.906 | 271 | 271 | 0.78 | 765.6 | 9.66 |
| claude | 2 | dialog | 9.349 | 0.461 | 0.461 | 271 | 271 | 0.74 | 765.5 | 9.25 |
| claude | 3 | dialog | 9.324 | 0.465 | 0.465 | 272 | 272 | 0.74 | 735.9 | 10.83 |
| codex | 1 | ready | 10.329 | 0.129 | 0.242 | 251 | 251 | 0.00 | 820.6 | 14.67 |
| codex | 2 | ready | 9.663 | 0.127 | 0.239 | 248 | 248 | 0.34 | 791.3 | 13.07 |
| codex | 3 | ready | 8.513 | 0.126 | 0.240 | 243 | 243 | 0.00 | 814.8 | 11.98 |
| opencode | 1 | ready | 9.494 | 4.041 | 4.153 | 212 | 212 | 0.65 | 849.5 | 11.93 |
| opencode | 2 | ready | 9.326 | 2.137 | 2.251 | 216 | 216 | 0.70 | 719.9 | 12.04 |
| opencode | 3 | ready | 9.784 | 4.989 | 5.101 | 212 | 212 | 0.59 | 673.6 | 11.36 |
| gemini (scratch) | 1 | dialog | 9.967 | 2.222 | 2.222 | 189 | 466 | 0.57 | 790.5 | 10.05 |
| gemini (scratch) | 2 | dialog | 9.071 | 2.029 | 2.029 | 188 | 468 | 0.58 | 782.3 | 9.56 |
| gemini (scratch) | 3 | dialog | 8.969 | 2.022 | 2.022 | 172 | 451 | 1.34 | 794.6 | 9.89 |
| qwen | 1 | ready | 9.535 | 1.475 | 1.587 | 264 | 507 | 1.92 | 789.5 | 9.90 |
| qwen | 2 | ready | 9.482 | 1.480 | 1.594 | 264 | 508 | 1.92 | 939.1 | 10.64 |
| qwen | 3 | ready | 10.278 | 1.600 | 1.709 | 263 | 507 | 2.12 | 1119.4 | 10.36 |
| hermes_tui | 1 | ready | 9.421 | 3.932 | 4.042 | 91 | 206 | 0.40 | 1275.0 | 9.73 |
| hermes_tui | 2 | ready | 15.399 | 4.880 | 4.992 | 92 | 203 | 0.40 | 1371.4 | 14.05 |
| hermes_tui | 3 | ready | 17.053 | 6.540 | 6.655 | 92 | 205 | 0.39 | 1521.0 | 18.50 |
| talaria_replay | 1 | ready | 18.690 | 1.198 | 1.311 | 66 | 66 | 0.93 | 1626.9 | 20.84 |
| talaria_replay | 2 | ready | 17.093 | 0.577 | 0.687 | 66 | 66 | 0.93 | 1575.5 | 21.57 |
| talaria_replay | 3 | ready | 20.979 | 0.803 | 0.914 | 66 | 66 | 0.93 | 1490.5 | 21.35 |
| claude_talaria_cwd | 1 | ready | 23.710 | 2.264 | 2.385 | 464 | 1363 | 11.11 | 1962.8 | 30.80 |
| claude_talaria_cwd | 2 | ready | 16.333 | 1.242 | 1.353 | 498 | 1411 | 9.96 | 1699.2 | 31.33 |
| claude_talaria_cwd | 3 | ready | 4.164 | 1.049 | 1.164 | 488 | 1567 | 4.33 | 823.8 | 29.92 |
| gemini_talaria_cwd | 1 | dialog | 1.888 | 1.881 | 1.881 | 185 | 465 | 1.39 | 937.5 | 14.17 |
| gemini_talaria_cwd | 2 | dialog | 1.891 | 2.073 | 2.073 | 172 | 452 | 0.58 | 914.8 | 12.12 |
| gemini_talaria_cwd | 3 | dialog | 1.987 | 1.961 | 1.961 | 188 | 470 | 0.62 | 891.4 | 12.09 |

Runs that did not count, kept for transparency in `research/raw/`:
- `hermes_tui_regex_miss_run1.json` to `run3.json`: the first ready pattern did not match the Hermes status line. Each run timed out at 90 s. The pattern was corrected and the three runs above replaced them.
- `claude_talaria_cwd_regex_miss_run1.json`: the first Claude pattern (`for shortcuts`) did not match the `bypass permissions` footer. Corrected, then three runs above.
- `gemini_talaria_cwd_undetected_dialog_run1.json` to `run3.json`: the dialog pattern did not match the terms-of-service screen. Corrected, then three runs above.

A single probe run of each tool was made before the timed runs, to learn each first screen. Those probes are not in the tables.

## Anomalies and side effects

These are things this unit did to the machine, or saw on it, that the handoff needs to know about.

1. **Gemini updated itself during a probe.** The early probe launched `gemini` in the scratch folder. Gemini printed "Update successful! The new version will be used on your next run." The Talaria-repo Gemini frame then showed `Gemini CLI v0.65.0`, and `gemini --version` now reports 0.65.0. The three scratch-folder Gemini UI runs came after the probe. Which Gemini version they ran is unverified, because their raw frames do not show the version banner. The `--version` timing for 0.57.0 was taken before the update, and the 0.65.0 timing was taken after it.
2. **Qwen announced an update.** On exit it printed "Qwen Code update available! 0.25.1 → 0.26.0. The update will be installed after you exit this session." Qwen still reported 0.25.1 at the end of this unit. Whether it installs later is unverified.
3. **Folder-trust and terms dialogs were not accepted.** Claude and Gemini stopped at a folder-trust question in the scratch folder. Gemini in the Talaria repo stopped at a terms-of-service screen. Each was exited with `ctrl+c`, as the rules require. Nothing was accepted or written to configuration.
4. **Hermes `--tui` started a Python child process.** The TUI's bootstrap script `<hermes>/ui-tui/scripts/gateway_bootstrap.py` line 72 calls `ensure_gateway_runtime(home, timeout=30, idle_exit=True)` when the TUI starts. In the probe, the TUI's process tree had a Python child (pid <pid>, run as `python3 -I -c ...`) with the Node front end (pid <pid>) under it. The child looks like the gateway launcher, so the tree-memory figures include it. That reading is unverified. When the TUI exited, neither process remained. A separate `hermes_cli.main gateway run --quiet --idle-exit` process (pid <pid>, parent pid 1, started 00:54:00 on 2026-10-22) was still running at the end. I did not start it on purpose. The bootstrap code can start that exact command, so my probe may be its origin. The timing gap (about six minutes after the probe's session start at 00:48:31) makes that unverified. A `tui_gateway.slash_worker` (pid <pid>, parent the dashboard <pid>) for the probe's session key `20261022_004831_cd8d94` was present after the probe and gone by the later check.
5. **Hermes sessions were created.** The TUI shows a session ID when it starts (the probe showed `Session: 5ac5db5b`), so each start likely creates a session. A read-only query of `~/.hermes/state.db` for today's session IDs returned no rows, so the number of sessions created is unverified.
6. **Hermes `--tui` opened with the <work-profile> profile.** The composer reads `<work-profile> <Profile> ›`. I did not change any profile setting.
7. **No Hermes process was stopped or restarted.** The baseline Hermes processes were still alive at the end: the dashboard (pid <pid>), the osascript-launched gateway chain (pids <pid>, <pid>, <pid>), and pids <pid> and <pid>. The rule "do not start Hermes processes" conflicts with the requested measurement of `hermes --tui`. I read that rule as forbidding changes to the running Hermes services, not as forbidding the TUI client this unit was asked to measure. The orchestrator should confirm that reading.
8. **Qwen, Claude (Talaria repo) and some OpenCode runs ignored `ctrl+c`.** Closing the pane ended them. No leftovers were found after closing.
9. **Talaria replay exit.** `q` and `ctrl+c` did nothing. `ctrl+q` quits, as the documentation says.
10. **Background load was heavy and rose through the session.** Rest-of-machine processor use was about 670 to 1,960 percent. The one-minute load average rose from about 9 to 31 over the unit. Each run's figure is recorded, but the trend means the later runs faced more contention.

## Caveats and limits

- **Talaria was not measured live.** It needs a credential to attach. Replay mode is a different screen and may not reflect live startup or idle cost. In replay, the footer reads "replay — not connected".
- **Codex's 0.24 s is near the floor of this method.** The first poll that saw the composer came 0.127 s after submission. The poll interval is 100 ms, and each Herdr call takes about 9 ms. The process start time from `ps -o lstart` has only one-second resolution, so it cannot confirm the sub-second figure. Read Codex's number as "under about a quarter second", not as an exact value.
- **Each UI figure comes from three runs.** A 90th percentile from three values would be meaningless, so the table shows the range instead. Spread is large for OpenCode (2.3 to 5.1 s) and Hermes (4.0 to 6.7 s).
- **Shell start dominates the wall clock but is excluded from the tool figures.** A user typing into a fresh terminal would feel the pane shell delay too. That is not part of the tool's own startup.
- **The scratch folder changes what some tools show.** Claude and Gemini stopped at trust dialogs there. Their Talaria-repo runs show composers (Claude) or a terms screen (Gemini). The two folders are not interchangeable.
- **Idle CPU was measured over 20 s after the first frame.** It excludes any work a tool does later, and it ran under heavy background load. Claude in a trusted folder runs hooks and tool servers that may account for its 10 percent.
- **Memory is a single sample per run,** taken at 5 s. Memory for Claude in the Talaria repo varied from 1.36 to 1.57 GB across runs.
- **`--version` measures process start only.** It does not measure when a UI is ready. Hermes's 2.6 s `--version` time may reflect Python imports, but that is a hypothesis, not verified.
- **Pane-based timing is not the same as a human measurement.** The clock starts when the harness sends the command, and it stops when the harness sees the screen.

## Exact commands

Version timing:

```
python3 <scratch>/research/version_timing.py
python3 <scratch>/research/version_retime_gemini.py
```

Version output checks:

```
talaria --version        # talaria 0.6.5
hermes --version         # Hermes Agent v0.22.2+1692.g22063fc (2026.9.24) · stable · local 22063fc099ec
claude --version         # 2.1.308 (Claude Code)
codex --version          # codex-cli 0.163.0
opencode --version       # opencode v2.1.2
gemini --version         # 0.57.0 before update, 0.65.0 after
qwen --version           # 0.25.1
pi --version             # Configured agent is unavailable. (exit 127)
```

Herdr setup and teardown:

```
herdr --skill
herdr tab create --workspace <workspace> --cwd <scratch> --label tui-timing --no-focus    # -> tab <tab>, root pane <pane>
herdr tab close <tab>                                                              # -> {"type":"ok"}
```

Per-run Herdr calls (inside `harness.py`):

```
herdr pane split <pane> --direction right --cwd <scratch> --no-focus
herdr pane process-info --pane <pane>
herdr pane run <pane> "<command>"
herdr pane read <pane> --source visible          # polled about every 100 ms
herdr pane send-keys <pane> ctrl+c               # ctrl+q for Talaria; ctrl+d then ctrl+c for Qwen
herdr pane close <pane>
```

Timed trials and summary:

```
python3 .../research/timing_run.py <config> 3
python3 .../research/summarize.py
```

Configs used: `claude`, `codex`, `opencode`, `gemini`, `qwen`, `hermes_tui`, `talaria_replay`, `claude_talaria_cwd`, `gemini_talaria_cwd`. Where `.../` is `<scratch>/`.

Other checks:

```
ps -axo pid,ppid,rss,pcpu,etime,command            # process list
ps -o pid=,ppid=,etime=,lstart=,command= -p <pids> # start times
sysctl -n vm.loadavg
top -l 2 -s 1 -n 0 | grep "^CPU usage"            # machine CPU sanity check: 38.38% user, 14.22% sys, 47.39% idle
bash -l -i -c exit                                  # login-shell startup, 3 runs
oh-my-posh init bash --config ~/default-posh-prompt.omp.json   # 0.35 s
sqlite3 -readonly ~/.hermes/state.db "select id, started_at from sessions where id like '20261022_%';"   # no rows
```

## Files

- Report: `<scratch>/research/u14-baseline-timing.md` (this file)
- Version timing: `.../research/version_timing.py`, `.../research/version_timing.json`, `.../research/version_retime_gemini.py`, `.../research/version_timing_gemini_0.65.0.json`
- Harness and driver: `.../research/harness.py`, `.../research/timing_run.py`, `.../research/summarize.py`
- Summary: `.../research/summary.json`
- Raw per-run records: `.../research/raw/` (one JSON per run, including the full first frame text and per-second CPU series)
- Shell and Herdr latency: `.../research/shell_and_herdr_latency.json`, `.../research/bash_startup_probe.json`
- Tab creation response: `.../research/tab_create.json`

Here `...` stands for `<scratch>`.

## What this means for the handoff

- The numbers give a baseline to beat. For a Talaria client, the first screen should come in under about one second, which Talaria replay already does. The live client is not yet measured.
- Hermes `--tui` takes about five seconds to show a usable screen and about 205 MB of memory in this environment. A Talaria client that talks to the running Hermes gateway might avoid starting its own, but that is a design question, not a measured result.
- Claude Code's idle processor use in a trusted folder (about 10 percent, 1.4 GB tree) is a warning about hooks and tool servers, not about the UI itself.
- Before any of these numbers are quoted, re-run on a quiet machine. The current load (1-minute average 9 to 31, rest-of-machine processor use 670 to 1,960 percent) is far above what a clean comparison needs.
