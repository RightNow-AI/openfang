//! Plugin lifecycle hooks — intercept points at key moments in agent execution.
//!
//! Provides a callback-based hook system (not dynamic loading) for safe extensibility.
//! Four hook types:
//! - `BeforeToolCall`: Fires before tool execution. Can block the call by returning Err.
//! - `AfterToolCall`: Fires after tool execution. Observe-only.
//! - `BeforePromptBuild`: Fires before system prompt construction. Observe-only.
//! - `AgentLoopEnd`: Fires after the agent loop completes. Observe-only.

use dashmap::DashMap;
use omtae_types::agent::HookEvent;
use std::sync::Arc;

/// Context passed to hook handlers.
pub struct HookContext<'a> {
    /// Agent display name.
    pub agent_name: &'a str,
    /// Agent ID string.
    pub agent_id: &'a str,
    /// Which hook event triggered this call.
    pub event: HookEvent,
    /// Event-specific payload (tool name, input, result, etc.).
    pub data: serde_json::Value,
}

/// Hook handler trait. Implementations must be thread-safe.
pub trait HookHandler: Send + Sync {
    /// Called when the hook fires.
    ///
    /// For `BeforeToolCall`: returning `Err(reason)` blocks the tool call.
    /// For all other events: return value is ignored (observe-only).
    fn on_event(&self, ctx: &HookContext) -> Result<(), String>;
}

/// Registry of hook handlers, keyed by event type.
///
/// Thread-safe via `DashMap`. Handlers fire in registration order.
pub struct HookRegistry {
    handlers: DashMap<HookEvent, Vec<Arc<dyn HookHandler>>>,
}

impl HookRegistry {
    /// Create an empty hook registry.
    pub fn new() -> Self {
        Self {
            handlers: DashMap::new(),
        }
    }

    /// Register a handler for a specific event type.
    pub fn register(&self, event: HookEvent, handler: Arc<dyn HookHandler>) {
        self.handlers.entry(event).or_default().push(handler);
    }

    /// Fire all handlers for an event. Returns Err if any handler blocks.
    ///
    /// For `BeforeToolCall`, the first Err stops execution and returns the reason.
    /// For other events, errors are logged but don't propagate.
    pub fn fire(&self, ctx: &HookContext) -> Result<(), String> {
        if let Some(handlers) = self.handlers.get(&ctx.event) {
            for handler in handlers.iter() {
                if let Err(reason) = handler.on_event(ctx) {
                    if ctx.event == HookEvent::BeforeToolCall {
                        return Err(reason);
                    }
                    // For non-blocking hooks, log and continue
                    tracing::warn!(
                        event = ?ctx.event,
                        agent = ctx.agent_name,
                        error = %reason,
                        "Hook handler returned error (non-blocking)"
                    );
                }
            }
        }
        Ok(())
    }

    /// Check if any handlers are registered for a given event.
    pub fn has_handlers(&self, event: HookEvent) -> bool {
        self.handlers
            .get(&event)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test handler that always succeeds.
    struct OkHandler;
    impl HookHandler for OkHandler {
        fn on_event(&self, _ctx: &HookContext) -> Result<(), String> {
            Ok(())
        }
    }

    /// A test handler that always blocks.
    struct BlockHandler {
        reason: String,
    }
    impl HookHandler for BlockHandler {
        fn on_event(&self, _ctx: &HookContext) -> Result<(), String> {
            Err(self.reason.clone())
        }
    }

    /// A test handler that records calls.
    struct RecordHandler {
        calls: std::sync::Mutex<Vec<String>>,
    }
    impl RecordHandler {
        fn new() -> Self {
            Self {
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }
        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }
    impl HookHandler for RecordHandler {
        fn on_event(&self, ctx: &HookContext) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("{:?}", ctx.event));
            Ok(())
        }
    }

    fn make_ctx(event: HookEvent) -> HookContext<'static> {
        HookContext {
            agent_name: "test-agent",
            agent_id: "abc-123",
            event,
            data: serde_json::json!({}),
        }
    }

    #[test]
    fn test_empty_registry_is_noop() {
        let registry = HookRegistry::new();
        let ctx = make_ctx(HookEvent::BeforeToolCall);
        assert!(registry.fire(&ctx).is_ok());
    }

    #[test]
    fn test_before_tool_call_can_block() {
        let registry = HookRegistry::new();
        registry.register(
            HookEvent::BeforeToolCall,
            Arc::new(BlockHandler {
                reason: "Not allowed".to_string(),
            }),
        );
        let ctx = make_ctx(HookEvent::BeforeToolCall);
        let result = registry.fire(&ctx);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Not allowed");
    }

    #[test]
    fn test_after_tool_call_receives_result() {
        let recorder = Arc::new(RecordHandler::new());
        let registry = HookRegistry::new();
        registry.register(HookEvent::AfterToolCall, recorder.clone());

        let ctx = HookContext {
            agent_name: "test-agent",
            agent_id: "abc-123",
            event: HookEvent::AfterToolCall,
            data: serde_json::json!({"tool_name": "file_read", "result": "ok"}),
        };
        assert!(registry.fire(&ctx).is_ok());
        assert_eq!(recorder.call_count(), 1);
    }

    #[test]
    fn test_multiple_handlers_all_fire() {
        let r1 = Arc::new(RecordHandler::new());
        let r2 = Arc::new(RecordHandler::new());
        let registry = HookRegistry::new();
        registry.register(HookEvent::AgentLoopEnd, r1.clone());
        registry.register(HookEvent::AgentLoopEnd, r2.clone());

        let ctx = make_ctx(HookEvent::AgentLoopEnd);
        assert!(registry.fire(&ctx).is_ok());
        assert_eq!(r1.call_count(), 1);
        assert_eq!(r2.call_count(), 1);
    }

    #[test]
    fn test_hook_errors_dont_crash_non_blocking() {
        let registry = HookRegistry::new();
        // Register a blocking handler for a non-blocking event
        registry.register(
            HookEvent::AfterToolCall,
            Arc::new(BlockHandler {
                reason: "oops".to_string(),
            }),
        );
        let ctx = make_ctx(HookEvent::AfterToolCall);
        // AfterToolCall is non-blocking, so error should be swallowed
        assert!(registry.fire(&ctx).is_ok());
    }

    #[test]
    fn test_all_four_events_fire() {
        let recorder = Arc::new(RecordHandler::new());
        let registry = HookRegistry::new();
        registry.register(HookEvent::BeforeToolCall, recorder.clone());
        registry.register(HookEvent::AfterToolCall, recorder.clone());
        registry.register(HookEvent::BeforePromptBuild, recorder.clone());
        registry.register(HookEvent::AgentLoopEnd, recorder.clone());

        for event in [
            HookEvent::BeforeToolCall,
            HookEvent::AfterToolCall,
            HookEvent::BeforePromptBuild,
            HookEvent::AgentLoopEnd,
        ] {
            let ctx = make_ctx(event);
            let _ = registry.fire(&ctx);
        }
        assert_eq!(recorder.call_count(), 4);
    }

    #[test]
    fn test_has_handlers() {
        let registry = HookRegistry::new();
        assert!(!registry.has_handlers(HookEvent::BeforeToolCall));
        registry.register(HookEvent::BeforeToolCall, Arc::new(OkHandler));
        assert!(registry.has_handlers(HookEvent::BeforeToolCall));
        assert!(!registry.has_handlers(HookEvent::AfterToolCall));
    }
}
