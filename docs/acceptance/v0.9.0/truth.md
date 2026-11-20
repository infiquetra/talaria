# truth acceptance record

Candidate: d556f0446722adf434ce0e4410fabaa36b055967
Hermes: 0.23.2
Herdr: herdr-app 0.1.16
OS: Darwin 27.0.0 arm64 (operator); Linux 6.8.0-149-generic x86_64 GNU/Linux (test host)
Date: 2026-11-20

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| UAT-1 | developer | PASS | #uat-1 | d556f0446722adf434ce0e4410fabaa36b055967 |
| UAT-2 | developer | PASS | #uat-2 | d556f0446722adf434ce0e4410fabaa36b055967 |
| UAT-3 | developer | PASS | #uat-3 | d556f0446722adf434ce0e4410fabaa36b055967 |
| UAT-4 | builder | PASS | #uat-4 | d556f0446722adf434ce0e4410fabaa36b055967 |

```text
/tmp/talaria-d556f044/talaria --version
talaria 0.8.1
hermes: pinned v0.23.1 (3b421578), supported from v0.23.0, tested to v0.23.1
/tmp/talaria-d556f044/talaria doctor --profile fixture --json | grep -o 'running: [^"]*'
running: 0.23.2 at 730d593f
```

## UAT-1

Operator confirmation for #568 (scrubbed):

> Local `hc` terminal address: `<hc-address>` (idle shell prompt in `/tmp/talaria-d556f044/run`, empty directory, >= 80 columns, no machine prefix).
> Operator: `herdr-app 0.1.16`, `hc 0.1.21`, Darwin 27.0.0 arm64. Test host: `hc 0.1.21`, Linux 6.8.0-149-generic x86_64 GNU/Linux.
> The `fixture` profile skin `skins/nullcolour.yaml` exists and `display.skin: nullcolour` is set. Ready for UAT-1..3 on `/tmp/talaria-d556f044/talaria`.

```text
hc --version
hc 0.1.21
hc send <hc-address> -- "cd /tmp/talaria-d556f044/run"
hc send <hc-address> -- "/tmp/talaria-d556f044/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud"
hc read <hc-address>
<host>:/tmp/talaria-d556f044/run$ cd /tmp/talaria-d556f044/run
<host>:/tmp/talaria-d556f044/run$ /tmp/talaria-d556f044/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
· This gateway runs Hermes 0.23.2. This Talaria was tested up to v0.23.1. See docs/compatibility.md.
│ > Type a message. Enter sends, Shift+Enter adds a line. │
minimax-m3.1-flash │ approvals: manual │ /tmp/talaria-d556f044/run
```

## UAT-2

The terminal pane wrapped the success notice across two rows; the rows are joined onto one line below.

```text
hc send <hc-address> -- "/clear"
hc read <hc-address>
· Started a new session in this folder. The previous session was not ended; resume it with talaria --resume local-bfd3e63e9b544e783268ec67db076877c39bfd2db869a048262150eadaac6377 --profile fixture
│ > Type a message. Enter sends, Shift+Enter adds a line. │
```

## UAT-3

```text
hc read <hc-address> | grep -c 'Started a new session in this folder.'
1
hc send <hc-address> -- "Write a detailed 400-word essay explaining how TCP congestion control evolved from Tahoe through Reno, NewReno, CUBIC, and BBR, using numbered paragraphs."
hc get <hc-address> --json
{"address":"<hc-address>","argv":["/bin/bash"],"cols":132,"created_ms":1794622061906,"cwd":"<checkout>","die_with_parent":false,"exit_code":null,"finished_ms":null,"foreground":{"argv":["/tmp/talaria-d556f044/talaria","--profile","fixture","--model","minimax-m3.1-flash","--provider","ollama-cloud"],"cwd":"/tmp/talaria-d556f044/run","name":"talaria","pid":1078836},"identity":"ghostty","keep":true,"labels":{"herdr-console.space-name":"Talaria v0.9.0 Test & UAT","herdr-console.tab-name":"<hc-address>","herdr.seen":"7","herdr.space":"spd27a01294b51431287bec3b450908543","where":"Talaria v0.9.0 Test & UAT / <hc-address>"},"name":"<hc-address>","notify":true,"parent":null,"pid":1889,"reason":null,"reported":true,"rm":false,"rows":43,"screen":"exact","signal":null,"state":"live","status":{"app":"talaria","state":"working"},"status_ms":1795150947696,"template":null,"title":"<host>: /tmp/talaria-d556f044/run","turn":7}
hc send <hc-address> -- "/clear"
hc get <hc-address> --json
{"address":"<hc-address>","argv":["/bin/bash"],"cols":132,"created_ms":1794622061906,"cwd":"<checkout>","die_with_parent":false,"exit_code":null,"finished_ms":null,"foreground":{"argv":["/tmp/talaria-d556f044/talaria","--profile","fixture","--model","minimax-m3.1-flash","--provider","ollama-cloud"],"cwd":"/tmp/talaria-d556f044/run","name":"talaria","pid":1078836},"identity":"ghostty","keep":true,"labels":{"herdr-console.space-name":"Talaria v0.9.0 Test & UAT","herdr-console.tab-name":"<hc-address>","herdr.seen":"7","herdr.space":"spd27a01294b51431287bec3b450908543","where":"Talaria v0.9.0 Test & UAT / <hc-address>"},"name":"<hc-address>","notify":true,"parent":null,"pid":1889,"reason":null,"reported":true,"rm":false,"rows":43,"screen":"exact","signal":null,"state":"live","status":{"app":"talaria","state":"working"},"status_ms":1795150947696,"template":null,"title":"<host>: /tmp/talaria-d556f044/run","turn":7}
hc read <hc-address>
· Started a new session in this folder. The previous session was not ended; resume it with talaria --resume local-bfd3e63e9b544e783268ec67db076877c39bfd2db869a048262150eadaac6377 --profile fixture
> Write a detailed 400-word essay explaining how TCP congestion control evolved from Tahoe through Reno, NewReno, CUBIC, and BBR, us
ing numbered paragraphs.
Finish or interrupt the current turn before /clear.
∴ thinking
hc read <hc-address> | grep -c 'Started a new session in this folder.'
1
```

The read above is an excerpt; the terminal wrapped the earlier success notice as described in UAT-2.

## UAT-4

```text
git rev-parse HEAD
d556f0446722adf434ce0e4410fabaa36b055967
cargo test -p talaria-cli --test docs
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
exit status: 0
```
