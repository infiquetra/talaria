# CREW-3: Ultracode offers to install its skill into the profile

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the call sites and the quotes. The requirements, tests, and acceptance criteria are only in the issue. This page refers to them by id.

## Where the skill is judged

`SKILL_COMMAND` is `/talaria-workflows` (`crates/talaria-core/src/ultracode.rs:7`). `skill_availability` (`ultracode.rs:90-108`) returns `Installed`, `Missing`, or `Collision`. The caller turns a catalogue error into `Unknown` (`crates/talaria-app/src/ultracode.rs:93-96`).

`notice::missing` (`ultracode.rs:116-119`) is the hand command. It does not ask:

```116:119:crates/talaria-core/src/ultracode.rs
pub fn missing(profile: &str) -> String {
    format!(
        "Ultracode needs the talaria-workflows skill in profile {profile}. Ultracode runs substantive prompts as Talaria workflows through that skill. Install it with: talaria workflow skill install --profile {profile}"
    )
}
```

REQ-1 adds `notice::offer` beside it and does not edit this function. `notice::unknown` is at `ultracode.rs:121-124`, `notice::unavailable` at `126-129`, and `notice::collision` at `131-134`. The collision text contains `Rename that command`. The on text is the `ON` constant at `ultracode.rs:111`, which starts `Ultracode on for this session`.

## Where the notice is chosen

`UltracodeService::new` (`crates/talaria-app/src/ultracode.rs:47-52`) takes only `default_on`. Call sites that must stay: `UltracodeService::default` (`app.rs:1185`), `new(true)` inside the existing app test (`app.rs:10321`), and `new(default_ultracode)` (`app.rs:14681`). REQ-3 adds `set_remote` after that last call. It does not change `new`.

`on_catalog_reply` (`ultracode.rs:85-114`) returns the status notice immediately for `UltracodeCommand::Status` (`97-103`). `Installed` turns the session on and returns `notice::ON` plus `Changed` (`105-107`). `Missing` is `notice::missing` at line 109. `Collision` is line 110. `Unknown` is line 111. REQ-1 changes the missing arm only.

`on_command_outcome` (`ultracode.rs:135-188`) maps `unsupported_command` to `notice::missing` (`163-166`) and `command_unavailable` to `notice::unavailable` (`167-170`). The function always returns the notice plus `RestoreComposer` (`184-187`). It stays `&self`, so it cannot store the ask. The foreground arm calls `record_shown` only when the shown notice equals `notice::offer` for that profile. The background arm is unchanged and does not call `on_command_outcome`. An unbound session uses the profile name `default` (`141`). `record_shown` and `show_kept` use that same profile. Tests 2 and 4 bind `fixture` first so the profile is not that fallback.

`Action` (`ultracode.rs:13-32`) has no install variant. REQ-2 and REQ-4 add `InstallSkill`. `answer_install`, `record_shown(&mut self, session: &SessionId)`, and `show_kept(&mut self, session: &SessionId) -> Option<String>` are new on this service. `record_shown` sets the asked flag from that profile and clears a kept offer. `show_kept` does that when an offer is still kept, and returns the offer text or `None`. Installed, Collision, or Unknown drops a kept offer. No other path drops it.

`apply_ultracode` (`app.rs:7076-7121`) turns `Notice` into `notice_lines` (`7085`). `notice_lines` (`app.rs:2345-2347`) prefixes each line with `· `. `RunCommand` and `FetchCatalog` become `AppEffect::Ultracode` (`7099-7117`). The `RunCommand` arm calls `clear_submitted_draft` when `clear_composer` is true (`7100-7102`). The `InstallSkill` arm does not. The match has no wildcard, so `InstallSkill` needs its own arm (REQ-4).

The composer returns `ComposerOutcome::Unchanged` for a blank draft (`composer/mod.rs:566-567`) and emits no `Submit`, so Enter on a blank line never reaches the app. `submit` (`app.rs:8389`) returns immediately when `text.trim()` is empty (`8390-8392`). A later missing result does not clear the pending question. A successful install does not mark the session on. The opened-session `intercept_submit` is at `8485-8487`. Slash, shell, and local commands return before that. `UserAction::Submit` clears the composer before calling `submit` (`app.rs:6705-6730`), so a consumed answer must not load the draft back.

The held-prompt loop is in `opened_session` (`app.rs:9167`). It calls `intercept_submit` at `9404` and then `apply_ultracode(state, actions, false)` (`9412`). REQ-2 inserts `answer_install` at the empty return, at `submit`'s `intercept_submit`, and at this held call.

The catalogue result is `AppMessage::UltracodeCatalog` (`app.rs:4185-4202`), which calls `on_catalog_reply`. `UltracodeDone` (`4204-4218`) calls `on_command_outcome` only in the foreground arm. The effect arm is `AppEffect::Ultracode` (`app.rs:12813-12857`). `FetchCatalog` and `RunCommand` are handled. Every other action hits `_ => {}` at `12857`. REQ-4 must not leave `InstallSkill` on that arm. The result message `UltracodeNotice` is new. Its match arm is only `notice_lines`. The catalogue handler shows a reply only when the session is in front (`app.rs:4198-4202`). It calls `record_shown` only when that shown notice equals `notice::offer` for the profile. A `Status` reply does not. A reply that is not in front records nothing. `opened_session` (`app.rs:9167`) calls `show_kept` once at `app.rs:9383` and shows the text. `install_session` (`app.rs:7948`) and `AppMessage::Opened` (`app.rs:4050`) reach that call. The service test calls `show_kept` because `opened_session` is private. REQ-4 does not drop the install result.

## What local means

`--remote` is `Cli.remote: Option<String>` (`crates/talaria-cli/src/args.rs:59`). `main` returns through `remote::attach` when it is set (`main.rs:71-72`) and does not call `run_with_ultracode`. `attach` (`remote.rs:348`) still starts Ultracode: it keeps `cli.ultracode` (`398`) and calls `run_remote` (`400`).

`run_with_ultracode` (`run.rs:4938-4939`) calls `run_selected` with no remote option. `run_remote` (`4942-4948`) passes one. `run_selected` sets `remote_gateway: remote.is_some()` (`run.rs:5007`). `run_inner` copies that onto the app (`app.rs:14678-14680`) by `attached_to_remote_gateway` (`app.rs:1494-1495`), then builds the service (`14681`). The only production call of `attached_to_remote_gateway` is that one. Two voice tests call it directly. REQ-3 uses this flag and adds no detector inside `skill::install` or `attach`.

## The installer the yes runs

`skill::install` is `crates/talaria-workflow/src/skill/install.rs:132`. `SKILL_NAME` is `talaria-workflows` and `SKILL_VERSION` is `1.0.0` (`skill/mod.rs:11-12`). A first install's text is built in `report_text` (`crates/talaria-cli/src/workflow_skill.rs:185-188`): `installed {SKILL_NAME} {SKILL_VERSION} into profile {profile}`. That is the line UAT-1 looks for. `report_text` (`102-107`) takes the target, the report, `dry_run`, and `uninstall`, and ends with a newline (`194`).

`resolve_target` (`workflow_skill.rs:52`) is `pub async fn resolve_target(profile: &str) -> Result<(SkillTarget, Vec<String>), SkillError>`. Its body stays. When no gateway is running it returns the local profile folder and the warning `gateway not running; using the local profile folder` (`94-97`). `execute` (`196`) awaits `resolve_target` at `223`, builds `InstallOptions` (`226-233`) with `lock_wait: Duration::from_secs(10)`, runs `skill::install` in `spawn_blocking` (`254-259`), splices the warnings (`264`), and prints `report_text` (`281`). Each printed line then goes through `args::neutralise` (`args.rs:182`), which is `pub(crate)` and stays in the command crate. REQ-4 shows `report_text` in the session and does not call `neutralise`. `InlineTerminal::commit_lines` runs `sanitize_line` (`terminal.rs:882`), which is `talaria_core::text::sanitize`. That path removes control characters from each span. The session path adds no sanitising of its own. An escape character (U+001B) is not left in the committed span. `InstallOptions.now` is `chrono::DateTime<chrono::Utc>` (`install.rs:105`). `talaria-app` does not depend on `chrono` today, so REQ-4 adds `chrono.workspace = true`. The workspace declares it at `Cargo.toml:84`. A normal install has no control characters, so the words match.

`talaria-app` depends on `talaria-workflow` and not on `talaria-cli`. `talaria-workflow` already depends on `hermes-client` and `tokio`. `LinkWireFactory` already lives in `talaria-workflow`'s hermes module, which is what `resolve_target` uses. Moving the two functions does not add a dependency. The only callers today are inside `execute`: `resolve_target` at `223` and `report_text` at `281`. `report_json` (`99`) stays next to the command.

`SkillError::Template` displays its string (`skill/mod.rs:367`). The command's missing-home error is `SkillError::Template("no home folder for Talaria".into())` (`workflow_skill.rs:225`).

## Tests that already exist

`missing_skill_notice_explains_why_and_preserves_profile_install_command` (`crates/talaria-core/tests/ultracode_notices.rs:15`) asserts the hand command for profile `fixture` and one other profile. REQ-1 leaves it green by not editing `notice::missing`. The other profile name is not part of this issue. Do not copy it into new text.

`on_checks_a_fresh_catalogue` (`crates/talaria-app/tests/ultracode_app.rs:51`) asserts that a missing catalogue and the collision catalogue each produce some `Action::Notice`, and that `Err("offline")` contains `offline`. Helpers in that file: `session` (`10`), `catalog` (`13`), `parsed` (`16`), `foreground` (`20`, profile `fixture`), `rpc` (`26`), `on` (`35`). Test 2's collision value is the second catalogue in the loop at `69-72`. `InstallSkill` is still a `Notice`-free action, so this existing test stays green if the missing reply remains a `Notice`.

`crates/talaria-workflow/tests/skill_install.rs` covers `skill::install` in a temporary directory, including `install_into_missing_writes_files_and_record` (`49`) and `dry_run_creates_nothing` (`468`). REQ-4 does not change that file. The service tests assert `Action::InstallSkill` and do not call `skill::install`.

## The sentence REQ-5 replaces

`docs/workflows/ultracode.md:53` is `A missing skill prints its install command.` The manual install block is lines 9-42. Line 6 says a previous yes never authorizes another run. That yes is the workflow run, not this install. REQ-5 does not edit it.
