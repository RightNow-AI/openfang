//! PostgreSQL implementation of the paired devices store.

use crate::backends::PairedDevicesBackend;
use deadpool_postgres::Pool;
use openfang_types::error::{OpenFangError, OpenFangResult};

pub struct PgPairedDevicesStore {
    pool: Pool,
}

impl PgPairedDevicesStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
    }
}

impl PairedDevicesBackend for PgPairedDevicesStore {
    fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .query(
                    "SELECT device_id, display_name, platform, paired_at, last_seen, push_token FROM paired_devices",
                    &[],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "device_id": r.get::<_, String>(0),
                        "display_name": r.get::<_, String>(1),
                        "platform": r.get::<_, String>(2),
                        "paired_at": r.get::<_, String>(3),
                        "last_seen": r.get::<_, String>(4),
                        "push_token": r.get::<_, Option<String>>(5),
                    })
                })
                .collect())
        })
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
        let device_id = device_id.to_string();
        let display_name = display_name.to_string();
        let platform = platform.to_string();
        let paired_at = paired_at.to_string();
        let last_seen = last_seen.to_string();
        let push_token = push_token.map(|s| s.to_string());
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO paired_devices (device_id, display_name, platform, paired_at, last_seen, push_token)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (device_id) DO UPDATE SET
                       display_name = EXCLUDED.display_name,
                       platform = EXCLUDED.platform,
                       paired_at = EXCLUDED.paired_at,
                       last_seen = EXCLUDED.last_seen,
                       push_token = EXCLUDED.push_token",
                    &[&device_id, &display_name, &platform, &paired_at, &last_seen, &push_token],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
        let device_id = device_id.to_string();
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "DELETE FROM paired_devices WHERE device_id = $1",
                    &[&device_id],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }
}
