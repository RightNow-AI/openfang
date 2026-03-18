# Production Release Checklist

This document is the release-engineering checklist for shipping OpenFang builds. It is not the primary deployment guide for running the daemon.

For actual deployment procedures, start with [deployment.md](deployment.md). For day-2 operation, use [operations-runbook.md](operations-runbook.md).

Everything that must be done before tagging the next release and shipping to users. Items are ordered by dependency — complete them top to bottom.

---

## 1. Generate Tauri Signing Keypair

**Status:** COMPLETE in repo — the current desktop config already contains a public updater key. Only rerun this step if you are rotating signing keys.

The Tauri updater requires an Ed25519 keypair. The private key signs every release bundle, and the public key is embedded in the app binary so it can verify updates.

```bash
# Install the Tauri CLI (if not already installed)
cargo install tauri-cli --locked

# Generate the keypair
cargo tauri signer generate -w ~/.tauri/openfang.key
```

The command will output:

```
Your public key was generated successfully:
dW50cnVzdGVkIGNvb...  <-- COPY THIS

Your private key was saved to: ~/.tauri/openfang.key
```

Save both values. You need the private key for step 3. The public key is already present in the current repository config.

---

## 2. Set the Public Key in `tauri.conf.json`

**Status:** COMPLETE in repo — verify only if you are rotating signing keys.

Open `crates/openfang-desktop/tauri.conf.json` and confirm the configured key matches the signer private key from step 1:

```json
"pubkey": "dW50cnVzdGVkIGNvb..."
```

If you rotate keys, replace the committed value with the new public key before building release artifacts.

---

## 3. Add GitHub Repository Secrets

**Status:** BLOCKING — CI/CD release workflow will fail without these.

Go to **GitHub repo → Settings → Secrets and variables → Actions → New repository secret** and add:

| Secret Name | Value | Required |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `~/.tauri/openfang.key` | Yes |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password you set during keygen (or empty string) | Yes |

### Optional — macOS Code Signing

Without these, macOS users will see "app from unidentified developer" warnings. Requires an Apple Developer account ($99/year).

| Secret Name | Value |
|---|---|
| `MAC_CERT_BASE64` | Base64-encoded `.p12` certificate file |
| `MAC_CERT_PASSWORD` | Password for the `.p12` file |
| `MAC_NOTARIZE_APPLE_ID` | Your Apple ID email |
| `MAC_NOTARIZE_PASSWORD` | App-specific password from appleid.apple.com |
| `MAC_NOTARIZE_TEAM_ID` | Your 10-character Team ID |

`APPLE_SIGNING_IDENTITY` is derived in the workflow after the certificate import, so it does not need to be stored as a repository secret.

To generate the base64 certificate:
```bash
base64 -i Certificates.p12 | pbcopy
```

### Optional — Windows Code Signing

Without this, Windows SmartScreen may warn users. Requires an EV code signing certificate.

Set `certificateThumbprint` in `tauri.conf.json` under `bundle.windows` and add the certificate to the Windows runner in CI.

---

## 4. Create Icon Assets

**Status:** VERIFY — icons may be placeholders.

The following icon files must exist in `crates/openfang-desktop/icons/`:

| File | Size | Usage |
|---|---|---|
| `icon.png` | 1024x1024 | Source icon, macOS .icns generation |
| `icon.ico` | multi-size | Windows taskbar, installer |
| `32x32.png` | 32x32 | System tray, small contexts |
| `128x128.png` | 128x128 | Application lists |
| `128x128@2x.png` | 256x256 | HiDPI/Retina displays |

Verify they are real branded icons (not Tauri defaults). Generate from a single source SVG:

```bash
# Using ImageMagick
convert icon.svg -resize 1024x1024 icon.png
convert icon.svg -resize 32x32 32x32.png
convert icon.svg -resize 128x128 128x128.png
convert icon.svg -resize 256x256 128x128@2x.png
convert icon.svg -resize 256x256 -define icon:auto-resize=256,128,64,48,32,16 icon.ico
```

---

## 5. Optional `openfang.sh` Vanity Domain

**Status:** OPTIONAL — release readiness must not depend on this domain.

Options:
- **GitHub Pages**: Point `openfang.sh` to a GitHub Pages site that redirects `/` to `scripts/install.sh` and `/install.ps1` to `scripts/install.ps1` from the repo's latest release.
- **Cloudflare Workers / Vercel**: Serve the install scripts with proper `Content-Type: text/plain` headers.
- **Raw GitHub redirect**: Use `openfang.sh` as a CNAME to a tag-pinned raw script URL such as `raw.githubusercontent.com/tytsxai/openfang-upstream-fork/v<release-tag>/scripts/install.sh` (less reliable).

If you enable the vanity domain, it should reference:
- `https://openfang.sh/install` → serves `scripts/install.sh`
- `https://openfang.sh/install.ps1` → serves `scripts/install.ps1` for this fork

The supported install source of truth is GitHub. Users can always install via:
```bash
curl -sSf https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/v<release-tag>/scripts/install.sh | sh
```

---

## 6. Verify Dockerfile Builds

**Status:** VERIFY — the Dockerfile must produce a working image.

```bash
docker build -t openfang:local .
docker run --rm openfang:local --version
OPENFANG_API_KEY="$(openssl rand -hex 32)"
docker run --rm -p 4200:4200 \
  -e OPENFANG_LISTEN=0.0.0.0:4200 \
  -e OPENFANG_API_KEY="$OPENFANG_API_KEY" \
  -v openfang-data:/data \
  openfang:local start
```

Confirm:
- Binary runs and prints version
- `start` command boots the kernel and API server
- Port 4200 is accessible
- `/data` volume persists between container restarts
- `/api/health/detail` reports `status = "ok"` with the same auth mode the deployment will use
- Container healthcheck follows `OPENFANG_LISTEN` (or explicit `OPENFANG_BASE_URL`) so non-default listen ports do not flap to `unhealthy`

Before shipping a production image or binary cutover, also verify operator safety rails:

```bash
scripts/backup-openfang.sh
OPENFANG_PREFLIGHT_OFFLINE=1 scripts/preflight-openfang.sh --offline
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

For systemd-style deployments using `/etc/openfang/env`, include:

```bash
OPENFANG_ENV_FILE=/etc/openfang/env scripts/backup-openfang.sh
OPENFANG_ENV_FILE=/etc/openfang/env OPENFANG_PREFLIGHT_OFFLINE=1 scripts/preflight-openfang.sh --offline
OPENFANG_ENV_FILE=/etc/openfang/env OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

Pass criteria:
- backup succeeds while the daemon is stopped, or live backup is explicitly opted into
- offline preflight validates config resolution, state-file integrity, writable runtime paths, and SQLite quick-check
- live preflight validates runtime reachability and checks `/api/health/detail` readiness when the provided auth context can access protected endpoints
- if the release is meant to serve provider-backed traffic, one real `scripts/provider-canary-openfang.sh` run succeeds and is archived with the release evidence

---

## 7. Verify Install Scripts Locally

**Status:** VERIFY before release.

### Linux/macOS
```bash
# Test against a real GitHub release (after first tag)
bash scripts/install.sh

# Or test syntax only
bash -n scripts/install.sh
shellcheck scripts/install.sh
```

### Windows (PowerShell)
```powershell
# Test against a real GitHub release (after first tag)
powershell -ExecutionPolicy Bypass -File scripts/install.ps1

# Or syntax check only
pwsh -NoProfile -Command "Get-Content scripts/install.ps1 | Out-Null"
```

### Docker smoke test
```bash
docker build -f scripts/docker/install-smoke.Dockerfile .
```

---

## 8. Write CHANGELOG.md for the Release

**Status:** VERIFY — confirm it covers all shipped features.

The release workflow includes a link to `CHANGELOG.md` in every GitHub release body. Ensure it exists at the repo root and covers:

- All 14 crates and what they do
- Key features: 40 channels, 60 skills, 20 providers, 51 models
- Security systems (9 SOTA + 7 critical fixes)
- Desktop app with auto-updater
- Migration path from OpenClaw
- Docker and CLI install options

---

## 9. First Release — Tag and Push

Once steps 1-8 are complete:

```bash
# Ensure version matches everywhere
grep '"version"' crates/openfang-desktop/tauri.conf.json
grep '^version' Cargo.toml

# Commit any final changes
git add -A
git commit -m "chore: prepare release"

# Tag and push
git tag v<release-version>
git push origin main --tags
```

This triggers the release workflow which:
1. Builds desktop installers for 5 targets (Linux, macOS x86, macOS ARM, Windows x86, Windows ARM)
2. Generates signed `latest.json` for the auto-updater
3. Builds CLI binaries for 6 targets (Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64/aarch64)
4. Builds and pushes multi-arch Docker image
5. Creates a GitHub Release with all artifacts

---

## 10. Post-Release Verification

After the release workflow completes (~15-30 min):

### GitHub Release Page
- [ ] `.msi` and `.exe` present (Windows desktop)
- [ ] `.dmg` present (macOS desktop)
- [ ] `.AppImage` and `.deb` present (Linux desktop)
- [ ] `latest.json` present (auto-updater manifest)
- [ ] CLI `.tar.gz` archives present (Linux + macOS builds, both x86_64 and ARM64)
- [ ] CLI `.zip` archives present (Windows x86_64 and Windows ARM64)
- [ ] SHA256 checksum files present for each CLI archive

### Auto-Updater Manifest
Visit: `https://github.com/tytsxai/openfang-upstream-fork/releases/latest/download/latest.json`

- [ ] JSON is valid
- [ ] Contains `signature` fields (not empty strings)
- [ ] Contains download URLs for all platforms
- [ ] Version matches the tag

### Docker Image
```bash
docker pull ghcr.io/tytsxai/openfang-upstream-fork:latest
docker pull ghcr.io/tytsxai/openfang-upstream-fork:<release-version>

# Verify both architectures
docker run --rm ghcr.io/tytsxai/openfang-upstream-fork:latest --version
```

### Desktop App Auto-Update (test with the next release)
1. Install `v<release-version>` from the release
2. Tag `v<next-release-version>` and push
3. Wait for release workflow to complete
4. Open the `v<release-version>` app — after 10 seconds it should:
   - Show "OpenFang Updating..." notification
   - Download and install `v<next-release-version>`
   - Restart automatically to `v<next-release-version>`
5. Right-click tray → "Check for Updates" → should show "Up to Date"

### Install Scripts
```bash
# Linux/macOS
curl -sSf https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/v<release-tag>/scripts/install.sh | sh
openfang --version  # Should print the released version

# Windows PowerShell
irm https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/v<release-tag>/scripts/install.ps1 | iex
openfang --version
```

---

## Quick Reference — What Blocks What

```
Step 1 (keygen) ──┬──> Step 2 (pubkey in config)
                  └──> Step 3 (secrets in GitHub)
                         │
Step 4 (icons) ──────────┤
Step 5 (domain verify) ──┤
Step 6 (Dockerfile) ─────┤
Step 7 (install scripts) ┤
Step 8 (CHANGELOG) ──────┘
                         │
                         v
                  Step 9 (tag + push)
                         │
                         v
                  Step 10 (verify)
```

Steps 4-8 can be done in parallel. Steps 1-3 are sequential and must be done first.
