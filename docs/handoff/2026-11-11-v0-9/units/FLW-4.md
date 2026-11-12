# FLW-4: A theme chosen with /theme use persists across launches

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the theme command and the config reader that exist today. The requirements, the tests and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`ThemeFileSession::command` (`crates/talaria-app/src/appearance.rs:722`) on `["use", id]` sets `session_file` when `load_selection` succeeds (`appearance.rs:726-729`). `["use"]` sets `session_file` to `None` and calls `refresh` (`appearance.rs:734-736`). Neither arm writes `config_path` (`appearance.rs:665`). `selected_id` (`appearance.rs:683-688`) is `session_file`, else `theme.profiles` for this profile, else `theme.file`. A new session starts with `session_file: None` (`appearance.rs:678`). `ThemeFileSession::new` (`appearance.rs:673`) reads no file. Launch passes `config.theme.clone()` at `run.rs:5120` after `config::parse` (`config.rs:338`). `selection_never_writes_config_toml` is `theme_builtin_select.rs:199`. `fixture` writes theme ids `global`, `profile`, and `session` (`theme_command.rs:20`).

`/theme light`, `/theme dark`, and `/theme auto` are `AppearanceService::theme_command` (`appearance.rs:207-226`). They do not call `ThemeFileSession::command`. The app routes `use` and `reload` to `AppEffect::ThemeFileCommand` (`app.rs:8665-8666`).

`ThemeConfig` holds `file` and `profiles` (`crates/talaria-app/src/config.rs:222-224`). The loader reads `[theme] file` and `[theme.profiles.<profile>] file` (`config.rs:667-686`). `theme.name` is refused (`config.rs:697-699`). The loader does not write.

`toml` is `=1.1.8+spec-1.1.0` in `[workspace.dependencies]` (`Cargo.toml:76`). `Cargo.lock` has one `toml_edit`, version `0.25.17+spec-1.1.0`. The `toml` package entry does not list it. `proc-macro-crate` does. No crate in this workspace names `toml_edit` as a direct dependency. `talaria-app` depends on `toml` (`Cargo.lock` package `talaria-app`, and `toml.workspace = true` at `crates/talaria-app/Cargo.toml:33`) and does not depend on `toml_edit`. REQ-1 adds that edge to the lock.

`docs/themes.md:41-43` says `/theme use` clears the session choice and that these commands never write configuration or Hermes files. `docs/configuration.md:428` ends with `None writes configuration.` REQ-5 replaces that prose sentence and does not edit the sample.

`use_leaves_config_unchanged` (`crates/talaria-app/tests/theme_command.rs:70`) asserts `/theme use session` leaves the file bytes unchanged. `fixture` (`theme_command.rs:16`) writes a temp `config.toml` and builds a `ThemeFileSession` for profile `fixture`.

## What this page does not restate

REQ-3 names the two parse notices. REQ-5 names the replacement paragraph. This page does not copy them.
