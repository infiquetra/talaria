# Keybindings

These are the built-in bindings. Configure them in [configuration](configuration.md); contexts keep the same key available for different actions.

## Workflow progress keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Workflow up | Up | Move the selection up. | `workflow_up` |
| Workflow down | Down | Move the selection down. | `workflow_down` |
| Workflow collapse | Left | Collapse the selected workflow item. | `workflow_collapse` |
| Workflow expand | Right | Expand the selected workflow item. | `workflow_expand` |
| Workflow open | Enter | Open the selected item. | `workflow_open` |
| Workflow steer | S | Steer the selected agent. | `workflow_steer` |
| Workflow stop agent | X | Stop the selected agent. | `workflow_stop_agent` |
| Workflow restart | R | Restart the selected item. | `workflow_restart` |
| Workflow pause | P | Pause or resume the run. | `workflow_pause` |
| Workflow stop run | Shift+X | Ask to stop the workflow run. | `workflow_stop_run` |
| Workflow filter | F | Filter the list. | `workflow_filter` |
| Workflow close | Esc | Close this view. | `workflow_close` |
| Workflow confirm | Y | Confirm the pending action. | `workflow_confirm` |

## Doctor keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Doctor up | Up, K | Move the selection up. | `doctor_up` |
| Doctor down | Down, J | Move the selection down. | `doctor_down` |
| Doctor toggle | Enter | Toggle the selected option. | `doctor_toggle` |
| Doctor rerun | R | Run the checks again. | `doctor_rerun` |
| Doctor restart | Shift+R | Request a gateway restart. | `doctor_restart` |
| Doctor close | Esc, Q | Close this view. | `doctor_close` |
| Doctor yes | Y | Confirm. | `doctor_yes` |
| Doctor no | N | Decline. | `doctor_no` |
| Doctor tab | Tab | Move to the next doctor control. | `doctor_tab` |
| Doctor left | Left | Move left. | `doctor_left` |
| Doctor right | Right | Move right. | `doctor_right` |

## Composer keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Submit | Enter | Submit the draft. | `submit` |
| Newline | Shift+Enter, Alt+Enter, Ctrl+J | Insert a newline. | `newline` |
| Interrupt | Esc | Interrupt the running turn. | `interrupt` |
| Cancel | Ctrl+C | Clear the draft, interrupt, or exit on a second press. | `cancel` |
| Move left | Left | Move the caret left. | `move_left` |
| Move right | Right | Move the caret right. | `move_right` |
| Move up | Up | Move the caret up. | `move_up` |
| Move down | Down | Move the caret down. | `move_down` |
| Line start | Home, Ctrl+A | Move to the start of the line. | `line_start` |
| Line end | End, Ctrl+E | Move to the end of the line. | `line_end` |
| Buffer start | Ctrl+Home | Move to the start of the draft. | `buffer_start` |
| Buffer end | Ctrl+End | Move to the end of the draft. | `buffer_end` |
| Word left | Alt+Left, Ctrl+Left, Alt+B | Move left by a word. | `word_left` |
| Word right | Alt+Right, Ctrl+Right, Alt+F | Move right by a word. | `word_right` |
| Delete backward | Backspace, Ctrl+H | Delete the preceding character. | `delete_backward` |
| Delete forward | Delete | Delete the following character. | `delete_forward` |
| Delete word backward | Ctrl+W, Alt+Backspace | Delete the preceding word. | `delete_word_backward` |
| Kill to line end | Ctrl+K | Cut to the end of the line. | `kill_to_line_end` |
| Kill to line start | Ctrl+U | Cut to the start of the line. | `kill_to_line_start` |
| Yank | Ctrl+Y | Insert the last cut text. | `yank` |
| Open editor | Ctrl+G, Alt+G | Edit the draft in an external editor. | `open_editor` |
| History search | Ctrl+R | Search saved prompt history. | `history_search` |
| Suspend | Ctrl+Z | Suspend Talaria to the shell. | `suspend` |
| Attach clipboard image | Ctrl+V, Alt+V | Attach an image from the clipboard. | `attach_clipboard_image` |
| Transcript open | Ctrl+O | Open or close the transcript pager. | `transcript_open` |
| Toggle tasks | Ctrl+T | Show or hide the task checklist. | `toggle_tasks` |
| Approvals toggle | Shift+Tab | Toggle this session’s approval bypass. | `approvals_toggle` |
| Composer send now | Ctrl+Enter | Send the draft immediately. | `composer_send_now` |
| Composer steer | none | Steer the active turn. | `composer_steer` |
| Session picker open | none | Open the session picker. | `session_picker_open` |
| Fleet view open | none | Open the fleet view. | `fleet_view_open` |
| Agents panel | Alt+S | Open the subagent panel. | `agents_panel` |
| Attention next | Alt+A | Jump to the next waiting request. | `attention_next` |
| Voice record | Alt+R | Start or stop push-to-talk recording. | `voice_record` |
| Toggle Vim editing | none | Turn Vim editing on or off in the composer. | `toggle_vim` |

## Completion keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Completion next | Down, Ctrl+N | Move to the next item. | `completion_next` |
| Completion prev | Up, Ctrl+P | Move to the previous item. | `completion_prev` |
| Completion accept | Tab | Accept the selected completion. | `completion_accept` |
| Completion close | Esc | Close this view. | `completion_close` |

## Pager keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Pager line up | Up, K | Move the selection up. | `pager_line_up` |
| Pager line down | Down, J | Move the selection down. | `pager_line_down` |
| Pager page up | PageUp, B | Move up one page. | `pager_page_up` |
| Pager page down | PageDown, Space | Move down one page. | `pager_page_down` |
| Pager half up | Ctrl+U | Move up half a page. | `pager_half_up` |
| Pager half down | Ctrl+D | Move down half a page. | `pager_half_down` |
| Pager top | G, Home | Go to the beginning. | `pager_top` |
| Pager bottom | Shift+G, End | Go to the end. | `pager_bottom` |
| Pager prev prompt | { | Move to the previous prompt. | `pager_prev_prompt` |
| Pager next prompt | } | Move to the next prompt. | `pager_next_prompt` |
| Pager search | / | Start a transcript search. | `pager_search` |
| Pager next match | N | Go to the next match. | `pager_next_match` |
| Pager prev match | Shift+N | Go to the previous match. | `pager_prev_match` |
| Pager details | T | Toggle details. | `pager_details` |
| Pager close | Q | Close this view. | `pager_close` |

## Queue selection keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Queue edit | Enter, E | Edit the selected queued prompt. | `queue_edit` |
| Queue remove | D, Delete, Ctrl+X | Remove the selected queued prompt. | `queue_remove` |
| Queue steer | S | Steer with the selected prompt. | `queue_steer` |
| Queue send now | N | Send the selected prompt now. | `queue_send_now` |
| Queue move up | Shift+K, Shift+Up | Move the selected prompt earlier. | `queue_move_up` |
| Queue move down | Shift+J, Shift+Down | Move the selected prompt later. | `queue_move_down` |
| Queue leave | Esc | Leave queue selection. | `queue_leave` |

## Session picker keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Picker up | Up, K | Move the selection up. | `picker_up` |
| Picker down | Down, J | Move the selection down. | `picker_down` |
| Picker page up | PageUp | Move the selection up. | `picker_page_up` |
| Picker page down | PageDown | Move the selection down. | `picker_page_down` |
| Picker top | Home, G | Go to the first item. | `picker_top` |
| Picker bottom | End, Shift+G | Go to the last item. | `picker_bottom` |
| Picker open | Enter | Open the selected item. | `picker_open` |
| Picker rename | R | Use picker rename. | `picker_rename` |
| Picker branch | B | Use picker branch. | `picker_branch` |
| Picker export | E | Use picker export. | `picker_export` |
| Picker search | / | Search the list. | `picker_search` |
| Picker profile next | Tab | Move to the next item. | `picker_profile_next` |
| Picker profile prev | Shift+Tab | Move to the previous item. | `picker_profile_prev` |
| Picker refresh | Ctrl+R | Refresh the view. | `picker_refresh` |
| Picker close | Q | Close this view. | `picker_close` |

## Session picker export keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Picker format markdown | M | Use picker format markdown. | `picker_format_markdown` |
| Picker format json | J | Use picker format json. | `picker_format_json` |
| Picker format html | H | Use picker format html. | `picker_format_html` |
| Picker toggle redact | X | Use picker toggle redact. | `picker_toggle_redact` |

## Fleet keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Fleet up | Up, K | Move the selection up. | `fleet_up` |
| Fleet down | Down, J | Move the selection down. | `fleet_down` |
| Fleet page up | PageUp | Move the selection up. | `fleet_page_up` |
| Fleet page down | PageDown | Move the selection down. | `fleet_page_down` |
| Fleet top | Home, G | Go to the first item. | `fleet_top` |
| Fleet bottom | End, Shift+G | Go to the last item. | `fleet_bottom` |
| Fleet open | Enter | Open the selected item. | `fleet_open` |
| Fleet open in pane | O | Open the selected session in another pane. | `fleet_open_in_pane` |
| Fleet peek | Space | Peek at the selected session. | `fleet_peek` |
| Fleet reply | R | Reply to the selected session. | `fleet_reply` |
| Fleet stop | X | Request that the selected session stop. | `fleet_stop` |
| Fleet answer | A | Answer the selected session’s prompt. | `fleet_answer` |
| Fleet filter | / | Filter the list. | `fleet_filter` |
| Fleet profile next | Tab | Move to the next item. | `fleet_profile_next` |
| Fleet profile prev | Shift+Tab | Move to the previous item. | `fleet_profile_prev` |
| Fleet new | N | Create a background session. | `fleet_new` |
| Fleet new worktree | W | Create a background session in a worktree. | `fleet_new_worktree` |
| Fleet review | V | Review the selected session’s changes. | `fleet_review` |

## Agents keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Agents up | Up, K | Move the selection up. | `agents_up` |
| Agents down | Down, J | Move the selection down. | `agents_down` |
| Agents tail | Enter | Use agents tail. | `agents_tail` |
| Agents detail | D, Right | Use agents detail. | `agents_detail` |
| Agents back | Esc, Left | Return to the previous view. | `agents_back` |
| Agents close | Q | Close this view. | `agents_close` |
| Agents steer | E | Use agents steer. | `agents_steer` |
| Agents stop | X | Stop the selected operation. | `agents_stop` |
| Agents stop all | Shift+X | Use agents stop all. | `agents_stop_all` |
| Agents pause | P | Use agents pause. | `agents_pause` |
| Agents next session | Tab | Use agents next session. | `agents_next_session` |
| Agents page up | PageUp | Move the selection up. | `agents_page_up` |
| Agents page down | PageDown | Move the selection down. | `agents_page_down` |
| Agents top | G, Home | Go to the first item. | `agents_top` |
| Agents bottom | Shift+G, End | Go to the last item. | `agents_bottom` |
| Agents open in pane | O | Use agents open in pane. | `agents_open_in_pane` |

## Review keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Review up | Up, K | Move the selection up. | `review_up` |
| Review down | Down, J | Move the selection down. | `review_down` |
| Review top | Home, G | Go to the first item. | `review_top` |
| Review bottom | End, Shift+G | Go to the last item. | `review_bottom` |
| Review show diff | Enter | Use review show diff. | `review_show_diff` |
| Review diff page up | PageUp | Move the selection up. | `review_diff_page_up` |
| Review diff page down | PageDown | Move the selection down. | `review_diff_page_down` |
| Review scope | Tab | Use review scope. | `review_scope` |
| Review stage | Space | Use review stage. | `review_stage` |
| Review stage all | A | Use review stage all. | `review_stage_all` |
| Review unstage all | Shift+A | Use review unstage all. | `review_unstage_all` |
| Review discard | Shift+X | Use review discard. | `review_discard` |
| Review commit | C | Use review commit. | `review_commit` |
| Review push | P | Use review push. | `review_push` |
| Review pull request | Shift+P | Use review pull request. | `review_pull_request` |
| Review open pr | O | Use review open pr. | `review_open_pr` |
| Review refresh | R | Refresh the view. | `review_refresh` |

## Commit message keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Commit submit | Ctrl+S | Submit the commit message. | `commit_submit` |
| Commit draft | Ctrl+D | Draft a commit message with the session model. | `commit_draft` |

## Profiles keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Profiles up | Up | Move the selection up. | `profiles_up` |
| Profiles down | Down | Move the selection down. | `profiles_down` |
| Profiles page up | PageUp | Move the selection up. | `profiles_page_up` |
| Profiles page down | PageDown | Move the selection down. | `profiles_page_down` |
| Profiles first | Home | Use profiles first. | `profiles_first` |
| Profiles last | End | Use profiles last. | `profiles_last` |
| Profiles open | Enter | Open the selected item. | `profiles_open` |
| Profiles filter | / | Filter the list. | `profiles_filter` |
| Profiles refresh | R | Refresh the view. | `profiles_refresh` |
| Profiles rescan | Shift+R | Use profiles rescan. | `profiles_rescan` |
| Profiles new | N | Use profiles new. | `profiles_new` |
| Profiles description | E | Use profiles description. | `profiles_description` |
| Profiles soul | S | Use profiles soul. | `profiles_soul` |
| Profiles back | Esc | Return to the previous view. | `profiles_back` |

## Providers keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Providers next tab | Tab | Use providers next tab. | `providers_next_tab` |
| Providers prev tab | Shift+Tab | Use providers prev tab. | `providers_prev_tab` |
| Providers reload | Ctrl+R | Reload the current data. | `providers_reload` |
| Providers remove | Ctrl+D | Remove the selected item. | `providers_remove` |
| Providers test | Ctrl+T | Use providers test. | `providers_test` |
| Providers save | Ctrl+S | Save the provider form. | `providers_save` |
| Providers clear key | Ctrl+X | Use providers clear key. | `providers_clear_key` |
| Providers open link | O | Use providers open link. | `providers_open_link` |
| Providers copy | C | Use providers copy. | `providers_copy` |

## Mcp keys

| Action | Default keys | What it does | Config name |
|---|---|---|---|
| Mcp next tab | Tab | Use mcp next tab. | `mcp_next_tab` |
| Mcp prev tab | Shift+Tab | Use mcp prev tab. | `mcp_prev_tab` |
| Mcp reload | Ctrl+R | Reload the current data. | `mcp_reload` |
| Mcp add | Ctrl+N | Add an item. | `mcp_add` |
| Mcp test | Ctrl+T | Use mcp test. | `mcp_test` |
| Mcp toggle | Ctrl+E | Toggle the selected option. | `mcp_toggle` |
| Mcp set key | Ctrl+K | Use mcp set key. | `mcp_set_key` |
| Mcp sign in | Ctrl+O | Use mcp sign in. | `mcp_sign_in` |
| Mcp remove | Ctrl+D | Remove the selected item. | `mcp_remove` |
| Mcp apply | Ctrl+Y | Use mcp apply. | `mcp_apply` |
| Mcp save | Ctrl+S | Save the server form. | `mcp_save` |
| Mcp open link | O | Use mcp open link. | `mcp_open_link` |
| Mcp copy | C | Use mcp copy. | `mcp_copy` |
| Mcp paste redirect | P | Use mcp paste redirect. | `mcp_paste_redirect` |

## Keys that do not reach Talaria

On macOS, F1 and F2 may be taken by the system. Nested Herdr can take Ctrl+B, and terminal flow control can take Ctrl+S. Three existing form and commit bindings use Ctrl+S; rebind them if the terminal intercepts it.

Shift+Enter needs the keyboard enhancement protocol. Otherwise use Alt+Enter or Ctrl+J for a newline.
