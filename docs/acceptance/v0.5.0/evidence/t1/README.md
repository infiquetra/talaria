# Talaria v0.5.0 T1 confirming acceptance evidence

This active evidence set records all twenty T1 assignments against the installed final candidate,
commit `0f5c8e3e44a43c5956f94ec3ccc348b7cdba1398` and wheel SHA-256
`720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b`. Nineteen items passed and
item 24 remains blocked by the accepted protocol limitation described below. Earlier T1 evidence is
preserved under `superseded/` and is excluded from the active generated manifest because it binds to
older candidates.

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 1 | passed | [install receipt](install-receipt.json) | recorded inside the install receipt | not applicable |
| 2 | passed | [receipt](receipts/item-02-talaria-t1.json) | [raw](raw/item-02-final2d.ansi) | [screenshot](screenshots/item-02-final2d.png) |
| 4 | passed | [receipt](receipts/item-04-talaria-t1.json) | [raw](raw/item-04-final2b.ansi) | [screenshot](screenshots/item-04-final2b.png) |
| 5 | passed | [receipt](receipts/item-05-talaria-t1.json) | [raw](raw/item-05-final2.ansi) | [screenshot](screenshots/item-05-final2.png) |
| 6 | passed | [receipt](receipts/item-06-talaria-t1.json) | [raw](raw/item-06-final2.ansi) | [screenshot](screenshots/item-06-final2.png) |
| 7 | passed | [receipt](receipts/item-07-talaria-t1.json) | [raw](raw/item-07-final2.ansi) | [screenshot](screenshots/item-07-final2.png) |
| 8 | passed | [receipt](receipts/item-08-talaria-t1.json) | [raw](raw/item-08-final2b.ansi) | [screenshot](screenshots/item-08-final2b.png) |
| 9 | passed | [receipt](receipts/item-09-talaria-t1.json) | [user](raw/item-09-user-final2.ansi), [repository](raw/item-09-repository-final2.ansi), [session](raw/item-09-session-final2.ansi) | [user](screenshots/item-09-user-final2.png), [repository](screenshots/item-09-repository-final2.png), [session](screenshots/item-09-session-final2.png) |
| 10 | passed | [receipt](receipts/item-10-talaria-t1.json) | [import](raw/item-10-import-final2.ansi), [interface](raw/item-10-final2.ansi) | [import](screenshots/item-10-import-final2.png), [interface](screenshots/item-10-final2.png) |
| 11 | passed | [receipt](receipts/item-11-talaria-t1.json) | [first import](raw/item-11-import-1-final2.ansi), [second import](raw/item-11-import-2-final2.ansi), [restart](raw/item-11-final2.ansi) | [first import](screenshots/item-11-import-1-final2.png), [second import](screenshots/item-11-import-2-final2.png), [restart](screenshots/item-11-final2.png) |
| 22 | passed | [receipt](receipts/item-22-talaria-t1.json) | [raw](raw/item-22-final2.ansi) | [screenshot](screenshots/item-22-final2.png) |
| 23 | passed | [receipt](receipts/item-23-talaria-t1.json) | [reconnect](raw/item-23-reconnect-final2.ansi), [disconnected](raw/item-23-down-final2.ansi), [authentication](raw/item-33-auth-final2b.ansi) | [recovered](screenshots/item-23-reconnect-final2.png), [disconnected](screenshots/item-23-down-final2.png), [authentication](screenshots/item-33-auth-final2b.png) |
| 24 | blocked | [receipt](receipts/item-24-talaria-t1.json) | [raw](raw/item-24-final2.ansi) | [screenshot](screenshots/item-24-final2.png) |
| 25 | passed | [receipt](receipts/item-25-talaria-t1.json) | [raw](raw/item-25-final2.ansi) | [screenshot](screenshots/item-25-final2.png) |
| 26 | passed | [receipt](receipts/item-26-talaria-t1.json) | [standard](raw/item-26-standard-final2.ansi), [reduced](raw/item-26-reduced-final2.ansi) | [standard](screenshots/item-26-standard-final2.png), [reduced](screenshots/item-26-reduced-final2.png) |
| 27 | passed | [receipt](receipts/item-27-talaria-t1.json) | [raw](raw/item-27-final2.ansi) | [screenshot](screenshots/item-27-final2.png) |
| 28 | passed | [receipt](receipts/item-28-talaria-t1.json) | [raw](raw/item-28-final2.ansi) | [screenshot](screenshots/item-28-final2.png) |
| 31 | passed | [receipt](receipts/item-31-talaria-t1.json) | [raw](raw/item-31-final2.ansi) | [screenshot](screenshots/item-31-error-visible-final2.png) |
| 33 | passed | [receipt](receipts/item-33-talaria-t1.json) | [raw](raw/item-33-auth-final2b.ansi) | [screenshot](screenshots/item-33-auth-final2b.png) |
| 35 | passed | [receipt](receipts/item-35-talaria-t1.json) | [before restart](raw/item-35-before-final2.ansi), [after restart](raw/item-35-after-final2.ansi) | [before restart](screenshots/item-35-before-final2.png), [after restart](screenshots/item-35-after-final2.png) |

Item 2 completed a real Hermes-backed turn on the approved primary route,
`opencode-go / muse-spark-1.2-contributor`. The answer was `1517`; both the inspector and the final
status-bar model segment named `muse-spark-1.2-contributor`. No fallback was attempted.

Item 23 used separate live drives so the deliberate authentication failure for item 33 could not
hide a healthy connect or reconnect transition. The selected captures contain all five required
non-colour forms: `[..] wait`, `[ok] up`, `[~] retry`, `[x] down`, and `[!] auth`. The reconnect
capture also contains `connection lost — reconnecting`, followed by recovery on the same dashboard
process.

Item 24 remains blocked. The live gateway assigns a request identifier, so its approval rows are
anchored and cannot become `possibly duplicate`. The replay format cannot encode the keyless
admin-polled approval shape. Constructing such a frame would simulate acceptance, which this run
forbids. The capture records every honestly reachable agent and queue form instead.

Item 33 used the one authorized restart of the isolated dashboard on port 8790. The listener changed
from process identifier 74947 to 19785 with the identical
`hermes -p default dashboard --host 127.0.0.1 --port 8790 --no-open` command. Talaria displayed an
explicit HTTP 403 authentication failure and `[!] auth`; the capture contains no credential. The
replacement dashboard remains detached under parent process identifier 1, and the scratch
credential was freshly minted after the failure leg.

The screenshots were rendered from the corresponding raw ANSI pseudo-terminal bytes with Pyte and
Pillow. This preserves terminal colour and layout without Computer Use or graphical user-interface
automation. Before publication, the selected evidence was checked for the scratch credential,
token-query parameters, bearer headers, email addresses, and operator home paths; none were present.
