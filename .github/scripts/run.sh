#!/usr/bin/env bash
# Run a command; if it fails, also report its errors as a GitHub annotation so they show
# on the run's summary page (and through the API) without opening the full log.
#   run.sh <title> <command...>
set -uo pipefail
title="$1"
shift
log="$(mktemp)"
"$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
if [ "$status" -ne 0 ]; then
  # Compiler errors with context, or else the end of the output.
  details=$(grep -E -A14 '^(error|warning)(\[|:)|FAIL|panicked' "$log" | head -c 30000)
  [ -n "$details" ] || details=$(tail -c 8000 "$log")
  details=${details//'%'/'%25'}
  details=${details//$'\r'/}
  details=${details//$'\n'/'%0A'}
  echo "::error title=$title::$details"
fi
rm -f "$log"
exit "$status"
