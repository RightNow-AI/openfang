#!/usr/bin/env bash
# Double-clickable entry point for macOS Finder (needs the .command
# extension — a plain .sh opens in a text editor instead of running).
# See run.sh for the actual logic.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/run.sh"
