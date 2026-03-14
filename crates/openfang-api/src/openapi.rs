//! OpenAPI 3.1 specification for the OpenFang API.
//!
//! The canonical spec is exposed at `GET /api-doc/openapi.json`.
//! TypeScript types are generated from it via `openapi-typescript`.
//!
//! # Usage
//!
//! ```
//! cargo xtask openapi-gen > openapi.json
//! ```
//!
//! Alternatively, start the daemon and fetch the live spec:
//!
//! ```
//! curl http://127.0.0.1:50051/api-doc/openapi.json > openapi.json
//! ```

use crate::types::{
    AgentSummary, AgentUpdateRequest, AttachmentRef, BudgetSnapshot, ClawHubInstallRequest,
    HealthResponse, MessageRequest, MessageResponse, MigrateRequest, MigrateScanRequest,
    SetModeRequest, SkillInstallRequest, SkillUninstallRequest, SpawnRequest, SpawnResponse,
};
use openfang_types::agent::AgentMode;
use utoipa::OpenApi;

/// The canonical OpenAPI document for the OpenFang HTTP API.
///
/// Registers all request/response schemas so that `openapi-typescript`
/// can generate a fully-typed TypeScript client with zero manual work.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "OpenFang API",
        version = "0.3.35",
        description = "OpenFang Agent OS — HTTP/WebSocket REST API.\n\nAll authenticated endpoints require `X-API-Key: <token>` header when an API key is configured.",
        license(name = "Apache-2.0 OR MIT"),
        contact(
            name = "OpenFang project",
            url = "https://github.com/RightNow-AI/openfang"
        )
    ),
    servers(
        (url = "http://127.0.0.1:50051", description = "Local daemon (default)"),
    ),
    components(schemas(
        // Core request/response types
        SpawnRequest,
        SpawnResponse,
        AttachmentRef,
        MessageRequest,
        MessageResponse,
        AgentUpdateRequest,
        SetModeRequest,
        SkillInstallRequest,
        SkillUninstallRequest,
        MigrateRequest,
        MigrateScanRequest,
        ClawHubInstallRequest,
        // New strongly-typed response schemas
        AgentSummary,
        HealthResponse,
        BudgetSnapshot,
        // Enum type
        AgentMode,
    )),
    tags(
        (name = "agents", description = "Agent lifecycle management"),
        (name = "budget", description = "Cost metering and budget controls"),
        (name = "health", description = "Health and diagnostics"),
        (name = "skills", description = "Skill marketplace"),
    )
)]
pub struct ApiDoc;
