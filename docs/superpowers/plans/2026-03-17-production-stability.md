# OpenFang Production Stability Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix API service startup issues and ensure production-grade reliability with proper service management, health checks, and monitoring.

**Architecture:** Fix default port bug in config (50051→4200), clean up zombie processes, update launchd service configuration for auto-restart, add health check script, and verify with complete integration tests.

**Tech Stack:** Rust, macOS launchd, bash scripts, curl for health checks

---

## Chunk 1: Code Fixes and Compilation

### Task 1: Fix Default Port in KernelConfig

**Files:**
- Modify: `crates/openfang-types/src/config.rs:1270` (Default impl)
- Modify: `crates/openfang-types/src/config.rs:3415` (test assertion)

- [ ] **Step 1: Locate and read the Default implementation**

Run: `grep -n "impl Default for KernelConfig" crates/openfang-types/src/config.rs`
Expected: Line number around 1270

- [ ] **Step 2: Change default port from 50051 to 4200**

In `crates/openfang-types/src/config.rs`, find the line:
```rust
api_listen: "127.0.0.1:50051".to_string(),
```

Change to:
```rust
api_listen: "127.0.0.1:4200".to_string(),
```

- [ ] **Step 3: Update test assertion**

In `crates/openfang-types/src/config.rs` at line ~3415, find:
```rust
assert_eq!(config.api_listen, "127.0.0.1:50051");
```

Change to:
```rust
assert_eq!(config.api_listen, "127.0.0.1:4200");
```

- [ ] **Step 4: Verify changes compile**

Run: `cargo build --workspace --lib`
Expected: Compiles successfully with no errors

- [ ] **Step 5: Run tests to verify fix**

Run: `cargo test --workspace`
Expected: All tests pass, including `test_default_config`

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: No warnings

- [ ] **Step 7: Commit code fix**

```bash
git add crates/openfang-types/src/config.rs
git commit -m "fix(config): change default api_listen port from 50051 to 4200

Aligns default port with documentation, CLI help text, and code comments.
Updates test assertion to match new default.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Chunk 2: Configuration and Process Cleanup

### Task 2: Backup and Update User Configuration

**Files:**
- Modify: `~/.openfang/config.toml`
- Create: `~/.openfang/config.toml.bak-YYYYMMDD-HHMMSS`

- [ ] **Step 1: Create timestamped backup**

```bash
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
cp ~/.openfang/config.toml ~/.openfang/config.toml.bak-${TIMESTAMP}
```

Expected: Backup file created

- [ ] **Step 2: Verify backup exists**

Run: `ls -la ~/.openfang/config.toml.bak-*`
Expected: Shows the new backup file

- [ ] **Step 3: Update api_listen in config.toml**

```bash
sed -i.tmp 's/api_listen = "127.0.0.1:50051"/api_listen = "127.0.0.1:4200"/' ~/.openfang/config.toml
rm ~/.openfang/config.toml.tmp
```

- [ ] **Step 4: Verify config change**

Run: `grep api_listen ~/.openfang/config.toml`
Expected: Shows `api_listen = "127.0.0.1:4200"`

- [ ] **Step 5: Clean up old backups (keep last 5)**

```bash
cd ~/.openfang
ls -t config.toml.bak-* 2>/dev/null | tail -n +6 | xargs rm -f
```

Expected: Only 5 most recent backups remain

### Task 3: Clean Up Zombie Processes

**Files:**
- None (process management only)

- [ ] **Step 1: List all openfang processes**

Run: `ps aux | grep openfang | grep -v grep`
Expected: Shows list of running openfang processes

- [ ] **Step 2: Stop launchd service first**

```bash
launchctl stop ai.openfang.daemon
sleep 2
```

Expected: Service stops gracefully

- [ ] **Step 3: Send SIGTERM to remaining processes**

```bash
pkill -TERM -f "openfang" || true
sleep 10
```

Expected: Processes begin shutting down

- [ ] **Step 4: Force kill any remaining processes**

```bash
pkill -KILL -f "openfang" || true
sleep 2
```

Expected: All openfang processes terminated

- [ ] **Step 5: Verify no processes remain**

Run: `ps aux | grep openfang | grep -v grep`
Expected: No output (all processes gone)

- [ ] **Step 6: Clean up lock files**

```bash
rm -f ~/.openfang/.external-hands-reconcile.lock
```

Expected: Lock file removed

- [ ] **Step 7: Verify port 4200 is free**

Run: `lsof -i :4200`
Expected: No output (port is free)

- [ ] **Step 8: Wait for port release**

```bash
for i in {1..5}; do
  if ! lsof -i :4200 >/dev/null 2>&1; then
    echo "Port 4200 is free"
    break
  fi
  sleep 1
done
```

Expected: "Port 4200 is free"

---

## Chunk 3: Service Configuration and Health Checks

### Task 4: Update launchd Service Configuration

**Files:**
- Modify: `~/Library/LaunchAgents/ai.openfang.daemon.plist`

- [ ] **Step 1: Unload existing service**

```bash
launchctl unload ~/Library/LaunchAgents/ai.openfang.daemon.plist
```

Expected: Service unloaded

- [ ] **Step 2: Backup existing plist**

```bash
cp ~/Library/LaunchAgents/ai.openfang.daemon.plist ~/Library/LaunchAgents/ai.openfang.daemon.plist.bak
```

Expected: Backup created

- [ ] **Step 3: Update plist with improved configuration**

Create new plist content:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ai.openfang.daemon</string>

    <key>ProgramArguments</key>
    <array>
        <string>/Users/xiaomo/.openfang/bin/openfang-daemon-runner.sh</string>
    </array>

    <key>WorkingDirectory</key>
    <string>/Users/xiaomo</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>/Users/xiaomo</string>
        <key>PATH</key>
        <string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
    </dict>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>

    <key>ThrottleInterval</key>
    <integer>60</integer>

    <key>ProcessType</key>
    <string>Background</string>

    <key>StandardOutPath</key>
    <string>/Users/xiaomo/Library/Logs/openfang-daemon.out.log</string>

    <key>StandardErrorPath</key>
    <string>/Users/xiaomo/Library/Logs/openfang-daemon.err.log</string>
</dict>
</plist>
```

Write to: `~/Library/LaunchAgents/ai.openfang.daemon.plist`

- [ ] **Step 4: Verify plist syntax**

Run: `plutil -lint ~/Library/LaunchAgents/ai.openfang.daemon.plist`
Expected: "OK"

- [ ] **Step 5: Load updated service**

```bash
launchctl load ~/Library/LaunchAgents/ai.openfang.daemon.plist
```

Expected: Service loaded successfully

- [ ] **Step 6: Verify service is loaded**

Run: `launchctl list | grep openfang`
Expected: Shows `ai.openfang.daemon` with PID

### Task 5: Create Health Check Script

**Files:**
- Create: `~/.openfang/bin/health-check.sh`

- [ ] **Step 1: Create health check script**

```bash
cat > ~/.openfang/bin/health-check.sh << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

API_URL="http://127.0.0.1:4200/api/health"
TIMEOUT=5

# Check 1: API endpoint responds
if ! curl -sf --max-time "${TIMEOUT}" "${API_URL}" | grep -q '"status":"ok"'; then
    echo "FAIL: API health endpoint not responding" >&2
    exit 1
fi

# Check 2: Process is alive
if ! pgrep -f "openfang" >/dev/null; then
    echo "FAIL: openfang process not running" >&2
    exit 2
fi

# Check 3: Port is listening
if ! lsof -i :4200 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "FAIL: Port 4200 not listening" >&2
    exit 3
fi

echo "OK: All health checks passed"
exit 0
EOF
```

- [ ] **Step 2: Make script executable**

```bash
chmod +x ~/.openfang/bin/health-check.sh
```

Expected: Script is executable

- [ ] **Step 3: Verify script syntax**

Run: `bash -n ~/.openfang/bin/health-check.sh`
Expected: No output (syntax OK)

---

## Chunk 4: Integration Testing and Verification

### Task 6: Build and Start Daemon

**Files:**
- Build: `target/release/openfang`

- [ ] **Step 1: Build release binary**

Run: `cargo build --release -p openfang-cli`
Expected: Builds successfully

- [ ] **Step 2: Verify binary exists**

Run: `ls -lh target/release/openfang`
Expected: Shows binary file (~39M)

- [ ] **Step 3: Wait for service to start**

```bash
sleep 6
```

Expected: Daemon has time to initialize

- [ ] **Step 4: Check if daemon is running**

Run: `ps aux | grep "openfang start" | grep -v grep`
Expected: Shows running process

- [ ] **Step 5: Verify daemon.json was created**

Run: `cat ~/.openfang/daemon.json`
Expected: Shows JSON with pid, listen_addr (4200), version

### Task 7: Run Integration Tests

**Files:**
- None (testing only)

- [ ] **Step 1: Test health endpoint**

Run: `curl -s http://127.0.0.1:4200/api/health`
Expected: `{"status":"ok","version":"0.4.4",...}`

- [ ] **Step 2: Test agents list endpoint**

Run: `curl -s http://127.0.0.1:4200/api/agents`
Expected: JSON array of agents

- [ ] **Step 3: Get first agent ID**

```bash
AGENT_ID=$(curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; print(json.load(sys.stdin)[0]['id'])")
echo "Agent ID: ${AGENT_ID}"
```

Expected: Prints agent ID

- [ ] **Step 4: Send test message to agent (requires GROQ_API_KEY)**

```bash
curl -s -X POST "http://127.0.0.1:4200/api/agents/${AGENT_ID}/message" \
  -H "Content-Type: application/json" \
  -d '{"message": "Say hello in 5 words."}' | python3 -m json.tool
```

Expected: JSON response with LLM reply

- [ ] **Step 5: Verify budget tracking updated**

Run: `curl -s http://127.0.0.1:4200/api/budget | python3 -m json.tool`
Expected: Shows cost > 0

- [ ] **Step 6: Check per-agent budget**

Run: `curl -s http://127.0.0.1:4200/api/budget/agents | python3 -m json.tool`
Expected: Shows agent with cost data

- [ ] **Step 7: Run health check script**

Run: `~/.openfang/bin/health-check.sh`
Expected: "OK: All health checks passed" (exit 0)

### Task 8: Verify launchd Service

**Files:**
- None (verification only)

- [ ] **Step 1: Check service status**

Run: `launchctl list | grep openfang`
Expected: Shows `ai.openfang.daemon` with PID and status 0

- [ ] **Step 2: Print service details**

Run: `launchctl print gui/$(id -u)/ai.openfang.daemon`
Expected: Shows service configuration and state

- [ ] **Step 3: Check log files exist**

Run: `ls -lh ~/Library/Logs/openfang-daemon.*.log`
Expected: Shows stdout and stderr log files

- [ ] **Step 4: Verify logs contain startup messages**

Run: `tail -20 ~/Library/Logs/openfang-daemon.out.log`
Expected: Shows recent daemon startup logs

- [ ] **Step 5: Test crash recovery (manual)**

```bash
# Kill daemon process
pkill -9 -f "openfang start"
# Wait for throttle interval + restart
sleep 70
# Check if restarted
ps aux | grep "openfang start" | grep -v grep
```

Expected: Process restarted automatically

- [ ] **Step 6: Verify health check after restart**

Run: `~/.openfang/bin/health-check.sh`
Expected: "OK: All health checks passed"

### Task 9: Final Verification and Commit

**Files:**
- None (verification and git only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 2: Run clippy final check**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Verify all acceptance criteria**

Checklist:
- [x] Code compiles without errors or warnings
- [x] All unit tests pass
- [x] API responds on port 4200
- [x] Real LLM call succeeds
- [x] Budget tracking works
- [x] launchd service running
- [x] Health check script passes
- [x] Logs recording properly
- [x] Auto-restart works after crash

- [ ] **Step 4: Create final commit**

```bash
git add -A
git commit -m "feat: complete production stability improvements

- Fix default API port from 50051 to 4200
- Update user config and launchd service
- Add health check script
- Verify with complete integration tests

All acceptance criteria met. System ready for production.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

- [ ] **Step 5: Tag release (optional)**

```bash
git tag -a v0.4.5 -m "Production stability release

- Fixed default port configuration
- Added launchd auto-restart
- Added health monitoring
- Complete integration test coverage"
```

---

## Rollback Procedure

If any step fails and you need to rollback:

1. **Restore config backup:**
   ```bash
   cp ~/.openfang/config.toml.bak-* ~/.openfang/config.toml
   ```

2. **Restart service:**
   ```bash
   launchctl kickstart -k gui/$(id -u)/ai.openfang.daemon
   ```

3. **Revert code changes:**
   ```bash
   git checkout crates/openfang-types/src/config.rs
   cargo build --release -p openfang-cli
   ```

4. **Verify rollback:**
   ```bash
   curl -s http://127.0.0.1:50051/api/health
   ```

---

## Notes

- **Environment variables**: The existing `openfang-daemon` script already loads `~/.openfang/.env` (symlink to `secrets.env`) via `dotenv::load_dotenv()`. No additional wrapper needed.

- **Port conflicts**: If port 4200 is occupied, identify the process with `lsof -i :4200` and stop it before proceeding.

- **GROQ_API_KEY**: Required for LLM integration tests. Ensure it's set in `~/.openfang/secrets.env`.

- **Log rotation**: Consider adding logrotate configuration for `~/Library/Logs/openfang-daemon.*.log` to prevent unbounded growth.

---

**Plan complete. Ready for execution via superpowers:subagent-driven-development.**
