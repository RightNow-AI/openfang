# Production Release Checklist

Everything that must be done before tagging `v0.1.0` and shipping to users. Items are ordered by dependency — complete them top to bottom.

---

## 1. Generate Tauri Signing Keypair

**Status:** BLOCKING — without this, auto-updater is dead. No user will ever receive an update.

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

Save both values. You need them for steps 2 and 3.

---

## 2. Set the Public Key in `tauri.conf.json`

**Status:** BLOCKING — the placeholder must be replaced before building.

Open `crates/openfang-desktop/tauri.conf.json` and replace:

```json
"pubkey": "PLACEHOLDER_REPLACE_WITH_GENERATED_PUBKEY"
```

with the actual public key string from step 1:

```json
"pubkey": "dW50cnVzdGVkIGNvb..."
```

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
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` certificate file |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the .p12 file |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Your Apple ID email |
| `APPLE_PASSWORD` | App-specific password from appleid.apple.com |
| `APPLE_TEAM_ID` | Your 10-character Team ID |

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

## 5. Set Up the `openfang.sh` Domain

**Status:** BLOCKING for install scripts — users run `curl -sSf https://openfang.sh | sh`.

Options:
- **GitHub Pages**: Point `openfang.sh` to a GitHub Pages site that redirects `/` to `scripts/install.sh` and `/install.ps1` to `scripts/install.ps1` from the repo's latest release.
- **Cloudflare Workers / Vercel**: Serve the install scripts with proper `Content-Type: text/plain` headers.
- **Raw GitHub redirect**: Use `openfang.sh` as a CNAME to `raw.githubusercontent.com/RightNow-AI/openfang/main/scripts/install.sh` (less reliable).

The install scripts reference:
- `https://openfang.sh` → serves `scripts/install.sh`
- `https://openfang.sh/install.ps1` → serves `scripts/install.ps1`

Until the domain is set up, users can install via:
```bash
curl -sSf https://raw.githubusercontent.com/RightNow-AI/openfang/main/scripts/install.sh | sh
```

---

## 6. Verify Dockerfile Builds

**Status:** VERIFY — the Dockerfile must produce a working image.

```bash
docker build -t openfang:local .
docker run --rm openfang:local --version
docker run --rm -p 4200:4200 -v openfang-data:/data openfang:local start
```

Confirm:
- Binary runs and prints version
- `start` command boots the kernel and API server
- Port 4200 is accessible
- `/data` volume persists between container restarts

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

## 8. Write CHANGELOG.md for v0.1.0

**Status:** VERIFY — confirm it covers all shipped features.

The release workflow includes a link to `CHANGELOG.md` in every GitHub release body. Ensure it exists at the repo root and covers:

- All 14 crates and what they do
- Key features: 40 channels, 60 skills, cloud/local LLM providers, builtin and runtime-discovered models
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
git commit -m "chore: prepare v0.1.0 release"

# Tag and push
git tag v0.1.0
git push origin main --tags
```

This triggers the release workflow which:
1. Builds desktop installers for 4 targets (Linux, macOS x86, macOS ARM, Windows)
2. Generates signed `latest.json` for the auto-updater
3. Builds CLI binaries for 5 targets
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
- [ ] CLI `.tar.gz` archives present (5 targets)
- [ ] CLI `.zip` present (Windows)
- [ ] SHA256 checksum files present for each CLI archive

### Auto-Updater Manifest
Visit: `https://github.com/RightNow-AI/openfang/releases/latest/download/latest.json`

- [ ] JSON is valid
- [ ] Contains `signature` fields (not empty strings)
- [ ] Contains download URLs for all platforms
- [ ] Version matches the tag

### Docker Image
```bash
docker pull ghcr.io/RightNow-AI/openfang:latest
docker pull ghcr.io/RightNow-AI/openfang:0.1.0

# Verify both architectures
docker run --rm ghcr.io/RightNow-AI/openfang:latest --version
```

### Desktop App Auto-Update (test with v0.1.1)
1. Install v0.1.0 from the release
2. Tag v0.1.1 and push
3. Wait for release workflow to complete
4. Open the v0.1.0 app — after 10 seconds it should:
   - Show "OpenFang Updating..." notification
   - Download and install v0.1.1
   - Restart automatically to v0.1.1
5. Right-click tray → "Check for Updates" → should show "Up to Date"

### Install Scripts
```bash
# Linux/macOS
curl -sSf https://openfang.sh | sh
openfang --version  # Should print v0.1.0

# Windows PowerShell
irm https://openfang.sh/install.ps1 | iex
openfang --version
```

---

## Quick Reference — What Blocks What

```
Step 1 (keygen) ──┬──> Step 2 (pubkey in config)
                  └──> Step 3 (secrets in GitHub)
                         │
Step 4 (icons) ──────────┤
Step 5 (domain) ─────────┤
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
