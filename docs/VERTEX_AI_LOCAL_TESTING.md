# Vertex AI Local Testing Guide

## Prerequisites

1. **GCP Service Account JSON** at `C:\Users\at384\Downloads\osc\dbg-grcit-dev-e1-c79e5571a5a7.json`
2. **gcloud CLI** installed and in PATH
3. **Rust toolchain** with cargo

## Quick Start (Recommended)

### Option 1: Use the Batch File

```batch
# Run this from the openfang directory:
start-vertex.bat
```

This automatically:
- Clears proxy settings
- Sets `GOOGLE_APPLICATION_CREDENTIALS`
- Pre-fetches OAuth token via `gcloud auth print-access-token`
- Sets `VERTEX_AI_ACCESS_TOKEN` env var
- Starts OpenFang

### Option 2: Manual PowerShell Setup

```powershell
# 1. Kill any existing instances
taskkill /F /IM openfang.exe 2>$null

# 2. Set environment variables (CRITICAL: clear proxy!)
$env:HTTPS_PROXY = ""
$env:HTTP_PROXY = ""
$env:GOOGLE_APPLICATION_CREDENTIALS = "C:\Users\at384\Downloads\osc\dbg-grcit-dev-e1-c79e5571a5a7.json"

# 3. Pre-fetch OAuth token (IMPORTANT: avoids subprocess issues on Windows)
$env:VERTEX_AI_ACCESS_TOKEN = gcloud auth print-access-token

# 4. Start OpenFang
cd C:\Users\at384\Downloads\osc\dllm\openfang
.\target\debug\openfang.exe start
```

## Testing the API

### Create an Agent

```powershell
$env:HTTPS_PROXY = ""
$env:HTTP_PROXY = ""

# Spawn agent with default Vertex AI provider (from config.toml)
$body = '{"manifest_toml":"name = \"test-agent\"\nmode = \"assistant\""}'
Invoke-RestMethod -Uri "http://127.0.0.1:50051/api/agents" -Method POST -ContentType "application/json" -Body $body
```

### Send Chat Request

```powershell
$env:HTTPS_PROXY = ""
$env:HTTP_PROXY = ""

$body = '{"model":"test-agent","messages":[{"role":"user","content":"What is 2+2?"}]}'
$response = Invoke-RestMethod -Uri "http://127.0.0.1:50051/v1/chat/completions" -Method POST -ContentType "application/json" -Body $body -TimeoutSec 120
Write-Host $response.choices[0].message.content
```

### Direct Vertex AI Test (Bypass OpenFang)

```powershell
$env:HTTPS_PROXY = ""
$env:HTTP_PROXY = ""

$token = gcloud auth print-access-token
$project = "dbg-grcit-dev-e1"
$region = "us-central1"
$model = "gemini-2.0-flash"
$url = "https://$region-aiplatform.googleapis.com/v1/projects/$project/locations/$region/publishers/google/models/$($model):generateContent"

$body = @{contents = @(@{role = "user"; parts = @(@{text = "Hello!"})})} | ConvertTo-Json -Depth 5
Invoke-RestMethod -Uri $url -Method POST -Headers @{Authorization = "Bearer $token"} -ContentType "application/json" -Body $body
```

## Configuration

### ~/.openfang/config.toml

```toml
[default_model]
provider = "vertex-ai"
model = "gemini-2.0-flash"

[memory]
decay_rate = 0.05

[network]
listen_addr = "127.0.0.1:4200"
```

## Environment Variables

| Variable | Purpose | Required |
|----------|---------|----------|
| `GOOGLE_APPLICATION_CREDENTIALS` | Path to service account JSON | Yes |
| `VERTEX_AI_ACCESS_TOKEN` | Pre-fetched OAuth token (bypasses gcloud subprocess) | Recommended on Windows |
| `GOOGLE_CLOUD_PROJECT` | Override project ID | No (auto-detected from JSON) |
| `GOOGLE_CLOUD_REGION` / `VERTEX_AI_REGION` | Override region | No (defaults to us-central1) |
| `HTTPS_PROXY` / `HTTP_PROXY` | **MUST be empty** for local testing | Critical |

## Troubleshooting

### "Agent processing failed" (500 Error)

**Cause:** gcloud subprocess not working properly on Windows.

**Solution:** Pre-fetch the token:
```powershell
$env:VERTEX_AI_ACCESS_TOKEN = gcloud auth print-access-token
```

### "Connection refused"

**Cause:** OpenFang not running or wrong port.

**Solution:** Ensure server is running on port 50051:
```powershell
Get-NetTCPConnection -LocalPort 50051 -ErrorAction SilentlyContinue
```

### Token Expired

**Cause:** OAuth tokens expire after ~1 hour.

**Solution:** Re-fetch token:
```powershell
$env:VERTEX_AI_ACCESS_TOKEN = gcloud auth print-access-token
```

## Build Commands

```powershell
cd C:\Users\at384\Downloads\osc\dllm\openfang
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# Debug build (faster compilation)
cargo build -p openfang-cli

# Run tests
cargo test -p openfang-runtime --lib vertex

# Check formatting
cargo fmt --check -p openfang-runtime

# Run clippy
cargo clippy -p openfang-runtime --lib -- -W warnings
```

## API Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `http://127.0.0.1:50051/api/agents` | GET | List agents |
| `http://127.0.0.1:50051/api/agents` | POST | Create agent |
| `http://127.0.0.1:50051/api/agents/{id}` | DELETE | Delete agent |
| `http://127.0.0.1:50051/v1/chat/completions` | POST | OpenAI-compatible chat |
| `http://127.0.0.1:50051/` | GET | Dashboard UI |

## Files Modified in PR

- `crates/openfang-runtime/src/drivers/vertex.rs` (NEW - ~790 lines)
- `crates/openfang-runtime/src/drivers/mod.rs` (+62 lines)
