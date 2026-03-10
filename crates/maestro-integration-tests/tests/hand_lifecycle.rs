//! Integration tests for the openfang-hands lifecycle management.

use openfang_hands::{
    registry::HandRegistry,
    scheduler::{HandScheduler, HandScheduleSpec},
    HandStatus,
};
use std::collections::HashMap;

#[tokio::test]
async fn test_registry_lists_all_bundled_hands() {
    let registry = HandRegistry::new();
    let loaded = registry.load_bundled();
    assert!(loaded >= 7, "load_bundled() should load at least 7 bundled Hands; loaded {loaded}");

    let hands = registry.list_definitions();
    assert!(hands.len() >= 7, "list_definitions() should return at least 7 Hands; found {}", hands.len());

    // list_definitions() returns HandDefinition structs; check by id (e.g. "clip"),
    // not by the human-readable display name (e.g. "Clip Hand").
    let ids: Vec<&str> = hands.iter().map(|h| h.id.as_str()).collect();
    assert!(ids.contains(&"clip"), "Should include the 'clip' Hand");
    assert!(ids.contains(&"lead"), "Should include the 'lead' Hand");
    assert!(ids.contains(&"researcher"), "Should include the 'researcher' Hand");
}

#[tokio::test]
async fn test_activate_and_deactivate_hand() {
    let registry = HandRegistry::new();
    registry.load_bundled();

    let instance = registry
        .activate("researcher", HashMap::new())
        .expect("Should be able to activate the 'researcher' Hand");

    let instances = registry.list_instances();
    assert!(
        instances.iter().any(|i| i.hand_id == "researcher"),
        "Researcher should appear in instances list after activation"
    );

    let instance_id = instance.instance_id;
    registry
        .deactivate(instance_id)
        .expect("Should be able to deactivate the 'researcher' Hand");

    let instances_after = registry.list_instances();
    assert!(
        !instances_after.iter().any(|i| i.hand_id == "researcher"),
        "Researcher should NOT appear in instances list after deactivation"
    );
}

#[tokio::test]
async fn test_pause_and_resume_hand() {
    let registry = HandRegistry::new();
    registry.load_bundled();

    let instance = registry.activate("clip", HashMap::new()).expect("activate clip");
    let instance_id = instance.instance_id;

    registry.pause(instance_id).expect("pause clip");
    let paused = registry.get_instance(instance_id).expect("get instance");
    assert!(
        matches!(paused.status, HandStatus::Paused),
        "Clip should be in Paused status after pause()"
    );

    registry.resume(instance_id).expect("resume clip");
    let resumed = registry.get_instance(instance_id).expect("get instance");
    assert!(
        matches!(resumed.status, HandStatus::Active),
        "Clip should be in Active status after resume()"
    );
}

#[tokio::test]
async fn test_activate_unknown_hand_returns_error() {
    let registry = HandRegistry::new();
    registry.load_bundled();
    let result = registry.activate("nonexistent-hand-xyz", HashMap::new());
    assert!(result.is_err(), "Activating an unknown Hand should return an error");
}

#[tokio::test]
async fn test_get_definition_returns_correct_hand() {
    let registry = HandRegistry::new();
    registry.load_bundled();
    let def = registry.get_definition("researcher");
    assert!(def.is_some(), "get_definition('researcher') should return Some");
    let def = def.unwrap();
    // The id is "researcher"; the display name is "Researcher Hand".
    assert_eq!(def.id, "researcher", "Definition id should be 'researcher'");
}

#[tokio::test]
async fn test_hand_scheduler_validates_cron_spec() {
    let spec = HandScheduleSpec::Cron("0 */6 * * *".to_string());
    let scheduler = HandScheduler::new();
    let result = scheduler.validate_spec(&spec);
    assert!(result.is_ok(), "Valid 5-field cron spec should validate without error");
}

#[tokio::test]
async fn test_hand_scheduler_validates_interval_spec() {
    let spec = HandScheduleSpec::Interval { seconds: 3600 };
    let scheduler = HandScheduler::new();
    let result = scheduler.validate_spec(&spec);
    assert!(result.is_ok(), "Valid interval spec should validate without error");
}

#[tokio::test]
async fn test_hand_scheduler_rejects_invalid_cron() {
    let spec = HandScheduleSpec::Cron("not-a-valid-cron".to_string());
    let scheduler = HandScheduler::new();
    let result = scheduler.validate_spec(&spec);
    assert!(result.is_err(), "Invalid cron spec should return an error");
}
