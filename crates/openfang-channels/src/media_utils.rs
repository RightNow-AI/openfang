//! Shared media processing utilities for channel adapters.
//!
//! Provides Gemini Vision image recognition, text file detection,
//! and HTTP download helpers used by Telegram, Discord, and other adapters.

use std::time::Duration;
use tracing::warn;

/// Recognize image content using Gemini Vision API.
///
/// Returns a human-readable description of the image. On failure, returns
/// an error string wrapped in brackets (e.g., `[Image recognition failed: ...]`).
pub async fn recognize_image_gemini(
    client: &reqwest::Client,
    image_data: &[u8],
    user_text: &str,
) -> String {
    let gemini_key = match std::env::var("GEMINI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            warn!("GEMINI_API_KEY not set, cannot perform image recognition");
            return "[Image recognition unavailable: GEMINI_API_KEY not configured]".to_string();
        }
    };

    // Detect MIME type from magic bytes
    let mime = detect_image_mime(image_data);

    use base64::Engine;
    let b64_data = base64::engine::general_purpose::STANDARD.encode(image_data);

    let prompt = if user_text.is_empty() {
        "Describe this image in detail."
    } else {
        user_text
    };

    let payload = serde_json::json!({
        "contents": [{
            "parts": [
                {"text": prompt},
                {
                    "inline_data": {
                        "mime_type": mime,
                        "data": b64_data,
                    }
                }
            ]
        }],
        "generationConfig": {
            "temperature": 0.4,
            "maxOutputTokens": 2048,
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={gemini_key}"
    );

    match client
        .post(&url)
        .json(&payload)
        .timeout(Duration::from_secs(60))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<serde_json::Value>().await {
                    Ok(result) => {
                        if let Some(candidates) = result["candidates"].as_array() {
                            if let Some(first) = candidates.first() {
                                if let Some(parts) = first["content"]["parts"].as_array() {
                                    let text: String = parts
                                        .iter()
                                        .filter_map(|p| p["text"].as_str())
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    if !text.is_empty() {
                                        return text;
                                    }
                                }
                            }
                        }
                        "[Image recognition returned no result]".to_string()
                    }
                    Err(e) => {
                        warn!("Gemini Vision parse error: {e}");
                        format!("[Image recognition parse error: {e}]")
                    }
                }
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!(
                    "Gemini Vision error [{status}]: {}",
                    openfang_types::truncate_str(&body, 200)
                );
                format!("[Image recognition failed: HTTP {status}]")
            }
        }
        Err(e) => {
            warn!("Gemini Vision request error: {e}");
            format!("[Image recognition request error: {e}]")
        }
    }
}

/// Detect image MIME type from magic bytes.
pub fn detect_image_mime(data: &[u8]) -> &'static str {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if data.starts_with(b"GIF8") {
        "image/gif"
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// Check if a file is text-based by extension or MIME type.
pub fn is_text_file(filename: &str, mime_type: &str) -> bool {
    const TEXT_EXTENSIONS: &[&str] = &[
        ".txt", ".md", ".py", ".js", ".ts", ".json", ".csv", ".xml", ".html", ".log", ".yaml",
        ".yml", ".toml", ".ini", ".cfg", ".conf", ".sh", ".bat", ".rs", ".go", ".java", ".c",
        ".cpp", ".h", ".hpp", ".css", ".sql", ".rb", ".php", ".lua",
    ];
    let lower = filename.to_lowercase();
    TEXT_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
        || mime_type.starts_with("text/")
        || mime_type == "application/json"
        || mime_type == "application/xml"
}

/// Check if a MIME type or filename represents an image.
pub fn is_image(filename: &str, content_type: &str) -> bool {
    content_type.starts_with("image/")
        || filename
            .to_lowercase()
            .ends_with(".jpg")
        || filename.to_lowercase().ends_with(".jpeg")
        || filename.to_lowercase().ends_with(".png")
        || filename.to_lowercase().ends_with(".gif")
        || filename.to_lowercase().ends_with(".webp")
        || filename.to_lowercase().ends_with(".bmp")
}

/// Download a file from a URL, returning the bytes on success.
pub async fn download_url(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    match client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                resp.bytes().await.ok().map(|b| b.to_vec())
            } else {
                warn!("Download failed [{}]: {}", resp.status(), url);
                None
            }
        }
        Err(e) => {
            warn!("Download error for {}: {e}", url);
            None
        }
    }
}

/// Process an attachment (image or file) into a text description.
///
/// - Images: recognized via Gemini Vision
/// - Text files (<50KB): content extracted
/// - Other files: metadata reported
pub async fn process_attachment_to_text(
    client: &reqwest::Client,
    url: &str,
    filename: &str,
    content_type: &str,
    file_size: u64,
    caption: &str,
) -> String {
    let bytes = match download_url(client, url).await {
        Some(b) => b,
        None => {
            return format!(
                "[File {filename} download failed (size: {file_size} bytes)]"
            );
        }
    };

    if is_image(filename, content_type) {
        let recognition = recognize_image_gemini(client, &bytes, caption).await;
        if caption.is_empty() {
            format!("[User sent an image: {filename}]\nImage content: {recognition}")
        } else {
            format!("[User sent an image: {filename}, saying: {caption}]\nImage content: {recognition}")
        }
    } else if is_text_file(filename, content_type) && bytes.len() < 50_000 {
        match std::str::from_utf8(&bytes) {
            Ok(content) => {
                let truncated = openfang_types::truncate_str(content, 3000);
                let mut text = format!(
                    "[User sent file: {filename} (size: {file_size} bytes)]\nFile content:\n```\n{truncated}\n```"
                );
                if content.len() > 3000 {
                    text.push_str(&format!("\n... ({} chars total, truncated)", content.len()));
                }
                text
            }
            Err(_) => {
                format!("[User sent file: {filename} (size: {file_size} bytes, binary file)]")
            }
        }
    } else {
        format!(
            "[User sent file: {filename} (size: {file_size} bytes, type: {content_type})]"
        )
    }
}
