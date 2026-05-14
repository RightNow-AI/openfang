//! Capability-based security types.
//!
//! OpenFang uses capability-based security: an agent can only perform actions
//! that it has been explicitly granted permission to do. Capabilities are
//! immutable after agent creation and enforced at the kernel level.

use serde::{Deserialize, Serialize};

/// A specific permission granted to an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Capability {
    // -- File system --
    /// Read files matching the given glob pattern.
    FileRead(String),
    /// Write files matching the given glob pattern.
    FileWrite(String),

    // -- Network --
    /// Connect to hosts matching the pattern (e.g., "api.openai.com:443").
    NetConnect(String),
    /// Listen on a specific port.
    NetListen(u16),

    // -- Tools --
    /// Invoke a specific tool by ID.
    ToolInvoke(String),
    /// Invoke any tool (dangerous, requires explicit grant).
    ToolAll,
    /// Subscribe to MCP resource updates matching the given server/URI pattern.
    McpSubscribe(String),

    // -- LLM --
    /// Query models matching the pattern.
    LlmQuery(String),
    /// Maximum token budget.
    LlmMaxTokens(u64),

    // -- Agent interaction --
    /// Can spawn sub-agents.
    AgentSpawn,
    /// Can send messages to agents matching the pattern.
    AgentMessage(String),
    /// Can kill agents matching the pattern (or "*" for any).
    AgentKill(String),

    // -- Memory --
    /// Read from memory scopes matching the pattern.
    MemoryRead(String),
    /// Write to memory scopes matching the pattern.
    MemoryWrite(String),

    // -- Shell --
    /// Execute shell commands matching the pattern.
    ShellExec(String),
    /// Read environment variables matching the pattern.
    EnvRead(String),

    // -- OFP (OpenFang Wire Protocol) --
    /// Can discover remote agents.
    OfpDiscover,
    /// Can connect to remote peers matching the pattern.
    OfpConnect(String),
    /// Can advertise services on the network.
    OfpAdvertise,

    // -- Economic --
    /// Can spend up to the given amount in USD.
    EconSpend(f64),
    /// Can accept incoming payments.
    EconEarn,
    /// Can transfer funds to agents matching the pattern.
    EconTransfer(String),
}

/// Result of a capability check.
#[derive(Debug, Clone)]
pub enum CapabilityCheck {
    /// The capability is granted.
    Granted,
    /// The capability is denied with a reason.
    Denied(String),
}

impl CapabilityCheck {
    /// Returns true if the capability is granted.
    pub fn is_granted(&self) -> bool {
        matches!(self, Self::Granted)
    }

    /// Returns an error if denied, Ok(()) if granted.
    pub fn require(&self) -> Result<(), crate::error::OpenFangError> {
        match self {
            Self::Granted => Ok(()),
            Self::Denied(reason) => Err(crate::error::OpenFangError::CapabilityDenied(
                reason.clone(),
            )),
        }
    }
}

/// Checks whether a required capability matches any granted capability.
///
/// Pattern matching rules:
/// - Exact match: "api.openai.com:443" matches "api.openai.com:443"
/// - Wildcard: "*" matches anything
/// - Glob: "*.openai.com:443" matches "api.openai.com:443"
pub fn capability_matches(granted: &Capability, required: &Capability) -> bool {
    match (granted, required) {
        // ToolAll grants any ToolInvoke
        (Capability::ToolAll, Capability::ToolInvoke(_)) => true,

        // Same variant, check pattern matching
        (Capability::FileRead(pattern), Capability::FileRead(path)) => glob_matches(pattern, path),
        (Capability::FileWrite(pattern), Capability::FileWrite(path)) => {
            glob_matches(pattern, path)
        }
        (Capability::NetConnect(pattern), Capability::NetConnect(host)) => {
            glob_matches(pattern, host)
        }
        (Capability::ToolInvoke(granted_id), Capability::ToolInvoke(required_id)) => {
            granted_id == required_id || granted_id == "*"
        }
        (Capability::McpSubscribe(pattern), Capability::McpSubscribe(resource)) => {
            glob_matches(pattern, resource)
        }
        (Capability::LlmQuery(pattern), Capability::LlmQuery(model)) => {
            glob_matches(pattern, model)
        }
        (Capability::AgentMessage(pattern), Capability::AgentMessage(target)) => {
            glob_matches(pattern, target)
        }
        (Capability::AgentKill(pattern), Capability::AgentKill(target)) => {
            glob_matches(pattern, target)
        }
        (Capability::MemoryRead(pattern), Capability::MemoryRead(scope)) => {
            glob_matches(pattern, scope)
        }
        (Capability::MemoryWrite(pattern), Capability::MemoryWrite(scope)) => {
            glob_matches(pattern, scope)
        }
        (Capability::ShellExec(pattern), Capability::ShellExec(cmd)) => glob_matches(pattern, cmd),
        (Capability::EnvRead(pattern), Capability::EnvRead(var)) => glob_matches(pattern, var),
        (Capability::OfpConnect(pattern), Capability::OfpConnect(peer)) => {
            glob_matches(pattern, peer)
        }
        (Capability::EconTransfer(pattern), Capability::EconTransfer(target)) => {
            glob_matches(pattern, target)
        }

        // Simple boolean capabilities
        (Capability::AgentSpawn, Capability::AgentSpawn) => true,
        (Capability::OfpDiscover, Capability::OfpDiscover) => true,
        (Capability::OfpAdvertise, Capability::OfpAdvertise) => true,
        (Capability::EconEarn, Capability::EconEarn) => true,

        // Numeric capabilities
        (Capability::NetListen(granted_port), Capability::NetListen(required_port)) => {
            granted_port == required_port
        }
        (Capability::LlmMaxTokens(granted_max), Capability::LlmMaxTokens(required_max)) => {
            granted_max >= required_max
        }
        (Capability::EconSpend(granted_max), Capability::EconSpend(required_amount)) => {
            granted_max >= required_amount
        }

        // Different variants never match
        _ => false,
    }
}

/// Validate that child capabilities are a subset of parent capabilities.
/// This prevents privilege escalation: a restricted parent cannot create
/// an unrestricted child.
pub fn validate_capability_inheritance(
    parent_caps: &[Capability],
    child_caps: &[Capability],
) -> Result<(), String> {
    for child_cap in child_caps {
        let is_covered = parent_caps
            .iter()
            .any(|parent_cap| capability_matches(parent_cap, child_cap));
        if !is_covered {
            return Err(format!(
                "Privilege escalation denied: child requests {:?} but parent does not have a matching grant",
                child_cap
            ));
        }
    }
    Ok(())
}

/// Simple glob pattern matching supporting '*' as wildcard.
fn glob_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern == value {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    // Check for middle wildcard: "prefix*suffix"
    if let Some(star_pos) = pattern.find('*') {
        let prefix = &pattern[..star_pos];
        let suffix = &pattern[star_pos + 1..];
        return value.starts_with(prefix)
            && value.ends_with(suffix)
            && value.len() >= prefix.len() + suffix.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(capability_matches(
            &Capability::NetConnect("api.openai.com:443".to_string()),
            &Capability::NetConnect("api.openai.com:443".to_string()),
        ));
    }

    #[test]
    fn test_wildcard_match() {
        assert!(capability_matches(
            &Capability::NetConnect("*.openai.com:443".to_string()),
            &Capability::NetConnect("api.openai.com:443".to_string()),
        ));
    }

    #[test]
    fn test_star_matches_all() {
        assert!(capability_matches(
            &Capability::AgentMessage("*".to_string()),
            &Capability::AgentMessage("any-agent".to_string()),
        ));
    }

    #[test]
    fn test_tool_all_grants_specific() {
        assert!(capability_matches(
            &Capability::ToolAll,
            &Capability::ToolInvoke("web_search".to_string()),
        ));
    }

    #[test]
    fn test_mcp_subscribe_matches_resource_pattern() {
        assert!(capability_matches(
            &Capability::McpSubscribe("github:*".to_string()),
            &Capability::McpSubscribe("github:repo://RightNow-AI/openfang".to_string()),
        ));
        assert!(!capability_matches(
            &Capability::McpSubscribe("gmail:*".to_string()),
            &Capability::McpSubscribe("github:repo://RightNow-AI/openfang".to_string()),
        ));
    }

    #[test]
    fn test_different_variants_dont_match() {
        assert!(!capability_matches(
            &Capability::FileRead("*".to_string()),
            &Capability::FileWrite("/tmp/test".to_string()),
        ));
    }

    #[test]
    fn test_numeric_capability_bounds() {
        assert!(capability_matches(
            &Capability::LlmMaxTokens(10000),
            &Capability::LlmMaxTokens(5000),
        ));
        assert!(!capability_matches(
            &Capability::LlmMaxTokens(1000),
            &Capability::LlmMaxTokens(5000),
        ));
    }

    #[test]
    fn test_capability_check_require() {
        assert!(CapabilityCheck::Granted.require().is_ok());
        assert!(CapabilityCheck::Denied("no".to_string()).require().is_err());
    }

    #[test]
    fn test_glob_matches_middle_wildcard() {
        assert!(glob_matches("api.*.com", "api.openai.com"));
        assert!(!glob_matches("api.*.com", "api.openai.org"));
    }

    #[test]
    fn test_agent_kill_capability() {
        assert!(capability_matches(
            &Capability::AgentKill("*".to_string()),
            &Capability::AgentKill("agent-123".to_string()),
        ));
        assert!(!capability_matches(
            &Capability::AgentKill("agent-1".to_string()),
            &Capability::AgentKill("agent-2".to_string()),
        ));
    }

    #[test]
    fn test_capability_inheritance_subset_ok() {
        let parent = vec![
            Capability::FileRead("*".to_string()),
            Capability::NetConnect("*.example.com:443".to_string()),
        ];
        let child = vec![
            Capability::FileRead("/data/*".to_string()),
            Capability::NetConnect("api.example.com:443".to_string()),
        ];
        assert!(validate_capability_inheritance(&parent, &child).is_ok());
    }

    #[test]
    fn test_capability_inheritance_escalation_denied() {
        let parent = vec![Capability::FileRead("/data/*".to_string())];
        let child = vec![
            Capability::FileRead("*".to_string()),
            Capability::ShellExec("*".to_string()),
        ];
        assert!(validate_capability_inheritance(&parent, &child).is_err());
    }
}
