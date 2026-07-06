//! Auto-reply background engine — trigger-driven background replies with concurrency control.

use omtae_types::agent::AgentId;
use omtae_types::config::AutoReplyConfig;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Where to deliver the auto-reply result.
#[derive(Debug, Clone)]
pub struct AutoReplyChannel {
    /// Channel type string (e.g., "telegram", "discord").
    pub channel_type: String,
    /// Peer/user ID to send the reply to.
    pub peer_id: String,
    /// Optional thread ID for threaded replies.
    pub thread_id: Option<String>,
}

/// Auto-reply engine with concurrency limits and suppression patterns.
pub struct AutoReplyEngine {
    config: AutoReplyConfig,
    semaphore: Arc<Semaphore>,
}

impl AutoReplyEngine {
    /// Create a new auto-reply engine from configuration.
    pub fn new(config: AutoReplyConfig) -> Self {
        let permits = config.max_concurrent.max(1);
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            config,
        }
    }

    /// Check if a message should trigger auto-reply.
    /// Returns `None` if suppressed or disabled, `Some(agent_id)` if should auto-reply.
    pub fn should_reply(
        &self,
        message: &str,
        _channel_type: &str,
        agent_id: AgentId,
    ) -> Option<AgentId> {
        if !self.config.enabled {
            return None;
        }

        // Check suppression patterns
        let lower = message.to_lowercase();
        for pattern in &self.config.suppress_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                debug!(pattern = %pattern, "Auto-reply suppressed by pattern");
                return None;
            }
        }

        Some(agent_id)
    }

    /// Execute an auto-reply in the background.
    /// Returns a JoinHandle for the spawned task.
    ///
    /// The `send_fn` is called with the agent response to deliver it back to the channel.
    pub async fn execute_reply<F>(
        &self,
        kernel_handle: Arc<dyn omtae_runtime::kernel_handle::KernelHandle>,
        agent_id: AgentId,
        message: String,
        reply_channel: AutoReplyChannel,
        send_fn: F,
    ) -> Result<tokio::task::JoinHandle<()>, String>
    where
        F: Fn(String, AutoReplyChannel) -> futures::future::BoxFuture<'static, ()>
            + Send
            + Sync
            + 'static,
    {
        // Try to acquire a semaphore permit
        let permit = match self.semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                return Err(format!(
                    "Auto-reply concurrency limit reached ({} max)",
                    self.config.max_concurrent
                ));
            }
        };

        let timeout_secs = self.config.timeout_secs;

        let handle = tokio::spawn(async move {
            let _permit = permit; // Hold permit until task completes

            info!(
                agent = %agent_id,
                channel = %reply_channel.channel_type,
                peer = %reply_channel.peer_id,
                "Starting auto-reply"
            );

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                kernel_handle.send_to_agent(&agent_id.to_string(), &message, None, None),
            )
            .await;

            match result {
                Ok(Ok(response)) => {
                    send_fn(response, reply_channel).await;
                }
                Ok(Err(e)) => {
                    warn!(agent = %agent_id, error = %e, "Auto-reply agent error");
                }
                Err(_) => {
                    warn!(agent = %agent_id, timeout = timeout_secs, "Auto-reply timed out");
                }
            }
        });

        Ok(handle)
    }

    /// Check if auto-reply is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the current configuration (read-only).
    pub fn config(&self) -> &AutoReplyConfig {
        &self.config
    }

    /// Get available permits (for monitoring).
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(enabled: bool) -> AutoReplyConfig {
        AutoReplyConfig {
            enabled,
            max_concurrent: 3,
            timeout_secs: 120,
            suppress_patterns: vec!["/stop".to_string(), "/pause".to_string()],
        }
    }

    #[test]
    fn test_disabled_engine() {
        let engine = AutoReplyEngine::new(test_config(false));
        let agent_id = AgentId::new();
        assert!(engine.should_reply("hello", "telegram", agent_id).is_none());
    }

    #[test]
    fn test_enabled_engine_allows() {
        let engine = AutoReplyEngine::new(test_config(true));
        let agent_id = AgentId::new();
        let result = engine.should_reply("hello there", "telegram", agent_id);
        assert_eq!(result, Some(agent_id));
    }

    #[test]
    fn test_suppression_patterns() {
        let engine = AutoReplyEngine::new(test_config(true));
        let agent_id = AgentId::new();

        // Should be suppressed
        assert!(engine.should_reply("/stop", "telegram", agent_id).is_none());
        assert!(engine
            .should_reply("please /pause this", "telegram", agent_id)
            .is_none());

        // Not suppressed
        assert!(engine.should_reply("hello", "telegram", agent_id).is_some());
    }

    #[test]
    fn test_concurrency_limit() {
        let config = AutoReplyConfig {
            enabled: true,
            max_concurrent: 2,
            timeout_secs: 120,
            suppress_patterns: Vec::new(),
        };
        let engine = AutoReplyEngine::new(config);
        assert_eq!(engine.available_permits(), 2);
    }

    #[test]
    fn test_is_enabled() {
        let on = AutoReplyEngine::new(test_config(true));
        assert!(on.is_enabled());

        let off = AutoReplyEngine::new(test_config(false));
        assert!(!off.is_enabled());
    }

    #[test]
    fn test_default_config() {
        let config = AutoReplyConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_concurrent, 3);
        assert_eq!(config.timeout_secs, 120);
        assert!(config.suppress_patterns.contains(&"/stop".to_string()));
    }
}
