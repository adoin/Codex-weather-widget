#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${CODEX_CALENDAR_INSTALL_DIR:-$HOME/Applications/CodexCalendar}"
EXECUTABLE="$INSTALL_DIR/codex-calendar"
PORT="${CODEX_CALENDAR_PORT:-8787}"
BASE_URL="http://127.0.0.1:$PORT"

if curl -fsS --max-time 1 "$BASE_URL/health" >/dev/null 2>&1; then
  curl -fsS -X POST "$BASE_URL/panel/toggle"
  exit 0
fi

if [[ ! -x "$EXECUTABLE" ]]; then
  printf 'Codex Calendar is not installed at %s\n' "$EXECUTABLE" >&2
  exit 1
fi
nohup "$EXECUTABLE" >"${TMPDIR:-/tmp}/codex-calendar.log" 2>&1 &
for _ in $(seq 1 50); do
  if curl -fsS --max-time 1 "$BASE_URL/health" >/dev/null 2>&1; then
    curl -fsS -X POST "$BASE_URL/panel/show"
    exit 0
  fi
  sleep 0.3
done
printf 'Codex Calendar did not start within 15 seconds.\n' >&2
exit 1
