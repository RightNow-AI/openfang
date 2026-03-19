/*
 * @Author             : Felix
 * @Email              : 307253927@qq.com
 * @Date               : 2026-03-09 14:47:41
 * @LastEditors        : Felix
 * @LastEditTime       : 2026-03-19 18:23:23
 */

use crate::routes::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use openfang_types::model_catalog::ModelCatalogEntry;
use openfang_types::model_catalog::ModelTier;
use std::sync::Arc;

pub async fn update_models(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Get the models array from the request body
    let models_array = match body.as_array() {
        Some(arr) => arr,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": "Request body must be an array of model objects"}),
                ),
            );
        }
    };

    // Get write access to the model catalog
    let mut catalog = state
        .kernel
        .model_catalog
        .write()
        .unwrap_or_else(|e| e.into_inner());

    // Get current unigpt models from catalog
    let current_unigpt_models: std::collections::HashMap<String, String> = catalog
        .list_models()
        .iter()
        .filter(|m| m.provider == "unigpt" && m.tier == ModelTier::Custom)
        .map(|m| (m.id.clone(), m.display_name.clone()))
        .collect();

    // Track models from request
    let mut request_model_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut added_count = 0;
    let mut updated_count = 0;

    // Step 1: Process models from request
    for model_data in models_array {
        // Extract model parameters
        let id = model_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if id.is_empty() {
            continue; // Skip models without id
        }

        let name = model_data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();

        let context_window = model_data
            .get("context_window")
            .and_then(|v| v.as_u64())
            .unwrap_or(128_000);

        let max_output_tokens = model_data
            .get("max_output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(8_192);

        // Add to request model IDs set
        request_model_ids.insert(id.clone());

        // Check if model exists in catalog
        if let Some(existing_name) = current_unigpt_models.get(&id) {
            // Model exists, check if name is the same
            if *existing_name == name {
                // Name is the same, do nothing
                continue;
            } else {
                // Name is different, need to update
                // First remove the existing model
                catalog.remove_custom_model(&id);
                // Then add the updated model
                let entry = ModelCatalogEntry {
                    id: id.clone(),
                    display_name: name,
                    provider: "unigpt".to_string(),
                    tier: ModelTier::Custom,
                    context_window,
                    max_output_tokens,
                    input_cost_per_m: 0.0,
                    output_cost_per_m: 0.0,
                    supports_tools: true,
                    supports_vision: false,
                    supports_streaming: true,
                    aliases: vec![],
                };
                if catalog.add_custom_model(entry) {
                    updated_count += 1;
                }
            }
        } else {
            // Model doesn't exist, add it
            let entry = ModelCatalogEntry {
                id: id.clone(),
                display_name: name,
                provider: "unigpt".to_string(),
                tier: ModelTier::Custom,
                context_window,
                max_output_tokens,
                input_cost_per_m: 0.0,
                output_cost_per_m: 0.0,
                supports_tools: true,
                supports_vision: false,
                supports_streaming: true,
                aliases: vec![],
            };
            if catalog.add_custom_model(entry) {
                added_count += 1;
            }
        }
    }

    // Step 2: Remove models that are in catalog but not in request
    let mut removed_count = 0;
    for (model_id, _) in current_unigpt_models {
        if !request_model_ids.contains(&model_id) && catalog.remove_custom_model(&model_id) {
            removed_count += 1;
        }
    }

    // Persist changes to disk
    let custom_path = state.kernel.config.home_dir.join("custom_models.json");
    if let Err(e) = catalog.save_custom_models(&custom_path) {
        tracing::warn!("Failed to persist custom models: {e}");
    }

    // Return success response
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "removed_count": removed_count,
            "added_count": added_count,
            "updated_count": updated_count
        })),
    )
}

pub async fn set_api_key(Json(body): Json<serde_json::Value>) -> impl IntoResponse {
    let api_key = body
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if api_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "message": "API key is required"
            })),
        );
    }
    std::env::set_var("UNIGPT_API_KEY", api_key);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "message": "API key set successfully"
        })),
    )
}

pub async fn get_default_model(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let default_model = state.kernel.config.default_model.clone();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "default_model": {
                "name": default_model.model,
                "provider": default_model.provider,
            }
        })),
    )
}
