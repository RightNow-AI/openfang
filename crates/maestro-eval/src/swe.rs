//! SWE (Software Engineering) Test Cases and Runner
//!
//! This module provides specialized test types for evaluating SWE agent
//! capabilities like file operations, code generation, bug fixing, and
//! refactoring.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// SWE Task Types
// ---------------------------------------------------------------------------

/// The type of SWE task being tested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SWETaskType {
    /// Read file contents
    FileRead,
    /// Write/create a file
    FileWrite,
    /// Execute a shell command
    CommandExecution,
    /// Generate code from a description
    CodeGeneration,
    /// Find and fix a bug
    BugFix,
    /// Refactor existing code
    Refactoring,
    /// Multi-step task involving multiple operations
    MultiStep,
}

/// Difficulty level for test cases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SWEDifficulty {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

// ---------------------------------------------------------------------------
// SWE Test Case
// ---------------------------------------------------------------------------

/// A test case for evaluating SWE agent behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SWETestCase {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Detailed description of the test
    pub description: String,
    /// Category of SWE task
    pub task_type: SWETaskType,
    /// Input specification
    pub input: SWETestInput,
    /// Expected output specification
    pub expected: SWETestExpectedOutput,
    /// Setup commands to run before the test
    pub setup: Option<String>,
    /// Cleanup commands to run after the test
    pub cleanup: Option<String>,
    /// Maximum time allowed in seconds
    pub timeout_seconds: u64,
    /// Difficulty level
    pub difficulty: SWEDifficulty,
    /// Tags for filtering
    pub tags: Vec<String>,
}

impl SWETestCase {
    /// Create a new SWE test case with defaults.
    pub fn new(
        name: impl Into<String>,
        task_type: SWETaskType,
        input: SWETestInput,
        expected: SWETestExpectedOutput,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            task_type,
            input,
            expected,
            setup: None,
            cleanup: None,
            timeout_seconds: 30,
            difficulty: SWEDifficulty::Intermediate,
            tags: Vec::new(),
        }
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set setup commands.
    pub fn with_setup(mut self, setup: impl Into<String>) -> Self {
        self.setup = Some(setup.into());
        self
    }

    /// Set cleanup commands.
    pub fn with_cleanup(mut self, cleanup: impl Into<String>) -> Self {
        self.cleanup = Some(cleanup.into());
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    /// Set difficulty.
    pub fn with_difficulty(mut self, difficulty: SWEDifficulty) -> Self {
        self.difficulty = difficulty;
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

// ---------------------------------------------------------------------------
// SWE Test Input
// ---------------------------------------------------------------------------

/// Input specification for a SWE test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SWETestInput {
    /// Files the agent should read
    pub files_to_read: Vec<String>,
    /// Commands the agent should run
    pub commands_to_run: Vec<String>,
    /// Natural language task description
    pub task_description: String,
    /// Optional working directory
    pub working_directory: Option<String>,
}

impl SWETestInput {
    /// Create a simple input with just a task description.
    pub fn from_description(desc: impl Into<String>) -> Self {
        Self {
            files_to_read: Vec::new(),
            commands_to_run: Vec::new(),
            task_description: desc.into(),
            working_directory: None,
        }
    }

    /// Create an input with files to read.
    pub fn with_files(desc: impl Into<String>, files: Vec<String>) -> Self {
        Self {
            files_to_read: files,
            commands_to_run: Vec::new(),
            task_description: desc.into(),
            working_directory: None,
        }
    }
}

// ---------------------------------------------------------------------------
// SWE Test Expected Output
// ---------------------------------------------------------------------------

/// Expected output specification for validation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SWETestExpectedOutput {
    /// Files that should be created
    pub files_created: Vec<String>,
    /// Files that should be modified
    pub files_modified: Vec<String>,
    /// Expected content patterns (file path -> pattern)
    pub content_patterns: Vec<(String, String)>,
    /// Expected command output patterns
    pub command_output_patterns: Vec<String>,
    /// Whether the code should compile/run
    pub should_compile: bool,
    /// Custom validation function name
    pub custom_validator: Option<String>,
}

impl SWETestExpectedOutput {
    /// Create an empty expected output (any result accepted).
    pub fn any() -> Self {
        Self::default()
    }

    /// Expect specific files to be created.
    pub fn files_created(files: Vec<String>) -> Self {
        Self {
            files_created: files,
            ..Default::default()
        }
    }

    /// Expect a content pattern in a file.
    pub fn with_content_pattern(mut self, file: impl Into<String>, pattern: impl Into<String>) -> Self {
        self.content_patterns.push((file.into(), pattern.into()));
        self
    }

    /// Expect code to compile.
    pub fn with_compile_check(mut self) -> Self {
        self.should_compile = true;
        self
    }
}

// ---------------------------------------------------------------------------
// SWE Test Result
// ---------------------------------------------------------------------------

/// Result of running a SWE test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SWETestResult {
    /// ID of the test case
    pub test_case_id: Uuid,
    /// Name of the test case
    pub test_case_name: String,
    /// Whether the test passed
    pub passed: bool,
    /// Score from 0.0 to 1.0
    pub score: f64,
    /// Execution time in milliseconds
    pub duration_ms: u64,
    /// Actual files created
    pub files_created: Vec<String>,
    /// Actual files modified
    pub files_modified: Vec<String>,
    /// Command outputs
    pub command_outputs: Vec<(String, String, i32)>,
    /// Failure message if not passed
    pub message: Option<String>,
    /// Detailed validation results
    pub validation_details: Vec<String>,
    /// Timestamp
    pub evaluated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// SWE Test Suite
// ---------------------------------------------------------------------------

/// A collection of SWE test cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SWETestSuite {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Description
    pub description: String,
    /// Test cases
    pub test_cases: Vec<SWETestCase>,
    /// Version
    pub version: String,
    /// Pass threshold (0.0-1.0)
    pub pass_threshold: f64,
}

impl SWETestSuite {
    /// Create a new test suite.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            test_cases: Vec::new(),
            version: "1.0.0".to_string(),
            pass_threshold: 0.8,
        }
    }

    /// Add a test case.
    pub fn add_case(&mut self, case: SWETestCase) -> &mut Self {
        self.test_cases.push(case);
        self
    }
}

// ---------------------------------------------------------------------------
// SWE Suite Run Report
// ---------------------------------------------------------------------------

/// Report from running a SWE test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SWESuiteReport {
    /// Suite ID
    pub suite_id: Uuid,
    /// Suite name
    pub suite_name: String,
    /// Run ID
    pub run_id: Uuid,
    /// Individual results
    pub results: Vec<SWETestResult>,
    /// Total tests
    pub total: usize,
    /// Passed count
    pub passed: usize,
    /// Failed count
    pub failed: usize,
    /// Pass rate (0.0-1.0)
    pub pass_rate: f64,
    /// Average score
    pub avg_score: f64,
    /// Total duration in ms
    pub total_duration_ms: u64,
    /// Timestamp
    pub run_at: DateTime<Utc>,
}

impl SWESuiteReport {
    /// Check if the suite passes the threshold.
    pub fn is_passing(&self, threshold: f64) -> bool {
        self.pass_rate >= threshold
    }
}