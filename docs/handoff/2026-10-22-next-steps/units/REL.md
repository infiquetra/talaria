# REL: Talaria v0.7.0 release: integrated regression, documentation, migration notes and tag

Status: `proposed`. The GitHub issue for this capability is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

The capability issue says what the release slice must deliver, who decides what, and which contradictions between the children the planner settles first. This document holds what a lead or reviewer would otherwise have to invent: the exact ruling lines and their deadlines (section 2), the full verification script behind the issue's acceptance criteria (section 3), the closeout template (section 4), the two recovery rules the parent adds to the children (section 5), notes for each settlement C-1 to C-8 (section 6), and the planning checks in detail (section 7). The children's own specifications are [REL-01.md](REL-01.md), [REL-02.md](REL-02.md) and [REL-03.md](REL-03.md). Handoff sources: [the roadmap's rules for every unit](../07-roadmap-and-work-units.md#rules-for-every-unit), [non-goals](../01-vision-and-requirements.md#non-goals), [performance budgets](../05-architecture.md#performance-budgets), [decisions the operator owns](../08-risks-and-open-questions.md#decisions-the-operator-owns), [ADR 0012](../../../../platform-specs/04-architecture/adrs/0012-verification-is-proportionate.md).

## 1. The slice in plain terms

Three children. `REL-01` freezes one candidate commit and runs every gate on it, then the operator drives the build for about 30 minutes. `REL-02` writes the user pages and the migration page, with tests that read the merged code. `REL-03` adds nine release-text tests before the freeze (pull request A), then the version, changelog and notes after the candidate passed (pull request B), and prepares the tag the operator pushes. `REL-01` is the end-to-end slice.

The issue set creates the three child cards with these exact titles from its inventory; `AC-1` checks the `(unit REL-0<n>)` suffix:

- `Release candidate: integrated regression, replay suites and performance budgets on one commit (unit REL-01)`
- `User documentation: guide, keybinding and configuration reference, and migration from 0.6.5 (unit REL-02)`
- `Version, changelog, tag and release notes for v0.7.0 (unit REL-03)`

## 2. Rulings: exact lines, values and deadlines

The run declaration names the operator's GitHub account. It is used as `$OP` in section 3 and is never written into the issue, a comment or a file. Each ruling is its own comment on the capability issue, posted from that account, whose first line is exactly one of the accepted lines below. When the operator posts the same ruling twice, the latest comment counts and the earlier one is listed in the closeout as superseded. An edited ruling counts at its `updated_at` time.

| Ruling | Accepted first line | Deadline (the ruling's `updated_at` must be earlier) | A value that stops the run |
|---|---|---|---|
| D-1 | `Ruling D-1: evidence in comments` | the creation of the first child pull request (any branch `rel-01-*`, `rel-02-*`, `rel-03-*`) | any other value (the children are then re-planned) |
| D-2 | `Ruling D-2: approved` or `Ruling D-2: refused` | the creation of `REL-03` pull request A (branch `rel-03-release-checks`) | `refused`: `REL-03` is re-planned before anything else |
| D-3 | `Ruling D-3: ship none` or `Ruling D-3: ship #<n>[ #<m>...]` (every open defect named in `RC-CLOSEOUT`) or `Ruling D-3: block` | the creation of `REL-03` pull request B (branch `rel-03-v0-7-0`) | `block`; a `ship` list that omits an open defect |
| D-4 | `Ruling D-4: rehearse` or `Ruling D-4: skip` | the tagger date of the annotated tag `v0.7.0` | none; both are valid |
| merge (optional) | `Ruling merge: granted` | before the first merge an agent makes | absent: the operator merges |

The first child pull request is a proxy for "before any child starts" because a branch with no pull request leaves no timestamp the reviewer can read; the lead also posts `Base:` before any branch (the issue's planning step 0), which a reviewer can compare by hand.

## 3. Verification script

Run in a fresh worktree of `origin/main` outside the primary checkout, with `bash`, `gh` signed in, `jq` and `git`. Every `# AC-<n>` comment names the acceptance criterion its lines check. A line marked "prints nothing" fails the criterion when it prints anything. `$OP` and `$LEAD` come from the run declaration; `Base:`, `Settled` and `## Closeout` comments count only from those two accounts. The `RC-CLOSEOUT` lines parsed below are those of `REL-01` spec section 9: `Candidate: <40 hex>`, `RC_RESULT: PASSED (...)`, `Linux CI run: <run url> ...` and `Operator drive: <comment url>`; planning step 3 confirms that template still uses them (section 7). `REL-03-CLOSEOUT` is not parsed here: `REL-03` AC-12 and AC-13 check it, and the tag commit comes from git.

```bash
set -o pipefail
R=infiquetra/talaria
PARENT=<capability issue number>; REL01=<REL-01 number>; REL02=<REL-02 number>; REL03=<REL-03 number>
OP=<the operator's GitHub login, from the run declaration>
LEAD=<the lead's GitHub login, from the run declaration; may equal OP>
X=$(mktemp -d)
OC=".[] | select(.user.login==\"$LEAD\" or .user.login==\"$OP\")"

# AC-1: membership is exactly the three resolved children
for p in "$REL01:REL-01" "$REL02:REL-02" "$REL03:REL-03"; do
  gh issue view "${p%%:*}" -R $R --json title --jq .title | grep -q "(unit ${p#*:})$" || echo "FAIL: ${p%%:*} is not ${p#*:}"; done   # prints nothing
diff <(gh api repos/$R/issues/$PARENT/sub_issues --jq '.[].number' | sort -n) <(printf '%s\n' $REL01 $REL02 $REL03 | sort -n) && echo "members ok"

# AC-2: each child closed as completed
for n in $REL01 $REL02 $REL03; do gh issue view $n -R $R --json state,stateReason --jq '.state + " " + .stateReason'; done   # CLOSED COMPLETED three times

# AC-3: C-1 to C-8 settled before the first child pull request
gh api repos/$R/issues/$PARENT/comments --paginate --jq "$OC | .body" | grep -oE '^Settled C-[1-8]:' | sort -u | wc -l   # 8
S=$(gh api repos/$R/issues/$PARENT/comments --paginate --jq "$OC"' | select(.body|test("^Settled C-")) | .created_at' | sort | tail -1)
F=$(gh pr list -R $R --state all --limit 200 --json headRefName,createdAt --jq '.[] | select(.headRefName|test("^rel-0[1-3]-")) | .createdAt' | sort | head -1)
[ -n "$S" ] && [ -n "$F" ] && [[ "$S" < "$F" ]] && echo "settled first" || echo "FAIL: settled '$S', first pull request '$F'"

# AC-4: the one RC-CLOSEOUT names the candidate, its complete passing run and its Linux run
gh api repos/$R/issues/$REL01/comments --paginate --jq '.[] | select(.body|startswith("RC-CLOSEOUT")) | .body' > $X/rc.txt
grep -c '^RC-CLOSEOUT' $X/rc.txt                                                     # 1
CAND=$(sed -nE 's/^Candidate: ([0-9a-f]{40})$/\1/p' $X/rc.txt); echo "$CAND" | grep -cxE '[0-9a-f]{40}'   # 1
sed -nE "s/^RC_RESULT: PASSED \(([0-9]+) of ([0-9]+) required gates pass, [0-9]+ informational gates not passing, commit ${CAND:0:7}\)$/\1 \2/p" $X/rc.txt | awk '$1==$2 && $1>0 {print "run ok"}'   # run ok
LR=$(sed -nE 's#^Linux CI run: https://github.com/infiquetra/talaria/actions/runs/([0-9]+).*#\1#p' $X/rc.txt)
gh run view "$LR" -R $R --json headSha,conclusion,event --jq '"\(.headSha) \(.conclusion) \(.event)"'   # <CAND> success workflow_dispatch

# AC-5: the drive comment that RC-CLOSEOUT links: the operator's, this candidate, six yes, PASS
DC=$(sed -nE 's#^Operator drive: https://github.com/infiquetra/talaria/issues/[0-9]+\#issuecomment-([0-9]+).*#\1#p' $X/rc.txt)
gh api repos/$R/issues/comments/$DC --jq .user.login | grep -cxF "$OP"                 # 1
gh api repos/$R/issues/comments/$DC --jq .body > $X/drive.txt
head -1 $X/drive.txt | grep -cxE "RC drive ${CAND:0:7}"                              # 1
grep -ciE ': *yes$' $X/drive.txt; grep -ciE ': *no$' $X/drive.txt                   # 6, then 0
grep -cE '^RC drive: ' $X/drive.txt; grep -cxF 'RC drive: PASS' $X/drive.txt       # 1, then 1

# AC-6 to AC-9: on the tag, in a worktree of v0.7.0
git fetch origin --tags; W=$X/v070; git worktree add --detach $W v0.7.0; cd $W
git cat-file -t v0.7.0                                                               # tag
TAGC=$(git rev-parse 'v0.7.0^{commit}')
sh tools/release/rc.sh allowed-diff "$CAND" "$TAGC"; echo "exit=$?"                  # ALLOWED_DIFF: OK (<k> files), exit=0
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace && cargo test -p talaria-cli --test docs --test release_metadata
cargo metadata --no-deps --format-version 1 | jq -c '[.packages[].publish] | unique'   # [[]]
gh release view v0.7.0 -R $R --json tagName,isDraft,isPrerelease --jq '"\(.tagName) \(.isDraft) \(.isPrerelease)"'   # v0.7.0 false false
gh api repos/$R/releases/latest --jq .tag_name                                       # v0.7.0

# The one parent closeout comment (AC-10, AC-11, AC-13)
gh api repos/$R/issues/$PARENT/comments --paginate --jq "$OC"' | select(.body|startswith("## Closeout")) | .body' > $X/cl.txt
grep -c '^## Closeout' $X/cl.txt                                                     # 1

# AC-10: exactly the nineteen scenario keys, once each, PASS, on the right commit, with a link
{ for i in 1 2 3 4 5 6; do echo "REL-01 UAT-$i"; done; for i in 1 2 3 4 5 6 7; do echo "REL-02 UAT-$i"; done
  for i in 1 2 3 4 5 6; do echo "REL-03 UAT-$i"; done; } | sort > $X/uat.want
sed -nE 's/^\| (REL-0[1-3] UAT-[0-9]+) \|.*/\1/p' $X/cl.txt | sort > $X/uat.got
uniq -d $X/uat.got; diff $X/uat.want <(sort -u $X/uat.got)                           # both print nothing
grep -E '^\| REL-0[1-3] UAT-' $X/cl.txt | awk -F'|' -v c="${CAND:0:7}" -v t="${TAGC:0:7}" '{for(i=2;i<=5;i++)gsub(/^ +| +$/,"",$i)
  want=($2 ~ /^REL-03/) ? t : c; if($2=="REL-02 UAT-7") want=($4 ~ /^[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]$/) ? $4 : "7 hex"
  if($3!="PASS" || $4!=want || $5 !~ /^https:\/\/github\.com\/infiquetra\/talaria\/(issues|pull)\/[0-9]+#issuecomment-[0-9]+$/) print "FAIL: " $2}'   # prints nothing

# AC-11: exactly the ledger's L-1 to L-43, once each, with the ledger's owner, PASS and a link
gh issue view $PARENT -R $R --json body --jq .body | sed -nE 's/^\| (L-[0-9]+) \| .* \| (REL-0[1-3]) \|$/\1 \2/p' | sort > $X/led.want
sed -nE 's/^\| (L-[0-9]+) \| (REL-0[1-3]) \| https:[^ |]+ \| PASS \|$/\1 \2/p' $X/cl.txt | sort > $X/led.got
wc -l < $X/led.want                                                                  # 43
cut -d' ' -f1 $X/led.got | uniq -d; diff $X/led.want $X/led.got                     # both print nothing
grep -E '^\| L-[0-9]+ \|' $X/cl.txt | grep -vcE '^\| L-[0-9]+ \| REL-0[1-3] \| https:[^ |]+ \| PASS \|$'   # 0

# AC-12: each ruling is the operator's, has an accepted value, and came before its deadline (section 2)
RL() { gh api repos/$R/issues/$PARENT/comments --paginate --jq ".[] | select(.user.login==\"$OP\" and (.body|startswith(\"Ruling D-$1:\"))) | [.created_at, .updated_at, (.body|split(\"\n\")[0])] | @tsv" | sort | tail -1; }
PRT() { gh pr list -R $R --state all --head "$1" --json createdAt --jq '.[0].createdAt // empty'; }
TAGT=$(TZ=UTC git for-each-ref --format='%(taggerdate:format-local:%Y-%m-%dT%H:%M:%SZ)' refs/tags/v0.7.0)
chk() { r=$(RL $1); u=$(printf '%s' "$r" | cut -f2); l=$(printf '%s' "$r" | cut -f3)
  printf '%s\n' "$l" | grep -qxE "$2" || echo "FAIL: D-$1 value '$l'"
  [ -n "$u" ] && [ -n "$3" ] && [[ "$u" < "$3" ]] || echo "FAIL: D-$1 late or missing ('$u' against '$3')"; }
chk 1 'Ruling D-1: evidence in comments' "$F"
chk 2 'Ruling D-2: approved' "$(PRT rel-03-release-checks)"
chk 3 'Ruling D-3: ship (none|#[0-9]+( #[0-9]+)*)' "$(PRT rel-03-v0-7-0)"
chk 4 'Ruling D-4: (rehearse|skip)' "$TAGT"                                          # the four chk lines print nothing
```

Then check by hand: every defect issue `RC-CLOSEOUT` names as open appears in the `Ruling D-3: ship` list; open the evidence links of `REL-01` UAT-1 and `REL-02` UAT-4 and confirm their pane text shows the stated result on the stated commit; read `$X/rc.txt` and `$X/cl.txt` for absolute paths, user or host names and profile names other than `default` and `fixture`.

Negative self-test for a reviewer: copy `$X/cl.txt`, then (1) duplicate one scenario row, (2) change one owner in a ledger row, (3) change one scenario result to `BLOCKED`, (4) set the `REL-02 UAT-7` commit cell to `abc`. Re-running the AC-10 and AC-11 lines on the copy must print a `FAIL` or a diff line for each.

## 4. Closeout template

Post one comment on the capability issue, starting with the line `## Closeout`:

```text
## Closeout
Base: <40 hex>   Candidate: <40 hex>   Tag commit: <40 hex>
Hermes: pinned <tag>; newest stable <tag>
RC-CLOSEOUT: <comment url>   REL-03-CLOSEOUT: <comment url>
Pull requests: REL-01 <url> (merge <7 hex>); REL-02 <url> (merge <7 hex>); REL-03 A <url>, B <url>
Release run: <id> <url>   Release page: <url>
Settled: C-1 <decision>; ... C-8 <decision>
Rulings: D-1 <value>; D-2 <value>; D-3 <value>; D-4 <value>; superseded: <comment urls or none>

| scenario | result | commit | evidence |
|---|---|---|---|
| REL-01 UAT-1 | PASS | <candidate 7 hex> | <comment url> |
... one row per scenario: REL-01 UAT-1 to UAT-6 and REL-02 UAT-1 to UAT-6 on the candidate,
    REL-02 UAT-7 on the commit it ran on, REL-03 UAT-1 to UAT-6 on the tag commit

| row | owner | evidence | result |
|---|---|---|---|
| L-1 | REL-01 | <url> | PASS |
... one row per ledger row L-1 to L-43, owner as in the issue's ledger

- Journal: <heading>   (one line per entry that landed)
```

Evidence links point at a pull request or issue comment (`.../issues/<n>#issuecomment-<id>` or `.../pull/<n>#issuecomment-<id>`), never at pasted evidence.

When the disposition is not planned, the lead records each failing criterion in `docs/engineering-journal/QUEUED.md` (the file exists on `main`) through a documentation pull request to `main` opened after the operator's ruling and merged under `Ruling merge: granted` or by the operator. One entry per criterion, in the file's existing shape, under the `## P<n>` heading the operator names:

```text
### Talaria v0.7.0 release: AC-<n> failed (<one line>)

**Priority.** P<n>: <why, one line>.

**Evidence.** <closeout or comment url>.

**Worth it when.** <the condition for the next attempt, for example "the fix in #<n> has merged">.
```

The closeout lists the pull request as `- Queued: <url>`.

## 5. Recovery rules the parent adds

These qualify the issue's settled decision 1. Both are already what the children say; the parent states them so its precedence does not remove them.

1. **Environment failure on the candidate.** A gate that is not PASS because of the environment (network, `gh`, Herdr, the gateway down, a missing tool; `REL-01` REQ-2 lists the exit codes) is recorded as `BLOCKED (environment)`, not as a defect. After the cause is removed, the whole run is repeated on the same candidate from `REL-01` runbook step 4 (spec section 11). The blocked attempt stays in the evidence. Only a later complete run with `RC_RESULT: PASSED` counts. The same applies to a Linux dispatch that failed on a runner or network fault: a new dispatch on the same commit. A product failure is never re-run on the same commit; it needs the owning unit's fix and a new candidate.
2. **Release workflow repair after a tag exists.** The freeze governs the difference between the candidate and the tag commit, which `rc.sh allowed-diff` checks. It does not govern a later change to `main` that leaves the tagged tree untouched. If the rehearsal tag's run or the final tag's run fails because of a defect in `.github/workflows/release.yml`, `REL-03` REQ-10 and its spec section 8 apply: a pull request that changes only that file, then the operator's `gh workflow run release.yml -f tag=v0.7.0`, whose log must show the tag and the unchanged tag commit. No tag moves, and the candidate does not restart. A failure in the tagged tree (a build, test or version check) stops the release; the operator decides on `v0.7.1`.

## 6. Settlement notes, C-1 to C-8

- C-1 to C-5: as the issue's Stop conditions state; each edit is small and lands in the named child body and, where the child's specification repeats the text, there too.
- C-4, rerun evidence. The agent tester posts each candidate rerun of `REL-02` UAT-1 to UAT-6 as its own comment on the `REL-02` issue; those are the evidence links of the six `REL-02` rows in the closeout. Pull-request-head runs stay in the `REL-02` pull request for its review and do not count for the closeout.
- C-6, serial live tests. `cargo test -- --ignored` runs the tests of one test binary on several threads unless `--test-threads=1` is given, so two live tests in one file can talk to the shared gateway at once. Edit `REL-01` spec section 4.1's `live-tests` command to add `--test-threads=1` after `--`, and add that string to the command shapes case 16 checks (its spec section 6, "16 parses the real `gates.tsv`"). Also confirm from [`P1-13`](https://github.com/infiquetra/talaria/issues/210)'s merged `smoke` code that one suite drives one session at a time; if a suite overlaps sessions on purpose (a fleet suite does), name it in `tools/release/README.md` as an intended exception, since the run as a whole still holds one live activity at a time.
- C-7, the drive comment's answer lines. `REL-01` REQ-10 asks for "yes or no for each of" six questions without a line format, and section 3's AC-5 lines count lines ending in `: yes`. Edit `REL-01` REQ-10 and its spec section 9 to fix the format: one line per question, `<question>: yes` or `<question>: no`, then the verdict line.
- Not a settlement: `REL-02` UAT-6 runs `talaria doctor` without `--no-ensure`. That is correct: [`P4-10`](https://github.com/infiquetra/talaria/issues/254) REQ-4 makes the doctor's service always use `EnsurePolicy::Never`, so it never runs `hermes gateway ensure`. The parent's decision 9 names this exception.
- Not a settlement: `REL-03` spec section 3 step 3 matches check runs with `grep -qF "$n<TAB>completed<TAB>success"`. The file holds real tab characters (checked with `od -c`), so the match is correct as written.
- C-8, the `rc-linux` dispatch. The parent's decision 7 and Authority boundaries keep every `gh workflow run` with the operator. `REL-01` as drafted already says so: REQ-1 step 4, its spec section 7 ("Who runs it") and its spec section 11 (the measurement dispatch on the tooling branch in step 2, the candidate's run in step 7, and step 10's repeat from step 4 after an environment failure). The lead posts on the `REL-01` issue the exact command of its spec section 7 and the 7-hex commit; when `main` has moved past the candidate the lead first pushes a branch at the candidate and names it in `--ref`; the operator runs the command; the lead finds and checks the run with the read-only `gh run list` and `gh run view` lines of spec section 7. The planner confirms that text against the merged workflow and comments `Settled C-8:`; an edit is needed only where they differ.

## 7. Planning checks in detail

- Step 0. The issue set commits every unit specification under `docs/handoff/2026-10-22-next-steps/units/` with the handoff before it creates the issues, so the four links resolve when this issue exists. If one is missing, the lead posts `Stop: units/<file> missing` on the capability issue and starts nothing. Agents in the run do not hold the drafts, so the operator restores the file by a documentation pull request; planning restarts at step 0 and comments the new commit.
- Step 3, consumed names most at risk: the suites `smoke run --help` lists; `record-fixture replay` and `scan`; `contract_gen.py check --require-typify`, `verify-vendor`, `drift --tag`; `talaria-bench run`; `live_session.rs::launch_to_usable_composer_within_budget`; `live_subagents.rs::live_pause_blocks_new_spawns`; `load_file`, `KeyMap::from_entries`, `Action::ALL`, `KeyChord::label`; the notice order of `PH1B` decision C-3; the `ci.yml` check names `rust-check (macos-14)`, `rust-check (ubuntu-latest)`, `rust-coverage`; `release.yml` as `REL-03` spec 2.4 says. Also the `RC-CLOSEOUT` template in `REL-01` spec section 9: its lines `Candidate:`, `RC_RESULT: PASSED (<p> of <n> required gates pass, <k> informational gates not passing, commit <7 hex>)`, `Linux CI run:` and `Operator drive:` must stay exactly as section 3 parses them; a difference is an edit to `REL-01` spec section 9, never to section 3.
- Step 4, Hermes facts to re-read when `PINNED` names a tag other than v0.22.4: `--tui` and `--native` (alias `--tui-native`) in `hermes_cli/_parser.py` (lines 208, 210 at v0.22.4); the check in `hermes_cli/main.py` (line 297); the `unattended` flag in `gateway/session_finite.py`; `display.skin` and `skin.changed`; `gateway/session_authority.py` present (first at v0.22.3).
- Step 5. The planner names each merged phase 6 unit in the planning comment and edits the affected child bodies (and their specifications where they repeat the text) before any lane starts, commenting why, as for a settlement.
- Step 7. Each child's "Work surface and collision custody" section names the paths it edits and the paths it creates. An edited path missing on `main`, or a created path already present, is an edit to that child before its lane starts.
