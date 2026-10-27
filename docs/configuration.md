# Configuration

Talaria reads `~/.talaria/config.toml` once at start; restart it to apply a change. A missing file
is fine, and copying this default file changes nothing. There is no repository-level file.

```toml
[keys]
doctor_up = ["up", "k"]
doctor_down = ["down", "j"]
doctor_toggle = ["enter"]
doctor_rerun = ["r"]
doctor_restart = ["shift+r"]
doctor_close = ["esc", "q"]
doctor_yes = ["y"]
doctor_no = ["n"]
doctor_tab = ["tab"]
doctor_left = ["left"]
doctor_right = ["right"]
submit = ["enter"]
newline = ["shift+enter", "alt+enter", "ctrl+j"]
interrupt = ["esc"]
cancel = ["ctrl+c"]
move_left = ["left"]
move_right = ["right"]
move_up = ["up"]
move_down = ["down"]
line_start = ["home", "ctrl+a"]
line_end = ["end", "ctrl+e"]
buffer_start = ["ctrl+home"]
buffer_end = ["ctrl+end"]
word_left = ["alt+left", "ctrl+left", "alt+b"]
word_right = ["alt+right", "ctrl+right", "alt+f"]
delete_backward = ["backspace", "ctrl+h"]
delete_forward = ["delete"]
delete_word_backward = ["ctrl+w", "alt+backspace"]
kill_to_line_end = ["ctrl+k"]
kill_to_line_start = ["ctrl+u"]
yank = ["ctrl+y"]
open_editor = ["ctrl+g", "alt+g"]
history_search = ["ctrl+r"]
suspend = ["ctrl+z"]
completion_next = ["down", "ctrl+n"]
completion_prev = ["up", "ctrl+p"]
completion_accept = ["tab"]
completion_close = ["esc"]
attach_clipboard_image = ["ctrl+v", "alt+v"]
transcript_open = ["ctrl+o"]
pager_line_up = ["up", "k"]
pager_line_down = ["down", "j"]
pager_page_up = ["pageup", "b"]
pager_page_down = ["pagedown", "space"]
pager_half_up = ["ctrl+u"]
pager_half_down = ["ctrl+d"]
pager_top = ["g", "home"]
pager_bottom = ["shift+g", "end"]
pager_prev_prompt = ["{"]
pager_next_prompt = ["}"]
pager_search = ["/"]
pager_next_match = ["n"]
pager_prev_match = ["shift+n"]
pager_details = ["t"]
pager_close = ["q"]
toggle_tasks = ["ctrl+t"]
approvals_toggle = ["shift+tab"]
queue_edit = ["enter", "e"]
queue_remove = ["d", "delete", "ctrl+x"]
queue_steer = ["s"]
queue_send_now = ["n"]
queue_move_up = ["K", "shift+up"]
queue_move_down = ["J", "shift+down"]
queue_leave = ["esc"]
composer_send_now = ["ctrl+enter"]
composer_steer = []
session_picker_open = []
picker_up = ["up", "k"]
picker_down = ["down", "j"]
picker_page_up = ["pageup"]
picker_page_down = ["pagedown"]
picker_top = ["home", "g"]
picker_bottom = ["end", "G"]
picker_open = ["enter"]
picker_rename = ["r"]
picker_branch = ["b"]
picker_export = ["e"]
picker_search = ["/"]
picker_profile_next = ["tab"]
picker_profile_prev = ["shift+tab"]
picker_refresh = ["ctrl+r"]
picker_close = ["q"]
picker_format_markdown = ["m"]
picker_format_json = ["j"]
picker_format_html = ["h"]
picker_toggle_redact = ["x"]
fleet_view_open = []
fleet_up = ["up", "k"]
fleet_down = ["down", "j"]
fleet_page_up = ["pageup"]
fleet_page_down = ["pagedown"]
fleet_top = ["home", "g"]
fleet_bottom = ["end", "shift+g"]
fleet_open = ["enter"]
fleet_open_in_pane = ["o"]
fleet_peek = ["space"]
fleet_reply = ["r"]
fleet_stop = ["x"]
fleet_answer = ["a"]
providers_next_tab = ["tab"]
providers_prev_tab = ["shift+tab"]
providers_reload = ["ctrl+r"]
providers_remove = ["ctrl+d"]
providers_test = ["ctrl+t"]
providers_save = ["ctrl+s"]
providers_clear_key = ["ctrl+x"]
providers_open_link = ["o"]
providers_copy = ["c"]
fleet_filter = ["/"]
fleet_profile_next = ["tab"]
fleet_profile_prev = ["shift+tab"]
agents_panel = ["alt+s"]
agents_open_in_pane = ["o"]
agents_up = ["up", "k"]
agents_down = ["down", "j"]
agents_tail = ["enter"]
agents_detail = ["d", "right"]
agents_back = ["esc", "left"]
agents_close = ["q"]
agents_steer = ["e"]
agents_stop = ["x"]
agents_stop_all = ["shift+x"]
agents_pause = ["p"]
agents_next_session = ["tab"]
agents_page_up = ["pageup"]
agents_page_down = ["pagedown"]
agents_top = ["g", "home"]
agents_bottom = ["shift+g", "end"]
fleet_new = ["n"]
fleet_new_worktree = ["w"]
attention_next = ["alt+a"]
fleet_review = ["v"]
review_up = ["up", "k"]
review_down = ["down", "j"]
review_top = ["home", "g"]
review_bottom = ["end", "shift+g"]
review_show_diff = ["enter"]
review_diff_page_up = ["pageup"]
review_diff_page_down = ["pagedown"]
review_scope = ["tab"]
review_stage = ["space"]
review_stage_all = ["a"]
review_unstage_all = ["shift+a"]
review_discard = ["shift+x"]
review_commit = ["c"]
review_push = ["p"]
review_pull_request = ["shift+p"]
review_open_pr = ["o"]
review_refresh = ["r"]
commit_submit = ["ctrl+s"]
commit_draft = ["ctrl+d"]
settings_up = ["up"]
settings_down = ["down"]
settings_page_up = ["pageup"]
settings_page_down = ["pagedown"]
settings_prev_category = ["left"]
settings_next_category = ["right"]
settings_edit = ["enter"]
settings_toggle = ["space"]
settings_filter = ["/"]
settings_default = ["d"]
settings_undo_field = ["u"]
settings_reveal = ["r"]
settings_review = ["a"]
settings_reload = ["ctrl+r"]
settings_help = ["?"]
settings_close = ["esc"]
profiles_up = ["up"]
profiles_down = ["down"]
profiles_page_up = ["pageup"]
profiles_page_down = ["pagedown"]
profiles_first = ["home"]
profiles_last = ["end"]
profiles_open = ["enter"]
profiles_filter = ["/"]
profiles_refresh = ["r"]
profiles_rescan = ["R"]
profiles_new = ["n"]
profiles_description = ["e"]
profiles_soul = ["s"]
profiles_back = ["esc"]
model_roles_section_main = ["1"]
model_roles_section_fallbacks = ["2"]
model_roles_section_helpers = ["3"]
model_roles_section_subagents = ["4"]
model_roles_section_moa = ["5"]
model_roles_next_section = ["tab"]
model_roles_prev_section = ["shift+tab"]
model_roles_up = ["up"]
model_roles_down = ["down"]
model_roles_home = ["home"]
model_roles_end = ["end"]
model_roles_page_up = ["pageup"]
model_roles_page_down = ["pagedown"]
model_roles_left = ["left"]
model_roles_right = ["right"]
model_roles_enter = ["enter"]
model_roles_effort = ["e"]
model_roles_context = ["c"]
model_roles_inherit = ["i"]
model_roles_add = ["a"]
model_roles_remove = ["x", "delete"]
model_roles_move_up = ["K", "alt+up"]
model_roles_move_down = ["J", "alt+down"]
model_roles_toggle = ["space"]
model_roles_make_default = ["D"]
model_roles_chain_mode = ["m"]
model_roles_reset_stale = ["R"]
model_roles_review = ["ctrl+s"]
model_roles_discard = ["u"]
model_roles_reload = ["ctrl+r"]
model_roles_close = ["esc"]
model_roles_help = ["?"]
route_picker_up = ["up"]
route_picker_down = ["down"]
route_picker_page_up = ["pageup"]
route_picker_page_down = ["pagedown"]
route_picker_home = ["home"]
route_picker_end = ["end"]
route_picker_choose = ["enter"]
route_picker_refresh = ["ctrl+r"]
route_picker_cancel = ["esc"]
mcp_next_tab = ["tab"]
mcp_prev_tab = ["shift+tab"]
mcp_reload = ["ctrl+r"]
mcp_add = ["ctrl+n"]
mcp_test = ["ctrl+t"]
mcp_toggle = ["ctrl+e"]
mcp_set_key = ["ctrl+k"]
mcp_sign_in = ["ctrl+o"]
mcp_remove = ["ctrl+d"]
mcp_apply = ["ctrl+y"]
mcp_save = ["ctrl+s"]
mcp_open_link = ["o"]
mcp_copy = ["c"]
mcp_paste_redirect = ["p"]
[editor]
command = ""
[history]
save = true
[notifications]
bell = ["needs_you", "failed"]
desktop = ["needs_you", "done", "failed"]
done_after_seconds = 20
fleet_needs_you = "all"
fleet_finished = "watched"
[herdr]
report_state = true
[composer]
paste_collapse_lines = 5
paste_collapse_chars = 2000
[status]
# command = "~/.talaria/status.sh"
interval_seconds = 5
[environment]
allowlist = []
[theme]
mode = "auto"
follow_hermes_skin = true
colors = "auto"
[ui]
reduced_motion = false
[fleet]
enabled = true
hot_poll_ms = 2000
cold_poll_ms = 4000
backstop_s = 30
[subagents]
poll_s = 5
tail_poll_ms = 2000
dock_rows = 3
[panes]
multiplexer = "auto"
direction = "auto"
focus = false
```

## Settings

`[keys] <action>` takes a key or a list; `[]` leaves the action unbound. Ctrl+G edits the draft in
`editor.command`, `$VISUAL`, `$EDITOR` or a fallback (graphical editors need a wait flag); Ctrl+R
searches saved prompts; Ctrl+Z suspends to the shell until `fg`. History is kept owner-only in
`~/.talaria/history.jsonl`; a leading space or over 100,000 characters keeps a prompt in memory
only, and `[history] save = false` stops reading and writing it. Notification kinds are `needs_you`,
`done` and `failed`; the bell and desktop lists choose channels separately, and `done` waits for
`done_after_seconds` (0 to 86400). Inside Herdr, `[herdr] report_state` reports blocked, working or
idle. `status.command` runs without a shell every `status.interval_seconds` (1 to 3600) and sees
only `environment.allowlist`. `theme.mode` is `auto`, `light` or `dark` (`/theme` overrides it for
one session); `theme.follow_hermes_skin = false` keeps Talaria's colours; `theme.colors` is `auto`,
`truecolor`, `256`, `16` or `none`. `ui.reduced_motion` stops timed changes except counters, which
change at most once a second. A paste of `paste_collapse_lines` lines or `paste_collapse_chars`
characters becomes a label (0 turns that rule off). `TALARIA_THEME` beats `theme.mode`,
`TALARIA_REDUCED_MOTION` beats `ui.reduced_motion`, and a non-empty `NO_COLOR` turns colour off.
See [themes](themes.md).

`[fleet]` runs the background fleet link of an interactive launch (never `-p`): `enabled = false`
turns it off. It polls the liveness of profiles with a waiting or working session every
`hot_poll_ms` (1000 to 10000), every other profile every `cold_poll_ms` (2000 to 30000, spread
evenly), and rediscovers profiles every `backstop_s` (10 to 300); out-of-range values are clamped
with one notice. `talaria fleet list` uses the same timings and ignores `enabled`; its JSON is
[`talaria.fleet.v1`](formats/fleet-list-v1.md).

`[subagents]` polls this session's children every `poll_s` seconds (2 to 60) while children
are live or the agents panel is open. The open transcript polls every `tail_poll_ms`
milliseconds (1000 to 10000). `dock_rows` (0 to 10) limits rows above the composer; 0 hides
the dock. Invalid values use the defaults above with one notice. `/agents` and `/tasks`,
or `agents_panel` (Alt+S), open the panel. Enter reads a transcript, D shows details, E steers,
X stops one running child, and Shift+X twice within 3 seconds stops this session's running
children. P asks before pausing new spawns for every session on the gateway; running children
continue. `/agents pause` opens that confirmation; `/agents resume` resumes immediately.

A background session reaches the operator through the same channels as the foreground one: a
background request that needs an answer, a finished turn and a failed turn all use the `bell`
and `desktop` lists, and `done_after_seconds` is the shortest finished turn that alerts.
`fleet_needs_you` and `fleet_finished` choose which background rows may alert: `all` is every
background row, `watched` is the rows this Talaria has touched (the session it started, one it
switched away from, one it peeked or answered), and `off` is never. A finished turn of an
untouched session stays quiet by default, because scheduled runs and other clients finish all
day. The first seconds after the fleet link comes up stay quiet while the fleet lists its rows,
so a request that was already waiting does not ring the bell; the counters show what the quiet
window skipped. One rate limit covers every alert, foreground and background: two alerts less
than two seconds apart pass only when the second one is more urgent (needs you, then failed,
then done), and a dropped alert is not repeated later.

`[panes]` controls O in `/fleet` and `/agents`: `multiplexer` is `auto`, `herdr`, `tmux`,
or `off`; auto prefers Herdr. `direction` is `auto`, `right`, or `down`; auto splits right
when the pane is at least 120 columns and twice as wide as it is tall, otherwise down.
`focus = false` keeps focus in the current pane. The new pane runs this Talaria binary's
`attach` command in a shell, so an error stays visible. A subagent must have a session id.
These actions are `fleet_open_in_pane` and `agents_open_in_pane` and can be rebound or unbound.

`talaria attach [--profile <name>] [--no-ensure] [--record <path>] [--] <SESSION>` resumes by
full id, id prefix (at least four characters), or title. Without a profile it searches served
profiles when permitted. Ambiguous matches require a longer id or `--profile`; listing searches
the newest 200 live and stored rows per profile. A full id with `--profile` skips listing.

Talaria never stops for this file: anything it cannot use falls back to its default with one notice
naming `~/.talaria/config.toml`. A syntax error (with line and column) or an unreadable file gives
all defaults; an unknown table, setting or `[keys]` action is ignored; a bad value or unreadable key
gets its default; a key bound twice stays with one action and the other gets a notice.

`~/.talaria/models.json` keeps favourites and per-profile defaults (model, provider, effort) for new
Talaria sessions; model and provider are saved together, and `/effort default --default` removes
saved effort. A malformed file is left untouched and switches them off. `--model` or `--provider`
overrides the saved pair and `--effort` the saved effort; continue and resume ignore saved defaults;
an unavailable saved model is dropped. Hermes's profile default never changes.

## MCP servers

`/mcp` opens the connected profile's Model Context Protocol servers. `/mcp catalogue` browses
Hermes's catalogue; `/mcp apply` asks Hermes to apply saved changes. Tab switches lists, typing
filters, Enter opens details or an install confirmation, and Esc clears the filter before closing.

On Servers, Ctrl+T tests, Ctrl+E enables or disables, Ctrl+K saves a key, Ctrl+O starts browser
sign-in, Ctrl+D asks before removal, and Ctrl+N adds a remote or local server. Plugin servers are
read-only here. A local server runs a program on the gateway machine: add only programs you trust.
Keys and environment values are masked and saved through Hermes's key API, outside server config.

Saving affects new sessions. Ctrl+Y shows Hermes's warning before applying changes to the current
session; a busy session may need another apply after its turn ends. Ctrl+R refreshes the list
without applying anything. Removing a server keeps its saved key and sign-in in the profile.

Browser sign-in opens a link only when you press O; C copies it. Over SSH, P accepts the browser's
loopback callback address when the redirect cannot reach the gateway machine. The detail card
also gives the command for device sign-in. Talaria shows and copies these commands; it never
runs them. Catalogue entries that need extra values provide a terminal install command.

The `mcp_*` actions can be rebound under `[keys]`: `mcp_next_tab`, `mcp_prev_tab`, `mcp_reload`,
`mcp_add`, `mcp_test`, `mcp_toggle`, `mcp_set_key`, `mcp_sign_in`, `mcp_remove`, `mcp_apply`,
`mcp_save`, `mcp_open_link`, `mcp_copy`, and `mcp_paste_redirect`. These bindings apply only
inside the MCP screen.

## Keys

`/config` opens the current Hermes profile's settings. `/config <text>` opens a filter;
each word must match the setting's key, description or category. Left and Right switch
categories, Enter edits, Space toggles a boolean, D selects the default, and U undoes the
selected field. Changes remain pending until A opens the review and Enter applies them.
Talaria checks for outside changes first and reads every save back before reporting success.
An outside change asks whether to keep yours or take theirs. A restart label explains when
a saved value takes effect; most settings may need a gateway restart.

`/models` opens the profile's model roles (main model, fallbacks, helpers, subagents and
Mixture of Agents presets); see [Model roles](model-roles.md).

The `settings_*` bindings above apply while browsing. `/` focuses the filter; Enter leaves
it focused on the first matching setting. In an editor, arrows, Home, End, Backspace,
Delete and Ctrl+U edit text; Enter accepts and Esc cancels. Option lists accept typing to
filter. Ctrl+R reloads settings, `?` shows help, and Esc clears the filter before closing.
Pending edits require confirmation before closing. Ctrl+Z suspends as usual.

Secrets are fully masked. R reveals a selected secret once for ten seconds or until the
next key. While editing a secret, Alt+R reveals it once; plain R inserts text.
Reviews and saved notices never reveal a secret. Environment references,
values containing secrets, unsupported types, model roles and provider settings are read-only
on this screen. Settings need a terminal at least 60 columns by 12 rows.

`/config display.compact=true display.skin="mono"` prepares multiple changes for review.
Quoted values support escaped quotes and backslashes. Unknown keys or invalid values reject
the whole command. Enter applies; Esc discards and closes. Enter secrets through the form:
commands containing a secret assignment are refused and excluded from prompt history.

`cancel` clears the draft; on an empty draft it stops the running turn; with nothing running, two
presses within 2 seconds exit. The `completion_*` keys apply only while a list is open, and a tab
inside pasted text is kept. `toggle_tasks` shows the task checklist; `attach_clipboard_image`
attaches the clipboard's image. `attention_next` (Alt+A) jumps to the oldest background request
that waits for an answer. The model picker keys are not in the block: `model_picker` Alt+P,
`effort_down` and `effort_up` Left and Right, `model_scope` Ctrl+G (also new sessions),
`model_favourite` Ctrl+F, `model_refresh` Ctrl+R. They resolve before composer keys while the picker
is open; Esc clears the filter, then closes; a confirmation card takes Enter or Y, Esc or N.

The transcript (`transcript_open`, Ctrl+O, which also closes it) has its own key context: plain
letters there never reach the draft. `transcript_open` is checked before the transcript keys, and
the key names that screen prints are the built-in defaults, not your bindings. Esc and Ctrl+C are
fixed there: Esc cancels, then clears a search, then closes; Ctrl+C closes; neither interrupts the
agent. The session picker context is separate from the composer too (`j` moves down there and picks
JSON in its export dialog); `session_picker_open` has no default; Esc and Ctrl+C are fixed.

The fleet overlay (`/fleet`, or `fleet_view_open`, which has no default) has its own context:
`fleet_peek` attaches and shows a session (moving the cursor never does), `fleet_reply` queues a
prompt behind its running turn, `fleet_stop` asks first and stops on a second press,
`fleet_answer` hands keys to the peeked card, and `fleet_open` makes the row the foreground
session. Esc and Ctrl+C are fixed there and never stop anything; in the card, Esc is the card's.
`fleet_new` opens a dialog that starts a background session (`talaria --bg` from a shell does the
same), and `fleet_new_worktree` starts it in a new git worktree at `.worktrees/talaria-<name>` on
the branch `talaria/<name>`, cut from freshly fetched `origin`. Files listed in `.worktreeinclude`
are copied into the tree, but folders listed there are links to the main checkout's, so edits in
them are shared; uncommitted changes in the main checkout are not carried into the tree.
`fleet_review` (`v`) on a fleet row, or `/changes` for the foreground session, opens a review of
the session's folder: `review_scope` switches between uncommitted changes and the branch's
commits (read-only), `review_stage`, `review_stage_all` and `review_unstage_all` stage,
`review_discard` discards one file on a second press, `review_commit` opens a message editor
(`commit_submit` commits, `commit_draft` asks the session's own model for a message),
`review_push` pushes the branch to `origin` (never forced, never the default branch) and
`review_pull_request` opens a draft pull request with `gh`; `review_open_pr` opens it in the
browser and `review_refresh` reloads. Esc and Ctrl+C are fixed there and never act.

A key is modifiers (`ctrl`, `alt`, `shift`, `super`, each once) and one key joined by `+`, in any
case: a character, `enter`, `esc`, `tab`, `backspace`, `delete`, `insert`, arrows, `home`, `end`,
`pageup`, `pagedown`, `space`, `plus` or `f1` to `f24`. A printable key needs `ctrl`, `alt` or
`super`; an uppercase letter means Shift. A key you wrote beats a default; between two you wrote,
the earlier action keeps it and the other gets a notice. Shift+Enter needs the Kitty keyboard
protocol, so Ctrl+J and Alt+Enter are the portable newline keys. No default uses F1, F2, Ctrl+B or
Ctrl+S (macOS, nested Herdr, flow control), except `commit_submit` in the review's commit editor
and `providers_save` in an endpoint form and `mcp_save` in an MCP form;
binding them works, with a notice.

## Providers

`/providers` opens **Keys** for the connected Hermes profile. `/providers keys`,
`/providers accounts` and `/providers endpoints` select a tab directly. Enter on a model
picker row labelled `set up in /providers` opens the matching provider here.

On a list, type to filter, use arrows to select, and press Tab or Shift+Tab to switch tabs.
Ctrl+R reloads. Esc closes a card first, then clears the filter, then returns to the transcript.
The `providers_*` entries in `[keys]` above configure the tab, reload, removal, test, save,
clear-key, browser and copy actions in this overlay. Printable keys such as `o` and `c` still
filter a list; they open or copy only when the displayed card offers that action.

**Keys.** Enter opens a masked field. Paste a key and press Enter to check and save it.
A rejected key stays in the card for correction. If the provider cannot be reached, the card
asks for a second Enter to save unchecked; editing the key clears that confirmation.
Ctrl+U clears the field. A saved key shows at most its last four characters.

`ready` means Hermes lists the provider as authenticated in its catalogue. It does not prove
that the saved key currently works. Ctrl+D confirms removal of every credential Hermes saves
for that provider, including its account sign-in. The confirmation names known shared key
variables and the other providers that would lose them.

**Accounts.** Enter starts the provider's device-code sign-in or shows instructions for an
external sign-in command. A device-code card shows the browser address, code and expiry.
Press `o` to open its https link, `c` to copy its code, or Esc to cancel. External commands
can be copied and run in a separate terminal. Ctrl+D signs out after confirmation when Hermes
supports it; otherwise the card gives the provider's removal instructions. Clipboard copying
depends on the terminal allowing access.

**Endpoints.** Choose `+ Add an endpoint`, or Enter on an existing row to edit it. Fill in Name,
Base URL, API key, API mode, Model and optional Context length. Tab and Shift+Tab move between
fields. Ctrl+T tests the endpoint and fills discovered model choices. Ctrl+S saves; a failed
test can still be saved for an endpoint that is currently offline. Testing can identify a
different answering base address, which the form displays before saving it.

While editing, an untouched empty API key field keeps the saved key; Ctrl+X explicitly clears
it. Leaving an existing Context length empty keeps its old Hermes value. Endpoint saves leave
the profile's default model selection as it was. Ctrl+D confirms deletion, including removal
of the endpoint's saved key. An endpoint set directly in model settings is read-only here.

While disconnected, the screen disables changes and reloads when Talaria reconnects. A change
whose reply was lost is reported as uncertain and the list is reloaded; the change is not
automatically repeated.

## Approvals and working folder

The status line reads `approvals: manual` or `smart` (the profile's mode, bypass off), `off` (the
profile asks for nothing), `off (this session)` (the bypass is on) or `unknown` (a read failed or a
change was not confirmed). `approvals_toggle` (Shift+Tab) switches only this session's bypass and
does nothing while the profile's mode is `off`. Talaria rereads both when a session opens, a turn
ends, after a reconnect and after Hermes's `/yolo` or `/approvals`. A gateway started with `--yolo`
is not visible: the line may say `manual` while Hermes approves everything.

```text
/permissions                    show this session's setting, the profile's mode and the bypass
/permissions manual|smart|off   set the profile's mode, for every session of the profile
/permissions session off        approvals off for this session only
/permissions session default    this session follows the profile's mode again
```

`/permissions off` affects every session of the profile, so it asks first: the same command within
30 seconds sends it, and any other `/permissions`, the toggle, a session change or a reconnect
cancels. `/cd` shows the session's folder; `/cd <folder>` moves the session and keeps the
conversation. `~` is your home, `~name` is refused, and a relative path starts from the session's
folder. Talaria checks the folder exists; Hermes decides. `/cd` is refused while a turn runs: wait,
or press Esc.

## Doctor

Use `/doctor` in Talaria or `talaria doctor [--profile NAME] [--json] [--restart [--yes]]`
to check the gateway. The selected profile defaults to `default` on the command line. A restart
requires confirmation; `--yes` provides it for scripts. The [JSON format](formats/doctor-v1.md)
describes the machine-readable report. Exit codes are 0 for success, 1 for a failed check or
restart, 2 for invalid arguments, 3 when restart confirmation requires a terminal or `--yes`,
and 4 when confirmation is declined.

## Profiles

`/profiles` opens the profile list without changing the foreground session. It joins the
installed profile roster with the gateway's served and parked names and the optional host
topology. Labels distinguish served, parked, stopped, own gateway, not served, starting, and
not discovered. Each row gives a next step; Talaria does not run those fix commands.
The Profiles entry in `/config` opens the same overlay.

`talaria profiles [--json] [--profile fixture] [NAME...]` lists the same states without creating
a session or starting a gateway. Names are normalized and deduplicated in the order supplied.
JSON includes a `warnings` array when the roster or host topology is unavailable. A missing
gateway exits 1; an unknown requested name exits 2 and prints no table or JSON.

The overlay refreshes when opened, on `r`, and after its own writes. `R` requests a rescan only
after `y` confirms: serving new profiles can also start their messaging bots. `n` creates a
profile after validation and review, `e` edits its description, and `s` opens its soul in the
configured external editor. Soul edits are reviewed and checked against a fresh copy before
saving. A concurrent change keeps the unsaved edit in memory for the next `s`. Model, skills,
toolsets, and Model Context Protocol servers are shown here without edit controls.

These bindings apply only inside the profiles overlay. Text fields reserve ordinary editing
keys, Tab, Shift+Tab, and Ctrl+U; Esc leaves the innermost panel before closing the overlay.

The default bindings are included in the `[keys]` table above.

## Coming from 0.6

The `profiles` and `connections` tables, 0.6 settings this release does not know, the `[status]`
column settings and the 0.6 `[keys]` names (`toggle_inspector`, `agents`, `commands`, `models`,
`profiles`, `config`, `follow`, `replay_*`) are ignored with notices. Remove an `interrupt` line
set to Ctrl+S, or Esc stops interrupting. The repository file `./.talaria/config.toml` and the 0.6
`TALARIA_*` overrides are gone.
