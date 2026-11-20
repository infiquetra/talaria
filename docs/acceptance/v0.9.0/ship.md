# ship acceptance record

Candidate: aa99090a63806ff54b3210d543785e01d2a8f3d6
Hermes: 0.23.2
Herdr: herdr-app 0.1.16
OS: Darwin 27.0.0 arm64 (operator); Linux 6.8.0-149-generic x86_64 GNU/Linux (test host)
Date: 2026-11-20

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| pin-suite | developer | PASS | #pin-suite | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| pin-drift | developer | PASS | #pin-drift | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| pin-live | developer | PASS | #pin-live | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| pin-no-skip | developer | PASS | #pin-no-skip | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| sync-1 | developer | PASS | #sync-1 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| sync-2 | developer | PASS | #sync-2 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| sig-1 | seat-lead | NOT RUN (the gateway sent no credit notice) | #sig-1 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| sig-2 | developer | PASS | #sig-2 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| act-1 | seat-lead | PASS | #act-1 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| act-2 | fleet-lead | PASS | #act-2 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| act-3 | fleet-lead | PASS | #act-3 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| act-4 | developer | PASS | #act-4 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| tru-1 | seat-lead | PASS | #tru-1 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| tru-2 | seat-lead | PASS | #tru-2 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| tru-3 | seat-lead | PASS | #tru-3 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| tru-4 | builder | PASS | #tru-4 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| crew-1 | workflow-runner | PASS | #crew-1 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| crew-2 | builder | PASS | #crew-2 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| crew-3 | configurer | PASS | #crew-3 | aa99090a63806ff54b3210d543785e01d2a8f3d6 |
| dogfood | seat-lead | PASS | #dogfood | aa99090a63806ff54b3210d543785e01d2a8f3d6 |

```text
/tmp/talaria-aa99090a/talaria --version
talaria 0.8.1
hermes: pinned v0.23.1 (3b421578), supported from v0.23.0, tested to v0.23.1
exit status: 0
/tmp/talaria-aa99090a/talaria doctor --profile fixture --json | grep -o 'running: [^"]*'
running: 0.23.2 at 730d593f
exit status: 0
```

## pin-suite

Operator capture on the candidate:

```text
$ sh tools/release/rc.sh run --only tests; echo "exit status: $?"
GATE tests PASS 1042 s
RC_PARTIAL: PASSED (1 of 1 required gates pass, 0 informational gates not passing, commit aa99090)
exit status: 0
```

## pin-drift

Operator capture on the candidate:

```text
$ sh tools/release/rc.sh run --only contract-pinned-drift; echo "exit status: $?"
GATE contract-pinned-drift PASS 94 s
RC_PARTIAL: PASSED (1 of 1 required gates pass, 0 informational gates not passing, commit aa99090)
exit status: 0
```

## pin-live

Operator gate and the separate REQ-4 capture:

```text
$ sh tools/release/rc.sh run --only live-tests; echo "exit status: $?"
GATE live-tests PASS 310 s
RC_PARTIAL: PASSED (1 of 1 required gates pass, 0 informational gates not passing, commit aa99090)
exit status: 0
```

```text
$ sh -c "$(awk -F'\t' '$1=="live-tests"{print $4}' tools/release/gates.tsv)" > /tmp/talaria-aa99090a/live.txt 2>&1; echo "exit status: $?"
exit status: 0
$ grep -m1 '^gateway code_version=' /tmp/talaria-aa99090a/live.txt
gateway code_version=0.23.2 code_sha=730d593fe3c6facd0e9c7a3fa2b6009f3b742c8e
$ grep -F '_RESULT: SKIPPED' /tmp/talaria-aa99090a/live.txt; echo "exit status: $?"
exit status: 1
```

## pin-no-skip

The same operator REQ-4 capture: the live command succeeded, and the skip search returned status 1 (no matching skip marker).

```text
$ sh -c "$(awk -F'\t' '$1=="live-tests"{print $4}' tools/release/gates.tsv)" > /tmp/talaria-aa99090a/live.txt 2>&1; echo "exit status: $?"
exit status: 0
$ grep -m1 '^gateway code_version=' /tmp/talaria-aa99090a/live.txt
gateway code_version=0.23.2 code_sha=730d593fe3c6facd0e9c7a3fa2b6009f3b742c8e
$ grep -F '_RESULT: SKIPPED' /tmp/talaria-aa99090a/live.txt; echo "exit status: $?"
exit status: 1
```

## sync-1

Operator evidence for `sync-1` and `sync-2` (`#589`, refreshed candidate `aa99090a63806ff54b3210d543785e01d2a8f3d6`):

- **Summary:**
  - `DATABASE_ID`: `44729687213` (`https://github.com/infiquetra/talaria/actions/runs/44729687213`)
  - `HEADSHA`: `aa99090a63806ff54b3210d543785e01d2a8f3d6`
  - `TAG`: `v0.23.3`, `PINTAG`: `v0.23.1`, `compare_versions`: `1`
  - `compare`: `success`, `bump`: `skipped` (`bump-failed: no`), `test`: `skipped` (`tests-failed: no`)
  - `outcome`: `stop-already-open`
  - `link`: `https://github.com/infiquetra/talaria/issues/749` (`Hermes v0.23.3 needs a manual sync`)
  - `merged: no`
  - `tagged: no`

```text
$ gh workflow run hermes-sync.yml --repo infiquetra/talaria --ref main; echo "exit status: $?"
https://github.com/infiquetra/talaria/actions/runs/44729687213
exit status: 0

$ gh run list --repo infiquetra/talaria --workflow hermes-sync.yml --limit 1 --json databaseId,status,conclusion,url,headSha,createdAt; echo "exit status: $?"
[{"conclusion":"success","createdAt":"2026-11-20T11:41:46Z","databaseId":44729687213,"headSha":"aa99090a63806ff54b3210d543785e01d2a8f3d6","status":"completed","url":"https://github.com/infiquetra/talaria/actions/runs/44729687213"}]
exit status: 0

$ gh run view 44729687213 --repo infiquetra/talaria --json jobs | jq '.jobs[] | {name, status, conclusion, steps: [.steps[] | {name, status, conclusion}]}'; echo "exit status: $?"
{"name":"hermes-sync","status":"completed","conclusion":"success","steps":[{"name":"Set up job","status":"completed","conclusion":"success"},{"name":"Checkout repository","status":"completed","conclusion":"success"},{"name":"compare","status":"completed","conclusion":"success"},{"name":"Install toolchain","status":"completed","conclusion":"skipped"},{"name":"Restore contract cache","status":"completed","conclusion":"skipped"},{"name":"Restore typify cache","status":"completed","conclusion":"skipped"},{"name":"Install cargo-typify","status":"completed","conclusion":"skipped"},{"name":"bump","status":"completed","conclusion":"skipped"},{"name":"test","status":"completed","conclusion":"skipped"},{"name":"pull","status":"completed","conclusion":"skipped"},{"name":"issue","status":"completed","conclusion":"skipped"},{"name":"Post Checkout repository","status":"completed","conclusion":"success"},{"name":"Complete job","status":"completed","conclusion":"success"}]}
exit status: 0

$ gh api repos/NousResearch/hermes-agent/releases/latest --jq .tag_name; echo "exit status: $?"
v0.23.3
exit status: 0

$ git show aa99090a63806ff54b3210d543785e01d2a8f3d6:crates/hermes-contract/PINNED; echo "exit status: $?"
tag=v0.23.1
commit=3b421578588243f920886e6fe79a6faa87ea026e
exit status: 0

$ python3 -c 'import sys; sys.path.insert(0,"tools/contract-gen"); from contract_gen import compare_versions; print(compare_versions(sys.argv[1], sys.argv[2]))' v0.23.3 v0.23.1; echo "exit status: $?"
1
exit status: 0

$ gh issue list --repo infiquetra/talaria --state open --search 'in:title "Hermes v0.23.3"' --limit 100 --json title,number; echo "exit status: $?"
[{"number":749,"title":"Hermes v0.23.3 needs a manual sync"}]
exit status: 0

outcome: stop-already-open
link: https://github.com/infiquetra/talaria/issues/749
merged: no
tagged: no
```

## sync-2

Operator evidence for `sync-1` and `sync-2` (`#589`, refreshed candidate `aa99090a63806ff54b3210d543785e01d2a8f3d6`):

- **Summary:**
  - `DATABASE_ID`: `44729687213` (`https://github.com/infiquetra/talaria/actions/runs/44729687213`)
  - `HEADSHA`: `aa99090a63806ff54b3210d543785e01d2a8f3d6`
  - `TAG`: `v0.23.3`, `PINTAG`: `v0.23.1`, `compare_versions`: `1`
  - `compare`: `success`, `bump`: `skipped` (`bump-failed: no`), `test`: `skipped` (`tests-failed: no`)
  - `outcome`: `stop-already-open`
  - `link`: `https://github.com/infiquetra/talaria/issues/749` (`Hermes v0.23.3 needs a manual sync`)
  - `merged: no`
  - `tagged: no`

```text
$ gh workflow run hermes-sync.yml --repo infiquetra/talaria --ref main; echo "exit status: $?"
https://github.com/infiquetra/talaria/actions/runs/44729687213
exit status: 0

$ gh run list --repo infiquetra/talaria --workflow hermes-sync.yml --limit 1 --json databaseId,status,conclusion,url,headSha,createdAt; echo "exit status: $?"
[{"conclusion":"success","createdAt":"2026-11-20T11:41:46Z","databaseId":44729687213,"headSha":"aa99090a63806ff54b3210d543785e01d2a8f3d6","status":"completed","url":"https://github.com/infiquetra/talaria/actions/runs/44729687213"}]
exit status: 0

$ gh run view 44729687213 --repo infiquetra/talaria --json jobs | jq '.jobs[] | {name, status, conclusion, steps: [.steps[] | {name, status, conclusion}]}'; echo "exit status: $?"
{"name":"hermes-sync","status":"completed","conclusion":"success","steps":[{"name":"Set up job","status":"completed","conclusion":"success"},{"name":"Checkout repository","status":"completed","conclusion":"success"},{"name":"compare","status":"completed","conclusion":"success"},{"name":"Install toolchain","status":"completed","conclusion":"skipped"},{"name":"Restore contract cache","status":"completed","conclusion":"skipped"},{"name":"Restore typify cache","status":"completed","conclusion":"skipped"},{"name":"Install cargo-typify","status":"completed","conclusion":"skipped"},{"name":"bump","status":"completed","conclusion":"skipped"},{"name":"test","status":"completed","conclusion":"skipped"},{"name":"pull","status":"completed","conclusion":"skipped"},{"name":"issue","status":"completed","conclusion":"skipped"},{"name":"Post Checkout repository","status":"completed","conclusion":"success"},{"name":"Complete job","status":"completed","conclusion":"success"}]}
exit status: 0

$ gh api repos/NousResearch/hermes-agent/releases/latest --jq .tag_name; echo "exit status: $?"
v0.23.3
exit status: 0

$ git show aa99090a63806ff54b3210d543785e01d2a8f3d6:crates/hermes-contract/PINNED; echo "exit status: $?"
tag=v0.23.1
commit=3b421578588243f920886e6fe79a6faa87ea026e
exit status: 0

$ python3 -c 'import sys; sys.path.insert(0,"tools/contract-gen"); from contract_gen import compare_versions; print(compare_versions(sys.argv[1], sys.argv[2]))' v0.23.3 v0.23.1; echo "exit status: $?"
1
exit status: 0

$ gh issue list --repo infiquetra/talaria --state open --search 'in:title "Hermes v0.23.3"' --limit 100 --json title,number; echo "exit status: $?"
[{"number":749,"title":"Hermes v0.23.3 needs a manual sync"}]
exit status: 0

outcome: stop-already-open
link: https://github.com/infiquetra/talaria/issues/749
merged: no
tagged: no
```

## sig-1

Feature: `credit notice`.

Operator preflight: `hc --version` reported `hc 0.1.21`. The local terminal was idle and at least 80 columns wide.

The screen reads below are excerpts beginning with this candidate's launch; older scrollback is omitted.

```text
$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- cd\ /tmp/talaria-aa99090a/run
{"started":false,"status":null,"turn":13}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /tmp/talaria-aa99090a/talaria\ --profile\ fixture\ --model\ minimax-m3.1-flash\ --provider\ ollama-cloud\ --record\ /tmp/talaria-aa99090a/frames.jsonl
{"started":false,"status":null,"turn":13}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc wait --timeout 30 --output Type\ a\ message. <hc-address>
{"children":[],"ended":false,"exit_code":null,"reason":"timeout","signal":null,"status":"idle","terminal":"<hc-address>","turn":13}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud --record /tmp/talaria-aa99090a/frames.jsonl
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- Reply\ with\ the\ single\ word\ ok.
{"started":true,"status":"idle","turn":13}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc wait --json --timeout 60 <hc-address>
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"done","terminal":"<hc-address>","turn":14}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud --record /tmp/talaria-aa99090a/frames.jsonl
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
> Reply with the single word ok.
ok
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run │ ctx 1%
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /quit
{"started":false,"status":"done","turn":14}
exit status: 0

$ hc get <hc-address> --json
{"address":"<hc-address>","argv":["/bin/bash"],"cols":132,"created_ms":1794622061906,"cwd":"<checkout>","die_with_parent":false,"exit_code":null,"finished_ms":null,"foreground":{"argv":["/bin/bash"],"cwd":"/tmp/talaria-aa99090a/run","name":"bash","pid":1889},"identity":"ghostty","keep":true,"labels":{"herdr-console.space-name":"Talaria v0.9.0 Test & UAT","herdr-console.tab-name":"<hc-address>","herdr.seen":"9","herdr.space":"spd27a01294b51431287bec3b450908543","where":"Talaria v0.9.0 Test & UAT / <hc-address>"},"name":"<hc-address>","notify":true,"parent":null,"pid":1889,"reason":null,"reported":true,"rm":false,"rows":43,"screen":"exact","signal":null,"state":"live","status":null,"status_ms":1795175165932,"template":null,"title":"<host>: /tmp/talaria-aa99090a/run","turn":14}
exit status: 0

$ grep -cF \"credits. /tmp/talaria-aa99090a/frames.jsonl
0
exit status: 1

$ grep -cF credits. /tmp/talaria-aa99090a/frames.jsonl
0
exit status: 1

$ wc -l /tmp/talaria-aa99090a/frames.jsonl
78 /tmp/talaria-aa99090a/frames.jsonl
exit status: 0
```

The flushed recording contains 78 frames and no `credits.` entry. There is no first matching credit key or credit-notice screen line to paste. The recording itself is not pasted.

NOT RUN (the gateway sent no credit notice)

## sig-2

**`sig-2` — Local sign-in notice and `/copy link` (fresh local session in `iTerm2 3.7.7` from `/tmp/talaria-aa99090a/run`)**

- **Session confirmation:** The sign-in session is **local**.
- **Screen before and after `/copy link`:**
```text
· /login
Sign in with a Nous account to unlock more models and tools.
· https://portal.nousresearch.com/manage-subscription?user_code=CQB8-FHPR
Type /copy link to copy it.
· CQB8-FHPR
Type /copy link to copy it.
· Do not share this code. Waiting for sign-in, up to 10 minutes.
Type /copy link to copy it.
· Copy requested: sign-in link and code. Your terminal must allow clipboard access.
```
- **Pasted clipboard contents outside Talaria (clipboard cleared before `/copy link`):**
```text
$ pbpaste > /tmp/talaria-aa99090a/sig2-signin.txt && cat /tmp/talaria-aa99090a/sig2-signin.txt; echo; echo "exit status: $?"
https://portal.nousresearch.com/manage-subscription?user_code=CQB8-FHPR
CQB8-FHPR
exit status: 0
```

## act-1

The screen reads are excerpts beginning with the new, unrecorded launch. The operator took the first clipboard paste after `/copy` and before `/copy 9` (recorded under act-4).

```text
$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /tmp/talaria-aa99090a/talaria\ --profile\ fixture\ --model\ minimax-m3.1-flash\ --provider\ ollama-cloud
{"started":false,"status":null,"turn":14}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- Reply\ with\ the\ single\ word\ pong.
{"started":true,"status":"idle","turn":14}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc wait --json --timeout 60 <hc-address>
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"done","terminal":"<hc-address>","turn":15}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
> Reply with the single word pong.
pong
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run │ ctx 1%
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /copy
{"started":false,"status":"done","turn":15}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
> Reply with the single word pong.
pong
· Copy requested: reply 1, 4 characters. Your terminal must allow clipboard access.
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run │ ctx 1%
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /copy\ 9
{"started":false,"status":"done","turn":15}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
> Reply with the single word pong.
pong
· Copy requested: reply 1, 4 characters. Your terminal must allow clipboard access.
· No reply 9 to copy.
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run │ ctx 1% │ ⚑ 1 needs you │ 1 working
exit status: 0
```

## act-2

**`act-2` — Fleet approval hint in operator's own terminal (`iTerm2 3.7.7`, outside Herdr, `140x42`)**

- Command from `/tmp/talaria-aa99090a/run`: `/tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud`
- Background approval session (`rm -rf /tmp/talaria-approval-probe-nonexistent`):
  - After `/fleet` + `Space`: `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close`
  - After `a`: `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list`
  - After first `Ctrl+C`: focus returns to list, card stays visible, `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close`
  - After second `Ctrl+C`: fleet view closes cleanly to composer
  - After reopened `/fleet` + `Space`: approval card still present, `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close`, `exit status: 0`, `deny and stop` appeared 0 times.

## act-3

**`act-3` — Fleet approval hint in a second Herdr terminal (`<second-hc-address>`, not `<hc-address>`, `>= 80` columns)**

- Same 5-step sequence verified on `/tmp/talaria-aa99090a/talaria`:
  - After `/fleet` + `Space`: `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close`
  - After `a`: `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list`
  - After first `Ctrl+C`: focus returns to list, card stays visible, `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close`
  - After second `Ctrl+C`: fleet view closes cleanly
  - After reopened `/fleet` + `Space`: approval card still present, `1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c close`, `exit status: 0`, `deny and stop` appeared 0 times.

## act-4

First `act-4` clipboard paste from `act-1` (`<hc-address>` inside Herdr on candidate `aa99090a`, immediately after `Copy requested: reply 1, 4 characters.`, before `/copy 9`):

```text
$ pbpaste | wc -m; echo "exit status: $?"
       4
exit status: 0
$ pbpaste > /tmp/talaria-aa99090a/act4-herdr.txt; head -n 1 /tmp/talaria-aa99090a/act4-herdr.txt; echo "exit status: $?"
pong
exit status: 0
```

**`act-4` — Second clipboard paste (fresh `iTerm2 3.7.7` foreground session outside Herdr, not reusing `act-2`)**

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
$ pbpaste > /tmp/talaria-aa99090a/act4-own.txt; head -n 1 /tmp/talaria-aa99090a/act4-own.txt; echo "exit status: $?"
pong
exit status: 0
```

## tru-1

The preceding act-1 seat was quit before a fresh launch. The read is an excerpt beginning with that launch, excluding earlier scrollback. It shows the composer without a ready-frame or runtime-description refusal.

```text
$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /quit
{"started":false,"status":"done","turn":15}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /tmp/talaria-aa99090a/talaria\ --profile\ fixture\ --model\ minimax-m3.1-flash\ --provider\ ollama-cloud
{"started":false,"status":null,"turn":15}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
· 1 background session needs you; press alt+a to see it.
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run │ ⚑ 1 needs you
exit status: 0

$ hc get <hc-address> --json
{"address":"<hc-address>","argv":["/bin/bash"],"cols":132,"created_ms":1794622061906,"cwd":"<checkout>","die_with_parent":false,"exit_code":null,"finished_ms":null,"foreground":{"argv":["/tmp/talaria-aa99090a/talaria","--profile","fixture","--model","minimax-m3.1-flash","--provider","ollama-cloud"],"cwd":"/tmp/talaria-aa99090a/run","name":"talaria","pid":2078307},"identity":"ghostty","keep":true,"labels":{"herdr-console.space-name":"Talaria v0.9.0 Test & UAT","herdr-console.tab-name":"<hc-address>","herdr.seen":"9","herdr.space":"spd27a01294b51431287bec3b450908543","where":"Talaria v0.9.0 Test & UAT / <hc-address>"},"name":"<hc-address>","notify":true,"parent":null,"pid":1889,"reason":null,"reported":true,"rm":false,"rows":43,"screen":"exact","signal":null,"state":"live","status":{"app":"talaria","state":"idle"},"status_ms":1795175360027,"template":null,"title":"<host>: /tmp/talaria-aa99090a/run","turn":15}
exit status: 0
```

## tru-2

The same seat was idle. The screen read is an excerpt from its current launch; the success notice wrapped across two terminal rows and is joined below.

```text
$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /clear
{"started":false,"status":"idle","turn":15}
exit status: 0

$ hc read <hc-address>
<host>:/tmp/talaria-aa99090a/run$ /tmp/talaria-aa99090a/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
· 1 background session needs you; press alt+a to see it.
· Started a new session in this folder. The previous session was not ended; resume it with talaria --resume local-6c2dd9eafcf2bfe8d09091e6b4d1cb7096840d77660c98ff010e80352e039696 --profile fixture
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run │ ⚑ 1 needs you
exit status: 0

$ bash -o pipefail -c hc\ read\ <hc-address>\ \|\ grep\ -c\ \"Started\ a\ new\ session\ in\ this\ folder.\"
1
exit status: 0
```

## tru-3

The same seat was working both before and after `/clear`. The last read is an excerpt around the refusal and live tools. The earlier success notice count was 1; the read after the refusal contains 0 success notices.

```text
$ bash -o pipefail -c hc\ read\ <hc-address>\ \|\ grep\ -c\ \"Started\ a\ new\ session\ in\ this\ folder.\"
1
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- Write\ a\ detailed\ 400-word\ essay\ explaining\ how\ TCP\ congestion\ control\ evolved\ from\ Tahoe\ through\ Reno\,\ NewReno\,\ CUBIC\,\ and\ BBR\,\ using\ numbered\ paragraphs.
{"started":true,"status":"idle","turn":15}
exit status: 0

$ hc get <hc-address> --json
{"address":"<hc-address>","argv":["/bin/bash"],"cols":132,"created_ms":1794622061906,"cwd":"<checkout>","die_with_parent":false,"exit_code":null,"finished_ms":null,"foreground":{"argv":["/tmp/talaria-aa99090a/talaria","--profile","fixture","--model","minimax-m3.1-flash","--provider","ollama-cloud"],"cwd":"/tmp/talaria-aa99090a/run","name":"talaria","pid":2078307},"identity":"ghostty","keep":true,"labels":{"herdr-console.space-name":"Talaria v0.9.0 Test & UAT","herdr-console.tab-name":"<hc-address>","herdr.seen":"9","herdr.space":"spd27a01294b51431287bec3b450908543","where":"Talaria v0.9.0 Test & UAT / <hc-address>"},"name":"<hc-address>","notify":true,"parent":null,"pid":1889,"reason":null,"reported":true,"rm":false,"rows":43,"screen":"exact","signal":null,"state":"live","status":{"app":"talaria","state":"working"},"status_ms":1795175379584,"template":null,"title":"<host>: /tmp/talaria-aa99090a/run","turn":15}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /clear
{"started":true,"status":"working","turn":15}
exit status: 0

$ hc get <hc-address> --json
{"address":"<hc-address>","argv":["/bin/bash"],"cols":132,"created_ms":1794622061906,"cwd":"<checkout>","die_with_parent":false,"exit_code":null,"finished_ms":null,"foreground":{"argv":["/tmp/talaria-aa99090a/talaria","--profile","fixture","--model","minimax-m3.1-flash","--provider","ollama-cloud"],"cwd":"/tmp/talaria-aa99090a/run","name":"talaria","pid":2078307},"identity":"ghostty","keep":true,"labels":{"herdr-console.space-name":"Talaria v0.9.0 Test & UAT","herdr-console.tab-name":"<hc-address>","herdr.seen":"9","herdr.space":"spd27a01294b51431287bec3b450908543","where":"Talaria v0.9.0 Test & UAT / <hc-address>"},"name":"<hc-address>","notify":true,"parent":null,"pid":1889,"reason":null,"reported":true,"rm":false,"rows":43,"screen":"exact","signal":null,"state":"live","status":{"app":"talaria","state":"working"},"status_ms":1795175379584,"template":null,"title":"<host>: /tmp/talaria-aa99090a/run","turn":15}
exit status: 0

$ hc read <hc-address>
I'll ground this in the primary sources before writing.
Finish or interrupt the current turn before /clear.
⏺ Fetch(https://www.rfc-editor.org/rfc/rfc9438.html)  … running 1s
⏺ Web search(BBR congestion control draft cardwell-ietf-iccrg-bbr-congestion-control bottl...)  … running 1s
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-aa99090a/run │ ctx 1% │ ⚑ 1 needs you │ 1 working
exit status: 0

$ bash -o pipefail -c hc\ read\ <hc-address>\ \|\ grep\ -c\ \"Started\ a\ new\ session\ in\ this\ folder.\"
0
exit status: 1

$ hc keys <hc-address> ctrl+c
{"status":"working","turn":15}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc wait --json --timeout 15 <hc-address>
{"children":[],"ended":true,"exit_code":null,"reason":"status","signal":null,"status":"idle","terminal":"<hc-address>","turn":16}
exit status: 0

$ env -u HC_TERMINAL_ID -u HC_SCOPE hc send <hc-address> -- /quit
{"started":false,"status":"idle","turn":16}
exit status: 0
```

## tru-4

Agent command in the candidate checkout, followed by the saved compilation and test output:

```text
cargo test -p talaria-cli --test docs
warning: crates/talaria-cli/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: crates/talaria-cli/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-cli` (manifest) generated 2 warnings
warning: crates/talaria-workflow/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: crates/talaria-workflow/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-workflow` (manifest) generated 2 warnings
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
warning: crates/hermes-client/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `hermes-client` (manifest) generated 1 warning
   Compiling ring v0.17.14
   Compiling rustls v0.23.46
   Compiling rustls-webpki v0.103.16
   Compiling tungstenite v0.26.2
   Compiling tokio-rustls v0.26.6
   Compiling tokio-tungstenite v0.26.2
   Compiling hermes-client v0.8.1 (<checkout>/crates/hermes-client)
   Compiling talaria-workflow v0.8.1 (<checkout>/crates/talaria-workflow)
   Compiling talaria-app v0.8.1 (<checkout>/crates/talaria-app)
   Compiling talaria-tui v0.8.1 (<checkout>/crates/talaria-tui)
   Compiling talaria-cli v0.8.1 (<checkout>/crates/talaria-cli)
    Finished `test` profile [unoptimized] target(s) in 51.53s
     Running tests/docs.rs (/tmp/talaria-v090-target/debug/deps/docs-0b7cc7c0f124bf36)

running 12 tests
test product_descriptions_match_the_rust_client ... ok
test required_pages_exist_with_headings ... ok
test migration_table_has_a_row_for_every_old_setting ... ok
test required_statements_are_present ... ok
test keybinding_tables_equal_default_key_map ... ok
test corrected_records_match_the_code ... ok
test migration_notices_block_is_what_a_reader_sees ... ok
test migration_default_notice_count_matches_the_page ... ok
test relative_links_and_anchors_resolve ... ok
test pages_and_fixtures_are_public_safe ... ok
test commands_in_docs_exist_in_help ... ok
test migration_table_outcomes_match_the_code ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
```

## crew-1

Operator capture with `TALARIA_CONFIG_DIR` naming an empty directory without `staffing.toml`:

```text
$ TALARIA_CONFIG_DIR="<empty>" /tmp/talaria-aa99090a/talaria workflow roster --profile fixture; echo "exit status: $?"
Roster for profile fixture at 2026-11-20 11:49 UTC

Providers (signed in)
  anthropic     20 models   ready
  deepseek       2 models   ready
  ollama-cloud  28 models   ready

Tiers

Limits: 5 agents at once
Budget: none
Estimate: no estimate without a token budget; tier prices:  per million tokens
Workflows:
Skill: foreign
exit status: 0
```

## crew-2

Operator capture using the consolidated integration target and exact test filter:

```text
$ cargo test -p talaria-workflow --test integration -- --exact staffing_table::refuses_unknown_and_invalid_keys; echo "exit status: $?"
test staffing_table::refuses_unknown_and_invalid_keys ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 337 filtered out; finished in 0.01s
exit status: 0
```

## crew-3

The `Workflow width default` decision states that the contiguous healthy width N is **5**. The operator captured the matching missing-file sentence and unchanged sample:

```text
$ grep -n -C 1 'missing file\|max_in_flight' docs/workflows/staffing.md; echo "exit status: $?"
6:A missing file means no tiers, five simultaneous agents and no budget.
19:max_in_flight = 10
exit status: 0
```

## dogfood

- **Result:** `PASS`
- **Candidate commit:** `aa99090a63806ff54b3210d543785e01d2a8f3d6`
- **Launch command:** `/tmp/talaria-aa99090a/talaria --profile <profile>` (switched active model inside Talaria via `/model deepseek-v4-pro` after provider HTTP 400 on default model name, without modifying `<hermes-home>`)
- **Issue implemented:** https://github.com/infiquetra/talaria/issues/565 (`TRU-7`: Help text for `--resume`, closed)
- **Pull request URL:** https://github.com/infiquetra/talaria/pull/752
- **Merge verification:** Merged by operator `@namredips` (not an agent, per Decision 6) at `2026-11-20T14:34:19Z`, merge commit `75b918836630793193916afb4993658eff81919b`
- **Code review (`reviewer-2`):** `APPROVE` (0 P0, 0 P1)
- **Local `tools/prepush.sh` (`env -u HERMES_HOME`):** PASS (`4535 passed, 0 failed, 151 skipped`, 81.54% line coverage, `DOC_SCAN: OK (636 files)`)
- **GitHub CI checks:** `bench` PASS, `rust-check (macos-14)` PASS, `rust-check (ubuntu-latest)` PASS, `rust-coverage` PASS

`checks passed`

**First Prompt Sent to `<hc-address>`**

```text
Implement issue #565 (TRU-7: Help text for --resume) in this worktree (/tmp/talaria-aa99090a/dogfood on branch ship-acc-dogfood-aa99090a). Read gh issue view 565 --repo infiquetra/talaria and docs/agents/patterns.md first, and follow every requirement REQ-1 through REQ-6, AC-1 through AC-6, UAT-1, UAT-2, and the two-commit convention exactly: (1) Commit 1 titled "test(cli): lock the resume help sentence" touching ONLY crates/talaria-cli/tests/cli_args.rs (verify flags_help_and_neutralised_errors fails before Commit 2); (2) Commit 2 titled "docs(cli): explain resume in the long help" touching ONLY crates/talaria-cli/src/args.rs, docs/engineering-journal/LEARNINGS.md, and docs/engineering-journal/DECISIONS.md (zero test edits in Commit 2, zero attribution lines anywhere, do not touch docs/guide.md, launch.rs, failure.rs, or docs.rs). Use CARGO_TARGET_DIR=/tmp/talaria-v090-target and unset RUSTFLAGS when running the three cargo test checks, tools/prepush.sh, sh tools/release/doc-scan.sh, and building the branch binary for UAT-1 (talaria --help) and UAT-2 (talaria --resume local-session-1 --model minimax-m3.1-flash). Push branch ship-acc-dogfood-aa99090a to origin and open a pull request targeting main for #565 with an "Acceptance evidence" section containing verbatim UAT-1 and UAT-2 blocks and zero attribution lines. Do NOT merge the pull request. Report the PR URL and verification summary when done.
```

**`hc get --json` State Snapshots While `<hc-address>` Worked**

```json
{"address":"<hc-address>","name":"<hc-address>","state":"live","status":{"app":"talaria","state":"idle"},"turn":0,"status_ms":1795176354062,"foreground":{"name":"talaria","cwd":"/tmp/talaria-aa99090a/dogfood","argv":["/tmp/talaria-aa99090a/talaria","--profile","<profile>"]}}
{"address":"<hc-address>","name":"<hc-address>","state":"live","status":{"app":"talaria","state":"error"},"turn":1,"status_ms":1795176390784,"foreground":{"name":"talaria","cwd":"/tmp/talaria-aa99090a/dogfood","argv":["/tmp/talaria-aa99090a/talaria","--profile","<profile>"]}}
{"address":"<hc-address>","name":"<hc-address>","state":"live","status":{"app":"talaria","state":"working"},"turn":1,"status_ms":1795176452930,"foreground":{"name":"talaria","cwd":"/tmp/talaria-aa99090a/dogfood","argv":["/tmp/talaria-aa99090a/talaria","--profile","<profile>"]}}
{"address":"<hc-address>","name":"<hc-address>","state":"live","status":{"app":"talaria","state":"working"},"turn":1,"status_ms":1795176452930,"foreground":{"name":"talaria","cwd":"/tmp/talaria-aa99090a/dogfood","argv":["/tmp/talaria-aa99090a/talaria","--profile","<profile>"]}}
{"address":"<hc-address>","name":"<hc-address>","state":"live","status":{"app":"talaria","state":"working"},"turn":1,"status_ms":1795176452930,"foreground":{"name":"talaria","cwd":"/tmp/talaria-aa99090a/dogfood","argv":["/tmp/talaria-aa99090a/talaria","--profile","<profile>"]}}
{"address":"<hc-address>","name":"<hc-address>","state":"live","status":{"app":"talaria","state":"working"},"turn":1,"status_ms":1795176452930,"foreground":{"name":"talaria","cwd":"/tmp/talaria-aa99090a/dogfood","argv":["/tmp/talaria-aa99090a/talaria","--profile","<profile>"]}}
{"address":"<hc-address>","name":"<hc-address>","state":"live","status":{"app":"talaria","state":"done"},"turn":2,"status_ms":1795178955181,"foreground":{"name":"talaria","cwd":"/tmp/talaria-aa99090a/dogfood","argv":["/tmp/talaria-aa99090a/talaria","--profile","<profile>"]}}
```
