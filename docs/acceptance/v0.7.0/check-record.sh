#!/bin/sh
# Check the Phase 1 acceptance record. Usage: check-record.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-1.md}
EVI=${2:-$DIR/phase-1-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-record: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|P1-03 UAT-1|P1-05 UAT-3|P1-06 UAT-1|P1-06 UAT-2|P1-06 UAT-3|P1-06 UAT-4|P1-06 UAT-5|P1-06 UAT-6|P1-07 UAT-1|P1-07 UAT-2|P1-07 UAT-3|P1-07 UAT-4|P1-07 UAT-5|P1-07 UAT-6|P1-08 UAT-1|P1-08 UAT-2|P1-08 UAT-3|P1-08 UAT-4|P1-08 UAT-5|P1-09 UAT-1|P1-09 UAT-2|P1-09 UAT-3|P1-09 UAT-4|P1-09 UAT-5|P1-09 UAT-6|P1-09 UAT-7|P1-10 UAT-1|P1-10 UAT-2|P1-10 UAT-3|P1-10 UAT-4|P1-10 UAT-5|P1-11 UAT-1|P1-11 UAT-2|P1-11 UAT-3|P1-11 UAT-4|P1-11 UAT-5|P1-12 UAT-1|P1-12 UAT-2|P1-12 UAT-3|P1-12 UAT-4|P1-12 UAT-5|P1-12 UAT-6|P1-12 UAT-7|P1-12 UAT-8|P1-13 UAT-1|P1-13 UAT-2|P1-13 UAT-3|P1-13 UAT-4|P1-13 UAT-5|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|OP-1|OP-2|OP-3|OP-4|OP-5|OP-6'
awk -v ids="$IDS" -v evi="$EVI" -v pathbad="$PATHS" '
function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
function slug(s) { s = tolower(s); gsub(/ /, "-", s); return s }
function bad(msg) { print msg; problems++; rowbad = 1 }
function iscand(s) { return length(s) == 40 && s ~ /^[0-9a-f]+$/ }
BEGIN {
  n = split(ids, list, "|"); for (i = 1; i <= n; i++) want[list[i]] = 1
  fence = "^\140\140\140"
  while ((getline line < evi) > 0) {            # evidence: "## <id>", a Candidate: line, then fenced blocks with text
    if (line ~ fence) { infence = !infence; if (infence) blk = 0; continue }
    if (infence) { if (cur == "") continue; blk++; if (blk > longest[cur]) longest[cur] = blk; if (line ~ /[^ \t]/) text[cur]++; continue }
    if (line ~ /^## /) { cur = trim(substr(line, 4)); sect[cur] = 1; nsect[cur]++ }
    else if (cur != "" && line ~ /^Candidate: /) { split(trim(substr(line, 12)), t, /[. ]/); evcand[cur] = t[1] }
  }
}
/^Candidate: / { cand = trim(substr($0, 12)) }
/^(Hermes|Herdr|OS|Date): / { k = $0; sub(/: .*/, "", k); hdr[k] = trim(substr($0, length(k) + 3)) }
/^\|/ {
  m = split($0, c, "|"); id = trim(c[2]); rowbad = 0
  if (id == "id" || id ~ /^-+$/) next
  if (!(id in want)) { bad("unexpected row: " id); next }
  if (m != 7) { bad(id ": the row needs five cells"); next }
  if (id in seen) { bad(id ": appears twice"); next }
  seen[id] = 1; result = trim(c[4]); evid = trim(c[5]); commit = trim(c[6])
  if (result !~ /^(PASS|FAIL|NOT RUN|BLOCKED \([^)]+\))$/) { bad(id ": the result must be PASS, FAIL, BLOCKED (cause) or NOT RUN, not: " result); next }
  if (result != "PASS") { bad(id ": " result); next }
  if (iscand(cand) && (length(commit) < 7 || index(cand, commit) != 1)) bad(id ": commit " commit " is not the candidate")
  if (evid != "phase-1-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-1-evidence.md#" slug(id))
  if (!(id in sect) || text[id] < 1) bad(id ": the evidence file has no section with a fenced block holding text for it")
  else if (longest[id] > 60) bad(id ": a fenced block of the evidence is longer than 60 lines")
  if (id in sect && iscand(cand) && (length(evcand[id]) < 7 || index(cand, evcand[id]) != 1)) bad(id ": the evidence section must name the candidate (Candidate: <7 hex>.)")
  if (!rowbad) passes++
}
END {
  if (!iscand(cand)) bad("the line Candidate: <40 hex characters> is missing")
  for (k in hdr) if (hdr[k] ~ /[<>]/) bad(k ": the header line is still a placeholder")
  if (!("Hermes" in hdr) || !("Herdr" in hdr)) bad("the header lines Hermes: and Herdr: are missing")
  if (hdr["OS"] !~ /^(macos|linux)$/) bad("OS: must be macos or linux")
  if (hdr["Date"] !~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) bad("Date: must be YYYY-MM-DD")
  for (i = 1; i <= n; i++) if (!(list[i] in seen)) bad(list[i] ": row missing")
  for (k in nsect) if (nsect[k] > 1) bad(k ": the evidence file has " nsect[k] " sections for it")
  if (pathbad > 0) problems++
  if (problems == 0) printf "ACCEPTANCE_RESULT: PASSED (%d of %d rows PASS, candidate %s)\n", passes, n, substr(cand, 1, 7)
  else printf "ACCEPTANCE_RESULT: FAILED (%d problems, %d of %d rows PASS)\n", problems, passes, n
  exit (problems == 0) ? 0 : 1
}' "$REC"
