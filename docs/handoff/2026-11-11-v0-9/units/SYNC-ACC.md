# SYNC-ACC: Proof: the watcher run by hand produces the issue and branch, or the early stop, it should

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read the issue for the hand-run commands, the record, and the pass rules. Use this document for what exists on this checkout and for how the four watcher outcomes map onto REQ-3. It restates no requirement.

## What exists on this checkout

`docs/acceptance/v0.7.0/phase-1.md:1-10` is the shape `PROOF-4` copies. Line 1 is the title. Line 3 is `Candidate:`. Line 4 is `Hermes:`. Line 5 is `Herdr:`. Line 6 is `OS:`. Line 7 is `Date:`. Line 9 is `| id | persona | result | evidence | commit |`. `docs/acceptance/v0.9.0/` does not exist yet. `PROOF-4` creates `docs/acceptance/v0.9.0/hermes-sync.md` with the title `# hermes-sync acceptance record`, those five labels empty, and the column header, and with no data rows. REQ-2 fills those five header lines and adds no other. No row starts Talaria, so the candidate build block is not used. Pasted output goes through `scrub`.

`.github/workflows/hermes-sync.yml` does not exist yet. `SYNC-4` adds it, workflow name `Hermes sync`, with `workflow_dispatch` and no inputs. `.github/workflows/contract.yml:18` is a `workflow_dispatch` with no inputs, and line 21 is `contents: read`. REQ-1 dispatches `hermes-sync.yml` and does not edit `contract.yml`.

`tools/contract-gen/contract_gen.py:22` sets `REPO` to `NousResearch/hermes-agent`. `resolve_tag` (`contract_gen.py:587-601`) treats `latest` as `gh api repos/<REPO>/releases/latest` and reads `tag_name` (`contract_gen.py:591-592`). The same function raises when that release is a draft or a prerelease, or when the tag is below v0.22.3, and it then resolves the tag to a commit. REQ-1 step 5 does not run those checks. `compare_versions` (`contract_gen.py:121-122`) is greater than zero when the left tag is newer. `read_pin` (`contract_gen.py:102-104`) reads `crates/hermes-contract/PINNED`. Before `PIN-3`, that file is `tag=v0.22.4` and a forty-digit commit. When this issue starts, `PIN-3` has set it to two lines, `tag=v0.23.1` and a `commit=` line. REQ-2 pastes step 6 in full and does not copy that tag into the Hermes line. The Hermes line is step 5. If the pasted step 6 output contains `v0.22.4`, stop. `main` (`contract_gen.py:710-724`) has `generate`, `check`, `verify-vendor`, `pin`, and `drift`. There is no `bump` sub-command yet. `SYNC-3` adds `python3 tools/contract-gen/contract_gen.py bump --tag <tag>`. `SYNC-4` runs it. REQ-1 does not.

`tools/prepush.sh` does not exist on this checkout. `SEAT-0` provides it, and it is on `main` before this issue starts.

`.github/workflows/release-candidate.yml:3-6` runs on a pull request only when `tools/release/**` changes. Line 22 runs `sh tools/release/tests/rc.test.sh`. `PROOF-4` adds `case_proof_gates_and_acceptance` to that script. The case reads only `docs/acceptance/v0.9.0/pin.md`, `hermes-sync.md`, `signal.md`, `act.md`, `truth.md`, `crew.md`, and `ship.md`. It asserts that those seven files keep their title and column header and do not contain `v0.22.4`. It does not read `docs/acceptance/v0.7.0/`. It does not look for `PASS`. REQ-2 writes `PASS`. That does not fail the case. REQ-4 forbids editing that script. If step 5 prints `v0.22.4`, or the pasted step 6 output contains `v0.22.4`, the issue says stop.

## Outcome map for REQ-3

REQ-3 picks one row. The operator does not pick a different row when two could be argued.

The job is `hermes-sync`. The step names are `compare`, `bump`, `test`, `pull`, and `issue`.

| What the jobs JSON and step 7 show | outcome | link |
|---|---|---|
| Step `compare` succeeded, and step `bump` or step `test` has conclusion `failure` | `issue` | `https://github.com/infiquetra/talaria/issues/` plus the `number` of the step 10 row titled exactly `Hermes <tag> needs a manual sync` |
| Step `compare` succeeded, both flags are `no`, and step `pull` has conclusion `success` | `pull` | open issue titled `Hermes <tag> is ready to sync`, then `chore/hermes-sync-<tag>` |
| Step `compare` succeeded, both flags are `no`, `bump` did not succeed, and step 7 is greater than zero | `stop-already-open` | that open issue's url, else `none` |
| Step `compare` succeeded, both flags are `no`, `bump` did not succeed, and step 7 is not greater than zero | `stop-not-newer` | `none` |

A step 5 tag of `v0.22.4` is not the last row. REQ-3 stops before any outcome. Step `compare` failing is not `stop-already-open`. For outcome `issue`, the link is `https://github.com/infiquetra/talaria/issues/` plus the `number` of the step 10 row with that exact title. If no row's title matches exactly, REQ-3 stops and does not write the link. The step named `issue` does not choose the outcome, and its conclusion is not read. For outcome `pull`, the link is that issue url, one space, and `chore/hermes-sync-<tag>`. The operator's comment includes `git ls-remote --exit-code --heads origin chore/hermes-sync-<tag>`, and exit status 0 is required. If the ready-to-sync issue is not open, or that command does not exit 0, REQ-3 stops and does not write a pull request url. The jobs JSON has step names and conclusions, not the shell command. `merged: no` and `tagged: no` are written after `git show HEADSHA:.github/workflows/hermes-sync.yml` shows no `gh pr merge`, no `git tag`, and no `gh release create`. The five lines `tag:`, `outcome:`, `link:`, `bump-failed:`, and `tests-failed:` are under `## UAT-1`. `merged: no` and `tagged: no` are under `## UAT-2`. An older merged pull request in step 9 does not change `merged: no`.
