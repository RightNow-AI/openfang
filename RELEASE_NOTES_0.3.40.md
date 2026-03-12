# Release Notes v0.3.40

**Released:** March 12, 2026

## Highlights

### Software Engineering Agent

We're excited to introduce the **Software Engineering (SWE) Agent**, an autonomous agent capable of performing file operations, executing commands, and collaborating with other agents. This is a major milestone in making OpenFang a complete agent operating system.

### New Features

#### SWE Agent Core
- ✅ **File Operations:** Read and write files with safety checks
- ✅ **Command Execution:** Execute shell commands with output capture
- ✅ **Task Lifecycle:** Full CRUD operations with status tracking
- ✅ **Event Streaming:** Real-time progress updates via SSE

#### SWE Dashboard
- ✅ **Software Engineer Tab:** New UI section for task management
- ✅ **Task Queue:** Visual tracking of pending, running, and completed tasks
- ✅ **Progress Monitoring:** Real-time status updates
- ✅ **Cancel & Retry:** Full control over task execution

#### SWE API Endpoints
- ✅ `GET /api/swe/tasks` — List all tasks
- ✅ `POST /api/swe/tasks` — Create new task
- ✅ `GET /api/swe/tasks/{id}` — Get task details
- ✅ `DELETE /api/swe/tasks/{id}` — Delete task
- ✅ `POST /api/swe/tasks/{id}/cancel` — Cancel running task
- ✅ `POST /api/swe/tasks/{id}/retry` — Retry failed task
- ✅ `GET /api/swe/tasks/{id}/events` — Stream task events

#### A2A Integration
- ✅ **Supervisor Wiring:** Automatic SWE task detection and delegation
- ✅ **Handler Registry:** Direct in-process routing (bypasses serialization)
- ✅ **Task Classification:** Keyword-based detection with LLM fallback planned

#### Evaluation Suite
- ✅ **Test Types:** FileRead, FileWrite, CommandExecution, CodeGeneration, BugFix, Refactoring, MultiStep
- ✅ **Difficulty Levels:** Beginner, Intermediate, Advanced, Expert
- ✅ **Pre-defined Suites:** 17 test cases across 4 difficulty levels
- ✅ **Validation Framework:** File creation, content patterns, compilation checks
- ✅ **Scoring System:** 0.0-1.0 score with 0.8 pass threshold
- ✅ **API Endpoints:** `GET /api/swe/evaluate?suite=<name>`

## Improvements

- Enhanced agent-to-agent communication via `A2AHandlerRegistry`
- Better task routing with automatic SWE detection
- Improved observability for SWE task execution
- Extended evaluation framework with SWE-specific tests

## Bug Fixes

- Fixed maestro-swe dependency resolution
- Cleaned up binary exports in maestro-swe
- Improved error handling in task execution

## API Changes

### New Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/swe/tasks` | GET, POST | List/create SWE tasks |
| `/api/swe/tasks/{id}` | GET, DELETE | Get/delete task |
| `/api/swe/tasks/{id}/cancel` | POST | Cancel task |
| `/api/swe/tasks/{id}/retry` | POST | Retry task |
| `/api/swe/tasks/{id}/events` | GET | Get task events |
| `/api/swe/evaluate` | GET | Run evaluation suite |
| `/api/swe/evaluate/suites` | GET | List evaluation suites |
| `/api/supervisor/delegate` | POST | Delegate task to supervisor |

### New Types

- `SWETask` — Task representation with status and events
- `SWETaskStatus` — Enum: Pending, Queued, Running, Completed, Failed, Cancelled
- `SWETestCase` — Evaluation test case definition
- `SWETaskType` — Test type enum
- `SWEDifficulty` — Difficulty level enum

## Migration Guide

No breaking changes. All existing APIs remain compatible.

## Known Issues

- SWE agent has full file system access within working directory (no sandboxing)
- Long-running commands may timeout
- Evaluation tests require additional test cases for full coverage

## Contributors

- Manus AI — Phase 17-18 implementation

## Documentation

- [SWE Agent Guide](docs/software-engineering-agent.md)
- [Evaluation Framework](docs/evaluation.md)
- [Architecture Documentation](ARCHITECTURE.md)
- [API Reference](docs/api-reference.md)

---

**Previous Release:** v0.3.39 — A2A Registry & Handler System