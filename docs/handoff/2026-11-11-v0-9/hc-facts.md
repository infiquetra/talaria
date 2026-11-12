# What herdr core (`hc`) does for a Talaria seat and for the smoke harness

Measured on 2026-11-11 with `hc 0.1.20`, the command line of herdr core (the newer Herdr). Each fact below was observed on a scratch terminal; none is taken from the manual. The exact replies are in the folder `hc-recordings/` beside this file, one file per call, with the scratch folder shown as `/tmp/scratch` and the account name removed.

Issues cite this page when they state how `hc` behaves. If a later `hc` release behaves differently, record new replies and correct this page in the same pull request.

## Replies and exit statuses

`hc` prints JSON when its output is not a terminal. A success is printed on standard output with exit status 0. An error is one object on standard error, `{"error":{"kind":…,"message":…,"hint":…}}`, with standard output empty.

| Call | Recording | Exit status | What it shows |
|---|---|---|---|
| `hc --version` | `version.txt` | 0 | `hc 0.1.20` |
| `hc run --bg --no-parent --name <name> --cwd <folder>` (no program) | `run_bg.json` | 0 | Starts the login shell and returns at once. One object; the terminal's address is the string field `address`. |
| The same call while a live terminal holds the name | `run_name_taken.json` | 1 | Error kind `name_taken`. |
| `hc run --bg --no-parent --cwd <folder>` (no name) | `run_split.json` | 0 | The same object without `name`. |
| `hc ls` (piped) | `ls.json` | 0 | One object whose `terminals` array has one entry per live terminal with `address`, `agent`, `cwd`, `name` (when it has one), `state` and `turn`. The recording keeps only the two terminals the probe started; the real reply also has the top-level keys `hidden_finished` and `machines`. |
| `hc get <address> --json`, shell at its prompt | `get_shell.json` | 0 | `foreground` is one object (`argv`, `cwd`, `name`, `pid`), here the shell itself; `status` is `null`. |
| `hc get`, after the program wrote `state=idle:app=talaria` | `get_idle.json` | 0 | `status` is `{"app":"talaria","state":"idle"}`; `foreground.argv` is the running program. |
| `hc get`, after `state=blocked:app=talaria:kind=permission` | `get_blocked.json` | 0 | `status` is `{"app":"talaria","kind":"permission","state":"blocked"}`. |
| `hc send <address> <text>` to an idle program | `send_idle.json` | 0 | The text is pasted and Enter is pressed. |
| `hc send` to a program whose state is `blocked` | `send_blocked.json` | 9 | Error kind `blocked`; nothing is typed. |
| `hc keys <address> h i space shift+a enter` | `keys_ok.json` | 0 | The keys are pressed. |
| `hc keys <address> a ?` | `keys_unknown.json` | 6 | Error kind `invalid_params`, `unknown key "?"`; no key of that call is pressed. |
| `hc keys <address> 1` to a program whose state is `blocked` | `keys_blocked.json` | 0 | The key is pressed. |
| `hc keys <address> o n e alt+enter t w o` | `keys_alt_enter.json` | 0 | The keys are pressed. `alt+enter` is a key name `hc keys` accepts. |
| `hc wait <address>` while the program's state is `working`, which then becomes `done` | `wait_done.json` | 0 | Returns when the state leaves `working`. One object with `"ended":true`, `"reason":"status"` and `"status":"done"`. |
| `hc wait <address>` when the state is already `done` | `wait_already_done.json` | 0 | Returns at once with the same object. |
| `hc wait <address>` while the state is `working`, which then becomes `blocked`, `idle` or `error` | `wait_blocked.json`, `wait_idle.json`, `wait_error.json` | 0 | The same object with `"ended":true`, `"reason":"status"` and `"status"` holding the new state. |
| `hc wait <address> --timeout 3` while the state stays `working` | `wait_timeout.json` | 0 | Returns after three seconds with `"ended":false`, `"reason":"timeout"` and `"status":"working"`. Without `--timeout` the limit is 180 seconds unless the configuration sets another. |
| `hc wait <address>` from a terminal that has an unread message | `wait_message.json` | 0 | Returns at once with `"ended":false`, `"reason":"message"` and `"from"` naming the sender. |
| `hc read <address>` | `read_screen.txt` | 0 | The screen as plain text, with no JSON even when piped. |
| `hc read <address> --lines 5` | `read_lines.txt` | 0 | Five lines of history above the screen, then the screen. |
| `hc kill <address>` on a live terminal | `kill_live.json` | 0 | Returns after the program has ended; the object has `state: "finished"`. |
| `hc get` on a finished terminal | `get_finished.json` | 0 | `foreground` is `null`. |
| `hc rm <address>` right after `hc kill` | `rm_finished.json` | 0 | `{"removed":"<address>"}` |
| `hc get` after `hc rm` | `get_removed.json` | 3 | Error kind `not_found`. |

## Waiting

`hc wait <address>` always ends with exit status 0, so read its reply, not its status. The program left `working` only when the reply has `"ended":true`; `"status"` then holds the state it moved to (`done`, `idle`, `blocked` or `error`). A reply with `"reason":"timeout"` or `"reason":"message"` means the program is still where it was: run the same `hc wait` again. A message for the caller ends the wait only when the caller's environment holds `HC_TERMINAL_ID`; `env -u HC_TERMINAL_ID -u HC_SCOPE hc wait <address>` is not ended by a message. The seven wait recordings were made on 2026-11-11 with a scratch program that wrote `state=working:app=talaria` and, a few seconds later, the next state (`done`, `blocked` with `kind=permission`, `idle` or `error`).

## Names

A terminal name is 1 to 32 characters of `a-z 0-9 - _ .`, starts with a letter or digit, and is not shaped like an address (`t7`). It is unique among live terminals.

## The sender tag on `hc send`

herdr core sets `HC_TERMINAL_ID` and `HC_SCOPE` in the environment of every program it runs. When the process that calls `hc send` has `HC_TERMINAL_ID` set and the target reports a status, the text arrives wrapped, for example `<hc from="t7 lead">hello</hc>`. With both variables removed from the caller's environment (`env -u HC_TERMINAL_ID -u HC_SCOPE hc send <address> "<text>"`) the text arrives exactly as given. A target that reports no status receives the text unwrapped either way.

So once Talaria reports its state, a prompt another agent sends arrives with the wrapper, and a slash command sent that way is not a command.

## What `hc keys` can press

- The key names the smoke suites use: `enter esc tab up down left right q ctrl+c ctrl+g ctrl+o ctrl+r ctrl+s ctrl+t ctrl+u shift+tab`. `backtab` is refused.
- `alt+enter` is accepted (recording `keys_alt_enter.json`). No other `alt+` chord was measured.
- A single character from `a-z`, `0-9`, `'`, `,`, `-`, `.`, `/`, `;`, `=`, `[`, `\`, `]` or the backtick. `space` is the name for a space.
- `shift+` and a lowercase letter presses the capital letter. A capital letter given by itself is pressed as the lowercase key.
- It cannot press a literal space character or any of `! " # $ % & ( ) * + : < > ? @ ^ _ { | } ~`. `shift+1`, `shift+;` and `shift+/` press `1`, `;` and `/`.

## No way to type text without Enter

`hc send` always presses Enter. `hc attach` refuses a pipe (`attach needs a terminal`). The programming interface (`hc schema`) has no call that pastes or types text without Enter. So, through `hc` 0.1.20, text can be typed without submitting it only when every character is one `hc keys` can press.

## Reading

`hc read <address>` prints the screen as text. `--lines <n>` adds n lines of history above the screen. It has one form and no "unwrapped" source.

## Not measured

- Which process `foreground` names when a wrapper script starts Talaria. For `bash -c 'sleep 25; true'` it named the outer `bash` command, not the `sleep` under it.
- Whether `hc read` joins a line that wrapped on screen.
- Any of this against a running Talaria.
