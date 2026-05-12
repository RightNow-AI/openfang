//! Skill install enforcement options.
//!
//! Wraps the per-source install clients (FangHub `marketplace`, ClawHub) with
//! optional supply-chain gates. The flagship gate is `require_signed`: when
//! true, an Ed25519 `SignedManifest` envelope must sit alongside the skill
//! payload and verify cleanly before the install is considered complete.
//!
//! The signature envelope is a JSON serialisation of
//! [`openfang_types::manifest_signing::SignedManifest`]. The installer looks
//! for it at one of these well-known names inside the freshly written skill
//! directory:
//!
//! - `signature.json`
//! - `skill.toml.sig.json`
//! - `SKILL.md.sig.json`
//!
//! On a `require_signed` failure the skill directory is removed and a
//! `SkillError::SecurityBlocked` is returned, matching the existing
//! prompt-injection-blocked path in `clawhub.rs`.

use crate::SkillError;
use openfang_types::manifest_signing::SignedManifest;
use std::path::Path;

/// Options controlling enforcement during skill install.
///
/// Defaults are permissive — `require_signed` is `false` so existing
/// callers (`Installer::install`, `Installer::install` on the marketplace)
/// behave exactly as before.
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// When true, reject any skill that does not ship with a valid Ed25519
    /// `SignedManifest` envelope. The `--require-signed` CLI flag maps here.
    pub require_signed: bool,
    /// Optional allow-list of acceptable signer public keys (hex-encoded,
    /// 32 bytes / 64 hex chars). When non-empty, the envelope's
    /// `signer_public_key` must match one of these entries in addition to
    /// passing cryptographic verification. Empty = any valid signature
    /// accepted (TOFU mode).
    pub allowed_signer_keys: Vec<String>,
}

impl InstallOptions {
    /// Convenience: `require_signed = true`, no key pinning.
    pub fn require_signed() -> Self {
        Self {
            require_signed: true,
            allowed_signer_keys: Vec::new(),
        }
    }

    /// Convenience: `require_signed = true` with a pinned signer key.
    pub fn require_signed_by(pubkey_hex: impl Into<String>) -> Self {
        Self {
            require_signed: true,
            allowed_signer_keys: vec![pubkey_hex.into()],
        }
    }
}

/// Well-known filenames the installer searches for a detached signature
/// envelope, in priority order.
const SIGNATURE_CANDIDATES: &[&str] = &[
    "signature.json",
    "skill.toml.sig.json",
    "SKILL.md.sig.json",
];

/// Locate a `SignedManifest` envelope inside `skill_dir`, if any.
///
/// Returns the parsed envelope on the first candidate that exists and parses
/// successfully. Files that exist but fail to parse return an error — a
/// malformed envelope is a stronger signal than an absent one.
pub fn load_signature(skill_dir: &Path) -> Result<Option<SignedManifest>, SkillError> {
    for name in SIGNATURE_CANDIDATES {
        let path = skill_dir.join(name);
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&path)?;
        let envelope: SignedManifest = serde_json::from_str(&raw).map_err(|e| {
            SkillError::InvalidManifest(format!(
                "Signature envelope at {} is not valid JSON: {e}",
                path.display()
            ))
        })?;
        return Ok(Some(envelope));
    }
    Ok(None)
}

/// Enforce `require_signed` against a freshly installed skill directory.
///
/// Returns `Ok(())` when:
/// - `opts.require_signed` is false (no enforcement); or
/// - a `SignedManifest` envelope is found, `verify()` passes, and (when
///   `allowed_signer_keys` is non-empty) the signer key is allow-listed.
///
/// Returns `SkillError::SecurityBlocked` when enforcement is on and the
/// skill fails any of those checks. On failure the caller is expected to
/// remove `skill_dir` to keep the skills directory clean.
pub fn enforce_require_signed(
    skill_dir: &Path,
    opts: &InstallOptions,
) -> Result<(), SkillError> {
    if !opts.require_signed {
        return Ok(());
    }

    let envelope = match load_signature(skill_dir)? {
        Some(e) => e,
        None => {
            return Err(SkillError::SecurityBlocked(format!(
                "require_signed: no signature envelope found in {} \
                 (looked for signature.json / skill.toml.sig.json / SKILL.md.sig.json)",
                skill_dir.display()
            )))
        }
    };

    if let Err(e) = envelope.verify() {
        return Err(SkillError::SecurityBlocked(format!(
            "require_signed: signature verification failed: {e}"
        )));
    }

    if !opts.allowed_signer_keys.is_empty() {
        let actual = hex::encode(&envelope.signer_public_key);
        let actual_lower = actual.to_lowercase();
        let matched = opts
            .allowed_signer_keys
            .iter()
            .any(|k| k.trim().to_lowercase() == actual_lower);
        if !matched {
            return Err(SkillError::SecurityBlocked(format!(
                "require_signed: signer key {actual} not in allow-list \
                 (signer_id = {:?})",
                envelope.signer_id
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use tempfile::TempDir;

    fn write_skill_toml(dir: &Path) -> String {
        let toml = r#"
[skill]
name = "signed-skill"
version = "0.1.0"
description = "A signed skill"

[runtime]
type = "python"
entry = "main.py"
"#;
        std::fs::write(dir.join("skill.toml"), toml).unwrap();
        toml.to_string()
    }

    fn write_signature(dir: &Path, envelope: &SignedManifest, name: &str) {
        let json = serde_json::to_string_pretty(envelope).unwrap();
        std::fs::write(dir.join(name), json).unwrap();
    }

    #[test]
    fn require_signed_off_passes_unsigned() {
        let dir = TempDir::new().unwrap();
        write_skill_toml(dir.path());
        let opts = InstallOptions::default();
        assert!(enforce_require_signed(dir.path(), &opts).is_ok());
    }

    #[test]
    fn require_signed_on_rejects_missing_signature() {
        let dir = TempDir::new().unwrap();
        write_skill_toml(dir.path());
        let opts = InstallOptions::require_signed();
        let err = enforce_require_signed(dir.path(), &opts).unwrap_err();
        match err {
            SkillError::SecurityBlocked(msg) => {
                assert!(msg.contains("no signature envelope"), "got: {msg}");
            }
            other => panic!("expected SecurityBlocked, got {other:?}"),
        }
    }

    #[test]
    fn require_signed_on_accepts_valid_signature() {
        let dir = TempDir::new().unwrap();
        let toml = write_skill_toml(dir.path());
        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(toml, &signing_key, "test-signer");
        write_signature(dir.path(), &envelope, "signature.json");

        let opts = InstallOptions::require_signed();
        assert!(enforce_require_signed(dir.path(), &opts).is_ok());
    }

    #[test]
    fn require_signed_on_rejects_tampered_envelope() {
        let dir = TempDir::new().unwrap();
        let toml = write_skill_toml(dir.path());
        let signing_key = SigningKey::generate(&mut OsRng);
        let mut envelope = SignedManifest::sign(toml, &signing_key, "test-signer");
        // Tamper with the manifest body — content_hash will no longer match.
        envelope.manifest.push_str("\n# evil append\n");
        write_signature(dir.path(), &envelope, "signature.json");

        let opts = InstallOptions::require_signed();
        let err = enforce_require_signed(dir.path(), &opts).unwrap_err();
        match err {
            SkillError::SecurityBlocked(msg) => {
                assert!(
                    msg.contains("signature verification failed")
                        || msg.contains("content hash mismatch"),
                    "got: {msg}"
                );
            }
            other => panic!("expected SecurityBlocked, got {other:?}"),
        }
    }

    #[test]
    fn require_signed_rejects_malformed_envelope() {
        let dir = TempDir::new().unwrap();
        write_skill_toml(dir.path());
        std::fs::write(dir.path().join("signature.json"), "{not valid json").unwrap();

        let opts = InstallOptions::require_signed();
        let err = enforce_require_signed(dir.path(), &opts).unwrap_err();
        match err {
            SkillError::InvalidManifest(msg) => {
                assert!(msg.contains("Signature envelope"), "got: {msg}");
            }
            other => panic!("expected InvalidManifest, got {other:?}"),
        }
    }

    #[test]
    fn require_signed_with_allowed_keys_accepts_listed_key() {
        let dir = TempDir::new().unwrap();
        let toml = write_skill_toml(dir.path());
        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(toml, &signing_key, "test-signer");
        let pk_hex = hex::encode(&envelope.signer_public_key);
        write_signature(dir.path(), &envelope, "signature.json");

        let opts = InstallOptions::require_signed_by(pk_hex);
        assert!(enforce_require_signed(dir.path(), &opts).is_ok());
    }

    #[test]
    fn require_signed_with_allowed_keys_rejects_unlisted_key() {
        let dir = TempDir::new().unwrap();
        let toml = write_skill_toml(dir.path());
        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(toml, &signing_key, "evil-signer");
        write_signature(dir.path(), &envelope, "signature.json");

        // Allow only a different key.
        let other_key = SigningKey::generate(&mut OsRng);
        let other_hex = hex::encode(other_key.verifying_key().to_bytes());
        let opts = InstallOptions::require_signed_by(other_hex);
        let err = enforce_require_signed(dir.path(), &opts).unwrap_err();
        match err {
            SkillError::SecurityBlocked(msg) => {
                assert!(msg.contains("not in allow-list"), "got: {msg}");
            }
            other => panic!("expected SecurityBlocked, got {other:?}"),
        }
    }

    #[test]
    fn load_signature_returns_none_when_absent() {
        let dir = TempDir::new().unwrap();
        write_skill_toml(dir.path());
        assert!(load_signature(dir.path()).unwrap().is_none());
    }

    #[test]
    fn load_signature_finds_alternate_filename() {
        let dir = TempDir::new().unwrap();
        let toml = write_skill_toml(dir.path());
        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(toml, &signing_key, "alt-name-signer");
        write_signature(dir.path(), &envelope, "skill.toml.sig.json");

        let loaded = load_signature(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.signer_id, "alt-name-signer");
    }
}
