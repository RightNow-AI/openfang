use crate::error::RegistryResult;
use surrealdb::{
    engine::any::{connect, Any},
    Surreal,
};

/// Initialize the SurrealDB connection and apply the FangHub schema.
pub async fn init_db(connection_str: &str) -> RegistryResult<Surreal<Any>> {
    let db = connect(connection_str).await?;
    db.use_ns("fanghub").use_db("registry").await?;
    apply_schema(&db).await?;
    Ok(db)
}

/// Apply the FangHub schema to the database.
/// Uses DEFINE TABLE ... IF NOT EXISTS so it is safe to call on every startup.
async fn apply_schema(db: &Surreal<Any>) -> RegistryResult<()> {
    // Users table
    db.query(
        "DEFINE TABLE IF NOT EXISTS fh_users SCHEMAFULL;
         DEFINE FIELD IF NOT EXISTS id ON TABLE fh_users TYPE uuid;
         DEFINE FIELD IF NOT EXISTS github_login ON TABLE fh_users TYPE string;
         DEFINE FIELD IF NOT EXISTS github_id ON TABLE fh_users TYPE int;
         DEFINE FIELD IF NOT EXISTS display_name ON TABLE fh_users TYPE string;
         DEFINE FIELD IF NOT EXISTS avatar_url ON TABLE fh_users TYPE option<string>;
         DEFINE FIELD IF NOT EXISTS api_token_hash ON TABLE fh_users TYPE string;
         DEFINE FIELD IF NOT EXISTS created_at ON TABLE fh_users TYPE datetime;
         DEFINE FIELD IF NOT EXISTS updated_at ON TABLE fh_users TYPE datetime;
         DEFINE INDEX IF NOT EXISTS idx_users_github_login ON TABLE fh_users COLUMNS github_login UNIQUE;
         DEFINE INDEX IF NOT EXISTS idx_users_github_id ON TABLE fh_users COLUMNS github_id UNIQUE;
         DEFINE INDEX IF NOT EXISTS idx_users_token_hash ON TABLE fh_users COLUMNS api_token_hash UNIQUE;",
    )
    .await?;

    // Packages table
    db.query(
        "DEFINE TABLE IF NOT EXISTS fh_packages SCHEMAFULL;
         DEFINE FIELD IF NOT EXISTS id ON TABLE fh_packages TYPE uuid;
         DEFINE FIELD IF NOT EXISTS package_id ON TABLE fh_packages TYPE string;
         DEFINE FIELD IF NOT EXISTS name ON TABLE fh_packages TYPE string;
         DEFINE FIELD IF NOT EXISTS description ON TABLE fh_packages TYPE string;
         DEFINE FIELD IF NOT EXISTS category ON TABLE fh_packages TYPE string;
         DEFINE FIELD IF NOT EXISTS owner ON TABLE fh_packages TYPE string;
         DEFINE FIELD IF NOT EXISTS latest_version ON TABLE fh_packages TYPE option<string>;
         DEFINE FIELD IF NOT EXISTS install_count ON TABLE fh_packages TYPE int DEFAULT 0;
         DEFINE FIELD IF NOT EXISTS tags ON TABLE fh_packages TYPE array<string> DEFAULT [];
         DEFINE FIELD IF NOT EXISTS repository_url ON TABLE fh_packages TYPE option<string>;
         DEFINE FIELD IF NOT EXISTS created_at ON TABLE fh_packages TYPE datetime;
         DEFINE FIELD IF NOT EXISTS updated_at ON TABLE fh_packages TYPE datetime;
         DEFINE INDEX IF NOT EXISTS idx_packages_package_id ON TABLE fh_packages COLUMNS package_id UNIQUE;
         DEFINE INDEX IF NOT EXISTS idx_packages_owner ON TABLE fh_packages COLUMNS owner;
         DEFINE INDEX IF NOT EXISTS idx_packages_category ON TABLE fh_packages COLUMNS category;",
    )
    .await?;

    // Versions table
    db.query(
        "DEFINE TABLE IF NOT EXISTS fh_versions SCHEMAFULL;
         DEFINE FIELD IF NOT EXISTS id ON TABLE fh_versions TYPE uuid;
         DEFINE FIELD IF NOT EXISTS package_id ON TABLE fh_versions TYPE string;
         DEFINE FIELD IF NOT EXISTS version ON TABLE fh_versions TYPE string;
         DEFINE FIELD IF NOT EXISTS manifest ON TABLE fh_versions TYPE string;
         DEFINE FIELD IF NOT EXISTS checksum ON TABLE fh_versions TYPE string;
         DEFINE FIELD IF NOT EXISTS signature ON TABLE fh_versions TYPE option<string>;
         DEFINE FIELD IF NOT EXISTS download_url ON TABLE fh_versions TYPE string;
         DEFINE FIELD IF NOT EXISTS archive_size_bytes ON TABLE fh_versions TYPE int DEFAULT 0;
         DEFINE FIELD IF NOT EXISTS release_notes ON TABLE fh_versions TYPE option<string>;
         DEFINE FIELD IF NOT EXISTS install_count ON TABLE fh_versions TYPE int DEFAULT 0;
         DEFINE FIELD IF NOT EXISTS published_at ON TABLE fh_versions TYPE datetime;
         DEFINE FIELD IF NOT EXISTS published_by ON TABLE fh_versions TYPE string;
         DEFINE INDEX IF NOT EXISTS idx_versions_package_version ON TABLE fh_versions COLUMNS package_id, version UNIQUE;
         DEFINE INDEX IF NOT EXISTS idx_versions_package_id ON TABLE fh_versions COLUMNS package_id;",
    )
    .await?;

    tracing::info!("FangHub registry schema applied successfully");
    Ok(())
}

/// Initialize an in-memory database for testing.
pub async fn init_test_db() -> RegistryResult<Surreal<Any>> {
    init_db("mem://").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schema_applies_without_error() {
        let db = init_test_db().await.unwrap();
        // Apply schema twice — should be idempotent
        apply_schema(&db).await.unwrap();
        apply_schema(&db).await.unwrap();
    }
}
