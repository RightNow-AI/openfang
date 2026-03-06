#!/usr/bin/env bash
# OpenFang installer — works on Linux, macOS, WSL
# Usage: curl -sSf https://openfang.sh | sh
#
# Environment variables:
#   OPENFANG_INSTALL_DIR  — custom install directory (default: ~/.openfang/bin)
#   OPENFANG_VERSION      — install a specific version tag (default: latest)

set -euo pipefail

REPO="RightNow-AI/openfang"
INSTALL_DIR="${OPENFANG_INSTALL_DIR:-$HOME/.openfang/bin}"

detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64|amd64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) echo "  Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    case "$OS" in
        linux) PLATFORM="${ARCH}-unknown-linux-gnu" ;;
        darwin) PLATFORM="${ARCH}-apple-darwin" ;;
        mingw*|msys*|cygwin*)
            echo ""
            echo "  For Windows, use PowerShell instead:"
            echo "    irm https://openfang.sh/install.ps1 | iex"
            echo ""
            echo "  Or download the .msi installer from:"
            echo "    https://github.com/$REPO/releases/latest"
            echo ""
            echo "  Or install via cargo:"
            echo "    cargo install --git https://github.com/$REPO openfang-cli"
            exit 1
            ;;
        *) echo "  Unsupported OS: $OS"; exit 1 ;;
    esac
}

install() {
    detect_platform

    echo ""
    echo "  OpenFang Installer"
    echo "  =================="
    echo ""

    # Get latest version
    if [ -n "${OPENFANG_VERSION:-}" ]; then
        VERSION="$OPENFANG_VERSION"
        echo "  Using specified version: $VERSION"
    else
        echo "  Fetching latest release..."
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | head -1 | cut -d '"' -f 4)
    fi

    if [ -z "$VERSION" ]; then
        echo "  Could not determine latest version."
        echo "  Install from source instead:"
        echo "    cargo install --git https://github.com/$REPO openfang-cli"
        exit 1
    fi

    URL="https://github.com/$REPO/releases/download/$VERSION/openfang-$PLATFORM.tar.gz"
    CHECKSUM_URL="$URL.sha256"

    echo "  Installing OpenFang $VERSION for $PLATFORM..."
    mkdir -p "$INSTALL_DIR"

    # Download to temp
    TMPDIR=$(mktemp -d)
    ARCHIVE="$TMPDIR/openfang.tar.gz"
    CHECKSUM_FILE="$TMPDIR/checksum.sha256"

    cleanup() { rm -rf "$TMPDIR"; }
    trap cleanup EXIT

    if ! curl -fsSL "$URL" -o "$ARCHIVE" 2>/dev/null; then
        echo "  Download failed. The release may not exist for your platform."
        echo "  Install from source instead:"
        echo "    cargo install --git https://github.com/$REPO openfang-cli"
        exit 1
    fi

    # Verify checksum if available
    if curl -fsSL "$CHECKSUM_URL" -o "$CHECKSUM_FILE" 2>/dev/null; then
        EXPECTED=$(cut -d ' ' -f 1 < "$CHECKSUM_FILE")
        if command -v sha256sum &>/dev/null; then
            ACTUAL=$(sha256sum "$ARCHIVE" | cut -d ' ' -f 1)
        elif command -v shasum &>/dev/null; then
            ACTUAL=$(shasum -a 256 "$ARCHIVE" | cut -d ' ' -f 1)
        else
            ACTUAL=""
        fi
        if [ -n "$ACTUAL" ]; then
            if [ "$EXPECTED" != "$ACTUAL" ]; then
                echo "  Checksum verification FAILED!"
                echo "    Expected: $EXPECTED"
                echo "    Got:      $ACTUAL"
                exit 1
            fi
            echo "  Checksum verified."
        else
            echo "  No sha256sum/shasum found, skipping checksum verification."
        fi
    fi

    # Extract
    tar xzf "$ARCHIVE" -C "$INSTALL_DIR"
    chmod +x "$INSTALL_DIR/openfang"

    # Ad-hoc codesign on macOS (prevents SIGKILL on Apple Silicon)
    if [ "$OS" = "darwin" ] && command -v codesign &>/dev/null; then
        codesign --force --sign - "$INSTALL_DIR/openfang" 2>/dev/null || true
    fi

    # Add to PATH
    SHELL_RC=""
    case "${SHELL:-}" in
        */zsh) SHELL_RC="$HOME/.zshrc" ;;
        */bash) SHELL_RC="$HOME/.bashrc" ;;
        */fish) SHELL_RC="$HOME/.config/fish/config.fish" ;;
    esac

    if [ -n "$SHELL_RC" ] && ! grep -q "openfang" "$SHELL_RC" 2>/dev/null; then
        case "${SHELL:-}" in
            */fish)
                mkdir -p "$(dirname "$SHELL_RC")"
                echo "set -gx PATH \"$INSTALL_DIR\" \$PATH" >> "$SHELL_RC"
                ;;
            *)
                echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_RC"
                ;;
        esac
        echo "  Added $INSTALL_DIR to PATH in $SHELL_RC"
    fi

    # Verify installation
    if "$INSTALL_DIR/openfang" --version >/dev/null 2>&1; then
        INSTALLED_VERSION=$("$INSTALL_DIR/openfang" --version 2>/dev/null || echo "$VERSION")
        echo ""
        echo "  OpenFang installed successfully! ($INSTALLED_VERSION)"
    else
        echo ""
        echo "  OpenFang binary installed to $INSTALL_DIR/openfang"
    fi

    echo ""
    echo "  Get started:"
    echo "    openfang init"
    echo ""
    echo "  The setup wizard will guide you through provider selection"
    echo "  and configuration."
    echo ""
}

install
