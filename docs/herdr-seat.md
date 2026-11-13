# Running Talaria as a Herdr seat

A seat is one Talaria session running in a Herdr terminal. Herdr is a terminal multiplexer. Its newer line is the `hc` command plus a window. This page says what that seat reports, how to drive it from outside, and what this release does not send. The command flags below are those of hc 0.1.20. What `hc` returns, and how a plain `hc send` wraps text, was measured on hc 0.1.20 on 2026-11-11. TERMINAL means the seat's address from `hc ls`.

## What a seat reports

Talaria writes a status record into the terminal. It does not call `hc` to report. A status record is one Operating System Command 7501 (OSC 7501). Talaria writes one only when the terminal answered its startup question. No answer means no record is ever written, including inside tmux and in a plain terminal. There is no environment variable and no setting that forces reporting on.

`[herdr] report_state` is a boolean and defaults to true. It switches both this record and the older reporter. Set it to false and Talaria sends neither. The five states go to a Herdr terminal that answered the startup question. The older reporter still sends `blocked`, `working` or `idle` through `herdr pane report-agent` when that earlier program is present. See [herdr settings](configuration.md#herdr).

Talaria writes a record only when the state or the kind changes. An unchanged state writes nothing. On exit, and whenever the terminal is restored, Talaria writes a clear once, and only if it ever wrote a status record. `hc get --json` shows the state on `status.state` and the program on `status.app`. A blocked seat also has `status.kind`. Before any record, `status` is null.

A state or kind the terminal did not list is never written. If the terminal did not list `done` or `error`, Talaria writes `idle` instead. If the terminal listed `blocked` but did not list the kind, Talaria writes `blocked` with no kind.

| State | When Talaria reports it |
| --- | --- |
| `idle` | A session is open and no turn has finished in it yet. Also after the session changes, and after a turn the operator interrupted. |
| `working` | A turn is running and no need is open. A long turn stays `working` for as long as the turn runs, including while it waits on sub-agents. |
| `blocked` | At least one need is open. This wins over every other state. The kind comes from the first open need. |
| `done` | The last turn ended normally and no need is open. It stays `done` until the next turn starts or the session changes. Typing does not change it. |
| `error` | The last turn failed, under the same condition as the existing failed alert, and no need is open. |

A need is something Talaria is waiting on a person for. An approval has kind `permission`. A question has kind `question`. A sudo password, a secret, a vault code, or a vault unlock has kind `auth`.

Talaria does not report a different state to avoid another tool's stall alert.

## Driving a seat

`hc send TERMINAL "prompt"` pastes the prompt and then presses Enter. The paste does not submit. The Enter does, because a bracketed paste resets the burst detector. Once Talaria reports its state, a plain `hc send` from another agent wraps the text as `<hc from="…">prompt</hc>`. A slash command sent that way is not a command. `HC_TERMINAL_ID` and `HC_SCOPE` are the variables herdr core sets on the caller. Remove both from that one call: `env -u HC_TERMINAL_ID -u HC_SCOPE hc send TERMINAL "/quit"`.

A fast burst of typed characters is a different path. The third character within 8 milliseconds of the one before starts a burst. The same 8 milliseconds of quiet flushes the burst. An Enter less than 120 milliseconds after the last burst event inserts a newline instead of submitting. A tool that types characters must wait until that window has passed before it sends Enter.

`hc send` is the paste path, not the burst path.

## Waiting and reading

`hc wait TERMINAL` returns when the reported state leaves `working`. A message to you also ends that wait.

The default view is inline. Finished lines are written into the terminal's scrollback. `hc read TERMINAL` prints the current screen. Its `--lines` value defaults to 0, so a bare `hc read` does not include history. `hc read TERMINAL --lines N` adds N lines of that scrollback above the screen.

The full-screen workspace is off by default. Alt+D or `/layout panel` turns it on. While it is on, finished lines are not written to scrollback, and a read sees only the current screen. Turning the workspace off writes the queued lines once. The [guide](guide.md#many-sessions-at-once) states the same limit for search.

## Approval cards

While a card is showing and the composer is empty, keys go to the card. The composer is the box where you type. If the composer is not empty, keys do not go to the card. Clear the draft first. The card draws its own hint. The [composer keys](keybindings.md#composer-keys) do not list that hint.

For the first 250 milliseconds after the card focuses, a key is taken and does not answer, except Ctrl+C. In the main session, while a turn is running, Ctrl+C on an approval card denies the approval when deny is one of the options, and then interrupts the turn. Talaria sends the denial first and the interrupt second. When deny is not offered, Ctrl+C interrupts the turn and sends no denial. In the fleet view, Ctrl+C does not reach the card. It goes back to the list, or it closes the view.

After that pause, with the note field closed, the key `1` chooses the first option and sends it, `2` the second, and so on through `9` while that option exists. Enter confirms the highlighted option. The first option starts highlighted. Up and Down move the highlight. Esc sends `deny` when deny is one of the options. When deny is not offered, Esc closes the card without an answer. When the note field is open, Esc closes the note field and sends no answer. Tab opens the note field. Enter then confirms the highlighted option and includes the note when the note is not blank.

From outside, press those keys with `hc keys`. That command presses named keys. Its help lists `enter`, `esc`, `up`, `tab`, `ctrl+c`, and a single character such as `a`. An approval digit is that same kind of name.

- `hc keys TERMINAL 1`
- `hc keys TERMINAL enter`
- `hc keys TERMINAL esc`
- `hc keys TERMINAL ctrl+c`

`hc send` refuses a seat whose state is `blocked`. Answer an approval from outside with `hc keys`, a digit or `enter`, not with `hc send`.

A masked card, for a sudo password, a secret, a vault code, or a vault unlock, takes a paste into the value. Enter sends it. Esc declines. Do not use the approval digit keys for a masked value.

A question card uses the hint it draws. This page does not repeat that hint.

This page describes the behavior. It is not the record of a live run.

## Limits in this release

This release sends no `progress`, no `title`, no `msg`, and no `id` in a status record. Prompt text and approval text never go into a status record.

The older reporter still runs `herdr pane report-agent` when the earlier `herdr` program is present. It is unchanged in this release. It still sends only `blocked`, `working` or `idle`. It is not the status record on this page.
