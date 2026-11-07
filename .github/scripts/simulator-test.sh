#!/usr/bin/env bash
# Runs `flutter test integration_test` on an iOS simulator with a watchdog.
#
#   simulator-test.sh <udid> <bundle-id> [extra flutter test args...]
#
# Launching the app on the CI simulator sometimes hangs after the Xcode build: no test
# output until the job's 90-minute timeout (seen on #14 and #15, while the same code
# passed on main). A healthy run takes about 7 minutes. So each attempt gets
# ATTEMPT_MINUTES; a hang (and only a hang) dumps the app's last simulator log lines and
# is retried once. A test failure fails at once and is never retried.
set -uo pipefail

udid="$1"
bundle="$2"
shift 2
minutes="${ATTEMPT_MINUTES:-15}"

for attempt in 1 2; do
  # `timeout` is not installed on the macOS runners; perl's alarm is.
  perl -e 'alarm shift; exec @ARGV' "$((minutes * 60))" \
    flutter test integration_test -d "$udid" "$@"
  status=$?
  if [ "$status" -ne 142 ]; then
    exit "$status"
  fi
  echo "::warning::attempt $attempt hung for ${minutes} min (no result); app log follows"
  xcrun simctl spawn "$udid" log show --last 10m --style compact \
    --predicate "subsystem BEGINSWITH \"$bundle\" OR process == \"Runner\"" 2>/dev/null \
    | tail -n 200 || true
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  pkill -f "flutter_tools.snapshot test" 2>/dev/null || true
done
echo "::error::the simulator test hung twice"
exit 1
