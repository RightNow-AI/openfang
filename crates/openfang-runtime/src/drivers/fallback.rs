//! Fallback driver — tries multiple LLM drivers in sequence.
//!
//! If the primary driver fails with a non-retryable error, the fallback driver
//! moves to the next driver in the chain.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::warn;

/// An entry in the fallback chain: a driver paired with an optional model
/// override. When `model_override` is `Some`, the request's `model` field is
/// replaced before forwarding to this driver.
struct FallbackEntry {
    driver: Arc<dyn LlmDriver>,
    /// Model name to inject into the request for this driver, or `None` to
    /// keep the original request model (i.e. the primary).
    model_override: Option<String>,
}

/// A driver that wraps multiple LLM drivers and tries each in order.
///
/// On failure (including rate-limit and overload), moves to the next driver.
/// Only returns an error when ALL drivers in the chain are exhausted.
pub struct FallbackDriver {
    entries: Vec<FallbackEntry>,
}

impl FallbackDriver {
    /// Create a new fallback driver from an ordered chain of drivers.
    ///
    /// The first driver is the primary; subsequent are fallbacks.
    /// All drivers share the same request model (legacy behaviour).
    pub fn new(drivers: Vec<Arc<dyn LlmDriver>>) -> Self {
        let entries = drivers
            .into_iter()
            .map(|driver| FallbackEntry {
                driver,
                model_override: None,
            })
            .collect();
        Self { entries }
    }

    /// Create a fallback driver where each driver has an associated model name.
    ///
    /// The first entry's model is treated as the primary (no override needed
    /// because the request already carries it). Subsequent entries override
    /// `request.model` with their own model name before forwarding.
    pub fn with_models(drivers_and_models: Vec<(Arc<dyn LlmDriver>, String)>) -> Self {
        let entries = drivers_and_models
            .into_iter()
            .enumerate()
            .map(|(i, (driver, model))| FallbackEntry {
                driver,
                // The primary (index 0) keeps the original request model;
                // fallbacks override it.
                model_override: if i == 0 { None } else { Some(model) },
            })
            .collect();
        Self { entries }
    }
}

#[async_trait]
impl LlmDriver for FallbackDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut last_error = None;

        for (i, entry) in self.entries.iter().enumerate() {
            let mut req = request.clone();
            if let Some(ref model) = entry.model_override {
                req.model = model.clone();
            }
            match entry.driver.complete(req).await {
                Ok(response) => return Ok(response),
                Err(e @ LlmError::RateLimited { .. }) | Err(e @ LlmError::Overloaded { .. }) => {
                    warn!(
                        driver_index = i,
                        error = %e,
                        "Driver rate-limited/overloaded, trying next fallback"
                    );
                    last_error = Some(e);
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

        for (i, entry) in self.entries.iter().enumerate() {
            let mut req = request.clone();
            if let Some(ref model) = entry.model_override {
                req.model = model.clone();
            }
            match entry.driver.stream(req, tx.clone()).await {
                Ok(response) => return Ok(response),
                Err(e @ LlmError::RateLimited { .. }) | Err(e @ LlmError::Overloaded { .. }) => {
                    warn!(
                        driver_index = i,
                        error = %e,
                        "Driver rate-limited/overloaded (stream), trying next fallback"
                    );
                    last_error = Some(e);
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
            Arc::new(OkDriver) as Arc<dyn LlmDriver>,
            Arc::new(FailDriver) as Arc<dyn LlmDriver>,
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text(), "OK");
    }

    #[tokio::test]
    async fn test_fallback_primary_fails_secondary_succeeds() {
        let driver = FallbackDriver::new(vec![
            Arc::new(FailDriver) as Arc<dyn LlmDriver>,
            Arc::new(OkDriver) as Arc<dyn LlmDriver>,
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fallback_all_fail() {
        let driver = FallbackDriver::new(vec![
            Arc::new(FailDriver) as Arc<dyn LlmDriver>,
            Arc::new(FailDriver) as Arc<dyn LlmDriver>,
        ]);
        let result = driver.complete(test_request()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit_falls_through() {
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
            Arc::new(RateLimitDriver) as Arc<dyn LlmDriver>,
            Arc::new(OkDriver) as Arc<dyn LlmDriver>,
        ]);
        let result = driver.complete(test_request()).await;
        // Rate limit should fall through to the OkDriver fallback
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text(), "OK");
    }

    #[tokio::test]
    async fn test_rate_limit_all_fail() {
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
            Arc::new(RateLimitDriver) as Arc<dyn LlmDriver>,
            Arc::new(RateLimitDriver) as Arc<dyn LlmDriver>,
        ]);
        let result = driver.complete(test_request()).await;
        // All drivers rate-limited — error should bubble up
        assert!(matches!(result, Err(LlmError::RateLimited { .. })));
    }

    #[tokio::test]
    async fn test_with_models_overrides_model_on_fallback() {
        /// A driver that captures the model name from the request.
        struct ModelCapture {
            expected_model: String,
        }

        #[async_trait]
        impl LlmDriver for ModelCapture {
            async fn complete(
                &self,
                req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                if req.model == self.expected_model {
                    Ok(CompletionResponse {
                        content: vec![ContentBlock::Text {
                            text: format!("model={}", req.model),
                        }],
                        stop_reason: StopReason::EndTurn,
                        tool_calls: vec![],
                        usage: TokenUsage {
                            input_tokens: 1,
                            output_tokens: 1,
                        },
                    })
                } else {
                    Err(LlmError::Api {
                        status: 400,
                        message: format!(
                            "wrong model: got '{}', expected '{}'",
                            req.model, self.expected_model
                        ),
                    })
                }
            }
        }

        // Primary fails, fallback should receive its own model name
        let driver = FallbackDriver::with_models(vec![
            (Arc::new(FailDriver) as Arc<dyn LlmDriver>, "primary-model".to_string()),
            (
                Arc::new(ModelCapture {
                    expected_model: "fallback-model".to_string(),
                }) as Arc<dyn LlmDriver>,
                "fallback-model".to_string(),
            ),
        ]);

        let mut req = test_request();
        req.model = "primary-model".to_string();
        let result = driver.complete(req).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text(), "model=fallback-model");
    }
}
