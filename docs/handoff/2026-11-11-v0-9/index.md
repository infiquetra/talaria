# Issue index

The issue number for every key in the [release map](release-map.md).

## Talaria v0.8.1

| Key | Issue | Type | Title |
|---|---|---|---|
| SEAT | [#519](https://github.com/infiquetra/talaria/issues/519) | parent | Talaria is a working Herdr seat: it reports its state and can be driven from outside |
| SEAT-0 | [#532](https://github.com/infiquetra/talaria/issues/532) | enhancement | One pre-push script that runs what continuous integration runs on a pull request |
| SEAT-1 | [#533](https://github.com/infiquetra/talaria/issues/533) | enhancement | Seat state in the domain core: idle, working, blocked with a kind, done and error |
| SEAT-2 | [#534](https://github.com/infiquetra/talaria/issues/534) | enhancement | Terminal module asks whether the terminal reads status records and writes one per state change |
| SEAT-3 | [#535](https://github.com/infiquetra/talaria/issues/535) | enhancement | Report the seat state to the terminal, behind the existing report_state setting |
| SEAT-4A | [#536](https://github.com/infiquetra/talaria/issues/536) | enhancement | Smoke harness: an hc-backed terminal beside the old Herdr one |
| SEAT-4B | [#537](https://github.com/infiquetra/talaria/issues/537) | enhancement | Smoke harness: choose the terminal, and run the first suite on hc |
| SEAT-4C | [#538](https://github.com/infiquetra/talaria/issues/538) | enhancement | Smoke harness: hc is chosen automatically and the first release gate runs on it |
| SEAT-5 | [#539](https://github.com/infiquetra/talaria/issues/539) | context-update | Guide: running Talaria as a Herdr seat |
| SEAT-ACC | [#540](https://github.com/infiquetra/talaria/issues/540) | enhancement | Live proof: a lead sends a prompt to a Talaria seat, waits for it and reads its state |
| REL-081 | [#541](https://github.com/infiquetra/talaria/issues/541) | enhancement | Release Talaria v0.8.1 |

## Talaria v0.9.0

| Key | Issue | Type | Title |
|---|---|---|---|
| PIN | [#520](https://github.com/infiquetra/talaria/issues/520) | parent | Talaria runs on the latest Hermes release |
| PIN-1 | [#542](https://github.com/infiquetra/talaria/issues/542) | enhancement | Contract generator accepts the Hermes v0.23 contracts |
| PIN-2 | [#543](https://github.com/infiquetra/talaria/issues/543) | enhancement | Tests stop hard-coding which Hermes release is pinned |
| PIN-3 | [#544](https://github.com/infiquetra/talaria/issues/544) | enhancement | Move the Hermes pin to Hermes v0.23.1 |
| PIN-4 | [#545](https://github.com/infiquetra/talaria/issues/545) | enhancement | Smoke example screens take the built-for Hermes release from the pin |
| PIN-5 | [#546](https://github.com/infiquetra/talaria/issues/546) | enhancement | Regression tests for print-mode cancel now that Hermes interrupts a started prompt |
| PIN-ACC | [#547](https://github.com/infiquetra/talaria/issues/547) | enhancement | Live proof: the suite, the pinned-drift gate and the live tests pass at the new pin |
| SYNC | [#521](https://github.com/infiquetra/talaria/issues/521) | parent | Talaria stays in step with Hermes |
| SYNC-1 | [#548](https://github.com/infiquetra/talaria/issues/548) | enhancement | Compatibility table: which Talaria release matches which Hermes releases |
| SYNC-2 | [#549](https://github.com/infiquetra/talaria/issues/549) | enhancement | Tell the operator once when the gateway is newer than tested or older than supported |
| SYNC-3 | [#550](https://github.com/infiquetra/talaria/issues/550) | enhancement | One command moves the Hermes pin |
| SYNC-4 | [#551](https://github.com/infiquetra/talaria/issues/551) | enhancement | Daily watcher pushes a branch and opens an issue when Hermes publishes a release |
| SYNC-5 | [#552](https://github.com/infiquetra/talaria/issues/552) | context-update | Record the rule for merging and releasing a Hermes sync |
| SYNC-ACC | [#553](https://github.com/infiquetra/talaria/issues/553) | enhancement | Proof: the watcher run by hand opens the issue it should, with a branch when the pin move passes |
| PROOF | [#522](https://github.com/infiquetra/talaria/issues/522) | parent | Every v0.9.0 behaviour can be proved live |
| PROOF-1 | [#554](https://github.com/infiquetra/talaria/issues/554) | enhancement | Live tests run against any supported gateway and record its version |
| PROOF-2 | [#555](https://github.com/infiquetra/talaria/issues/555) | exploration | Runtime compatibility pass against a current Hermes gateway |
| PROOF-3 | [#556](https://github.com/infiquetra/talaria/issues/556) | enhancement | Recorded sudo and secret prompts, and a helper that replays them |
| PROOF-4 | [#557](https://github.com/infiquetra/talaria/issues/557) | enhancement | Release gate rows and the v0.9.0 acceptance record |
| PROOF-5 | [#558](https://github.com/infiquetra/talaria/issues/558) | exploration | The six later smoke suites without old Herdr |
| TRUTH | [#523](https://github.com/infiquetra/talaria/issues/523) | parent | Talaria launches everywhere and its records match the code |
| TRU-1 | [#559](https://github.com/infiquetra/talaria/issues/559) | defect | A Hermes skin with an empty colour stops Talaria at connect |
| TRU-2 | [#560](https://github.com/infiquetra/talaria/issues/560) | defect | A foreground alert that the shared gate drops still silences the next one |
| TRU-3 | [#561](https://github.com/infiquetra/talaria/issues/561) | enhancement | Local /clear starts a fresh session in the same folder and profile |
| TRU-4 | [#562](https://github.com/infiquetra/talaria/issues/562) | defect | Debug output of the recorder options and the redactor can print a single-use ticket |
| TRU-5 | [#563](https://github.com/infiquetra/talaria/issues/563) | enhancement | Workspace footer names the transcript and search keys |
| TRU-6 | [#564](https://github.com/infiquetra/talaria/issues/564) | context-update | Records pass: correct the documents that contradict the code |
| TRU-7 | [#565](https://github.com/infiquetra/talaria/issues/565) | enhancement | Help text for --resume |
| TRU-8 | [#566](https://github.com/infiquetra/talaria/issues/566) | enhancement | Theme colours on the default inline view |
| TRU-9 | [#567](https://github.com/infiquetra/talaria/issues/567) | enhancement | Move the snapshot-rebuild path out of app.rs into its own module |
| TRU-ACC | [#568](https://github.com/infiquetra/talaria/issues/568) | enhancement | Live proof: Talaria connects with an empty skin colour, /clear starts a fresh session, and the documentation tests pass |
| ACT | [#524](https://github.com/infiquetra/talaria/issues/524) | parent | Cards and commands do what they say |
| ACT-1 | [#569](https://github.com/infiquetra/talaria/issues/569) | defect | Fleet peek approval hint says Ctrl+C denies and stops, but it only closes or goes back |
| ACT-2 | [#570](https://github.com/infiquetra/talaria/issues/570) | enhancement | Regression test: an approval Hermes withdraws removes its card |
| ACT-3 | [#571](https://github.com/infiquetra/talaria/issues/571) | enhancement | /copy puts an assistant reply on the clipboard |
| ACT-5 | [#572](https://github.com/infiquetra/talaria/issues/572) | defect | A note typed on an approval is lost when Hermes refuses the follow-up prompt |
| ACT-ACC | [#573](https://github.com/infiquetra/talaria/issues/573) | enhancement | Live proof: the fleet peek's approval hint is true and a paste matches what /copy sent |
| CREW | [#525](https://github.com/infiquetra/talaria/issues/525) | parent | A crew can build through Talaria workflows |
| CREW-1 | [#574](https://github.com/infiquetra/talaria/issues/574) | exploration | How many agents can one workflow run at once on a current Hermes gateway? |
| CREW-2 | [#575](https://github.com/infiquetra/talaria/issues/575) | enhancement | Set the default number of agents a workflow runs at once to the measured number |
| CREW-3 | [#576](https://github.com/infiquetra/talaria/issues/576) | enhancement | Ultracode offers to install its skill into the profile |
| SIGNAL | [#526](https://github.com/infiquetra/talaria/issues/526) | parent | No Hermes signal that needs the operator is lost |
| SIG-1 | [#577](https://github.com/infiquetra/talaria/issues/577) | enhancement | Notice store: keep, replace, expire and clear Hermes notices |
| SIG-2 | [#578](https://github.com/infiquetra/talaria/issues/578) | enhancement | Show the current Hermes notice on the status row, with a count of warnings |
| SIG-3 | [#579](https://github.com/infiquetra/talaria/issues/579) | enhancement | Sign-in link and code stay readable and can be copied |
| SIG-4 | [#580](https://github.com/infiquetra/talaria/issues/580) | enhancement | Recover waiting sudo, secret and vault prompts after a reconnect |
| SIG-5 | [#581](https://github.com/infiquetra/talaria/issues/581) | enhancement | Answer a recovered prompt, and show the ones that cannot be answered |
| SIG-6 | [#582](https://github.com/infiquetra/talaria/issues/582) | context-update | Record a decision for every Hermes event Talaria ignores, and publish the signal reference |
| SIG-7 | [#583](https://github.com/infiquetra/talaria/issues/583) | enhancement | A connection request is shown as a waiting request and can be answered |
| SIG-8 | [#584](https://github.com/infiquetra/talaria/issues/584) | enhancement | Show the compaction, error and budget lines Hermes prints during a turn |
| SIG-9 | [#585](https://github.com/infiquetra/talaria/issues/585) | enhancement | Print a review summary into the conversation |
| SIG-ACC | [#586](https://github.com/infiquetra/talaria/issues/586) | enhancement | Live proof: a notice shows, and a sudo prompt comes back after a dropped connection |
| SHIP | [#527](https://github.com/infiquetra/talaria/issues/527) | parent | Talaria v0.9.0 is released and dogfooded |
| SHIP-1 | [#587](https://github.com/infiquetra/talaria/issues/587) | context-update | Record the operator's rulings of 2026-11-11 and what Talaria will not build |
| SHIP-2 | [#588](https://github.com/infiquetra/talaria/issues/588) | context-update | Dogfood protocol: how Talaria is used to build Talaria, and where findings go |
| SHIP-ACC | [#589](https://github.com/infiquetra/talaria/issues/589) | enhancement | Operator acceptance run for v0.9.0, including one issue built through a Talaria seat |
| REL-090 | [#590](https://github.com/infiquetra/talaria/issues/590) | enhancement | Release Talaria v0.9.0 |

## Talaria v0.9.1

| Key | Issue | Type | Title |
|---|---|---|---|
| FLEET | [#528](https://github.com/infiquetra/talaria/issues/528) | parent | The fleet view shows more and carries less |
| FLT-1 | [#591](https://github.com/infiquetra/talaria/issues/591) | enhancement | Sub-agent and background-process counts in the fleet peek |
| FLT-2 | [#592](https://github.com/infiquetra/talaria/issues/592) | enhancement | Remove old Herdr support from Talaria: the old state reporter and the Herdr-pane fleet backend |
| FLT-3 | [#593](https://github.com/infiquetra/talaria/issues/593) | enhancement | Two Talaria processes on one remote gateway stay signed in |
| FLOW | [#529](https://github.com/infiquetra/talaria/issues/529) | parent | Session work takes fewer steps |
| FLW-1 | [#594](https://github.com/infiquetra/talaria/issues/594) | enhancement | Fork from a chosen prompt in the rewind picker |
| FLW-2 | [#595](https://github.com/infiquetra/talaria/issues/595) | enhancement | A usage-limit card on a refused turn |
| FLW-3 | [#596](https://github.com/infiquetra/talaria/issues/596) | enhancement | A copy key in the transcript pager |
| FLW-4 | [#597](https://github.com/infiquetra/talaria/issues/597) | enhancement | A theme chosen with /theme use persists across launches |
| FIXES | [#530](https://github.com/infiquetra/talaria/issues/530) | parent | Known defects are fixed and three earlier operator requests are built |
| FIX-1 | [#598](https://github.com/infiquetra/talaria/issues/598) | defect | A grow larger than the scrollback leaves stale live-view rows on screen |
| FIX-2 | [#599](https://github.com/infiquetra/talaria/issues/599) | defect | A failed Ultracode command can replace a draft typed while it was running |
| FIX-3 | [#600](https://github.com/infiquetra/talaria/issues/600) | exploration | Decide and record what Talaria does with a PDF attachment |
| FIX-4 | [#601](https://github.com/infiquetra/talaria/issues/601) | defect | A failed skin restore can leave an unknown skin saved |
| FIX-5 | [#602](https://github.com/infiquetra/talaria/issues/602) | defect | A goal first seen already finished gives no completion notice |
| FIX-6 | [#603](https://github.com/infiquetra/talaria/issues/603) | defect | A partial reply is lost when a credential fails mid-stream |
| FIX-7 | [#604](https://github.com/infiquetra/talaria/issues/604) | enhancement | A status-line field for the agent's working folder |
| FIX-8 | [#605](https://github.com/infiquetra/talaria/issues/605) | enhancement | Say when a configuration change needs a restart |
| FIX-9 | [#606](https://github.com/infiquetra/talaria/issues/606) | enhancement | A command that scaffolds a theme file |
| DOGFOOD | [#531](https://github.com/infiquetra/talaria/issues/531) | parent | Every finding from dogfooding Talaria v0.9.0 has an issue and an outcome |
| STUDY-1 | [#607](https://github.com/infiquetra/talaria/issues/607) | exploration | Study: a viewer for the plans Hermes saves |
| STUDY-2 | [#608](https://github.com/infiquetra/talaria/issues/608) | exploration | Study: full-duplex voice with a local audio helper on stock Hermes |
| ACT-4 | [#609](https://github.com/infiquetra/talaria/issues/609) | exploration | Study: a correction row on the approval card |
| REL-091 | [#610](https://github.com/infiquetra/talaria/issues/610) | enhancement | Release Talaria v0.9.1 |

## Talaria v1.0.0

| Key | Issue | Type | Title |
|---|---|---|---|
| ONE-1 | [#611](https://github.com/infiquetra/talaria/issues/611) | exploration | What does Talaria 1.0 promise to keep stable? |
| ONE-2 | [#612](https://github.com/infiquetra/talaria/issues/612) | exploration | Security re-decisions: bearer values in recorded URL paths, and how the status command is stopped |
| ONE-3 | [#613](https://github.com/infiquetra/talaria/issues/613) | exploration | Version 2 of the status-command input, including cache-hit counts |
| ONE-4 | [#614](https://github.com/infiquetra/talaria/issues/614) | context-update | Give every deferred inventory item a recorded outcome and rewrite the queue |
| ONE-5 | [#615](https://github.com/infiquetra/talaria/issues/615) | exploration | Platform proof: an interactive Linux run, Zellij, and macOS notarization |
| ONE-6 | [#616](https://github.com/infiquetra/talaria/issues/616) | enhancement | Run the live tests and the smoke harness on a schedule |
| REL-100 | [#617](https://github.com/infiquetra/talaria/issues/617) | enhancement | Release Talaria v1.0.0 |
