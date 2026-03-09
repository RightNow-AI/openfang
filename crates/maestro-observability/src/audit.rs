//! Audit log — append-only compliance log for all agent actions.

use crate::AuditEntry;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

/// In-memory append-only audit log (ring buffer, 50k entries max).
#[derive(Clone)]
pub struct AuditLog {
    entries: Arc<RwLock<Vec<AuditEntry>>>,
    max_entries: usize,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            max_entries,
        }
    }

    /// Append a new audit entry.
    pub async fn log(
        &self,
        actor: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        details: serde_json::Value,
        ip_address: Option<String>,
    ) -> AuditEntry {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            actor: actor.into(),
            action: action.into(),
            resource: resource.into(),
            details,
            ip_address,
        };
        info!(
            audit_id = %entry.id,
            actor = %entry.actor,
            action = %entry.action,
            resource = %entry.resource,
            "Audit entry recorded"
        );
        let mut entries = self.entries.write().await;
        if entries.len() >= self.max_entries {
            entries.drain(0..1000);
        }
        entries.push(entry.clone());
        entry
    }

    /// Get the N most recent audit entries.
    pub async fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries.iter().rev().take(limit).cloned().collect()
    }

    /// Search entries by actor.
    pub async fn by_actor(&self, actor: &str, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .rev()
            .filter(|e| e.actor == actor)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Search entries by resource.
    pub async fn by_resource(&self, resource: &str, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .rev()
            .filter(|e| e.resource == resource)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Search entries by action keyword.
    pub async fn by_action(&self, action: &str, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .rev()
            .filter(|e| e.action.contains(action))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Total number of entries.
    pub async fn count(&self) -> usize {
        self.entries.read().await.len()
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new(50_000)
    }
}
