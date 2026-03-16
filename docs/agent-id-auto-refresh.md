# Agent ID Auto-refresh for Channel Defaults

## Problem

Channel bridges resolve `default_agent` names to concrete `AgentId` values when
they start. That cached UUID can become stale after a reconcile cycle, a daemon
restart, or a hand re-activation recreates the agent under the same name.

When that happens, channel traffic can still be routed to the old UUID and the
bridge starts returning `Agent not found` even though the named agent exists.

## Implementation

The fix keeps channel defaults attached to agent names and refreshes the cached
UUID whenever a matching agent is spawned again.

### Router state

`AgentRouter` already stores both:

- the resolved `channel -> AgentId` mapping used for fast routing
- the original `channel -> agent_name` mapping used for re-resolution

The router exposes `refresh_channel_defaults_for_agent()` so callers can update
every matching channel default in one pass and log exactly which routes moved.

### Bridge startup

`start_channel_bridge_with_config()` now does three things during bootstrap:

1. stores both the agent name and the resolved UUID with
   `set_channel_default_with_name()`
2. registers every currently known agent in the router name cache before
   loading bindings and broadcast routes
3. starts a background listener, tied to bridge shutdown, that subscribes to
   the kernel event bus

When the listener receives `LifecycleEvent::Spawned`, it:

1. refreshes the router's name cache with the new `AgentId`
2. updates any channel default whose configured agent name matches the respawned
   agent
3. logs the affected channel keys for diagnostics

### Kernel event publishing

The listener depends on spawn lifecycle events actually reaching the event bus.
`OpenFangKernel::spawn_agent_with_parent()` now publishes the spawned event
immediately after trigger evaluation, so synchronous spawn paths notify bridge
subscribers without requiring an async handoff.

### Hot reload cleanup

`reload_channels_from_disk()` now clears `kernel.channel_adapters` after the
old bridge is stopped. This prevents stale adapters from surviving a channel
hot reload.

## Operator impact

- No config migration is required.
- Existing `default_agent = "name"` settings keep working.
- Channel defaults recover automatically after a hand or agent is recreated
  under the same name.
- The bridge still shuts down cleanly because the lifecycle listener exits when
  the bridge shutdown signal is triggered.

## Validation

The change is covered at three levels:

- router tests verify that matching channel defaults are refreshed together
- kernel tests verify that `spawn_agent()` publishes `LifecycleEvent::Spawned`
- live verification should confirm the bridge logs
  `Updated channel default agent ID after respawn` after an agent is recreated

Recommended live check:

1. start the daemon with a channel config that sets `default_agent`
2. note the agent's current UUID
3. recreate that hand or agent
4. confirm the UUID changes
5. inspect the daemon log for the channel-default refresh message

## Related files

- `crates/openfang-api/src/channel_bridge.rs`
- `crates/openfang-channels/src/router.rs`
- `crates/openfang-kernel/src/kernel.rs`
