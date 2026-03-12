# Evaluation Framework

## Overview

The evaluation framework (`maestro-eval`) provides comprehensive testing capabilities for agent performance assessment. This document focuses on the SWE (Software Engineering) Agent evaluation capabilities.

## SWE Agent Evaluation

### Test Types

The framework supports seven types of SWE tasks:

| Type | Description | Example |
|------|-------------|---------|
| `FileRead` | Read file contents and extract data | Read configuration file |
| `FileWrite` | Create or modify files | Write a new module |
| `CommandExecution` | Execute shell commands | Run tests, build code |
| `CodeGeneration` | Generate code from description | Implement a function |
| `BugFix` | Identify and fix bugs | Fix compilation error |
| `Refactoring` | Restructure code | Extract method |
| `MultiStep` | Complex multi-operation workflows | Create project structure |

### Difficulty Levels

Four difficulty levels provide progressive evaluation:

| Level | Tests | Focus Areas |
|-------|-------|-------------|
| **Beginner** | 5 | Basic file read/write, simple commands |
| **Intermediate** | 5 | Multi-file operations, code generation basics |
| **Advanced** | 4 | Code generation, bug fixing, refactoring |
| **Expert** | 3 | Project setup, trait implementation, lifetime fixes |

### Validation Checks

Each test validates multiple criteria:

- **Files Created:** Verify expected files were created
- **Files Modified:** Confirm files were updated correctly
- **Content Patterns:** Check for specific content in files
- **Command Outputs:** Validate command execution results
- **Compilation:** Verify generated code compiles (optional)

### Scoring

Scores are calculated from passed validation checks:

```
score = passed_checks / total_checks
```

- **Pass Threshold:** 0.8 (80% of checks must pass)
- **Score Range:** 0.0 to 1.0
- **Validation Details:** Each check produces a ✓ or ✗ marker

## Running Evaluations

### Via API

```bash
# Run basic test suite
curl "http://127.0.0.1:4200/api/swe/evaluate?suite=basic"

# Run intermediate suite
curl "http://127.0.0.1:4200/api/swe/evaluate?suite=intermediate"

# Run advanced suite
curl "http://127.0.0.1:4200/api/swe/evaluate?suite=advanced"

# Run expert suite
curl "http://127.0.0.1:4200/api/swe/evaluate?suite=expert"

# List available suites
curl "http://127.0.0.1:4200/api/swe/evaluate/suites"
```

### Response Format

```json
{
  "suite_id": "uuid",
  "suite_name": "basic",
  "run_id": "uuid",
  "results": [
    {
      "test_case_id": "uuid",
      "test_case_name": "Read README file",
      "passed": true,
      "score": 1.0,
      "duration_ms": 150,
      "files_created": [],
      "files_modified": [],
      "command_outputs": [],
      "validation_details": ["✓ File exists: README.md"]
    }
  ],
  "total": 5,
  "passed": 4,
  "failed": 1,
  "pass_rate": 0.8,
  "avg_score": 0.85,
  "total_duration_ms": 1250,
  "run_at": "2026-03-12T10:30:00Z"
}
```

### Via Dashboard

1. Navigate to `http://127.0.0.1:4200/#swe`
2. Scroll to "Evaluation Suite" section
3. Select test suite from dropdown
4. Click "Run Evaluation"
5. View results in the results table

## Creating Custom Tests

### Test Case Structure

Define test cases in `maestro-eval/src/swe_suites.rs`:

```rust
SWETestCase::new(
    "Custom Test Name",
    SWETaskType::CodeGeneration,
    SWETestInput::from_description("Generate a function that calculates factorial"),
)
.with_description("Test the agent's ability to generate recursive functions")
.with_difficulty(SWEDifficulty::Intermediate)
.with_setup("mkdir -p /tmp/test_project")
.with_cleanup("rm -rf /tmp/test_project")
.with_timeout(60)
.with_tag("code-gen")
```

### Expected Output Definition

```rust
SWETestExpectedOutput::files_created(vec!["src/factorial.rs".to_string()])
    .with_content_pattern("src/factorial.rs", "fn factorial")
    .with_compile_check()
```

### Test Input Options

```rust
// Simple description
SWETestInput::from_description("Read the configuration file")

// With files to read
SWETestInput::with_files(
    "Parse the config",
    vec!["config.toml".to_string()]
)
```

## Test Suites Reference

### Basic Suite

| Test | Type | Description |
|------|------|-------------|
| basic-read-001 | FileRead | Read README.md file |
| basic-write-001 | FileWrite | Create a simple text file |
| basic-cmd-001 | CommandExecution | List directory contents |
| basic-cmd-002 | CommandExecution | Echo test |
| basic-read-002 | FileRead | Read Cargo.toml |

### Intermediate Suite

| Test | Type | Description |
|------|------|-------------|
| inter-multi-001 | MultiStep | Read and modify multiple files |
| inter-write-001 | FileWrite | Create module structure |
| inter-gen-001 | CodeGeneration | Generate simple function |
| inter-cmd-001 | CommandExecution | Run cargo check |
| inter-modify-001 | FileWrite | Modify existing file |

### Advanced Suite

| Test | Type | Description |
|------|------|-------------|
| adv-gen-001 | CodeGeneration | Implement data structure |
| adv-fix-001 | BugFix | Fix compilation error |
| adv-refactor-001 | Refactoring | Extract function |
| adv-multi-001 | MultiStep | Create feature module |

### Expert Suite

| Test | Type | Description |
|------|------|-------------|
| exp-setup-001 | MultiStep | Initialize new project |
| exp-trait-001 | CodeGeneration | Implement trait with lifetimes |
| exp-fix-001 | BugFix | Fix lifetime annotation error |

## Metrics

### Available Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| Pass Rate | Percentage of tests passed | % |
| Average Score | Mean score across all tests | 0.0-1.0 |
| Duration | Time to complete test | milliseconds |
| Files Created | Count of files created | number |
| Files Modified | Count of files modified | number |

### Performance Tracking

Track evaluation results over time to identify:

- Regression in agent capabilities
- Performance improvements from changes
- Areas needing additional training
- Optimal model configuration

## Future Enhancements

- **Regression Testing:** Automated detection of capability regression
- **Performance Benchmarking:** Compare across model versions
- **Continuous Integration:** Run evaluations on every commit
- **Comparative Evaluation:** Test multiple models side-by-side
- **Custom Validators:** User-defined validation functions
- **LLM-as-Judge:** Use LLM to evaluate subjective outputs