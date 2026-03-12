//! A2A Handler Registry for in-process agent communication.
//!
//! This module provides a direct handler registry for A2A messages,
//! bypassing the transport layer for efficient in-kernel communication.

use openfang_a2a::protocol::A2AMessage;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for A2A message handlers.
#[async_trait::async_trait]
pub trait A2AHandler: Send + Sync {
    /// Handle an incoming A2A message and optionally return a response.
    async fn handle_message(&self, message: A2AMessage) -> Option<A2AMessage>;
}

/// Registry mapping agent types to their handlers.
pub struct A2AHandlerRegistry {
    handlers: RwLock<HashMap<String, Arc<dyn A2AHandler>>>,
}

impl A2AHandlerRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a handler for an agent type.
    pub async fn register(&self, agent_type: &str, handler: Arc<dyn A2AHandler>) {
        let mut handlers = self.handlers.write().await;
        handlers.insert(agent_type.to_string(), handler);
    }

    /// Get a handler for an agent type.
    pub async fn get(&self, agent_type: &str) -> Option<Arc<dyn A2AHandler>> {
        let handlers = self.handlers.read().await;
        handlers.get(agent_type).cloned()
    }

    /// Check if a handler is registered for an agent type.
    pub async fn has_handler(&self, agent_type: &str) -> bool {
        let handlers = self.handlers.read().await;
        handlers.contains_key(agent_type)
    }

    /// List all registered agent types.
    pub async fn list_handlers(&self) -> Vec<String> {
        let handlers = self.handlers.read().await;
        handlers.keys().cloned().collect()
    }
}

impl Default for A2AHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
