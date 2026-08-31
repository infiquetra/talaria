# Talaria v0.5.0 T2 acceptance evidence

This active evidence set was produced from the installed wheel for reviewed candidate commit
`d9c82443f51932483ecb37c653d5c0cd8c342dac`, wheel SHA-256
`839f7a26985267db5e0cc2fa52b46ac3924f7791cdfbc70fbadfdc0e7f6cdfda`. All 15 T2 receipts pass.
The immediately preceding candidate evidence is retained unchanged under `superseded/122bd918/`,
and the older `d869791` evidence remains under `superseded/d869791/`.

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 3 | passed | [receipt](receipts/item-03-talaria-t2.json) | [raw](raw/item-03.ansi) | [screenshot](screenshots/item-03.png) |
| 12 | passed | [receipt](receipts/item-12-talaria-t2.json) | [raw](raw/item-12.ansi) | [screenshot](screenshots/item-12.png) |
| 13 | passed | [receipt](receipts/item-13-talaria-t2.json) | [raw](raw/item-13-rerun.ansi) | [screenshot](screenshots/item-13.png) |
| 14 | passed | [receipt](receipts/item-14-talaria-t2.json) | [full sequence](raw/item-14.ansi) | [19-column endpoint](screenshots/item-14-width-19.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t2.json) | [invalid configuration](raw/item-15-invalid.ansi), [bounded command](raw/item-15-command.ansi) | [invalid configuration](screenshots/item-15-invalid.png), [bounded command](screenshots/item-15-command.png) |
| 16 | passed | [receipt](receipts/item-16-talaria-t2.json) | [raw](raw/item-16.ansi) | [screenshot](screenshots/item-16.png) |
| 17 | passed | [receipt](receipts/item-17-talaria-t2.json) | [empty state](raw/item-17-empty-rerun.ansi), [populated state](raw/item-17-populated-rerun.ansi) | [empty state](screenshots/item-17-empty.png), [populated state](screenshots/item-17-populated.png) |
| 18 | passed | [receipt](receipts/item-18-talaria-t2.json) | [responsive drive](raw/item-18.ansi), [119-column overlay](raw/item-18-overlay.ansi), [fresh process](raw/item-18-fresh.ansi) | [responsive drive](screenshots/item-18.png), [overlay](screenshots/item-18-overlay.png), [fresh process](screenshots/item-18-fresh.png) |
| 19 | passed | [receipt](receipts/item-19-talaria-t2.json) | [raw](raw/item-19-screenshot.ansi) | [screenshot](screenshots/item-19.png) |
| 20 | passed | [receipt](receipts/item-20-talaria-t2.json) | [mode transitions](raw/item-20-screenshot.ansi), [111-column refusal](raw/item-20-refusal.ansi) | [mode transitions](screenshots/item-20.png), [refusal](screenshots/item-20-refusal.png) |
| 21 | passed | [receipt](receipts/item-21-talaria-t2.json) | [navigation and unbound keys](raw/item-21-screenshot.ansi), [command palette](raw/item-21-palette.ansi) | [navigation](screenshots/item-21.png), [palette](screenshots/item-21-palette.png) |
| 29 | passed | [receipt](receipts/item-29-talaria-t2.json) | [raw](raw/item-29.ansi) | [wide screenshot](screenshots/item-29.png) |
| 30 | passed | [receipt](receipts/item-30-talaria-t2.json) | [raw](raw/item-30-screenshot.ansi) | [narrow screenshot](screenshots/item-30.png) |
| 32 | passed | [receipt](receipts/item-32-talaria-t2.json) | [session toggle](raw/item-32.ansi), [fresh process](raw/item-32-restart.ansi) | [session toggle](screenshots/item-32.png), [fresh process](screenshots/item-32-restart.png) |
| 34 | passed | [receipt](receipts/item-34-talaria-t2.json) | [live killed session](raw/item-34-live-killed-rerun.ansi) | [screenshot](screenshots/item-34.png) |

Item 14 was also run as 18 independent fresh processes at 144, 143, 120, 119, 112, 111, 96, 95,
80, 79, 64, 63, 48, 47, 32, 31, 20, and 19 columns. The paired raw captures, pseudo-terminal
results, and screenshots are named `item-14-width-<columns>` in their respective evidence
directories. The last status row at those widths shows the required progression: full forms at
144; compact forms at 143; then version, cwd, git, context, agent, and task drop at their documented
boundaries; connection remains alone at 19 columns. Every row stayed one cell high.

Item 17 independently confirms the repair that motivated this candidate. The populated process
shows held task, context, three changed files, and operation data. The header-only process keeps all
four headings and renders `[none available from this session]` in every section. It contains no
synthetic `needs-you unavailable` row. Because both processes use `talaria replay`, the inspector
cannot issue a gateway request; its values come only from the recorded session state.

Items 13 and 32 include fresh-process proof. Item 13's custom config remained SHA-256
`de189cd94d976240a6ba2f1748e82334cbbeb0938a1ce0321cc87310e98c0c1c`, 186 bytes, and modification
time `1788194417` before and after the drive. Item 32's config remained SHA-256
`1123f0cdd03ffbf3dd793248a822d92cce59376a71d18b06b06c7a0297aff697`, 74 bytes, and modification
time `1788194478` before the toggle, after the toggle, and after the fresh process. The restarted
item 32 process restores `cwd`.

Item 21's user-visible mutation boundary is structural: the command palette offers only `/diffs`,
the modal labels itself `read only`, and its hints contain navigation and view-mode actions only.
The held state contains three files, five hunks, intraline replacements, and long clipped lines.
The navigation drive sent unbound edit-like keys after moving through files, hunks, and horizontal
scroll; the diff data did not change and no edit, stage, revert, discard, or apply surface appeared.

Item 34 used the isolated dashboard at `ws://127.0.0.1:8790/api/ws`, Hermes profile `default`, and
dashboard process 77984. The observed primary route was
`opencode-go / muse-spark-1.2-contributor`; no fallback was requested or used. A real turn completed
with `TALARIA-T2-LIVE-OK`. A separate authenticated gateway connection then called `session.close`
for exactly the session identifier recorded by that Talaria process; Hermes returned `closed: true`
(session identifier SHA-256
`3531848184eac3d131dea886264bf800ab4148f4ab863545d2a27690206c6064`). The next prompt stayed
bounded and visible: `prompt.submit was refused by the gateway (code 4001): session not found`.
Talaria retained the completed transcript, profile/model/session context, and deliberate clean-exit
path instead of hanging or blanking. The dashboard itself remained running.

All screenshots were rendered from the corresponding real raw American National Standards
Institute terminal bytes with Pyte and Pillow. This preserves the pseudo-terminal geometry and
colours without graphical user interface automation or a simulated Talaria application. Before
publication, selected raw captures and rendered text were checked for credentials, operator home
paths, email addresses, and unrelated live-session identifiers; none were present.
