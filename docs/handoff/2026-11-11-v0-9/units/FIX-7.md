# FIX-7: A status-line field for the agent's working folder

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

The issue states REQ-1 through REQ-5, the tests, and the stop conditions. This note maps those ids onto the current tree. It does not add a requirement and it does not restate a test. Line numbers are from Talaria `main` at the draft. REQ-4 says this issue states no Hermes fact, so there is no Hermes file to re-read.

## What the input document is today

`DOCUMENT_FIELDS` is eight names (`crates/talaria-tui/src/status/document.rs:9-18`): version, mode, connection, session, turn, pending_prompts, subagents, usage. `StatusDocument` (`document.rs:22-39`) documents `version` as always 1 (`document.rs:24`). `from_inputs` (`document.rs:70-91`) returns `None` when `inputs.session` is missing (`document.rs:71`) and otherwise writes `version: 1` (`document.rs:73`). It does not read `working_dir`. `encode` (`document.rs:94-99`) writes compact JSON and one trailing newline. The comment at `document.rs:95` says the document holds only strings and integers. An optional string is still a string or null, so that comment still holds after REQ-1.

`StatusInputs.working_dir` is already `Option<String>` (`crates/talaria-tui/src/status/segments.rs:57`), documented as the working folder on the gateway's machine (`segments.rs:56`). `home_dir` (`segments.rs:59`) and `branch` (`segments.rs:61`) sit beside it. `status_inputs` copies the foreground session folder into `working_dir` (`crates/talaria-cli/src/run.rs:1253`) and copies the branch on the next line (`run.rs:1255`). REQ-1 does not edit `run.rs`. The document does not gain `branch`.

## Why the copied value is not the text the bar draws

`folder_segment` (`segments.rs:411-444`) reads `working_dir`, trims it, and drops an empty string (`segments.rs:412-416`). It then calls `shorten_path` (`segments.rs:427`, defined at `segments.rs:448`). `shorten_path` rewrites a path under `home_dir` to start with `~` (`segments.rs:453-459`) and cuts a long path to `FOLDER_WIDTH` columns (`segments.rs:15`, `segments.rs:462`). REQ-1 copies `StatusInputs.working_dir` before any of that. `None` stays null. An empty string and a string of spaces stay those strings. Serde emits struct fields in declaration order, and the frozen sample asserts that order, so `working_dir` is declared last. `usage` is already `Option` and is present as null in the frozen sample (`status_contract.rs:160-165`). `working_dir` uses the same shape: no `skip_serializing_if`.

## The two version 2 documents

The format page (`docs/formats/status-line.md`) is included by the contract test (`crates/talaria-tui/tests/status_contract.rs:17`). Its opening says the field set does not change without a version 2 bump (`status-line.md:9-11`). The input sample is version 1 and has no folder (`status-line.md:38-49`). The input table says `version` is always 1 (`status-line.md:53`). The header line is `Version: 1` (`status-line.md:5`). REQ-2 is the edit to that input section, and it changes that header line to `Version: 2` because the page says its field set does not change without that bump. The two sentences REQ-2 requires are what the page says about the new field.

The output document is already version 2, introduced as a separate namespace from "the version-1 input document above, which is unchanged" (`status-line.md:88-90`). After REQ-2 that parenthetical is false: the input is version 2 as well, and the two documents stay separate namespaces. The output sample (`status-line.md:94-99`) and the output table (`status-line.md:101-104`) stay. The rejection of an output document that carries unknown fields (`status-line.md:111-114`) is about the document the command writes. It is not the input rule that a command may ignore an unknown field. `output_version_two_document_yields_rows_with_tones` (`status_contract.rs:760`) locks the output shape.

## The child process directory

`run_command_shared` sets the child's directory with `.current_dir(&spec.launch_dir)` (`crates/talaria-tui/src/status/command.rs:213`). The format page says that directory is the directory Talaria was launched from, not `~/.talaria` (`status-line.md:122-124`). `working_directory_is_the_launch_directory` (`crates/talaria-tui/tests/status_command.rs:181-198`) runs `pwd -P` and compares it to the launch directory. REQ-3 leaves both the call and the test in place.

## The migration sentence

Under `## Your status-line command`, `docs/migrating-from-0.6.5.md:70` says the command receives a version 1 JSON document. The same sentence says the command may return version 2 JSON. `required_statements_are_present` (`crates/talaria-cli/tests/docs.rs:431`) requires the phrase `version 1` in that section (`docs.rs:507`). REQ-5 changes the input clause and replaces that phrase with the input clause. A pin of the bare words `version 2` would already match the output clause.

## How the existing tests lock the old shape

`session_inputs` (`status_contract.rs:26-35`) sets the connection and a session and leaves the rest default, so `working_dir` is none. `encoded` (`status_contract.rs:37-41`) requires `from_inputs` to return a document. `stdin_document_has_exactly_the_frozen_field_set` (`status_contract.rs:123`) asserts eight names, a text prefix `{"version":1,`, key order, and one byte sample (`status_contract.rs:160-165`). The default `session_inputs()` sample is mode `live`, connection `connected`, turn `idle`, title null, and usage null. REQ-1's sample is that same text with version 2 and `working_dir` null at the end.

`stdin_document_fields_match_the_format_document` (`status_contract.rs:321`) takes the first table whose header starts with `| field` (`status_contract.rs:322-346`). The output section has a second table with the same header, so the new row has to be in the input table, and the input sample has to stay the first fenced `json` block (`status_contract.rs:354`). The test compares names as a list (`status_contract.rs:348-350`) and keys as a set (`status_contract.rs:358-371`). It does not compare the page sample's bytes to the frozen sample. The page sample may keep its filled `usage` and its title. It must gain the key.

The test commit updates the page and both existing tests. Until `document.rs` changes, the page lists nine names while `DOCUMENT_FIELDS` lists eight, and `encode` still omits `working_dir`. That is why those tests fail on assertions. The new test reads JSON from `encode`. It does not name a field that does not exist yet, so it compiles.

## Edges, by requirement

| Case | Result | Id |
| --- | --- | --- |
| `working_dir` is `/work/app` | JSON string `/work/app`, `version` 2 | REQ-1 |
| `working_dir` is none | key present, JSON null, `version` 2 | REQ-1 |
| `working_dir` is an empty string | JSON `""`, not null | REQ-1 |
| `working_dir` is two spaces | JSON `"  "`, not null | REQ-1 |
| no session | `from_inputs` returns `None`, as today | REQ-1 |
| `branch` is set | the document still has no `branch` field | REQ-1 |
| input table and input sample | nine names, `working_dir` last, the two sentences | REQ-2 |
| format page header | `Version: 2` | REQ-2 |
| output document with unknown fields | still rejected, as today | REQ-2 |
| migration input clause | `receives a version 2 JSON document` | REQ-5 |
| child process directory | still `launch_dir` | REQ-3 |
| Hermes payload | this issue states none | REQ-4 |
