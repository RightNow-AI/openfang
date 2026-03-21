//! Telegram media batch structures for structured media group handling.

use serde::{Deserialize, Serialize};

/// Status of a media item in a Telegram media batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MediaItemStatus {
    /// Media is downloaded and ready at local_path.
    Ready,
    /// Media exceeds safe download limit, needs project-side download.
    NeedsProjectDownload,
    /// Media was skipped due to safety limits.
    SkippedSafeLimit,
    /// Download attempt failed.
    DownloadFailed,
}

/// Type of media item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MediaItemKind {
    Image,
    Video,
    Document,
}

/// A single media item within a Telegram media batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMediaItem {
    /// Type of media (image/video/document).
    pub kind: MediaItemKind,
    /// Telegram file_id.
    pub file_id: String,
    /// Original filename (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    /// File size in bytes.
    pub file_size: u64,
    /// Duration in seconds (for video/audio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    /// Current status of this media item.
    pub status: MediaItemStatus,
    /// Local file path (if downloaded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// Download hint for project-side downloaders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_hint: Option<TelegramDownloadHint>,
}

/// Structured hint for project-side Telegram Bot API download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramDownloadHint {
    /// Download strategy name.
    pub strategy: String,
    /// Telegram file ID.
    pub file_id: String,
    /// Telegram Bot API base URL.
    pub api_base_url: String,
    /// Whether upstream bridge uses Telegram Local Bot API.
    pub use_local_api: bool,
    /// Optional direct download URL if already resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// Optional reason for fallback/deferred download.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Structured representation of a Telegram media group batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMediaBatch {
    /// Unique stable batch key, aligned with shipinbot batch_id.
    pub batch_key: String,
    /// Telegram chat ID.
    pub chat_id: i64,
    /// First message ID in the batch.
    pub message_id: i64,
    /// Telegram media_group_id.
    pub media_group_id: String,
    /// Combined caption from the media group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// All media items in this batch.
    pub items: Vec<TelegramMediaItem>,
}

impl TelegramMediaBatch {
    fn sanitize(input: &str) -> String {
        let cleaned: String = input
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        let collapsed = cleaned
            .split('_')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        if collapsed.is_empty() {
            "unknown".to_string()
        } else {
            collapsed
        }
    }

    /// Build stable batch key with normalized format:
    /// `group_<safe_chat_id>_<safe_media_group_id>_<hash8>`.
    /// The 8-hex-digit hash of the raw media_group_id prevents collisions
    /// when different IDs sanitize to the same string (e.g. "abc@def" vs "abc_def").
    pub fn stable_batch_key(chat_id: i64, media_group_id: &str) -> String {
        let hash = media_group_id
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
        format!(
            "group_{}_{}_{:08x}",
            Self::sanitize(&chat_id.to_string()),
            Self::sanitize(media_group_id),
            hash,
        )
    }

    /// Build a stable key for a non-media-group Telegram message.
    pub fn single_message_key(chat_id: i64, message_id: i64) -> String {
        format!(
            "single_{}_{}",
            Self::sanitize(&chat_id.to_string()),
            Self::sanitize(&message_id.to_string()),
        )
    }

    /// Count how many items of each kind are in this batch.
    pub fn count_by_kind(&self) -> (usize, usize, usize) {
        let mut images = 0;
        let mut videos = 0;
        let mut documents = 0;
        for item in &self.items {
            match item.kind {
                MediaItemKind::Image => images += 1,
                MediaItemKind::Video => videos += 1,
                MediaItemKind::Document => documents += 1,
            }
        }
        (images, videos, documents)
    }

    /// Generate a human-readable summary of this batch.
    pub fn summary(&self) -> String {
        let (images, videos, documents) = self.count_by_kind();
        let mut parts = Vec::new();
        if videos > 0 {
            parts.push(format!("{} 个视频", videos));
        }
        if images > 0 {
            parts.push(format!("{} 张图片", images));
        }
        if documents > 0 {
            parts.push(format!("{} 个文件", documents));
        }
        format!("收到 Telegram 媒体批次：{}。", parts.join("、"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_batch_summary() {
        let batch = TelegramMediaBatch {
            batch_key: "test-batch".to_string(),
            chat_id: 123,
            message_id: 456,
            media_group_id: "group-1".to_string(),
            caption: None,
            items: vec![
                TelegramMediaItem {
                    kind: MediaItemKind::Video,
                    file_id: "vid1".to_string(),
                    original_name: None,
                    file_size: 1000,
                    duration_seconds: Some(30),
                    status: MediaItemStatus::Ready,
                    local_path: Some("/tmp/vid1.mp4".to_string()),
                    download_hint: None,
                },
                TelegramMediaItem {
                    kind: MediaItemKind::Image,
                    file_id: "img1".to_string(),
                    original_name: None,
                    file_size: 500,
                    duration_seconds: None,
                    status: MediaItemStatus::Ready,
                    local_path: Some("/tmp/img1.jpg".to_string()),
                    download_hint: None,
                },
            ],
        };

        assert_eq!(batch.count_by_kind(), (1, 1, 0));
        assert_eq!(
            batch.summary(),
            "收到 Telegram 媒体批次：1 个视频、1 张图片。"
        );
    }

    #[test]
    fn test_media_item_status_serde() {
        let statuses = vec![
            MediaItemStatus::Ready,
            MediaItemStatus::NeedsProjectDownload,
            MediaItemStatus::SkippedSafeLimit,
            MediaItemStatus::DownloadFailed,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let back: MediaItemStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, back);
        }
    }

    #[test]
    fn test_stable_batch_key() {
        // hash("173552@abc") via djb31 = 0x9f98b547
        assert_eq!(
            TelegramMediaBatch::stable_batch_key(-100123, "173552@abc"),
            "group_100123_173552_abc_9f98b547"
        );
    }

    #[test]
    fn test_stable_batch_key_collision_prevention() {
        // "abc@def" and "abc_def" both sanitize to "abc_def" but must produce different keys
        let key1 = TelegramMediaBatch::stable_batch_key(1, "abc@def");
        let key2 = TelegramMediaBatch::stable_batch_key(1, "abc_def");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_single_message_key() {
        assert_eq!(
            TelegramMediaBatch::single_message_key(-100123, 456),
            "single_100123_456"
        );
    }
}
