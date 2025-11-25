use crate::error::Result;
use crate::llm::models::{LlmGatewayResponse, LlmMessage};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;

/// Format specification for LLM responses
#[derive(Debug, Clone)]
pub enum ResponseFormat {
    /// Plain text response
    Text,
    /// JSON object response with optional schema
    JsonObject { schema: Option<Value> },
}

/// Configuration for LLM completion
#[derive(Debug, Clone)]
pub struct CompletionConfig {
    pub temperature: f32,
    pub num_ctx: usize,
    pub max_tokens: usize,
    pub num_predict: Option<i32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub response_format: Option<ResponseFormat>,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            num_ctx: 32768,
            max_tokens: 16384,
            num_predict: None,
            top_p: None,
            top_k: None,
            response_format: None,
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

    /// Stream LLM responses chunk by chunk
    ///
    /// Returns a stream that yields either content chunks or tool calls.
    /// Tool calls will be accumulated and yielded when complete.
    fn complete_stream<'a>(
        &'a self,
        model: &'a str,
        messages: &'a [LlmMessage],
        tools: Option<&'a [Box<dyn LlmTool>]>,
        config: &'a CompletionConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>>;
}

/// Streaming response chunk
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Content text chunk
    Content(String),
    /// Complete tool calls (accumulated from stream)
    ToolCalls(Vec<crate::llm::models::LlmToolCall>),
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
        assert_eq!(config.top_p, None);
        assert_eq!(config.top_k, None);
        assert!(config.response_format.is_none());
    }

    #[test]
    fn test_completion_config_custom() {
        let config = CompletionConfig {
            temperature: 0.5,
            num_ctx: 2048,
            max_tokens: 1024,
            num_predict: Some(100),
            top_p: Some(0.9),
            top_k: Some(40),
            response_format: Some(ResponseFormat::Text),
        };

        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.num_ctx, 2048);
        assert_eq!(config.max_tokens, 1024);
        assert_eq!(config.num_predict, Some(100));
        assert_eq!(config.top_p, Some(0.9));
        assert_eq!(config.top_k, Some(40));
        assert!(matches!(config.response_format, Some(ResponseFormat::Text)));
    }

    #[test]
    fn test_completion_config_clone() {
        let config1 = CompletionConfig {
            temperature: 0.7,
            num_ctx: 4096,
            max_tokens: 2048,
            num_predict: Some(50),
            top_p: Some(0.95),
            top_k: Some(50),
            response_format: Some(ResponseFormat::JsonObject { schema: None }),
        };

        let config2 = config1.clone();

        assert_eq!(config1.temperature, config2.temperature);
        assert_eq!(config1.num_ctx, config2.num_ctx);
        assert_eq!(config1.max_tokens, config2.max_tokens);
        assert_eq!(config1.num_predict, config2.num_predict);
        assert_eq!(config1.top_p, config2.top_p);
        assert_eq!(config1.top_k, config2.top_k);
    }

    #[test]
    fn test_response_format_text() {
        let format = ResponseFormat::Text;
        assert!(matches!(format, ResponseFormat::Text));
    }

    #[test]
    fn test_response_format_json_no_schema() {
        let format = ResponseFormat::JsonObject { schema: None };
        assert!(matches!(format, ResponseFormat::JsonObject { schema: None }));
    }

    #[test]
    fn test_response_format_json_with_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });
        let format = ResponseFormat::JsonObject {
            schema: Some(schema.clone()),
        };

        match format {
            ResponseFormat::JsonObject { schema: Some(s) } => {
                assert_eq!(s, schema);
            }
            _ => panic!("Expected JsonObject with schema"),
        }
    }

    #[test]
    fn test_completion_config_with_all_sampling_params() {
        let config = CompletionConfig {
            temperature: 0.8,
            num_ctx: 8192,
            max_tokens: 4096,
            num_predict: Some(2000),
            top_p: Some(0.92),
            top_k: Some(60),
            response_format: Some(ResponseFormat::JsonObject {
                schema: Some(serde_json::json!({"type": "object"})),
            }),
        };

        assert_eq!(config.temperature, 0.8);
        assert_eq!(config.top_p, Some(0.92));
        assert_eq!(config.top_k, Some(60));
        assert!(config.response_format.is_some());
    }
}
