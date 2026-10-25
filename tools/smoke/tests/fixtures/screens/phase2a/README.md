# Phase 2a screens for the smoke suite

Twenty-one screens the phase 2a steps check, all recorded on 2026-10-25 from a debug build of
`talaria` from `main` at `d4deb0ed`, with Herdr 0.9.4 on macOS, against the pinned Hermes
v0.22.4 gateway with the `fixture` profile and `minimax-m3.1-flash` on `ollama-cloud`. The two
marked "fixed build" came from the same build with two local changes that do not reach those
screens: the order of the Talaria commands, and the paste routing of a line that starts with
`/`. Each run used a throwaway Herdr tab in a throwaway folder holding the smoke
suite's project files; the tab was closed afterwards. Reads were `herdr pane read <pane>
--source visible` unless noted.

Each file is a contiguous excerpt of one read, at most 60 lines, with trailing spaces removed.
Scrubbing: the throwaway folder is written `<scratch>`, and a path under it is written
`<scratch>:` followed by its relative name, so the fixture scan does not read it as an
absolute path; `<tmp>` stands for the temporary root in the two files taken from a smoke run's
own (already scrubbed) failure report. No file holds a home directory, a login name, a host
name or a profile name other than `fixture`; `fixtures_public_safe2a.rs` checks this.

| File | How it was made |
|---|---|
| `tool_shell_failed.txt` | recorded: the smoke run's failure report (recent text of the first pane), from the tool-shell prompt to its reply |
| `tool_edit_diff.txt` | recorded: the same report, from the tool-edit prompt to its reply |
| `palette_open.txt` | recorded: `/vers` typed into an idle composer |
| `palette_browse.txt` | recorded: a bare `/`; the window holds eight rows, so `/skin` and `/theme` are below it |
| `mention_list.txt` | recorded (fixed build): `Summarize @cal` sent with `herdr pane send-text`, a 300 ms pause, then `c`; the selected row starts with `›` |
| `paste_label.txt` | recorded: a 120-line bracketed paste through `tmux paste-buffer -p -r -d`, Talaria running in tmux |
| `file_label.txt` | recorded: a pasted path of a file outside the folder, after the paste above |
| `shell_cells.txt` | recorded: `!echo heron-4243`, then the refused `!rm -r ./keepme` |
| `side_answer.txt` | recorded: `/btw which two words did I ask you to join in my first message?` |
| `side_failed.txt` | recorded (fixed build): the same `/btw` question, in an empty throwaway folder, in a session started with a model the provider does not have |
| `pager_open.txt` | recorded: Ctrl+O at the bottom of the transcript |
| `pager_search.txt` | recorded: `/heron-4242` and Enter in the pager |
| `pager_nomatch.txt` | recorded: `/zzqx-nomatch` and Enter in the pager |
| `theme_report.txt` | recorded: `/theme` (recent text); system lines carry Talaria's `· ` gutter |
| `usage_lines.txt` | recorded: `/skin ../etc` and `/theme purple`, each typed and then Enter (recent text) |
| `tasks_expanded.txt` | recorded: the panel while the plan's sleep step ran |
| `tasks_collapsed.txt` | recorded: after Ctrl+T |
| `tasks_record.txt` | recorded: the record at the end of the turn |
| `pane_get.txt` | recorded: `herdr pane get <pane>` for an idle Talaria pane (JSON; folder and title scrubbed) |
| `history_box.txt` | recorded: Ctrl+R and `heron and 4242` |
| `editor_result.txt` | recorded: `draft-one`, then Ctrl+G with an editor script that appends ` plus-two` |
