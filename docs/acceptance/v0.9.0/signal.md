# signal acceptance record

Candidate: 50457c8001822e218bd1b45f75c3a361d6348804
Hermes: 0.23.2
Herdr: herdr-app 0.1.16
OS: linux
Date: 2026-11-20

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| UAT-1 | seat-lead | NOT RUN (the gateway sent no credit notice) | #uat-1 | 50457c8001822e218bd1b45f75c3a361d6348804 |
| UAT-2 | developer | PASS | #uat-2 | 50457c8001822e218bd1b45f75c3a361d6348804 |

```text
/tmp/talaria-50457c80/talaria --version
talaria 0.8.1
hermes: pinned v0.23.1 (3b421578), supported from v0.23.0, tested to v0.23.1
/tmp/talaria-50457c80/talaria doctor --profile fixture --json | grep -o 'running: [^"]*'
running: 0.23.2 at 730d593f
```

## UAT-1

```text
/tmp/talaria-50457c80/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud --record /tmp/talaria-50457c80/frames.jsonl
```

The local session started in `/tmp/talaria-50457c80/run`. The tester sent `Reply with the single word ok.` and the completed turn showed `ok`.

```text
hc --version
hc 0.1.21
```

The public NousResearch/hermes-agent source paths `agent/rate_limit_credits.py`, `hermes_cli/config_defaults.py`, `agent/credits_tracker.py`, `gateway/run_turn_progress.py`, and `gateway/slash_commands_login.py` were re-verified at live gateway commit `730d593f`. The `fixture` profile left `display.credits_notices` at its default, `True`.

After Talaria exited and the recording was flushed:

```text
grep -cE '"key": ?"credits\.' /tmp/talaria-50457c80/frames.jsonl
0
wc -l /tmp/talaria-50457c80/frames.jsonl
115 /tmp/talaria-50457c80/frames.jsonl
```

NOT RUN (the gateway sent no credit notice)

## UAT-2

```text
/tmp/talaria-50457c80/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud --record /tmp/talaria-50457c80/frames.jsonl
```

The operator confirmed that the sign-in session was local: the same recording session launched from `/tmp/talaria-50457c80/run`. Screen text after `/login` and `/copy link`:

```text
· /login
Sign in with a Nous account to unlock more models and tools.
· https://portal.nousresearch.com/manage-subscription?user_code=T7U2-BLZK
Type /copy link to copy it.
· T7U2-BLZK
Type /copy link to copy it.
· Do not share this code. Waiting for sign-in, up to 10 minutes.
Type /copy link to copy it.
· Copy requested: sign-in link and code. Your terminal must allow clipboard access.
```

Pasted clipboard contents outside Talaria:

```text
$ pbpaste > /tmp/talaria-50457c80/uat2-signin.txt && cat /tmp/talaria-50457c80/uat2-signin.txt; echo; echo "exit status: $?"
https://portal.nousresearch.com/manage-subscription?user_code=T7U2-BLZK
T7U2-BLZK
exit status: 0
```

## Live test

`TALARIA_LIVE_REQUIRE` and `SUDO_PASSWORD` were unset.

```text
cargo test -p hermes-client --test live_sudo_reconnect -- --ignored --nocapture waiting_sudo_request_survives_a_reconnect; echo "exit status: $?"
gateway code_version=0.23.2 code_sha=730d593fe3c6facd0e9c7a3fa2b6009f3b742c8e
no sudo request arrived: sudo must ask for a password and SUDO_PASSWORD must be unset
LIVE_SUDO_RECONNECT_RESULT: SKIPPED (no sudo request arrived: sudo must ask for a password and SUDO_PASSWORD must be unset)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 14.50s
exit status: 0
```
