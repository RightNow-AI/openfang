//! Fallback driver — tries multiple LLM drivers in sequence.
//!
//! If the primary driver fails with a non-retryable error, the fallback driver
//! moves to the next driver in the chain. Each driver can optionally override
//! the model name in the request (e.g., a local model falling back to a cloud
//! model with a different name).

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::warn;

/// A driver that wraps multiple LLM drivers and tries each in order.
///
/// On failure, moves to the next driver. Rate-limit and overload errors
/// are bubbled up for retry logic to handle.
///
/// Each entry in the chain is a `(driver, optional_model_override)` pair.
/// The primary driver (index 0) typically has `None` (uses the request's model),
/// while fallback drivers specify their own model name.
pub struct FallbackDriver {
    drivers: Vec<(Arc<dyn LlmDriver>, Option<String>)>,
}

impl FallbackDriver {
    /// Create a new fallback driver from an ordered chain of drivers.
    ///
    /// The first driver is the primary; subsequent are fallbacks.
    /// Each entry is `(driver, model_override)` — if `model_override` is `Some`,
    /// the request's model field is replaced before sending to that driver.
    pub fn new(drivers: Vec<(Arc<dyn LlmDriver>, Option<String>)>) -> Self {
        Self { drivers }
    }
}

#[async_trait]
impl LlmDriver for FallbackDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut last_error = None;

        for (i, (driver, model_override)) in self.drivers.iter().enumerate() {
            let mut req = request.clone();
            if let Some(ref model) = model_override {
                req.model = model.clone();
            }
            match driver.complete(req).await {
                Ok(response) => return Ok(response),
                Err(e @ LlmError::RateLimited { .. }) | Err(e @ LlmError::Overloaded { .. }) => {
                    // Retryable errors — bubble up for the retry loop to handle
                    return Err(e);
                }
                Err(e) => {
                    warn!(
                        driver_index = i,
                        error = %e,
                        "Fallback driver failed, trying next"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| LlmError::Api {
            status: 0,
            message: "No drivers configured in fallback chain".to_string(),
        }))
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let mut last_error = None;

        for (i, (driver, model_override)) in self.drivers.iter().enumerate() {
            let mut req = request.clone();
            if let Some(ref model) = model_override {
                req.model = model.clone();
            }
            match driver.stream(req, tx.clone()).await {
                Ok(response) => return Ok(response),
                Err(e @ LlmError::RateLimited { .. }) | Err(e @ LlmError::Overloaded { .. }) => {
                    return Err(e);
                }
                Err(e) => {
                    warn!(
                        driver_index = i,
                        error = %e,
                        "Fallback driver (stream) failed, trying next"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| LlmError::Api {
            status: 0,
            message: "No drivers configured in fallback chain".to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_driver::CompletionResponse;
    use openfang_types::message::{ContentBlock, StopReason, TokenUsage};

    struct FailDriver;

    #[async_trait]
    impl LlmDriver for FailDriver {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::Api {
                status: 500,
                message: "Internal error".to_string(),
            })
        }
    }

    struct OkDriver;

    #[async_trait]
    impl LlmDriver for OkDriver {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: "OK".to_string(),
                }],
                stop_reason: StopReason::EndTurn,
                tool_calls: vec![],
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            })
        }
    }

    /// Captures the model name from the request for assertion.
    struct ModelCapture {
        expected: String,
    }

    #[async_trait]
    impl LlmDriver for ModelCapture {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            assert_eq!(req.model, self.expected, "Model override not applied");
            Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: format!("model={}", req.model),
                }],
                stop_reason: StopReason::EndTurn,
                tool_calls: vec![],
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            })
        }
    }

    fn test_request() -> CompletionRequest {
        CompletionRequest {
            model: "test".to_string(),
            messages: vec![],
            tools: vec![],
            max_tokens: 100,
            temperature: 0.0,
            system: None,
            thinking: None,
        }
    }

    #[tokio::test]
    async fn test_fallback_primary_succeeds() {
        let driver = FallbackDriver::new(vec![
            (Arc::new(OkDriver) as Arc<dyn LlmDriver>, None),
            (Arc::new(FailDriver) as Arc<dyn LlmDriver>, None),
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text(), "OK");
    }

    #[tokio::test]
    async fn test_fallback_primary_fails_secondary_succeeds() {
        let driver = FallbackDriver::new(vec![
            (Arc::new(FailDriver) as Arc<dyn LlmDriver>, None),
            (Arc::new(OkDriver) as Arc<dyn LlmDriver>, None),
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fallback_all_fail() {
        let driver = FallbackDriver::new(vec![
            (Arc::new(FailDriver) as Arc<dyn LlmDriver>, None),
            (Arc::new(FailDriver) as Arc<dyn LlmDriver>, None),
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit_bubbles_up() {
        struct RateLimitDriver;

        #[async_trait]
        impl LlmDriver for RateLimitDriver {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                Err(LlmError::RateLimited {
                    retry_after_ms: 5000,
                })
            }
        }

        let driver = FallbackDriver::new(vec![
            (Arc::new(RateLimitDriver) as Arc<dyn LlmDriver>, None),
            (Arc::new(OkDriver) as Arc<dyn LlmDriver>, None),
        ]);
        let result = driver.complete(test_request()).await;
        // Rate limit should NOT fall through to next driver
        assert!(matches!(result, Err(LlmError::RateLimited { .. })));
    }

    #[tokio::test]
    async fn test_model_override_applied_on_fallback() {
        let driver = FallbackDriver::new(vec![
            (Arc::new(FailDriver) as Arc<dyn LlmDriver>, None),
            (
                Arc::new(ModelCapture {
                    expected: "claude-opus-4-6".to_string(),
                }) as Arc<dyn LlmDriver>,
                Some("claude-opus-4-6".to_string()),
            ),
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text(), "model=claude-opus-4-6");
    }

    #[tokio::test]
    async fn test_primary_keeps_original_model() {
        let driver = FallbackDriver::new(vec![
            (
                Arc::new(ModelCapture {
                    expected: "test".to_string(),
                }) as Arc<dyn LlmDriver>,
                None,
            ),
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text(), "model=test");
    }
}
