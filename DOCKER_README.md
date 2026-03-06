# OpenFang for Lazycat NAS

Custom [OpenFang](https://github.com/RightNow-AI/openfang) Docker image optimized for deployment on Lazycat LCMD Microserver.

**Automatically rebuilt on every new upstream release via GitHub Actions.**

## What's included

- **OpenFang Agent OS** — Rust-based autonomous AI agent daemon
- **Claude Code CLI** — Anthropic's CLI for Claude, as LLM provider
- **Node.js 22** — JavaScript runtime
- **Python 3** — Python runtime
- **Go** — via Homebrew
- **Homebrew** — package manager for additional tools
- **uv** — fast Python package manager
- **gh** — GitHub CLI
- **gog** — [Google Workspace CLI](https://gogcli.sh/) (Gmail, Calendar, Drive, Sheets, etc.)
- **ffmpeg** — multimedia processing
- **jq** — JSON processor
- **git, curl, wget** — standard utilities

## Non-root execution

The image uses `gosu` to drop root privileges to the `openfang` user at runtime. This is required because Claude Code's `--dangerously-skip-permissions` flag refuses to run as root.

The `openfang` user has passwordless `sudo` access, so it can still install system packages when needed.

## Usage

```bash
docker run -d \
  -p 4200:4200 \
  -v openfang-data:/data \
  -v openfang-home:/home/openfang \
  -e OPENFANG_HOME=/data \
  fliva/openfang:latest
```

## Source

- **This fork**: [github.com/f-liva/openfang](https://github.com/f-liva/openfang)
- **Upstream**: [github.com/RightNow-AI/openfang](https://github.com/RightNow-AI/openfang)
