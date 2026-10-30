# Migrating from 0.6.5

## Who this is for

The client has been rewritten in Rust, with the same `talaria` command name. The Python client code under `talaria/` is identical at `v0.6.5` and `python-final`, although those tags point to different commits. Install `v0.6.5` if you need that Python client.

## Staying on 0.6.5 for now

The older client's approval handling has been reported unreliable with current Hermes. For an interim terminal interface, use `hermes --tui --native`. If you choose to keep the Python client, reinstall it with bytecode compilation:

```bash
uv tool install --compile-bytecode git+https://github.com/infiquetra/talaria@v0.6.5
```

## Installing 0.7.0 next to 0.6.5

Both versions use the name `talaria`. Build the Rust client from source, keep it in a separate directory, and adjust your path to select which binary runs. Use `command -v talaria` and `talaria --version` to verify the selection. See the [guide](guide.md) for building and installation.

## Your config file

Talaria reads `~/.talaria/config.toml` at startup. The table lists each setting in the supplied older configuration examples. A notice means the old setting does not take effect.

| 0.6.5 setting | Outcome | 0.7.0 setting | What to do |
|---|---|---|---|
| `composer.attachment_max_mb` | notice | - | Remove this old setting and review the current configuration. |
| `composer.paste_collapse_bytes` | notice | - | Remove this old setting and review the current configuration. |
| `composer.paste_collapse_lines` | kept | - | Keep the setting. |
| `connections.fixture.label` | notice | - | Remove this old setting and review the current configuration. |
| `environment.allowlist` | kept | - | Keep the setting. |
| `keys.agents` | notice | - | Remove this old setting and review the current configuration. |
| `keys.commands` | notice | - | Remove this old setting and review the current configuration. |
| `keys.config` | notice | - | Remove this old setting and review the current configuration. |
| `keys.follow` | notice | - | Remove this old setting and review the current configuration. |
| `keys.interrupt` | kept with a warning | - | Remove this old binding to restore Esc; terminal flow control may intercept it. |
| `keys.models` | notice | - | Remove this old setting and review the current configuration. |
| `keys.profiles` | notice | - | Remove this old setting and review the current configuration. |
| `keys.replay_faster` | notice | - | Remove this old setting and review the current configuration. |
| `keys.replay_pause` | notice | - | Remove this old setting and review the current configuration. |
| `keys.replay_slower` | notice | - | Remove this old setting and review the current configuration. |
| `keys.toggle_inspector` | notice | - | Remove this old setting and review the current configuration. |
| `notifications.transcript_line` | notice | - | Remove this old setting and review the current configuration. |
| `profiles.endpoints` | notice | - | Remove the old endpoint map; use the Hermes gateway. |
| `status.agent_model_max_columns` | notice | - | Remove this old setting and review the current configuration. |
| `status.command` | kept | - | Keep the setting. |
| `status.cwd_max_columns` | notice | - | Remove this old setting and review the current configuration. |
| `status.git_branch_max_columns` | notice | - | Remove this old setting and review the current configuration. |
| `status.interval_seconds` | kept | - | Keep the setting. |
| `status.segments` | notice | - | Remove this old setting and review the current configuration. |
| `theme.name` | notice | - | Remove this setting; choose the Hermes skin instead. |
| `ui.diff_side_by_side_min_columns` | notice | - | Remove this old setting and review the current configuration. |
| `ui.inspector_dock_min_columns` | notice | - | Remove this old setting and review the current configuration. |
| `ui.inspector_open_at_start` | notice | - | Remove this old setting and review the current configuration. |
| `ui.inspector_width` | notice | - | Remove this old setting and review the current configuration. |
| `ui.reduced_motion` | kept | - | Keep the setting. |
| `ui.show_timestamps` | notice | - | Remove this old setting and review the current configuration. |

The following is the exact notice list for the supplied custom example. A quoted notice describes what the program prints; do not infer that every command it mentions is available in Talaria.

```text
theme.name is no longer used: Talaria follows the Hermes skin (see /skin and /theme).
Ignoring [connections] in ~/.talaria/config.toml: Talaria does not read it yet.
Ignoring [keys] commands in ~/.talaria/config.toml: it is not a known action. Known actions: workflow_up, workflow_down, workflow_collapse, workflow_expand, workflow_open, workflow_steer, workflow_stop_agent, workflow_restart, workflow_pause, workflow_stop_run, workflow_filter, workflow_close, workflow_confirm, doctor_up, doctor_down, doctor_toggle, doctor_rerun, doctor_restart, doctor_close, doctor_yes, doctor_no, doctor_tab, doctor_left, doctor_right, submit, newline, interrupt, cancel, move_left, move_right, move_up, move_down, line_start, line_end, buffer_start, buffer_end, word_left, word_right, delete_backward, delete_forward, delete_word_backward, kill_to_line_end, kill_to_line_start, yank, completion_next, completion_prev, completion_accept, completion_close, open_editor, history_search, suspend, attach_clipboard_image, transcript_open, pager_line_up, pager_line_down, pager_page_up, pager_page_down, pager_half_up, pager_half_down, pager_top, pager_bottom, pager_prev_prompt, pager_next_prompt, pager_search, pager_next_match, pager_prev_match, pager_details, pager_close, toggle_tasks, approvals_toggle, queue_edit, queue_remove, queue_steer, queue_send_now, queue_move_up, queue_move_down, queue_leave, composer_send_now, composer_steer, session_picker_open, picker_up, picker_down, picker_page_up, picker_page_down, picker_top, picker_bottom, picker_open, picker_rename, picker_branch, picker_export, picker_search, picker_profile_next, picker_profile_prev, picker_refresh, picker_close, picker_format_markdown, picker_format_json, picker_format_html, picker_toggle_redact, fleet_view_open, fleet_up, fleet_down, fleet_page_up, fleet_page_down, fleet_top, fleet_bottom, fleet_open, fleet_open_in_pane, fleet_peek, fleet_reply, fleet_stop, fleet_answer, fleet_filter, fleet_profile_next, fleet_profile_prev, agents_panel, agents_up, agents_down, agents_tail, agents_detail, agents_back, agents_close, agents_steer, agents_stop, agents_stop_all, agents_pause, agents_next_session, agents_page_up, agents_page_down, agents_top, agents_bottom, agents_open_in_pane, fleet_new, fleet_new_worktree, attention_next, fleet_review, review_up, review_down, review_top, review_bottom, review_show_diff, review_diff_page_up, review_diff_page_down, review_scope, review_stage, review_stage_all, review_unstage_all, review_discard, review_commit, review_push, review_pull_request, review_open_pr, review_refresh, commit_submit, commit_draft, profiles_up, profiles_down, profiles_page_up, profiles_page_down, profiles_first, profiles_last, profiles_open, profiles_filter, profiles_refresh, profiles_rescan, profiles_new, profiles_description, profiles_soul, profiles_back, providers_next_tab, providers_prev_tab, providers_reload, providers_remove, providers_test, providers_save, providers_clear_key, providers_open_link, providers_copy, mcp_next_tab, mcp_prev_tab, mcp_reload, mcp_add, mcp_test, mcp_toggle, mcp_set_key, mcp_sign_in, mcp_remove, mcp_apply, mcp_save, mcp_open_link, mcp_copy, mcp_paste_redirect, toggle_vim.
[keys] interrupt in ~/.talaria/config.toml: Ctrl+S may not reach Talaria in some terminals (the system, Herdr or terminal flow control can take it); the binding is kept.
```

A file holding every 0.6.5 default shows 25 notices.

## Your status-line command

The command is split without a shell, runs in the launch folder, receives a version 1 JSON document, and has a 2 second timeout. Its environment is default-deny. It may return text rows or version 2 JSON, with at most eight rows. Check [status-line.md](formats/status-line.md); its repository-config and `TALARIA_STATUS_INTERVAL_SECONDS` descriptions apply to 0.6.5 only. Fetch the old starter from a clone with `git show v0.6.5:docs/examples/status_bar_starter.py`.

A missing, invalid, or timed-out command displays a status marker; escape characters in its output are shown visibly. The current built-in status line adjusts to the available width.

## Your theme

The old `theme.name` selection and theme importer are not used. Set `display.skin` in Hermes to choose a skin. A previous theme name does not promise a matching skin. Talaria also supports local light, dark, and reduced-motion settings as described in [configuration](configuration.md).

## What no longer exists

- The old theme picker, marketplace, and Visual Studio Code importer: choose a Hermes skin instead.
- The per-profile gateway map: use the Hermes gateway's served profiles.
- Dashboard-token scraping: use the gateway's supported authentication.
- User-facing replay pacing: use recorded fixtures for development.
- The old repository-level configuration file: edit the user configuration instead.
- The old acceptance-receipt scripts: use the current release checks.
- The Python package on the main branch: use the tagged Python release when needed.

## Going back to 0.6.5

Remove the source-built Rust binary (for example `rm ~/.local/bin/talaria`), then reinstall the Python release using the command above. Check `command -v talaria` and `talaria --version` before continuing.

```bash
rm ~/.local/bin/talaria
command -v talaria
talaria --version
```

Keep your configuration directory; review its settings instead of deleting it.
