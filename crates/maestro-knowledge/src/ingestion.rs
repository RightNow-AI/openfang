use crate::{Document, KnowledgeError, KnowledgeResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionConfig {
    pub source_type: IngestionSourceType,
    pub connection_string: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionSourceType {
    File,
    Directory,
    Url,
    S3,
    GithubRepository,
}

#[async_trait]
pub trait IngestionSource: Send + Sync {
    async fn ingest(&self, config: &IngestionConfig) -> KnowledgeResult<Vec<Document>>;
}

pub struct FileIngestor;

#[async_trait]
impl IngestionSource for FileIngestor {
    async fn ingest(&self, config: &IngestionConfig) -> KnowledgeResult<Vec<Document>> {
        let path = std::path::Path::new(&config.connection_string);
        if !path.exists() {
            return Err(KnowledgeError::NotFound(config.connection_string.clone()));
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| KnowledgeError::Config(format!("Failed to read file: {}", e)))?;

        let doc = Document::new(
            config.connection_string.clone(),
            path.file_name().unwrap().to_str().unwrap(),
            content,
            config.connection_string.clone(),
            config.metadata.clone(),
        );

        Ok(vec![doc])
    }
}

pub struct DirectoryIngestor;

#[async_trait]
impl IngestionSource for DirectoryIngestor {
    async fn ingest(&self, config: &IngestionConfig) -> KnowledgeResult<Vec<Document>> {
        let path = std::path::Path::new(&config.connection_string);
        if !path.is_dir() {
            return Err(KnowledgeError::Config("Path is not a directory".to_string()));
        }

        let mut documents = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|e| KnowledgeError::Config(format!("Failed to read directory: {}", e)))? {
            let entry = entry.map_err(|e| KnowledgeError::Config(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();
            if path.is_file() {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| KnowledgeError::Config(format!("Failed to read file: {}", e)))?;

                let doc = Document::new(
                    path.to_str().unwrap(),
                    path.file_name().unwrap().to_str().unwrap(),
                    content,
                    path.to_str().unwrap(),
                    config.metadata.clone(),
                );
                documents.push(doc);
            }
        }

        Ok(documents)
    }
}

pub struct UrlIngestor;

#[async_trait]
impl IngestionSource for UrlIngestor {
    async fn ingest(&self, config: &IngestionConfig) -> KnowledgeResult<Vec<Document>> {
        let url = &config.connection_string;
        let response = reqwest::get(url)
            .await
            .map_err(|e| KnowledgeError::Config(format!("Failed to fetch URL: {}", e)))?;

        let content = response
            .text()
            .await
            .map_err(|e| KnowledgeError::Config(format!("Failed to read response body: {}", e)))?;

        let doc = Document::new(
            url.clone(),
            url.clone(),
            content,
            url.clone(),
            config.metadata.clone(),
        );

        Ok(vec![doc])
    }
}
