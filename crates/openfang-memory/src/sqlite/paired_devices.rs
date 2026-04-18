//! SQLite implementation of the paired devices store.

use crate::backends::PairedDevicesBackend;
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Paired-devices store backed by SQLite.
#[derive(Clone)]
pub struct SqlitePairedDevicesStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqlitePairedDevicesStore {
    /// Create a new paired-devices store wrapping the given connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

impl PairedDevicesBackend for SqlitePairedDevicesStore {
    fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT device_id, display_name, platform, paired_at, last_seen, push_token FROM paired_devices",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "device_id": row.get::<_, String>(0)?,
                    "display_name": row.get::<_, String>(1)?,
                    "platform": row.get::<_, String>(2)?,
                    "paired_at": row.get::<_, String>(3)?,
                    "last_seen": row.get::<_, String>(4)?,
                    "push_token": row.get::<_, Option<String>>(5)?,
                }))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let mut devices = Vec::new();
        for row in rows {
            devices.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(devices)
    }

    fn save_paired_device(
        &self,
        device_id: &str,
        display_name: &str,
        platform: &str,
        paired_at: &str,
        last_seen: &str,
        push_token: Option<&str>,
    ) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO paired_devices (device_id, display_name, platform, paired_at, last_seen, push_token) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![device_id, display_name, platform, paired_at, last_seen, push_token],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute(
            "DELETE FROM paired_devices WHERE device_id = ?1",
            rusqlite::params![device_id],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }
}
