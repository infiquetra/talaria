# SEAT-4C: Smoke harness: `hc` is chosen automatically and the first release gate runs on it

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the call sites and the quotes. The requirements, tests, and acceptance criteria are only in the issue. This page refers to them by id.

## What refuses a run today

`run` (`tools/smoke/src/main.rs:94`) is the preflight. A `phase4a`, `phase4b` or `phase5` run checks `jq` first (`main.rs:96-110`). The Herdr refusal is later:

```30:31:tools/smoke/src/main.rs
const NOT_IN_HERDR: &str =
    "This test must run inside a Herdr pane (HERDR_ENV=1). Start it from a Herdr tab.";
```

```126:132:tools/smoke/src/main.rs
    if std::env::var("HERDR_ENV").as_deref() != Ok("1") {
        return Err(NOT_IN_HERDR.to_owned());
    }
    let workspace = std::env::var("HERDR_WORKSPACE_ID").unwrap_or_default();
    if workspace.is_empty() {
        return Err(NOT_IN_HERDR.to_owned());
    }
```

`main` prints that string and exits 2 (`main.rs:85-87`). The default suite is `phase1` (`main.rs:67`). There is no `HC_TERMINAL_ID` read anywhere under `tools/smoke`. `bare_cmd` (`tests/support/mod.rs:1735`) starts from `env_clear`, then sets `HERDR_ENV=1` and a workspace id (`mod.rs:1743-1744`). `outside_herdr` (`mod.rs:1788`) removes only those two variables.

`cli.rs:134` asserts the old sentence. `cli5.rs:150` runs `phase5` with `jq` on `PATH`, removes `HERDR_ENV`, and leaves the workspace id set. It asserts the output contains `Herdr pane`. REQ-1 tightens that assertion. The both-ways line also contains those two words, so the old assertion would not fail first.

[#537](https://github.com/infiquetra/talaria/issues/537) (`SEAT-4B`) reads `TALARIA_SMOKE_TERMINAL` before the block at lines 126-132 and leaves an unset variable as `herdr`. Re-verify the line numbers after that issue merges. REQ-1 replaces that block and that unset rule. It does not move the `jq` check.

## Why the other six suites stop

`send_text` does not press Enter. `phase2a` types `Summarize @calc` (`steps2a.rs:84`) by splitting off the last character and calling `typed` (`steps2a.rs:925-927`). `typed` calls `send_text` (`steps2a.rs:544-545`). `phase4a` types `api_key` (`steps4a.rs:201`) and `Bad Name!` (`steps4a.rs:518`) through `text` (`steps4a.rs:34-38`), which also calls `send_text`. The measured `hc` is 0.1.20. It cannot press `!` or `@` or `_`, and it cannot type text without Enter. `phase2b`, `phase3`, `phase4b` and `phase5` were not proved clean.

REQ-2 returns the suite line before `HerdrCli::new` (`main.rs:215`) and before any suite function. On the `phase1` plus `hc` path, `HcCli::check(&self) -> Result<(), TermError>` replaces `HerdrCli::check` (`main.rs:216`). [#536](https://github.com/infiquetra/talaria/issues/536) (`SEAT-4A`) provides that function. It runs `ls`. The checks at `main.rs:133-214` stay where they are. A fake command with no reply prints nothing and exits 0, so `check` succeeds and `create_tab` then runs `run`, not `tab`. This issue does not edit `steps2a.rs` or `steps4a.rs`. It does not invent a second terminal name. [#536](https://github.com/infiquetra/talaria/issues/536) (`SEAT-4A`) owns every `Terminal` method. [#558](https://github.com/infiquetra/talaria/issues/558) (`PROOF-5`) owns the question of how the six suites run later.

## The gate rows

`tools/release/gates.tsv` lines 19-25 are `smoke-phase1` through `smoke-phase5`. Each command is the same except `--suite`. The shell it writes exports `HERMES_HOME` when the child has none and the outer value is non-empty, then runs the binary with `"$@"`. It does not use `exec`. None of the seven commands sets `HERDR_ENV`, `HERDR_WORKSPACE_ID` or `TALARIA_SMOKE_TERMINAL`.

REQ-3 inserts `TALARIA_SMOKE_TERMINAL=hc` immediately before `TALARIA_LIVE_REQUIRE=1` on line 19 only. Lines 20-25 stay byte for byte. The README sentences are not edited.

## What stays on the trait

`Terminal::foreground` is `tools/smoke/src/terminal.rs:65`, and `HcCli::foreground` is `terminal.rs:226`. `pane_get` is `terminal.rs:68` and `terminal.rs:246`. `create_tab` is `terminal.rs:52` and `terminal.rs:162`. `run` is `terminal.rs:55` and `terminal.rs:197`. REQ-4 does not edit those methods and does not add a call to `foreground` or `pane_get`. `phase1` reaches `create_tab` and `run` only through the path [#537](https://github.com/infiquetra/talaria/issues/537) (`SEAT-4B`) already built. This issue starts no second program from `run` (`main.rs:94`).
