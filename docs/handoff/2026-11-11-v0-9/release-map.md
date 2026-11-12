# Talaria release map

Four releases, each a board Objective. Under each: parent issues (an outcome with an end-to-end proof) and their sub-issues, each sized to one pull request. Keys such as `SEAT-2` are planning names; the issue numbers are in the index beside this file.

Gates: a unit starts only after the units it waits on have merged.

## Talaria v0.8.1: "Herdr seat"

A patch release. Talaria reports its state to the newer Herdr and can be driven from outside, so the operator and agent crews can use it as a seat.

11 issues.

| Key | Type | Size | What it delivers | Waits on |
|---|---|---|---|---|
| SEAT | parent |  | Talaria is a working Herdr seat: it reports its state and can be driven from outside |  |
| SEAT-0 | enhancement | S | One pre-push script that runs what continuous integration runs on a pull request |  |
| SEAT-1 | enhancement | S | Seat state in the domain core: idle, working, blocked with a kind, done and error |  |
| SEAT-2 | enhancement | M | Terminal module asks whether the terminal reads status records and writes one per state change | SEAT-1 |
| SEAT-3 | enhancement | S | Report the seat state to the terminal, behind the existing report_state setting | SEAT-1, SEAT-2 |
| SEAT-4A | enhancement | M | Smoke harness: an hc-backed terminal beside the old Herdr one |  |
| SEAT-4B | enhancement | M | Smoke harness: choose the terminal, and run the first suite on hc | SEAT-4A, SEAT-3 |
| SEAT-4C | enhancement | S | Smoke harness: hc is chosen automatically and the first release gate runs on it | SEAT-4B, SEAT-3 |
| SEAT-5 | context-update | S | Guide: running Talaria as a Herdr seat | SEAT-3 |
| SEAT-ACC | enhancement | S | Live proof: a lead sends a prompt to a Talaria seat, waits for it and reads its state | SEAT-3, SEAT-4C |
| REL-081 | enhancement | S | Release Talaria v0.8.1 | SEAT-1, SEAT-2, SEAT-3, SEAT-4A, SEAT-4B, SEAT-4C, SEAT-5, SEAT-ACC |

## Talaria v0.9.0: "Signals you can trust, on the latest Hermes"

Talaria moves to the latest Hermes release and stays in step with it, no Hermes signal that needs the operator is lost, and cards and commands do what they say.

57 issues.

| Key | Type | Size | What it delivers | Waits on |
|---|---|---|---|---|
| PIN | parent |  | Talaria runs on the latest Hermes release |  |
| PIN-1 | enhancement | S | Contract generator accepts the Hermes v0.23 contracts |  |
| PIN-2 | enhancement | M | Tests stop hard-coding which Hermes release is pinned | PIN-1 |
| PIN-3 | enhancement | M | Move the Hermes pin to Hermes v0.23.1 | PIN-1, PIN-2, PIN-4 |
| PIN-4 | enhancement | S | Smoke example screens take the built-for Hermes release from the pin |  |
| PIN-5 | enhancement | S | Regression tests for print-mode cancel now that Hermes interrupts a started prompt | PIN-3 |
| PIN-ACC | enhancement | S | Live proof: the suite, the pinned-drift gate and the live tests pass at the new pin | PIN-5, PROOF-1, PROOF-4 |
| SYNC | parent |  | Talaria stays in step with Hermes |  |
| SYNC-1 | enhancement | S | Compatibility table: which Talaria release matches which Hermes releases | PIN-2, PIN-3, PROOF-1 |
| SYNC-2 | enhancement | M | Tell the operator once when the gateway is newer than tested or older than supported | SYNC-1 |
| SYNC-3 | enhancement | S | One command moves the Hermes pin | SYNC-1, PIN-3 |
| SYNC-4 | enhancement | M | Daily watcher pushes a branch and opens an issue when Hermes publishes a release | SYNC-3 |
| SYNC-5 | context-update | S | Record the rule for merging and releasing a Hermes sync | SYNC-4 |
| SYNC-ACC | enhancement | S | Proof: the watcher run by hand opens the issue it should, with a branch when the pin move passes | SYNC-4, PROOF-4 |
| PROOF | parent |  | Every v0.9.0 behaviour can be proved live |  |
| PROOF-1 | enhancement | M | Live tests run against any supported gateway and record its version |  |
| PROOF-2 | exploration | S | Runtime compatibility pass against a current Hermes gateway | PROOF-1 |
| PROOF-3 | enhancement | M | Recorded sudo and secret prompts, and a helper that replays them | PROOF-1 |
| PROOF-4 | enhancement | S | Release gate rows and the v0.9.0 acceptance record | PROOF-3 |
| PROOF-5 | exploration | M | The six later smoke suites without old Herdr | SEAT-4C |
| TRUTH | parent |  | Talaria launches everywhere and its records match the code |  |
| TRU-1 | defect | S | A Hermes skin with an empty colour stops Talaria at connect |  |
| TRU-2 | defect | S | A foreground alert that the shared gate drops still silences the next one | SEAT-1, SEAT-3 |
| TRU-3 | enhancement | S | Local /clear starts a fresh session in the same folder and profile |  |
| TRU-4 | defect | S | Debug output of the recorder options and the redactor can print a single-use ticket |  |
| TRU-5 | enhancement | S | Workspace footer names the transcript and search keys |  |
| TRU-6 | context-update | M | Records pass: correct the documents that contradict the code |  |
| TRU-7 | enhancement | S | Help text for --resume |  |
| TRU-8 | enhancement | M | Theme colours on the default inline view |  |
| TRU-9 | enhancement | M | Move the snapshot-rebuild path out of app.rs into its own module |  |
| TRU-ACC | enhancement | S | Live proof: Talaria connects with an empty skin colour, /clear starts a fresh session, and the documentation tests pass | TRU-1, TRU-3, TRU-6, PROOF-4 |
| ACT | parent |  | Cards and commands do what they say |  |
| ACT-1 | defect | S | Fleet peek approval hint says Ctrl+C denies and stops, but it only closes or goes back |  |
| ACT-2 | enhancement | S | Regression test: an approval Hermes withdraws removes its card |  |
| ACT-3 | enhancement | M | /copy puts an assistant reply on the clipboard |  |
| ACT-5 | defect | S | A note typed on an approval is lost when Hermes refuses the follow-up prompt |  |
| ACT-ACC | enhancement | S | Live proof: the fleet peek's approval hint is true and a paste matches what /copy sent | ACT-1, ACT-3, PROOF-4 |
| CREW | parent |  | A crew can build through Talaria workflows |  |
| CREW-1 | exploration | M | How many agents can one workflow run at once on a current Hermes gateway? | PROOF-1 |
| CREW-2 | enhancement | S | Set the default number of agents a workflow runs at once to the measured number | CREW-1 |
| CREW-3 | enhancement | S | Ultracode offers to install its skill into the profile |  |
| SIGNAL | parent |  | No Hermes signal that needs the operator is lost |  |
| SIG-1 | enhancement | M | Notice store: keep, replace, expire and clear Hermes notices |  |
| SIG-2 | enhancement | M | Show the current Hermes notice on the status row, with a count of warnings | SIG-1, TRU-2 |
| SIG-3 | enhancement | S | Sign-in link and code stay readable and can be copied | SIG-2, ACT-3 |
| SIG-4 | enhancement | M | Recover waiting sudo, secret and vault prompts after a reconnect | PIN-3, TRU-9 |
| SIG-5 | enhancement | M | Answer a recovered prompt, and show the ones that cannot be answered | SIG-4, PROOF-3 |
| SIG-6 | context-update | S | Record a decision for every Hermes event Talaria ignores, and publish the signal reference |  |
| SIG-7 | enhancement | M | A connection request is shown as a waiting request and can be answered | SEAT-1, SIG-4 |
| SIG-8 | enhancement | S | Show the compaction, error and budget lines Hermes prints during a turn | SIG-1 |
| SIG-9 | enhancement | S | Print a review summary into the conversation | SIG-1 |
| SIG-ACC | enhancement | S | Live proof: a notice shows, and a sudo prompt comes back after a dropped connection | SIG-3, SIG-5, PROOF-3, PROOF-4 |
| SHIP | parent |  | Talaria v0.9.0 is released and dogfooded |  |
| SHIP-1 | context-update | S | Record the operator's rulings of 2026-11-11 and what Talaria will not build |  |
| SHIP-2 | context-update | S | Dogfood protocol: how Talaria is used to build Talaria, and where findings go |  |
| SHIP-ACC | enhancement | M | Operator acceptance run for v0.9.0, including one issue built through a Talaria seat | PIN-ACC, SYNC-ACC, SIG-ACC, ACT-ACC, TRU-ACC, CREW-2, PROOF-4 |
| REL-090 | enhancement | S | Release Talaria v0.9.0 | SHIP-ACC |

## Talaria v0.9.1: "Follow-through"

The follow-through: the fleet view, fewer steps in session work, known defects, and whatever dogfooding v0.9.0 turned up.

24 issues.

| Key | Type | Size | What it delivers | Waits on |
|---|---|---|---|---|
| FLEET | parent |  | The fleet view shows more and carries less |  |
| FLT-1 | enhancement | S | Sub-agent and background-process counts in the fleet peek | PIN-3 |
| FLT-2 | enhancement | M | Remove old Herdr support from Talaria: the old state reporter and the Herdr-pane fleet backend | SEAT-3 |
| FLT-3 | enhancement | M | Two Talaria processes on one remote gateway stay signed in |  |
| FLOW | parent |  | Session work takes fewer steps |  |
| FLW-1 | enhancement | M | Fork from a chosen prompt in the rewind picker | TRU-3, PIN-3 |
| FLW-2 | enhancement | M | A usage-limit card on a refused turn | PIN-3 |
| FLW-3 | enhancement | S | A copy key in the transcript pager | ACT-3 |
| FLW-4 | enhancement | S | A theme chosen with /theme use persists across launches | FIX-9 |
| FIXES | parent |  | Known defects are fixed and three earlier operator requests are built |  |
| FIX-1 | defect | S | A grow larger than the scrollback leaves stale live-view rows on screen |  |
| FIX-2 | defect | S | A failed Ultracode command can replace a draft typed while it was running | CREW-3 |
| FIX-3 | exploration | S | Decide and record what Talaria does with a PDF attachment |  |
| FIX-4 | defect | S | A failed skin restore can leave an unknown skin saved |  |
| FIX-5 | defect | S | A goal first seen already finished gives no completion notice |  |
| FIX-6 | defect | S | A partial reply is lost when a credential fails mid-stream |  |
| FIX-7 | enhancement | S | A status-line field for the agent's working folder |  |
| FIX-8 | enhancement | S | Say when a configuration change needs a restart |  |
| FIX-9 | enhancement | S | A command that scaffolds a theme file |  |
| DOGFOOD | parent |  | Every finding from dogfooding Talaria v0.9.0 has an issue and an outcome | SHIP-2 |
| STUDY-1 | exploration | M | Study: a viewer for the plans Hermes saves |  |
| STUDY-2 | exploration | M | Study: full-duplex voice with a local audio helper on stock Hermes |  |
| ACT-4 | exploration | S | Study: a correction row on the approval card |  |
| REL-091 | enhancement | S | Release Talaria v0.9.1 | REL-090 |

## Talaria v1.0.0: "Stable promises"

Deliberately thin. Its building work depends on what v0.9.0 and v0.9.1 show; most of it is studies that decide what 1.0 promises.

7 issues.

| Key | Type | Size | What it delivers | Waits on |
|---|---|---|---|---|
| ONE-1 | exploration | M | What does Talaria 1.0 promise to keep stable? |  |
| ONE-2 | exploration | S | Security re-decisions: bearer values in recorded URL paths, and how the status command is stopped |  |
| ONE-3 | exploration | S | Version 2 of the status-command input, including cache-hit counts | REL-090 |
| ONE-4 | context-update | M | Give every deferred inventory item a recorded outcome and rewrite the queue |  |
| ONE-5 | exploration | S | Platform proof: an interactive Linux run, Zellij, and macOS notarization |  |
| ONE-6 | enhancement | M | Run the live tests and the smoke harness on a schedule | SEAT-4C, PROOF-1 |
| REL-100 | enhancement | S | Release Talaria v1.0.0 | ONE-1 |

Every building issue also waits on SEAT-0 (the one pre-push check script), which is left out of the last column.
