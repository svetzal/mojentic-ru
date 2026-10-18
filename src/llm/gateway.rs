use crate::error::Result;
use crate::llm::models::{LlmGatewayResponse, LlmMessage};
use crate::llm::stream_events::{StreamEventError, StreamEventStream};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
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

/// Reasoning effort level for models that support extended thinking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    /// Disable provider-native extended thinking when the provider supports it.
    Disabled,
    /// Enable reasoning using the model default (for boolean-control models).
    Enabled,
    Low,
    Medium,
    High,
    /// Extra-high reasoning effort; distinct from high.
    XHigh,
}

/// Configuration for LLM completion.
///
/// Provider controls default to `None`: the provider applies its own settings.
/// To restore the former 2.1 defaults explicitly:
///
/// ```
/// use mojentic::llm::gateway::CompletionConfig;
/// let config = CompletionConfig {
///     temperature: Some(1.0),
///     num_ctx: Some(32768),
///     max_tokens: Some(16384),
///     ..Default::default()
/// };
/// assert_eq!(config.max_tokens, Some(16384));
/// ```
#[derive(Debug, Clone)]
pub struct CompletionConfig {
    /// Sampling temperature. Unset delegates to provider settings.
    pub temperature: Option<f32>,
    /// Ollama context size. Other maintained providers omit this control.
    pub num_ctx: Option<usize>,
    /// Output limit: Ollama `options.num_predict`, OpenAI model-specific token
    /// limit, or oMLX `max_tokens`. Unset sends no output limit.
    pub max_tokens: Option<usize>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub response_format: Option<ResponseFormat>,
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Explicit model capabilities. None delegates support/defaults to the provider.
    pub reasoning_profile: Option<crate::llm::reasoning::ReasoningProfile>,
    /// Retain completed assistant reasoning during broker-managed tool rounds.
    /// This does not turn reasoning on, and does not retain cancelled partial calls.
    pub preserve_reasoning: bool,
    /// Maximum tool rounds; use `with_unlimited_tool_iterations` to disable the limit.
    pub max_tool_iterations: usize,
}

impl CompletionConfig {
    /// Validate explicit model capabilities before sending a provider request.
    pub fn validate_reasoning(&self) -> Result<()> {
        if let Some(profile) = &self.reasoning_profile {
            profile.validate(self.reasoning_effort, self.preserve_reasoning)?;
        }
        Ok(())
    }

    /// Disable the tool-round limit while retaining the existing numeric configuration API.
    /// `usize::MAX` is a symbolic unlimited value; the broker skips its limit check.
    pub fn with_unlimited_tool_iterations(mut self) -> Self {
        self.max_tool_iterations = usize::MAX;
        self
    }

    pub(crate) fn tool_iteration_limit_reached(&self, iteration: usize) -> bool {
        self.max_tool_iterations != usize::MAX && iteration >= self.max_tool_iterations
    }
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            temperature: None,
            num_ctx: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            response_format: None,
            reasoning_effort: None,
            reasoning_profile: None,
            preserve_reasoning: false,
            max_tool_iterations: 10,
        }
    }
}

/// Provider-advertised context capacity; independent of completion configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelContextWindow {
    /// Positive provider maximum, in tokens.
    pub max_context_tokens: std::num::NonZeroUsize,
    /// Provider field path, or `registry` for OpenAI.
    pub field: String,
    /// Ollama model parameter used when the request omits `num_ctx`.
    pub model_num_ctx: Option<usize>,
}

impl ModelContextWindow {
    pub(crate) fn advertised(value: &Value, field: String) -> Result<Self> {
        let tokens = value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .and_then(std::num::NonZeroUsize::new)
            .ok_or_else(|| {
                crate::error::MojenticError::GatewayError(format!(
                    "Invalid positive integer at {field}"
                ))
            })?;
        Ok(Self {
            max_context_tokens: tokens,
            field,
            model_num_ctx: None,
        })
    }
}

/// Abstract interface for LLM providers
#[async_trait]
pub trait LlmGateway: Send + Sync {
    /// Monotonic clock used by maintained streaming adapters.
    fn stream_clock(&self) -> std::sync::Arc<dyn crate::llm::stream_timing::StreamClock> {
        crate::llm::stream_timing::system_clock()
    }

    /// Query advertised context capacity without changing subsequent requests.
    ///
    /// Third-party gateways default to not advertised.
    ///
    /// # Errors
    /// Returns the gateway's existing transport, status or malformed-body error.
    async fn get_model_context_window(&self, _model: &str) -> Result<Option<ModelContextWindow>> {
        Ok(None)
    }

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

    /// Complete a structured JSON request and keep the provider's evidence.
    ///
    /// The response carries the parsed object, the raw content when the
    /// gateway has it, and [`crate::llm::models::ResponseEvidence`]. The
    /// default implementation wraps [`LlmGateway::complete_json`] and reports
    /// no evidence; gateways that see provider metadata override it.
    async fn complete_json_response(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse<Value>> {
        let object = self.complete_json(model, messages, schema, config).await?;
        Ok(LlmGatewayResponse {
            object: Some(object),
            ..Default::default()
        })
    }

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

    /// Stream one turn as [`crate::llm::StreamEvent`]s with terminal completion evidence.
    ///
    /// No tools are sent. The returned stream sends exactly one request when
    /// first polled and ends with exactly one terminal event. Dropping it
    /// cancels the request.
    ///
    /// # Errors
    ///
    /// Returns [`StreamEventError::StreamEventsUnsupported`] when the gateway
    /// does not implement the events API (the default), or
    /// [`StreamEventError::RequestFailed`] when the request cannot be built.
    /// No request is sent in either case. The broker turns either into a
    /// single terminal error event.
    fn complete_stream_events<'a>(
        &'a self,
        _model: &'a str,
        _messages: &'a [LlmMessage],
        _config: &'a CompletionConfig,
    ) -> std::result::Result<StreamEventStream<'a>, StreamEventError> {
        Err(StreamEventError::StreamEventsUnsupported)
    }
}

/// Streaming response chunk
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Content text chunk
    Content(String),
    /// Reasoning/thinking text chunk that providers expose separately from assistant content.
    Thinking(String),
    /// Complete tool calls (accumulated from stream)
    ToolCalls(Vec<crate::llm::models::LlmToolCall>),
    /// Provider-neutral heartbeat/progress for a streamed provider frame.
    Progress(StreamProgress),
    /// Provider-neutral final generation metrics when the provider exposes them.
    Metrics(StreamMetrics),
}

/// Provider-neutral progress for a streamed provider frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamProgress {
    pub provider: String,
    pub frame_index: usize,
    pub done: bool,
    pub content_chars: usize,
    pub thinking_chars: usize,
    pub tool_call_count: usize,
    pub accumulated_tool_call_count: usize,
}

/// Provider-neutral final generation metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StreamMetrics {
    /// Client milliseconds from first dispatch to first semantic token.
    pub time_to_first_token_ms: Option<u64>,
    /// First semantic kind: thinking, content, or tool_call.
    pub first_token_kind: Option<String>,
    /// Client milliseconds from first dispatch to terminal success or failure.
    pub request_duration_ms: Option<u64>,
    /// Provider usage and durations in their original shape and units.
    pub provider_usage: Option<Value>,
    pub provider: String,
    pub total_duration_ns: Option<u64>,
    pub load_duration_ns: Option<u64>,
    pub prompt_eval_count: Option<u64>,
    pub prompt_eval_duration_ns: Option<u64>,
    pub eval_count: Option<u64>,
    pub eval_duration_ns: Option<u64>,
    pub tokens_per_second: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_config_default() {
        let config = CompletionConfig::default();

        assert_eq!(config.temperature, None);
        assert_eq!(config.num_ctx, None);
        assert_eq!(config.max_tokens, None);
        assert_eq!(config.top_p, None);
        assert_eq!(config.top_k, None);
        assert!(config.response_format.is_none());
        assert!(config.reasoning_effort.is_none());
        assert_eq!(config.max_tool_iterations, 10);
    }

    #[test]
    fn test_completion_config_custom() {
        let config = CompletionConfig {
            temperature: Some(0.5),
            num_ctx: Some(2048),
            max_tokens: Some(1024),
            top_p: Some(0.9),
            top_k: Some(40),
            response_format: Some(ResponseFormat::Text),
            reasoning_effort: None,
            reasoning_profile: None,
            preserve_reasoning: false,
            max_tool_iterations: 10,
        };

        assert_eq!(config.temperature, Some(0.5));
        assert_eq!(config.num_ctx, Some(2048));
        assert_eq!(config.max_tokens, Some(1024));
        assert_eq!(config.top_p, Some(0.9));
        assert_eq!(config.top_k, Some(40));
        assert!(matches!(config.response_format, Some(ResponseFormat::Text)));
    }

    #[test]
    fn test_completion_config_clone() {
        let config1 = CompletionConfig {
            temperature: Some(0.7),
            num_ctx: Some(4096),
            max_tokens: Some(2048),
            top_p: Some(0.95),
            top_k: Some(50),
            response_format: Some(ResponseFormat::JsonObject { schema: None }),
            reasoning_effort: None,
            reasoning_profile: None,
            preserve_reasoning: false,
            max_tool_iterations: 10,
        };

        let config2 = config1.clone();

        assert_eq!(config1.temperature, config2.temperature);
        assert_eq!(config1.num_ctx, config2.num_ctx);
        assert_eq!(config1.max_tokens, config2.max_tokens);
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
            temperature: Some(0.8),
            num_ctx: Some(8192),
            max_tokens: Some(4096),
            top_p: Some(0.92),
            top_k: Some(60),
            response_format: Some(ResponseFormat::JsonObject {
                schema: Some(serde_json::json!({"type": "object"})),
            }),
            reasoning_effort: None,
            reasoning_profile: None,
            preserve_reasoning: false,
            max_tool_iterations: 10,
        };

        assert_eq!(config.temperature, Some(0.8));
        assert_eq!(config.top_p, Some(0.92));
        assert_eq!(config.top_k, Some(60));
        assert!(config.response_format.is_some());
    }

    #[test]
    fn test_reasoning_effort_serialization() {
        assert_eq!(serde_json::to_string(&ReasoningEffort::Disabled).unwrap(), "\"disabled\"");
        assert_eq!(serde_json::to_string(&ReasoningEffort::Low).unwrap(), "\"low\"");
        assert_eq!(serde_json::to_string(&ReasoningEffort::Medium).unwrap(), "\"medium\"");
        assert_eq!(serde_json::to_string(&ReasoningEffort::High).unwrap(), "\"high\"");
    }

    #[test]
    fn test_reasoning_effort_deserialization() {
        assert_eq!(
            serde_json::from_str::<ReasoningEffort>("\"disabled\"").unwrap(),
            ReasoningEffort::Disabled
        );
        assert_eq!(
            serde_json::from_str::<ReasoningEffort>("\"low\"").unwrap(),
            ReasoningEffort::Low
        );
        assert_eq!(
            serde_json::from_str::<ReasoningEffort>("\"medium\"").unwrap(),
            ReasoningEffort::Medium
        );
        assert_eq!(
            serde_json::from_str::<ReasoningEffort>("\"high\"").unwrap(),
            ReasoningEffort::High
        );
    }

    #[test]
    fn test_completion_config_with_reasoning_effort() {
        let config = CompletionConfig {
            temperature: Some(1.0),
            num_ctx: Some(32768),
            max_tokens: Some(16384),
            top_p: None,
            top_k: None,
            response_format: None,
            reasoning_effort: Some(ReasoningEffort::High),
            reasoning_profile: None,
            preserve_reasoning: false,
            max_tool_iterations: 10,
        };

        assert_eq!(config.reasoning_effort, Some(ReasoningEffort::High));
    }
}
