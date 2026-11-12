# PIN-4: Smoke example screens take the built-for line from the pin

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`tools/smoke/src/steps4a.rs:587-598` reads `crates/hermes-contract/PINNED` and fails the doctor-overlay step unless the screen contains `built for: {tag} ({first 8 of commit})`. The label is `DOCTOR_BUILT_FOR` in `tools/smoke/src/wording4a.rs:64`, the string `built for: `.

The three screens are `docs/fixtures/smoke/phase4a/doctor-overlay-r1.txt` (the phrase at lines 3 and 7), `doctor-cli-r1.txt` (lines 3, 13 and 15) and `no-gateway-r1.txt` (line 3). Each quotes `v0.22.4` and `4d043038`. `doctor-overlay-r1.txt` also has `running: 0.22.4 at 4d043038` at line 6 and the release sentence at line 5. `doctor-cli-r1.txt` line 13 holds `hermes_tag` `v0.22.4` and `hermes_commit` `4d04303818b61bf3aa09278af48356315515965b`. REQ-1 turns every one of those spellings into `PIN_TAG`, `PIN_VERSION`, `PIN_COMMIT8`, or `PIN_COMMIT`. `Talaria 0.7.0-dev` stays. The checker only requires the prefix `running: `.

`fixture` in `tools/smoke/tests/support/scripts4a.rs:16` loads those three names from the docs path and then applies fault replacements. `screen` in `tools/smoke/tests/screen4a.rs:4` loads `doctor-overlay-r1.txt` from the same docs path. `doctor_rows_counted` (`screen4a.rs:128`) expects 7 rows. The overlay checker also expects 7 (`steps4a.rs:573-575`).

The folder README's heading is `# Recorded doctor output`. Its two prose lines say the recordings retain the captured version and that the tests use them to tell historical evidence from current source. REQ-1 replaces those two lines with the two sentences in the issue, naming the four tokens, and keeps the heading. `fails` (`tools/smoke/tests/steps4a.rs:117`) takes `&str` find-strings, and `pair!` (`steps4a.rs:220`) evaluates them inside the generated test, so REQ-3's `built_for_find` returns `&'static str`.

`doctor_screens_have_pin_tokens` is in the same commit as the screens, the README, the two loaders and the plant. It reads the screens from `env!("CARGO_MANIFEST_DIR")` joined with `../../docs/fixtures/smoke/phase4a`, and `PINNED` with `include_str!("../../../crates/hermes-contract/PINNED")`. It fails when a raw screen contains the pinned tag, the version, or the 8-character commit prefix. `fixture` and `screen` expand `PIN_COMMIT8` before `PIN_COMMIT`, then `PIN_TAG` and `PIN_VERSION`, and use `include_str!` at the path for their own file. The plant is `tools/smoke/tests/steps4a.rs:755-759`. The source file ends at line 641.
