#!/usr/bin/env bash
# FreEco.ai portable launcher — Linux & macOS
# Run from a USB drive or any folder: all config/data stays alongside this
# script (in ./data), so the drive is fully self-contained and portable
# across machines. Invoked via run-linux.sh or run-macos.command.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export OPENFANG_HOME="$ROOT/data"
export OPENFANG_LISTEN="${OPENFANG_LISTEN:-127.0.0.1:4200}"

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) echo "  Unsupported architecture: $ARCH"; exit 1 ;;
esac
case "$OS" in
    Linux) PLATFORM_DIR="linux" ;;
    Darwin) PLATFORM_DIR="macos" ;;
    *) echo "  Unsupported OS: $OS (this launcher is for Linux/macOS)"; exit 1 ;;
esac

EXE="$ROOT/bin/$PLATFORM_DIR/$ARCH/openfang"
if [ ! -f "$EXE" ]; then
    echo "  Could not find a FreEco.ai binary for $PLATFORM_DIR/$ARCH at:"
    echo "    $EXE"
    exit 1
fi
chmod +x "$EXE" 2>/dev/null || true

# Ad-hoc codesign on macOS (prevents SIGKILL on Apple Silicon Gatekeeper).
if [ "$PLATFORM_DIR" = "macos" ]; then
    command -v xattr >/dev/null 2>&1 && xattr -cr "$EXE" 2>/dev/null || true
    command -v codesign >/dev/null 2>&1 && codesign --force --sign - "$EXE" 2>/dev/null || true
fi

mkdir -p "$OPENFANG_HOME"
if [ ! -f "$OPENFANG_HOME/config.toml" ]; then
    echo "  First run — starting FreEco.ai setup wizard..."
    "$EXE" init
fi

# Open the dashboard in the default browser a few seconds after launch,
# once the server has had time to bind its port.
(
    sleep 3
    if command -v xdg-open >/dev/null 2>&1; then
        xdg-open "http://$OPENFANG_LISTEN" >/dev/null 2>&1 || true
    elif command -v open >/dev/null 2>&1; then
        open "http://$OPENFANG_LISTEN" >/dev/null 2>&1 || true
    fi
) &

echo ""
echo "  FreEco.ai starting — dashboard: http://$OPENFANG_LISTEN"
echo "  Data directory: $OPENFANG_HOME"
echo "  Press Ctrl+C to stop."
echo ""
exec "$EXE" start
