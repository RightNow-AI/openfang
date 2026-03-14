#!/usr/bin/env pwsh
# scripts/check-drift.ps1 — CI drift enforcement for Rust ↔ TypeScript API contract.
#
# Fails with exit code 1 if the committed openapi.json or src/types/api.ts
# is out of sync with the current Rust source.
#
# Usage:
#   pwsh scripts/check-drift.ps1
#
# In CI (GitHub Actions / Azure DevOps):
#   - run: pwsh scripts/check-drift.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

try {
    # ── Step 1: Regenerate OpenAPI JSON spec from Rust source ────────────────
    Write-Host "==> Building OpenAPI spec via xtask..."
    cargo xtask openapi-gen --out openapi.json
    if ($LASTEXITCODE -ne 0) { throw "cargo xtask openapi-gen failed" }
    Write-Host "    openapi.json written."

    # ── Step 2: Regenerate TypeScript types from spec ────────────────────────
    $nextjsDir = "sdk/javascript/examples/nextjs-app-router"
    Push-Location $nextjsDir
    try {
        Write-Host "==> Regenerating TypeScript types..."
        # install openapi-typescript if not present
        if (-not (Test-Path "node_modules/openapi-typescript")) {
            npm install --save-dev openapi-typescript@^7 --silent
        }
        npm run generate:types:file
        if ($LASTEXITCODE -ne 0) { throw "generate:types:file failed" }
        Write-Host "    src/types/api.ts regenerated."
    } finally {
        Pop-Location
    }

    # ── Step 3: Check git diff for both generated artifacts ─────────────────
    Write-Host "==> Checking for drift..."

    # Collect changed files
    $changed = git diff --name-only openapi.json "$nextjsDir/src/types/api.ts" 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git diff failed" }

    if ([string]::IsNullOrWhiteSpace($changed)) {
        Write-Host ""
        Write-Host "[OK] No drift detected — contract is in sync."
        exit 0
    } else {
        Write-Host ""
        Write-Host "[FAIL] API drift detected in:"
        $changed -split "`n" | Where-Object { $_ -ne "" } | ForEach-Object {
            Write-Host "  - $_"
        }
        Write-Host ""
        Write-Host "To fix: regenerate and commit the updated files:"
        Write-Host "  cargo xtask openapi-gen --out openapi.json"
        Write-Host "  cd $nextjsDir && npm run generate:types:file"
        Write-Host "  git add openapi.json $nextjsDir/src/types/api.ts"
        Write-Host "  git commit -m 'chore: regenerate API contract'"
        exit 1
    }

} finally {
    Pop-Location
}
