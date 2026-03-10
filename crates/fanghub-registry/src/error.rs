use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// All errors that can occur in the FangHub registry.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Version not found: {package} v{version}")]
    VersionNotFound { package: String, version: String },

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Package already exists: {0}")]
    PackageAlreadyExists(String),

    #[error("Version already published: {package} v{version}")]
    VersionAlreadyPublished { package: String, version: String },

    #[error("Invalid package ID: {0}")]
    InvalidPackageId(String),

    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("Signature verification failed: {0}")]
    SignatureInvalid(String),

    #[error("Authentication required")]
    Unauthenticated,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type RegistryResult<T> = Result<T, RegistryError>;

/// Convert RegistryError into an HTTP response with appropriate status code.
impl IntoResponse for RegistryError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            RegistryError::PackageNotFound(_) | RegistryError::VersionNotFound { .. } | RegistryError::UserNotFound(_) => {
                (StatusCode::NOT_FOUND, self.to_string())
            }
            RegistryError::PackageAlreadyExists(_) | RegistryError::VersionAlreadyPublished { .. } => {
                (StatusCode::CONFLICT, self.to_string())
            }
            RegistryError::InvalidPackageId(_)
            | RegistryError::InvalidVersion(_)
            | RegistryError::InvalidManifest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            RegistryError::SignatureInvalid(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            RegistryError::Unauthenticated => (StatusCode::UNAUTHORIZED, self.to_string()),
            RegistryError::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            RegistryError::Database(_) | RegistryError::Serialization(_) | RegistryError::Io(_) | RegistryError::Internal(_) => {
                tracing::error!("Internal registry error: {}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal error occurred".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
