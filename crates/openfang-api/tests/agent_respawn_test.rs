//! Test that channel router auto-updates agent IDs when agents are respawned.

use openfang_channels::router::AgentRouter;
use openfang_channels::types::ChannelType;
use openfang_types::agent::AgentId;

#[test]
fn test_agent_respawn_updates_router() {
    // Create a router and register an agent as Telegram default
    let router = AgentRouter::new();
    let agent_name = "test-telegram-agent".to_string();
    let old_agent_id = AgentId::new();

    router.set_channel_default_with_name("Telegram".to_string(), old_agent_id, agent_name.clone());
    router.register_agent(agent_name.clone(), old_agent_id);

    // Verify initial state - use resolve() to check routing
    let resolved = router.resolve(&ChannelType::Telegram, "test_user", None);
    assert_eq!(resolved, Some(old_agent_id));

    // Simulate agent respawn
    let new_agent_id = AgentId::new();

    // Manually update router (simulating what the background task does)
    router.register_agent(agent_name.clone(), new_agent_id);
    if let Some(expected_name) = router.channel_default_name("Telegram") {
        if expected_name == agent_name {
            router.update_channel_default("Telegram", new_agent_id);
        }
    }

    // Verify router was updated - use resolve() to check routing
    let resolved_after = router.resolve(&ChannelType::Telegram, "test_user", None);
    assert_eq!(resolved_after, Some(new_agent_id));
    assert_ne!(resolved_after, Some(old_agent_id));
}

#[test]
fn test_agent_respawn_only_updates_matching_channel() {
    let router = AgentRouter::new();

    // Register two different agents for two channels
    let telegram_agent = "telegram-agent".to_string();
    let discord_agent = "discord-agent".to_string();
    let telegram_id_old = AgentId::new();
    let discord_id = AgentId::new();

    router.set_channel_default_with_name(
        "Telegram".to_string(),
        telegram_id_old,
        telegram_agent.clone(),
    );
    router.set_channel_default_with_name("Discord".to_string(), discord_id, discord_agent.clone());

    // Respawn only the Telegram agent
    let telegram_id_new = AgentId::new();
    router.register_agent(telegram_agent.clone(), telegram_id_new);
    if let Some(expected_name) = router.channel_default_name("Telegram") {
        if expected_name == telegram_agent {
            router.update_channel_default("Telegram", telegram_id_new);
        }
    }

    // Verify Telegram was updated
    let telegram_resolved = router.resolve(&ChannelType::Telegram, "test_user", None);
    assert_eq!(telegram_resolved, Some(telegram_id_new));

    // Verify Discord was NOT updated
    let discord_resolved = router.resolve(&ChannelType::Discord, "test_user", None);
    assert_eq!(discord_resolved, Some(discord_id));
}
