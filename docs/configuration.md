# Configuration

Talaria reads one file, `~/.talaria/config.toml`, once when it starts. Restart Talaria to apply
a change. This release has no repository-level configuration file; the environment overrides
are listed under the schema. A missing file is fine: every setting has a default.

## The default file

This file keeps every setting at its default. Picker bindings use their defaults when
omitted; they are listed in the key table below. Copying this unchanged changes nothing.

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
```

## Schema

Ctrl+G opens the draft in the configured editor, then `$VISUAL`, `$EDITOR`, or an installed
fallback. Graphical editors need a wait flag. Changes return to the composer unsent.
Ctrl+R searches saved prompts; Tab switches between this folder and all folders. Enter loads
the selected text without sending; Esc restores the original draft. Ctrl+Z suspends to a
job-control shell; `fg` restores the terminal and the draft.

History lives in `~/.talaria/history.jsonl`: version, Unix timestamp, workspace folder key and
prompt text, one JSON object per line. Its folder is private and its file is owner-readable
and writable only. A leading space or more than 100,000 characters keeps a prompt in memory
only. `[history] save = false` disables both reading and writing. Startup reads at most 1 MiB;
after the first frame, files over 4 MiB are compacted to the newest 5,000 lines within 1 MiB.

Notification kinds are `needs_you`, `done`, and `failed`. Bell and desktop lists independently
choose channels. A completed turn alerts only after `done_after_seconds` (0–86400, default 20).
Interrupted turns and restored prompts stay silent. Desktop delivery depends on the terminal.
`[herdr] report_state` enables pane reporting when the process was launched inside Herdr;
blocked means a prompt is still visible, working means a turn is running, otherwise idle.

| Setting | Type | Default | Meaning |
|---|---|---|---|
| `[keys] <action>` | a key name, or a list of key names | the table below | The keys that run a composer action. `[]` leaves the action with no key. |
| `status.command` | text | none (off) | The status-line command, as one command line. Talaria does not run it through a shell. |
| `status.interval_seconds` | whole number, 1 to 3600 | 5 | Seconds between runs of the status command. |
| `environment.allowlist` | list of variable names | empty | Environment variables the status command may see. |
| `theme.mode` | `auto`, `light` or `dark` | `auto` | Light or dark terminal. `auto` asks the terminal; `/theme` overrides it for one session. |
| `theme.follow_hermes_skin` | true or false | true | Draw in the Hermes profile's skin colours. `false` keeps Talaria's own colours. |
| `theme.colors` | `auto`, `truecolor`, `256`, `16` or `none` | `auto` | Colour depth. `auto` reads `NO_COLOR`, `TERM` and `COLORTERM`; `16` and `none` draw no skin. |
| `ui.reduced_motion` | true or false | false | Reduced motion: nothing changes on a timer except counters, at most once per second. |

Environment overrides, read once at start: `TALARIA_THEME` (`light`, `dark` or `auto`) beats
`theme.mode`; `TALARIA_REDUCED_MOTION` (`1`, `true`, `yes`, `on`, `0`, `false`, `no` or `off`)
beats `ui.reduced_motion`; a non-empty `NO_COLOR` turns colour off. See [themes](themes.md).
| `composer.paste_collapse_lines` | whole number, 0 or more | 5 | A paste of this many lines becomes a label. 0 turns the line rule off. |
| `composer.paste_collapse_chars` | whole number, 0 or more | 2000 | A paste of this many characters becomes a label. 0 turns the character rule off. |

## Default keys

| Action | Default keys | What it does |
|---|---|---|
| `submit` | Enter | Send the draft. A blank draft is not sent. |
| `newline` | Shift+Enter, Alt+Enter, Ctrl+J | Insert a line break. |
| `interrupt` | Esc | Stop the running turn and keep the draft. Does nothing when no turn runs. |
| `cancel` | Ctrl+C | Clear the draft; on an empty draft stop the running turn; with nothing running, press twice within 2 seconds to exit. |
| `move_left`, `move_right` | Left, Right | Move one character. |
| `move_up`, `move_down` | Up, Down | Move one row; on the first or last row, walk the prompt history. |
| `line_start`, `line_end` | Home or Ctrl+A; End or Ctrl+E | Start or end of the line. |
| `buffer_start`, `buffer_end` | Ctrl+Home; Ctrl+End | Start or end of the draft. |
| `word_left`, `word_right` | Alt+Left, Ctrl+Left, Alt+B; Alt+Right, Ctrl+Right, Alt+F | Move one word. |
| `delete_backward`, `delete_forward` | Backspace or Ctrl+H; Delete | Delete one character. |
| `delete_word_backward` | Ctrl+W, Alt+Backspace | Delete the word before the caret. |
| `kill_to_line_end`, `kill_to_line_start` | Ctrl+K; Ctrl+U | Delete to the end or start of the line. |
| `yank` | Ctrl+Y | Insert the text the last delete-word or kill removed. |
| `completion_next` | Down, Ctrl+N | Next row in an open completion list. |
| `completion_prev` | Up, Ctrl+P | Previous row in an open completion list. |
| `completion_accept` | Tab | Accept the selected completion. |
| `completion_close` | Esc | Close the open completion list. |
| `model_picker` | Alt+P | Open the foreground session's model picker from the composer. |
| `effort_down`, `effort_up` | Left, Right | Change the chosen reasoning level in the model picker. |
| `model_scope` | Ctrl+G | Toggle whether the choice also applies to new Talaria sessions. |
| `model_favourite` | Ctrl+F | Star or unstar the selected model. |
| `model_refresh` | Ctrl+R | Refresh the model list; gateway actions can wait behind this call. |
| `attach_clipboard_image` | Ctrl+V, Alt+V | Attach the image on the clipboard. |
| `transcript_open` | Ctrl+O | Open the full-screen transcript. While it is open, the same key closes it. |
| `pager_line_up` | Up, K | One line toward the start of the transcript. |
| `pager_line_down` | Down, J | One line toward the end of the transcript. |
| `pager_page_up` | PageUp, B | One page toward the start of the transcript. |
| `pager_page_down` | PageDown, Space | One page toward the end of the transcript. Space never closes it. |
| `pager_half_up` | Ctrl+U | Half a page toward the start of the transcript. |
| `pager_half_down` | Ctrl+D | Half a page toward the end of the transcript. |
| `pager_top` | G, Home | The start of the transcript. |
| `pager_bottom` | Shift+G, End | The end of the transcript, following new output. |
| `pager_prev_prompt` | { | The previous message you sent. |
| `pager_next_prompt` | } | The next message you sent. |
| `pager_search` | / | Search the whole transcript. |
| `pager_next_match` | N | The next search match. |
| `pager_prev_match` | Shift+N | The previous search match. |
| `pager_details` | T | Switch between the full cell and the compact one. |
| `pager_close` | Q | Close the transcript. |
| `toggle_tasks` | Ctrl+T | Show the agent's task checklist in one row, or every step of it, above the composer. |
| `approvals_toggle` | Shift+Tab | Switch this session's approvals off or back on. |

Tab accepts the highlighted row of a completion list. A tab inside pasted text is kept.
Completion keys share chords with composer keys. Each applies only while its list is open.

Picker actions resolve before composer actions while the model overlay is open.
The same chord may be bound in these two separate contexts. Typing filters the list;
Esc clears the filter, then closes the picker. Enter switches only this session unless
Ctrl+G selected the wider scope. A confirmation card uses Enter or Y to switch and
Esc or N to keep the current model; a draft must be cleared before these keys answer it.

## Saved model choices

`~/.talaria/models.json` stores favourites and defaults for new Talaria sessions:

```json
{"version":1,"favourites":["ollama-cloud::minimax-m3.1-flash"],"defaults":{"fixture":{"model":"minimax-m3.1-flash","provider":"ollama-cloud","effort":"high"}}}
```

The model, provider and effort fields are optional. Saving a model keeps model and
provider together; saving effort changes only that field. `/effort default --default`
removes saved effort. Unknown top-level keys are ignored and dropped on the next write.
Malformed files are left untouched, with favourites and defaults disabled until repaired.
Writes use a private temporary file and atomic replacement; new folders are mode 700
and files mode 600.

`--model` or `--provider` overrides the saved model/provider pair; `--effort` overrides
saved effort. Continue and resume ignore saved defaults. New sessions validate a saved
model against the profile catalogue before creating it. Unavailable saved model/provider
fields are dropped together, keeping effort; unsupported create fields are also dropped.
A typed create refusal carrying saved fields retries once without those fields.
These defaults never change Hermes's profile default.

## The transcript screen

The full-screen transcript has its own key context. A plain letter is a transcript key there and
never reaches your draft, so the same letter may name a transcript key and a draft key.

Esc and Ctrl+C are fixed in the transcript and cannot be bound to anything. Esc cancels a search,
then clears the search, then closes the transcript. Ctrl+C always closes the transcript. Neither
one interrupts the agent, and neither one touches your draft.

`transcript_open` is checked before the transcript keys, so binding it to a letter that is also a
transcript key still closes the transcript. The key names the transcript prints on screen, in the
hints, in the banner and in the "window too small" line, are the built-in defaults and do not
follow your bindings.

## Writing a key

A key is zero or more modifiers and then one key, joined by `+`. Case does not matter and
spaces around `+` are ignored.

- Modifiers: `ctrl` (or `control`), `alt` (or `option`), `shift`, `super`. Each at most once.
- Keys: one character such as `j` or `/`; `enter`, `esc` (or `escape`), `tab`, `backspace`,
  `delete`, `insert`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`,
  `space`, `plus` (the `+` character), and `f1` to `f24`.
- A letter, digit or other printable key needs `ctrl`, `alt` or `super`, because on its own it
  would stop that key from typing. Shift alone is not enough.
- An uppercase letter means Shift plus that letter.

```text
[keys]
submit = "alt+enter"          # one key may be written without a list
newline = ["enter", "ctrl+j"]
yank = []                     # no key
```

When two actions name the same key, the action you wrote wins over a default. If you wrote
both, the action earlier in the table above keeps it and the other gets a notice.

## When something is wrong

Talaria never stops because of this file. It starts with the default for anything it cannot
use and shows one plain notice per problem. Notices name the file as `~/.talaria/config.toml`,
never by its full path.

- A syntax error: all defaults, and a notice with the line and column.
- A file Talaria cannot read (permissions, a directory, text that is not UTF-8): all defaults.
- A table or setting Talaria does not know: ignored, with a notice.
- A value of the wrong type or out of range: that setting's default, with a notice.
- An unknown action name in `[keys]`: ignored, with the list of known actions.
- A key Talaria cannot read: that key is ignored. If none of an action's keys can be read, the
  action keeps its defaults.
- A key bound to two actions: one keeps it, the other gets a notice.
- A default key taken by another action, leaving its action with no key: a notice says so.

## Approvals

The status line shows whether Hermes asks before running a dangerous command in this session:
`approvals: manual` or `approvals: smart` (the profile's mode, and this session's bypass is off),
`approvals: off` (the profile asks for nothing), `approvals: off (this session)` (this session's
bypass is on) or `approvals: unknown` (a read failed, or a change could not be confirmed). Talaria
shows only values Hermes confirmed; it stores none of them.

`approvals_toggle` (Shift+Tab) switches this session's bypass on or off, and only this session's.
While the profile's mode is `off` it changes nothing and says so. A completion list or an
approval card that has the key keeps it. Unbind it with `approvals_toggle = []`.

Hermes sends no event when a setting changes, so Talaria reads both again when a session opens,
when a turn ends, after a reconnect, and after Hermes's own `/yolo` or `/approvals` runs. A change
made in another window shows after the next of these. A gateway started with `--yolo` is not
visible here: the line may say `manual` while Hermes approves everything.

`/permissions` shows and changes both settings. Words are case-insensitive.

```text
/permissions                    show this session's setting, the profile's mode and the bypass
/permissions manual|smart|off   set the profile's mode, for every session of the profile
/permissions session off        approvals off for this session only
/permissions session default    this session follows the profile's mode again
```

The profile's mode is shared by every session of the profile, background sessions included, and
needs no open session. `/permissions off` first explains that and asks to be run again: the same
command within 30 seconds sends it. Another `/permissions` command, the toggle key, a session change
or a reconnect cancels the wait. Hermes's own `/approvals` still runs as a Hermes command.

## Working folder

`/cd` alone shows the session's working folder. `/cd <folder>` moves the session there and keeps
the conversation; its next command runs in the new folder, and the status line shows the new
folder and branch.

- `~` and `~/rest` use your home folder; `~name` is refused. A relative path starts from the
  session's folder, not from where Talaria or Hermes was started. One pair of outer quotes is
  removed; spaces inside the path are kept.
- Talaria checks that the folder exists before it asks Hermes, and Hermes decides last.
- `/cd` is refused while a turn runs: wait for it, or press Esc to stop it, then run `/cd` again.

## Terminals and keys

Shift+Enter needs a terminal that supports the Kitty keyboard protocol. Elsewhere Shift+Enter
arrives as a plain Enter and sends the draft; use Ctrl+J or Alt+Enter for a new line, which work
in every terminal. The empty composer shows the newline key your terminal can send.

No default uses F1, F2, Ctrl+B or Ctrl+S. macOS takes F1 and F2 before a program sees them;
Herdr takes Ctrl+B when it runs nested; Ctrl+S is the old flow-control stop key. Talaria's own
raw mode turns flow control off for itself, but a multiplexer or terminal in between may still
take the key. You may still bind these keys; Talaria keeps the binding and shows a notice.

If you leave `cancel` with no key, Ctrl+C no longer exits Talaria. Close the terminal, or use
another way out once a later release adds one.

## Coming from 0.6

- These tables are now ignored, each with a notice: `theme`, `ui`, `notifications`,
  `profiles`, `connections`.
- These `[status]` settings are now ignored: `segments`, `cwd_max_columns`,
  `git_branch_max_columns`, `agent_model_max_columns`.
- The 0.6 `[keys]` names `toggle_inspector`, `agents`, `commands`, `models`, `profiles`,
  `config`, `follow`, `replay_pause`, `replay_slower` and `replay_faster` are unknown here.
  `interrupt` still exists: its 0.6 value was Ctrl+S. Remove that line, or Esc stops
  interrupting and you get a notice.
- The repository file `./.talaria/config.toml` and the `TALARIA_*` overrides are gone.
- Later releases add tables back as their features ship.
