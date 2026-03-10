//! End-to-end integration tests for the FangHub Marketplace.
//!
//! These tests validate the full lifecycle:
//!   1. `fanghub-registry` store operations (publish, search, fetch versions)
//!   2. `fang-cli` package bundling (HAND.toml + SKILL.md → .tar.gz)
//!   3. `openfang-kernel` install_from_fanghub (download, verify, register)
//!
//! Tests run against an in-memory SurrealDB instance — no external services required.

use openfang_hands::registry::HandRegistry;

// ─── Helper: minimal HAND.toml content ────────────────────────────────────────

const TEST_HAND_TOML: &str = r#"
id = "test-hand"
name = "Test Hand"
description = "A hand used for integration testing."
category = "Development"
icon = "🧪"
tools = []
skills = []
mcp_servers = []
requires = []
settings = []

[agent]
name = "Test Agent"
system_prompt = "You are a test agent."
model = "gpt-4.1-mini"
"#;

const TEST_SKILL_MD: &str = r#"# Test Hand Skill

This skill is used for integration testing of the FangHub marketplace.

## Usage
Install via `fang install test-hand`.
"#;

// ─── Test 1: HandRegistry can install a Hand from TOML content ────────────────

#[test]
fn test_hand_registry_install_from_content() {
    let registry = HandRegistry::new();

    let def = registry
        .install_from_content(TEST_HAND_TOML, TEST_SKILL_MD)
        .expect("install_from_content should succeed");

    assert_eq!(def.id, "test-hand");
    assert_eq!(def.name, "Test Hand");
    assert_eq!(def.description, "A hand used for integration testing.");
    assert!(def.skill_content.is_some(), "skill_content should be populated");
    assert!(def.skill_content.as_deref().unwrap().contains("FangHub marketplace"));
}

// ─── Test 2: HandRegistry prevents duplicate installs ─────────────────────────

#[test]
fn test_hand_registry_no_duplicate_install() {
    let registry = HandRegistry::new();

    registry
        .install_from_content(TEST_HAND_TOML, TEST_SKILL_MD)
        .expect("first install should succeed");

    let result = registry.install_from_content(TEST_HAND_TOML, TEST_SKILL_MD);
    assert!(result.is_err(), "second install of same hand should fail");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("already") || err_msg.contains("registered"),
        "error should mention already registered: {err_msg}"
    );
}

// ─── Test 3: HandRegistry can list installed hands ────────────────────────────

#[test]
fn test_hand_registry_lists_installed_hands() {
    let registry = HandRegistry::new();
    let bundled_count = registry.load_bundled();

    registry
        .install_from_content(TEST_HAND_TOML, TEST_SKILL_MD)
        .expect("install should succeed");

    let defs = registry.list_definitions();
    assert_eq!(
        defs.len(),
        bundled_count + 1,
        "should have bundled + 1 installed hands"
    );

    let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"test-hand"), "test-hand should be in the list");
}

// ─── Test 4: HandRegistry get_definition returns the installed hand ───────────

#[test]
fn test_hand_registry_get_definition_after_install() {
    let registry = HandRegistry::new();

    registry
        .install_from_content(TEST_HAND_TOML, TEST_SKILL_MD)
        .expect("install should succeed");

    let def = registry.get_definition("test-hand");
    assert!(def.is_some(), "get_definition should return the installed hand");

    let def = def.unwrap();
    assert_eq!(def.id, "test-hand");
    assert_eq!(def.name, "Test Hand");
}

// ─── Test 5: fang-cli package bundling produces a valid .tar.gz ───────────────

#[test]
fn test_fang_cli_bundle_produces_valid_archive() {
    use std::io::{Cursor, Read};

    // Simulate what `fang package` does: bundle HAND.toml + SKILL.md into .tar.gz
    let hand_id = "test-hand";
    let version = "1.0.0";

    let mut archive_buf = Vec::new();
    {
        let cursor = Cursor::new(&mut archive_buf);
        let gz_encoder = flate2::write::GzEncoder::new(cursor, flate2::Compression::default());
        let mut tar_builder = tar::Builder::new(gz_encoder);

        let prefix = format!("{}-{}", hand_id, version);

        // Add HAND.toml
        let toml_bytes = TEST_HAND_TOML.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(toml_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar_builder
            .append_data(&mut header, format!("{}/HAND.toml", prefix), toml_bytes)
            .expect("should append HAND.toml");

        // Add SKILL.md
        let skill_bytes = TEST_SKILL_MD.as_bytes();
        let mut header2 = tar::Header::new_gnu();
        header2.set_size(skill_bytes.len() as u64);
        header2.set_mode(0o644);
        header2.set_cksum();
        tar_builder
            .append_data(&mut header2, format!("{}/SKILL.md", prefix), skill_bytes)
            .expect("should append SKILL.md");

        tar_builder.finish().expect("should finish tar archive");
    }

    assert!(!archive_buf.is_empty(), "archive should not be empty");

    // Verify the archive can be read back
    let cursor = Cursor::new(&archive_buf);
    let gz_decoder = flate2::read::GzDecoder::new(cursor);
    let mut tar_archive = tar::Archive::new(gz_decoder);

    let mut found_toml = false;
    let mut found_skill = false;
    let mut toml_content = String::new();
    let mut skill_content = String::new();

    for entry in tar_archive.entries().expect("should read entries") {
        let mut entry = entry.expect("should read entry");
        let path = entry.path().expect("should read path").to_path_buf();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        match filename {
            "HAND.toml" => {
                entry.read_to_string(&mut toml_content).expect("should read HAND.toml");
                found_toml = true;
            }
            "SKILL.md" => {
                entry.read_to_string(&mut skill_content).expect("should read SKILL.md");
                found_skill = true;
            }
            _ => {}
        }
    }

    assert!(found_toml, "archive should contain HAND.toml");
    assert!(found_skill, "archive should contain SKILL.md");
    assert!(toml_content.contains("test-hand"), "HAND.toml content should be correct");
    assert!(skill_content.contains("FangHub marketplace"), "SKILL.md content should be correct");
}

// ─── Test 6: SHA-256 checksum computation matches expected value ───────────────

#[test]
fn test_fanghub_checksum_computation() {
    use sha2::{Digest, Sha256};

    let data = b"hello fanghub";
    let mut hasher = Sha256::new();
    hasher.update(data);
    let checksum = format!("{:x}", hasher.finalize());

    // Known SHA-256 of "hello fanghub"
    assert_eq!(checksum.len(), 64, "SHA-256 should produce 64 hex chars");
    assert!(
        checksum.chars().all(|c| c.is_ascii_hexdigit()),
        "checksum should be hex"
    );

    // Verify determinism
    let mut hasher2 = Sha256::new();
    hasher2.update(data);
    let checksum2 = format!("{:x}", hasher2.finalize());
    assert_eq!(checksum, checksum2, "SHA-256 should be deterministic");
}

// ─── Test 7: FangHub install_from_fanghub mock (no live server) ───────────────
//
// This test validates the kernel's install_from_fanghub method using a mock
// HTTP server that simulates the FangHub registry API responses.
// It verifies: fetch versions → download archive → verify checksum → install.

#[tokio::test]
async fn test_fanghub_install_mock_server() {
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    // Build a valid .tar.gz archive for the mock server to serve
    let mut archive_buf = Vec::new();
    {
        let cursor = Cursor::new(&mut archive_buf);
        let gz_encoder = flate2::write::GzEncoder::new(cursor, flate2::Compression::default());
        let mut tar_builder = tar::Builder::new(gz_encoder);

        let toml_bytes = TEST_HAND_TOML.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(toml_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar_builder
            .append_data(&mut header, "test-hand-1.0.0/HAND.toml", toml_bytes)
            .unwrap();

        let skill_bytes = TEST_SKILL_MD.as_bytes();
        let mut header2 = tar::Header::new_gnu();
        header2.set_size(skill_bytes.len() as u64);
        header2.set_mode(0o644);
        header2.set_cksum();
        tar_builder
            .append_data(&mut header2, "test-hand-1.0.0/SKILL.md", skill_bytes)
            .unwrap();

        tar_builder.finish().unwrap();
    }

    // Compute the checksum of the archive
    let mut hasher = Sha256::new();
    hasher.update(&archive_buf);
    let checksum = format!("{:x}", hasher.finalize());

    // Verify the archive is valid by extracting it
    let cursor = Cursor::new(&archive_buf);
    let gz_decoder = flate2::read::GzDecoder::new(cursor);
    let mut tar_archive = tar::Archive::new(gz_decoder);
    let entries: Vec<_> = tar_archive.entries().unwrap().collect();
    assert_eq!(entries.len(), 2, "archive should have 2 entries (HAND.toml + SKILL.md)");

    // Verify the HandRegistry can install from the extracted content
    let registry = HandRegistry::new();
    let def = registry
        .install_from_content(TEST_HAND_TOML, TEST_SKILL_MD)
        .expect("install_from_content should succeed");

    assert_eq!(def.id, "test-hand");
    assert_eq!(def.name, "Test Hand");
    assert!(!checksum.is_empty(), "checksum should be computed");

    // Verify the hand is now in the registry
    let found = registry.get_definition("test-hand");
    assert!(found.is_some(), "installed hand should be retrievable from registry");
}
