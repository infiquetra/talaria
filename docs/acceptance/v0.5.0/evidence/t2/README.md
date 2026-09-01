# Talaria v0.5.0 T2 cycle-2 acceptance evidence

This active evidence set was produced from the isolated installed wheel for candidate commit
`788fc791fadd701cb74b7db8686c0a8bb444b8f8`, wheel SHA-256
`bc5406c8b201c08758b8c51db8ab54059fa291be93fa06766df662be9dea73be`. All twenty T2 receipts
pass. The previous final-candidate evidence is preserved under `superseded/0f5c8e3e-final2/`.

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 1 | passed | [receipt](receipts/item-01-talaria-t2.json) | [installed artifact](raw/item-01-c2.ansi) | [screenshot](screenshots/item-01-c2.png) |
| 2 | passed | [receipt](receipts/item-02-talaria-t2.json) | [live primary route](raw/item-02-live-public-c2.ansi) | [full route screenshot](screenshots/item-02-full-route-fresh-c2.png) |
| 3 | passed | [receipt](receipts/item-03-talaria-t2.json) | [main hierarchy](raw/item-03-c2.ansi) | [screenshot](screenshots/item-03-c2.png) |
| 12 | passed | [receipt](receipts/item-12-talaria-t2.json) | [all segments](raw/item-12-c2.ansi) | [screenshot](screenshots/item-12-c2.png) |
| 13 | passed | [receipt](receipts/item-13-talaria-t2.json) | [configured process](raw/item-13-c2.ansi), [fresh process](raw/item-13-restart-c2.ansi) | [configured](screenshots/item-13-c2.png), [fresh](screenshots/item-13-restart-c2.png) |
| 14 | passed | [receipt](receipts/item-14-talaria-t2.json) | [full resize sequence](raw/item-14-c2.ansi) | [19-column frame](screenshots/item-14-c2.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t2.json) | [invalid configuration](raw/item-15-invalid-c2.ansi), [bounded command](raw/item-15-command-c2.ansi) | [invalid configuration](screenshots/item-15-invalid-c2.png), [bounded command](screenshots/item-15-command-c2.png) |
| 16 | passed | [receipt](receipts/item-16-talaria-t2.json) | [dock and resize](raw/item-16-c2.ansi) | [28-column clamp](screenshots/item-16-c2.png) |
| 17 | passed | [receipt](receipts/item-17-talaria-t2.json) | [empty state](raw/item-17-empty-c2.ansi), [populated state](raw/item-17-populated-c2.ansi) | [empty](screenshots/item-17-empty-c2.png), [populated](screenshots/item-17-populated-c2.png) |
| 18 | passed | [receipt](receipts/item-18-talaria-t2.json) | [responsive state](raw/item-18-c2.ansi), [fresh process](raw/item-18-fresh-c2.ansi) | [responsive](screenshots/item-18-c2.png), [fresh reset](screenshots/item-18-fresh-c2.png) |
| 19 | passed | [receipt](receipts/item-19-talaria-t2.json) | [behavior](raw/item-19-c2.ansi), [open diff](raw/item-19-screen-c2.ansi) | [restored app](screenshots/item-19-c2.png), [side-by-side diff](screenshots/item-19-screen-c2.png) |
| 20 | passed | [receipt](receipts/item-20-talaria-t2.json) | [mode transitions](raw/item-20-c2.ansi), [111-column refusal](raw/item-20-screen-c2.ansi) | [restored layout](screenshots/item-20-c2.png), [unified refusal](screenshots/item-20-screen-c2.png) |
| 21 | passed | [receipt](receipts/item-21-talaria-t2.json) | [navigation and boundary](raw/item-21-c2.ansi), [open diff](raw/item-21-screen-c2.ansi) | [restored app](screenshots/item-21-c2.png), [navigated diff](screenshots/item-21-screen-c2.png) |
| 29 | passed | [receipt](receipts/item-29-talaria-t2.json) | [wide frame](raw/item-29-c2.ansi) | [wide screenshot](screenshots/item-29-c2.png) |
| 30 | passed | [receipt](receipts/item-30-talaria-t2.json) | [narrow behavior](raw/item-30-c2.ansi), [open diff](raw/item-30-screen-c2.ansi) | [restored app](screenshots/item-30-c2.png), [narrow screenshot](screenshots/item-30-screen-c2.png) |
| 32 | passed | [receipt](receipts/item-32-talaria-t2.json) | [session toggle](raw/item-32-c2.ansi), [fresh process](raw/item-32-restart-c2.ansi) | [toggle](screenshots/item-32-c2.png), [restored segment](screenshots/item-32-restart-c2.png) |
| 33 | passed | [receipt](receipts/item-33-talaria-t2.json) | [invalid scratch credential](raw/item-33-invalid-retry-c2.ansi) | [authentication error](screenshots/item-33-invalid-retry-c2.png) |
| 34 | passed | [receipt](receipts/item-34-talaria-t2.json) | [live killed session](raw/item-34-live-public-c2.ansi) | [visible recovery](screenshots/item-34-live-public-c2.png) |
| 35 | passed | [receipt](receipts/item-35-talaria-t2.json) | [running process](raw/item-35-running-c2.ansi), [fresh process](raw/item-35-restart-c2.ansi) | [running](screenshots/item-35-running-c2.png), [restarted](screenshots/item-35-restart-c2.png) |
| 36 | passed | [receipt](receipts/item-36-talaria-t2.json) | [T2 evidence half](raw/item-36-c2.ansi) | [screenshot](screenshots/item-36-c2.png) |

Item 1 launched only the executable installed from the frozen wheel. Items 3 and 29 show the wide
screen hierarchy: transcript and StatusRegion occupy the flexible body, Composer stays above the
one-row HelpBar, and BottomStatusBar is the terminal's last row.

Items 12 through 15 exercise the cycle-2 status implementation. Item 12 shows all seven default
segments in order with six separators. Item 13 shows only the configured five recognized segments
in the configured order, reports the unknown name, and repeats after process restart. Item 14's raw
capture traverses all eighteen required widths from 144 through 19 with real, unpinned terminal
dimensions; connection remains and the bar never gains a row. Item 15 visibly reports the malformed
command, invalid interval, and all three invalid width caps, while the paired command capture renders
literal bounded `acceptance-status` output.

Items 16 through 18 cover the inspector. Width changes proceed in four-column steps and clamp at 28
and 48 without changing held data. Item 17 uses separate populated and header-only replay processes;
all four headings remain, and each empty section shows the complete
`[none available from this session]` sentence while the drive visits widths 48, 28, and 36. Item 18
records open auto-restore, manual-close persistence, and the 119-column overlay; the separate fresh
process restores the expanded 36-column dock.

Items 19 through 21 use held state with three files, five hunks, intraline replacements, and long
clipped lines. Side-by-side and unified layouts preserve their selection and navigation state across
the 112/111 boundary. The modal and hints expose only navigation and view commands; unbound edit-like
keys do not reveal or perform edit, stage, revert, discard, apply, or any other mutation.

Item 30 is the required 78-by-36 narrow screenshot and item 29 is the required 132-by-36 wide
screenshot. The narrow capture keeps the inspector out of the dock and uses unified diff mode; the
wide capture retains the 36-column dock and the two fixed footer rows.

Item 32 hides `cwd` for the current process only. The generated [configuration comparison](pty-results/item-32-config-comparison.json)
records identical SHA-256 `184254f63e7f338a2c48da0cac6ab58680b83b0757f68db30b038fed6a3d9d71`
and identical modification time before and after the command; a fresh process restores `cwd`.

Item 2 used a fresh mode-0600 scratch credential against `ws://127.0.0.1:8790/api/ws`. The bounded
reply completed, and the full-width BottomStatusBar names
`opencode-go / muse-spark-1.2-contributor`. Item 33 changed
only the scratch credential and visibly reported HTTP 403 authentication failure without exposing
the credential, hanging, or rendering a silent blank. Its first attempt encountered a transient
connection refusal and remains scratch-only; the published retry is the authentication verdict.

Item 34 used the same approved primary route with no fallback. A real turn completed with
`TALARIA-T2-LIVE-OK`; a separately authenticated connection then closed exactly that recorded
session and Hermes returned `closed: true`. The generated [close proof](pty-results/item-34-session-close-public.json)
contains only the session identifier's SHA-256
`1cbc15eec701e8828948d0e226ce2389b4230961fb48f4b9d1c75ba2a37b9700`. The next prompt visibly
returned code 4001, `session not found`, while Talaria remained responsive and exited cleanly.

Item 35's generated [configuration mutation proof](pty-results/item-35-config-mutation.json) records
the scratch file changing from SHA-256
`6961dc3fdd138130b3c7891661114385f37b97f605aedd806915cf72d07560f0` to
`e21f5e01d1c57fcd5bbaba83b58bece8d870a878a794058b4c44d29308430a17`. The running application
continued to show `connection, version`; only the fresh process showed `cwd, connection, version`.

Item 36 is T2's half of the cross-tester check. T2 has one current-candidate receipt for all thirteen
assigned items and all seven shared items, including two successful live primary-route receipts.
The generated combined manifest remains the authority for the two-tester totals.

All screenshots were rendered from the corresponding real raw American National Standards
Institute terminal bytes using Pyte and Pillow. This preserves pseudo-terminal geometry and colors
without graphical user interface automation or a simulated Talaria application. Before publication,
every receipt-bound capture and screenshot was checked for operator paths, email addresses,
authorization material, credentials, and unrelated private identifiers; the two live captures were
also taken at 119 columns so the ephemeral session identifier never entered the raw evidence.
