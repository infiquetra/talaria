#!/bin/sh
# Fake Herdr for the pane reporting tests (issue 228). Appends every argument, one
# per line, then a `--end--` line, to `$FAKE_HERDR_LOG`; sleeps `$FAKE_HERDR_SLEEP`
# seconds when set; exits `$FAKE_HERDR_EXIT` (default 0).
set -u
log="${FAKE_HERDR_LOG:?FAKE_HERDR_LOG is not set}"
for arg in "$@"; do
  printf '%s\n' "$arg" >>"$log"
done
printf '%s\n' '--end--' >>"$log"
sleep_time="${FAKE_HERDR_SLEEP:-0}"
if [ "$sleep_time" != "0" ]; then
  sleep "$sleep_time"
fi
exit "${FAKE_HERDR_EXIT:-0}"
