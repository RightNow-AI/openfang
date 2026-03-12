# Release Checklist v0.3.40

## Pre-Release

- [x] All Phase 17-18 tasks completed
- [ ] All tests passing (`cargo test --workspace`)
- [ ] Zero clippy warnings (`cargo clippy --workspace --all-targets -- -D warnings`)
- [x] Documentation updated
  - [x] README.md
  - [x] CHANGELOG.md
  - [x] ARCHITECTURE.md
  - [x] ROADMAP.md
  - [x] docs/software-engineering-agent.md
  - [x] docs/evaluation.md
  - [x] docs/swe-integration-summary.md
- [x] Version updated to 0.3.40
- [x] Release notes created
- [ ] Release tag prepared

## Build Verification

```bash
# Full workspace build
cargo build --workspace --lib

# Clippy check
cargo clippy --workspace --all-targets -- -D warnings

# Run tests
cargo test --workspace --lib
```

## Feature Verification

```bash
# Start daemon
GROQ_API_KEY=<key> target/release/openfang start &
sleep 6

# Test health
curl -s http://127.0.0.1:4200/api/health

# Test SWE API
curl -s http://127.0.0.1:4200/api/swe/tasks
curl -s -X POST http://127.0.0.1:4200/api/swe/tasks \
  -H "Content-Type: application/json" \
  -d '{"description": "Test task"}'

# Test evaluation
curl -s "http://127.0.0.1:4200/api/swe/evaluate?suite=basic"

# Test supervisor delegation
curl -s -X POST http://127.0.0.1:4200/api/supervisor/delegate \
  -H "Content-Type: application/json" \
  -d '{"description": "Read the README"}'

# Stop daemon
# (find PID and kill)
```

## Dashboard Verification

1. Open browser to `http://127.0.0.1:4200/`
2. Navigate to Software Engineer tab (`#swe`)
3. Verify task input form works
4. Create a test task
5. Verify task appears in Active Tasks
6. Cancel the task
7. Verify it moves to history
8. Test evaluation UI
9. Select a suite and run
10. Verify results display correctly

## Documentation Verification

```bash
# Verify all docs exist
ls -la README.md CHANGELOG.md ARCHITECTURE.md ROADMAP.md
ls -la docs/software-engineering-agent.md docs/evaluation.md
ls -la RELEASE_NOTES_0.3.40.md
```

## Git Verification

```bash
# Check branch
git branch --show-current

# Check version in Cargo.toml
grep "version = \"0.3.40\"" Cargo.toml

# Check uncommitted changes
git status

# Check commit history
git log --oneline -5
```

## Release Tags

```bash
# Create tag
git tag -a v0.3.40 -m "Release v0.3.40: SWE Agent Framework"

# Push tag
git push origin v0.3.40
```

## Post-Release

- [ ] Create GitHub release
- [ ] Publish to crates.io (if applicable)
- [ ] Announce on communication channels
- [ ] Update website/docs
- [ ] Monitor for issues

## Files Changed Summary

| File | Action | Lines Changed |
|------|--------|---------------|
| Cargo.toml | Modified | +1 |
| README.md | Modified | +17 |
| CHANGELOG.md | Modified | +19 |
| ARCHITECTURE.md | Modified | +18 |
| ROADMAP.md | Modified | +4 |
| docs/software-engineering-agent.md | Modified | +41 |
| docs/evaluation.md | Created | +250 |
| docs/swe-integration-summary.md | Modified | +16 |
| RELEASE_NOTES_0.3.40.md | Created | +120 |
| crates/maestro-eval/src/swe.rs | Created | +331 |
| crates/maestro-eval/src/swe_runner.rs | Created | +406 |
| crates/maestro-eval/src/swe_suites.rs | Created | +422 |
| crates/openfang-api/src/swe_routes.rs | Modified | +92 |
| crates/openfang-api/src/server.rs | Modified | +9 |
| crates/openfang-api/static/index_body.html | Modified | +54 |
| crates/openfang-api/static/js/pages/swe.js | Modified | +32 |

## Rollback Plan

If critical issues are found:

1. Revert merge commit
2. Re-create release tag pointing to previous version
3. Update GitHub release notes
4. Announce rollback on communication channels