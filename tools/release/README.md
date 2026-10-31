# Release candidate checks

The runner evaluates one clean, frozen commit. Run it from the candidate checkout; if a required gate fails, do not retry that gate on the same candidate to obtain a pass. Correct the cause, freeze a new candidate and rerun the required gates. Only the full `RC_RESULT` is release evidence; `RC_PARTIAL` and the class-specific results are feedback.

```sh
sh tools/release/rc.sh --list
sh tools/release/rc.sh run --class ci
sh tools/release/rc.sh run --class live
sh tools/release/rc.sh run --class operator
sh tools/release/rc.sh run --only fmt
sh tools/release/rc.sh allowed-diff CANDIDATE TAG_COMMIT
```

`--list` prints the gate catalogue without running any gate. For runs, the class filter applies before `--only`; an empty selection is a usage error.

A full run requires all classes and the operator's explicit authorization for the pause gate. Run gateway activities serially. The runner never starts or reconfigures Hermes or pauses the gateway itself except when the separately authorized operator gate runs. Live smoke commands use the smoke tool's safe launch behavior.

Prerequisites include the pinned Rust toolchain, Python 3.9 or newer, Git, a signed-in GitHub CLI and network access. Contract generation also needs cargo-typify 0.8.0. Live checks need the pinned Hermes gateway, a configured fixture profile, a Herdr pane, tmux for the relevant suite, and jq for later suites. Missing tools or gateway setup block the relevant gate; they are not passes. A caller may set `CARGO_TARGET_DIR` to a shared build directory; release-binary gates use that directory when set.

Each required gate must pass. Exit 0 is PASS; exit 77 is SKIP and its last non-empty output line gives the reason; any other code is FAIL. An informational gate may fail or skip without failing the run, and its result belongs in the release notes. A gate that changes tracked or unignored files, or changes HEAD, fails and stops the run. Exit 0 means all selected required gates passed, 1 means they did not, and 2 means invalid input or an unsafe output destination. A class filter changes the result prefix; `--only` or a custom gate file always produces `RC_PARTIAL`.

`--evidence FILE` and `--summary FILE` require an existing parent directory outside the repository. They are replaced on each run. Evidence includes the last 40 lines of each gate, with the repository root, home directory and temporary directory replaced by `<repo>`, `<home>` and `<tmp>`. Other user-directory prefixes are replaced too, and the private prefix of temporary paths is removed. Review the output before sharing it. The summary includes selected gates that were not run after a frozen-tree violation.

`allowed-diff` accepts documentation and selected version changes only. In manifests it accepts package or workspace package versions, inline path dependency version changes, and expanded path dependency version changes only when all non-version text remains equal. In the lockfile it accepts balanced version-only edits in local package blocks without a source. Registry or Git dependency changes, code, tests, fixtures and workflows are rejected. Both arguments must be commits and the candidate must be an ancestor of the tag commit.

The document scanner checks tracked Markdown and text files for user paths, temporary paths, key-shaped text, credential-bearing URLs, and the current home, login and host strings. It prints file, line and rule only, never a match. Unlike the frame fixture scan, it reads documentation recursively. A hit is reported to the owning document; there is no allow-list.

The gate list is reviewed before freezing. It covers formatting, lint, tests, release builds, fixture replay, scans, contract checks, performance, all listed smoke suites, live tests and the operator-controlled pause. Informational drift against the newest Hermes release is recorded but does not change the result. Live test targets report skips with `<SUITE>_RESULT: SKIPPED` markers (`LIVE_ACP_RESULT`, `LIVE_ATTENTION_RESULT`, `LIVE_AUTOMATION_RESULT`, `LIVE_BACKGROUND_RESULT`, `LIVE_BG_RESULT`, `LIVE_CALL_RESULT`, `LIVE_CHECKLIST_RESULT`, `LIVE_COMFORTS_RESULT`, `LIVE_CONFIG_SURFACE_RESULT`, `LIVE_CONFIGURE_RESULT`, `LIVE_CONTEXT_RESULT`, `LIVE_CRON_RESULT`, `LIVE_FLEET_RESULT`, `LIVE_FLEET_VIEW_RESULT`, `LIVE_MCP_RESULT`, `LIVE_MEMORY_WRITE_RESULT`, `LIVE_MODEL_RESULT`, `LIVE_MODEL_ROLES_RESULT`, `LIVE_PAGER_RESULT`, `LIVE_PERMISSIONS_RESULT`, `LIVE_PRINT_RESULT`, `LIVE_PROVIDERS_RESULT`, `LIVE_QUEUE_RESULT`, `LIVE_RECONNECT_RESULT`, `LIVE_RECORD_RESULT`, `LIVE_REDUCE_RESULT`, `LIVE_REMOTE_RESULT`, `LIVE_REVIEW_RESULT`, `LIVE_REWIND_RESULT`, `LIVE_SESSION_PICKER_RESULT`, `LIVE_SESSION_RESULT`, `LIVE_SIDE_RESULT`, `LIVE_SKIN_RESULT`, `LIVE_STREAM_RESULT`, `LIVE_SUBAGENT_RESULT`, `LIVE_TOOL_CELLS_RESULT`), which `live-tests` rejects even if the process exits 0. Recheck command availability, live skip markers, timing and workflow timeouts on the candidate; do not remove a required gate to make a run pass.
