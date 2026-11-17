# pin acceptance record

Candidate: b9238a92b5c7d63f01136abf1da3be155f7b8d18
Hermes: ok    Hermes version  0.23.1 (3b421578), the release Talaria was built for
Herdr:
OS: linux
Date: 2026-11-17

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| suite | operator | PASS | pin.md#suite | b9238a92 |
| pinned-drift | operator | PASS | pin.md#pinned-drift | b9238a92 |
| live | operator | PASS | pin.md#live | b9238a92 |

```text
/tmp/talaria-b9238a92/talaria --version
talaria 0.8.1
hermes: pinned v0.23.1 (3b421578), supported from v0.23.0, tested to v0.23.1
```

## suite

```text
sh tools/release/rc.sh run --only tests; echo "exit status: $?"
GATE tests PASS 684 s
RC_PARTIAL: PASSED (1 of 1 required gates pass, 0 informational gates not passing, commit b9238a9)
exit status: 0
```

## pinned-drift

```text
sh tools/release/rc.sh run --only contract-pinned-drift; echo "exit status: $?"
GATE contract-pinned-drift PASS 96 s
RC_PARTIAL: PASSED (1 of 1 required gates pass, 0 informational gates not passing, commit b9238a9)
exit status: 0
```

## live

```text
sh tools/release/rc.sh run --only live-tests; echo "exit status: $?"
GATE live-tests PASS 389 s
RC_PARTIAL: PASSED (1 of 1 required gates pass, 0 informational gates not passing, commit b9238a9)
exit status: 0
gateway code_version=0.23.1 code_sha=3b421578588243f920886e6fe79a6faa87ea026e
```
