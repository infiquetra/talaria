# Configuration

Talaria reads one file, `~/.talaria/config.toml`, once when it starts. Restart Talaria to apply
a change. This release has no repository-level configuration file and no `TALARIA_*`
environment overrides. A missing file is fine: every setting has a default.

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

[status]
# command = "~/.talaria/status.sh"
interval_seconds = 5

[environment]
allowlist = []
```

## Schema

| Setting | Type | Default | Meaning |
|---|---|---|---|
| `[keys] <action>` | a key name, or a list of key names | the table below | The keys that run a composer action. `[]` leaves the action with no key. |
| `status.command` | text | none (off) | The status-line command, as one command line. Talaria does not run it through a shell. |
| `status.interval_seconds` | whole number, 1 to 3600 | 5 | Seconds between runs of the status command. |
| `environment.allowlist` | list of variable names | empty | Environment variables the status command may see. |

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

Tab accepts a row only while a completion list is open. A tab inside pasted text is kept.

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

- These tables are now ignored, each with a notice: `theme`, `ui`, `composer`, `notifications`,
  `profiles`, `connections`.
- These `[status]` settings are now ignored: `segments`, `cwd_max_columns`,
  `git_branch_max_columns`, `agent_model_max_columns`.
- The 0.6 `[keys]` names `toggle_inspector`, `agents`, `commands`, `models`, `profiles`,
  `config`, `follow`, `replay_pause`, `replay_slower` and `replay_faster` are unknown here.
  `interrupt` still exists: its 0.6 value was Ctrl+S. Remove that line, or Esc stops
  interrupting and you get a notice.
- The repository file `./.talaria/config.toml` and the `TALARIA_*` overrides are gone.
- Later releases add tables back as their features ship.
