#!/bin/sh
set -eu

# OpenFang Installer (1BigBear fork - with musl/Android/proot support)
# https://raw.githubusercontent.com/1BigBear/openfang/main/install.sh

REPO="1BigBear/openfang"
INSTALL_DIR="$HOME/.openfang"
BIN_DIR="$HOME/.openfang/bin"
BINARY="openfang"

main() {
  need_cmd curl
  need_cmd tar
  need_cmd uname

  _os="$(uname -s)"
  _arch="$(uname -m)"

  # Detect environment on Linux aarch64
  _libc="gnu"
  _native="false"
  if [ "$_os" = "Linux" ]; then
    # Check for musl (nix-on-droid)
    if ldd /bin/sh 2>&1 | grep -q musl; then
      _libc="musl"
    # Check for proot-distro Ubuntu 24 — use native build with OpenSSL 3.x
    elif [ -f /etc/os-release ]; then
      _ubuntu_ver="$(grep '^VERSION_ID' /etc/os-release 2>/dev/null | cut -d'"' -f2)"
      if [ "$_ubuntu_ver" = "24.04" ] || [ "$_ubuntu_ver" = "24.10" ]; then
        _native="true"
      fi
    fi
  fi

  case "$_os" in
    Linux)
      case "$_arch" in
        x86_64|amd64)
          _target="x86_64-unknown-linux-gnu"
          ;;
        aarch64|arm64)
          if [ "$_libc" = "musl" ]; then
            # nix-on-droid — static musl binary, no libc dependency
            _target="aarch64-unknown-linux-musl"
          elif [ "$_native" = "true" ]; then
            # proot-distro Ubuntu 24 — native build with OpenSSL 3.x
            _target="aarch64-unknown-linux-gnu-native"
          else
            # fallback gnu build
            _target="aarch64-unknown-linux-gnu"
          fi
          ;;
        *)
          err "Unsupported architecture: $_arch"
          ;;
      esac
      ;;
    Darwin)
      case "$_arch" in
        x86_64|amd64)  _target="x86_64-apple-darwin" ;;
        aarch64|arm64) _target="aarch64-apple-darwin" ;;
        *)             err "Unsupported architecture: $_arch" ;;
      esac
      ;;
    *)
      err "Unsupported OS: $_os (use irm https://openfang.sh/install.ps1 | iex on Windows)"
      ;;
  esac

  _url="https://github.com/${REPO}/releases/latest/download/openfang-${_target}.tar.gz"

  say "Detected: $_os $_arch -> $_target"
  say "Downloading from: $_url"

  _tmpdir="$(mktemp -d 2>/dev/null || mktemp -d -t openfang)"
  trap 'rm -rf "$_tmpdir"' EXIT

  _code=$(curl -fsSL -w "%{http_code}" "$_url" -o "${_tmpdir}/openfang.tar.gz") || true
  if [ "$_code" = "404" ]; then
    err "Release not found for ${_target}. Check https://github.com/${REPO}/releases"
  fi
  if [ ! -f "${_tmpdir}/openfang.tar.gz" ] || [ "$(wc -c < "${_tmpdir}/openfang.tar.gz")" -lt 1000 ]; then
    err "Download failed (HTTP ${_code}). Check https://github.com/${REPO}/releases"
  fi

  say "Extracting..."
  tar -xzf "${_tmpdir}/openfang.tar.gz" -C "$_tmpdir"

  # Find the binary
  _bin="$(find "$_tmpdir" -name "$BINARY" -type f -perm +111 2>/dev/null | head -1)"
  if [ -z "$_bin" ]; then
    _bin="$(find "$_tmpdir" -name "$BINARY" -type f | head -1)"
  fi
  if [ -z "$_bin" ]; then
    err "Could not find openfang binary in archive"
  fi

  mkdir -p "$BIN_DIR"
  cp "$_bin" "${BIN_DIR}/${BINARY}"
  chmod +x "${BIN_DIR}/${BINARY}"

  # Verify it runs
  if "${BIN_DIR}/${BINARY}" --version >/dev/null 2>&1; then
    _ver=$("${BIN_DIR}/${BINARY}" --version 2>/dev/null || echo "unknown")
    say "Installed: $_ver"
  else
    say "Installed to: ${BIN_DIR}/${BINARY}"
  fi

  add_to_path

  say ""
  say "OpenFang installed successfully!"
  say ""
  say "  Run: openfang init"
  say "  Docs: https://openfang.sh/docs"
  say ""

  # Check if binary is reachable
  if ! command -v openfang >/dev/null 2>&1; then
    say "Note: restart your shell or run:"
    say "  export PATH=\"${BIN_DIR}:\$PATH\""
    say ""
  fi
}

add_to_path() {
  # Skip if already in PATH
  case ":$PATH:" in
    *":${BIN_DIR}:"*) return ;;
  esac

  _line="export PATH=\"${BIN_DIR}:\$PATH\""

  # Detect shell profile
  _profile=""
  if [ -n "${SHELL:-}" ]; then
    case "$SHELL" in
      */zsh)  _profile="$HOME/.zshrc" ;;
      */bash)
        if [ -f "$HOME/.bashrc" ]; then
          _profile="$HOME/.bashrc"
        else
          _profile="$HOME/.bash_profile"
        fi ;;
      */fish) _profile="$HOME/.config/fish/config.fish" ;;
      *)      _profile="$HOME/.profile" ;;
    esac
  else
    _profile="$HOME/.profile"
  fi

  if [ -n "$_profile" ] && [ -f "$_profile" ]; then
    if ! grep -q "/.openfang/bin" "$_profile" 2>/dev/null; then
      printf "\n# OpenFang\n%s\n" "$_line" >> "$_profile"
      say "Added to PATH via $_profile"
    fi
  fi
}

say() {
  printf "  \033[1;36mopenfang\033[0m %s\n" "$1"
}

err() {
  printf "  \033[1;36mopenfang\033[0m \033[1;31merror:\033[0m %s\n" "$1" >&2
  exit 1
}

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    err "need '$1' (command not found)"
  fi
}

main
