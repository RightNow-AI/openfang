//! Agent router — routes incoming channel messages to the correct agent.

use crate::types::ChannelType;
use dashmap::DashMap;
use openfang_types::agent::AgentId;
use openfang_types::config::{AgentBinding, BroadcastConfig, BroadcastStrategy};
use std::sync::Mutex;
use tracing::warn;

/// Context for evaluating binding match rules against incoming messages.
#[derive(Debug, Default)]
pub struct BindingContext {
    /// Channel type string (e.g., "telegram", "discord").
    pub channel: String,
    /// Account/bot ID within the channel.
    pub account_id: Option<String>,
    /// Peer/user ID (platform_user_id).
    pub peer_id: String,
    /// Guild/server ID.
    pub guild_id: Option<String>,
    /// User's roles.
    pub roles: Vec<String>,
}

/// Routes incoming messages to the correct agent.
///
/// Routing priority: bindings (most specific first) > direct routes > user defaults > system default.
pub struct AgentRouter {
    /// Default agent per user (keyed by openfang_user or platform_id).
    user_defaults: DashMap<String, AgentId>,
    /// Direct routes: (channel_type_key, platform_user_id) -> AgentId.
    direct_routes: DashMap<(String, String), AgentId>,
    /// System-wide default agent.
    default_agent: Option<AgentId>,
    /// Per-channel-type default agent (e.g., Telegram -> agent_a, Discord -> agent_b).
    channel_defaults: DashMap<String, AgentId>,
    /// Sorted bindings (most specific first). Uses Mutex for runtime updates via Arc.
    bindings: Mutex<Vec<(AgentBinding, String)>>,
    /// Broadcast configuration. Uses Mutex for runtime updates via Arc.
    broadcast: Mutex<BroadcastConfig>,
    /// Agent name -> AgentId cache for binding resolution.
    agent_name_cache: DashMap<String, AgentId>,
}

impl AgentRouter {
    /// Create a new router.
    pub fn new() -> Self {
        Self {
            user_defaults: DashMap::new(),
            direct_routes: DashMap::new(),
            default_agent: None,
            channel_defaults: DashMap::new(),
            bindings: Mutex::new(Vec::new()),
            broadcast: Mutex::new(BroadcastConfig::default()),
            agent_name_cache: DashMap::new(),
        }
    }

    /// Set the system-wide default agent.
    pub fn set_default(&mut self, agent_id: AgentId) {
        self.default_agent = Some(agent_id);
    }

    /// Set a per-channel-type default agent (e.g., "Telegram" -> agent_id).
    pub fn set_channel_default(&self, channel_key: String, agent_id: AgentId) {
        self.channel_defaults.insert(channel_key, agent_id);
    }

    /// Set a user's default agent.
    pub fn set_user_default(&self, user_key: String, agent_id: AgentId) {
        self.user_defaults.insert(user_key, agent_id);
    }

    /// Set a direct route for a specific (channel, user) pair.
    pub fn set_direct_route(
        &self,
        channel_key: String,
        platform_user_id: String,
        agent_id: AgentId,
    ) {
        self.direct_routes
            .insert((channel_key, platform_user_id), agent_id);
    }

    /// Load agent bindings from configuration. Sorts by specificity (most specific first).
    pub fn load_bindings(&self, bindings: &[AgentBinding]) {
        let mut sorted: Vec<(AgentBinding, String)> = bindings
            .iter()
            .map(|b| (b.clone(), b.agent.clone()))
            .collect();
        // Sort by specificity descending (most specific first)
        sorted.sort_by(|a, b| {
            b.0.match_rule
                .specificity()
                .cmp(&a.0.match_rule.specificity())
        });
        *self.bindings.lock().unwrap_or_else(|e| e.into_inner()) = sorted;
    }

    /// Load broadcast configuration.
    pub fn load_broadcast(&self, broadcast: BroadcastConfig) {
        *self.broadcast.lock().unwrap_or_else(|e| e.into_inner()) = broadcast;
    }

    /// Register an agent name -> ID mapping for binding resolution.
    pub fn register_agent(&self, name: String, id: AgentId) {
        self.agent_name_cache.insert(name, id);
    }

    /// Resolve which agent should handle a message.
    ///
    /// Priority: bindings > direct route > user default > system default.
    pub fn resolve(
        &self,
        channel_type: &ChannelType,
        platform_user_id: &str,
        user_key: Option<&str>,
    ) -> Option<AgentId> {
        let channel_key = format!("{channel_type:?}");

        // 0. Check bindings (most specific first)
        let ctx = BindingContext {
            channel: channel_type_to_str(channel_type).to_string(),
            account_id: None,
            peer_id: platform_user_id.to_string(),
            guild_id: None,
            roles: Vec::new(),
        };
        if let Some(agent_id) = self.resolve_binding(&ctx) {
            return Some(agent_id);
        }

        // 1. Check direct routes
        if let Some(agent) = self
            .direct_routes
            .get(&(channel_key.clone(), platform_user_id.to_string()))
        {
            return Some(*agent);
        }

        // 2. Check user defaults
        if let Some(key) = user_key {
            if let Some(agent) = self.user_defaults.get(key) {
                return Some(*agent);
            }
        }
        // Also check by platform_user_id
        if let Some(agent) = self.user_defaults.get(platform_user_id) {
            return Some(*agent);
        }

        // 3. Per-channel-type default
        if let Some(agent) = self.channel_defaults.get(&channel_key) {
            return Some(*agent);
        }

        // 4. System default
        self.default_agent
    }

    /// Resolve with full binding context (supports guild_id, roles, account_id).
    pub fn resolve_with_context(
        &self,
        channel_type: &ChannelType,
        platform_user_id: &str,
        user_key: Option<&str>,
        ctx: &BindingContext,
    ) -> Option<AgentId> {
        // 0. Check bindings first
        if let Some(agent_id) = self.resolve_binding(ctx) {
            return Some(agent_id);
        }
        // Fall back to standard resolution
        let channel_key = format!("{channel_type:?}");
        if let Some(agent) = self
            .direct_routes
            .get(&(channel_key.clone(), platform_user_id.to_string()))
        {
            return Some(*agent);
        }
        if let Some(key) = user_key {
            if let Some(agent) = self.user_defaults.get(key) {
                return Some(*agent);
            }
        }
        if let Some(agent) = self.user_defaults.get(platform_user_id) {
            return Some(*agent);
        }
        if let Some(agent) = self.channel_defaults.get(&channel_key) {
            return Some(*agent);
        }
        self.default_agent
    }

    /// Resolve broadcast: returns all agents that should receive a message for the given peer.
    pub fn resolve_broadcast(&self, peer_id: &str) -> Vec<(String, Option<AgentId>)> {
        let bc = self.broadcast.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(agent_names) = bc.routes.get(peer_id) {
            agent_names
                .iter()
                .map(|name| {
                    let id = self.agent_name_cache.get(name).map(|r| *r);
                    (name.clone(), id)
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get broadcast strategy.
    pub fn broadcast_strategy(&self) -> BroadcastStrategy {
        self.broadcast
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .strategy
    }

    /// Check if a peer has broadcast routing configured.
    pub fn has_broadcast(&self, peer_id: &str) -> bool {
        self.broadcast
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .routes
            .contains_key(peer_id)
    }

    /// Get current bindings (read-only).
    pub fn bindings(&self) -> Vec<AgentBinding> {
        self.bindings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(b, _)| b.clone())
            .collect()
    }

    /// Add a single binding at runtime.
    pub fn add_binding(&self, binding: AgentBinding) {
        let name = binding.agent.clone();
        let mut bindings = self.bindings.lock().unwrap_or_else(|e| e.into_inner());
        bindings.push((binding, name));
        // Re-sort by specificity
        bindings.sort_by(|a, b| {
            b.0.match_rule
                .specificity()
                .cmp(&a.0.match_rule.specificity())
        });
    }

    /// Remove a binding by index (original insertion order after sort).
    pub fn remove_binding(&self, index: usize) -> Option<AgentBinding> {
        let mut bindings = self.bindings.lock().unwrap_or_else(|e| e.into_inner());
        if index < bindings.len() {
            Some(bindings.remove(index).0)
        } else {
            None
        }
    }

    /// Evaluate bindings against a context, returning the first matching agent ID.
    fn resolve_binding(&self, ctx: &BindingContext) -> Option<AgentId> {
        let bindings = self.bindings.lock().unwrap_or_else(|e| e.into_inner());
        for (binding, _agent_name) in bindings.iter() {
            if self.binding_matches(binding, ctx) {
                // Look up agent by name in cache
                if let Some(id) = self.agent_name_cache.get(&binding.agent) {
                    return Some(*id);
                }
                warn!(
                    agent = %binding.agent,
                    "Binding matched but agent not found in cache"
                );
            }
        }
        None
    }

    /// Check if a single binding's match_rule matches the context.
    fn binding_matches(&self, binding: &AgentBinding, ctx: &BindingContext) -> bool {
        let rule = &binding.match_rule;

        // All specified fields must match
        if let Some(ref ch) = rule.channel {
            if ch != &ctx.channel {
                return false;
            }
        }
        if let Some(ref acc) = rule.account_id {
            if ctx.account_id.as_ref() != Some(acc) {
                return false;
            }
        }
        if let Some(ref pid) = rule.peer_id {
            if pid != &ctx.peer_id {
                return false;
            }
        }
        if let Some(ref gid) = rule.guild_id {
            if ctx.guild_id.as_ref() != Some(gid) {
                return false;
            }
        }
        if !rule.roles.is_empty() {
            // User must have at least one of the specified roles
            let has_role = rule.roles.iter().any(|r| ctx.roles.contains(r));
            if !has_role {
                return false;
            }
        }
        true
    }
}

/// Convert ChannelType to lowercase string for binding matching.
fn channel_type_to_str(ct: &ChannelType) -> &str {
    match ct {
        ChannelType::Telegram => "telegram",
        ChannelType::Discord => "discord",
        ChannelType::Slack => "slack",
        ChannelType::WhatsApp => "whatsapp",
        ChannelType::Signal => "signal",
        ChannelType::Matrix => "matrix",
        ChannelType::Email => "email",
        ChannelType::Teams => "teams",
        ChannelType::Mattermost => "mattermost",
        ChannelType::WebChat => "webchat",
        ChannelType::CLI => "cli",
        ChannelType::Custom(s) => s.as_str(),
    }
}

impl Default for AgentRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_priority() {
        let mut router = AgentRouter::new();
        let default_agent = AgentId::new();
        let user_agent = AgentId::new();
        let direct_agent = AgentId::new();

        router.set_default(default_agent);
        router.set_user_default("alice".to_string(), user_agent);
        router.set_direct_route("Telegram".to_string(), "tg_123".to_string(), direct_agent);

        // Direct route wins
        let resolved = router.resolve(&ChannelType::Telegram, "tg_123", Some("alice"));
        assert_eq!(resolved, Some(direct_agent));

        // User default for non-direct-routed user
        let resolved = router.resolve(&ChannelType::WhatsApp, "wa_456", Some("alice"));
        assert_eq!(resolved, Some(user_agent));

        // System default for unknown user
        let resolved = router.resolve(&ChannelType::Discord, "dc_789", None);
        assert_eq!(resolved, Some(default_agent));
    }

    #[test]
    fn test_no_route() {
        let router = AgentRouter::new();
        let resolved = router.resolve(&ChannelType::CLI, "local", None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_binding_channel_match() {
        let router = AgentRouter::new();
        let agent_id = AgentId::new();
        router.register_agent("coder".to_string(), agent_id);
        router.load_bindings(&[AgentBinding {
            agent: "coder".to_string(),
            match_rule: openfang_types::config::BindingMatchRule {
                channel: Some("telegram".to_string()),
                ..Default::default()
            },
        }]);

        // Should match telegram
        let resolved = router.resolve(&ChannelType::Telegram, "user1", None);
        assert_eq!(resolved, Some(agent_id));

        // Should NOT match discord
        let resolved = router.resolve(&ChannelType::Discord, "user1", None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_binding_peer_id_match() {
        let router = AgentRouter::new();
        let agent_id = AgentId::new();
        router.register_agent("support".to_string(), agent_id);
        router.load_bindings(&[AgentBinding {
            agent: "support".to_string(),
            match_rule: openfang_types::config::BindingMatchRule {
                peer_id: Some("vip_user".to_string()),
                ..Default::default()
            },
        }]);

        let resolved = router.resolve(&ChannelType::Discord, "vip_user", None);
        assert_eq!(resolved, Some(agent_id));

        let resolved = router.resolve(&ChannelType::Discord, "other_user", None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_binding_guild_and_role_match() {
        let router = AgentRouter::new();
        let agent_id = AgentId::new();
        router.register_agent("admin-bot".to_string(), agent_id);
        router.load_bindings(&[AgentBinding {
            agent: "admin-bot".to_string(),
            match_rule: openfang_types::config::BindingMatchRule {
                guild_id: Some("guild_123".to_string()),
                roles: vec!["admin".to_string()],
                ..Default::default()
            },
        }]);

        let ctx = BindingContext {
            channel: "discord".to_string(),
            peer_id: "user1".to_string(),
            guild_id: Some("guild_123".to_string()),
            roles: vec!["admin".to_string(), "user".to_string()],
            ..Default::default()
        };
        let resolved = router.resolve_with_context(&ChannelType::Discord, "user1", None, &ctx);
        assert_eq!(resolved, Some(agent_id));

        // Wrong guild
        let ctx2 = BindingContext {
            channel: "discord".to_string(),
            peer_id: "user1".to_string(),
            guild_id: Some("guild_999".to_string()),
            roles: vec!["admin".to_string()],
            ..Default::default()
        };
        let resolved = router.resolve_with_context(&ChannelType::Discord, "user1", None, &ctx2);
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_binding_specificity_ordering() {
        let router = AgentRouter::new();
        let general_id = AgentId::new();
        let specific_id = AgentId::new();
        router.register_agent("general".to_string(), general_id);
        router.register_agent("specific".to_string(), specific_id);

        // Load in wrong order — less specific first
        router.load_bindings(&[
            AgentBinding {
                agent: "general".to_string(),
                match_rule: openfang_types::config::BindingMatchRule {
                    channel: Some("discord".to_string()),
                    ..Default::default()
                },
            },
            AgentBinding {
                agent: "specific".to_string(),
                match_rule: openfang_types::config::BindingMatchRule {
                    channel: Some("discord".to_string()),
                    peer_id: Some("user1".to_string()),
                    guild_id: Some("guild_1".to_string()),
                    ..Default::default()
                },
            },
        ]);

        // More specific binding should win despite being loaded second
        let ctx = BindingContext {
            channel: "discord".to_string(),
            peer_id: "user1".to_string(),
            guild_id: Some("guild_1".to_string()),
            ..Default::default()
        };
        let resolved = router.resolve_with_context(&ChannelType::Discord, "user1", None, &ctx);
        assert_eq!(resolved, Some(specific_id));
    }

    #[test]
    fn test_broadcast_routing() {
        let router = AgentRouter::new();
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        router.register_agent("agent-a".to_string(), id1);
        router.register_agent("agent-b".to_string(), id2);

        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "vip_user".to_string(),
            vec!["agent-a".to_string(), "agent-b".to_string()],
        );
        router.load_broadcast(BroadcastConfig {
            strategy: BroadcastStrategy::Parallel,
            routes,
        });

        assert!(router.has_broadcast("vip_user"));
        assert!(!router.has_broadcast("normal_user"));

        let targets = router.resolve_broadcast("vip_user");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0, "agent-a");
        assert_eq!(targets[0].1, Some(id1));
        assert_eq!(targets[1].0, "agent-b");
        assert_eq!(targets[1].1, Some(id2));
    }

    #[test]
    fn test_channel_default_routing() {
        let mut router = AgentRouter::new();
        let system_default = AgentId::new();
        let telegram_default = AgentId::new();
        let discord_default = AgentId::new();

        router.set_default(system_default);
        router.set_channel_default("Telegram".to_string(), telegram_default);
        router.set_channel_default("Discord".to_string(), discord_default);

        // Telegram should use Telegram-specific default
        let resolved = router.resolve(&ChannelType::Telegram, "user1", None);
        assert_eq!(resolved, Some(telegram_default));

        // Discord should use Discord-specific default
        let resolved = router.resolve(&ChannelType::Discord, "user1", None);
        assert_eq!(resolved, Some(discord_default));

        // WhatsApp has no channel default — falls to system default
        let resolved = router.resolve(&ChannelType::WhatsApp, "user1", None);
        assert_eq!(resolved, Some(system_default));
    }

    #[test]
    fn test_empty_bindings_legacy_behavior() {
        let mut router = AgentRouter::new();
        let default_id = AgentId::new();
        router.set_default(default_id);
        router.load_bindings(&[]);

        // Should fall through to system default
        let resolved = router.resolve(&ChannelType::Telegram, "user1", None);
        assert_eq!(resolved, Some(default_id));
    }

    #[test]
    fn test_binding_nonexistent_agent_warning() {
        let router = AgentRouter::new();
        // Don't register the agent — binding should match but resolve_binding returns None
        router.load_bindings(&[AgentBinding {
            agent: "ghost-agent".to_string(),
            match_rule: openfang_types::config::BindingMatchRule {
                channel: Some("telegram".to_string()),
                ..Default::default()
            },
        }]);

        let resolved = router.resolve(&ChannelType::Telegram, "user1", None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_add_remove_binding() {
        let router = AgentRouter::new();
        let id = AgentId::new();
        router.register_agent("test".to_string(), id);

        assert!(router.bindings().is_empty());

        router.add_binding(AgentBinding {
            agent: "test".to_string(),
            match_rule: openfang_types::config::BindingMatchRule {
                channel: Some("slack".to_string()),
                ..Default::default()
            },
        });
        assert_eq!(router.bindings().len(), 1);

        let removed = router.remove_binding(0);
        assert!(removed.is_some());
        assert!(router.bindings().is_empty());
    }

    #[test]
    fn test_binding_specificity_scores() {
        use openfang_types::config::BindingMatchRule;

        let empty = BindingMatchRule::default();
        assert_eq!(empty.specificity(), 0);

        let channel_only = BindingMatchRule {
            channel: Some("discord".to_string()),
            ..Default::default()
        };
        assert_eq!(channel_only.specificity(), 1);

        let full = BindingMatchRule {
            channel: Some("discord".to_string()),
            peer_id: Some("user".to_string()),
            guild_id: Some("guild".to_string()),
            roles: vec!["admin".to_string()],
            account_id: Some("bot".to_string()),
        };
        assert_eq!(full.specificity(), 17); // 8+4+2+2+1
    }
}
