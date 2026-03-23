# Release Runbook

This runbook is the shortest operator path for publishing and validating a release from this fork.

Repository target:

- GitHub repo: `tytsxai/openfang-upstream-fork`
- GHCR image: `ghcr.io/tytsxai/openfang-upstream-fork`

Use this together with [production-checklist.md](production-checklist.md) when you are ready to ship.

## 1. Preconditions

Before tagging a release, confirm all of the following:

```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
OPENFANG_API_KEY=release-preflight-placeholder docker compose config
bash -n scripts/backup-openfang.sh
bash -n scripts/preflight-openfang.sh
bash -n scripts/provider-canary-openfang.sh
bash -n scripts/restore-openfang.sh
bash -n scripts/smoke-openfang.sh
bash -n scripts/live-api-smoke-openfang.sh
python3 -m py_compile scripts/healthcheck-openfang.py
```

If you are mirroring the current `Release` workflow exactly, also run these host-tool checks on a Linux environment that has the binaries installed:

```bash
systemd-analyze verify deploy/openfang.service
promtool check rules deploy/openfang-alerts.yml
promtool check config deploy/prometheus-scrape.yml
```

Confirm release metadata is aligned:

```bash
grep '^version' Cargo.toml
grep '"version"' crates/openfang-desktop/tauri.conf.json
git remote -v
```

Expected:

- workspace version and desktop version match
- `origin` points at `tytsxai/openfang-upstream-fork`
- `release.yml` parses and current docs/scripts reference this fork

Confirm GitHub-side prerequisites:

- Actions secrets include `TAURI_SIGNING_PRIVATE_KEY`
- Actions secrets include `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- Actions secrets include `GROQ_API_KEY` for the mandatory release provider canary
- optional CI provider-canary secrets are distinct from the release workflow:
  `OPENFANG_PROVIDER_CANARY_API_KEY`, `OPENFANG_CANARY_BASE_URL`, `OPENFANG_CANARY_PROVIDER`,
  `OPENFANG_CANARY_MODEL`, and `OPENFANG_CANARY_API_KEY_ENV` only control the main CI job
- optional macOS signing/notarization secrets are present if you are shipping signed macOS builds
- the `ghcr.io/tytsxai/openfang-upstream-fork` package exists or can be created by the workflow

## 2. Tag and Publish

Choose a release version and keep it consistent everywhere.

Example:

```bash
RELEASE_VERSION="0.4.5"
git status --short
git add -A
git commit -m "chore: prepare v${RELEASE_VERSION} release"
git tag "v${RELEASE_VERSION}"
git push origin main --tags
```

Then watch the workflow:

- GitHub Actions → `Release`
- confirm `preflight`, `desktop`, `verify-desktop-release-assets`, `cli`, `docker`, `verify-docker-release-image`, `provider-canary`, and `publish-release` all finish successfully
- expect the GitHub release to stay in draft until `publish-release` runs

## 3. Verify GitHub Release Assets

After the workflow completes, verify the release exists and contains the expected assets:

```bash
curl -fsSL "https://api.github.com/repos/tytsxai/openfang-upstream-fork/releases/tags/v${RELEASE_VERSION}" > /tmp/openfang-release.json
python3 - <<'PY'
import json
from pathlib import Path
r = json.loads(Path("/tmp/openfang-release.json").read_text())
print("tag:", r["tag_name"])
print("asset count:", len(r.get("assets", [])))
for a in r.get("assets", []):
    print(a["name"])
PY
```

Minimum expected:

- desktop installers for the configured platforms
- CLI archives and checksum files
- `latest.json`

Verify `latest.json` directly:

```bash
curl -fsSL "https://github.com/tytsxai/openfang-upstream-fork/releases/latest/download/latest.json" > /tmp/latest.json
python3 - <<'PY'
import json
from pathlib import Path
m = json.loads(Path("/tmp/latest.json").read_text())
print("version:", m.get("version"))
print("platforms:", sorted((m.get("platforms") or {}).keys()))
for name, meta in (m.get("platforms") or {}).items():
    assert meta.get("url"), f"{name}: missing url"
    assert meta.get("signature"), f"{name}: missing signature"
PY
```

Expected:

- HTTP 200 from the `latest.json` URL
- manifest version equals `${RELEASE_VERSION}`
- every platform entry has a non-empty `url`
- every platform entry has a non-empty `signature`

## 4. Verify GHCR Publication

Check the image tags:

```bash
docker manifest inspect "ghcr.io/tytsxai/openfang-upstream-fork:latest"
docker manifest inspect "ghcr.io/tytsxai/openfang-upstream-fork:${RELEASE_VERSION}"
```

`latest` is promoted only after the release draft and asset checks are fully green. If the workflow fails before `publish-release`, only the versioned image tag should exist.

If pulls are meant to be public, also test:

```bash
docker pull "ghcr.io/tytsxai/openfang-upstream-fork:latest"
docker run --rm "ghcr.io/tytsxai/openfang-upstream-fork:latest" --version
```

The release workflow now smoke-tests the just-pushed versioned image before it
promotes `latest`, so a broken container boot path blocks publication instead
of reaching users.

If you get `unauthorized`:

- open GitHub Packages for `tytsxai/openfang-upstream-fork`
- set the container package visibility and permissions intentionally
- confirm the package is linked to this repo
- retry the manifest/pull checks

## 5. Quick Smoke Tests

### CLI archive path

Use the tagged installer source, not `main`:

```bash
curl -fsSL "https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/v${RELEASE_VERSION}/scripts/install.sh" | sh
openfang --version
```

### Docker path

If Docker is available locally:

```bash
docker build -t openfang:local .
docker run --rm openfang:local --version
OPENFANG_API_KEY="$(openssl rand -hex 32)"
docker run --rm -p 4200:4200 \
  -e OPENFANG_LISTEN=0.0.0.0:4200 \
  -e OPENFANG_API_KEY="$OPENFANG_API_KEY" \
  -e OPENFANG_STRICT_PRODUCTION=1 \
  openfang:local start
```

In another terminal:

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/smoke-openfang.sh
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/live-api-smoke-openfang.sh
```

If you are validating the shipped systemd unit, install the strict preflight helper before enabling the unit:

```bash
sudo install -d /usr/local/lib/openfang
sudo install -m 0755 scripts/preflight-openfang.sh /usr/local/lib/openfang/preflight-openfang.sh
```

### Provider path

Before calling the release healthy, run one real provider-backed canary and keep the output:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" \
OPENFANG_CANARY_PROVIDER=groq \
OPENFANG_CANARY_MODEL=llama-3.3-70b-versatile \
OPENFANG_CANARY_API_KEY_ENV=GROQ_API_KEY \
scripts/provider-canary-openfang.sh
```

## 6. Rollback and Triage

If the workflow fails before publishing:

- fix the failing job
- delete the bad local tag if needed: `git tag -d "v${RELEASE_VERSION}"`
- delete the remote tag if it was pushed: `git push --delete origin "v${RELEASE_VERSION}"`
- delete the draft release if one was created
- create a corrected tag and push again

If the release exists but `latest.json` is missing or invalid:

- do not treat the desktop updater as healthy
- inspect `crates/openfang-desktop/tauri.conf.json` and `.github/workflows/release.yml`
- confirm `bundle.createUpdaterArtifacts` is enabled
- confirm signing secrets were present
- publish a corrected release; do not rely on the broken one

If the GHCR image is missing or private:

- inspect the `docker` job in the `Release` workflow
- verify GitHub Packages visibility and repo linkage
- re-run the workflow after correcting package settings

If install scripts point at the wrong source:

- verify release body links use `${{ github.repository }}` and `${{ github.ref_name }}`
- verify script defaults still target `tytsxai/openfang-upstream-fork`

## 7. Final Go/No-Go

Treat the release as ready only if all are true:

- workspace checks are green
- release workflow is green
- `latest.json` returns `200` and matches the release version
- GHCR image exists for `latest` and the version tag
- at least one install path has been smoke tested after publish
