# OpenFang installer for Windows
# Usage: iwr -useb https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/main/scripts/install.ps1 | iex
#   or:  powershell -c "irm https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/main/scripts/install.ps1 | iex"
# Tagged fallback example: irm https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/v<release-tag>/scripts/install.ps1 | iex
#
# Flags (via environment variables):
#   $env:OPENFANG_INSTALL_DIR = custom install directory
#   $env:OPENFANG_VERSION     = specific version tag (e.g. "v0.4.4")
#   $env:OPENFANG_RELEASE_REPO = override release repo (default: tytsxai/openfang-upstream-fork)

$ErrorActionPreference = 'Stop'

$Repo = if ($env:OPENFANG_RELEASE_REPO) { $env:OPENFANG_RELEASE_REPO } else { "tytsxai/openfang-upstream-fork" }
$DefaultInstallDir = Join-Path $env:USERPROFILE ".openfang\bin"
$InstallDir = if ($env:OPENFANG_INSTALL_DIR) { $env:OPENFANG_INSTALL_DIR } else { $DefaultInstallDir }

function Write-Banner {
    Write-Host ""
    Write-Host "  OpenFang Installer" -ForegroundColor Cyan
    Write-Host "  ==================" -ForegroundColor Cyan
    Write-Host ""
}

function Get-Architecture {
    # Try multiple detection methods — piped iex can break some approaches
    $arch = ""

    # Method 1: .NET RuntimeInformation
    try {
        $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    } catch {}

    # Method 2: PROCESSOR_ARCHITECTURE env var
    if (-not $arch -or $arch -eq "") {
        try { $arch = $env:PROCESSOR_ARCHITECTURE } catch {}
    }

    # Method 3: WMI
    if (-not $arch -or $arch -eq "") {
        try {
            $wmiArch = (Get-CimInstance Win32_Processor).Architecture
            if ($wmiArch -eq 9) { $arch = "AMD64" }
            elseif ($wmiArch -eq 12) { $arch = "ARM64" }
        } catch {}
    }

    # Method 4: pointer size fallback (64-bit = 8 bytes)
    if (-not $arch -or $arch -eq "") {
        if ([IntPtr]::Size -eq 8) { $arch = "X64" }
    }

    $archUpper = "$arch".ToUpper().Trim()
    switch ($archUpper) {
        { $_ -in "X64", "AMD64", "X86_64" }     { return "x86_64" }
        { $_ -in "ARM64", "AARCH64", "ARM" }     { return "aarch64" }
        default {
            Write-Host "  Unsupported architecture: $arch (detection may have failed)" -ForegroundColor Red
            Write-Host "  Try: cargo install --git https://github.com/$Repo openfang-cli" -ForegroundColor Yellow
            exit 1
        }
    }
}

function Get-LatestVersion {
    if ($env:OPENFANG_VERSION) {
        return $env:OPENFANG_VERSION
    }

    Write-Host "  Fetching latest release..."
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
        return $release.tag_name
    }
    catch {
        Write-Host "  Could not determine latest version." -ForegroundColor Red
        Write-Host "  Install from source instead:" -ForegroundColor Yellow
        Write-Host "    cargo install --git https://github.com/$Repo openfang-cli"
        exit 1
    }
}

function Install-OpenFang {
    Write-Banner

    $arch = Get-Architecture
    $version = Get-LatestVersion
    $target = "${arch}-pc-windows-msvc"
    $archive = "openfang-${target}.zip"
    $url = "https://github.com/$Repo/releases/download/$version/$archive"
    $checksumUrl = "$url.sha256"

    Write-Host "  Installing OpenFang $version for $target..."

    # Create install directory
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    # Download to temp
    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) "openfang-install"
    if (Test-Path $tempDir) { Remove-Item -Recurse -Force $tempDir }
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    $archivePath = Join-Path $tempDir $archive
    $checksumPath = Join-Path $tempDir "$archive.sha256"

    try {
        Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing
    }
    catch {
        Write-Host "  Download failed. The release may not exist for your platform." -ForegroundColor Red
        Write-Host "  Install from source instead:" -ForegroundColor Yellow
        Write-Host "    cargo install --git https://github.com/$Repo openfang-cli"
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
        exit 1
    }

    # Verify checksum if available
    $checksumDownloaded = $false
    try {
        Invoke-WebRequest -Uri $checksumUrl -OutFile $checksumPath -UseBasicParsing
        $checksumDownloaded = $true
    }
    catch {
        Write-Host "  Checksum file not available, skipping verification." -ForegroundColor Yellow
    }
    if ($checksumDownloaded) {
        $expectedHash = (Get-Content $checksumPath -Raw).Split(" ")[0].Trim().ToLower()
        $actualHash = (Get-FileHash $archivePath -Algorithm SHA256).Hash.ToLower()
        if ($expectedHash -ne $actualHash) {
            Write-Host "  Checksum verification FAILED!" -ForegroundColor Red
            Write-Host "    Expected: $expectedHash" -ForegroundColor Red
            Write-Host "    Got:      $actualHash" -ForegroundColor Red
            Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
            exit 1
        }
        Write-Host "  Checksum verified." -ForegroundColor Green
    }

    # Extract
    Expand-Archive -Path $archivePath -DestinationPath $tempDir -Force
    $exePath = Join-Path $tempDir "openfang.exe"
    if (-not (Test-Path $exePath)) {
        # May be nested in a directory
        $found = Get-ChildItem -Path $tempDir -Filter "openfang.exe" -Recurse | Select-Object -First 1
        if ($found) {
            $exePath = $found.FullName
        }
        else {
            Write-Host "  Could not find openfang.exe in archive." -ForegroundColor Red
            Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
            exit 1
        }
    }

    # Install
    Copy-Item -Path $exePath -Destination (Join-Path $InstallDir "openfang.exe") -Force

    # Clean up temp
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue

    # Add to user PATH if not already present
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($currentPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$currentPath", "User")
        Write-Host "  Added $InstallDir to user PATH." -ForegroundColor Green
        Write-Host "  Restart your terminal for PATH changes to take effect." -ForegroundColor Yellow
    }

    # Verify
    $installedExe = Join-Path $InstallDir "openfang.exe"
    if (Test-Path $installedExe) {
        try {
            $versionOutput = & $installedExe --version 2>&1
            Write-Host ""
            Write-Host "  OpenFang installed successfully! ($versionOutput)" -ForegroundColor Green
        }
        catch {
            Write-Host ""
            Write-Host "  OpenFang binary installed to $installedExe" -ForegroundColor Green
        }
    }

    Write-Host ""
    Write-Host "  Get started:" -ForegroundColor Cyan
    Write-Host "    openfang init"
    Write-Host ""
    Write-Host "  The setup wizard will guide you through provider selection"
    Write-Host "  and configuration."
    Write-Host ""
}

Install-OpenFang
