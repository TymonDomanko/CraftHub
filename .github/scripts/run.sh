#!/usr/bin/env bash
# Run a command; if it fails, also report its errors as a GitHub annotation so they show
# on the run's summary page (and through the API) without opening the full log.
#   run.sh <title> <command...>
set -uo pipefail
title="$1"
shift
log="$(mktemp)"
export CARGO_TERM_COLOR=never
"$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
if [ "$status" -ne 0 ]; then
  # Compiler errors with context, or else the end of the output.
  details=$(sed 's/\x1b\[[0-9;]*m//g' "$log" | grep -v -E '^ *(Compiling|Checking|Downloaded|Downloading) ' \
    | grep -E -A8 '^(error|warning)(\[|:)|FAIL|panicked' | head -c 3500)
  [ -n "$details" ] || details=$(sed 's/\x1b\[[0-9;]*m//g' "$log" | tail -c 3000)
  details=${details//'%'/'%25'}
  details=${details//$'\r'/}
  details=${details//$'\n'/'%0A'}
  echo "::error title=$title::$details"
fi
rm -f "$log"
exit "$status"
