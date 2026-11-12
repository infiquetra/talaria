# FIX-9: A command that scaffolds a theme file

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

The issue states REQ-1 through REQ-5, the five new tests, and the stop conditions. This note maps those ids onto the current tree. It does not add a requirement and it does not restate a test. Line numbers are from Talaria `main` at the draft. REQ-5 says this issue states no Hermes fact, so there is no Hermes file to re-read.

## What `/theme` does today

`THEME_USAGE` is `Usage: /theme [light|dark|auto|use [<id>]|reload]` (`crates/talaria-app/src/appearance.rs:660`). The palette hint is a separate string, `args_hint` at `crates/talaria-core/src/commands/local.rs:164`, today `[light|dark|auto|use [<id>]|reload]`. The palette prints that field as `usage: /theme {args_hint}` (`crates/talaria-core/src/commands/palette.rs:519-520`). REQ-1 sets the hint to `[light|dark|auto|use [<id>]|new <id>|reload]`. No test asserts this hint. `find` (`local.rs:572`) returns `Option<&LocalCommand>`. The new test unwraps it, as `commands_local.rs` already does for `quit`, then compares `args_hint`. `ThemeFileSession::command` (`appearance.rs:722`) splits the argument on whitespace. `["use", id]` with `valid_id` loads that id and sets `session_file` (`appearance.rs:726-730`). `["use"]` clears the session choice (`appearance.rs:734-736`). `["reload"]` refreshes (`appearance.rs:738`). Anything else returns `THEME_USAGE` and `AppearanceChange::Unchanged` (`appearance.rs:739`). A successful `use` or `reload` ends with `Theme file: themes/<id>.toml.` (`appearance.rs:749-752`).

`theme_command` in `crates/talaria-app/src/app.rs:8661` sends a first word of `use` or `reload` to `AppEffect::ThemeFileCommand` (`app.rs:8665-8666`). Any other argument goes to `state.appearance.theme_command`, which is the light, dark, and auto polarity command (`app.rs:8668`, `appearance.rs:207`). REQ-1 adds `new` beside `use` and `reload` and nowhere else.

`selected_id` (`appearance.rs:683-688`) is the session choice, then the profile table, then `[theme] file`. REQ-2 does not set `session_file`, so this order is unchanged. `load_selection` (`appearance.rs:768`) serves a built-in from memory, rejects `dark` and `light` as bases, and otherwise calls `load`.

## The file the command writes

`valid_id` (`crates/talaria-app/src/theme_file/load.rs:6-14`) allows 1 to 64 bytes, starting with a lower-case letter or digit, then lower-case letters, digits, `_`, or `-`. `reserved_id` (`load.rs:15-17`) is `terminal`, `dark`, `light`, `ember`, and `paper`. `load` (`load.rs:19`) rejects an invalid id with `invalid theme identifier` (`load.rs:21`), then a reserved id with `themes/<id>.toml: reserved theme identifier` (`load.rs:25`), then opens `themes` and the file with `NOFOLLOW` (`load.rs:34-40`). A failed open of `themes` is `themes/<id>.toml: cannot open themes directory` (`load.rs:34-35`). A failed open of the file is `themes/<id>.toml: cannot open theme file` (`load.rs:36-42`). The parent directory is `config.parent()`, or `.` when that parent is missing (`load.rs:27-30`).

`parse` (`crates/talaria-app/src/theme_file/parse.rs:149`) requires `schema` to be the integer 1 (`parse.rs:173-176` and `parse.rs:271-273`). `extends` accepts `terminal`, `dark`, or `light` and defaults to terminal (`parse.rs:160`, `parse.rs:185-191`). `name` is an optional string (`parse.rs:179-184`). A file with only those three keys parses. An unknown key is rejected (`parse.rs:269`). REQ-2's bytes are those three lines and nothing else, so `load` succeeds and the parsed `name` is the id. `write_scaffold` does not call `load`. Test 1 does.

`write_scaffold` creates `themes/<id>.toml` with `OpenOptions` `write(true).create_new(true)`, the same call as `crates/talaria-app/src/models_file.rs:76` and `crates/talaria-app/src/profiles_editor.rs:37`. It writes REQ-2's bytes and flushes, as `profiles_editor.rs:53-55` does. If the write or the flush fails, it removes that file, ignores an error from the remove, and returns `themes/<id>.toml: cannot open theme file`, so the next call can try again. `models_file.rs:62-64` removes a temporary file on error. This command removes `themes/<id>.toml` itself. Nothing in the tree can fail that write without a new hook, and the issue adds no hook and no test for it.

`builtin` (`crates/talaria-app/src/theme_file/builtin.rs:5-12`) embeds `terminal`, `ember`, and `paper`. `selection_never_writes_config_toml` (`crates/talaria-app/tests/theme_builtin_select.rs:199-209`) asserts that `use` of those three does not create `themes/<id>.toml` and does not change `config.toml`. REQ-3 refuses those ids before creating a file, so that test stays true.

`fixture` in `theme_command.rs:16` writes `config.toml`, creates `themes`, and builds a session on profile `fixture`. The same helper in `theme_builtin_select.rs:18` does not pre-create `themes`. The new tests use the `theme_command.rs` helper, except the run that starts with no `themes` directory.

## Why the notice does not select

`/theme use <id>` is what sets `session_file` (`appearance.rs:728`). Writing `[theme] file` is how a file is selected at launch (`docs/themes.md:39-43`). The same paragraph says those selection commands never write configuration or Hermes files (`themes.md:43`). REQ-2 leaves both statements true: the new command writes only `themes/<id>.toml`, and the operator then runs `/theme use <id>`. `FLW-4` is the later persistence of that selection. This issue does not wait on it. `FLW-4` starts after this issue has merged, and that gate is start.

## Edges, by requirement

| Case | Result | Id |
| --- | --- | --- |
| `new quiet-amber`, `themes` present or missing | The three-line file, the wrote-notice, selection unchanged, `config.toml` unchanged | REQ-2 |
| `new`, `new <id> extra`, or an id that fails `valid_id` | `THEME_USAGE`, no file | REQ-1 |
| Reserved id, whether or not that path exists | `themes/<id>.toml: reserved theme identifier`, filesystem untouched | REQ-3 |
| File already exists as a regular file | That error, bytes unchanged. A link is not this row | REQ-3 |
| `themes` is a symbolic link, even when the target exists | `themes/<id>.toml: cannot open themes directory`, nothing written through it | REQ-3 |
| `themes/<id>.toml` is a symbolic link, even when the target exists | `themes/<id>.toml: cannot open theme file`, nothing written through it | REQ-3 |
| `themes` is a regular file, or creating or opening it fails, including permission | The themes-directory line above, nothing written | REQ-3 |
| Create, write, or flush of the theme file fails | `create_new(true)`, then the theme-file line. Remove the partial file. Ignore a failed remove | REQ-3 |
| `/theme new quiet-amber` submitted from the app | One `ThemeFileCommand` | REQ-1 |
| `/theme light`, `/theme dark`, `/theme auto` | Still not `ThemeFileCommand` | REQ-1 |
| Palette `args_hint` | `[light|dark|auto|use [<id>]|new <id>|reload]` | REQ-1 |
| Theme page | Gains REQ-4's sentence after `These commands never write configuration or Hermes files.` That sentence stays. No bullet under `## Commands` | REQ-4 |
| Hermes payload | This issue states none | REQ-5 |
