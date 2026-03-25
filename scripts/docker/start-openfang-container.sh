#!/bin/sh
set -eu

if [ "${OPENFANG_BOOTSTRAP_SHIPINBOT:-0}" != "1" ]; then
  exec openfang "$@"
fi

openfang "$@" &
daemon_pid="$!"

cleanup() {
  if kill -0 "$daemon_pid" 2>/dev/null; then
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
  fi
}

trap cleanup INT TERM

python3 /usr/local/bin/bootstrap-shipinfabu-hand.py

wait "$daemon_pid"
