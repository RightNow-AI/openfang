//! Pre-defined SWE Test Suites
//!
//! This module provides ready-to-use test suites for evaluating SWE agent
//! capabilities at various difficulty levels.

use crate::swe::{
    SWEDifficulty, SWETestCase, SWETestExpectedOutput, SWETestInput, SWETestSuite, SWETaskType,
};

// ---------------------------------------------------------------------------
// Basic Test Suite
// ---------------------------------------------------------------------------

/// Create the basic SWE test suite.
///
/// Covers fundamental file operations and simple commands.
pub fn create_basic_suite() -> SWETestSuite {
    let mut suite = SWETestSuite::new(
        "Basic SWE Suite",
        "Fundamental file operations and command execution tests",
    );

    // Test 1: Read a file
    suite.add_case(
        SWETestCase::new(
            "Read README file",
            SWETaskType::FileRead,
            SWETestInput::with_files(
                "Read the contents of the README.md file",
                vec!["README.md".to_string()],
            ),
            SWETestExpectedOutput::any(),
        )
        .with_description("Tests basic file reading capability")
        .with_setup("echo '# Test Project' > README.md")
        .with_cleanup("rm -f README.md")
        .with_difficulty(SWEDifficulty::Beginner)
        .with_tag("file-read")
        .with_tag("basic"),
    );

    // Test 2: Write a file
    suite.add_case(
        SWETestCase::new(
            "Create a new file",
            SWETaskType::FileWrite,
            SWETestInput::from_description("Create a new file called test.txt with content 'Hello World'"),
            SWETestExpectedOutput::files_created(vec!["test.txt".to_string()])
                .with_content_pattern("test.txt", "Hello"),
        )
        .with_cleanup("rm -f test.txt")
        .with_difficulty(SWEDifficulty::Beginner)
        .with_tag("file-write")
        .with_tag("basic"),
    );

    // Test 3: Execute a command
    suite.add_case(
        SWETestCase::new(
            "Execute echo command",
            SWETaskType::CommandExecution,
            SWETestInput::from_description("Run 'echo hello' and capture the output"),
            SWETestExpectedOutput {
                command_output_patterns: vec!["hello".to_string()],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_difficulty(SWEDifficulty::Beginner)
        .with_tag("command")
        .with_tag("basic"),
    );

    // Test 4: List directory
    suite.add_case(
        SWETestCase::new(
            "List directory contents",
            SWETaskType::CommandExecution,
            SWETestInput::from_description("List all files in the current directory"),
            SWETestExpectedOutput::any(),
        )
        .with_difficulty(SWEDifficulty::Beginner)
        .with_tag("command")
        .with_tag("basic"),
    );

    // Test 5: Check file exists
    suite.add_case(
        SWETestCase::new(
            "Check file existence",
            SWETaskType::CommandExecution,
            SWETestInput::from_description("Check if Cargo.toml exists"),
            SWETestExpectedOutput {
                command_output_patterns: vec!["Cargo.toml".to_string()],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_difficulty(SWEDifficulty::Beginner)
        .with_tag("command")
        .with_tag("basic"),
    );

    suite
}

// ---------------------------------------------------------------------------
// Intermediate Test Suite
// ---------------------------------------------------------------------------

/// Create the intermediate SWE test suite.
///
/// Covers multi-file operations, simple code generation, and error handling.
pub fn create_intermediate_suite() -> SWETestSuite {
    let mut suite = SWETestSuite::new(
        "Intermediate SWE Suite",
        "Multi-file operations, code generation, and error handling",
    );

    // Test 1: Create multiple files
    suite.add_case(
        SWETestCase::new(
            "Create multiple configuration files",
            SWETaskType::FileWrite,
            SWETestInput::from_description(
                "Create config.json with {} and settings.toml with [settings]",
            ),
            SWETestExpectedOutput::files_created(vec![
                "config.json".to_string(),
                "settings.toml".to_string(),
            ]),
        )
        .with_cleanup("rm -f config.json settings.toml")
        .with_difficulty(SWEDifficulty::Intermediate)
        .with_tag("file-write")
        .with_tag("multi-file"),
    );

    // Test 2: Modify existing file
    suite.add_case(
        SWETestCase::new(
            "Append to existing file",
            SWETaskType::FileWrite,
            SWETestInput::from_description("Add a new line to the end of notes.txt"),
            SWETestExpectedOutput::files_created(vec!["notes.txt".to_string()]),
        )
        .with_setup("echo 'First line' > notes.txt")
        .with_cleanup("rm -f notes.txt")
        .with_difficulty(SWEDifficulty::Intermediate)
        .with_tag("file-write")
        .with_tag("modify"),
    );

    // Test 3: Grep for pattern
    suite.add_case(
        SWETestCase::new(
            "Search for pattern in files",
            SWETaskType::CommandExecution,
            SWETestInput::from_description("Find all files containing 'fn main'"),
            SWETestExpectedOutput {
                command_output_patterns: vec!["main".to_string()],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_difficulty(SWEDifficulty::Intermediate)
        .with_tag("command")
        .with_tag("search"),
    );

    // Test 4: Copy file
    suite.add_case(
        SWETestCase::new(
            "Copy a file",
            SWETaskType::CommandExecution,
            SWETestInput::from_description("Copy source.txt to destination.txt"),
            SWETestExpectedOutput::files_created(vec!["destination.txt".to_string()]),
        )
        .with_setup("echo 'source content' > source.txt")
        .with_cleanup("rm -f source.txt destination.txt")
        .with_difficulty(SWEDifficulty::Intermediate)
        .with_tag("command")
        .with_tag("file-ops"),
    );

    // Test 5: Create directory structure
    suite.add_case(
        SWETestCase::new(
            "Create nested directories",
            SWETaskType::CommandExecution,
            SWETestInput::from_description("Create directory structure src/lib/utils"),
            SWETestExpectedOutput::any(),
        )
        .with_cleanup("rm -rf src")
        .with_difficulty(SWEDifficulty::Intermediate)
        .with_tag("command")
        .with_tag("directory"),
    );

    suite
}

// ---------------------------------------------------------------------------
// Advanced Test Suite
// ---------------------------------------------------------------------------

/// Create the advanced SWE test suite.
///
/// Covers code generation, bug fixing, and refactoring tasks.
pub fn create_advanced_suite() -> SWETestSuite {
    let mut suite = SWETestSuite::new(
        "Advanced SWE Suite",
        "Code generation, bug fixing, and refactoring challenges",
    );

    // Test 1: Generate a simple Rust function
    suite.add_case(
        SWETestCase::new(
            "Generate Rust function",
            SWETaskType::CodeGeneration,
            SWETestInput::from_description(
                "Create a Rust function 'add' that takes two i32 and returns their sum",
            ),
            SWETestExpectedOutput {
                files_created: vec!["src/add.rs".to_string()],
                content_patterns: vec![("src/add.rs".to_string(), "fn add".to_string())],
                should_compile: false, // Would need full crate setup
                ..SWETestExpectedOutput::default()
            },
        )
        .with_setup("mkdir -p src")
        .with_cleanup("rm -rf src")
        .with_difficulty(SWEDifficulty::Advanced)
        .with_tag("code-gen")
        .with_tag("rust"),
    );

    // Test 2: Fix a bug in code
    suite.add_case(
        SWETestCase::new(
            "Fix off-by-one error",
            SWETaskType::BugFix,
            SWETestInput::from_description(
                "Fix the bug in buggy.rs where the loop should be 0..10 not 0..=10",
            ),
            SWETestExpectedOutput {
                files_modified: vec!["buggy.rs".to_string()],
                content_patterns: vec![("buggy.rs".to_string(), "0..10".to_string())],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_setup("echo 'fn buggy() { for i in 0..=10 { println!(\"{}\", i); } }' > buggy.rs")
        .with_cleanup("rm -f buggy.rs")
        .with_difficulty(SWEDifficulty::Advanced)
        .with_tag("bug-fix"),
    );

    // Test 3: Refactor code
    suite.add_case(
        SWETestCase::new(
            "Extract function",
            SWETaskType::Refactoring,
            SWETestInput::from_description(
                "Refactor messy.rs to extract the calculation into a separate function",
            ),
            SWETestExpectedOutput {
                files_modified: vec!["messy.rs".to_string()],
                content_patterns: vec![("messy.rs".to_string(), "fn ".to_string())],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_setup("echo 'fn main() { let x = 1 + 2 + 3; println!(\"{}\", x); }' > messy.rs")
        .with_cleanup("rm -f messy.rs")
        .with_difficulty(SWEDifficulty::Advanced)
        .with_tag("refactor"),
    );

    // Test 4: Multi-file refactoring
    suite.add_case(
        SWETestCase::new(
            "Move function to module",
            SWETaskType::MultiStep,
            SWETestInput::from_description(
                "Move the helper function from main.rs to a new module helpers.rs and update imports",
            ),
            SWETestExpectedOutput {
                files_created: vec!["helpers.rs".to_string()],
                files_modified: vec!["main.rs".to_string()],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_setup("echo 'fn helper() {} fn main() { helper(); }' > main.rs")
        .with_cleanup("rm -f main.rs helpers.rs")
        .with_difficulty(SWEDifficulty::Advanced)
        .with_tag("refactor")
        .with_tag("multi-file"),
    );

    suite
}

// ---------------------------------------------------------------------------
// Expert Test Suite
// ---------------------------------------------------------------------------

/// Create the expert SWE test suite.
///
/// Complex multi-step tasks requiring deep understanding.
pub fn create_expert_suite() -> SWETestSuite {
    let mut suite = SWETestSuite::new(
        "Expert SWE Suite",
        "Complex multi-step tasks requiring architectural decisions",
    );

    // Test 1: Create a mini project structure
    suite.add_case(
        SWETestCase::new(
            "Create project structure",
            SWETaskType::MultiStep,
            SWETestInput::from_description(
                "Create a complete Rust project structure with Cargo.toml, src/main.rs, and src/lib.rs",
            ),
            SWETestExpectedOutput {
                files_created: vec![
                    "Cargo.toml".to_string(),
                    "src/main.rs".to_string(),
                    "src/lib.rs".to_string(),
                ],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_cleanup("rm -rf src Cargo.toml")
        .with_timeout(60)
        .with_difficulty(SWEDifficulty::Expert)
        .with_tag("project-setup")
        .with_tag("multi-file"),
    );

    // Test 2: Implement a trait
    suite.add_case(
        SWETestCase::new(
            "Implement trait",
            SWETaskType::CodeGeneration,
            SWETestInput::from_description(
                "Implement the Display trait for a custom struct",
            ),
            SWETestExpectedOutput {
                content_patterns: vec![
                    ("impl_display.rs".to_string(), "impl Display".to_string()),
                ],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_setup("echo 'struct Point { x: i32, y: i32 }' > impl_display.rs")
        .with_cleanup("rm -f impl_display.rs")
        .with_difficulty(SWEDifficulty::Expert)
        .with_tag("code-gen")
        .with_tag("traits"),
    );

    // Test 3: Complex bug fix
    suite.add_case(
        SWETestCase::new(
            "Fix lifetime issue",
            SWETaskType::BugFix,
            SWETestInput::from_description(
                "Fix the lifetime annotation issue in the function signature",
            ),
            SWETestExpectedOutput {
                files_modified: vec!["lifetime.rs".to_string()],
                ..SWETestExpectedOutput::default()
            },
        )
        .with_setup("echo 'fn get_ref(data: &String) -> &str { data }' > lifetime.rs")
        .with_cleanup("rm -f lifetime.rs")
        .with_difficulty(SWEDifficulty::Expert)
        .with_tag("bug-fix")
        .with_tag("lifetimes"),
    );

    suite
}

// ---------------------------------------------------------------------------
// Utility Functions
// ---------------------------------------------------------------------------

/// Get a test suite by name.
pub fn get_suite_by_name(name: &str) -> Option<SWETestSuite> {
    match name {
        "basic" => Some(create_basic_suite()),
        "intermediate" => Some(create_intermediate_suite()),
        "advanced" => Some(create_advanced_suite()),
        "expert" => Some(create_expert_suite()),
        _ => None,
    }
}

/// Get all available suite names.
pub fn available_suites() -> &'static [&'static str] {
    &["basic", "intermediate", "advanced", "expert"]
}

/// Create a combined suite with all test cases.
pub fn create_full_suite() -> SWETestSuite {
    let mut suite = SWETestSuite::new(
        "Full SWE Suite",
        "Complete evaluation suite with all difficulty levels",
    );

    for case in create_basic_suite().test_cases {
        suite.test_cases.push(case);
    }
    for case in create_intermediate_suite().test_cases {
        suite.test_cases.push(case);
    }
    for case in create_advanced_suite().test_cases {
        suite.test_cases.push(case);
    }
    for case in create_expert_suite().test_cases {
        suite.test_cases.push(case);
    }

    suite
}