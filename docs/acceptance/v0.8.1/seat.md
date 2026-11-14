# Herdr seat acceptance record

Talaria: 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0
Hermes: 0.23.0
Herdr version: herdr-app 0.1.15
hc version: hc 0.1.21
Date: 2026-11-13

| id | persona | result | evidence | commit |
| --- | --- | --- | --- | --- |
| S1 | seat-lead | PASS | #s1 | 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0 |
| S2 | seat-lead | PASS | #s2 | 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0 |
| S3 | seat-lead | PASS | #s3 | 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0 |
| S4 | seat-lead | PASS | #s4 | 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0 |
| S5 | configurer | PASS | #s5 | 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0 |
| S6 | developer | PASS | #s6 | 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0 |
| S7 | operator | Herdr sound: no; desktop alert: yes | #s7 | 6ccc9246ca9f84bdcf26ae5a742962cdb470fda0 |

This record assembles operator comments A–C and the agent tester's two scrubbed
captures from the same candidate. The seat address was local. Manual approvals
were set on `fixture`; status reporting retained its default. Status reads below
use only the two filtered fields, never the whole terminal reply:

```sh
hc get t2b --json | grep -oE '"reported":(true|false)|"status":(null|\{[^}]*\})'
```

Read excerpts omit unrelated scrollback. A reported display observation is
retained under S4.

## S1

Launch command as typed:

```sh
/tmp/talaria-6ccc9246/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
```

Before launch, the probe example ran in the candidate checkout:

```sh
cargo run -p talaria-tui --example seat_probe
hc read t2b
```

```text
seat-probe idle
seat-probe working
seat-probe blocked permission
seat-probe blocked question
seat-probe blocked auth
seat-probe done
seat-probe error
seat-probe clear
```

Status read after the example cleared its record:

```text
"reported":true
"status":null
```

Given Talaria was open on the new session, the status read was:

```text
"reported":true
"status":{"app":"talaria","state":"idle"}
```

```sh
hc send t2b "Reply with the single word pong."
hc read t2b
```

```text
> <hc from="t8 uat-tester-1">Reply with the single word pong.</hc>
pong
```

The first status read after sending showed `working` (S2), about half a second
later. No Enter nudge was used. The wait started at that read:

```sh
env -u HC_TERMINAL_ID -u HC_SCOPE hc wait t2b
```

```json
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"done","terminal":"t2b","turn":6}
```

## S2

The status read that started S1's wait:

```text
"reported":true
"status":{"app":"talaria","state":"working"}
```

The status read after that wait:

```text
"reported":true
"status":{"app":"talaria","state":"done"}
```

## S3

After operator comment B's first three actions, the composer was empty and no
approval card was showing. The tester sent the exact prompt in comment A:

```sh
hc send t2b "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache"
```

The first status read, half a second later, showed:

```text
"reported":true
"status":{"app":"talaria","state":"working"}
```

```sh
env -u HC_TERMINAL_ID -u HC_SCOPE hc wait t2b
```

```json
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"blocked","terminal":"t2b","turn":11}
```

The wrapped prompt and approval card were present in `hc read t2b`:

```text
> <hc from="t8 uat-tester-1">Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache</hc>
? rm -rf .pytest_cache; echo "EXIT_CODE=$?"
Approval needed
$ rm -rf .pytest_cache; echo "EXIT_CODE=$?"
recursive delete
Hermes denies this after 300 s without an answer.
› 1. once
  2. session
  3. always
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop
```

The final status read confirmed permission blocking. The card was left up for
the tester's fourth action in S4:

```text
"reported":true
"status":{"app":"talaria","kind":"permission","state":"blocked"}
```

## S4

Both runs exercised four actions: open and close the transcript, make and clear
a two-line draft, interrupt a running turn, then answer an approval card.

### Agent tester run

The starting status was `done`, with an empty composer and no card.

Action 1 used `hc keys t2b ctrl+o` twice. The first `hc read t2b` showed:

```text
Transcript · untitled session · 3 cells                                                                                                     idle
/ search · n N next/prev · { } prompts · g G top/bottom · t details · q close                                                cell 1/3 · following
```

After the second Ctrl+O, the pager footer was gone and the read ended at `pong`.
The composer was undrawn (border and placeholder counts both zero), until the
next input redrew it intact. The tester observed this twice. The status after
closing remained:

```text
"reported":true
"status":{"app":"talaria","state":"done"}
```

Action 2 used `hc keys t2b o n e alt+enter t w o`. The read showed the draft on
two lines; nothing was submitted and the turn remained 6:

```text
│ > one
│   two
```

One `hc keys t2b ctrl+c` cleared the draft. The read and status were:

```text
│ > Type a message. Enter sends, Shift+Enter adds a line.
"reported":true
"status":{"app":"talaria","state":"done"}
```

Action 3 sent `hc send t2b "Count from 1 to 300, one number per line."`. The
first status read showed:

```text
"reported":true
"status":{"app":"talaria","state":"working"}
```

The tester pressed `hc keys t2b ctrl+c` once while the turn was still running,
without a wait. The read showed:

```text
> <hc from="t8 uat-tester-1">Count from 1 to 300, one number per line.</hc>
· Interrupted
│ > Type a message. Enter sends, Shift+Enter adds a line.
```

The first status read after interrupting reached:

```text
"reported":true
"status":{"app":"talaria","state":"idle"}
```

An extra plain `hc send t2b "Reply with the single word pong."` restored `done`
before the operator's run. Its first status read showed `working`, and its wait
ended with:

```json
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"done","terminal":"t2b","turn":8}
```

The following status was `done`; the read showed the empty composer, with one
border and one placeholder, and no card.

Action 4 occurred after S3, with its card left up, an empty composer, and more
than half a second elapsed since the card appeared. `hc keys t2b enter` selected
`once`. Four half-second status polls showed `working`; the fifth showed:

```text
"reported":true
"status":{"app":"talaria","state":"done"}
```

The `hc read t2b` excerpt after answering was:

```text
Let me run the exact command.
Shell(rm -rf .pytest_cache; echo "EXIT_CODE=$?")  ✓ · 29s
  EXIT_CODE=0
? rm -rf .pytest_cache; echo "EXIT_CODE=$?" → once
Exit code: 0
```

The approval card was gone (`Approval needed` count zero). The composer was
empty, with one border and one placeholder.

### Operator keyboard run

Comment B records actions 1–3, after the reset turn reached `done`. Comment C
records action 4. The operator used the keyboard in the Herdr window.

Action 1 pressed Ctrl+O twice. After the first press, `hc read t2b` showed:

```text
Transcript · untitled session · 13 cells                                                                                                    idle
/ search · n N next/prev · { } prompts · g G top/bottom · t details · q close                                              cell 12/13 · following
exit status: 0
```

The second press returned to the conversation scrollback, without the transcript
header or pager footer. Its read excerpt was:

```text
Green Bar
exit status: 0
```

Action 2 typed `one`, pressed Alt+Enter, and typed `two`. The read excerpt was:

```text
│ > one                                                                                                                                         │
│   two                                                                                                                                         │
exit status: 0
```

One Ctrl+C cleared the draft. The read and subsequent status read showed:

```text
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                                       │
exit status: 0
"reported":true
"status":{"app":"talaria","state":"done"}
exit status: 0
```

Action 3 typed `count from 1 to 300, one number per line`, pressed Enter, then
pressed Ctrl+C as the reply started. The read excerpt and status were:

```text
> count from 1 to 300, one number per line
· Interrupted
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                                       │
exit status: 0
"reported":true
"status":{"app":"talaria","state":"idle"}
exit status: 0
```

Action 4 typed the comment A prompt, waited for the approval card, and pressed
Enter. Comment C's read excerpt showed the new prompt and its resolved approval:

```text
> Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache
⏺ Shell(rm -rf .pytest_cache; echo "EXIT_CODE=$?")  ✓ · 7.4s
  ⎿  EXIT_CODE=0
? rm -rf .pytest_cache; echo "EXIT_CODE=$?" → once
Exit code: 0
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                                       │
exit status: 0
"reported":true
"status":{"app":"talaria","state":"done"}
exit status: 0
```

Both runs reached the required states and removed the approval card after the
answer. The undrawn composer after closing the transcript remains a display
observation from the tester's run.

## S5

Comment A named `Nous Portal`, an account offering device-code sign-in. After
action 4, the operator opened `/providers`, selected its Accounts tab and that
account, waited for a user code, and pressed unshifted `c`. Comment C records
the comparison with the system clipboard:

```text
Nous Portal
clipboard matched: yes
```

The code was not included in the evidence.

## S6

The tester ran the negative case in tmux; the operator ran it in a plain
terminal outside Herdr and tmux. Neither terminal answered the startup probe.

The tmux example command and scrubbed output were:

```sh
cargo run -p talaria-tui --example seat_probe; echo "exit status: $?"
```

```text
Running `target/debug/examples/seat_probe`
exit status: 0
```

There were no `seat-probe` output lines. The tester then captured a Talaria
session using:

```sh
script -q -c "/tmp/talaria-6ccc9246/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud" ../seat-neg-tmux.log
```

The typed pong prompt received `pong`, then `/quit` returned to the shell. The
tmux session was removed. Its log contained one probe (`]7501;?`, count 1,
exit status 0) and no status records. The saved count was:

```sh
grep -a -c ']7501;state=' /tmp/talaria-6ccc9246/seat-neg-tmux.log
```

```text
0
exit status: 1
tmux: ]7501;state= 0
```

The operator ran `cargo run -p talaria-tui --example seat_probe` in the plain
terminal. Comment C records only the line immediately after Cargo's `Running`
line, as required:

```text
exit status: 0
```

The plain log's count was:

```sh
grep -c ']7501;state=' /tmp/talaria-6ccc9246/seat-neg-plain.log; echo "exit status: $?"
```

```text
0
exit status: 1
plain: ]7501;state= 0
```

Exit status 1 is expected when grep counts zero matches. Both logs contain no
status record; neither example wrote a state without a probe answer.

## S7

From a different focused terminal in the Herdr window, the operator sent the
pong prompt while the Talaria terminal was unfocused. Comment C records:

```sh
hc send t2b "Reply with the single word pong."
```

```text
{"started":true,"status":"done","turn":14}
exit status: 0
"reported":true
"status":{"app":"talaria","state":"working"}
exit status: 0
```

At the `working` read, the operator waited:

```sh
env -u HC_TERMINAL_ID -u HC_SCOPE hc wait t2b
```

```text
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"done","terminal":"t2b","turn":15}
exit status: 0
Herdr sound: no; desktop alert: yes
```

This is an observation of the background turn ending normally, with no pass or
fail assigned to the alert behavior.
