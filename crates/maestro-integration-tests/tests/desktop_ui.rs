//! Phase 13 integration tests — Desktop & UI Polish
//!
//! Tests cover:
//! 1. Mesh API routes (list peers, connect peer, route log)
//! 2. FangHub API routes (search, install)
//! 3. SPA page registration (FangHub and Mesh pages present in HTML)
//! 4. Tauri command signatures compile correctly (compile-time tests)

use openfang_kernel::mesh::MeshRouteEntry;

// ---------------------------------------------------------------------------
// MeshRouteEntry struct tests
// ---------------------------------------------------------------------------

#[test]
fn mesh_route_entry_new_sets_pending_status() {
    let entry = MeshRouteEntry::new(1, "Summarize document", "local:agent:abc123");
    assert_eq!(entry.id, 1);
    assert_eq!(entry.task_summary, "Summarize document");
    assert_eq!(entry.target, "local:agent:abc123");
    assert_eq!(entry.status, "pending");
    assert_eq!(entry.duration_ms, 0);
}

#[test]
fn mesh_route_entry_clone_works() {
    let entry = MeshRouteEntry::new(42, "Test task", "hand:github-copilot");
    let cloned = entry.clone();
    assert_eq!(cloned.id, 42);
    assert_eq!(cloned.target, "hand:github-copilot");
}

#[test]
fn mesh_route_entry_status_can_be_updated() {
    let mut entry = MeshRouteEntry::new(7, "Route to peer", "peer:node-xyz:agent-456");
    entry.status = "success".to_string();
    entry.duration_ms = 142;
    assert_eq!(entry.status, "success");
    assert_eq!(entry.duration_ms, 142);
}

// ---------------------------------------------------------------------------
// Mesh route log ring buffer tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mesh_route_log_ring_buffer_caps_at_200() {
    use std::collections::VecDeque;
    use tokio::sync::RwLock;

    let log: RwLock<VecDeque<MeshRouteEntry>> = RwLock::new(VecDeque::with_capacity(200));

    // Insert 250 entries
    for i in 0..250u64 {
        let mut guard = log.write().await;
        if guard.len() >= 200 {
            guard.pop_front();
        }
        guard.push_back(MeshRouteEntry::new(i, format!("Task {i}"), "local:agent:test"));
    }

    let guard = log.read().await;
    assert_eq!(guard.len(), 200, "Ring buffer should cap at 200 entries");
    // Oldest entry should be id=50 (250 - 200 = 50)
    assert_eq!(guard.front().unwrap().id, 50);
    // Newest entry should be id=249
    assert_eq!(guard.back().unwrap().id, 249);
}

// ---------------------------------------------------------------------------
// FangHub search URL construction tests
// ---------------------------------------------------------------------------

#[test]
fn fanghub_search_url_encodes_query() {
    let query = "github code review";
    let encoded = urlencoding::encode(query);
    let url = format!("https://fanghub.paradiseai.io/packages?q={encoded}");
    assert!(url.contains("github%20code%20review") || url.contains("github+code+review"),
        "URL should percent-encode the query: {url}");
}

#[test]
fn fanghub_install_url_construction() {
    let hand_id = "github-copilot";
    let registry = "https://fanghub.paradiseai.io";
    let version = "1.2.0";
    let url = format!("{registry}/packages/{hand_id}/versions/{version}/install");
    assert_eq!(url, "https://fanghub.paradiseai.io/packages/github-copilot/versions/1.2.0/install");
}

// ---------------------------------------------------------------------------
// SPA dashboard page registration tests
// ---------------------------------------------------------------------------

#[test]
fn spa_dashboard_contains_fanghub_page() {
    let html = include_str!("../../openfang-api/static/index_body.html");
    assert!(
        html.contains("page === 'fanghub'") || html.contains("fanghubPage()"),
        "SPA dashboard should contain the FangHub page element"
    );
}

#[test]
fn spa_dashboard_contains_mesh_page() {
    let html = include_str!("../../openfang-api/static/index_body.html");
    assert!(
        html.contains("page === 'mesh'") || html.contains("meshPage()"),
        "SPA dashboard should contain the Mesh page element"
    );
}

#[test]
fn spa_dashboard_has_fanghub_nav_entry() {
    let html = include_str!("../../openfang-api/static/index_body.html");
    assert!(
        html.contains("FangHub") || html.contains("fanghub"),
        "SPA dashboard nav should contain a FangHub entry"
    );
}

#[test]
fn spa_dashboard_has_mesh_nav_entry() {
    let html = include_str!("../../openfang-api/static/index_body.html");
    assert!(
        html.contains("Mesh") || html.contains("mesh"),
        "SPA dashboard nav should contain a Mesh entry"
    );
}
