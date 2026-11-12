# SYNC-4: Daily watcher pushes a branch and opens an issue when Hermes publishes a release

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the workflow and the generator functions that exist today, for the two Hermes files the run is allowed to cause to be read, and for how the early stop and the failure update fit together. The requirements, the tests, and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

Read at commit `ee5c48812b08843b186452fd7d3083d92e871711`.

`.github/workflows/hermes-sync.yml` is not in the tree. REQ-1 adds it. The weekly workflow `.github/workflows/contract.yml` is the one this issue leaves alone.

That weekly file is named `Hermes contract` (`contract.yml:1`). Its schedule is `cron: '17 6 * * 1'` (`contract.yml:16-18`) and it also has `workflow_dispatch` (`contract.yml:18`). GitHub schedules that cron in UTC (Coordinated Universal Time). Permissions are `contents: read` (`contract.yml:20-21`). Both jobs use `runs-on: ubuntu-latest` (`contract.yml:38`, `contract.yml:70`) and `timeout-minutes: 30` (`contract.yml:36`, `contract.yml:68`). The drift job sets `GH_TOKEN: ${{ github.token }}` (`contract.yml:72`). Checkout is `actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` (`contract.yml:42`, `contract.yml:74`). The toolchain step is `rustup toolchain install` (`contract.yml:44`, `contract.yml:76`). cargo-typify is installed as `cargo install cargo-typify --version 0.8.0 --locked` with both `CARGO_TARGET_DIR` and `CARGO_BUILD_BUILD_DIR` set to `${{ github.workspace }}/target/ci-tools/cargo-typify-0.8.0` (`contract.yml:55-59`). REQ-7 copies that install onto the new workflow. It does not copy the drift job's clone.

The drift job clones the newest tag into the runner temp directory (`contract.yml:92-105`):

```bash
git clone --depth 1 --branch "$tag" https://github.com/NousResearch/hermes-agent "$RUNNER_TEMP/hermes-contract-source"
```

The tag in that job comes from `resolve_tag('latest')` (`contract.yml:95-101`). The next step passes `--hermes-checkout` (`contract.yml:105`). REQ-5 forbids both the clone and that flag. The drift job's cache role is `contract-drift` (`contract.yml:81`). That role stores no workspace objects (`.github/actions/rust-cache/cache.py:34`). REQ-7 uses `contract-check-debug` (`cache.py:32`, path `target/debug`) and `typify-install-release` (`cache.py:13`, path `target/ci-tools/cargo-typify-0.8.0`) because those roles already exist for a debug build and for the typify install.

`tools/contract-gen/contract_gen.py` is Python 3.9, standard library only. `TYPIFY_VERSION` is `"0.8.0"` (`contract_gen.py:16`). `generate` raises if `cargo typify --version` is not `cargo-typify 0.8.0` (`contract_gen.py:407-409`, `contract_gen.py:512-513`). That is why REQ-7 installs cargo-typify only when `proceed` is true, in its own step after `compare` and before the step whose id is `bump`. The step `bump` is only the bump command. The `compare` step does not run that install.

`resolve_tag` (`contract_gen.py:587-603`) accepts `latest`. For `latest` it calls `gh api repos/NousResearch/hermes-agent/releases/latest`, reads `tag_name`, and rejects a draft, a prerelease, or a tag older than `v0.22.3`. It then resolves the tag to a commit and returns `(tag, sha)`. An annotated tag is peeled until the object type is `commit` (`contract_gen.py:597-600`). REQ-2 uses that tag. It does not need a second lookup.

`read_pin` (`contract_gen.py:102-104`) returns the tag and the commit from `crates/hermes-contract/PINNED`. `parse_pinned` (`contract_gen.py:107-114`) requires two lines, `tag=` and `commit=` with 40 lowercase hex digits. The file today is:

```text
tag=v0.22.4
commit=4d04303818b61bf3aa09278af48356315515965b
```

`compare_versions` (`contract_gen.py:121-122`) is greater than zero when the left tag is newer. The pin test asserts `compare_versions("v0.22.10", "v0.22.4") > 0` (`tools/contract-gen/tests/test_pin.py:29`). REQ-2 uses that direction. The watcher tests do not hard-code `v0.22.4`. The pin on `main` changes when earlier units land.

`main` (`contract_gen.py:710-728`) accepts `generate`, `check`, `verify-vendor`, `pin`, and `drift`. There is no `bump` sub-command in this tree. [#550](https://github.com/infiquetra/talaria/issues/550) (`SYNC-3`) adds `python3 tools/contract-gen/contract_gen.py bump --tag <tag>`, and that command is on `main` before this issue starts. REQ-3 runs it. This issue does not edit `contract_gen.py`.

`gh_raw` (`contract_gen.py:570-571`) fetches one file with `gh api` and `Accept: application/vnd.github.raw+json`. `verify_vendor` (`contract_gen.py:574-577`) already limits a pair of reads to the contract path and `LICENSE`:

```python
for local, remote in ((Path(contract_path), CONTRACT_PATH), (CRATE / "vendor/LICENSE.hermes", "LICENSE")):
```

`REPO` is `NousResearch/hermes-agent` (`contract_gen.py:22`). `CONTRACT_PATH` is `apps/shared/src/gateway-contract.openrpc.json` (`contract_gen.py:23`). Both that path and `LICENSE` exist at Hermes v0.23.0. Re-verify both at the tag `resolve_tag("latest")` returns before relying on them. REQ-4 and REQ-5 do not add a third path.

The generator tests already run in continuous integration as `python3 -I -m unittest discover -s tools/contract-gen/tests -t tools/contract-gen` (`contract.yml:61`). That command is the push and pull-request job. The weekly drift job does not run it. `tools/contract-gen/tests/test_watch.py` does not exist yet. This issue adds it in that folder. The four tests only read the new workflow file.

`tools/contract-gen/tests/` has no test whose name starts with `test_watch`. The class `WatcherWorkflowTest` and the four method names in the issue are new.

## How the stop and the update fit

REQ-2 and REQ-4 use one reading. If the newest tag is not newer than `PINNED`, or an open issue is titled exactly `Hermes <tag> is ready to sync` or exactly `Hermes <tag> needs a manual sync`, or `git ls-remote --exit-code --heads origin chore/hermes-sync-<tag>` exits 0, `proceed` is `false` and the job exits 0. Exit status 2 means the branch is absent and does not fail the step. Any other status fails the step. The run does not push. A person leaves the issue open to make the watcher leave that tag alone. The next day still sees that open issue, so it stops and does not edit the issue again. `gh issue edit` runs only when this same run has already passed the compare step and then `bump` or `cargo test --workspace` has exited non-zero, and a second overlapping run finds the manual-sync title open. The issue step runs when `failure()` is true, `proceed` is true, and `steps.bump.outcome == 'failure'` or `steps.test.outcome == 'failure'`. A failed checkout, toolchain install, cache restore, or `pull` step does not open an issue. An error from `resolve_tag` fails `compare`, leaves `proceed` unset, and opens no issue.

| Situation | Job result | Where it is decided |
| --- | --- | --- |
| Newest tag is not newer than `PINNED` | `proceed` false, exit 0, no `bump`, no toolchain install | REQ-2, REQ-7, test 1 |
| Open issue titled `Hermes <tag> is ready to sync` | same stop | REQ-2, test 1 |
| Open issue titled `Hermes <tag> needs a manual sync` | same stop | REQ-2, test 1 |
| `git ls-remote` exits 0 for `chore/hermes-sync-<tag>` | same stop, no push | REQ-2, test 1 |
| `git ls-remote` exits with a status other than 0 or 2 | the compare step fails | REQ-2, test 1 |
| Newer tag, nothing open, `bump` and `cargo test --workspace` both exit 0 | one branch and one issue titled `Hermes <tag> is ready to sync`; no `gh pr` command | REQ-3, test 2 |
| `bump` exits non-zero | one manual-sync issue, no push, `cargo test` does not run | REQ-4, test 3 |
| `cargo test --workspace` exits non-zero | one manual-sync issue, no push | REQ-4, test 3 |
| Overlapping run finds the manual-sync title already open after a failure | `gh issue edit` replaces the body | REQ-4, test 3 |
| bump stdout lists older `recorded_against` paths | the ready issue includes that stdout, and the run still succeeds | REQ-3, intent item 2 |
| Weekly file `.github/workflows/contract.yml` | not in the diff | REQ-5, AC-9 |

The captured files are `$RUNNER_TEMP/hermes-sync-bump.txt`, `$RUNNER_TEMP/hermes-sync-bump-err.txt`, `$RUNNER_TEMP/hermes-sync-test.txt`, `$RUNNER_TEMP/hermes-sync-ready.txt`, and `$RUNNER_TEMP/hermes-sync-failure.txt`. The bump file is that command's stdout. The bump err file is its stderr. The test file is stdout and stderr together. For a failed `bump`, the failure tail joins the stdout file and then the stderr file. `pull` runs `git switch -c chore/hermes-sync-<tag>`, stages only the paths `bump` wrote, and pushes with `git push origin HEAD`. That push does not update `main`. The toolchain install, the two cache restores, and the cargo-typify install are their own steps after `compare` and before `bump`, each gated on `proceed`. A `uses:` step is not placed inside a `run:` block. The ready file is, in order, the line `both commands passed`, the compare link REQ-3 names, those stdout bytes, and the `Closes` sentence REQ-3 names. The failure file starts with the failing command on one line, then a fenced block of the last 100 lines of the combined output, taken with `tail -n 100`. The failure text is the last 100 lines and at most 50,000 characters. If those 100 lines are longer than 50,000 characters, only their last 50,000 characters are kept. The failure file writes the tag the run resolved. The workflow source does not contain the characters `<tag>`.

`recorded_against` is not scanned by this workflow. The field and the printed list arrive inside `bump`, which [#550](https://github.com/infiquetra/talaria/issues/550) (`SYNC-3`) provides. Intent item 2 says nothing fails because of that list. Test 2 asserts the pull step does not mention the field, and that the ready file includes the bump stdout.

## Schedule, branch, and commit

The schedule is `cron: "23 7 * * *"` (07:23 UTC every day), plus `workflow_dispatch`. It does not share a minute with `17 6 * * 1` (`contract.yml:17`). REQ-1 states that cron. Test 1 asserts the cron string.

Steps `compare`, `bump`, `test`, `pull`, and `issue` each have `- name:` and `id:` set to that word. Before the commit, step `pull` runs `git config user.name "github-actions[bot]"` and `git config user.email "41898282+github-actions[bot]@users.noreply.github.com"`. The commit message is exactly `chore(contract): sync with Hermes <tag>` and has no trailer line. The step then pushes one branch named `chore/hermes-sync-<tag>` (for example `chore/hermes-sync-v0.23.2`) and does not force-push. It opens one issue titled `Hermes <tag> is ready to sync`. REQ-3 states the issue body. Test 2 asserts the author, the branch prefix, the commit message, and that the workflow has no `gh pr` command. If `git ls-remote --exit-code --heads origin chore/hermes-sync-<tag>` exits 0, REQ-2 sets `proceed` false and the run does not push.

The runner, the checkout pin, the cargo-typify install, the permissions, the token, the titles, the commands, the schedule, the branch name, the commit message, the git author, the branch check, the failure-text length, and the early stop are the values above. Permissions are `contents: write` and `issues: write`, and nothing else. The token does not open a pull request.

## Hermes files

At Hermes v0.23.0 the contract file is `apps/shared/src/gateway-contract.openrpc.json` and the licence file is `LICENSE`. REQ-5 allows the workflow to cause `bump` to read those two, and no other file, at the tag being considered. The workflow does not check out that repository, so it cannot execute a file from it.
