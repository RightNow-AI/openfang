#!/usr/bin/env bash
# Assembles the FreEco.ai "Portable Edition" bundle: downloads the Windows,
# macOS, and Linux CLI release binaries and packages them with the launcher
# scripts from scripts/portable/ into a single folder that can be copied
# straight onto a USB drive.
#
# Usage: scripts/build-portable.sh [version] [output-dir]
#   version     - release tag to package (default: latest)
#   output-dir  - where to assemble the bundle (default: dist/freeco-portable)

set -euo pipefail

REPO="FreecoDAO/FreEco-ai"
VERSION="${1:-${OPENFANG_VERSION:-}}"
OUT_DIR="${2:-dist/freeco-portable}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORTABLE_SRC="$SCRIPT_DIR/portable"

# os_dir:arch:target-triple:archive-ext
TARGETS=(
    "windows:x86_64:x86_64-pc-windows-msvc:zip"
    "windows:aarch64:aarch64-pc-windows-msvc:zip"
    "macos:x86_64:x86_64-apple-darwin:tar.gz"
    "macos:aarch64:aarch64-apple-darwin:tar.gz"
    "linux:x86_64:x86_64-unknown-linux-gnu:tar.gz"
    "linux:aarch64:aarch64-unknown-linux-gnu:tar.gz"
)

if [ -z "$VERSION" ]; then
    echo "  Fetching latest release..."
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"//' | sed 's/".*//')
fi
if [ -z "$VERSION" ]; then
    echo "  Could not determine a release version. Pass one explicitly:"
    echo "    scripts/build-portable.sh v0.6.9"
    exit 1
fi
echo "  Building portable bundle for $VERSION..."

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/data"

TMPDIR=$(mktemp -d)
cleanup() { rm -rf "$TMPDIR"; }
trap cleanup EXIT

for entry in "${TARGETS[@]}"; do
    IFS=':' read -r OS_DIR ARCH TARGET EXT <<< "$entry"
    ARCHIVE="openfang-$TARGET.$EXT"
    URL="https://github.com/$REPO/releases/download/$VERSION/$ARCHIVE"
    CHECKSUM_URL="$URL.sha256"
    DEST_DIR="$OUT_DIR/bin/$OS_DIR/$ARCH"

    echo "  Fetching $OS_DIR/$ARCH ($TARGET)..."
    if ! curl -fsSL "$URL" -o "$TMPDIR/$ARCHIVE" 2>/dev/null; then
        echo "    Skipped: no release asset for $TARGET (may not be built for this version)."
        continue
    fi

    if curl -fsSL "$CHECKSUM_URL" -o "$TMPDIR/$ARCHIVE.sha256" 2>/dev/null; then
        EXPECTED=$(cut -d ' ' -f 1 < "$TMPDIR/$ARCHIVE.sha256")
        if command -v sha256sum &>/dev/null; then
            ACTUAL=$(sha256sum "$TMPDIR/$ARCHIVE" | cut -d ' ' -f 1)
        elif command -v shasum &>/dev/null; then
            ACTUAL=$(shasum -a 256 "$TMPDIR/$ARCHIVE" | cut -d ' ' -f 1)
        else
            ACTUAL=""
        fi
        if [ -n "$ACTUAL" ] && [ "$EXPECTED" != "$ACTUAL" ]; then
            echo "    Checksum verification FAILED for $ARCHIVE! Skipping."
            continue
        fi
    fi

    mkdir -p "$DEST_DIR"
    if [ "$EXT" = "zip" ]; then
        unzip -oq "$TMPDIR/$ARCHIVE" -d "$DEST_DIR"
    else
        tar xzf "$TMPDIR/$ARCHIVE" -C "$DEST_DIR"
    fi
done

cp "$PORTABLE_SRC/run.sh" "$PORTABLE_SRC/run-linux.sh" "$PORTABLE_SRC/run-macos.command" \
   "$PORTABLE_SRC/run-windows.bat" "$PORTABLE_SRC/README.txt" "$OUT_DIR/"
chmod +x "$OUT_DIR/run.sh" "$OUT_DIR/run-linux.sh" "$OUT_DIR/run-macos.command"

if [ -z "$(find "$OUT_DIR/bin" -type f 2>/dev/null)" ]; then
    echo ""
    echo "  Warning: no binaries were downloaded — check the version/network and try again."
    exit 1
fi

echo ""
echo "  Portable bundle assembled at: $OUT_DIR"
echo "  Copy this folder to a USB drive, then run the script for your OS."
