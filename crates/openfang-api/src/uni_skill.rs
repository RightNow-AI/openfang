/*
* @Author             : Felix
* @Email              : 307253927@qq.com
* @Date               : 2026-03-19 17:02:07
 * @LastEditors        : Felix
 * @LastEditTime       : 2026-03-19 17:30:33
*/
use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

use openfang_skills::openclaw_compat;
use openfang_skills::verify::SkillVerifier;
use reqwest::StatusCode;
use std::sync::Arc;
use tracing::{info, warn};

use crate::routes::AppState;

/// Reload skills 会重新加载 skills 目录下的所有技能。
/// 会重新解析所有技能的 Skill.md 文件。
pub async fn reload_skills(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let skill_registry = match state.kernel.skill_registry.write() {
        Ok(registry) => registry,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Failed to reload skills"})),
            )
        }
    };

    if !skill_registry.skills_dir.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No skills directory found"})),
        );
    }

    let mut count = 0;
    let entries = match std::fs::read_dir(&skill_registry.skills_dir) {
        Ok(entries) => entries,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Failed to reload skills: {:?}", e)})),
            );
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Auto-detect SKILL.md and convert to skill.toml + prompt_context.md
        if openclaw_compat::detect_skillmd(&path) {
            match openclaw_compat::convert_skillmd(&path) {
                Ok(converted) => {
                    // SECURITY: Scan prompt content for injection attacks
                    // before accepting the skill. 341 malicious skills were
                    // found on ClawHub — block critical threats at load time.
                    let warnings = SkillVerifier::scan_prompt_content(&converted.prompt_context);
                    let has_critical = warnings.iter().any(|w| {
                        matches!(
                            w.severity,
                            openfang_skills::verify::WarningSeverity::Critical
                        )
                    });
                    if has_critical {
                        warn!(
                            skill = %converted.manifest.skill.name,
                            "BLOCKED: SKILL.md contains critical prompt injection patterns"
                        );
                        for w in &warnings {
                            warn!("  [{:?}] {}", w.severity, w.message);
                        }
                        continue;
                    }
                    if !warnings.is_empty() {
                        for w in &warnings {
                            warn!(
                                skill = %converted.manifest.skill.name,
                                "[{:?}] {}",
                                w.severity,
                                w.message
                            );
                        }
                    }

                    info!(
                        skill = %converted.manifest.skill.name,
                        "Auto-converting SKILL.md to OpenFang format"
                    );
                    if let Err(e) =
                        openclaw_compat::write_openfang_manifest(&path, &converted.manifest)
                    {
                        warn!("Failed to write skill.toml for {}: {e}", path.display());
                        continue;
                    }
                    if let Err(e) =
                        openclaw_compat::write_prompt_context(&path, &converted.prompt_context)
                    {
                        warn!(
                            "Failed to write prompt_context.md for {}: {e}",
                            path.display()
                        );
                    }
                    // Fall through to load the newly written skill.toml
                }
                Err(e) => {
                    warn!("Failed to convert SKILL.md at {}: {e}", path.display());
                    continue;
                }
            }
        } else {
            continue;
        }
        count += 1;
    }
    // Drop the write lock to allow other threads to access the registry
    drop(skill_registry);
    // Reload skills from the kernel
    state.kernel.reload_skills();
    (StatusCode::OK, Json(serde_json::json!({"count": count})))
}

/// GET /api/skills — List installed skills (bundled + user-installed).
pub async fn list_skills(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let registry = state
        .kernel
        .skill_registry
        .read()
        .unwrap_or_else(|e| e.into_inner());

    let skills: Vec<serde_json::Value> = registry
        .list()
        .iter()
        .map(|s| {
            let source = match &s.manifest.source {
                Some(openfang_skills::SkillSource::ClawHub { slug, version }) => {
                    serde_json::json!({"type": "clawhub", "slug": slug, "version": version})
                }
                Some(openfang_skills::SkillSource::OpenClaw) => {
                    serde_json::json!({"type": "openclaw"})
                }
                Some(openfang_skills::SkillSource::Bundled) => {
                    serde_json::json!({"type": "bundled"})
                }
                Some(openfang_skills::SkillSource::Native) | None => {
                    serde_json::json!({"type": "local"})
                }
            };
            serde_json::json!({
                "name": s.manifest.skill.name,
                "description": s.manifest.skill.description,
                "version": s.manifest.skill.version,
                "author": s.manifest.skill.author,
                "runtime": format!("{:?}", s.manifest.runtime.runtime_type),
                "tools_count": s.manifest.tools.provided.len(),
                "tags": s.manifest.skill.tags,
                "enabled": s.enabled,
                "source": source,
                "has_prompt_context": s.manifest.prompt_context.is_some(),
                "path":s.path.display().to_string()
            })
        })
        .collect();

    Json(serde_json::json!({ "skills": skills, "total": skills.len() }))
}

/// POST /api/skills/install_local — Install a skill from a local zip upload.
///
/// Accepts raw zip bytes via the request body. The client must set:
/// - `Content-Type: application/zip` or `application/octet-stream`
/// - `X-Skill-Name` header (optional, used as fallback skill name)
///
/// The zip is extracted into `~/.openfang/skills/{name}/`, then the same
/// format detection + security pipeline as ClawHub install runs.
pub async fn install_local_skill(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    use openfang_skills::openclaw_compat;
    use openfang_skills::verify::{SkillVerifier, WarningSeverity};
    use sha2::{Digest, Sha256};

    const MAX_SKILL_ZIP_SIZE: usize = 50 * 1024 * 1024; // 50 MB

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Empty request body"})),
        );
    }

    if body.len() > MAX_SKILL_ZIP_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "Zip file too large (max 50 MB)"})),
        );
    }

    // SHA256 of uploaded content
    let sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(&body);
        hex::encode(hasher.finalize())
    };

    let skill_name_hint = headers
        .get("X-Skill-Name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("local-skill")
        .to_string();

    let skills_dir = state.kernel.config.home_dir.join("skills");

    // Detect content type: zip (PK magic) or SKILL.md (starts with ---)
    let content_str = String::from_utf8_lossy(&body);
    let is_skillmd = content_str.trim_start().starts_with("---");

    // Determine skill name from zip contents or header hint
    let slug = if !is_skillmd && body.len() >= 4 && body[0] == 0x50 && body[1] == 0x4b {
        // Peek into zip to find skill name from skill.toml or SKILL.md frontmatter
        extract_skill_name_from_zip(&body).unwrap_or_else(|| sanitize_name(&skill_name_hint))
    } else {
        sanitize_name(&skill_name_hint)
    };

    // For zip files, extract directly into skills_dir (zip already contains the root folder).
    // For non-zip, create skill_dir as before.
    let skill_dir = skills_dir.join(&slug);

    // Extract content
    if is_skillmd {
        // SKILL.md — create skill_dir and write into it
        if skill_dir.join("skill.toml").exists() {
            if let Err(e) = std::fs::remove_dir_all(&skill_dir) {
                tracing::warn!("Failed to remove old skill dir: {e}");
            }
        }
        if let Err(e) = std::fs::create_dir_all(&skill_dir) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"error": format!("Failed to create skill directory: {e}")}),
                ),
            );
        }
        if let Err(e) = std::fs::write(skill_dir.join("SKILL.md"), &*body) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to write SKILL.md: {e}")})),
            );
        }
    } else if body.len() >= 4 && body[0] == 0x50 && body[1] == 0x4b {
        // Zip archive — extract directly into skills_dir root so the zip's
        // internal folder becomes the skill directory (avoids double nesting).
        let zip_root = detect_zip_root_folder(&body);

        // If the zip contains a root folder, clean up the old install
        if let Some(ref root_name) = zip_root {
            let existing = skills_dir.join(root_name);
            if existing.exists() {
                if let Err(e) = std::fs::remove_dir_all(&existing) {
                    tracing::warn!("Failed to remove old skill dir: {e}");
                }
            }
        }

        if let Err(e) = std::fs::create_dir_all(&skills_dir) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"error": format!("Failed to create skills directory: {e}")}),
                ),
            );
        }

        let cursor = std::io::Cursor::new(&*body);
        match zip::ZipArchive::new(cursor) {
            Ok(mut archive) => {
                for i in 0..archive.len() {
                    let mut file = match archive.by_index(i) {
                        Ok(f) => f,
                        Err(e) => {
                            tracing::warn!(index = i, error = %e, "Skipping zip entry");
                            continue;
                        }
                    };
                    let Some(enclosed_name) = file.enclosed_name() else {
                        tracing::warn!("Skipping zip entry with unsafe path");
                        continue;
                    };
                    // Skip macOS resource fork metadata (__MACOSX/ and ._xxx files)
                    let path_str = enclosed_name.to_string_lossy();
                    if path_str.starts_with("__MACOSX") || path_str.contains("/__MACOSX") {
                        continue;
                    }
                    if enclosed_name
                        .file_name()
                        .map_or(false, |n| n.to_string_lossy().starts_with("._"))
                    {
                        continue;
                    }
                    // Extract into skills_dir directly (not skill_dir)
                    let out_path = skills_dir.join(enclosed_name);
                    if file.is_dir() {
                        if let Err(e) = std::fs::create_dir_all(&out_path) {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(
                                    serde_json::json!({"error": format!("Failed to create dir: {e}")}),
                                ),
                            );
                        }
                    } else {
                        if let Some(parent) = out_path.parent() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        serde_json::json!({"error": format!("Failed to create parent dir: {e}")}),
                                    ),
                                );
                            }
                        }
                        let mut out_file = match std::fs::File::create(&out_path) {
                            Ok(f) => f,
                            Err(e) => {
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        serde_json::json!({"error": format!("Failed to create file {}: {e}", out_path.display())}),
                                    ),
                                );
                            }
                        };
                        if let Err(e) = std::io::copy(&mut file, &mut out_file) {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(
                                    serde_json::json!({"error": format!("Failed to write file {}: {e}", out_path.display())}),
                                ),
                            );
                        }
                    }
                }
                tracing::info!(slug = %slug, entries = archive.len(), "Extracted local skill zip into skills root");

                // Zip extracted successfully — reload and return immediately
                state.kernel.reload_skills();
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "status": "installed",
                        "slug": slug,
                        "sha256": sha256,
                    })),
                );
            }
            Err(e) => {
                // Fallback: save raw zip into skill_dir
                tracing::warn!(slug = %slug, error = %e, "Failed to read zip, saving raw");
                if let Err(e2) = std::fs::create_dir_all(&skill_dir) {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            serde_json::json!({"error": format!("Failed to create skill dir: {e2}")}),
                        ),
                    );
                }
                if let Err(e2) = std::fs::write(skill_dir.join("skill.zip"), &*body) {
                    let _ = std::fs::remove_dir_all(&skill_dir);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": format!("Failed to save raw zip: {e2}")})),
                    );
                }
            }
        }
    } else {
        // Fallback: treat as package.json
        if let Err(e) = std::fs::create_dir_all(&skill_dir) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"error": format!("Failed to create skill directory: {e}")}),
                ),
            );
        }
        if let Err(e) = std::fs::write(skill_dir.join("package.json"), &*body) {
            let _ = std::fs::remove_dir_all(&skill_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to write package.json: {e}")})),
            );
        }
    }

    // Format detection + conversion + security scan (same pipeline as clawhub.rs install())
    let mut all_warnings = Vec::new();
    let mut tool_translations = Vec::new();
    let mut is_prompt_only = false;

    let manifest = if is_skillmd || openclaw_compat::detect_skillmd(&skill_dir) {
        match openclaw_compat::convert_skillmd(&skill_dir) {
            Ok(converted) => {
                tool_translations = converted.tool_translations;
                is_prompt_only = converted.manifest.runtime.runtime_type
                    == openfang_skills::SkillRuntime::PromptOnly;

                // Prompt injection scan
                let prompt_warnings = SkillVerifier::scan_prompt_content(&converted.prompt_context);
                if prompt_warnings
                    .iter()
                    .any(|w| w.severity == WarningSeverity::Critical)
                {
                    let critical_msgs: Vec<_> = prompt_warnings
                        .iter()
                        .filter(|w| w.severity == WarningSeverity::Critical)
                        .map(|w| w.message.clone())
                        .collect();
                    let _ = std::fs::remove_dir_all(&skill_dir);
                    return (
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": format!("Skill blocked due to prompt injection: {}", critical_msgs.join("; ")),
                        })),
                    );
                }
                all_warnings.extend(prompt_warnings);

                // Write prompt context
                if let Err(e) =
                    openclaw_compat::write_prompt_context(&skill_dir, &converted.prompt_context)
                {
                    let _ = std::fs::remove_dir_all(&skill_dir);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            serde_json::json!({"error": format!("Failed to write prompt_context: {e}")}),
                        ),
                    );
                }

                // Binary dependency check (same as clawhub)
                for bin in &converted.required_bins {
                    if which_check(bin).is_none() {
                        all_warnings.push(openfang_skills::verify::SkillWarning {
                            severity: WarningSeverity::Warning,
                            message: format!("Required binary not found: {bin}"),
                        });
                    }
                }

                converted.manifest
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&skill_dir);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("Failed to convert SKILL.md: {e}")})),
                );
            }
        }
    } else if openclaw_compat::detect_openclaw_skill(&skill_dir) {
        match openclaw_compat::convert_openclaw_skill(&skill_dir) {
            Ok(m) => m,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&skill_dir);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("Failed to convert skill: {e}")})),
                );
            }
        }
    } else {
        // let _ = std::fs::remove_dir_all(&skill_dir);
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Downloaded content is not a recognized skill format"
            })),
        );
    };

    // Manifest security scan
    let manifest_warnings = SkillVerifier::security_scan(&manifest);
    all_warnings.extend(manifest_warnings);

    // Write skill.toml (always, same as clawhub)
    // if let Err(e) = openclaw_compat::write_openfang_manifest(&skill_dir, &manifest) {
    //     let _ = std::fs::remove_dir_all(&skill_dir);
    //     return (
    //         StatusCode::INTERNAL_SERVER_ERROR,
    //         Json(serde_json::json!({"error": format!("Failed to write skill.toml: {e}")})),
    //     );
    // }

    // Hot-reload skills
    state.kernel.reload_skills();

    let warnings: Vec<serde_json::Value> = all_warnings
        .iter()
        .map(|w| {
            serde_json::json!({
                "severity": format!("{:?}", w.severity),
                "message": w.message,
            })
        })
        .collect();

    let translations: Vec<serde_json::Value> = tool_translations
        .iter()
        .map(|(from, to)| serde_json::json!({"from": from, "to": to}))
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "installed",
            "name": manifest.skill.name,
            "version": manifest.skill.version,
            "slug": slug,
            "sha256": sha256,
            "is_prompt_only": is_prompt_only,
            "warnings": warnings,
            "tool_translations": translations,
        })),
    )
}

/// Detect the root folder name inside a zip archive (first path component of the first entry).
fn detect_zip_root_folder(bytes: &[u8]) -> Option<String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    for i in 0..archive.len() {
        let file = archive.by_index(i).ok()?;
        let path = file.enclosed_name()?;
        if let Some(first) = path.components().next() {
            let name = first.as_os_str().to_string_lossy().to_string();
            // Skip __MACOSX metadata folder
            if !name.is_empty() && name != "__MACOSX" {
                return Some(name);
            }
        }
    }
    None
}

/// Extract skill name from a zip archive by peeking at skill.toml or SKILL.md frontmatter.
fn extract_skill_name_from_zip(bytes: &[u8]) -> Option<String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;

    // Try skill.toml first
    if let Ok(mut file) = archive.by_name("skill.toml") {
        let mut content = String::new();
        std::io::Read::read_to_string(&mut file, &mut content).ok()?;
        if let Ok(manifest) = toml::from_str::<openfang_skills::SkillManifest>(&content) {
            if !manifest.skill.name.is_empty() {
                return Some(sanitize_name(&manifest.skill.name));
            }
        }
    }

    // Try SKILL.md frontmatter
    if let Ok(mut file) = archive.by_name("SKILL.md") {
        let mut content = String::new();
        std::io::Read::read_to_string(&mut file, &mut content).ok()?;
        // Simple frontmatter name extraction: look for "name: xxx"
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("name:") {
                let name = trimmed.trim_start_matches("name:").trim().trim_matches('"');
                if !name.is_empty() {
                    return Some(sanitize_name(name));
                }
            }
            if trimmed == "---" && !content.starts_with("---") {
                break; // End of frontmatter
            }
        }
    }

    None
}

/// Sanitize a skill name to be filesystem-safe.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Check if a binary is available on PATH (same as clawhub.rs).
fn which_check(name: &str) -> Option<std::path::PathBuf> {
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("where").arg(name).output()
    } else {
        std::process::Command::new("which").arg(name).output()
    };

    match result {
        Ok(output) if output.status.success() => {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let first_line = path_str.lines().next()?;
            Some(std::path::PathBuf::from(first_line.trim()))
        }
        _ => None,
    }
}
