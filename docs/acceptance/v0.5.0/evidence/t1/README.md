# Talaria v0.5.0 cycle-four acceptance evidence — talaria-t1

This directory records the T1 half of acceptance against candidate
`d16357900f6f3b472ff6e0309e7bf7fe70ad9b06`, installed from the shared wheel whose SHA-256 digest is
`7322029e90617c4ef91657f16c5cf0e1baad92ee4a02907b4705be8df8d08625`. The installed artifact,
not the source checkout, produced every capture below. Every item receipt records harness commit
`d16357900f6f3b472ff6e0309e7bf7fe70ad9b06`.

<!-- BEGIN GENERATED ACCEPTANCE MANIFEST COUNTS -->
The generated manifest reports 0 stale receipts, 0 missing current receipts, and 0 invalid item receipts.
<!-- END GENERATED ACCEPTANCE MANIFEST COUNTS -->

| Item | Verdict | Receipt | Raw terminal evidence | Screenshot |
| ---: | --- | --- | --- | --- |
| 1 | passed | [install receipt](install-receipt.json) | install probe, version, bare launch, and replay gate are embedded in the receipt | not applicable |
| 2 | passed | [receipt](receipts/item-02-talaria-t1.json) | [live primary turn](raw/item-02-c4.ansi) | [screenshot](screenshots/item-02-c4.png) |
| 4 | passed | [receipt](receipts/item-04-talaria-t1.json) | [Refined Default](raw/item-04-c4.ansi) | [screenshot](screenshots/item-04-c4.png) |
| 5 | passed | [receipt](receipts/item-05-talaria-t1.json) | [Dark Green Terminal](raw/item-05-c4.ansi) | [screenshot](screenshots/item-05-c4.png) |
| 6 | passed | [receipt](receipts/item-06-talaria-t1.json) | [Neutral Dark](raw/item-06-c4.ansi) | [screenshot](screenshots/item-06-c4.png) |
| 7 | passed | [receipt](receipts/item-07-talaria-t1.json) | [Accessible High Contrast](raw/item-07-c4.ansi) | [screenshot](screenshots/item-07-c4.png) |
| 8 | passed | [receipt](receipts/item-08-talaria-t1.json) | [preview cancellation](raw/item-08-c4.ansi) | [screenshot](screenshots/item-08-c4.png) |
| 9 | passed | [receipt](receipts/item-09-talaria-t1.json) | [user save](raw/item-09-user-c4.ansi), [repository save](raw/item-09-repository-c4.ansi), [session preview](raw/item-09-session-c4.ansi) | [user](screenshots/item-09-user-c4.png), [repository](screenshots/item-09-repository-c4.png), [session](screenshots/item-09-session-c4.png) |
| 10 | passed | [receipt](receipts/item-10-talaria-t1.json) | [fixture import](raw/item-10-import-c4b.ansi), [fallback and selection](raw/item-10-c4.ansi) | [import](screenshots/item-10-import-c4b.png), [interface](screenshots/item-10-c4.png) |
| 11 | passed | [receipt](receipts/item-11-talaria-t1.json) | [first import](raw/item-11-import-1-c4.ansi), [second import](raw/item-11-import-2-c4.ansi), [installed-theme restart](raw/item-11-c4b.ansi) | [first](screenshots/item-11-import-1-c4.png), [second](screenshots/item-11-import-2-c4.png), [interface](screenshots/item-11-c4b.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t1.json) | [invalid settings](raw/item-15-invalid-c4.ansi), [bounded command](raw/item-15-command-c4.ansi) | [invalid](screenshots/item-15-invalid-c4.png), [command](screenshots/item-15-command-c4.png) |
| 22 | passed | [receipt](receipts/item-22-talaria-t1.json) | [focus and caret](raw/item-22-c4.ansi) | [screenshot](screenshots/item-22-c4.png) |
| 23 | passed | [receipt](receipts/item-23-talaria-t1.json) | [healthy and reconnect cycle](raw/item-23-c4.ansi); item 33 supplies authentication | [screenshot](screenshots/item-23-c4.png) |
| 24 | blocked | [receipt](receipts/item-24-talaria-t1.json) | [reachable agent and queue states](raw/item-24-c4.ansi) | [screenshot](screenshots/item-24-c4.png) |
| 25 | passed | [receipt](receipts/item-25-talaria-t1.json) | [monochrome identities](raw/item-25-c4.ansi) | [screenshot](screenshots/item-25-c4.png) |
| 26 | passed | [receipt](receipts/item-26-talaria-t1.json) | [standard motion](raw/item-26-standard-c4.ansi), [reduced motion](raw/item-26-reduced-c4.ansi) | [standard](screenshots/item-26-standard-c4.png), [reduced](screenshots/item-26-reduced-c4.png) |
| 27 | passed | [receipt](receipts/item-27-talaria-t1.json) | [unpinned scroll](raw/item-27-c4b.ansi) | [screenshot](screenshots/item-27-c4b.png) |
| 28 | passed | [receipt](receipts/item-28-talaria-t1.json) | [pinned scroll](raw/item-28-c4.ansi) | [screenshot](screenshots/item-28-c4.png) |
| 31 | passed | [receipt](receipts/item-31-talaria-t1.json) | [malformed import](raw/item-31-c4.ansi) | [screenshot](screenshots/item-31-c4.png) |
| 33 | passed | [receipt](receipts/item-33-talaria-t1.json) | [stale credential](raw/item-33-c4b.ansi) | [screenshot](screenshots/item-33-c4b.png) |
| 34 | passed | [receipt](receipts/item-34-talaria-t1.json) | [completed turn and killed session](raw/item-34-c4.ansi), [hashed close audit](audits/item-34-session-close-c4.json) | [screenshot](screenshots/item-34-c4.png) |
| 35 | passed | [receipt](receipts/item-35-talaria-t1.json) | [running process](raw/item-35-before-c4.ansi), [restart](raw/item-35-after-c4.ansi) | [before](screenshots/item-35-before-c4.png), [after](screenshots/item-35-after-c4.png) |
| 36 | passed | [receipt](receipts/item-36-talaria-t1.json) | [T1 evidence half](raw/item-36-c4.ansi) | [screenshot](screenshots/item-36-c4.png) |

The cycle-four binding audit found no T1 event JSON that needed editing. The theme flows use slash
commands rather than the changed function keys, item 22 types nothing while a no-text region holds
focus, and the scroll flows already use upward wheel input plus F5 rather than Down-at-bottom to
unpin. All eighteen committed event scripts parsed through
`scripts.acceptance.v050_pty_driver.parse_events` before execution.

The live legs requested and observed `opencode-go / muse-spark-1.2-contributor`, with no fallback.
Item 2 completed a real turn and its recording contains matching `session.info` provider and model
fields. Item 23 captured `[..] wait`, `[ok] up`, `[~] retry`, and `[x] down` while only the isolated
dashboard on port 8790 restarted; the first replacement listener exited after becoming healthy, and
the identical command restored the dashboard as PID 11988. Item 33 then separately captured `[!] auth`
and HTTP 403 from the stale scratch credential. Item 34 closed only its acceptance-created session
and then showed gateway code 4001 while the dashboard remained available.

Item 24 remains blocked on current evidence. Hermes 0.21.0 assigns request identifiers to live
approval rows, so they do not enter the possibly-duplicate state. The shipped replay format cannot
encode the keyless admin-polled row that triggers that state without simulating acceptance. The
current replay nevertheless proves every reachable agent and queue form.

Screenshots are deterministic Portable Network Graphics renderings of the corresponding real raw
American National Standards Institute terminal bytes through Pyte and Pillow. This preserves the
pseudo-terminal geometry and colours without graphical user interface automation or a simulated
Talaria application. Captures were reviewed for the scratch credential, token queries,
authorization headers, bearer values, operator home paths, and email addresses before publication;
none were present. Publication replaced only the isolated scratch-root identifier and rebound the
published capture and pseudo-terminal result hashes.
