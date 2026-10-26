# Configuration

Talaria reads `~/.talaria/config.toml` once at start; restart it to apply a change. A missing file
is fine, and copying this default file changes nothing. There is no repository-level file.

```toml
[keys]
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
fleet_peek = ["space"]
fleet_reply = ["r"]
fleet_stop = ["x"]
fleet_answer = ["a"]
fleet_filter = ["/"]
fleet_profile_next = ["tab"]
fleet_profile_prev = ["shift+tab"]
[editor]
command = ""
[history]
save = true
[notifications]
bell = ["needs_you", "failed"]
desktop = ["needs_you", "done", "failed"]
done_after_seconds = 20
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

Talaria never stops for this file: anything it cannot use falls back to its default with one notice
naming `~/.talaria/config.toml`. A syntax error (with line and column) or an unreadable file gives
all defaults; an unknown table, setting or `[keys]` action is ignored; a bad value or unreadable key
gets its default; a key bound twice stays with one action and the other gets a notice.

`~/.talaria/models.json` keeps favourites and per-profile defaults (model, provider, effort) for new
Talaria sessions; model and provider are saved together, and `/effort default --default` removes
saved effort. A malformed file is left untouched and switches them off. `--model` or `--provider`
overrides the saved pair and `--effort` the saved effort; continue and resume ignore saved defaults;
an unavailable saved model is dropped. Hermes's profile default never changes.

## Keys

`cancel` clears the draft; on an empty draft it stops the running turn; with nothing running, two
presses within 2 seconds exit. The `completion_*` keys apply only while a list is open, and a tab
inside pasted text is kept. `toggle_tasks` shows the task checklist; `attach_clipboard_image`
attaches the clipboard's image. The model picker keys are not in the block: `model_picker` Alt+P,
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

A key is modifiers (`ctrl`, `alt`, `shift`, `super`, each once) and one key joined by `+`, in any
case: a character, `enter`, `esc`, `tab`, `backspace`, `delete`, `insert`, arrows, `home`, `end`,
`pageup`, `pagedown`, `space`, `plus` or `f1` to `f24`. A printable key needs `ctrl`, `alt` or
`super`; an uppercase letter means Shift. A key you wrote beats a default; between two you wrote,
the earlier action keeps it and the other gets a notice. Shift+Enter needs the Kitty keyboard
protocol, so Ctrl+J and Alt+Enter are the portable newline keys. No default uses F1, F2, Ctrl+B or
Ctrl+S (macOS, nested Herdr, flow control); binding them works, with a notice.

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

## Coming from 0.6

The `profiles` and `connections` tables, 0.6 settings this release does not know, the `[status]`
column settings and the 0.6 `[keys]` names (`toggle_inspector`, `agents`, `commands`, `models`,
`profiles`, `config`, `follow`, `replay_*`) are ignored with notices. Remove an `interrupt` line
set to Ctrl+S, or Esc stops interrupting. The repository file `./.talaria/config.toml` and the 0.6
`TALARIA_*` overrides are gone.
