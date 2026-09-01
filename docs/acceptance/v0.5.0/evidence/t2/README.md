# Talaria v0.5.0 T2 cycle-4 acceptance evidence

This active evidence set was produced from the isolated installed wheel for candidate and harness
commit `d16357900f6f3b472ff6e0309e7bf7fe70ad9b06`, wheel SHA-256
`7322029e90617c4ef91657f16c5cf0e1baad92ee4a02907b4705be8df8d08625`. All twenty T2 receipts
pass. The cycle-3 evidence is preserved under `superseded/4c2d8db-cycle3/`.

<!-- BEGIN GENERATED ACCEPTANCE MANIFEST COUNTS -->
The generated manifest reports 0 stale receipts, 0 missing current receipts, and 0 invalid item receipts.
<!-- END GENERATED ACCEPTANCE MANIFEST COUNTS -->

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 1 | passed | [receipt](receipts/item-01-talaria-t2.json) | [installed artifact](raw/item-01-c4.ansi) | [screenshot](screenshots/item-01-c4.png) |
| 2 | passed | [receipt](receipts/item-02-talaria-t2.json) | [live primary route](raw/item-02-live-public-c4.ansi) | [full route screenshot](screenshots/item-02-full-route-fresh-c4.png) |
| 3 | passed | [receipt](receipts/item-03-talaria-t2.json) | [main hierarchy](raw/item-03-c4.ansi) | [screenshot](screenshots/item-03-c4.png) |
| 12 | passed | [receipt](receipts/item-12-talaria-t2.json) | [all segments](raw/item-12-c4.ansi) | [screenshot](screenshots/item-12-c4.png) |
| 13 | passed | [receipt](receipts/item-13-talaria-t2.json) | [configured process](raw/item-13-c4.ansi), [fresh process](raw/item-13-restart-c4.ansi) | [configured](screenshots/item-13-c4.png), [fresh](screenshots/item-13-restart-c4.png) |
| 14 | passed | [receipt](receipts/item-14-talaria-t2.json) | [full resize sequence](raw/item-14-c4.ansi) | [19-column frame](screenshots/item-14-c4.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t2.json) | [invalid configuration](raw/item-15-invalid-c4.ansi), [bounded command](raw/item-15-command-c4.ansi) | [invalid configuration](screenshots/item-15-invalid-c4.png), [bounded command](screenshots/item-15-command-c4.png) |
| 16 | passed | [receipt](receipts/item-16-talaria-t2.json) | [dock and resize](raw/item-16-c4.ansi) | [28-column clamp](screenshots/item-16-c4.png) |
| 17 | passed | [receipt](receipts/item-17-talaria-t2.json) | [empty state](raw/item-17-empty-c4.ansi), [populated state](raw/item-17-populated-c4.ansi) | [empty](screenshots/item-17-empty-c4.png), [populated](screenshots/item-17-populated-c4.png) |
| 18 | passed | [receipt](receipts/item-18-talaria-t2.json) | [responsive state](raw/item-18-c4.ansi), [fresh process](raw/item-18-fresh-c4.ansi) | [responsive](screenshots/item-18-c4.png), [fresh reset](screenshots/item-18-fresh-c4.png) |
| 19 | passed | [receipt](receipts/item-19-talaria-t2.json) | [behavior](raw/item-19-c4.ansi), [open diff](raw/item-19-screen-c4.ansi) | [restored app](screenshots/item-19-c4.png), [side-by-side diff](screenshots/item-19-screen-c4.png) |
| 20 | passed | [receipt](receipts/item-20-talaria-t2.json) | [mode transitions](raw/item-20-c4.ansi), [111-column refusal](raw/item-20-screen-c4.ansi) | [restored layout](screenshots/item-20-c4.png), [unified refusal](screenshots/item-20-screen-c4.png) |
| 21 | passed | [receipt](receipts/item-21-talaria-t2.json) | [navigation and boundary](raw/item-21-c4.ansi), [open diff](raw/item-21-screen-c4.ansi) | [restored app](screenshots/item-21-c4.png), [navigated diff](screenshots/item-21-screen-c4.png) |
| 29 | passed | [receipt](receipts/item-29-talaria-t2.json) | [wide frame](raw/item-29-c4.ansi) | [wide screenshot](screenshots/item-29-c4.png) |
| 30 | passed | [receipt](receipts/item-30-talaria-t2.json) | [narrow behavior](raw/item-30-c4.ansi), [open diff](raw/item-30-screen-c4.ansi) | [restored app](screenshots/item-30-c4.png), [narrow screenshot](screenshots/item-30-screen-c4.png) |
| 32 | passed | [receipt](receipts/item-32-talaria-t2.json) | [session toggle](raw/item-32-c4.ansi), [fresh process](raw/item-32-restart-c4.ansi) | [toggle](screenshots/item-32-c4.png), [restored segment](screenshots/item-32-restart-c4.png) |
| 33 | passed | [receipt](receipts/item-33-talaria-t2.json) | [invalid scratch credential](raw/item-33-invalid-c4.ansi) | [authentication error](screenshots/item-33-invalid-c4.png) |
| 34 | passed | [receipt](receipts/item-34-talaria-t2.json) | [live killed session](raw/item-34-live-public-c4.ansi) | [visible recovery](screenshots/item-34-live-public-c4.png) |
| 35 | passed | [receipt](receipts/item-35-talaria-t2.json) | [running process](raw/item-35-running-c4.ansi), [fresh process](raw/item-35-restart-c4.ansi) | [running](screenshots/item-35-running-c4.png), [restarted](screenshots/item-35-restart-c4.png) |
| 36 | passed | [receipt](receipts/item-36-talaria-t2.json) | [T2 evidence half](raw/item-36-c4.ansi) | [screenshot](screenshots/item-36-c4.png) |

Items 1 and 3 prove the installed-artifact launch and main hierarchy. Items 12 through 15 exercise
the current status implementation: all segments, configured order and omissions, the full resize
sequence, every visible configuration fallback, and literal bounded command output. Item 14 records
the current single 20-47-column band and keeps connection visible below 20 without growing the bar.

Items 16 through 18 cover the inspector. Width changes proceed in four-column steps and clamp at 28
and 48 without changing held data. Item 17 uses separate populated and header-only processes; all
four headings remain and every empty section shows the complete
`[none available from this session]` sentence with the current wrapping. Item 18 records open
auto-restore, manual-close persistence, and the 119-column overlay; a separate fresh process restores
the expanded 36-column dock.

Items 19 through 21 use held state with three files, five hunks, intraline replacements, and long
clipped lines. Side-by-side and unified layouts preserve navigation state across the 112/111
boundary. The modal and hints expose only navigation and view commands; unbound edit-like keys do
not reveal or perform edit, stage, revert, discard, apply, or any other mutation.

Items 29 and 30 are the required screenshot pair: 132 by 36 with a docked 36-column inspector, and
78 by 36 with overlay-only inspector behavior and unified diff mode. Item 32 hides `cwd` only for
the running process. The generated [configuration comparison](pty-results/item-32-config-comparison.json)
records identical content hashes and nanosecond modification times before and after the command;
the fresh process restores `cwd`.

Item 2 requested and observed `opencode-go / muse-spark-1.2-contributor` with no fallback. Two real
bounded turns completed, and the full-width status bar names `muse-spark-1.2-contributor`. Hermes
0.21.0 also caused Talaria to display a compatibility warning that `commands.catalog` returned an
unrecorded `commands` key; the warning was honest and did not prevent either completion.

Item 33 changed only the scratch credential and visibly reported HTTP 403 authentication failure
without exposing the credential, hanging, or rendering a silent blank. Item 34 used the approved
primary route, completed `TALARIA-T2-LIVE-OK`, and then closed exactly the recorded session. The
generated [close proof](pty-results/item-34-session-close-public.json) records Hermes returning
`closed: true` and only a one-way session-identifier hash. The next prompt visibly returned code
4001, `session not found`, while Talaria remained responsive and exited cleanly.

Item 35's generated [configuration mutation proof](pty-results/item-35-config-mutation.json) records
different content hashes and modification times before and after the external edit. The running
application continued to show `connection, version`; only the fresh process showed
`cwd, connection, version`. Item 36 is T2's independent half of the cross-tester evidence check;
the generated combined manifest remains the authority after both tester branches are merged.

All screenshots were rendered from the corresponding real raw American National Standards
Institute terminal bytes using Pyte and Pillow. Before publication, every receipt-bound and
supplemental capture and screenshot was checked for operator paths, email addresses, authorization
material, credentials, and unrelated private identifiers.
