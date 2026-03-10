use crate::{
    auth::{extract_bearer, hash_token, sha256_hex, validate_package_id},
    error::{RegistryError, RegistryResult},
    models::{
        HandPackage, PackageVersion, PublishRequest, PublishResponse, SearchQuery, SearchResponse,
        UserAccount,
    },
    store::RegistryStore,
};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::HeaderMap,
    Json,
};
use chrono::Utc;
use leptos::prelude::LeptosOptions;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

/// Shared application state for all route handlers.
/// `LeptosOptions` is included so that `LeptosRoutes` (which requires `LeptosOptions: FromRef<S>`)
/// can be used with this state type.
#[derive(Clone)]
pub struct AppState {
    pub store: RegistryStore,
    pub jwt_secret: Vec<u8>,
    /// Base URL for download links (e.g. "https://fanghub.paradiseai.io")
    pub base_url: String,
    /// Leptos configuration — required by `LeptosRoutes` trait.
    pub leptos_options: LeptosOptions,
}

impl axum::extract::FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}


// ─── Authentication helper ───────────────────────────────────────────────────

/// Extract and validate the Bearer token from request headers.
/// Returns the authenticated user's GitHub login.
async fn authenticate(
    headers: &HeaderMap,
    state: &AppState,
) -> RegistryResult<UserAccount> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(RegistryError::Unauthenticated)?;

    let token = extract_bearer(auth_header).ok_or(RegistryError::Unauthenticated)?;

    // Validate JWT
    let claims = crate::auth::validate_token(token, &state.jwt_secret)?;

    // Look up user in DB by token hash
    let token_hash = hash_token(token);
    let user = state
        .store
        .find_user_by_token_hash(&token_hash)
        .await?
        .ok_or(RegistryError::Unauthenticated)?;

    // Verify the token belongs to the claimed user
    if user.github_login != claims.sub {
        return Err(RegistryError::Unauthenticated);
    }

    Ok(user)
}

// ─── Health & Stats ──────────────────────────────────────────────────────────

/// GET /health — liveness probe
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "fanghub-registry" }))
}

/// GET /stats — aggregate registry statistics
pub async fn get_stats(State(state): State<AppState>) -> Result<Json<Value>, RegistryError> {
    let stats = state.store.get_stats().await?;
    Ok(Json(json!({
        "total_packages": stats.total_packages,
        "total_versions": stats.total_versions,
        "total_installs": stats.total_installs,
        "total_publishers": stats.total_publishers,
    })))
}

// ─── Package routes ──────────────────────────────────────────────────────────

/// GET /packages?q=...&category=...&tag=...&sort=...&page=...&per_page=...
pub async fn search_packages(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, RegistryError> {
    let (results, total) = state.store.search(&query).await?;
    let per_page = query.per_page.unwrap_or(20).min(100).max(1);
    let page = query.page.unwrap_or(1).max(1);
    let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;

    Ok(Json(SearchResponse {
        results,
        total,
        page,
        per_page,
        total_pages,
    }))
}

/// GET /packages/:package_id — get package metadata
pub async fn get_package(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
) -> Result<Json<HandPackage>, RegistryError> {
    let pkg = state
        .store
        .get_package(&package_id)
        .await?
        .ok_or_else(|| RegistryError::PackageNotFound(package_id.clone()))?;
    Ok(Json(pkg))
}

/// GET /packages/:package_id/versions — list all versions
pub async fn list_versions(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
) -> Result<Json<Vec<PackageVersion>>, RegistryError> {
    // Ensure the package exists
    state
        .store
        .get_package(&package_id)
        .await?
        .ok_or_else(|| RegistryError::PackageNotFound(package_id.clone()))?;

    let versions = state.store.list_versions(&package_id).await?;
    Ok(Json(versions))
}

/// GET /packages/:package_id/versions/:version — get a specific version
pub async fn get_version(
    State(state): State<AppState>,
    Path((package_id, version)): Path<(String, String)>,
) -> Result<Json<PackageVersion>, RegistryError> {
    let ver = state
        .store
        .get_version(&package_id, &version)
        .await?
        .ok_or_else(|| RegistryError::VersionNotFound {
            package: package_id.clone(),
            version: version.clone(),
        })?;
    Ok(Json(ver))
}

/// POST /packages/:package_id/versions — publish a new version
///
/// Accepts multipart/form-data with:
/// - `manifest` (text): HAND.toml content
/// - `archive` (file): .tar.gz package archive
/// - `release_notes` (text, optional): release notes
/// - `signature` (text, optional): Ed25519 signature of checksum
pub async fn publish_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(package_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<PublishResponse>, RegistryError> {
    // Authenticate
    let user = authenticate(&headers, &state).await?;

    // Validate package ID
    validate_package_id(&package_id)?;

    // Parse multipart fields
    let mut manifest_content: Option<String> = None;
    let mut archive_bytes: Option<Vec<u8>> = None;
    let mut release_notes: Option<String> = None;
    let mut signature: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        RegistryError::InvalidManifest(format!("Multipart parse error: {e}"))
    })? {
        match field.name() {
            Some("manifest") => {
                manifest_content = Some(field.text().await.map_err(|e| {
                    RegistryError::InvalidManifest(format!("Failed to read manifest: {e}"))
                })?);
            }
            Some("archive") => {
                archive_bytes = Some(field.bytes().await.map_err(|e| {
                    RegistryError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to read archive: {e}"),
                    ))
                })?.to_vec());
            }
            Some("release_notes") => {
                release_notes = Some(field.text().await.map_err(|e| {
                    RegistryError::InvalidManifest(format!("Failed to read release_notes: {e}"))
                })?);
            }
            Some("signature") => {
                signature = Some(field.text().await.map_err(|e| {
                    RegistryError::InvalidManifest(format!("Failed to read signature: {e}"))
                })?);
            }
            _ => {} // ignore unknown fields
        }
    }

    let manifest = manifest_content
        .ok_or_else(|| RegistryError::InvalidManifest("Missing 'manifest' field".to_string()))?;
    let archive = archive_bytes
        .ok_or_else(|| RegistryError::InvalidManifest("Missing 'archive' field".to_string()))?;

    // Parse manifest to extract version
    let manifest_toml: toml::Value = toml::from_str(&manifest)
        .map_err(|e| RegistryError::InvalidManifest(format!("Invalid TOML: {e}")))?;
    let version = manifest_toml
        .get("hand")
        .and_then(|h| h.get("version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| RegistryError::InvalidManifest("Missing [hand].version field".to_string()))?
        .to_string();

    // Validate version is semver
    version.parse::<semver::Version>()
        .map_err(|e| RegistryError::InvalidVersion(format!("Invalid semver '{version}': {e}")))?;

    // Compute archive checksum
    let checksum = sha256_hex(&archive);

    // Check if package exists; create it if not
    let pkg_exists = state.store.get_package(&package_id).await?.is_some();
    if !pkg_exists {
        // Auto-create the package on first publish
        let name = manifest_toml
            .get("hand")
            .and_then(|h| h.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&package_id)
            .to_string();
        let description = manifest_toml
            .get("hand")
            .and_then(|h| h.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let category = manifest_toml
            .get("hand")
            .and_then(|h| h.get("category"))
            .and_then(|v| v.as_str())
            .unwrap_or("Other")
            .to_string();
        let tags: Vec<String> = manifest_toml
            .get("hand")
            .and_then(|h| h.get("tags"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let repository_url = manifest_toml
            .get("hand")
            .and_then(|h| h.get("repository"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let new_pkg = HandPackage {
            id: Uuid::new_v4(),
            package_id: package_id.clone(),
            name,
            description,
            category,
            owner: user.github_login.clone(),
            latest_version: None,
            install_count: 0,
            tags,
            repository_url,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state.store.create_package(&new_pkg).await?;
    } else {
        // Verify ownership
        let pkg = state.store.get_package(&package_id).await?.unwrap();
        if pkg.owner != user.github_login {
            return Err(RegistryError::Forbidden(format!(
                "Package '{}' is owned by '{}', not '{}'",
                package_id, pkg.owner, user.github_login
            )));
        }
    }

    // Build download URL (in production this would point to object storage)
    let download_url = format!(
        "{}/packages/{}/versions/{}/download",
        state.base_url, package_id, version
    );

    let now = Utc::now();
    let ver = PackageVersion {
        id: Uuid::new_v4(),
        package_id: package_id.clone(),
        version: version.clone(),
        manifest,
        checksum: checksum.clone(),
        signature,
        download_url: download_url.clone(),
        archive_size_bytes: archive.len() as u64,
        release_notes,
        install_count: 0,
        published_at: now,
        published_by: user.github_login.clone(),
    };

    state.store.publish_version(&ver).await?;
    state.store.update_package_version(&package_id, &version).await?;

    tracing::info!(
        package_id = %package_id,
        version = %version,
        publisher = %user.github_login,
        "Published new Hand package version"
    );

    Ok(Json(PublishResponse {
        package_id,
        version,
        download_url,
        checksum,
        published_at: now,
    }))
}

/// POST /packages/:package_id/versions/:version/install — record an install
pub async fn record_install(
    State(state): State<AppState>,
    Path((package_id, version)): Path<(String, String)>,
) -> Result<Json<Value>, RegistryError> {
    // Verify the version exists
    state
        .store
        .get_version(&package_id, &version)
        .await?
        .ok_or_else(|| RegistryError::VersionNotFound {
            package: package_id.clone(),
            version: version.clone(),
        })?;

    state
        .store
        .increment_install_count(&package_id, &version)
        .await?;

    Ok(Json(json!({ "recorded": true })))
}

// ─── User routes ─────────────────────────────────────────────────────────────

/// GET /users/:login — get public user profile
pub async fn get_user_profile(
    State(state): State<AppState>,
    Path(login): Path<String>,
) -> Result<Json<Value>, RegistryError> {
    let user = state
        .store
        .find_user_by_login(&login)
        .await?
        .ok_or_else(|| RegistryError::UserNotFound(login.clone()))?;

    let packages = state.store.list_packages_by_owner(&login).await?;

    Ok(Json(json!({
        "github_login": user.github_login,
        "display_name": user.display_name,
        "avatar_url": user.avatar_url,
        "package_count": packages.len(),
        "packages": packages.iter().map(|p| json!({
            "package_id": p.package_id,
            "name": p.name,
            "latest_version": p.latest_version,
            "install_count": p.install_count,
        })).collect::<Vec<_>>(),
        "member_since": user.created_at,
    })))
}

/// GET /me — get the authenticated user's profile
pub async fn get_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, RegistryError> {
    let user = authenticate(&headers, &state).await?;
    let packages = state.store.list_packages_by_owner(&user.github_login).await?;

    Ok(Json(json!({
        "github_login": user.github_login,
        "display_name": user.display_name,
        "avatar_url": user.avatar_url,
        "package_count": packages.len(),
        "packages": packages.iter().map(|p| json!({
            "package_id": p.package_id,
            "name": p.name,
            "latest_version": p.latest_version,
            "install_count": p.install_count,
        })).collect::<Vec<_>>(),
    })))
}
