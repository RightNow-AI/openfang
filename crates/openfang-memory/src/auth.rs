use chrono::Utc;
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthUserRecord {
    pub id: String,
    pub provider: String,
    pub provider_user_id: String,
    pub login: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthSessionRecord {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub subject: String,
    pub issued_at: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthUserUpsert<'a> {
    pub provider: &'a str,
    pub provider_user_id: &'a str,
    pub login: Option<&'a str>,
    pub name: Option<&'a str>,
    pub email: Option<&'a str>,
    pub avatar_url: Option<&'a str>,
    pub default_role: &'a str,
}

pub struct AuthStore {
    conn: Arc<Mutex<Connection>>,
}

impl AuthStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn count_users(&self) -> OpenFangResult<u64> {
        let conn = self.lock_conn()?;
        conn.query_row("SELECT COUNT(*) FROM auth_users", [], |row| row.get(0))
            .map_err(db_error)
    }

    pub fn get_user_by_id(&self, user_id: &str) -> OpenFangResult<Option<AuthUserRecord>> {
        let conn = self.lock_conn()?;
        self.get_user_by_id_with_conn(&conn, user_id)
    }

    pub fn upsert_oauth_user(
        &self,
        input: OAuthUserUpsert<'_>,
    ) -> OpenFangResult<AuthUserRecord> {
        let conn = self.lock_conn()?;
        let now = Utc::now().to_rfc3339();

        if let Some(existing) = self.get_user_by_provider_subject_with_conn(
            &conn,
            input.provider,
            input.provider_user_id,
        )? {
            conn.execute(
                "UPDATE auth_users
                 SET login = COALESCE(?3, login),
                     name = COALESCE(?4, name),
                     email = COALESCE(?5, email),
                     avatar_url = COALESCE(?6, avatar_url),
                     updated_at = ?7,
                     last_login_at = ?7
                 WHERE provider = ?1 AND provider_user_id = ?2",
                params![
                    input.provider,
                    input.provider_user_id,
                    input.login,
                    input.name,
                    input.email,
                    input.avatar_url,
                    now,
                ],
            )
            .map_err(db_error)?;

            return self
                .get_user_by_id_with_conn(&conn, &existing.id)?
                .ok_or_else(|| {
                    OpenFangError::Memory(
                        "OAuth user disappeared after update".to_string(),
                    )
                });
        }

        let user_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO auth_users (
                id, provider, provider_user_id, login, name, email, avatar_url,
                role, created_at, updated_at, last_login_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?9)",
            params![
                user_id,
                input.provider,
                input.provider_user_id,
                input.login,
                input.name,
                input.email,
                input.avatar_url,
                input.default_role,
                now,
            ],
        )
        .map_err(db_error)?;

        self.get_user_by_id_with_conn(&conn, &user_id)?
            .ok_or_else(|| OpenFangError::Memory("Failed to load created OAuth user".to_string()))
    }

    pub fn create_session(
        &self,
        user_id: &str,
        provider: &str,
        subject: &str,
        expires_at: &str,
    ) -> OpenFangResult<AuthSessionRecord> {
        let conn = self.lock_conn()?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let issued_at = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO auth_sessions (
                id, user_id, provider, subject, issued_at, expires_at, revoked_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![session_id, user_id, provider, subject, issued_at, expires_at],
        )
        .map_err(db_error)?;

        self.get_session_with_conn(&conn, &session_id)?.ok_or_else(|| {
            OpenFangError::Memory("Failed to load created auth session".to_string())
        })
    }

    pub fn get_session(&self, session_id: &str) -> OpenFangResult<Option<AuthSessionRecord>> {
        let conn = self.lock_conn()?;
        self.get_session_with_conn(&conn, session_id)
    }

    pub fn is_session_active(&self, session_id: &str) -> OpenFangResult<bool> {
        let conn = self.lock_conn()?;
        let now = Utc::now().to_rfc3339();
        let active = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM auth_sessions
                    WHERE id = ?1
                      AND revoked_at IS NULL
                      AND expires_at > ?2
                )",
                params![session_id, now],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_error)?;
        Ok(active != 0)
    }

    pub fn revoke_session(&self, session_id: &str) -> OpenFangResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE auth_sessions SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            params![session_id, Utc::now().to_rfc3339()],
        )
        .map_err(db_error)?;
        Ok(())
    }

    fn lock_conn(&self) -> OpenFangResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| OpenFangError::Memory("Auth store connection lock poisoned".to_string()))
    }

    fn get_user_by_id_with_conn(
        &self,
        conn: &Connection,
        user_id: &str,
    ) -> OpenFangResult<Option<AuthUserRecord>> {
        conn.query_row(
            "SELECT id, provider, provider_user_id, login, name, email, avatar_url,
                    role, created_at, updated_at, last_login_at
             FROM auth_users WHERE id = ?1",
            params![user_id],
            map_auth_user_row,
        )
        .optional()
        .map_err(db_error)
    }

    fn get_user_by_provider_subject_with_conn(
        &self,
        conn: &Connection,
        provider: &str,
        provider_user_id: &str,
    ) -> OpenFangResult<Option<AuthUserRecord>> {
        conn.query_row(
            "SELECT id, provider, provider_user_id, login, name, email, avatar_url,
                    role, created_at, updated_at, last_login_at
             FROM auth_users
             WHERE provider = ?1 AND provider_user_id = ?2",
            params![provider, provider_user_id],
            map_auth_user_row,
        )
        .optional()
        .map_err(db_error)
    }

    fn get_session_with_conn(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> OpenFangResult<Option<AuthSessionRecord>> {
        conn.query_row(
            "SELECT id, user_id, provider, subject, issued_at, expires_at, revoked_at
             FROM auth_sessions WHERE id = ?1",
            params![session_id],
            map_auth_session_row,
        )
        .optional()
        .map_err(db_error)
    }
}

fn map_auth_user_row(row: &rusqlite::Row<'_>) -> Result<AuthUserRecord, rusqlite::Error> {
    Ok(AuthUserRecord {
        id: row.get(0)?,
        provider: row.get(1)?,
        provider_user_id: row.get(2)?,
        login: row.get(3)?,
        name: row.get(4)?,
        email: row.get(5)?,
        avatar_url: row.get(6)?,
        role: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        last_login_at: row.get(10)?,
    })
}

fn map_auth_session_row(
    row: &rusqlite::Row<'_>,
) -> Result<AuthSessionRecord, rusqlite::Error> {
    Ok(AuthSessionRecord {
        id: row.get(0)?,
        user_id: row.get(1)?,
        provider: row.get(2)?,
        subject: row.get(3)?,
        issued_at: row.get(4)?,
        expires_at: row.get(5)?,
        revoked_at: row.get(6)?,
    })
}

fn db_error(error: rusqlite::Error) -> OpenFangError {
    OpenFangError::Memory(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    #[test]
    fn upsert_oauth_user_creates_and_updates_records() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let store = AuthStore::new(Arc::new(Mutex::new(conn)));

        let created = store
            .upsert_oauth_user(OAuthUserUpsert {
                provider: "github",
                provider_user_id: "12345",
                login: Some("octocat"),
                name: Some("Octo Cat"),
                email: None,
                avatar_url: Some("https://example.com/avatar.png"),
                default_role: "owner",
            })
            .unwrap();
        assert_eq!(created.role, "owner");
        assert_eq!(store.count_users().unwrap(), 1);

        let updated = store
            .upsert_oauth_user(OAuthUserUpsert {
                provider: "github",
                provider_user_id: "12345",
                login: Some("octocat"),
                name: Some("The Octocat"),
                email: Some("octo@example.com"),
                avatar_url: None,
                default_role: "user",
            })
            .unwrap();
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.role, "owner");
        assert_eq!(updated.name.as_deref(), Some("The Octocat"));
        assert_eq!(updated.email.as_deref(), Some("octo@example.com"));
    }

    #[test]
    fn sessions_can_be_revoked() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let store = AuthStore::new(Arc::new(Mutex::new(conn)));
        let user = store
            .upsert_oauth_user(OAuthUserUpsert {
                provider: "github",
                provider_user_id: "999",
                login: Some("user"),
                name: Some("User"),
                email: None,
                avatar_url: None,
                default_role: "user",
            })
            .unwrap();
        let expires_at = (Utc::now() + chrono::Duration::hours(4)).to_rfc3339();
        let session = store
            .create_session(&user.id, "github", "999", &expires_at)
            .unwrap();

        assert!(store.is_session_active(&session.id).unwrap());
        store.revoke_session(&session.id).unwrap();
        assert!(!store.is_session_active(&session.id).unwrap());
    }
}
