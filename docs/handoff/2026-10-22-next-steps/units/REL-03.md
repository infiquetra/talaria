# REL-03: Version, changelog, tag and release notes for v0.7.0 (unit REL-03)

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what the two pull requests contain, which nine tests to write first, and which six scenarios a tester runs. This document holds what a builder would otherwise guess: the facts about the repository, tags and releases that were checked on 2026-10-22, the exact order of the release day with commands, the version bump procedure, the shape of the changelog section and the release notes, the parsing rules of each test and how to see each one fail, the closeout template, the edge cases and the reasons behind the design. Facts that depend on a unit that has not merged yet are marked "re-verify at the freeze"; read the merged file, not this document, when they differ.

## 1. What this unit is for, in plain terms

Talaria 0.7.0 is the first release of the Rust client. Four things turn a green main branch into a release a person can trust.

1. The version says `0.7.0`, in one place, and the program reports it.
2. The changelog tells a user what is different, in the user's words, without planning codes.
3. A tag `v0.7.0` points at exactly the commit that was tested.
4. The GitHub release page carries release notes that are true, and the notes tell the 0.6.5 reader where to go.

The tag cannot be undone (section 2.1). That single fact shapes the whole unit: the work is arranged so that the last step is small, checked by commands that fail loudly, and approved by the operator in writing.

Related units, so scope stays apart: `{{REL-01}}` proves the candidate commit works and owns `tools/release/`; `{{REL-02}}` writes the user documentation and the migration page; `{{P6-07}}` will package binaries and a Homebrew tap, later; `{{P0-02}}` created the release workflow this unit runs for the first time.

## 2. Facts checked on 2026-10-22

### 2.1 Version tags cannot be moved or deleted

Command: `gh api repos/infiquetra/talaria/rulesets --jq '.[]|[.name,.target,.enforcement]|@tsv'` printed one row: `Immutable version tags`, `tag`, `active`. The detail of that ruleset (`gh api repos/infiquetra/talaria/rulesets/<id>`): conditions include `refs/tags/v*` and exclude nothing; rules are `deletion`, `non_fast_forward` and `update`; there are no bypass actors. Meaning: once `v0.7.0` is pushed, it cannot be deleted, re-pointed or force-moved by anyone. A wrong tag is fixed by releasing `v0.7.1`. The same holds for a rehearsal tag such as `v0.7.0-rc.1` (it matches `refs/tags/v*`). Deleting the GitHub release object of a rehearsal is a different operation and is not covered by the ruleset (not tested; do not rely on it without trying it on the rehearsal release first).

Re-verify at the freeze, because the operator may change rulesets.

### 2.2 Releases and tags that exist

- `gh release list -R infiquetra/talaria -L 4` printed `Talaria v0.6.5` as `Latest` (published 2026-09-17) and v0.6.3, v0.6.2, v0.6.4 below it.
- `v0.6.5` is an annotated tag (object type `tag`). `v0.6.4` is a lightweight tag (it points straight at a commit). Use an annotated tag for `v0.7.0`: it carries a message and a tagger, and `git cat-file -t v0.7.0` prints `tag`.
- `python-final` is created by `{{P0-02}}`; it does not match `refs/tags/v*`.

### 2.3 The changelog today

`CHANGELOG.md` on `main` (read 2026-10-22, before `{{P0-02}}` edits it): follows Keep a Changelog. The first release heading is `## [Unreleased]`, then `## [0.6.5] — 2026-09-16` (an em dash between version and date), twelve `## [` headings in all (`[Unreleased]` and eleven dated releases, 0.6.5 down to 0.1.0). Sections inside a release are `### Added`, `### Changed`, `### Fixed`, `### Removed`. Reference links sit at the end of the file, one per heading, for example `[Unreleased]: https://github.com/infiquetra/talaria/compare/v0.6.5...HEAD` and `[0.6.5]: <url>`; older releases link `https://github.com/infiquetra/talaria/releases/tag/v0.5.0`, newer ones link a file under `docs/releases/`. `{{P0-02}}` re-points the `docs/releases/` links to the files at `python-final` and adds a `### Removed` entry under `## [Unreleased]`. Re-verify at the freeze what the file looks like after the phases have added their entries.

Links into folders `{{P0-02}}` removes, counted on 2026-10-22 (`grep -oE '\]\([^h#)][^)]*\)' CHANGELOG.md`): 13 relative links, namely 8 to `docs/releases/v<version>.md`, 2 to `docs/acceptance/` and 3 to `docs/plans/`; plus 5 absolute reference links of the form `https://github.com/infiquetra/talaria/blob/main/docs/releases/v<version>.md`. The `{{P0-02}}` drafts disagree with themselves here: their unit specification section 7.5 redirects only "links of the form `docs/releases/v<version>.md`", while their REQ-20 and AC-17 require that `CHANGELOG.md` has no dead relative link at all, which also covers the 5 links into `docs/acceptance/` and `docs/plans/`. Test 4 checks relative links only, so it is green on the candidate only if AC-17 held. If it did not, PR-A redirects each dead relative link to `https://github.com/infiquetra/talaria/blob/python-final/<same path>` (a `CHANGELOG.md` change, allowed before and after the freeze), names it in the PR-A description and reports it on `{{P0-02}}`. The 5 absolute `blob/main/docs/releases/` links are outside test 4; if they still point at `main` after `{{P0-02}}`, PR-B redirects them to `python-final` in the same way and says so.

Format decisions for 0.7.0: heading `## [0.7.0] — YYYY-MM-DD` (same em dash; the test accepts a plain hyphen too); the reference link `[0.7.0]: https://github.com/infiquetra/talaria/releases/tag/v0.7.0`; and `[Unreleased]: https://github.com/infiquetra/talaria/compare/v0.7.0...HEAD`.

### 2.4 The release workflow (from the drafts of `{{P0-02}}`; re-verify at the freeze)

`.github/workflows/release.yml` after `{{P0-02}}` (REQ-17 and unit specification section 5.2 of that issue): triggers are a pushed tag `v*` and `workflow_dispatch` with a required string input `tag` ("Existing tag to release"); runner `macos-14`; permissions `contents: write`; a step `id: resolve` validates the tag against `^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$` and sets `prerelease=true` when the tag has a suffix; the tagged tree is checked out; the tag check compares the tag's version and the `talaria-cli` version after removing any `-suffix` from both (`v0.7.0-rc.1` and `v0.7.0` both pass against `0.7.0`; `v0.8.0` fails); the project check runs; then `gh release create "$TAG" --verify-tag --title "Talaria $TAG" --generate-notes` with `--prerelease` for suffixed tags. No binaries are attached. The first real run of this file is this unit's release.

Consequences: (1) the workflow creates the release page with generated notes, so this unit replaces the body afterwards with `gh release edit`; (2) a failed run is retried with `gh workflow run release.yml -f tag=v0.7.0` against the existing tag (the dispatch runs the workflow file from the branch you name, `main` by default, and checks out the tag), so a dispatched run reports `headBranch` `main` and `event` `workflow_dispatch`, and the tag and commit it released are read from its log; (3) the tag check passes only if the `talaria-cli` version, minus any suffix, equals the tag's version minus any suffix.

If the merged file differs (for example it takes a notes file, or `{{P6-07}}` has added binaries), follow the merged file and report the difference on the issue.

### 2.5 Hermes facts the notes repeat (verified at Hermes v0.22.4)

| Fact | Evidence |
|---|---|
| Hermes v0.22.3 and v0.22.4 are stable releases, both published 2026-10-21; the newest stable release on 2026-10-22 was v0.22.4 | `gh api repos/NousResearch/hermes-agent/releases/tags/v0.22.3`, `.../tags/v0.22.4` (field `prerelease` false) and `.../releases/latest` (field `tag_name` v0.22.4) |
| `gateway/session_authority.py` is absent at the stable release v0.22.2 and present at v0.22.3 | `gh api repos/NousResearch/hermes-agent/contents/gateway/session_authority.py?ref=v0.22.2` returned 404 Not Found; with `ref=v0.22.3` it returned the file; `.../releases/tags/v0.22.2` shows `prerelease` false (checked 2026-10-22) |
| `hermes --tui` is a command line option | `hermes_cli/_parser.py:208` (`inherited(parser, "--tui", action="store_true", default=False, ...`) |
| `--native`, alias `--tui-native`, is a command line option stored as `tui_native` | `hermes_cli/_parser.py:210` |
| The entry point treats `--tui`, `--native` and `--tui-native` (and the environment variable `HERMES_TUI=1`) as requests for the terminal interface | `hermes_cli/main.py:297` |

Source URLs: `https://github.com/NousResearch/hermes-agent/blob/v0.22.4/hermes_cli/_parser.py#L208`, `.../hermes_cli/_parser.py#L210`, `.../hermes_cli/main.py#L297`.

What the file check does and does not show: the session authority module first appears at v0.22.3 among stable releases (the row above); whether earlier pre-releases had it, and whether its behaviour at the pinned tag is what Talaria relies on, was not checked. The notes therefore state the minimum as the roadmap's rule ("v0.22.3 or newer"), not as a measured compatibility result. The three `hermes_cli` lines above are verified at v0.22.4 only; re-read them at the pinned tag before the notes repeat them.

### 2.6 What `{{REL-01}}` provides (its REQ-1 freeze, REQ-7 allowed-diff, REQ-11 re-pin rule, and unit specification section 3.5; re-verify at the freeze)

`sh tools/release/rc.sh allowed-diff <candidate> <tag-commit>`: exit 2 unless both are commits and the candidate is an ancestor. Allowed without further checks: exactly `CHANGELOG.md`, `README.md`, `llms.txt`, and any path under `docs/`. In a `Cargo.toml` only these changed lines: `version = "..."` in `[package]` or `[workspace.package]`, an inline dependency line containing `path = "`, or `version = "..."` inside an expanded dependency table with a `path` key; removed and added lines must be equal once every `version = "..."` is masked. In `Cargo.lock` only `version = "..."` lines, as many removed as added, each in a `[[package]]` block with no `source` line. Anything else is listed under `not allowed:` with one indented path per line, exit 1. Success prints `ALLOWED_DIFF: OK (<k> files)`. `sh tools/release/rc.sh run --class ci` ends with `RC_RESULT_CI: PASSED (...)` when every required `ci` gate passes; it refuses a dirty working tree (exit 2). The `ci` class needs no gateway but does need network, a signed-in `gh`, `python3` 3.9 or later and `cargo-typify` 0.8.0 (`{{REL-01}}` REQ-2; its `contract-vendor` and drift gates fetch from GitHub, and `contract-check` skips without `cargo-typify`, which fails the run); without them the result is BLOCKED (environment), never a pass.

Consequence for this unit: the pull request that makes the release commit may change only the files above. It may not add a test file, a fixture or a script. That is why the tests are a separate pull request (A) that merges before the freeze.

## 3. The release day, in order

```text
before the freeze            freeze (REL-01)                after the drive            release day
PR-A tests merge     ──►     CANDIDATE recorded     ──►     RC_RESULT: PASSED   ──►   PR-B (version, changelog, notes)
                             gates, Linux run, drive        drive verdict PASS         merges  ──►  TAG_COMMIT checks
                                                                                              ──►  operator: "tag it"
                                                                                              ──►  tag, workflow, notes edit, checks
                                                                                              ──►  closeout comment
```

Commands. `$C` (the candidate, from the closeout comment of `{{REL-01}}`) and `$T` (the tag commit) are always full 40-character commit hashes. `$W` is the directory of the fresh worktree of `$T`, and `$X` a temporary directory outside the repository. Never pass `$W` where a commit is expected, or `$T` where a directory is expected.

```bash
# 1. After PR-B has merged. Work in a fresh worktree outside the repository.
git fetch origin --tags
T=$(git rev-parse origin/main)     # right after PR-B merged; merge, squash or rebase are all allowed
git rev-parse --verify "$C^{commit}" > /dev/null                # the candidate exists
git merge-base --is-ancestor "$C" "$T" && echo "candidate is an ancestor"
git log --oneline "$C..$T"                                      # PR-B's commits (and its merge commit, if any); any other commit is named in the closeout and judged by allowed-diff
W="$X/release-tree"
git worktree add --detach "$W" "$T"
cd "$W" && [ "$(git rev-parse HEAD)" = "$T" ] && echo "worktree is the tag commit"

# 2. The two checks of REQ-6.
sh tools/release/rc.sh allowed-diff "$C" "$T"; echo "exit=$?"   # ALLOWED_DIFF: OK (<k> files)
sh tools/release/rc.sh run --class ci; echo "exit=$?"           # RC_RESULT_CI: PASSED (...)

# 3. GitHub's own checks on the same commit are green. Read every page; an empty list fails.
gh api --paginate "repos/infiquetra/talaria/commits/$T/check-runs" --jq '.check_runs[] | [.name, .status, .conclusion] | @tsv' > "$X/checks.tsv"
test -s "$X/checks.tsv" || echo "no check runs: stop"
awk -F'\t' '$2!="completed" || ($3!="success" && $3!="skipped" && $3!="neutral")' "$X/checks.tsv"   # prints nothing
for n in 'rust-check (macos-14)' 'rust-check (ubuntu-latest)' rust-coverage; do
  grep -qF "$n	completed	success" "$X/checks.tsv" || echo "missing or not success: $n"; done   # prints nothing
# The three names are the CI check names of {{P0-02}}; re-read them from the merged ci.yml.

# 4. The final tag does not exist yet (here or on origin). Match the exact ref, never 'v0.7.0*',
#    which also matches the permanent rehearsal tag v0.7.0-rc.1.
git tag -l v0.7.0                                               # prints nothing
git ls-remote origin refs/tags/v0.7.0 'refs/tags/v0.7.0^{}'     # prints nothing
# Rehearsal state (section 8): before the rehearsal, nothing; after it, the rc tag peels to $T.
git ls-remote --tags origin 'v0.7.0-rc.*'
[ "$(git rev-parse 'v0.7.0-rc.1^{commit}' 2>/dev/null)" = "$T" ] && echo "rehearsal tag is on the tag commit"

# 5. Only after the operator has written "tag it <YYYY-MM-DD>" on the issue, the date equal to the changelog heading (operator runs these).
git tag -a v0.7.0 -m "Talaria v0.7.0: the first release of the Rust client" "$T"
git push origin refs/tags/v0.7.0

# 6. Identify the run of this tag push (never 'the newest run': a rehearsal or a dispatch may be newer), record its ID, watch it.
gh run list --workflow release.yml --event push --branch v0.7.0 --commit "$T" \
  --json databaseId,headBranch,headSha,event,status,conclusion
# expect exactly one entry: headBranch v0.7.0, headSha $T, event push
# Fallback when it lists zero or several runs (for example if headBranch is not the tag name for tag pushes,
# which was not verified before the rehearsal): list the push runs of the commit and pick by tag name.
gh api "repos/infiquetra/talaria/actions/runs?head_sha=$T&event=push" \
  --jq '.workflow_runs[] | select(.path==".github/workflows/release.yml") | [.id, .head_branch, .status, .conclusion, .created_at] | @tsv'
# exactly one row whose head_branch is v0.7.0 (the rehearsal's row says v0.7.0-rc.1); none or several: stop (issue stop condition 13)
RUN=<databaseId>
gh run watch "$RUN" --exit-status
gh run view "$RUN" --json url,conclusion,headSha,event,headBranch

# 7. In $W: replace the generated notes with the reviewed ones (operator runs the edit), then compare.
cd "$W"
gh release edit v0.7.0 --notes-file docs/release-notes/v0.7.0.md
gh release view v0.7.0 --json tagName,name,isDraft,isPrerelease,targetCommitish,assets
# expect name "Talaria v0.7.0", isDraft false, isPrerelease false, assets [] (or exactly the files the
# packaging unit documents, if {{P6-07}} is closed as completed: gh issue view <its number> --json state,stateReason)
# The one comparison method: remove carriage returns from both sides, drop trailing blank lines from
# both sides (command substitution strips every trailing newline), then compare. Portable to the BSD
# tools of macOS; do not use GNU-only sed tricks such as sed -e '$a\'.
a=$(tr -d '\r' < docs/release-notes/v0.7.0.md)
b=$(gh release view v0.7.0 --json body --jq .body | tr -d '\r')
if [ "$a" = "$b" ]; then echo "notes match"; else printf '%s\n' "$a" > "$X/notes.txt"; printf '%s\n' "$b" > "$X/body.txt"; diff "$X/notes.txt" "$X/body.txt"; fi
for i in 1 2 3; do l=$(gh api repos/infiquetra/talaria/releases/latest --jq .tag_name); [ "$l" = v0.7.0 ] && break; sleep 60; done; echo "$l"  # v0.7.0, else report
```

The carriage-return and trailing-blank-line handling in step 7 is the only comparison method; the closeout comment names it. How GitHub stores and returns a release body byte for byte was not verified; the rehearsal (section 8) runs the same command first. Checked while repairing this document: on macOS, BSD `sed -e '$a\'` appends nothing to a file without a final newline, so the earlier GNU-style command reported a difference for equal text; the command above does not depend on `sed`. Trailing spaces on a line are not removed and still count as a difference.

## 4. The version bump

1. Edit one line: in the root `Cargo.toml`, under `[workspace.package]`, change `version = "0.7.0-dev"` to `version = "0.7.0"`. `{{P0-02}}` REQ-11 says every member inherits it with `version.workspace = true`. If a member sets its own `version`, stop (stop condition 1): that is a defect in the candidate, and fixing it is a pull request before the freeze.
2. Let cargo refresh the lock file, without updating anything else: `cargo check --workspace` (not `cargo update`, which can move other dependencies). The lock file is never edited by hand; if cargo changes more than version lines, stop. If cargo says the lock file needs changing under `--locked`, that is expected before this step and a problem after it.
3. Look at what changed: `git diff --stat` shows `Cargo.toml` (1 line changed) and `Cargo.lock` (one `version` line per workspace member, as many removed as added). Count the members with `cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))'`; the number of changed lock lines is that number. `git diff -U0 Cargo.lock | grep -E '^[-+]' | grep -vE '^(---|\+\+\+)'` must show only `-version = "0.7.0-dev"` and `+version = "0.7.0"` lines.
4. `cargo build --workspace --locked` exits 0, and `cargo run -q -p talaria-cli -- --version` prints `talaria 0.7.0`.
5. Search for the old string in the allowed files: `git grep -n -F '0.7.0-dev'`. In `README.md` and `llms.txt` every mention must go (the issue's AC-4 greps every file except `CHANGELOG.md` and `docs/`): change it to `0.7.0`, or reword a sentence such as "the placeholder build" or "not released yet" minimally. Test 2 allows the string in these two files only so that it is green on the candidate before the bump. Under `docs/` (not `docs/handoff/` and not the engineering journal, which record history) a mention is changed only if it describes the current version. In `AGENTS.md`, `CLAUDE.md`, a test, a fixture or a source file, a hit cannot be changed in this pull request (the allowed-diff rule), so stop (stop condition 2).

## 5. The changelog section

### 5.1 Shape (the tests parse this)

```markdown
## [Unreleased]

## [0.7.0] — YYYY-MM-DD

Talaria 0.7.0 is the first release of the Rust client for Hermes. <One or two sentences: what it is, who it is for, what to read first.> See the [release notes](https://github.com/infiquetra/talaria/blob/v0.7.0/docs/release-notes/v0.7.0.md).

### Added
- <one line a user can act on or check>

### Changed
- <...>

### Removed
- <...>

## [0.6.5] — 2026-09-16
```

and at the end of the file: `[Unreleased]: https://github.com/infiquetra/talaria/compare/v0.7.0...HEAD` and `[0.7.0]: https://github.com/infiquetra/talaria/releases/tag/v0.7.0`, above the existing `[0.6.5]:` line. `### Fixed` and `### Security` appear only when there is something true to put under them. `## [Unreleased]` is left in place and empty.

### 5.2 Where the entries come from

In this order; stop adding when an entry has no evidence.

1. The entries already under `## [Unreleased]` (units add them as they merge; `{{P0-02}}` adds the `### Removed` entry for the Python implementation). Move them into the 0.7.0 section and rewrite them in the user's words if they use unit codes.
2. The "what no longer exists" list on the migration page `docs/migrating-from-0.6.5.md` (`{{REL-02}}`): each removed thing is one `### Removed` line, worded the same way.
3. The requirements R1 to R9 of the handoff (R10 says the build must be doable by moderately capable coding agents; it is about how Talaria is made, not something a reader of the client can do, so it yields no line) ([01](../01-vision-and-requirements.md#what-the-operator-said-restated-as-testable-requirements)): for each, ask "can the reader do this on the tag commit, and did an acceptance issue prove it?" If yes, one `### Added` line naming what the reader can do (not the requirement id). If the answer is "partly", the notes' "Known limits" section says what is missing; the changelog says only what works.
4. What behaves differently from 0.6.5 and is not a removal (for example the configuration loader prints a notice for a setting it ignores): `### Changed`.

A changelog line says what the reader can do or what is different, in one sentence, with no unit codes (`P2-03`, `PH3`, `REL-01`), no issue numbers as the only explanation, no adjectives that cannot be checked ("blazing", "seamless") and no numbers that were not measured. A performance number is copied from the `bench` gate output in the candidate's evidence file (`{{REL-01}}` writes it), with the budget it met.

### 5.3 The date

The date is the day the tag will be pushed, as agreed with the operator, in the form `YYYY-MM-DD`. If the tag day moves, a one-line change to the heading is allowed after the freeze (it is under `CHANGELOG.md`), and the checks of section 3 run again on the new tag commit.

## 6. The release notes file

`docs/release-notes/v0.7.0.md`, plain markdown, absolute URLs only (a release page does not resolve relative links). Links to files in this repository use the tag: `https://github.com/infiquetra/talaria/blob/v0.7.0/<path>`; they return 404 until the tag exists, which is expected, and test 7 checks them against the working tree instead. Template (replace every angle-bracket part; delete this guidance in the real file):

````markdown
# Talaria v0.7.0

<Two to four sentences. What Talaria is (a terminal client for the Hermes agent backend), that this is a rewrite in Rust, and the one thing a new reader should do first.>

## Read this part first

<The limits, before the praise. Each is one bullet, taken from the closeout comment of the release candidate check and from the open defect issues: which platform a person drove (state exactly what the operator's drive covered), what only ran in automated tests, known defects with links, anything that needs an operator step. If a limit is carried from 0.6.5 and still true, say so.>

## What is new

<Bullets, each one something the reader can do, each with a link to the page or section of the guide that explains it. No unit codes. Numbers only from the benchmark output of the candidate, with the budget beside them.>

## What changed from 0.6.5

<Three to six bullets: the command is still `talaria`; the configuration file and what happens to settings it no longer reads; the status line; themes. Then the link to the migration page: https://github.com/infiquetra/talaria/blob/v0.7.0/docs/migrating-from-0.6.5.md>

## Install

Talaria 0.7.0 is built from source. Install the Rust toolchain that `rust-toolchain.toml` names (rustup reads it), then:

```bash
git clone --branch v0.7.0 https://github.com/infiquetra/talaria
cd talaria
cargo build --release
target/release/talaria --version
```

This release has no binary files on the release page and no Homebrew tap. Nothing is published to crates.io, npm or the Python Package Index under the name `talaria`; a package with that name there is someone else's software.

<If the packaging issue is closed as completed (`gh issue view <its number> --json state,stateReason` shows `CLOSED` and `COMPLETED`), replace the sentence about binary files with what its merged documentation page says, and keep the registry sentence.>

## Works with Hermes

Talaria 0.7.0 was tested against Hermes <pinned tag>. It needs Hermes v0.22.3 or newer. On <candidate day> the newest stable Hermes release was <newest tag>; the contract check against it <passed | found these differences: ...>.

<Written at PR-B time. `<pinned tag>`: the `tag=` value of `crates/hermes-contract/PINNED` on the candidate (it may be newer than v0.22.4). `<newest tag>` and the drift result: the `RC-CLOSEOUT` comment (its both Hermes tags and its latest-drift line). If that comment lacks either, stop (issue stop condition 4). Keep "Hermes v0.22.3 or newer" verbatim.>

## Staying on 0.6.5

If you are not moving to 0.7.0 yet, use Hermes's own terminal interface with `hermes --tui --native`. If you keep Talaria 0.6.5 installed, reinstall it with bytecode compilation so it starts faster: `uv tool install --compile-bytecode git+https://github.com/infiquetra/talaria@v0.6.5` (or set `UV_COMPILE_BYTECODE=1`). The migration page has the rest: <link at the tag>.

<Before the notes repeat the `uv` command, run it once in a throwaway environment and keep it only if it installs 0.6.5; otherwise use the handoff's form `uv tool install --compile-bytecode` with no source and say "reinstall it the way you installed it". Leave out the handoff's launch times (one machine's measurement). Re-read the three `hermes_cli` lines of 2.5 at the pinned tag.>

## Known limits

<Every open defect is listed here once, with its URL: the open issues labelled `defect` in `infiquetra/talaria` at PR-B time (`gh issue list -R infiquetra/talaria --label defect --state open --json number,title,url`; the label exists, checked 2026-10-22). Those that stop a daily task are also named under "Read this part first". Then what 0.7.0 does not do: the later-phase list of the roadmap (remote gateways, other agents through the Agent Client Protocol, kanban view, Vim mode, streaming JSON for scripts, voice, packaged binaries), minus anything that merged. Open defects with links.>

## What was checked

<Three lines: the candidate commit (7 hex) and the release candidate result line, the Linux run, the operator's drive verdict, each linked to the comment on the release candidate issue.>
````

The notes never contain a local path, a user name, a host name, a profile name other than `default` or `fixture`, or a planning code.

### 6.1 The claims table

The pull request description of PR-B carries a table `| claim | where it appears | evidence |` with one row per sentence that states a fact about the product, a number or a compatibility, in the changelog section and the notes (a sentence that only directs the reader, such as "see the guide", needs no row): the evidence is a link plus at most one quoted output line, namely a test name, a scenario result in an acceptance issue, a documentation page that a test guards, a benchmark row, or a command the builder ran (with its output). A claim with no evidence is deleted, not softened. The reviewer samples five rows, re-runs their evidence, and names the five rows and their results in the review comment on PR-B (AC-7). Benchmark numbers come from the `## bench (PASS)` block of the scrubbed evidence collapsed in the `RC-CLOSEOUT` comment.

## 7. The tests (file `crates/talaria-cli/tests/release_metadata.rs`)

The rules in this section are part of the issue's contract (the issue's Tests section says so). Where the issue's table summarises, this section decides.

### 7.0 Rules for all nine

- Standard library only (no new dependency, no regex crate: every pattern below is matched by hand-written code). The repository root is `Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")`, canonicalised. Files are read as UTF-8 text. Every assertion message names the file (relative to the root), the line number and the broken property. No test reads the network, a profile or `HOME`. No absolute path appears in a message.
- A "line" is one element of `str::lines()` (so a final newline does not add a line, and `\r\n` is handled). "Trimmed" means leading and trailing spaces and tabs removed.
- Code blocks: in Markdown files, a line whose trimmed text starts with three backticks opens or closes a fenced block. Unless a test says otherwise, lines inside fenced blocks are ignored (tests 6 and 7 ignore them; test 8 reads only them).
- Changelog sections: a section starts at a line that starts with `## [` and ends before the next such line or at the end of the file. If `## [Unreleased]` is missing, test 3 fails and the other tests treat that section as empty.
- `workspace_version()`: in the root `Cargo.toml`, find the line whose trimmed text is exactly `[workspace.package]`; read the following lines up to the next line whose trimmed text starts with `[`; skip lines whose trimmed text starts with `#`; the first trimmed line that starts with `version = "` gives the text between the quotes. `rust-version = "..."` does not start with `version` and never matches.
- `is_final(v)`: true when `v` contains neither `-` nor `+`. `notes_path()`: `docs/release-notes/v<core>.md`, where `<core>` is the version cut at the first `-` or `+` (so on `0.7.0-dev` it is `v0.7.0.md`, which may not exist yet).
- Test order in the file is the table order, each `#[test] fn` at column 0 (AC-1 reads it with `grep -oE '^fn [a-z_]+'`); helpers come after the nine tests.

### 7.1 The tests

| # | Test | Algorithm | Red run |
|---|---|---|---|
| 1 | `workspace_version_has_one_source` | Root: within `[workspace.package]` (rules of 7.0), exactly one non-comment trimmed line starts with `version = "`. Members: every `crates/<dir>/Cargo.toml` and `tools/<dir>/Cargo.toml`, one level deep only (deeper manifests, such as test fixtures, are not members). In each, take the lines from the one whose trimmed text is `[package]` to the next `[` heading, skipping `#` lines: exactly one trimmed line equals `version.workspace = true`, and no trimmed line starts with `version = "`. A missing `[package]` table fails. Every such manifest is assumed to be a workspace member (the roadmap layout); a non-member is the issue's stop condition 1, checked once by hand with `cargo metadata --no-deps --format-version 1`. | In one member replace `version.workspace = true` with `version = "9.9.9"`. Do not add the line beside the existing key: TOML refuses a table and a string under one key, so Cargo fails to parse before the test runs (checked with Python `tomllib`: "Cannot overwrite a value"). |
| 2 | `no_dev_version_literal_outside_allowed_files` | Needle `"0.7.0-" + "dev"` (two literals, so this file does not contain it). Files: the output of `git -C <root> ls-files --cached --others --exclude-standard` (tracked plus untracked-not-ignored, so build output under `target/` is never read and a scratch file is); a failing `git` fails the test with a plain message. Skip any path with a component named `corpora` or `node_modules`; skip symbolic links and files that are not valid UTF-8. Allowed, by path relative to the root: a file named `Cargo.toml` or `Cargo.lock` at any depth; `CHANGELOG.md` at the root; a path starting with `docs/`; and, only while `is_final` is false, `README.md` and `llms.txt` at the root. When the version is final, the Cargo files are no longer exempt either, which makes the test match the issue's AC-4. Any other hit fails, naming file and line. | Add `fixtures/scratch.txt` containing the needle (no `git add` needed). |
| 3 | `changelog_shape` | `CHANGELOG.md`. The first line starting `## [` is exactly `## [Unreleased]`. Every later one must match, character by character: `## [`, then `X.Y.Z` (each part one or more ASCII digits, no leading zero unless the part is `0`), then `] `, then either `—` (U+2014, em dash) or `-`, then a space, then `YYYY-MM-DD` (exactly four, two and two digits, zero-padded), then end of line (trailing spaces fail). The date must exist (month 1 to 12; day 1 to the month's length; February has 29 days in a leap year: divisible by 4, except centuries not divisible by 400). Versions strictly descend comparing (X, Y, Z) as numbers. Dates do not increase going down; equal dates are allowed (0.6.2 to 0.6.5 share one). The reference-link block is every line after the last dated section's heading that matches `[<name>]: <url>` (name without `]`, then `]: `, then a URL without spaces); a heading name of each section must appear exactly once there; extra link lines are ignored. The `Unreleased` URL must end with `compare/v<newest>...HEAD`, where `<newest>` is the first dated heading's version. No other test validates dates. | Change the newest date to `2026-13-45`; or swap two version headings. |
| 4 | `changelog_relative_links_resolve` | Files: `CHANGELOG.md` and every `docs/release-notes/*.md` (a missing folder means no notes files, which passes). Outside fenced blocks and outside inline code spans, every inline link or image `[text](target)` or `![alt](target)`: take `target` up to the first space or `)`. Skip targets starting with `http://`, `https://`, `mailto:` or `#`. Fail on a target starting with `/` or containing `:` before any `/` (another scheme). Otherwise strip `#...` and `?...` and require the path, relative to the linking file's folder, to exist as a file or directory. Reference-style link definitions are in the reference block, which test 3 owns and which holds absolute URLs only. | Add a link to `docs/does-not-exist.md`. |
| 5 | `final_version_requires_changelog_section` | If not `is_final`: pass. Otherwise: the first dated heading starts with `## [<version>] ` (only the version is compared here; test 3 owns the date); every line of the `[Unreleased]` section after its heading is blank; inside the new section, the lines `### Added`, `### Changed` and `### Removed` each appear exactly once (trimmed comparison), and each is followed, before the next line starting `### ` or `## `, by at least one line starting `- `; the section's text contains `https://github.com/infiquetra/talaria/blob/v<version>/docs/release-notes/v<version>.md`; the reference block (test 3's definition) contains the exact line `[<version>]: https://github.com/infiquetra/talaria/releases/tag/v<version>`. | Set the version to `0.7.0` only, then `cargo check --workspace`. |
| 6 | `final_version_requires_release_notes` | If not final: pass. Otherwise the notes file exists; its first line is exactly `# Talaria v<version>` (a byte-order mark fails); the lines outside fenced blocks that start with `## ` are, in order and with nothing else, the eight headings of section 6; the file contains the value after `tag=` on the line of `crates/hermes-contract/PINNED` that starts with `tag=`; it contains `cargo build --release`, `crates.io`, `npm` and `Python Package Index` (anywhere; the review checks the sentence); it contains `https://github.com/infiquetra/talaria/blob/v<version>/docs/migrating-from-0.6.5.md`; `lines().count()` is at most 400. Facts the test cannot see (drift result, drive verdict, benchmark numbers) are guarded by the claims table and the review. | Same edit as test 5. |
| 7 | `release_links_point_at_real_files` | If not final, or if the notes file is missing: pass (test 6 reports a missing file). Scan the notes and the new changelog section, outside fenced blocks. For each occurrence of `https://github.com/infiquetra/talaria/blob/v<version>/`, the URL continues up to the first space, `)`, `>`, `"`, `'` or backtick; strip trailing `.`, `,`, `;`, `:`. Split off `#anchor`. A path containing `%` fails (no percent-encoding: name files without spaces). The path must exist in the working tree. If there is an anchor, the target must be a `.md` file (an anchor on another file fails), and some heading of it, outside its fenced blocks, must have that slug. Slug, GitHub's rule: take the heading text after the leading `#` marks and one space, trimmed; remove backticks but keep the text inside them; replace `[text](url)` by `text`; lower-case ASCII letters (other letters unchanged); delete every character that is not a letter, a digit, a space, a hyphen or an underscore; turn each space into a hyphen, without collapsing; the second heading with the same slug gets `-1`, the third `-2`. If a link the test rejects opens the right section in a browser, the test is wrong: report it, do not loosen it silently. | On the scratch release set (7.2), link `https://github.com/infiquetra/talaria/blob/v0.7.0/docs/guide-missing.md` in the notes. |
| 8 | `release_notes_flags_exist_in_help` | If not final, or if the notes file is missing: pass. Collect the lines inside fenced blocks whose opening line, after the backticks, is exactly `bash` or `sh`. From each, remove leading spaces and one leading `$ `; keep the lines that start with `talaria ` or `target/release/talaria `, or equal one of those without the space. At least one such line must exist (the Install section shows `target/release/talaria --version`). Split each on whitespace; keep tokens starting with `-`, cut at `=`, drop bare `-` and `--`. Run `env!("CARGO_BIN_EXE_talaria")` (the `[[bin]] name = "talaria"` of `{{P0-02}}`) with `--help`, `env_clear()`, `PATH=/usr/bin:/bin`, the current directory set to the repository root, and `LLVM_PROFILE_FILE` copied onto the child only when the test process has it (`{{P0-02}}` unit specification 9.4: otherwise an instrumented child writes `default.profraw` into its working folder and its lines go uncounted). It must exit 0; otherwise fail with the exit code and the first line of standard error (issue stop condition 12). Read standard output only (clap writes `--help` there; verify on the candidate). A token matches when it occurs in standard output with no letter, digit or hyphen directly before it and none directly after it, so `-c` does not match inside `--check` and `--ver` does not match `--version`. | On the scratch release set, add a line `talaria --no-such-flag` to its `bash` block. |
| 9 | `release_text_is_public_safe` | Always. Text scanned: the `[Unreleased]` section, the first dated section of `CHANGELOG.md`, the file `notes_path()` if it exists, and the lines of this test file whose trimmed text starts with `//`. Path and attribution fragments, matched as plain substrings, each a constant built from two pieces so that neither this file nor the repository's scanners (and the issue validator) flag it: `"/Us" + "ers/"` (macOS home), `"/ho" + "me/"` (Linux home), `"/pri" + "vate/"` and `"/var/fol" + "ders/"` (macOS private and temporary folders), `"Co-auth" + "ored-by"` (commit trailer), `"Generated" + " with"` (generated-content notice). Unattended flag: remove every space and tab from the scanned text, then fail on any of the six strings that make up the five true forms of `{{REL-01}}`'s `no-unattended-flag` gate (the builder form has two): `unattended` + `:true`, `unattended` + `:Some(true)`, `unattended` + `=true`, `.unattended(` + `true)`, `.unattended(` + `Some(true))`, and the JSON form `"unattended"` + `:true`. Unit codes, case-sensitive, on the original text: at each position where the previous character is absent or is not an ASCII letter, digit or hyphen, fail when the text continues with `P`, one digit, `-`, two digits; or `PH`, one digit, optionally one capital letter, optionally `-ACC`; or `REL-`, two digits; and the next character is absent or not an ASCII letter or digit (a following hyphen still matches, so `P2-03-foo` fails). `-ACC` belongs to the `PH` form only. Lowercase codes are not matched on purpose. Checked on 2026-10-22: `[Unreleased]` and the 0.6.5 section hold no match. | Add `see P2-03` under `## [Unreleased]`. |

### 7.2 The scratch final release set (for the red runs of tests 7 and 8)

In a throwaway worktree of the candidate with PR-A merged: set the version to `0.7.0` and run `cargo check --workspace`. Insert below `## [Unreleased]` and its blank line in `CHANGELOG.md`:

```markdown
## [0.7.0] — 2026-12-01

Scratch entry. See the [release notes](https://github.com/infiquetra/talaria/blob/v0.7.0/docs/release-notes/v0.7.0.md).

### Added
- Scratch added line.

### Changed
- Scratch changed line.

### Removed
- Scratch removed line.

```

Change the `[Unreleased]:` line of the reference block to end `compare/v0.7.0...HEAD` and add above the `[0.6.5]:` line `[0.7.0]: https://github.com/infiquetra/talaria/releases/tag/v0.7.0`. Use any real date not earlier than the 0.6.5 date. Write `docs/release-notes/v0.7.0.md` with this text, replacing `<pinned tag>` by the `tag=` value of `crates/hermes-contract/PINNED`:

````markdown
# Talaria v0.7.0

Scratch notes.

## Read this part first

- Scratch.

## What is new

- Scratch.

## What changed from 0.6.5

- See https://github.com/infiquetra/talaria/blob/v0.7.0/docs/migrating-from-0.6.5.md

## Install

```bash
cargo build --release
talaria --version
```

Nothing is published to crates.io, npm or the Python Package Index under the name talaria.

## Works with Hermes

- Pinned: <pinned tag>.

## Staying on 0.6.5

- Scratch.

## Known limits

- Scratch.

## What was checked

- Scratch.
````

If `docs/migrating-from-0.6.5.md` is not on `main` yet, create an empty file of that name in the same throwaway worktree, so test 7 can resolve the link. Run `cargo test -p talaria-cli --test release_metadata`: all nine must pass. If one fails, the set or the test is wrong; fix that before any red run. Then apply each mutation of tests 7 and 8 alone, record the failure, and revert it. The `talaria --version` line needs `--version` in the help output, which a clap-based help lists.

### 7.3 Notes on the tests

- Only tests 5 to 8 are gated on a final version; test 9 always runs. On the version-only edit tests 7 and 8 pass, because a missing notes file is test 6's failure.
- Red runs are a declared departure from roadmap rule 4 ("a test must fail before the change"): the release text these tests guard does not exist yet. Every red run is taken in a throwaway worktree outside the repository and discarded. The PR-A description records, per test: the scratch edit, the failing test name and the first line of the failure, which must name the broken property, never a Cargo parse error or a compile error.
- The test author and the developer are two separate agent sessions; the run declaration names both. The same session in both roles defeats the test-first rule even though every command would pass.
- Test 2 is the most valuable. After the bump, any literal of the old version in a test, fixture or source file would make a test fail, and fixing it would need a change that the allowed-diff rule refuses. Finding it before the freeze costs one small pull request; finding it after costs a new candidate and a full re-run.
- Test 8 uses only flags that the binary's own help lists. It does not run the commands. The notes author reads `target/release/talaria --help` on the candidate before writing any `talaria ` command line.
- Test 9 cannot see every leak (a person's name, a host name). The reviewer's read of the final text and the `doc-scan` gate of `{{REL-01}}` (its script over every tracked `*.md` and `*.txt`, recursively) are the other two nets. The `repo-scan` gate of `{{REL-01}}` scans only the frame recordings under `fixtures/frames`, not documentation. Roadmap rule 7 (no planning codes in source comments) is checked by test 9 only for this unit's own file; other files are the reviewer's.
- Windows is not supported; the tests use `/` and need no path conversion.

## 8. The rehearsal and the retry

Because tags are permanent (2.1) and the release workflow has never run, one rehearsal is recommended and is the operator's decision. Procedure: after the checks of section 3 steps 1 to 4, and only when `git ls-remote --tags origin 'v0.7.0-rc.*'` prints nothing, the operator pushes `v0.7.0-rc.1` pointing at the same `$T` (annotated, message "Talaria v0.7.0 release candidate 1"). Its run is identified the same way as the final one (`--event push --branch v0.7.0-rc.1 --commit "$T"`) and its ID recorded. The operator runs every command that writes to GitHub (the tag push and `gh release edit`). Expected: the workflow succeeds, the release page `Talaria v0.7.0-rc.1` exists and is marked pre-release (`isPrerelease` true), and `gh release edit v0.7.0-rc.1 --notes-file docs/release-notes/v0.7.0.md` followed by the comparison of section 3 step 7 works. What the rehearsal teaches: the tag check, the project check on the hosted runner, the `gh release create` flags, the body comparison, and how `gh run list` names the run. The tag `v0.7.0-rc.1` stays forever, and from then on it is the expected state, not a blocker: the final release proceeds when `git rev-parse 'v0.7.0-rc.1^{commit}'` equals `$T` and the recorded rehearsal run succeeded. A rehearsal tag on another commit stops the release (issue stop condition 6). A failed rehearsal run also stops it until the operator chooses one recovery and writes it on the issue. GitHub runs a push-triggered workflow from the workflow file at the pushed commit, so a fix merged to `main` does not reach a later tag push on `$T`; recoveries: (a) for a runner or network fault, `gh run rerun <id>`; (b) for a workflow defect, merge a pull request that changes only `.github/workflows/release.yml`, then `gh workflow run release.yml -f tag=v0.7.0-rc.1`, and accept that the final tag's push run will fail the same way and be completed by dispatch (below); (c) for a defect in the tagged tree, a new candidate through `{{REL-01}}` and a later rehearsal tag on the new `$T`. A second rehearsal tag on the same `$T` teaches nothing new, because it runs the same workflow file. The pre-release page may be left in place (recommended: it is honest history) or deleted by the operator.

If the final run fails after the tag exists: do not try to move the tag. Read the log (`gh run view <id> --log-failed`) and classify the cause. Runner or network fault (a runner that never started, a timeout fetching a tool, an HTTP 5xx from GitHub): `gh run rerun <id>`, no pull request. Workflow defect (a YAML error, a wrong `gh` flag, a missing permission, a step that reads a removed path): fix it in a pull request that changes only `.github/workflows/release.yml` (it is the owner `{{P0-02}}`'s file; say so on the issue), merge it, then record the time (`date -u +%Y-%m-%dT%H:%M:%SZ`) and run `gh workflow run release.yml -f tag=v0.7.0`. Identify that run with `gh run list --workflow release.yml --event workflow_dispatch --limit 5 --json databaseId,createdAt,headBranch,conclusion`: exactly one entry created after the recorded time (several: stop), record its ID, and watch it. Its `headBranch` is `main`, which is expected. It counts only when it succeeded and `gh run view <id> --log` shows the `resolve` step's tag `v0.7.0` and the checkout of commit `$T` (`gh run view <id> --log | grep -F "$T"` finds the checkout line; the exact log wording of the merged workflow is seen first in the rehearsal and recorded in the closeout). If the cause is in the tagged tree (a test that fails on the hosted runner, a version mismatch), the tag is wrong: stop, report, and the operator decides on `v0.7.1`.

## 9. Closeout comment (post on the REL-03 issue when the release page is verified)

```text
REL-03-CLOSEOUT
Version: 0.7.0 (workspace); `talaria --version` printed: talaria 0.7.0
Candidate: <40 hex>   Tag commit: <40 hex>   Files differing: <k> (ALLOWED_DIFF: OK (<k> files))
RC_RESULT_CI on the tag commit: RC_RESULT_CI: PASSED (...)
GitHub checks on the tag commit: <n> passed, 0 failed
Tag: v0.7.0 (annotated: yes/no) pushed by the operator on <YYYY-MM-DD>
Rehearsal: <v0.7.0-rc.1 run URL and conclusion, or "skipped by the operator's decision">
Release workflow run: <id> <url> (event: push | workflow_dispatch; conclusion: success; released commit: <40 hex>)
Release page: <url>; draft: false; pre-release: false; assets: 0; latest: v0.7.0
Notes body equals docs/release-notes/v0.7.0.md: yes (comparison method: after removing carriage returns and trailing blank lines)
Link check of the notes: <n> links, all 200
PR-A: <url>   PR-B: <url>
Open defect issues named in the notes: <numbers or "none">
UAT-1: <PASS|FAIL|BLOCKED|NOT RUN> <evidence link>
UAT-2: <...> <evidence link>
UAT-3: <...> <evidence link>
UAT-4: <...> <evidence link>
UAT-5: <...> <evidence link>
UAT-6: <...> <evidence link>
```

Rules: no absolute path, user name, host name or profile name other than `default` or `fixture`; a link to the evidence, not the evidence pasted; exactly one line per scenario, written at the start of a line. Evidence lives in comments, as for `{{REL-01}}`: the tester posts each scenario's evidence (pane text from `herdr pane read`, scrubbed, in a fenced block, headed `UAT-<n> evidence <7 hex of $T>`) as its own comment on this issue, and the scenario's `<evidence link>` is that comment's URL, `https://github.com/infiquetra/talaria/issues/<n>#issuecomment-<id>`. The 7 hex in the heading binds the evidence to `$T`; the reviewer checks it.

The check of the issue's AC-12, run against this one comment (`N` is this issue's number):

```bash
gh api "repos/infiquetra/talaria/issues/$N/comments" --paginate --jq '.[] | select(.body | startswith("REL-03-CLOSEOUT")) | .body' > "$X/closeout.txt"
grep -c '^REL-03-CLOSEOUT' "$X/closeout.txt"                       # 1: exactly one closeout comment
grep -oE 'Tag commit: [0-9a-f]{40}' "$X/closeout.txt" | cut -d' ' -f3 # equals $T
grep -E '^UAT-[0-9]+: ' "$X/closeout.txt" | wc -l                  # 6
grep -E '^UAT-[0-9]+: ' "$X/closeout.txt" | cut -d' ' -f1 | sort -u | tr '\n' ' '  # UAT-1: ... UAT-6: (six distinct)
grep -cE '^UAT-[1-6]: PASS https://' "$X/closeout.txt"            # 6
```

All five must hold. Six lines for one scenario, a missing number, or any `FAIL`, `BLOCKED`, `SKIP` or `NOT RUN` verdict fails the criterion.

## 10. Edge cases

| Case | What to do |
|---|---|
| Another pull request merges to `main` between the freeze and PR-B | The lead holds merges after the freeze. If one happened, `allowed-diff` decides: a docs-only merge passes and its commits are named in the closeout (the issue's REQ-6); anything else is a new candidate and `{{REL-01}}` runs again. |
| `rc.sh allowed-diff` exits 1 on the tag commit | Stop. Print the `not allowed:` list on the issue. The cause is either a change in PR-B beyond version, changelog, notes, README and `llms.txt` (move it out) or another merge (new candidate). |
| `Cargo.lock` shows more than version lines after `cargo check` | Another dependency moved. Stop and report; the lock file is never edited by hand (the issue's work surface says it is written by cargo). |
| PR-B was squash-merged or rebased | Allowed. `$T` is still `origin/main` right after the merge; `git log $C..$T` then lists one commit or PR-B's commits without a merge commit. |
| A `{{REL-02}}` page (`docs/guide.md`, `docs/keybindings.md`, `docs/configuration.md`, `docs/migrating-from-0.6.5.md`) states the development version | Report to that issue's owner, who fixes it in a documentation pull request (allowed after the freeze), then re-run section 3 on the new `origin/main`. This issue does not edit those pages. |
| The tag day is not the date in the changelog | Change the heading date (allowed), re-run the section 3 checks on the new tag commit, and ask the operator to confirm again. |
| A newer stable Hermes release appears before the tag | It is a note, not a re-pin (`{{REL-01}}` REQ-11). The notes say which release was pinned and which was newest on the candidate day. |
| `{{P6-07}}` merged before the release | The release workflow may attach binaries and the install section may list them; re-verify the workflow (2.4) and write the install section from the merged documentation page, not from this document. |
| `v0.7.0` exists on origin already | Stop. Never push, never try to replace it. Report which commit it points at. |
| `v0.7.0-rc.1` exists after the rehearsal | Expected. Proceed when it peels to `$T` and its recorded run succeeded (section 8); a check that matches `v0.7.0*` would wrongly stop here, so match `refs/tags/v0.7.0` exactly. |
| Test 4 fails on the candidate because `CHANGELOG.md` links into `docs/acceptance/` or `docs/plans/` | `{{P0-02}}` AC-17 did not hold. PR-A redirects those links to `python-final` (section 2.3) and reports it on `{{P0-02}}`. Any other dead link: stop (issue stop condition 12). |
| The operator wants a release candidate tag only | The sections 3 and 8 apply to `v0.7.0-rc.<n>`; the version in `Cargo.toml` is still `0.7.0`. Do not create a different version string. |
| Release page shows "Latest" on v0.6.5 after the release | A pre-release never becomes Latest. Check `isPrerelease` of the final release; if false and Latest is still v0.6.5, record the output and report: GitHub's rule for "latest" was not verified here. |
| The notes mention a key or a flag the guide does not | The guide and the binary win; fix the notes. |

## 11. Design reasoning and rejected alternatives

- Two pull requests, tests first and merged early. This departs from roadmap rule 1 ("one unit, one branch, one pull request"), so the issue declares it as an exception that the operator approves on the parent `{{REL}}` before PR-A starts. Rejected: one pull request after the freeze (the allowed-diff rule forbids a new test file there); one pull request before the freeze (the changelog date, the notes and the Hermes facts they repeat are known only after the candidate passed); a separate predecessor unit for the tests (it would change the issue inventory, which this unit does not own; the lead may still choose it); no tests, only a checklist (a checklist is what the Python era's release ceremony was; the nine tests are cheap, run in the normal project check, and make every later release safer too).
- Branch names `rel-03-release-checks` and `rel-03-v0-7-0`. Roadmap rule 1 asks for `p<phase>-<nn>-<slug>`; the release slice has no phase number, so `rel-03` stands in that place. The departure is declared in the issue.
- Version in one place. `{{P0-02}}` made the workspace version the single source; a one-line bump keeps the lock file change to version lines that `allowed-diff` accepts.
- Replace the generated notes with `gh release edit` instead of changing the workflow. Changing `.github/workflows/release.yml` is another unit's file and would need a new candidate; the edit is one command and is verified by a body comparison. Rejected: a draft release the operator publishes (one more manual step and a state the workflow does not create).
- The notes live in `docs/release-notes/` and not in `docs/releases/`. `{{P0-02}}` removed `docs/releases/` and re-pointed old links at `python-final`; reusing that name would blur the two eras. One file per release, no index.
- A rehearsal tag. Rejected: skipping it (the workflow has never run); a fork or a dry-run tool (`act` does not reproduce `macos-14` or the repository ruleset). The cost is one permanent pre-release tag, which the operator accepts or declines.
- The operator pushes the tag. The release is outward-facing and irreversible (2.1); the roadmap keeps releases with the operator. The agent prepares the command and verifies the result.

## 12. Things to re-verify at the freeze (the planner's list)

1. `[workspace.package] version` and member inheritance (`{{P0-02}}`); every manifest under `crates/` and `tools/` is a member; the literal old version appears only in allowed files (`git grep -n -F '0.7.0-dev' -- crates tools`).
2. The merged `release.yml`: triggers, inputs, tag check, release step, flags (2.4).
3. The ruleset (2.1), the existing releases (2.2), the permission of the operator to push tags.
4. `rc.sh allowed-diff` and `rc.sh run --class ci` as merged (2.6), and the closeout comment of `{{REL-01}}`.
5. The shape of `CHANGELOG.md` after all phases (2.3), including that no relative link points into a removed folder and that `[Unreleased]` holds no planning code; the migration page and guide as merged (`{{REL-02}}`).
8. That `talaria --help` exits 0 with standard output piped and no terminal (`{{P1-12}}`).
9. The operator's approval of the two-pull-request exception on `{{REL}}`.
10. That this document is on `main` at `docs/handoff/2026-10-22-next-steps/units/REL-03.md` (on 2026-10-22 that folder held only the phase 0 and phase 1 documents); if not, stop.
11. The CI check names in the merged `ci.yml`, the `defect` label, and that `gh` is signed in for the agent's read-only checks.
6. Hermes: the pinned tag in `crates/hermes-contract/PINNED`; the three source lines of 2.5 at the pinned tag; the minimum release statement.
7. Whether `{{P6-07}}` merged.
