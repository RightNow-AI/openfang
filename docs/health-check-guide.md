# OpenFang Health Check Guide

Quick reference for diagnosing OpenFang system health and common issues.

## Quick Health Check

Run these commands to get a complete health overview:

```bash
# 1. Check API health
curl -s http://127.0.0.1:4200/api/health

# 2. Check process status
ps aux | grep openfang | grep -v grep

# 3. Verify environment variables (for Telegram)
ps eww -p $(pgrep openfang) | tr ' ' '\n' | grep -E "TELEGRAM|NVIDIA"

# 4. Check listening ports
lsof -i :4200  # OpenFang API
lsof -i :8081  # Telegram Local Bot API (if enabled)

# 5. Check agents
curl -s http://127.0.0.1:4200/api/agents | python3 -m json.tool
```

## Expected Healthy Output

### 1. API Health
```json
{"status":"ok","version":"0.4.7"}
```

### 2. Process Status
```
xiaomo  71703  0.0  0.2  ./target/release/openfang start
xiaomo  71795  6.9  0.2  telegram-bot-api --api-id ... --http-port 8081
```

### 3. Environment Variables
```
TELEGRAM_BOT_TOKEN=8698293972:AAFT...
TELEGRAM_API_HASH=e930cb0c87...
NVIDIA_INTEGRATE_API_KEY=nvapi-...
```

**Warning Signs:**
- Empty values: `TELEGRAM_BOT_TOKEN=` ← Environment not loaded
- Missing variables: No output from grep ← Variables not exported

### 4. Listening Ports
```
openfang  71703  TCP 127.0.0.1:4200 (LISTEN)
telegram  71795  TCP *:8081 (LISTEN)
```

### 5. Agent Status
```json
[{
  "id": "3d2efa2d-ac12-512c-a946-8f9451f03feb",
  "name": "shipinfabu-hand",
  "ready": true,
  "auth_status": "configured",
  "model_name": "qwen/qwen3.5-397b-a17b"
}]
```

## Common Issues and Quick Fixes

### Issue 1: Empty Environment Variables

**Symptom:**
```bash
ps eww -p $(pgrep openfang) | grep TELEGRAM_BOT_TOKEN
# Output: TELEGRAM_BOT_TOKEN=
```

**Fix:**
```bash
kill $(pgrep openfang)
sleep 2
source .env.telegram && ./target/release/openfang start
```

### Issue 2: Telegram Local Bot API Not Running

**Symptom:**
```bash
lsof -i :8081
# No output
```

**Check config:**
```bash
grep -A 5 "use_local_api" ~/.openfang/config.toml
```

**Should see:**
```toml
use_local_api = true
auto_start_local_api = true
telegram_api_id = "31033835"
telegram_api_hash_env = "TELEGRAM_API_HASH"
local_api_port = 8081
```

### Issue 3: Agent Not Ready

**Symptom:**
```json
{"ready": false, "auth_status": "missing"}
```

**Check:**
1. Model provider API key is set
2. Config has correct `api_key_env` reference
3. Environment variable exists

```bash
# Check config
grep -A 5 "default_model" ~/.openfang/config.toml

# Verify env var
env | grep NVIDIA_INTEGRATE_API_KEY
```

### Issue 4: Port Already in Use

**Symptom:**
```
Error: Address already in use (os error 48)
```

**Fix:**
```bash
# Find process using port 4200
lsof -i :4200

# Kill old process
kill -9 <PID>

# Restart
./target/release/openfang start
```

## Telegram-Specific Health Checks

### Check Telegram Connection

```bash
# View logs for Telegram status
tail -50 /tmp/openfang.log | grep -i telegram
```

**Expected output:**
```
INFO openfang_channels::telegram: Telegram bot @linyiagibot connected
INFO openfang_channels::telegram: Telegram: cleared webhook, polling mode active
INFO openfang_channels::telegram: Telegram polling loop started
INFO openfang_channels::telegram: Telegram getUpdates returned messages count=11
```

### Check Telegram Downloads

```bash
# Check download directory
ls -lh ~/.openfang/workspaces/*/data/telegram-intake/

# Check inbox manifests
ls -lh ~/.openfang/workspaces/*/inbox/telegram/*.json
```

### Verify Telegram Bot Token

```bash
# Test token directly with Telegram API
curl -s "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/getMe" | python3 -m json.tool
```

**Expected:**
```json
{
  "ok": true,
  "result": {
    "id": 8698293972,
    "is_bot": true,
    "first_name": "linyiagi",
    "username": "linyiagibot"
  }
}
```

## Dashboard Health Check

### Access Dashboard
```bash
open http://127.0.0.1:4200/
```

### Check Dashboard Data Loading

```bash
# Verify HTML loads
curl -s http://127.0.0.1:4200/ | grep -o "<title>.*</title>"
# Expected: <title>OpenFang Dashboard</title>

# Check API endpoints
curl -s http://127.0.0.1:4200/api/status
curl -s http://127.0.0.1:4200/api/agents
```

## Log Locations

| Component | Log Location |
|-----------|-------------|
| OpenFang daemon | `/tmp/openfang.log` (if started with nohup) |
| Telegram Local Bot API | Embedded in OpenFang logs |
| Agent workspace | `~/.openfang/workspaces/<agent-name>/logs/` |
| API requests | Embedded in OpenFang logs (INFO level) |

## Performance Metrics

### Check Resource Usage

```bash
# CPU and Memory
ps aux | grep -E "openfang|telegram-bot-api" | grep -v grep

# Disk usage
du -sh ~/.openfang/
du -sh ~/.openfang/workspaces/*/data/telegram-intake/
```

### Check API Response Times

```bash
# Health endpoint (should be <5ms)
time curl -s http://127.0.0.1:4200/api/health

# Agents list (should be <50ms)
time curl -s http://127.0.0.1:4200/api/agents
```

## Automated Health Check Script

Save as `check-openfang-health.sh`:

```bash
#!/bin/bash

echo "=== OpenFang Health Check ==="
echo

echo "1. API Health:"
curl -s http://127.0.0.1:4200/api/health || echo "❌ API not responding"
echo

echo "2. Process Status:"
ps aux | grep openfang | grep -v grep || echo "❌ No OpenFang process"
echo

echo "3. Environment Variables:"
ps eww -p $(pgrep openfang) 2>/dev/null | tr ' ' '\n' | grep -E "TELEGRAM_BOT_TOKEN|TELEGRAM_API_HASH" | sed 's/=.*/=***/' || echo "❌ Cannot read process environment"
echo

echo "4. Listening Ports:"
lsof -i :4200 | grep LISTEN && echo "✅ API port 4200 listening" || echo "❌ Port 4200 not listening"
lsof -i :8081 | grep LISTEN && echo "✅ Telegram Bot API port 8081 listening" || echo "⚠️  Port 8081 not listening (OK if not using Local Bot API)"
echo

echo "5. Agents:"
curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; agents=json.load(sys.stdin); print(f'Total: {len(agents)}'); [print(f'  - {a[\"name\"]}: ready={a.get(\"ready\")}, auth={a.get(\"auth_status\")}') for a in agents]" || echo "❌ Cannot fetch agents"
echo

echo "6. Recent Telegram Activity:"
tail -20 /tmp/openfang.log 2>/dev/null | grep -i telegram | tail -5 || echo "⚠️  No recent Telegram logs"
echo

echo "=== Health Check Complete ==="
```

Run with:
```bash
chmod +x check-openfang-health.sh
./check-openfang-health.sh
```

## When to Restart

Restart OpenFang if:
- Environment variables are empty
- API health check fails
- Telegram bot not responding after config change
- Port conflicts detected
- After updating config files that require restart (see troubleshooting.md section 9)

## See Also

- [troubleshooting.md](troubleshooting.md) - Detailed troubleshooting guide
- [telegram-deployment-guide.md](telegram-deployment-guide.md) - Telegram setup
- [operations-runbook.md](operations-runbook.md) - Production operations
