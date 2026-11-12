# CREW-2: Raise the default number of agents a workflow runs at once, and make the staffing guide match

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for the measured width and the three files it may change. This page maps the constant and the two sentences that disagree today. It does not restate the requirements.

## What exists today

`pub const DEFAULT_MAX_IN_FLIGHT: u32 = 1` is in `crates/talaria-workflow/src/staffing/table.rs:8`. `StaffingTable::parse` starts `limits.max_in_flight` at that constant (`table.rs:175-180`). `StaffingTable::load` parses an empty string when the file is missing (`table.rs:317`). `StaffingTable::TEMPLATE` has the active line `max_in_flight = 1` and the comment `default 1` (`table.rs:131-132`). The commented provider example stays `max_in_flight = 4`.

`docs/workflows/staffing.md:5-6` says a missing file means ten simultaneous agents. The sample sets `max_in_flight = 10` (`staffing.md:17-18`) and the provider sample sets 4 (`staffing.md:19-20`).

`parses_the_documented_example` asserts the constant is 1 and the parsed sample is 10 (`crates/talaria-workflow/tests/staffing_table.rs:34` and `:40`). `absent_file_is_an_empty_table` and `template_parses` compare the loaded limit to the constant (`staffing_table.rs:127` and `:157`). `refuses_unknown_and_invalid_keys` rejects `max_in_flight=0` and `65` (`staffing_table.rs:77-84`).

`talaria workflow roster` prints `Limits: N agents at once` from the loaded table (`crates/talaria-workflow/src/staffing/report.rs:359`). It loads `staffing.toml` from `TALARIA_CONFIG_DIR` or the Talaria home (`workflow_roster.rs:118-124`). A gateway must answer before that line is printed.

The scheduler reads `cfg.limits.max_in_flight` (`crates/talaria-workflow/src/schedule/scheduler.rs:105`).

## The entry this unit reads

[#574](https://github.com/infiquetra/talaria/issues/574) (`CREW-1`) writes a dated entry titled `Workflow width default` in `docs/engineering-journal/DECISIONS.md`. The measured value in that entry is 1, 5, or 10. This unit does not edit the journal. The 2026-10-28 entry that proposes `max_in_flight` of 1 stays where it is.

## Edge cases

| Measured value | Constant and template | Missing-file sentence | Sample `max_in_flight = 10` |
| --- | --- | --- | --- |
| 1 | stay at 1 | one simultaneous agent | stays |
| 5 | become 5 | five simultaneous agents | stays |
| 10 | become 10 | ten simultaneous agents | stays |
