//! RBAC authentication and authorization for multi-user access control.
//!
//! The AuthManager maps platform user identities (Telegram ID, Discord ID, etc.)
//! to OpenFang users with roles, then enforces permission checks on actions.

use dashmap::DashMap;
use openfang_types::agent::UserId;
use openfang_types::config::UserConfig;
use openfang_types::error::{OpenFangError, OpenFangResult};
use std::fmt;
use tracing::info;

/// User roles with hierarchical permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UserRole {
    /// Read-only access — can view agent output but cannot interact.
    Viewer = 0,
    /// Standard user — can chat with agents.
    User = 1,
    /// Admin — can spawn/kill agents, install skills, view usage.
    Admin = 2,
    /// Owner — full access including user management and config changes.
    Owner = 3,
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserRole::Viewer => write!(f, "viewer"),
            UserRole::User => write!(f, "user"),
            UserRole::Admin => write!(f, "admin"),
            UserRole::Owner => write!(f, "owner"),
        }
    }
}

impl UserRole {
    /// Parse a role from a string.
    pub fn from_str_role(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "owner" => UserRole::Owner,
            "admin" => UserRole::Admin,
            "viewer" => UserRole::Viewer,
            _ => UserRole::User,
        }
    }
}

/// Actions that can be authorized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Chat with an agent.
    ChatWithAgent,
    /// Spawn a new agent.
    SpawnAgent,
    /// Kill a running agent.
    KillAgent,
    /// Install a skill.
    InstallSkill,
    /// View kernel configuration.
    ViewConfig,
    /// Modify kernel configuration.
    ModifyConfig,
    /// View usage/billing data.
    ViewUsage,
    /// Manage users (create, delete, change roles).
    ManageUsers,
    /// Resolve (approve/deny) a pending execution-approval request.
    ApproveExecution,
}

impl Action {
    /// Minimum role required for this action.
    fn required_role(&self) -> UserRole {
        match self {
            Action::ChatWithAgent => UserRole::User,
            Action::ViewConfig => UserRole::User,
            Action::ViewUsage => UserRole::Admin,
            Action::SpawnAgent => UserRole::Admin,
            Action::KillAgent => UserRole::Admin,
            Action::InstallSkill => UserRole::Admin,
            Action::ModifyConfig => UserRole::Owner,
            Action::ManageUsers => UserRole::Owner,
            // Approving a dangerous tool call is an operator action; Admin is
            // the floor. (#2a binding-owner gate — role-based proxy until
            // per-binding ownership lands as its own schema change.)
            Action::ApproveExecution => UserRole::Admin,
        }
    }
}

/// A resolved user identity.
#[derive(Debug, Clone)]
pub struct UserIdentity {
    /// OpenFang user ID.
    pub id: UserId,
    /// Display name.
    pub name: String,
    /// Role.
    pub role: UserRole,
}

/// RBAC authentication and authorization manager.
pub struct AuthManager {
    /// Known users by their OpenFang user ID.
    users: DashMap<UserId, UserIdentity>,
    /// Channel binding index: "channel_type:platform_id" → UserId.
    channel_index: DashMap<String, UserId>,
}

impl AuthManager {
    /// Create a new AuthManager from kernel user configuration.
    pub fn new(user_configs: &[UserConfig]) -> Self {
        let manager = Self {
            users: DashMap::new(),
            channel_index: DashMap::new(),
        };

        for config in user_configs {
            let user_id = UserId::new();
            let role = UserRole::from_str_role(&config.role);
            let identity = UserIdentity {
                id: user_id,
                name: config.name.clone(),
                role,
            };

            manager.users.insert(user_id, identity);

            // Index channel bindings
            for (channel_type, platform_id) in &config.channel_bindings {
                let key = format!("{channel_type}:{platform_id}");
                manager.channel_index.insert(key, user_id);
            }

            info!(
                user = %config.name,
                role = %role,
                bindings = config.channel_bindings.len(),
                "Registered user"
            );
        }

        manager
    }

    /// Identify a user from a channel identity.
    ///
    /// Returns the OpenFang UserId if a matching channel binding exists,
    /// or None for unrecognized users.
    pub fn identify(&self, channel_type: &str, platform_id: &str) -> Option<UserId> {
        let key = format!("{channel_type}:{platform_id}");
        self.channel_index.get(&key).map(|r| *r.value())
    }

    /// Get a user's identity by their UserId.
    pub fn get_user(&self, user_id: UserId) -> Option<UserIdentity> {
        self.users.get(&user_id).map(|r| r.value().clone())
    }

    /// Authorize a user for an action.
    ///
    /// Returns Ok(()) if the user has sufficient permissions, or AuthDenied error.
    pub fn authorize(&self, user_id: UserId, action: &Action) -> OpenFangResult<()> {
        let identity = self
            .users
            .get(&user_id)
            .ok_or_else(|| OpenFangError::AuthDenied("Unknown user".to_string()))?;

        let required = action.required_role();
        if identity.role >= required {
            Ok(())
        } else {
            Err(OpenFangError::AuthDenied(format!(
                "User '{}' (role: {}) lacks permission for {:?} (requires: {})",
                identity.name, identity.role, action, required
            )))
        }
    }

    /// Check if RBAC is configured (any users registered).
    pub fn is_enabled(&self) -> bool {
        !self.users.is_empty()
    }

    /// Get the count of registered users.
    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    /// List all registered users.
    pub fn list_users(&self) -> Vec<UserIdentity> {
        self.users.iter().map(|r| r.value().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_configs() -> Vec<UserConfig> {
        vec![
            UserConfig {
                name: "Alice".to_string(),
                role: "owner".to_string(),
                channel_bindings: {
                    let mut m = HashMap::new();
                    m.insert("telegram".to_string(), "123456".to_string());
                    m.insert("discord".to_string(), "987654".to_string());
                    m
                },
                api_key_hash: None,
            },
            UserConfig {
                name: "Guest".to_string(),
                role: "user".to_string(),
                channel_bindings: {
                    let mut m = HashMap::new();
                    m.insert("telegram".to_string(), "999999".to_string());
                    m
                },
                api_key_hash: None,
            },
            UserConfig {
                name: "ReadOnly".to_string(),
                role: "viewer".to_string(),
                channel_bindings: HashMap::new(),
                api_key_hash: None,
            },
        ]
    }

    #[test]
    fn test_user_registration() {
        let manager = AuthManager::new(&test_configs());
        assert!(manager.is_enabled());
        assert_eq!(manager.user_count(), 3);
    }

    #[test]
    fn test_identify_from_channel() {
        let manager = AuthManager::new(&test_configs());

        // Alice on Telegram
        let owner_tg = manager.identify("telegram", "123456");
        assert!(owner_tg.is_some());

        // Alice on Discord
        let owner_dc = manager.identify("discord", "987654");
        assert!(owner_dc.is_some());

        // Same user across channels
        assert_eq!(owner_tg.unwrap(), owner_dc.unwrap());

        // Unknown user
        assert!(manager.identify("telegram", "unknown").is_none());
    }

    #[test]
    fn test_owner_can_do_everything() {
        let manager = AuthManager::new(&test_configs());
        let owner_id = manager.identify("telegram", "123456").unwrap();

        assert!(manager.authorize(owner_id, &Action::ChatWithAgent).is_ok());
        assert!(manager.authorize(owner_id, &Action::SpawnAgent).is_ok());
        assert!(manager.authorize(owner_id, &Action::KillAgent).is_ok());
        assert!(manager.authorize(owner_id, &Action::ManageUsers).is_ok());
        assert!(manager.authorize(owner_id, &Action::ModifyConfig).is_ok());
    }

    #[test]
    fn test_user_limited_access() {
        let manager = AuthManager::new(&test_configs());
        let guest_id = manager.identify("telegram", "999999").unwrap();

        // User can chat and view config
        assert!(manager.authorize(guest_id, &Action::ChatWithAgent).is_ok());
        assert!(manager.authorize(guest_id, &Action::ViewConfig).is_ok());

        // User cannot spawn/kill/manage
        assert!(manager.authorize(guest_id, &Action::SpawnAgent).is_err());
        assert!(manager.authorize(guest_id, &Action::KillAgent).is_err());
        assert!(manager.authorize(guest_id, &Action::ManageUsers).is_err());
    }

    #[test]
    fn test_viewer_read_only() {
        let manager = AuthManager::new(&test_configs());
        let users = manager.list_users();
        let viewer = users.iter().find(|u| u.name == "ReadOnly").unwrap();

        // Viewer cannot even chat
        assert!(manager
            .authorize(viewer.id, &Action::ChatWithAgent)
            .is_err());
    }

    #[test]
    fn test_unknown_user_denied() {
        let manager = AuthManager::new(&test_configs());
        let fake_id = UserId::new();
        assert!(manager.authorize(fake_id, &Action::ChatWithAgent).is_err());
    }

    /// Build a config with one user per role, all bound on discord.
    fn rbac_role_configs() -> Vec<UserConfig> {
        let bind = |id: &str| {
            let mut m = HashMap::new();
            m.insert("discord".to_string(), id.to_string());
            m
        };
        vec![
            UserConfig {
                name: "Owner".to_string(),
                role: "owner".to_string(),
                channel_bindings: bind("owner-1"),
                api_key_hash: None,
            },
            UserConfig {
                name: "Admin".to_string(),
                role: "admin".to_string(),
                channel_bindings: bind("admin-1"),
                api_key_hash: None,
            },
            UserConfig {
                name: "Member".to_string(),
                role: "user".to_string(),
                channel_bindings: bind("user-1"),
                api_key_hash: None,
            },
            UserConfig {
                name: "Viewer".to_string(),
                role: "viewer".to_string(),
                channel_bindings: bind("viewer-1"),
                api_key_hash: None,
            },
        ]
    }

    // #2a (ANAI-81) M3: the ApproveExecution gate authorizes approval
    // resolution at an Admin floor. Recognized Admin/Owner resolve; recognized
    // below-floor roles are denied.
    #[test]
    fn test_approve_execution_admin_floor() {
        let manager = AuthManager::new(&rbac_role_configs());

        let owner = manager.identify("discord", "owner-1").unwrap();
        let admin = manager.identify("discord", "admin-1").unwrap();
        assert!(manager.authorize(owner, &Action::ApproveExecution).is_ok());
        assert!(manager.authorize(admin, &Action::ApproveExecution).is_ok());

        let member = manager.identify("discord", "user-1").unwrap();
        let viewer = manager.identify("discord", "viewer-1").unwrap();
        assert!(manager
            .authorize(member, &Action::ApproveExecution)
            .is_err());
        assert!(manager
            .authorize(viewer, &Action::ApproveExecution)
            .is_err());
    }

    // #2a (ANAI-81) M3: unrecognized channel identity yields no UserId (the
    // resolve path rejects before authorize), and an unknown UserId is denied.
    #[test]
    fn test_approve_execution_unrecognized_denied() {
        let manager = AuthManager::new(&rbac_role_configs());

        assert!(manager.identify("discord", "stranger").is_none());

        let fake = UserId::new();
        assert!(manager.authorize(fake, &Action::ApproveExecution).is_err());
    }

    #[test]
    fn test_no_users_means_disabled() {
        let manager = AuthManager::new(&[]);
        assert!(!manager.is_enabled());
        assert_eq!(manager.user_count(), 0);
    }

    #[test]
    fn test_role_parsing() {
        assert_eq!(UserRole::from_str_role("owner"), UserRole::Owner);
        assert_eq!(UserRole::from_str_role("admin"), UserRole::Admin);
        assert_eq!(UserRole::from_str_role("viewer"), UserRole::Viewer);
        assert_eq!(UserRole::from_str_role("user"), UserRole::User);
        assert_eq!(UserRole::from_str_role("OWNER"), UserRole::Owner);
        assert_eq!(UserRole::from_str_role("unknown"), UserRole::User);
    }
}
