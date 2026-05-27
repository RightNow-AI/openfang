# Installation

OpenFang can be installed with Homebrew on macOS, the shell installer on macOS or Linux, PowerShell on Windows, or from source for development.

## macOS with Homebrew

```bash
brew tap RightNow-AI/openfang
brew install openfang
```

The tap formula is generated from each stable `v*` release after the macOS CLI tarballs are published. Pre-release tags such as `v1.2.3-rc.1` do not update the tap.

## macOS or Linux

```bash
curl -fsSL https://openfang.sh/install | sh
```

To install a specific release, set `OPENFANG_VERSION` to the tag:

```bash
OPENFANG_VERSION=v0.6.9 curl -fsSL https://openfang.sh/install | sh
```

## Windows

```powershell
irm https://openfang.sh/install.ps1 | iex
```

## From Source

```bash
cargo install --path crates/openfang-cli
```

## Maintainer Notes

The release workflow publishes `Formula/openfang.rb` to `RightNow-AI/homebrew-openfang` using the `HOMEBREW_TAP_TOKEN` secret. The token needs contents write access to the tap repository.

Tap-side CI should run `brew test-bot --only-formulae openfang` against the rendered formula before releases are announced.
