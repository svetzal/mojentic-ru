use crate::error::Result;
use crate::llm::models::{LlmGatewayResponse, LlmMessage};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use serde_json::Value;

/// Configuration for LLM completion
#[derive(Debug, Clone)]
pub struct CompletionConfig {
    pub temperature: f32,
    pub num_ctx: usize,
    pub max_tokens: usize,
    pub num_predict: Option<i32>,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            num_ctx: 32768,
            max_tokens: 16384,
            num_predict: None,
        }
    }
}

/// Abstract interface for LLM providers
#[async_trait]
pub trait LlmGateway: Send + Sync {
    /// Complete an LLM request with text response
    async fn complete(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse>;

    /// Complete an LLM request with structured JSON response
    async fn complete_json(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<Value>;

    /// Get list of available models
    async fn get_available_models(&self) -> Result<Vec<String>>;

    /// Calculate embeddings for text
    async fn calculate_embeddings(&self, text: &str, model: Option<&str>) -> Result<Vec<f32>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_config_default() {
        let config = CompletionConfig::default();

        assert_eq!(config.temperature, 1.0);
        assert_eq!(config.num_ctx, 32768);
        assert_eq!(config.max_tokens, 16384);
        assert_eq!(config.num_predict, None);
    }

    #[test]
    fn test_completion_config_custom() {
        let config = CompletionConfig {
            temperature: 0.5,
            num_ctx: 2048,
            max_tokens: 1024,
            num_predict: Some(100),
        };

        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.num_ctx, 2048);
        assert_eq!(config.max_tokens, 1024);
        assert_eq!(config.num_predict, Some(100));
    }

    #[test]
    fn test_completion_config_clone() {
        let config1 = CompletionConfig {
            temperature: 0.7,
            num_ctx: 4096,
            max_tokens: 2048,
            num_predict: Some(50),
        };

        let config2 = config1.clone();

        assert_eq!(config1.temperature, config2.temperature);
        assert_eq!(config1.num_ctx, config2.num_ctx);
        assert_eq!(config1.max_tokens, config2.max_tokens);
        assert_eq!(config1.num_predict, config2.num_predict);
    }
}
