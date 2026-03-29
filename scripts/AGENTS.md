<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# scripts

## Purpose
Installation and build automation scripts for Linux/macOS/WSL (shell) and Windows (PowerShell), plus Docker Smoke test image.

## Key Files
| File | Description |
|------|-------------|
| `install.sh` | POSIX installer for Linux/macOS/WSL — platform detection, binary download, PATH setup |
| `install.ps1` | PowerShell installer for Windows — same functionality as shell script |
| `docker/install-smoke.Dockerfile` | Smoke test image — minimal container to verify daemon binary works |

## For AI Agents

### Working In This Directory
- Both install scripts use the same logic — keep them in sync.
- Scripts must support `OPENFANG_INSTALL_DIR` and `OPENFANG_VERSION` environment variables.
- Scripts must detect platform (Linux, macOS, Windows) and architecture (x86_64, aarch64) correctly.
- Before modifying install scripts, test on actual target platforms (Linux, macOS, WSL, Windows).
- Smoke test Dockerfile should be minimal — just enough to verify the binary runs and serves HTTP.

<!-- MANUAL: -->
