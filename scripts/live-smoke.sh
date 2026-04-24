#!/usr/bin/env bash
# OpenFang live integration smoke test
# Builds release, starts daemon, hits core endpoints, tears down.
# Referenced from CLAUDE.md "MANDATORY: Live Integration Testing".
#
# Usage: scripts/live-smoke.sh
# Exit codes: 0 = all checks passed, non-zero = failure (daemon killed on exit)
#
# Environment:
#   OPENFANG_PORT   — API port (default 4200)
#   OPENFANG_BIND   — bind host (default 127.0.0.1)
#   BUILD_PROFILE   — release | debug (default release)

set -euo pipefail

PORT="${OPENFANG_PORT:-4200}"
BIND="${OPENFANG_BIND:-127.0.0.1}"
BUILD_PROFILE="${BUILD_PROFILE:-release}"
BASE="http://${BIND}:${PORT}"

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) BIN_EXT=".exe" ;;
    *) BIN_EXT="" ;;
esac

BIN="target/${BUILD_PROFILE}/openfang${BIN_EXT}"
PID=""

log() { printf '[live-smoke] %s\n' "$*"; }
fail() { log "FAIL: $*"; exit 1; }

cleanup() {
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        log "stopping daemon pid=$PID"
        kill "$PID" 2>/dev/null || true
        # Give it a moment to release the port
        for _ in 1 2 3 4 5; do
            kill -0 "$PID" 2>/dev/null || break
            sleep 1
        done
        kill -9 "$PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Step 1 — build
log "building ${BUILD_PROFILE} binary"
if [ "$BUILD_PROFILE" = "release" ]; then
    cargo build --release -p openfang-cli
else
    cargo build -p openfang-cli
fi
[ -x "$BIN" ] || fail "binary not found at $BIN"

# Step 2 — make sure port is free
if curl -sSf "${BASE}/api/health" >/dev/null 2>&1; then
    fail "port ${PORT} already in use — stop the running daemon first"
fi

# Step 3 — start daemon
log "starting daemon on ${BASE}"
"$BIN" start >/tmp/openfang-smoke.log 2>&1 &
PID=$!
log "daemon pid=$PID (log: /tmp/openfang-smoke.log)"

# Step 4 — wait for /api/health to respond
HEALTHY=0
for i in $(seq 1 30); do
    if curl -sSf "${BASE}/api/health" >/dev/null 2>&1; then
        HEALTHY=1
        break
    fi
    sleep 1
done
[ "$HEALTHY" = "1" ] || fail "daemon did not respond on /api/health within 30s"
log "health OK"

# Step 5 — hit core endpoints (non-destructive reads)
log "GET /api/agents"
curl -sSf "${BASE}/api/agents" >/dev/null || fail "/api/agents failed"

log "GET /api/budget"
curl -sSf "${BASE}/api/budget" >/dev/null || fail "/api/budget failed"

log "GET /api/network/status"
curl -sSf "${BASE}/api/network/status" >/dev/null || fail "/api/network/status failed"

# Step 6 — dashboard HTML
log "GET /"
curl -sSf "${BASE}/" >/dev/null || fail "dashboard / failed"

log "ALL CHECKS PASSED"
