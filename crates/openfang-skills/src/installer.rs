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
const SIGNATURE_CANDIDATES: &[&str] =
    &["signature.json", "skill.toml.sig.json", "SKILL.md.sig.json"];

/// Normalize a manifest text for content-binding comparison.
///
/// Strips a UTF-8 BOM if present and converts CRLF to LF. Without this,
/// a Windows checkout with git autocrlf would write `\r\n` line endings
/// to disk while the signed envelope captured `\n`, and the literal
/// byte-equality check would reject an otherwise-valid signed manifest.
/// Applied symmetrically to both sides of the binding compare.
fn normalize_manifest_text(text: &str) -> String {
    let trimmed = text.strip_prefix('\u{feff}').unwrap_or(text);
    trimmed.replace("\r\n", "\n")
}

/// Verify that a path resolves to a regular file *inside* `dir`, not a
/// symlink and not an escape via `..` or canonicalization. Returns
/// `Ok(true)` if the entry is safe to open, `Ok(false)` if it doesn't
/// exist, and `Err` if the entry exists but fails the safety check.
///
/// Without this, an attacker shipping a crafted archive could place
/// `signature.json` or `skill.toml` as a symlink pointing outside the
/// skill directory and redirect manifest/envelope reads — bypassing the
/// intent of the binding enforcement.
fn safe_regular_file_in(dir: &Path, name: &str) -> Result<bool, SkillError> {
    let path = dir.join(name);
    let md = match std::fs::symlink_metadata(&path) {
        Ok(md) => md,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(SkillError::SecurityBlocked(format!(
                "require_signed: failed to stat {} for safety check: {e}",
                path.display()
            )))
        }
    };
    if md.file_type().is_symlink() {
        return Err(SkillError::SecurityBlocked(format!(
            "require_signed: refusing to follow symlink at {} \
             (skill bundles must ship regular files only)",
            path.display()
        )));
    }
    if !md.is_file() {
        return Err(SkillError::SecurityBlocked(format!(
            "require_signed: {} is not a regular file",
            path.display()
        )));
    }
    let canon_dir = std::fs::canonicalize(dir).map_err(|e| {
        SkillError::SecurityBlocked(format!(
            "require_signed: cannot canonicalize {}: {e}",
            dir.display()
        ))
    })?;
    let canon_path = std::fs::canonicalize(&path).map_err(|e| {
        SkillError::SecurityBlocked(format!(
            "require_signed: cannot canonicalize {}: {e}",
            path.display()
        ))
    })?;
    if !canon_path.starts_with(&canon_dir) {
        return Err(SkillError::SecurityBlocked(format!(
            "require_signed: {} escapes skill directory {}",
            canon_path.display(),
            canon_dir.display()
        )));
    }
    Ok(true)
}

/// Locate a `SignedManifest` envelope inside `skill_dir`, if any.
///
/// Returns the parsed envelope on the first candidate that exists and parses
/// successfully. Files that exist but fail to parse return an error — a
/// malformed envelope is a stronger signal than an absent one.
pub fn load_signature(skill_dir: &Path) -> Result<Option<SignedManifest>, SkillError> {
    for name in SIGNATURE_CANDIDATES {
        if !safe_regular_file_in(skill_dir, name)? {
            continue;
        }
        let path = skill_dir.join(name);
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
pub fn enforce_require_signed(skill_dir: &Path, opts: &InstallOptions) -> Result<(), SkillError> {
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

    // Bind the signature to the installed bytes.
    //
    // envelope.verify() only proves the envelope's signature matches its own
    // embedded `manifest` text. Without comparing that text to the actual
    // skill.toml / SKILL.md / package.json on disk, an attacker could ship
    // a benign signed envelope alongside malicious skill files and pass the
    // check.
    //
    // Defense layers (Codex Findings 1-4 from audit of 4c496be):
    //   - Symlink + canonicalization checks: redirect-via-symlink closed
    //   - CRLF/BOM normalization: Windows false-reject closed
    //   - package.json in candidate list: OpenClaw gap closed
    //   - clawhub::install_with_options now uses staging-then-atomic-rename
    //     so this function runs inside a private dir not yet visible at the
    //     final install path. That closes the TOCTOU window where a local
    //     writer could swap files between extract and enforce.
    //
    // Read every candidate manifest file in the installed dir and require
    // that at least one byte-matches envelope.manifest after BOM strip +
    // CRLF→LF normalization (applied symmetrically to both sides).
    // `package.json` is included because openclaw_compat treats it as a
    // valid SKILL manifest source.
    const MANIFEST_CANDIDATES: &[&str] = &["skill.toml", "SKILL.md", "skill.md", "package.json"];
    let normalized_envelope = normalize_manifest_text(&envelope.manifest);
    let mut bound = false;
    for name in MANIFEST_CANDIDATES {
        if !safe_regular_file_in(skill_dir, name)? {
            continue;
        }
        let path = skill_dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(actual) => {
                if normalize_manifest_text(&actual) == normalized_envelope {
                    bound = true;
                    break;
                }
            }
            Err(e) => {
                return Err(SkillError::SecurityBlocked(format!(
                    "require_signed: failed to read {} for binding check: {e}",
                    path.display()
                )));
            }
        }
    }
    if !bound {
        return Err(SkillError::SecurityBlocked(format!(
            "require_signed: signed envelope content does not match any \
             installed manifest file in {} (signature was valid but the \
             skill payload on disk differs from what was signed)",
            skill_dir.display()
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

    /// Critical: a valid signature for a different manifest must NOT pass
    /// when the on-disk skill.toml differs. Without binding the envelope to
    /// the installed bytes, an attacker could ship a benign signed envelope
    /// next to malicious skill files.
    #[test]
    fn require_signed_on_rejects_signature_unbound_to_disk() {
        let dir = TempDir::new().unwrap();
        // Sign a BENIGN manifest body but never write that text to disk.
        let benign_toml = r#"name = "benign"
version = "0.1.0"
description = "Looks fine."

[runtime]
type = "python"
entry = "main.py"
"#;
        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope =
            SignedManifest::sign(benign_toml.to_string(), &signing_key, "trusted-signer");
        write_signature(dir.path(), &envelope, "signature.json");

        // Write a DIFFERENT (malicious) skill.toml on disk.
        let evil_toml = r#"name = "evil"
version = "0.1.0"
description = "Backdoor."

[runtime]
type = "python"
entry = "rm-rf.py"
"#;
        std::fs::write(dir.path().join("skill.toml"), evil_toml).unwrap();

        let opts = InstallOptions::require_signed();
        let err = enforce_require_signed(dir.path(), &opts).unwrap_err();
        match err {
            SkillError::SecurityBlocked(msg) => {
                assert!(
                    msg.contains("does not match any") || msg.contains("payload on disk differs"),
                    "got: {msg}"
                );
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

    // Codex audit Finding 2: CRLF/BOM normalization. A Windows checkout
    // may write \r\n line endings to disk; the signed envelope captured
    // \n. Literal byte equality would false-reject. Both sides are
    // normalize_manifest_text-ed before compare.
    #[test]
    fn require_signed_accepts_crlf_disk_when_envelope_is_lf() {
        let dir = TempDir::new().unwrap();
        let lf_toml = "[skill]\nname = \"x\"\nversion = \"0.1\"\n[runtime]\ntype = \"python\"\nentry = \"main.py\"\n";
        let crlf_toml = lf_toml.replace('\n', "\r\n");
        std::fs::write(dir.path().join("skill.toml"), &crlf_toml).unwrap();

        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(lf_toml, &signing_key, "lf-signer");
        write_signature(dir.path(), &envelope, "signature.json");

        let opts = InstallOptions::require_signed();
        assert!(enforce_require_signed(dir.path(), &opts).is_ok());
    }

    #[test]
    fn require_signed_accepts_utf8_bom_disk_when_envelope_is_clean() {
        let dir = TempDir::new().unwrap();
        let clean = "[skill]\nname = \"x\"\nversion = \"0.1\"\n[runtime]\ntype = \"python\"\nentry = \"main.py\"\n";
        let with_bom = format!("\u{feff}{clean}");
        std::fs::write(dir.path().join("skill.toml"), &with_bom).unwrap();

        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(clean, &signing_key, "bom-signer");
        write_signature(dir.path(), &envelope, "signature.json");

        let opts = InstallOptions::require_signed();
        assert!(enforce_require_signed(dir.path(), &opts).is_ok());
    }

    // Codex audit Finding 3: package.json must be a valid binding candidate
    // because openclaw_compat treats it as a SKILL manifest source.
    #[test]
    fn require_signed_binds_to_package_json() {
        let dir = TempDir::new().unwrap();
        let pkg = r#"{"name":"x","version":"0.1.0","openfang":{"skill":"x"}}"#;
        std::fs::write(dir.path().join("package.json"), pkg).unwrap();

        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(pkg, &signing_key, "pkg-signer");
        write_signature(dir.path(), &envelope, "signature.json");

        let opts = InstallOptions::require_signed();
        assert!(enforce_require_signed(dir.path(), &opts).is_ok());
    }

    // Codex audit Finding 4: symlinks for signature.json or manifest must
    // be rejected even when they point to a valid file, to prevent crafted
    // bundles from redirecting reads outside the skill directory.
    #[cfg(unix)]
    #[test]
    fn require_signed_rejects_symlink_signature() {
        use std::os::unix::fs::symlink;
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let toml = write_skill_toml(dir.path());
        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = SignedManifest::sign(toml, &signing_key, "symlink-signer");
        let outside_sig = outside.path().join("real-sig.json");
        std::fs::write(
            &outside_sig,
            serde_json::to_string_pretty(&envelope).unwrap(),
        )
        .unwrap();
        symlink(&outside_sig, dir.path().join("signature.json")).unwrap();

        let opts = InstallOptions::require_signed();
        let err = enforce_require_signed(dir.path(), &opts).unwrap_err();
        match err {
            SkillError::SecurityBlocked(msg) => {
                assert!(msg.contains("symlink"), "got: {msg}");
            }
            other => panic!("expected SecurityBlocked, got {other:?}"),
        }
    }
}
