#!/usr/bin/env bash
# check-windows.sh — cross-compile the workspace for Windows from a unix host
# to catch `#[cfg(unix)]`-gated import warnings (and similar Windows-only
# clippy/build errors) before CI does.
#
# What it does:
#   1. Verifies the `x86_64-pc-windows-gnu` rustup target is installed
#      (installs it if missing).
#   2. Verifies the `x86_64-w64-mingw32-gcc` cross-compiler is on PATH
#      (errors with install hint if not).
#   3. Runs `cargo clippy --workspace --all-targets -- -D warnings`
#      against the Windows target.
#
# Usage:
#   scripts/check-windows.sh                 # clippy (default)
#   scripts/check-windows.sh check           # cargo check only (faster)
#   scripts/check-windows.sh build           # full build
#
# Designed to be invoked from anywhere; it cd's to the repo root.

set -euo pipefail

TARGET="x86_64-pc-windows-gnu"
MODE="${1:-clippy}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Windows cross-compile check (target: $TARGET, mode: $MODE)"

# ---- 1. rustup target ------------------------------------------------------
if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup not found on PATH" >&2
  exit 1
fi

if ! rustup target list --installed | grep -q "^${TARGET}\$"; then
  echo "==> Installing rustup target $TARGET"
  rustup target add "$TARGET"
fi

# ---- 2. mingw-w64 cross-compiler ------------------------------------------
if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  cat >&2 <<EOF
error: x86_64-w64-mingw32-gcc not found on PATH.

This is required to link C/C++ build-script artifacts (ring, aws-lc-sys, etc.)
for the Windows target.

  macOS:   brew install mingw-w64
  Debian:  sudo apt-get install mingw-w64
  Arch:    sudo pacman -S mingw-w64-gcc
EOF
  exit 1
fi

# ---- 3. Env: linker + C compiler for build scripts ------------------------
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="x86_64-w64-mingw32-gcc"
export CC_x86_64_pc_windows_gnu="x86_64-w64-mingw32-gcc"
export CXX_x86_64_pc_windows_gnu="x86_64-w64-mingw32-g++"
export AR_x86_64_pc_windows_gnu="x86_64-w64-mingw32-ar"

# ---- 4. Run the chosen cargo command --------------------------------------
case "$MODE" in
  clippy)
    exec cargo clippy --workspace --all-targets --target "$TARGET" -- -D warnings
    ;;
  check)
    exec cargo check --workspace --all-targets --target "$TARGET"
    ;;
  build)
    exec cargo build --workspace --all-targets --target "$TARGET"
    ;;
  *)
    echo "error: unknown mode '$MODE' (expected: clippy | check | build)" >&2
    exit 2
    ;;
esac
