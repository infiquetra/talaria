# Talaria v0.5.0 T1 targeted acceptance evidence

This active evidence set contains only the five targeted reruns against installed candidate commit
`17ce4eda8e82a18b5d47766c0c279aa9751dce9f`, wheel SHA-256
`0ed001392dabbc52071e8b795b31a68adb988c9d1639146da0a958764f2c31eb`. Three items passed, item 23
failed, and item 24 remains blocked. The complete earlier T1 sweep for candidate `d9c82443` is
preserved under `superseded/d9c82443/`; it is excluded from the active generated manifest because
those receipts bind to a different candidate.

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 2 | passed | [receipt](receipts/item-02-talaria-t1.json) | [raw](raw/item-02-r3.ansi) | [screenshot](screenshots/item-02-r3.png) |
| 23 | failed | [receipt](receipts/item-23-talaria-t1.json) | [reconnect](raw/item-23-reconnect-r3.ansi), [connect/disconnect](raw/item-23-r3.ansi), [authentication](raw/item-23-auth-r3.ansi) | [reconnect](screenshots/item-23-reconnect-r3.png), [connect/disconnect](screenshots/item-23-r3.png), [authentication](screenshots/item-23-auth-r3.png) |
| 24 | blocked | [receipt](receipts/item-24-talaria-t1.json) | [live approval](raw/item-24-live-r3.ansi), [relaunch](raw/item-24-polled-r3.ansi), [replay states](raw/item-24-r3.ansi) | [live approval](screenshots/item-24-live-r3.png), [relaunch](screenshots/item-24-polled-r3.png), [replay states](screenshots/item-24-r3.png) |
| 25 | passed | [receipt](receipts/item-25-talaria-t1.json) | [raw](raw/item-25-r3.ansi) | [screenshot](screenshots/item-25-r3.png) |
| 27 | passed | [receipt](receipts/item-27-talaria-t1.json) | [raw](raw/item-27-r3.ansi) | [screenshot](screenshots/item-27-r3.png) |

Item 2 completed a real Hermes-backed turn on the approved primary route,
`opencode-go / muse-spark-1.2-contributor`. The response was `1517`; both the inspector and the final
status-bar model segment named `muse-spark-1.2-contributor`. No fallback route was attempted.

Item 23 used separate monochrome drives so the authentication failure did not hide the healthy and
reconnecting states. The captures contain `[..] wait`, `[ok] up`, `[x] down`, and `[!] auth`. After
a healthy connection was deliberately lost, Talaria also displayed `connection lost —
reconnecting…`, proving that the reconnect cycle ran, but the footer never displayed its required
`[~]` reconnecting form. The footer remained `[x] down`, so the item failed rather than being
inferred from focused tests.

Item 24 exercised a real approval on the live gateway and every replayable agent and queue state.
The live approval was anchored by the gateway and appeared as `[!] approval waiting`, without a
`possibly duplicate` marker. A clean relaunch found no lingering approval row. The installed Hermes
gateway assigns each approval a `request_id`, while Talaria sets `possibly_duplicate` only for a
keyless or otherwise unanchored approval. The available live protocol therefore cannot produce that
state, and replay cannot encode the required admin-polled shape. Authoring such a frame would be
simulated acceptance, so the receipt remains blocked.

Item 25 shows the six required transcript identities on consecutive monochrome rows without spacer
rows. Item 27 shows a real wheel event moving the transcript to `READING-ANCHOR-007`; later replay
frames and the scripted resize preserve that top anchor while the view remains unpinned.

The screenshots were rendered from the corresponding raw ANSI pseudo-terminal bytes with Pyte and
Pillow. This preserves terminal layout without Computer Use or graphical user-interface automation.
Before publication, the selected evidence was checked for the scratch credential, token-query
parameters, bearer headers, email addresses, and operator home paths; none were present.
