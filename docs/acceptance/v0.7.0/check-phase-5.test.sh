#!/bin/sh
# Tests for check-phase-5.sh. Run: sh docs/acceptance/v0.7.0/check-phase-5.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-phase-5.sh"
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
C=0123456789abcdef0123456789abcdef01234567
S=$(echo "$C" | cut -c1-7)
IDS=$(sed -n "s/^IDS='\(.*\)'\$/\1/p" "$CHECK" | tr '|' '\n')
fails=0

# make_record <file> <id-to-change> <result> <commit>   (an unchanged record has every row PASS on the candidate)
make_record() {
  { printf 'Candidate: %s\nHermes: v0.22.4\nHerdr: 0.9.4\nOS: macos\nDate: 2026-11-02\n\n' "$C"
    printf '| id | persona | result | evidence | commit |\n|---|---|---|---|---|\n'
    echo "$IDS" | while IFS= read -r id; do
      slug=$(echo "$id" | tr 'A-Z ' 'a-z-'); res=PASS; com=$(echo "$C" | cut -c1-12)
      [ "$id" = "$2" ] && { res=$3; [ -n "$4" ] && com=$4; }
      echo "| $id | developer | $res | phase-5-evidence.md#$slug | $com |"
    done; } > "$1"
}
section() {   # section <id> <block text lines>: one evidence section with a runner line and a fenced block
  printf '## %s\n\nCandidate: %s. Runner: test.\n\n```text\n%s\n```\n\n' "$1" "$S" "$2"
}
make_evidence() {   # XF-6's block starts with a line that looks like a heading: it must not end the section
  echo "$IDS" | while IFS= read -r id; do
    if [ "$id" = "$2" ]; then :; elif [ "$id" = "XF-6" ]; then section "$id" "$(printf '## not a heading\nok')"; else section "$id" ok; fi
  done > "$1"
}
expect() {   # expect <name> <expected exit> <expected text> <record> <evidence>
  out=$(sh "$CHECK" "$4" "$5"); code=$?
  if [ "$code" = "$2" ] && echo "$out" | grep -qF -- "$3"; then echo "CASE $1: ok"; else echo "CASE $1: FAILED (exit $code; wanted $2 and '$3'; got: $out)"; fails=$((fails+1)); fi
}

make_evidence "$T/ev.md" none
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (84 of 84 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "P5-09 UAT-6" "NOT RUN";               expect not-run-fails 1 "P5-09 UAT-6: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-5" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-5: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P5-06 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| OP-1' "$T/good.md" > "$T/f.md";            expect row-missing 1 "OP-1: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P5-08 UAT-6";               expect no-evidence-section 1 "P5-08 UAT-6: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-5-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
{ cat "$T/ev.md"; printf 'see /%s/someone/project\n' Users; } > "$T/ev3.md"; expect local-path 1 "a local path appears" "$T/good.md" "$T/ev3.md"
expect missing-file 2 "not found" "$T/nope.md" "$T/ev.md"
make_evidence "$T/ev4.md" "AUTO-2"; section AUTO-2 "" >> "$T/ev4.md"; expect empty-evidence-block 1 "AUTO-2: the evidence file has no section" "$T/good.md" "$T/ev4.md"
make_evidence "$T/ev5.md" "AUTO-2"; section AUTO-2 "$(seq 1 61)" >> "$T/ev5.md"; expect long-evidence-block 1 "AUTO-2: a fenced block of the evidence is longer than 60 lines" "$T/good.md" "$T/ev5.md"
make_record "$T/k.md" "AUTO-3" PASS 012345;            expect short-commit 1 "AUTO-3: commit 012345 is not the candidate" "$T/k.md" "$T/ev.md"
{ cat "$T/good.md"; printf 'see /%s/someone/project\n' tmp; } > "$T/l.md"; expect local-path-in-record 1 "a local path appears" "$T/l.md" "$T/ev.md"
sed 's/^Hermes: .*/Hermes: <pinned release tag>/' "$T/good.md" > "$T/m.md"; expect placeholder-header 1 "Hermes: the header line is still a placeholder" "$T/m.md" "$T/ev.md"
sed 's/^Candidate: 0123456\./Candidate: deadbee./' "$T/ev.md" > "$T/ev6.md"; expect evidence-names-another-candidate 1 "AUTO-1: the evidence section must name the candidate" "$T/good.md" "$T/ev6.md"
make_evidence "$T/ev7.md" none; section AUTO-2 ok >> "$T/ev7.md"; expect duplicate-section 1 "AUTO-2: the evidence file has 2 sections for it" "$T/good.md" "$T/ev7.md"
sed 's/^Herdr: .*/Herdr: /' "$T/good.md" > "$T/n.md"; expect empty-header-value 1 "Herdr: the header line has no value" "$T/n.md" "$T/ev.md"
sed '$d' "$T/ev.md" | sed '$d' > "$T/ev8.md"; expect unclosed-fence 1 "the evidence file ends inside an unclosed fenced block" "$T/good.md" "$T/ev8.md"
{ grep -v '^Candidate: ' "$T/good.md"; echo "Candidate: $C"; } | sed 's/| [0-9a-f]\{12\} |$/| deadbeefdead |/' > "$T/o.md"; sed "s/^Candidate: $S\./Candidate: deadbee./" "$T/ev.md" > "$T/ev9.md"; expect late-candidate-header 1 "the line Candidate: must appear once, above the table" "$T/o.md" "$T/ev9.md"
make_evidence "$T/ev10.md" "AUTO-2"; { section AUTO-2 ok; printf '```text\n \n```\n\n'; } >> "$T/ev10.md"; expect empty-block-after-text 1 "AUTO-2: a fenced block of the evidence holds no text" "$T/good.md" "$T/ev10.md"
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-5.md" && cp "$T/ev.md" "$T/d/phase-5-evidence.md"
out=$(sh "$T/d/check-phase-5.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

awk '
  $0 == "## AUTO-4" {
    print
    print ""
    print "Candidate: deadbee. Runner: test."
    print ""
    print "```text"
    print "earlier"
    print "```"
    print ""
    next
  }
  { print }
' "$T/ev.md" > "$T/ev11.md"
expect early-run-other-candidate 1 "AUTO-4: the evidence section must name the candidate" "$T/good.md" "$T/ev11.md"

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
