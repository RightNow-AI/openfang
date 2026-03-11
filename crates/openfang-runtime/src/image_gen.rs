//! Image generation — DALL-E 3, DALL-E 2, GPT-Image-1 via OpenAI API.

use base64::Engine;
use openfang_types::media::{GeneratedImage, ImageGenRequest, ImageGenResult};
use tracing::warn;

/// Generate images via OpenAI's image generation API.
///
/// Requires OPENAI_API_KEY to be set.
pub async fn generate_image(request: &ImageGenRequest) -> Result<ImageGenResult, String> {
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

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/images/generations")
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
        let truncated = if error_body.len() > 500 {
            let mut end = 500;
            while end > 0 && !error_body.is_char_boundary(end) { end -= 1; }
            &error_body[..end]
        } else {
            &error_body
        };
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
