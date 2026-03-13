use chrono::Utc;
use openfang_types::agent_profile::AgentProfile;
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AgencyProfileStore {
    conn: Arc<Mutex<Connection>>,
}

impl AgencyProfileStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn upsert_profile(
        &self,
        profile: &AgentProfile,
        source_path: &Path,
    ) -> OpenFangResult<AgentProfile> {
        let conn = self.lock_conn()?;
        let payload = serde_json::to_string(profile)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO agent_profiles (id, source_path, payload, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 source_path = excluded.source_path,
                 payload = excluded.payload,
                 enabled = excluded.enabled,
                 updated_at = excluded.updated_at",
            params![
                profile.id,
                source_path.display().to_string(),
                payload,
                if profile.enabled { 1 } else { 0 },
                now,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        Ok(profile.clone())
    }

    pub fn list_profiles(&self) -> OpenFangResult<Vec<AgentProfile>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT payload, enabled
                 FROM agent_profiles
                 ORDER BY updated_at DESC, id ASC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map([], row_to_profile)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(profiles)
    }

    pub fn get_profile(&self, profile_id: &str) -> OpenFangResult<Option<AgentProfile>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT payload, enabled
                 FROM agent_profiles
                 WHERE id = ?1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        match stmt.query_row(params![profile_id], row_to_profile) {
            Ok(profile) => Ok(Some(profile)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    pub fn set_profile_enabled(
        &self,
        profile_id: &str,
        enabled: bool,
    ) -> OpenFangResult<AgentProfile> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT payload
                 FROM agent_profiles
                 WHERE id = ?1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let payload: String = match stmt.query_row(params![profile_id], |row| row.get(0)) {
            Ok(payload) => payload,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(OpenFangError::InvalidInput(format!(
                    "Agent profile not found: {profile_id}"
                )));
            }
            Err(e) => return Err(OpenFangError::Memory(e.to_string())),
        };

        let mut profile: AgentProfile = serde_json::from_str(&payload)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        profile.enabled = enabled;
        let updated_payload = serde_json::to_string(&profile)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;

        conn.execute(
            "UPDATE agent_profiles
             SET enabled = ?2, payload = ?3, updated_at = ?4
             WHERE id = ?1",
            params![
                profile_id,
                if enabled { 1 } else { 0 },
                updated_payload,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        Ok(profile)
    }

    fn lock_conn(&self) -> OpenFangResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))
    }
}

fn row_to_profile(row: &rusqlite::Row<'_>) -> Result<AgentProfile, rusqlite::Error> {
    let payload: String = row.get(0)?;
    let enabled = row.get::<_, i64>(1)? != 0;
    let mut profile: AgentProfile = serde_json::from_str(&payload).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    profile.enabled = enabled;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::AgencyProfileStore;
    use crate::migration::run_migrations;
    use openfang_agency_import::import_profile_from_path;
    use rusqlite::Connection;
    use std::fs;
    use std::sync::{Arc, Mutex};

    fn write_profile_file() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let profile_dir = temp_dir.path().join("support");
        fs::create_dir_all(&profile_dir).unwrap();
        let profile_path = profile_dir.join("support-support-responder.md");
        fs::write(
            &profile_path,
            "# Support Responder Agent Personality\n\n## 🧠 Your Identity & Memory\n- **Role**: Customer support specialist\n- **Personality**: Empathetic, precise\n- **Memory**: Successful support patterns\n\n## 🎯 Your Core Mission\n### Resolve customer issues\n- Keep response quality high\n\n## 🔄 Your Workflow Process\n### Step 1: Intake\n- Review context\n\n## 📋 Your Deliverable Template\n```markdown\n# Support Report\n## Summary\n```\n",
        )
        .unwrap();
        (temp_dir, profile_path)
    }

    fn make_store() -> AgencyProfileStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        AgencyProfileStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn imported_profile_round_trips_through_store() {
        let (_temp_dir, profile_path) = write_profile_file();
        let profile = import_profile_from_path(&profile_path).unwrap();
        let store = make_store();

        let saved = store.upsert_profile(&profile, &profile_path).unwrap();
        assert_eq!(saved.id, "support-responder");

        let fetched = store.get_profile("support-responder").unwrap().unwrap();
        assert_eq!(fetched.display_name, "Support Responder");
        assert!(fetched.enabled);

        let listed = store.list_profiles().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "support-responder");
    }

    #[test]
    fn enabled_toggle_persists_in_store() {
        let (_temp_dir, profile_path) = write_profile_file();
        let profile = import_profile_from_path(&profile_path).unwrap();
        let store = make_store();

        store.upsert_profile(&profile, &profile_path).unwrap();
        let updated = store.set_profile_enabled("support-responder", false).unwrap();
        assert!(!updated.enabled);

        let fetched = store.get_profile("support-responder").unwrap().unwrap();
        assert!(!fetched.enabled);
    }
}