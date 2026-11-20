# act acceptance record

Candidate: 0f7c62be5d39fdff90c40204e6d0e81485280153
Hermes: 0.23.2
Herdr: herdr-app 0.1.16
OS: Darwin 27.0.0 arm64 (operator); Linux 6.8.0-149-generic x86_64 GNU/Linux (test host)
Date: 2026-11-20

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| UAT-1 | seat-lead | PASS | #uat-1 | 0f7c62be5d39fdff90c40204e6d0e81485280153 |
| UAT-2 | fleet-lead | PASS | #uat-2 | 0f7c62be5d39fdff90c40204e6d0e81485280153 |
| UAT-3 | fleet-lead | PASS | #uat-3 | 0f7c62be5d39fdff90c40204e6d0e81485280153 |
| UAT-4 | developer | PASS | #uat-4 | 0f7c62be5d39fdff90c40204e6d0e81485280153 |

```text
/tmp/talaria-0f7c62be/talaria --version
talaria 0.8.1
hermes: pinned v0.23.1 (3b421578), supported from v0.23.0, tested to v0.23.1
```

## UAT-1

```text
hc --version
hc 0.1.21
hc send operator-uat -- "cd /tmp/talaria-0f7c62be/run"
hc send operator-uat -- "/tmp/talaria-0f7c62be/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud"
hc send operator-uat -- "Reply with the single word pong."
hc wait --timeout 60 operator-uat
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"done","terminal":"t2y","turn":9}
hc read operator-uat
<host>:/tmp/talaria-d556f044/run$ cd /tmp/talaria-0f7c62be/run
<host>:/tmp/talaria-0f7c62be/run$ /tmp/talaria-0f7c62be/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
· 2 background sessions need you; press alt+a to see the first.
> Reply with the single word pong.
∴ thinking
The user asks to reply with the single word "pong". Simple.

Should I check for relevant skills? No, this is trivial.
pong
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-0f7c62be/run │ ctx 1% │ ⚑ 2 need you
hc send operator-uat -- "/copy"
hc read operator-uat
<host>:/tmp/talaria-d556f044/run$ cd /tmp/talaria-0f7c62be/run
<host>:/tmp/talaria-0f7c62be/run$ /tmp/talaria-0f7c62be/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
· 2 background sessions need you; press alt+a to see the first.
> Reply with the single word pong.
∴ thinking
The user asks to reply with the single word "pong". Simple.

Should I check for relevant skills? No, this is trivial.
pong
· Copy requested: reply 1, 4 characters. Your terminal must allow clipboard access.
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-0f7c62be/run │ ctx 1% │ ⚑ 2 need you
```

The first read contains one copy request and no gateway refusal. The tester paused until the operator confirmed the first UAT-4 paste before sending `/copy 9`.

```text
hc send operator-uat -- "/copy 9"
hc read operator-uat
<host>:/tmp/talaria-d556f044/run$ cd /tmp/talaria-0f7c62be/run
<host>:/tmp/talaria-0f7c62be/run$ /tmp/talaria-0f7c62be/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
· 2 background sessions need you; press alt+a to see the first.
> Reply with the single word pong.
∴ thinking
The user asks to reply with the single word "pong". Simple.

Should I check for relevant skills? No, this is trivial.
pong
· Copy requested: reply 1, 4 characters. Your terminal must allow clipboard access.
· No reply 9 to copy.
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-0f7c62be/run │ ctx 1% │ ⚑ 2 need you
```

The second read still contains one copy request and no gateway refusal.

## UAT-2

The operator's background approval prompt was `Run the shell command: rm -rf /tmp/talaria-approval-probe-nonexistent`, in session `local-e2802019875e1d0756ff1a7cf1aaa3986bb66eaa346e46c5f2a02a7a5a4fd9d6`. The operator used iTerm2 3.7.7 outside Herdr (140x42).

Command run from `/tmp/talaria-0f7c62be/run`:
```text
/tmp/talaria-0f7c62be/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
```

- **Step 1 (`/fleet`, select background approval session, `Space`):**
```text
Fleet · 7 sessions
NEEDS YOU (1)
›  ●  Remove nonexistent talaria …  minimax-m3.1-flash  approval: rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT…  now
── peek ──
● Remove nonexistent talaria approval probe · fixture · minimax-m3.1-flash · needs you now
Approval needed · profile fixture
$ rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT=$?"
delete in root path
Hermes denies this after 300 s without an answer.
› 1. once
  2. session
  3. always
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close
✻ working
enter open · space peek · r reply · x stop · a answer · v review · / filter · esc close · o new pane
```

- **Step 2 (`a`, lowercase):**
```text
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list
✻ working
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list
```

- **Step 3 (first `Ctrl+C` — focus returns to list, card stays visible):**
```text
Approval needed · profile fixture
$ rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT=$?"
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close
✻ working
enter open · space peek · r reply · x stop · a answer · v review · / filter · esc close · o new pane
```

- **Step 4 (second `Ctrl+C` — fleet view closes):**
```text
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
· 1 background session needs you; press alt+a to see it.
│ > Type a message. Enter sends, Shift+Enter adds a line. │
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-0f7c62be/run │ ⚑ 1 needs you
```

- **Step 5 (`/fleet`, select same session, `Space` — approval card still present):**
```text
Approval needed · profile fixture
$ rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT=$?"
delete in root path
Hermes denies this after 300 s without an answer.
› 1. once
  2. session
  3. always
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close
✻ working
enter open · space peek · r reply · x stop · a answer · v review · / filter · esc close · o new pane
exit status: 0
```
The withdrawn denial clause did not appear.


## UAT-3

The operator used a second Herdr terminal, `t54`, at least 80 columns wide, with the same background approval session.

Command run from `/tmp/talaria-0f7c62be/run`:
```text
/tmp/talaria-0f7c62be/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
```

- **Step 1 (`/fleet`, select background approval session, `Space`):**
```text
Fleet · 4 sessions
NEEDS YOU (1)
›  ●  Remove nonexistent talaria …  minimax-m3.1-flash  approval: rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT=$?"  now
── peek ──
● Remove nonexistent talaria approval probe · fixture · minimax-m3.1-flash · needs you now
Approval needed · profile fixture
$ rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT=$?"
delete in root path
Hermes denies this after 300 s without an answer.
› 1. once
  2. session
  3. always
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close
✻ working
enter open · space peek · r reply · x stop · a answer · v review · / filter · esc close · o new pane
```

- **Step 2 (`a`, lowercase):**
```text
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list
✻ working
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list
```

- **Step 3 (first `Ctrl+C` — focus returns to list, card stays visible):**
```text
Approval needed · profile fixture
$ rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT=$?"
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close
✻ working
enter open · space peek · r reply · x stop · a answer · v review · / filter · esc close · o new pane
```

- **Step 4 (second `Ctrl+C` — fleet view closes):**
```text
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
· 1 background session needs you; press alt+a to see it.
│ > Type a message. Enter sends, Shift+Enter adds a line. │
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-0f7c62be/run │ ⚑ 1 needs you
```

- **Step 5 (`/fleet`, select same session, `Space` — approval card still present):**
```text
Approval needed · profile fixture
$ rm -rf /tmp/talaria-approval-probe-nonexistent; echo "EXIT=$?"
delete in root path
Hermes denies this after 300 s without an answer.
› 1. once
  2. session
  3. always
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close
✻ working
enter open · space peek · r reply · x stop · a answer · v review · / filter · esc close · o new pane
exit status: 0
```
The withdrawn denial clause did not appear.


## UAT-4

The UAT-1 seat showed `pong` as the first line of the reply and `Copy requested: reply 1, 4 characters.` The operator pasted outside Talaria before `/copy 9`:

```text
$ pbpaste | wc -m; echo "exit status: $?"
       4
exit status: 0
$ pbpaste > /tmp/talaria-0f7c62be/uat4-herdr.txt; head -n 1 /tmp/talaria-0f7c62be/uat4-herdr.txt; echo "exit status: $?"
pong
exit status: 0
```

The second paste was from a fresh foreground session in iTerm2 3.7.7, outside Herdr, not reusing UAT-2:

Command run from `/tmp/talaria-0f7c62be/run`:
```text
/tmp/talaria-0f7c62be/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
```

Screen after `Reply with the single word pong.` and `/copy`:
```text
> Reply with the single word pong.
pong
· Copy requested: reply 1, 4 characters. Your terminal must allow clipboard access.
```

Clipboard verification outside Talaria (clipboard cleared before `/copy`):
```text
$ pbpaste | wc -m; echo "exit status: $?"
       4
exit status: 0
$ pbpaste > /tmp/talaria-0f7c62be/uat4-own.txt; head -n 1 /tmp/talaria-0f7c62be/uat4-own.txt; echo "exit status: $?"
pong
exit status: 0
```
