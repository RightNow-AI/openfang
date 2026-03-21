//! ClawHub marketplace client — search and install skills from clawhub.ai.
//!
//! ClawHub hosts 3,000+ community skills in both SKILL.md (prompt-only)
//! and package.json (Node.js) formats. This client downloads, converts,
//! and security-scans skills before installation.
//!
//! API reference: <https://clawhub.ai/api/v1/>
//! - Search: `GET /api/v1/search?q=...&limit=20`
//! - Browse: `GET /api/v1/skills?limit=20&sort=trending`
//! - Detail: `GET /api/v1/skills/{slug}`
//! - Download: `GET /api/v1/download?slug=...`
//! - File: `GET /api/v1/skills/{slug}/file?path=SKILL.md`
#![allow(unused)]
use crate::openclaw_compat;
use crate::verify::{SkillVerifier, SkillWarning, WarningSeverity};
use crate::SkillError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Retry constants for ClawHub API rate-limit handling
// ---------------------------------------------------------------------------

/// Maximum number of retry attempts for ClawHub API calls (including the first try).
const MAX_RETRIES: u32 = 5;

/// Base delay in milliseconds for exponential backoff (doubles each attempt).
const BASE_DELAY_MS: u64 = 1_500;

/// Maximum delay cap in milliseconds.
const MAX_DELAY_MS: u64 = 30_000;

// ---------------------------------------------------------------------------
// API response types (matching actual ClawHub v1 API — verified Feb 2026)
// ---------------------------------------------------------------------------

// -- Shared nested types ---------------------------------------------------

/// Stats nested inside browse entries and skill detail.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClawHubStats {
    #[serde(default)]
    pub comments: u64,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub installs_all_time: u64,
    #[serde(default)]
    pub installs_current: u64,
    #[serde(default)]
    pub stars: u64,
    #[serde(default)]
    pub versions: u64,
}

/// Version info nested inside browse entries and skill detail.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClawHubVersionInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub changelog: String,
}

/// Owner info from the skill detail endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClawHubOwner {
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub image: String,
}

// -- Browse: GET /api/v1/skills?limit=N&sort=trending ----------------------

/// A skill entry from the browse endpoint (`GET /api/v1/skills`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClawHubBrowseEntry {
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub description_zh: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub installs: u64,
    #[serde(default)]
    pub stars: u64,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub owner_name: String,
    #[serde(default)]
    pub homepage: String,
    /// Tags is a list of strings, or null.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Unix ms timestamp.
    #[serde(default)]
    pub updated_at: i64,
}

/// Data nested inside the browse response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClawHubBrowseData {
    #[serde(default)]
    pub skills: Vec<ClawHubBrowseEntry>,
    #[serde(default)]
    pub total: u64,
}

/// Paginated response from the browse endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClawHubBrowseResponse {
    #[serde(default)]
    pub code: i32,
    #[serde(default)]
    pub data: ClawHubBrowseData,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

// -- Search: GET /api/v1/search?q=...&limit=N ------------------------------

/// A skill entry from the search endpoint (`GET /api/v1/search`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClawHubSearchEntry {
    #[serde(default)]
    pub score: f64,
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub stars: u64,
    #[serde(default)]
    pub owner_name: String,
    /// Unix ms timestamp.
    #[serde(default)]
    pub updated_at: i64,
}

/// Data nested inside the search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClawHubSearchData {
    pub skills: Vec<ClawHubSearchEntry>,
    #[serde(default)]
    pub total: u64,
}

/// Response from the search endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClawHubSearchResponse {
    pub data: ClawHubSearchData,
}

// -- Detail: GET /api/v1/skills/{slug} -------------------------------------

/// The `skill` object nested inside the detail response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClawHubSkillInfo {
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub stats: ClawHubStats,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

/// Full detail response from `GET /api/v1/skills/{slug}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClawHubSkillDetail {
    pub skill: ClawHubSkillInfo,
    #[serde(default)]
    pub latest_version: Option<ClawHubVersionInfo>,
    #[serde(default)]
    pub owner: Option<ClawHubOwner>,
    /// Moderation status (null when clean).
    #[serde(default)]
    pub moderation: Option<serde_json::Value>,
}

// -- Sort enum -------------------------------------------------------------

/// Sort order for browsing skills.
#[derive(Debug, Clone, Copy)]
pub enum ClawHubSort {
    Trending,
    Updated,
    Downloads,
    Stars,
    Rating,
}

impl ClawHubSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Trending => "trending",
            Self::Updated => "updated",
            Self::Downloads => "downloads",
            Self::Stars => "stars",
            Self::Rating => "rating",
        }
    }
}

// -- Backward compat aliases -----------------------------------------------

/// Alias kept for code that still references the old name.
pub type ClawHubListResponse = ClawHubBrowseResponse;
/// Alias kept for code that still references the old name.
pub type ClawHubSearchResults = ClawHubSearchResponse;
/// Alias kept for code that still references the old name.
pub type ClawHubEntry = ClawHubBrowseEntry;

/// Result of installing a skill from ClawHub.
#[derive(Debug, Clone)]
pub struct ClawHubInstallResult {
    /// Installed skill name.
    pub skill_name: String,
    /// Installed version.
    pub version: String,
    /// The skill slug on ClawHub.
    pub slug: String,
    /// Security warnings from the scan pipeline.
    pub warnings: Vec<SkillWarning>,
    /// Tool name translations applied (OpenClaw → OpenFang).
    pub tool_translations: Vec<(String, String)>,
    /// Whether this is a prompt-only skill.
    pub is_prompt_only: bool,
}

/// Client for the ClawHub marketplace (clawhub.ai).
pub struct ClawHubClient {
    /// Base URL for the ClawHub API.
    base_url: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Local cache directory for downloaded skills.
    _cache_dir: PathBuf,
}

impl ClawHubClient {
    /// Create a new ClawHub client with default settings.
    ///
    /// Uses the official ClawHub API at `https://clawhub.ai/api/v1`.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self::with_url("https://lightmake.site/api", cache_dir)
    }

    /// Create a ClawHub client with a custom API URL.
    pub fn with_url(base_url: &str, cache_dir: PathBuf) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            _cache_dir: cache_dir,
        }
    }

    // -----------------------------------------------------------------------
    // Private: HTTP GET with retry on 429 / 5xx
    // -----------------------------------------------------------------------

    /// Issue a GET request with automatic retry on rate-limit (429) and
    /// server-error (5xx) responses. Respects the `Retry-After` header
    /// when present, otherwise uses exponential backoff with jitter.
    ///
    /// Returns the successful `reqwest::Response` or a `SkillError`.
    async fn get_with_retry(
        &self,
        url: &str,
        context: &str,
    ) -> Result<reqwest::Response, SkillError> {
        let mut last_status: Option<u16> = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                // Compute delay: use Retry-After from previous response if we
                // saved it, otherwise exponential backoff with jitter.
                let base = BASE_DELAY_MS.saturating_mul(1u64 << attempt.min(5));
                let delay_ms = base.min(MAX_DELAY_MS);
                // Add light jitter (0-25%) using system clock nanos.
                let jitter = {
                    let nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_nanos();
                    let frac = (nanos.wrapping_mul(2654435761) as f64) / (u32::MAX as f64);
                    (delay_ms as f64 * frac * 0.25) as u64
                };
                let total = delay_ms + jitter;
                debug!(
                    attempt,
                    delay_ms = total,
                    context,
                    "retrying ClawHub request after rate limit / server error"
                );
                tokio::time::sleep(std::time::Duration::from_millis(total)).await;
            }

            let result = self
                .client
                .get(url)
                .header("User-Agent", "OpenFang/0.1")
                .send()
                .await;

            match result {
                Ok(resp) => {
                    let status = resp.status();

                    if status.is_success() {
                        return Ok(resp);
                    }

                    // Rate-limited or server error — retryable.
                    if status.as_u16() == 429 || status.is_server_error() {
                        last_status = Some(status.as_u16());

                        // If the server sent Retry-After, respect it (capped).
                        if let Some(ra) = resp
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                        {
                            let capped = (ra * 1000).min(MAX_DELAY_MS);
                            if attempt + 1 < MAX_RETRIES {
                                debug!(
                                    retry_after_secs = ra,
                                    "ClawHub sent Retry-After, sleeping {capped}ms"
                                );
                                tokio::time::sleep(std::time::Duration::from_millis(capped)).await;
                            }
                        }

                        let is_last = attempt + 1 >= MAX_RETRIES;
                        if is_last {
                            if status.as_u16() == 429 {
                                return Err(SkillError::RateLimited(format!(
                                    "{context} returned 429 Too Many Requests after {MAX_RETRIES} attempts \
                                     — the ClawHub API rate limit has been exceeded, \
                                     please wait a few seconds and try again"
                                )));
                            }
                            return Err(SkillError::Network(format!(
                                "{context} returned {status} after {MAX_RETRIES} attempts"
                            )));
                        }
                        // Loop around to retry.
                        continue;
                    }

                    // Non-retryable HTTP error (4xx other than 429).
                    return Err(SkillError::Network(format!("{context} returned {status}")));
                }
                Err(e) => {
                    // Network / timeout error — retryable.
                    last_status = None;
                    let is_last = attempt + 1 >= MAX_RETRIES;
                    if is_last {
                        return Err(SkillError::Network(format!(
                            "{context} failed after {MAX_RETRIES} attempts: {e}"
                        )));
                    }
                    warn!(attempt, context, error = %e, "ClawHub request failed, will retry");
                }
            }
        }

        // Should be unreachable, but handle gracefully.
        Err(SkillError::Network(format!(
            "{context} failed (status: {last_status:?}) after {MAX_RETRIES} attempts"
        )))
    }

    // -----------------------------------------------------------------------
    // Public API methods — all use get_with_retry
    // -----------------------------------------------------------------------

    /// Search for skills on ClawHub using vector/semantic search.
    ///
    /// Uses `GET /api/v1/search?keyword=...&pageSize=...`.
    pub async fn search(
        &self,
        keyword: &str,
        page_size: u32,
        page: u32,
    ) -> Result<ClawHubSearchResponse, SkillError> {
        let url = format!(
            "{}/skills?keyword={}&pageSize={}&page={}&sortBy=score&order=desc",
            self.base_url,
            urlencoded(keyword),
            page_size.min(50),
            page
        );

        let response = self.get_with_retry(&url, "ClawHub search").await?;

        let results: ClawHubSearchResponse = response
            .json()
            .await
            .map_err(|e| SkillError::Network(format!("Failed to parse ClawHub response: {e}")))?;

        Ok(results)
    }

    /// Browse skills by category, sort field and order.
    ///
    /// Uses `GET /api/v1/skills?pageSize=...&category=...&sortBy=...&order=...`.
    pub async fn browse(
        &self,
        category: &str,
        page_size: u32,
        sort_by: &str,
        order: &str,
        cursor: Option<&str>,
        page: u32,
    ) -> Result<ClawHubBrowseResponse, SkillError> {
        let mut url = format!(
            "{}/skills?pageSize={}&page={}&category={}&sortBy=score&order=desc",
            self.base_url,
            page_size,
            page,
            urlencoded(category),
            // urlencoded(sort_by),
            // urlencoded(order),
        );

        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", urlencoded(c)));
        }

        let response = self.get_with_retry(&url, "ClawHub browse").await?;

        let raw = response
            .text()
            .await
            .map_err(|e| SkillError::Network(format!("Failed to read ClawHub browse body: {e}")))?;

        let results: ClawHubBrowseResponse = serde_json::from_str(&raw)
            .map_err(|e| SkillError::Network(format!("Failed to parse ClawHub browse: {e}")))?;

        info!(
            url = %url,
            items = results.data.skills.len(),
            next_cursor = ?results.next_cursor,
            "ClawHub browse response"
        );

        Ok(results)
    }

    /// Get detailed info about a specific skill.
    ///
    /// Uses `GET /api/v1/skills/{slug}`.
    /// Response is `{ skill: {...}, latestVersion: {...}, owner: {...}, moderation: null }`.
    pub async fn get_skill(&self, slug: &str) -> Result<ClawHubSkillDetail, SkillError> {
        let url = format!("{}/skills/{}", self.base_url, urlencoded(slug));

        let response = self.get_with_retry(&url, "ClawHub skill detail").await?;

        let detail: ClawHubSkillDetail = response
            .json()
            .await
            .map_err(|e| SkillError::Network(format!("Failed to parse ClawHub detail: {e}")))?;

        Ok(detail)
    }

    /// Helper: extract the version string from a browse entry.
    pub fn entry_version(entry: &ClawHubBrowseEntry) -> &str {
        entry.version.as_str()
    }

    /// Fetch a specific file from a skill (e.g., SKILL.md, README).
    ///
    /// Uses `GET /api/v1/skills/{slug}/file?path=SKILL.md`.
    pub async fn get_file(&self, slug: &str, path: &str) -> Result<String, SkillError> {
        let url = format!(
            "{}/skills/{}/file?path={}",
            self.base_url,
            urlencoded(slug),
            urlencoded(path)
        );

        let response = self.get_with_retry(&url, "ClawHub file fetch").await?;

        let text = response
            .text()
            .await
            .map_err(|e| SkillError::Network(format!("Failed to read ClawHub file: {e}")))?;

        Ok(text)
    }

    /// Install a skill from ClawHub into the target directory.
    ///
    /// Security pipeline:
    /// 1. Download skill zip and compute SHA256
    /// 2. Detect format (SKILL.md vs package.json)
    /// 3. Convert to OpenFang manifest
    /// 4. Run manifest security scan
    /// 5. If prompt-only: run prompt injection scan
    /// 6. Check binary dependencies
    /// 7. Write skill.toml with `verified: false`
    pub async fn install(
        &self,
        slug: &str,
        target_dir: &Path,
    ) -> Result<ClawHubInstallResult, SkillError> {
        // Use /api/v1/download?slug=... endpoint
        let url = format!("{}/download?slug={}", self.base_url, urlencoded(slug));

        info!(slug, "Downloading skill from ClawHub");

        // Use get_with_retry for the download — same 429/5xx handling as all
        // other endpoints, with 5 attempts and exponential backoff.
        let response = self.get_with_retry(&url, "ClawHub download").await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| SkillError::Network(format!("Failed to read download body: {e}")))?;

        // Step 1: SHA256 of downloaded content
        let sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            hex::encode(hasher.finalize())
        };
        info!(slug, sha256 = %sha256, "Downloaded skill");

        // Create skill directory
        let skill_dir = target_dir.join(slug);
        std::fs::create_dir_all(&skill_dir)?;

        // Detect content type and extract accordingly
        let content_str = String::from_utf8_lossy(&bytes);
        let is_skillmd = content_str.trim_start().starts_with("---");

        if is_skillmd {
            std::fs::write(skill_dir.join("SKILL.md"), &*bytes)?;
        } else if bytes.len() >= 4 && bytes[0] == 0x50 && bytes[1] == 0x4b {
            // Zip archive — extract all files
            let cursor = std::io::Cursor::new(&*bytes);
            match zip::ZipArchive::new(cursor) {
                Ok(mut archive) => {
                    for i in 0..archive.len() {
                        let mut file = match archive.by_index(i) {
                            Ok(f) => f,
                            Err(e) => {
                                warn!(index = i, error = %e, "Skipping zip entry");
                                continue;
                            }
                        };
                        let Some(enclosed_name) = file.enclosed_name() else {
                            warn!("Skipping zip entry with unsafe path");
                            continue;
                        };
                        let out_path = skill_dir.join(enclosed_name);
                        if file.is_dir() {
                            std::fs::create_dir_all(&out_path)?;
                        } else {
                            if let Some(parent) = out_path.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let mut out_file = std::fs::File::create(&out_path)?;
                            std::io::copy(&mut file, &mut out_file)?;
                        }
                    }
                    info!(slug, entries = archive.len(), "Extracted skill zip");
                }
                Err(e) => {
                    warn!(slug, error = %e, "Failed to read zip, saving raw");
                    std::fs::write(skill_dir.join("skill.zip"), &*bytes)?;
                }
            }
        } else {
            std::fs::write(skill_dir.join("package.json"), &*bytes)?;
        }

        // Step 2-3: Detect format and convert
        let mut all_warnings = Vec::new();
        let mut tool_translations = Vec::new();
        let mut is_prompt_only = false;

        let manifest = if is_skillmd || openclaw_compat::detect_skillmd(&skill_dir) {
            let converted = openclaw_compat::convert_skillmd(&skill_dir)?;
            tool_translations = converted.tool_translations;
            is_prompt_only =
                converted.manifest.runtime.runtime_type == crate::SkillRuntime::PromptOnly;

            // Step 5: Prompt injection scan
            let prompt_warnings = SkillVerifier::scan_prompt_content(&converted.prompt_context);
            if prompt_warnings
                .iter()
                .any(|w| w.severity == WarningSeverity::Critical)
            {
                // Block installation of skills with critical prompt injection
                let critical_msgs: Vec<_> = prompt_warnings
                    .iter()
                    .filter(|w| w.severity == WarningSeverity::Critical)
                    .map(|w| w.message.clone())
                    .collect();

                // Clean up skill directory on blocked install
                let _ = std::fs::remove_dir_all(&skill_dir);

                return Err(SkillError::SecurityBlocked(format!(
                    "Skill blocked due to prompt injection: {}",
                    critical_msgs.join("; ")
                )));
            }
            all_warnings.extend(prompt_warnings);

            // Write prompt context
            openclaw_compat::write_prompt_context(&skill_dir, &converted.prompt_context)?;

            // Step 6: Binary dependency check
            for bin in &converted.required_bins {
                if which_check(bin).is_none() {
                    all_warnings.push(SkillWarning {
                        severity: WarningSeverity::Warning,
                        message: format!("Required binary not found: {bin}"),
                    });
                }
            }

            converted.manifest
        } else if openclaw_compat::detect_openclaw_skill(&skill_dir) {
            openclaw_compat::convert_openclaw_skill(&skill_dir)?
        } else {
            return Err(SkillError::InvalidManifest(
                "Downloaded content is not a recognized skill format".to_string(),
            ));
        };

        // Step 4: Manifest security scan
        let manifest_warnings = SkillVerifier::security_scan(&manifest);
        all_warnings.extend(manifest_warnings);

        // Step 7: Write skill.toml
        openclaw_compat::write_openfang_manifest(&skill_dir, &manifest)?;

        let result = ClawHubInstallResult {
            skill_name: manifest.skill.name.clone(),
            version: manifest.skill.version.clone(),
            slug: slug.to_string(),
            warnings: all_warnings,
            tool_translations,
            is_prompt_only,
        };

        info!(
            slug,
            skill_name = %result.skill_name,
            warnings = result.warnings.len(),
            "Installed skill from ClawHub"
        );

        Ok(result)
    }

    /// Check if a ClawHub skill is already installed locally.
    pub fn is_installed(&self, slug: &str, skills_dir: &Path) -> bool {
        let skill_dir = skills_dir.join(slug);
        skill_dir.join("skill.toml").exists()
    }
}

/// RFC 3986 percent-encoding for query parameters.
/// Unreserved characters pass through, space becomes `+`, everything else is `%XX`.
fn urlencoded(s: &str) -> String {
    const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push(HEX_UPPER[(b >> 4) as usize] as char);
                result.push(HEX_UPPER[(b & 0xf) as usize] as char);
            }
        }
    }
    result
}

/// Check if a binary is available on PATH.
fn which_check(name: &str) -> Option<PathBuf> {
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("where").arg(name).output()
    } else {
        std::process::Command::new("which").arg(name).output()
    };

    match result {
        Ok(output) if output.status.success() => {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let first_line = path_str.lines().next()?;
            Some(PathBuf::from(first_line.trim()))
        }
        _ => None,
    }
}
