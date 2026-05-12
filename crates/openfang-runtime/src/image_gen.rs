//! Image generation — DALL-E 3, DALL-E 2, GPT-Image-1 via OpenAI API.

use base64::Engine;
use openfang_types::media::{GeneratedImage, ImageGenRequest, ImageGenResult};
use tracing::warn;

/// Generate images via OpenAI's image generation API.
///
/// Requires OPENAI_API_KEY to be set.
///
/// `base_url_override` (sourced from `MediaConfig.image_gen_base_url`) lets
/// callers redirect the request to a local OpenAI-compatible image service
/// (e.g. Lemonade/Flux, LM Studio). When `None`, the hardcoded
/// `https://api.openai.com/v1/images/generations` endpoint is used. Closes #1051.
pub async fn generate_image(
    request: &ImageGenRequest,
    base_url_override: Option<&str>,
) -> Result<ImageGenResult, String> {
    // Validate request
    request.validate()?;

    // Check for API key (presence only — never read the actual value into logs)
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY not set. Image generation requires an OpenAI API key.")?;

    let model_str = request.model.to_string();

    let mut body = serde_json::json!({
        "model": model_str,
        "prompt": request.prompt,
        "n": request.count,
        "size": request.size,
        "response_format": "b64_json",
    });

    // DALL-E 3 specific fields
    if request.model == openfang_types::media::ImageGenModel::DallE3 {
        body["quality"] = serde_json::json!(request.quality);
    }

    // `image_gen_base_url` (config.media.image_gen_base_url) overrides the
    // hardcoded provider URL when set, allowing the same OpenAI-compat JSON
    // wire format to be sent to a local image generation service
    // (Lemonade/Flux, LM Studio, etc.) instead of the cloud provider. The
    // Authorization header is still built from `OPENAI_API_KEY`; local
    // services typically accept any non-empty bearer token. Closes #1051.
    let url = base_url_override
        .map(|base| format!("{}/v1/images/generations", base.trim_end_matches('/')))
        .unwrap_or_else(|| "https://api.openai.com/v1/images/generations".to_string());

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| format!("Image generation API request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        // SECURITY: don't include full error body which might contain key info
        let truncated = crate::str_utils::safe_truncate_str(&error_body, 500);
        return Err(format!(
            "Image generation failed (HTTP {}): {}",
            status, truncated
        ));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse image generation response: {e}"))?;

    let mut images = Vec::new();
    let mut revised_prompt = None;

    if let Some(data) = result.get("data").and_then(|d| d.as_array()) {
        for item in data {
            let b64 = item
                .get("b64_json")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // SECURITY: bound image data size (max 10MB base64)
            if b64.len() > 10 * 1024 * 1024 {
                warn!("Generated image data exceeds 10MB, skipping");
                continue;
            }

            images.push(GeneratedImage {
                data_base64: b64,
                url,
            });

            // Capture revised prompt from first image
            if revised_prompt.is_none() {
                revised_prompt = item
                    .get("revised_prompt")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
    }

    if images.is_empty() {
        return Err("No images returned by the API".into());
    }

    Ok(ImageGenResult {
        images,
        model: model_str,
        revised_prompt,
    })
}

/// Save generated images to workspace output directory.
pub fn save_images_to_workspace(
    result: &ImageGenResult,
    workspace: &std::path::Path,
) -> Result<Vec<String>, String> {
    let output_dir = workspace.join("output");
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Failed to create output dir: {e}"))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let mut paths = Vec::new();

    for (i, image) in result.images.iter().enumerate() {
        let filename = if result.images.len() == 1 {
            format!("image_{timestamp}.png")
        } else {
            format!("image_{timestamp}_{i}.png")
        };

        let path = output_dir.join(&filename);

        // Decode base64 and save
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&image.data_base64)
            .map_err(|e| format!("Failed to decode base64 image: {e}"))?;

        // SECURITY: verify decoded size
        if decoded.len() > 10 * 1024 * 1024 {
            return Err("Decoded image exceeds 10MB limit".into());
        }

        std::fs::write(&path, &decoded)
            .map_err(|e| format!("Failed to write image to {}: {e}", path.display()))?;

        paths.push(path.display().to_string());
    }

    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::media::ImageGenModel;

    #[test]
    fn test_validate_valid_request() {
        let req = ImageGenRequest {
            prompt: "A beautiful sunset".to_string(),
            model: ImageGenModel::DallE3,
            size: "1024x1024".to_string(),
            quality: "hd".to_string(),
            count: 1,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_prompt() {
        let req = ImageGenRequest {
            prompt: String::new(),
            model: ImageGenModel::DallE3,
            size: "1024x1024".to_string(),
            quality: "standard".to_string(),
            count: 1,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_validate_dalle2_sizes() {
        for size in &["256x256", "512x512", "1024x1024"] {
            let req = ImageGenRequest {
                prompt: "test".to_string(),
                model: ImageGenModel::DallE2,
                size: size.to_string(),
                quality: "standard".to_string(),
                count: 1,
            };
            assert!(req.validate().is_ok(), "Failed for size {size}");
        }
    }

    #[test]
    fn test_validate_gpt_image_sizes() {
        for size in &["1024x1024", "1536x1024", "1024x1536"] {
            let req = ImageGenRequest {
                prompt: "test".to_string(),
                model: ImageGenModel::GptImage1,
                size: size.to_string(),
                quality: "auto".to_string(),
                count: 2,
            };
            assert!(req.validate().is_ok(), "Failed for size {size}");
        }
    }

    /// Closes #1051: when `image_gen_base_url` is set, the URL building
    /// logic must use the override (with `/v1/images/generations` appended)
    /// and strip any trailing slash from the user-supplied base. When unset,
    /// the hardcoded provider URL is used.
    #[test]
    fn test_image_gen_base_url_override_logic() {
        // Helper mirroring the URL construction in `generate_image`.
        fn build(base: Option<&str>) -> String {
            base.map(|b| format!("{}/v1/images/generations", b.trim_end_matches('/')))
                .unwrap_or_else(|| "https://api.openai.com/v1/images/generations".to_string())
        }

        // Default: hardcoded URL preserved (backward compatibility).
        assert_eq!(build(None), "https://api.openai.com/v1/images/generations");

        // Override applied.
        assert_eq!(
            build(Some("http://127.0.0.1:7000")),
            "http://127.0.0.1:7000/v1/images/generations"
        );

        // Trailing slash on the user-supplied base is stripped.
        assert_eq!(
            build(Some("http://127.0.0.1:7000/")),
            "http://127.0.0.1:7000/v1/images/generations"
        );
        assert_eq!(
            build(Some("https://images.example.com/")),
            "https://images.example.com/v1/images/generations"
        );
    }

    #[test]
    fn test_save_images_creates_dir() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let result = ImageGenResult {
            images: vec![GeneratedImage {
                // Minimal valid base64 (8 zero bytes)
                data_base64: base64::engine::general_purpose::STANDARD.encode([0u8; 8]),
                url: None,
            }],
            model: "dall-e-3".to_string(),
            revised_prompt: None,
        };
        let paths = save_images_to_workspace(&result, workspace).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(std::path::Path::new(&paths[0]).exists());
    }
}
