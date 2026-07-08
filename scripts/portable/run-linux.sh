#!/usr/bin/env bash
# Double-click / double-run entry point for Linux. See run.sh for the logic.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/run.sh"
