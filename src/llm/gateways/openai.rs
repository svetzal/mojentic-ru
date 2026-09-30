//! OpenAI Gateway for LLM interactions.
//!
//! This module provides a gateway for interacting with OpenAI's API,
//! including chat completions, streaming, and embeddings.

use crate::error::{MojenticError, Result};
use crate::llm::gateway::{CompletionConfig, LlmGateway, ResponseFormat, StreamChunk};
use crate::llm::gateways::openai_legacy_stream::{legacy_body_stream, OpenAiLegacyParser};
use crate::llm::gateways::openai_messages_adapter::{adapt_messages_to_openai, convert_tool_calls};
use crate::llm::gateways::openai_model_registry::{get_model_registry, ModelType};
use crate::llm::gateways::openai_stream_events::OpenAiEventParser;
use crate::llm::models::{LlmGatewayResponse, LlmMessage, ResponseEvidence};
use crate::llm::stream_events::{drive_event_stream, StreamEventError, StreamEventStream};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use futures::stream::Stream;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use tracing::{debug, info, warn};

/// Configuration for connecting to OpenAI API.
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub timeout: Option<std::time::Duration>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            base_url: std::env::var("OPENAI_API_ENDPOINT")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            timeout: None,
        }
    }
}

/// Gateway for OpenAI LLM service.
///
/// This gateway provides access to OpenAI models through their API,
/// supporting text generation, structured output, tool calling, and embeddings.
pub struct OpenAIGateway {
    client: Client,
    config: OpenAIConfig,
}

impl OpenAIGateway {
    /// Create a new OpenAI gateway with default configuration.
    pub fn new() -> Self {
        Self::with_config(OpenAIConfig::default())
    }

    /// Create a new OpenAI gateway with custom configuration.
    pub fn with_config(config: OpenAIConfig) -> Self {
        let mut client_builder = Client::builder();

        if let Some(timeout) = config.timeout {
            client_builder = client_builder.timeout(timeout);
        }

        let client = client_builder.build().unwrap();

        Self { client, config }
    }

    /// Create gateway with custom API key.
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self::with_config(OpenAIConfig {
            api_key: api_key.into(),
            ..Default::default()
        })
    }

    /// Create gateway with custom API key and base URL.
    pub fn with_api_key_and_base_url(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self::with_config(OpenAIConfig {
            api_key: api_key.into(),
            base_url: base_url.into(),
            ..Default::default()
        })
    }

    /// Adapt parameters based on model type and capabilities.
    fn adapt_parameters_for_model(
        &self,
        model: &str,
        config: &CompletionConfig,
    ) -> (HashMap<String, Value>, bool) {
        let registry = get_model_registry();
        let capabilities = registry.get_model_capabilities(model);

        let mut params = HashMap::new();

        debug!(
            model = model,
            model_type = ?capabilities.model_type,
            supports_tools = capabilities.supports_tools,
            supports_streaming = capabilities.supports_streaming,
            "Adapting parameters for model"
        );

        // Handle token limit parameter conversion
        let max_tokens = if config.max_tokens > 0 {
            config.max_tokens
        } else if let Some(np) = config.num_predict {
            np as usize
        } else {
            16384
        };

        if capabilities.model_type == ModelType::Reasoning {
            params.insert("max_completion_tokens".to_string(), serde_json::json!(max_tokens));
        } else {
            params.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
        }

        // Handle temperature restrictions
        if capabilities.supports_temperature(config.temperature) {
            params.insert("temperature".to_string(), serde_json::json!(config.temperature));
        } else if capabilities.supported_temperatures.as_ref().is_some_and(|t| t.is_empty()) {
            // Model doesn't support temperature at all - don't add it
            warn!(
                model = model,
                requested_temperature = config.temperature,
                "Model does not support temperature parameter at all"
            );
        } else {
            // Use default temperature
            warn!(
                model = model,
                requested_temperature = config.temperature,
                default_temperature = 1.0,
                "Model does not support requested temperature, using default"
            );
            params.insert("temperature".to_string(), serde_json::json!(1.0));
        }

        // Add optional sampling parameters
        if let Some(top_p) = config.top_p {
            params.insert("top_p".to_string(), serde_json::json!(top_p));
        }

        // Handle reasoning effort for reasoning models
        if let Some(reasoning_effort) = config
            .reasoning_effort
            .filter(|effort| *effort != crate::llm::gateway::ReasoningEffort::Disabled)
        {
            if capabilities.model_type == ModelType::Reasoning {
                use crate::llm::gateway::ReasoningEffort;
                let effort_str = match reasoning_effort {
                    ReasoningEffort::Disabled => unreachable!("disabled effort was filtered"),
                    ReasoningEffort::Low => "low",
                    ReasoningEffort::Medium => "medium",
                    ReasoningEffort::High => "high",
                };
                params.insert("reasoning_effort".to_string(), serde_json::json!(effort_str));
            } else {
                warn!(
                    model = model,
                    "reasoning_effort specified but model is not a reasoning model, ignoring"
                );
            }
        }

        (params, capabilities.supports_tools)
    }

    /// Build a chat completion body with adapted parameters and response format.
    ///
    /// Returns the body and whether the model supports tools. Callers add
    /// tools and streaming fields.
    fn chat_body(
        &self,
        model: &str,
        messages: &[LlmMessage],
        config: &CompletionConfig,
    ) -> Result<(Value, bool)> {
        let openai_messages = adapt_messages_to_openai(messages)?;
        let (adapted_params, supports_tools) = self.adapt_parameters_for_model(model, config);

        let mut body = serde_json::json!({
            "model": model,
            "messages": openai_messages,
        });
        for (key, value) in adapted_params {
            body[key] = value;
        }
        add_response_format(&mut body, config);

        Ok((body, supports_tools))
    }

    /// A POST to the chat completions endpoint carrying `body`.
    fn chat_request(&self, body: &Value) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(body)
    }

    /// Split cl100k_base tokens into bounded embedding inputs.
    fn chunk_text(&self, text: &str, chunk_size: usize) -> Result<Vec<Vec<u32>>> {
        let tokenizer = tiktoken_rs::cl100k_base()
            .map_err(|error| MojenticError::GatewayError(format!("Tokenizer error: {error}")))?;
        let tokens = tokenizer.encode_ordinary(text);
        if tokens.is_empty() {
            return Ok(vec![vec![]]);
        }
        Ok(tokens.chunks(chunk_size).map(<[u32]>::to_vec).collect())
    }

    /// Calculate weighted average of embeddings.
    fn weighted_average_embeddings(&self, embeddings: &[Vec<f32>], weights: &[f32]) -> Vec<f32> {
        if embeddings.is_empty() {
            return vec![];
        }

        let dimension = embeddings[0].len();
        let total_weight: f32 = weights.iter().sum();

        // Build weighted sum for each dimension
        let average: Vec<f32> = (0..dimension)
            .map(|dim_idx| {
                embeddings
                    .iter()
                    .zip(weights.iter())
                    .map(|(embedding, &weight)| {
                        embedding.get(dim_idx).unwrap_or(&0.0) * (weight / total_weight)
                    })
                    .sum()
            })
            .collect();

        // Normalize
        let norm: f32 = average.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            average.iter().map(|x| x / norm).collect()
        } else {
            average
        }
    }
}

impl Default for OpenAIGateway {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmGateway for OpenAIGateway {
    async fn complete(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse> {
        info!("Delegating to OpenAI for completion");
        debug!("Model: {}, Message count: {}", model, messages.len());

        let (mut body, supports_tools) = self.chat_body(model, messages, config)?;

        // Add tools if provided and supported
        if let Some(tools) = tools {
            if supports_tools {
                let tool_defs: Vec<_> = tools.iter().map(|t| t.descriptor()).collect();
                body["tools"] = serde_json::to_value(tool_defs)?;
            } else {
                warn!(model = model, "Model does not support tools, ignoring tool configuration");
            }
        }

        // Make API request
        let response = self.chat_request(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(MojenticError::GatewayError(format!(
                "OpenAI API error: {} - {}",
                status, error_text
            )));
        }

        let response_body: Value = response.json().await?;

        // Parse content
        let content = response_body["choices"][0]["message"]["content"].as_str().map(String::from);

        // Parse tool calls if present
        let tool_calls =
            if let Some(calls) = response_body["choices"][0]["message"]["tool_calls"].as_array() {
                convert_tool_calls(calls)
            } else {
                vec![]
            };

        Ok(LlmGatewayResponse {
            content,
            object: None,
            tool_calls,
            thinking: None,
            evidence: openai_response_evidence(&response_body),
        })
    }

    async fn complete_json(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<Value> {
        self.complete_json_response(model, messages, schema, config)
            .await?
            .object
            .ok_or_else(|| MojenticError::GatewayError("No content in response".to_string()))
    }

    async fn complete_json_response(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse<Value>> {
        info!("Requesting structured output from OpenAI");

        let openai_messages = adapt_messages_to_openai(messages)?;
        let (adapted_params, _) = self.adapt_parameters_for_model(model, config);

        let mut body = serde_json::json!({
            "model": model,
            "messages": openai_messages,
            "response_format": openai_response_format(&ResponseFormat::JsonObject {
                schema: Some(schema)
            }),
        });

        // Add adapted parameters
        for (key, value) in adapted_params {
            body[key] = value;
        }

        let response = self.chat_request(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(MojenticError::GatewayError(format!(
                "OpenAI API error: {} - {}",
                status, error_text
            )));
        }

        let response_body: Value = response.json().await?;
        let content = response_body["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| MojenticError::GatewayError("No content in response".to_string()))?;

        // Parse the JSON response
        let object: Value = serde_json::from_str(content)?;

        Ok(LlmGatewayResponse {
            content: Some(content.to_string()),
            object: Some(object),
            evidence: openai_response_evidence(&response_body),
            ..Default::default()
        })
    }

    async fn get_available_models(&self) -> Result<Vec<String>> {
        debug!("Fetching available OpenAI models");

        let response = self
            .client
            .get(format!("{}/models", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(MojenticError::GatewayError(format!(
                "Failed to get models: {}",
                response.status()
            )));
        }

        let body: Value = response.json().await?;

        let mut models = body["data"]
            .as_array()
            .ok_or_else(|| MojenticError::GatewayError("Invalid response format".to_string()))?
            .iter()
            .filter_map(|m| m["id"].as_str().map(String::from))
            .collect::<Vec<_>>();

        models.sort();
        Ok(models)
    }

    async fn calculate_embeddings(&self, text: &str, model: Option<&str>) -> Result<Vec<f32>> {
        let model = model.unwrap_or("text-embedding-3-large");
        debug!("Calculating embeddings with model: {}", model);

        // Chunk the text to handle token limits
        let chunks = self.chunk_text(text, 8191)?;

        if chunks.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::new();
        let mut weights = Vec::new();

        for chunk in &chunks {
            let body = serde_json::json!({
                "model": model,
                "input": if chunks.len() == 1 { serde_json::json!(text) } else { serde_json::json!(chunk) }
            });

            let response = self
                .client
                .post(format!("{}/embeddings", self.config.base_url))
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(MojenticError::GatewayError(format!(
                    "Embeddings API error: {}",
                    response.status()
                )));
            }

            let response_body: Value = response.json().await?;

            let embedding: Vec<f32> = response_body["data"][0]["embedding"]
                .as_array()
                .ok_or_else(|| {
                    MojenticError::GatewayError("Invalid embeddings response".to_string())
                })?
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();

            weights.push(chunk.len() as f32);
            all_embeddings.push(embedding);
        }

        // If only one chunk, return it directly
        if all_embeddings.len() == 1 {
            return Ok(all_embeddings.remove(0));
        }

        // Calculate weighted average
        Ok(self.weighted_average_embeddings(&all_embeddings, &weights))
    }

    fn complete_stream<'a>(
        &'a self,
        model: &'a str,
        messages: &'a [LlmMessage],
        tools: Option<&'a [Box<dyn LlmTool>]>,
        config: &'a CompletionConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
        Box::pin(async_stream::stream! {
            info!("Starting OpenAI streaming completion");
            debug!("Model: {}, Message count: {}", model, messages.len());

            // Check if model supports streaming
            let registry = get_model_registry();
            let capabilities = registry.get_model_capabilities(model);
            if !capabilities.supports_streaming {
                yield Err(MojenticError::GatewayError(format!(
                    "Model {} does not support streaming",
                    model
                )));
                return;
            }

            let (mut body, supports_tools) = match self.chat_body(model, messages, config) {
                Ok(built) => built,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };
            body["stream"] = serde_json::json!(true);

            // Add tools if provided and supported
            if let Some(tools) = tools {
                if supports_tools {
                    let tool_defs: Vec<_> = tools.iter().map(|t| t.descriptor()).collect();
                    if let Ok(tools_value) = serde_json::to_value(tool_defs) {
                        body["tools"] = tools_value;
                    }
                }
            }

            // Make streaming API request
            let response = match self.chat_request(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    yield Err(e.into());
                    return;
                }
            };

            if !response.status().is_success() {
                yield Err(MojenticError::GatewayError(format!(
                    "OpenAI API error: {}",
                    response.status()
                )));
                return;
            }

            let mut parser = OpenAiLegacyParser::default();
            let chunks = legacy_body_stream(response.bytes_stream(), move |line: &str| {
                parser.parse_line(line)
            });
            for await chunk in chunks {
                yield chunk;
            }
        })
    }

    fn complete_stream_events<'a>(
        &'a self,
        model: &'a str,
        messages: &'a [LlmMessage],
        config: &'a CompletionConfig,
    ) -> std::result::Result<StreamEventStream<'a>, StreamEventError> {
        info!("Starting OpenAI stream events");
        let (mut body, _) = self
            .chat_body(model, messages, config)
            .map_err(StreamEventError::RequestFailed)?;
        body["stream"] = serde_json::json!(true);
        body["stream_options"] = serde_json::json!({"include_usage": true});

        Ok(drive_event_stream(self.chat_request(&body), OpenAiEventParser::default()))
    }
}

/// Response fields OpenAI reports that are kept as evidence metadata.
const OPENAI_METADATA_FIELDS: [&str; 4] = ["id", "created", "system_fingerprint", "service_tier"];

/// Read the evidence a chat completion body reports, leaving absent values unknown.
pub(crate) fn openai_response_evidence(body: &Value) -> ResponseEvidence {
    ResponseEvidence {
        usage: present(&body["usage"]),
        provider_model: body["model"].as_str().map(String::from),
        finish_reason: body["choices"][0]["finish_reason"].as_str().map(String::from),
        metadata: openai_metadata(body),
    }
}

/// Collect the reported metadata fields of a completion body or stream chunk.
pub(crate) fn openai_metadata(body: &Value) -> HashMap<String, Value> {
    OPENAI_METADATA_FIELDS
        .iter()
        .filter_map(|field| present(&body[*field]).map(|value| (field.to_string(), value)))
        .collect()
}

/// A JSON value the provider actually reported: neither missing nor null.
pub(crate) fn present(value: &Value) -> Option<Value> {
    (!value.is_null()).then(|| value.clone())
}

/// Map a configured response format to OpenAI's `response_format` request field.
pub(crate) fn openai_response_format(format: &ResponseFormat) -> Value {
    match format {
        ResponseFormat::Text => serde_json::json!({"type": "text"}),
        ResponseFormat::JsonObject { schema: None } => serde_json::json!({"type": "json_object"}),
        ResponseFormat::JsonObject {
            schema: Some(schema),
        } => serde_json::json!({
            "type": "json_schema",
            "json_schema": {"name": "response", "schema": schema}
        }),
    }
}

/// Forward the configured response format, leaving the body unchanged when none is set.
pub(crate) fn add_response_format(body: &mut Value, config: &CompletionConfig) {
    if let Some(format) = &config.response_format {
        body["response_format"] = openai_response_format(format);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_openai_config_default() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("OPENAI_API_ENDPOINT");
        let config = OpenAIConfig::default();
        assert_eq!(config.api_key, "");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert!(config.timeout.is_none());
    }

    #[test]
    fn test_openai_config_from_env() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("OPENAI_API_KEY", "test-key");
        std::env::set_var("OPENAI_API_ENDPOINT", "https://custom.openai.com");
        let config = OpenAIConfig::default();
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://custom.openai.com");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("OPENAI_API_ENDPOINT");
    }

    #[test]
    fn test_gateway_new() {
        let gateway = OpenAIGateway::new();
        assert_eq!(gateway.config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_gateway_with_api_key() {
        let gateway = OpenAIGateway::with_api_key("my-api-key");
        assert_eq!(gateway.config.api_key, "my-api-key");
    }

    #[test]
    fn test_gateway_with_api_key_and_base_url() {
        let gateway = OpenAIGateway::with_api_key_and_base_url("key", "https://custom.com");
        assert_eq!(gateway.config.api_key, "key");
        assert_eq!(gateway.config.base_url, "https://custom.com");
    }

    #[test]
    fn test_gateway_default() {
        let gateway = OpenAIGateway::default();
        assert_eq!(gateway.config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_chunk_text_short() {
        let gateway = OpenAIGateway::new();
        let chunks = gateway.chunk_text("Hello world", 100).expect("tokens");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 2);
    }

    #[test]
    fn test_chunk_text_long() {
        let gateway = OpenAIGateway::new();
        let long_text = "a".repeat(50000);
        let chunks = gateway.chunk_text(&long_text, 100).expect("tokens");
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 100));
    }

    #[test]
    fn test_weighted_average_embeddings_single() {
        let gateway = OpenAIGateway::new();
        let embeddings = vec![vec![1.0, 2.0, 3.0]];
        let weights = vec![1.0];
        let result = gateway.weighted_average_embeddings(&embeddings, &weights);

        // Normalized [1, 2, 3] / sqrt(14)
        let norm = (1.0_f32 + 4.0 + 9.0).sqrt();
        assert!((result[0] - 1.0 / norm).abs() < 0.001);
        assert!((result[1] - 2.0 / norm).abs() < 0.001);
        assert!((result[2] - 3.0 / norm).abs() < 0.001);
    }

    #[test]
    fn test_weighted_average_embeddings_multiple() {
        let gateway = OpenAIGateway::new();
        let embeddings = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let weights = vec![1.0, 1.0];
        let result = gateway.weighted_average_embeddings(&embeddings, &weights);

        // Equal weights, average is [0.5, 0.5], normalized to [1/sqrt(2), 1/sqrt(2)]
        let expected = 1.0 / (2.0_f32).sqrt();
        assert!((result[0] - expected).abs() < 0.001);
        assert!((result[1] - expected).abs() < 0.001);
    }

    #[test]
    fn test_weighted_average_embeddings_empty() {
        let gateway = OpenAIGateway::new();
        let embeddings: Vec<Vec<f32>> = vec![];
        let weights: Vec<f32> = vec![];
        let result = gateway.weighted_average_embeddings(&embeddings, &weights);
        assert!(result.is_empty());
    }

    #[test]
    fn test_adapt_parameters_chat_model() {
        let gateway = OpenAIGateway::new();
        let config = CompletionConfig {
            temperature: 0.7,
            max_tokens: 1000,
            ..Default::default()
        };

        let (params, supports_tools) = gateway.adapt_parameters_for_model("gpt-4", &config);

        assert!(params.contains_key("max_tokens"));
        assert!(!params.contains_key("max_completion_tokens"));
        assert!(supports_tools);
    }

    #[test]
    fn test_adapt_parameters_reasoning_model() {
        let gateway = OpenAIGateway::new();
        let config = CompletionConfig {
            temperature: 0.7,
            max_tokens: 1000,
            ..Default::default()
        };

        let (params, supports_tools) = gateway.adapt_parameters_for_model("o1", &config);

        assert!(!params.contains_key("max_tokens"));
        assert!(params.contains_key("max_completion_tokens"));
        assert!(supports_tools); // o1 now supports tools (audit 2026-02-04)
    }

    #[tokio::test]
    async fn test_complete_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"Hello!"}}]}"#)
            .create();

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();

        let result = gateway.complete("gpt-4", &messages, None, &config).await;

        mock.assert();
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, Some("Hello!".to_string()));
    }

    #[tokio::test]
    async fn test_complete_with_tool_calls() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"location\": \"NYC\"}"}}]}}]}"#)
            .create();

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Weather?")];
        let config = CompletionConfig::default();

        let result = gateway.complete("gpt-4", &messages, None, &config).await;

        mock.assert();
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "get_weather");
    }

    #[tokio::test]
    async fn test_complete_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(401)
            .with_body("Unauthorized")
            .create();

        let gateway = OpenAIGateway::with_api_key_and_base_url("bad-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();

        let result = gateway.complete("gpt-4", &messages, None, &config).await;

        mock.assert();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complete_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(
                r#"{"choices":[{"message":{"content":"{\"name\":\"test\",\"value\":42}"}}]}"#,
            )
            .create();

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Generate JSON")];
        let schema = serde_json::json!({"type": "object"});
        let config = CompletionConfig::default();

        let result = gateway.complete_json("gpt-4", &messages, schema, &config).await;

        mock.assert();
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["value"], 42);
    }

    #[tokio::test]
    async fn test_get_available_models() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/models")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"gpt-4"},{"id":"gpt-3.5-turbo"}]}"#)
            .create();

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let result = gateway.get_available_models().await;

        mock.assert();
        assert!(result.is_ok());
        let models = result.unwrap();
        assert_eq!(models.len(), 2);
        // Should be sorted
        assert_eq!(models[0], "gpt-3.5-turbo");
        assert_eq!(models[1], "gpt-4");
    }

    #[tokio::test]
    async fn test_calculate_embeddings() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/embeddings")
            .with_status(200)
            .with_body(r#"{"data":[{"embedding":[0.1,0.2,0.3,0.4]}]}"#)
            .create();

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let result = gateway.calculate_embeddings("test text", None).await;

        mock.assert();
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[tokio::test]
    async fn test_calculate_embeddings_weights_by_token_count() {
        let text = " hello".repeat(8291);
        let tokenizer = tiktoken_rs::cl100k_base().expect("tokenizer");
        assert_eq!(tokenizer.encode_ordinary(&text).len(), 8291);
        let mut server = mockito::Server::new_async().await;
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let chunk_lengths = std::sync::Arc::new(Mutex::new(Vec::new()));
        let captured_lengths = chunk_lengths.clone();
        let mock = server
            .mock("POST", "/embeddings")
            .with_status(200)
            .with_body_from_request(move |request| {
                let body: Value = serde_json::from_slice(request.body().expect("request body"))
                    .expect("JSON body");
                captured_lengths
                    .lock()
                    .expect("lengths")
                    .push(body["input"].as_array().expect("token input").len());
                let embedding = if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                };
                serde_json::json!({"data": [{"embedding": embedding}]}).to_string().into_bytes()
            })
            .expect(2)
            .create();
        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let result = gateway.calculate_embeddings(&text, None).await.expect("embeddings");
        mock.assert();
        assert_eq!(*chunk_lengths.lock().expect("lengths"), vec![8191, 100]);
        let norm = (8191.0_f32.powi(2) + 100.0_f32.powi(2)).sqrt();
        assert!((result[0] - 8191.0 / norm).abs() < 1e-6, "{result:?}");
        assert!((result[1] - 100.0 / norm).abs() < 1e-6, "{result:?}");
        assert!((result.iter().map(|v| v * v).sum::<f32>().sqrt() - 1.0).abs() < 1e-6);
    }

    /// Send one streaming request and return the JSON body the server received.
    async fn streamed_request_body(config: CompletionConfig) -> Value {
        let captured = std::sync::Arc::new(Mutex::new(None::<Value>));
        let sink = captured.clone();
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_request(move |request| {
                let body = serde_json::from_slice(request.body().expect("request body"))
                    .expect("JSON request body");
                *sink.lock().unwrap() = Some(body);
                true
            })
            .with_status(200)
            .with_body("data: [DONE]\n\n")
            .create_async()
            .await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let mut stream = gateway.complete_stream("gpt-4o", &messages, None, &config);
        while stream.next().await.is_some() {}

        mock.assert_async().await;
        let body = captured.lock().unwrap().take();
        body.expect("streaming request body was captured")
    }

    /// Stream `body` through the legacy streaming API and collect every item.
    async fn legacy_stream_items(body: &str) -> Vec<Result<StreamChunk>> {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .create_async()
            .await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();
        gateway.complete_stream("gpt-4o", &messages, None, &config).collect().await
    }

    #[tokio::test]
    async fn legacy_stream_yields_non_empty_content_and_skips_unreadable_frames() {
        let items = legacy_stream_items(concat!(
            ": keep-alive\n\n",
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {not json\n\n",
            "data: {\"choices\":[]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        ))
        .await;

        let texts: Vec<_> = items
            .iter()
            .map(|item| match item {
                Ok(StreamChunk::Content(text)) => text.clone(),
                other => panic!("expected only content, got {other:?}"),
            })
            .collect();
        assert_eq!(texts, ["Hel", "lo"]);
    }

    #[tokio::test]
    async fn legacy_stream_accumulates_tool_call_fragments_until_the_finish_reason() {
        let items = legacy_stream_items(concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"location\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\" \\\"NYC\\\"}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ))
        .await;

        let [Ok(StreamChunk::ToolCalls(calls))] = items.as_slice() else {
            panic!("expected one tool-calls chunk, got {items:?}");
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments["location"], "NYC");
    }

    #[tokio::test]
    async fn legacy_stream_yields_pending_tool_calls_at_done() {
        let items = legacy_stream_items(concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_b\",\"function\":{\"name\":\"b\",\"arguments\":\"{}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"function\":{\"name\":\"a\",\"arguments\":\"{}\"}}]}}]}\n\n",
            "data: [DONE]\n\n",
        ))
        .await;

        let [Ok(StreamChunk::ToolCalls(calls))] = items.as_slice() else {
            panic!("expected one tool-calls chunk, got {items:?}");
        };
        let names: Vec<_> = calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[tokio::test]
    async fn legacy_stream_keeps_a_character_split_across_network_chunks() {
        use crate::llm::stream_events::testing::{
            split_body_server, split_inside_first_multibyte_char,
        };
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Caf\u{e9} \u{1f30a}\"}}]}\n\n",
            "data: [DONE]\n\n",
        );
        let url = split_body_server(split_inside_first_multibyte_char(body)).await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", url);
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();
        let items: Vec<_> =
            gateway.complete_stream("gpt-4o", &messages, None, &config).collect().await;

        assert!(
            matches!(items.as_slice(), [Ok(StreamChunk::Content(text))] if text == "Caf\u{e9} \u{1f30a}"),
            "{items:?}"
        );
    }

    #[tokio::test]
    async fn legacy_stream_reports_http_error_status() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(500)
            .with_body("boom")
            .create_async()
            .await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();
        let items: Vec<_> =
            gateway.complete_stream("gpt-4o", &messages, None, &config).collect().await;

        assert!(matches!(
            items.as_slice(),
            [Err(MojenticError::GatewayError(message))] if message == "OpenAI API error: 500 Internal Server Error"
        ));
    }

    fn config_with_format(format: Option<ResponseFormat>) -> CompletionConfig {
        CompletionConfig {
            response_format: format,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn streaming_request_without_format_leaves_body_unchanged() {
        let body = streamed_request_body(config_with_format(None)).await;

        assert_eq!(body["stream"], true);
        assert!(body.get("response_format").is_none());
    }

    #[tokio::test]
    async fn streaming_request_forwards_text_format() {
        let body = streamed_request_body(config_with_format(Some(ResponseFormat::Text))).await;

        assert_eq!(body["response_format"], serde_json::json!({"type": "text"}));
    }

    #[tokio::test]
    async fn streaming_request_forwards_json_object_format() {
        let body = streamed_request_body(config_with_format(Some(ResponseFormat::JsonObject {
            schema: None,
        })))
        .await;

        assert_eq!(body["response_format"], serde_json::json!({"type": "json_object"}));
    }

    #[tokio::test]
    async fn streaming_request_forwards_json_schema_format() {
        let schema =
            serde_json::json!({"type": "object", "properties": {"n": {"type": "integer"}}});
        let body = streamed_request_body(config_with_format(Some(ResponseFormat::JsonObject {
            schema: Some(schema.clone()),
        })))
        .await;

        assert_eq!(
            body["response_format"],
            serde_json::json!({
                "type": "json_schema",
                "json_schema": {"name": "response", "schema": schema}
            })
        );
    }

    #[tokio::test]
    async fn non_streaming_request_forwards_configured_format() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "response_format": {"type": "json_object"}
            })))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"{}"}}]}"#)
            .create_async()
            .await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = config_with_format(Some(ResponseFormat::JsonObject { schema: None }));

        gateway.complete("gpt-4o", &messages, None, &config).await.unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn complete_carries_reported_evidence_unchanged() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(
                r#"{"id":"chatcmpl-9","object":"chat.completion","created":1727000000,
                    "model":"gpt-4o-2024-08-06","system_fingerprint":"fp_1",
                    "choices":[{"index":0,"message":{"role":"assistant","content":"Hi"},"finish_reason":"length"}],
                    "usage":{"prompt_tokens":4,"completion_tokens":1,"total_tokens":5,
                             "completion_tokens_details":{"reasoning_tokens":0}}}"#,
            )
            .create_async()
            .await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];

        let response = gateway
            .complete("gpt-4o", &messages, None, &CompletionConfig::default())
            .await
            .unwrap();

        mock.assert_async().await;
        let evidence = response.evidence;
        assert_eq!(evidence.provider_model.as_deref(), Some("gpt-4o-2024-08-06"));
        assert_eq!(evidence.finish_reason.as_deref(), Some("length"));
        assert_eq!(
            evidence.usage,
            Some(serde_json::json!({"prompt_tokens":4,"completion_tokens":1,"total_tokens":5,
                                    "completion_tokens_details":{"reasoning_tokens":0}}))
        );
        assert_eq!(evidence.metadata["id"], "chatcmpl-9");
        assert_eq!(evidence.metadata["system_fingerprint"], "fp_1");
        assert_eq!(evidence.metadata["created"], 1727000000);
    }

    #[tokio::test]
    async fn complete_leaves_unreported_usage_unknown() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"Hello!"}}]}"#)
            .create_async()
            .await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("Hi")];

        let response = gateway
            .complete("gpt-4o", &messages, None, &CompletionConfig::default())
            .await
            .unwrap();

        mock.assert_async().await;
        assert_eq!(response.evidence, crate::llm::models::ResponseEvidence::default());
    }

    #[tokio::test]
    async fn complete_json_response_carries_object_and_evidence() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(
                r#"{"model":"gpt-4o-mini","choices":[{"message":{"content":"{\"n\":1}"},"finish_reason":"stop"}],
                    "usage":{"prompt_tokens":2,"completion_tokens":3}}"#,
            )
            .create_async()
            .await;

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let messages = vec![LlmMessage::user("JSON")];

        let response = gateway
            .complete_json_response(
                "gpt-4o",
                &messages,
                serde_json::json!({"type": "object"}),
                &CompletionConfig::default(),
            )
            .await
            .unwrap();

        mock.assert_async().await;
        assert_eq!(response.object, Some(serde_json::json!({"n": 1})));
        assert_eq!(response.content.as_deref(), Some(r#"{"n":1}"#));
        assert_eq!(response.evidence.provider_model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(response.evidence.finish_reason.as_deref(), Some("stop"));
        assert_eq!(
            response.evidence.usage,
            Some(serde_json::json!({"prompt_tokens":2,"completion_tokens":3}))
        );
    }

    mod stream_events {
        use super::*;
        use crate::llm::stream_events::testing::endless_stream_server;
        use crate::llm::{StreamEvent, StreamEventError};

        async fn collect_events(
            server: &mockito::Server,
            config: &CompletionConfig,
        ) -> Vec<StreamEvent> {
            let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
            let messages = vec![LlmMessage::user("Hi")];
            let stream = gateway
                .complete_stream_events("gpt-4o", &messages, config)
                .expect("OpenAI supports stream events");
            stream.collect().await
        }

        #[tokio::test]
        async fn request_streams_without_tools_and_asks_for_usage() {
            let captured = std::sync::Arc::new(Mutex::new(None::<Value>));
            let sink = captured.clone();
            let mut server = mockito::Server::new_async().await;
            let mock = server
                .mock("POST", "/chat/completions")
                .match_header("authorization", "Bearer test-key")
                .match_request(move |request| {
                    *sink.lock().unwrap() =
                        serde_json::from_slice(request.body().expect("request body")).ok();
                    true
                })
                .with_status(200)
                .with_body("data: [DONE]\n\n")
                .create_async()
                .await;
            let config = CompletionConfig {
                response_format: Some(ResponseFormat::JsonObject { schema: None }),
                ..Default::default()
            };

            collect_events(&server, &config).await;

            mock.assert_async().await;
            let body = captured.lock().unwrap().take().expect("captured body");
            assert_eq!(body["model"], "gpt-4o");
            assert_eq!(body["stream"], true);
            assert_eq!(body["stream_options"], serde_json::json!({"include_usage": true}));
            assert_eq!(body["response_format"], serde_json::json!({"type": "json_object"}));
            assert!(body.get("tools").is_none());
        }

        #[tokio::test]
        async fn completes_over_http_with_usage() {
            let mut server = mockito::Server::new_async().await;
            let mock = server
                .mock("POST", "/chat/completions")
                .with_status(200)
                .with_header("content-type", "text/event-stream")
                .with_body(concat!(
                    "data: {\"model\":\"gpt-4o-x\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
                    "data: {\"model\":\"gpt-4o-x\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                    "data: {\"model\":\"gpt-4o-x\",\"choices\":[],\"usage\":{\"total_tokens\":3}}\n\n",
                    "data: [DONE]\n\n",
                ))
                .create_async()
                .await;

            let events = collect_events(&server, &CompletionConfig::default()).await;

            mock.assert_async().await;
            assert!(matches!(
                events.as_slice(),
                [StreamEvent::Content(text), StreamEvent::Completed(evidence)]
                    if text == "Hi"
                        && evidence.usage == Some(serde_json::json!({"total_tokens": 3}))
                        && evidence.provider_model.as_deref() == Some("gpt-4o-x")
            ));
        }

        #[tokio::test]
        async fn end_of_stream_without_done_is_incomplete_stream() {
            let mut server = mockito::Server::new_async().await;
            let _mock = server
                .mock("POST", "/chat/completions")
                .with_status(200)
                .with_body(concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                ))
                .create_async()
                .await;

            let events = collect_events(&server, &CompletionConfig::default()).await;

            assert!(matches!(
                events.as_slice(),
                [
                    StreamEvent::Content(_),
                    StreamEvent::Error(StreamEventError::IncompleteStream(_))
                ]
            ));
        }

        #[tokio::test]
        async fn http_error_status_is_reported_with_body() {
            let mut server = mockito::Server::new_async().await;
            let _mock = server
                .mock("POST", "/chat/completions")
                .with_status(429)
                .with_body(r#"{"error":{"message":"slow down"}}"#)
                .create_async()
                .await;

            let events = collect_events(&server, &CompletionConfig::default()).await;

            assert!(matches!(
                events.as_slice(),
                [StreamEvent::Error(StreamEventError::ProviderError { status: Some(429), error })]
                    if error["message"] == "slow down"
            ));
        }

        #[tokio::test]
        async fn dropping_the_stream_cancels_the_request() {
            let (url, closed) =
                endless_stream_server("data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n")
                    .await;
            let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", url);
            let messages = vec![LlmMessage::user("Hi")];
            let config = CompletionConfig::default();

            let mut stream =
                gateway.complete_stream_events("gpt-4o", &messages, &config).expect("supported");
            assert!(matches!(stream.next().await, Some(StreamEvent::Content(_))));
            drop(stream);

            tokio::time::timeout(std::time::Duration::from_secs(5), closed)
                .await
                .expect("the request was not cancelled")
                .expect("server reported the disconnect");
        }
    }

    #[tokio::test]
    async fn test_calculate_embeddings_custom_model() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/embeddings")
            .match_body(mockito::Matcher::JsonString(
                r#"{"model":"text-embedding-3-small","input":"test"}"#.to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"data":[{"embedding":[0.5,0.6]}]}"#)
            .create();

        let gateway = OpenAIGateway::with_api_key_and_base_url("test-key", server.url());
        let result = gateway.calculate_embeddings("test", Some("text-embedding-3-small")).await;

        mock.assert();
        assert!(result.is_ok());
    }
}
