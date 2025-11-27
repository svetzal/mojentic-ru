//! OpenAI Gateway for LLM interactions.
//!
//! This module provides a gateway for interacting with OpenAI's API,
//! including chat completions, streaming, and embeddings.

use crate::error::{MojenticError, Result};
use crate::llm::gateway::{CompletionConfig, LlmGateway, StreamChunk};
use crate::llm::gateways::openai_messages_adapter::{adapt_messages_to_openai, convert_tool_calls};
use crate::llm::gateways::openai_model_registry::{get_model_registry, ModelType};
use crate::llm::models::{LlmGatewayResponse, LlmMessage, LlmToolCall};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
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

        (params, capabilities.supports_tools)
    }

    /// Chunk tokens for embedding calculation.
    fn chunk_text(&self, text: &str, chunk_size: usize) -> Vec<String> {
        // Simple character-based chunking as a fallback
        // In production, you'd use a proper tokenizer
        let chars: Vec<char> = text.chars().collect();
        let avg_chars_per_token = 4; // Rough estimate
        let max_chars = chunk_size * avg_chars_per_token;

        if chars.len() <= max_chars {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < chars.len() {
            let end = std::cmp::min(start + max_chars, chars.len());
            let chunk: String = chars[start..end].iter().collect();
            chunks.push(chunk);
            start = end;
        }

        chunks
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

        let openai_messages = adapt_messages_to_openai(messages)?;
        let (adapted_params, supports_tools) = self.adapt_parameters_for_model(model, config);

        let mut body = serde_json::json!({
            "model": model,
            "messages": openai_messages,
        });

        // Add adapted parameters
        for (key, value) in adapted_params {
            body[key] = value;
        }

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
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

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
        })
    }

    async fn complete_json(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<Value> {
        info!("Requesting structured output from OpenAI");

        let openai_messages = adapt_messages_to_openai(messages)?;
        let (adapted_params, _) = self.adapt_parameters_for_model(model, config);

        let mut body = serde_json::json!({
            "model": model,
            "messages": openai_messages,
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "schema": schema
                }
            }
        });

        // Add adapted parameters
        for (key, value) in adapted_params {
            body[key] = value;
        }

        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

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
        let json_value: Value = serde_json::from_str(content)?;

        Ok(json_value)
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
        let chunks = self.chunk_text(text, 8191);

        if chunks.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::new();
        let mut weights = Vec::new();

        for chunk in &chunks {
            let body = serde_json::json!({
                "model": model,
                "input": chunk
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

            weights.push(embedding.len() as f32);
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

            let openai_messages = match adapt_messages_to_openai(messages) {
                Ok(msgs) => msgs,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            let (adapted_params, supports_tools) = self.adapt_parameters_for_model(model, config);

            let mut body = serde_json::json!({
                "model": model,
                "messages": openai_messages,
                "stream": true
            });

            // Add adapted parameters
            for (key, value) in adapted_params {
                body[key] = value;
            }

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
            let response = match self
                .client
                .post(format!("{}/chat/completions", self.config.base_url))
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
            {
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

            // Process SSE stream
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            // Accumulate tool calls as they stream in
            let mut tool_calls_accumulator: HashMap<usize, ToolCallAccumulator> = HashMap::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            buffer.push_str(text);

                            // Process complete SSE lines
                            while let Some(line_end) = buffer.find('\n') {
                                let line = buffer[..line_end].trim().to_string();
                                buffer = buffer[line_end + 1..].to_string();

                                if line.is_empty() || !line.starts_with("data: ") {
                                    continue;
                                }

                                let data = line.strip_prefix("data: ").unwrap();

                                if data == "[DONE]" {
                                    // Final chunk - yield accumulated tool calls if any
                                    if !tool_calls_accumulator.is_empty() {
                                        let complete_tool_calls = build_complete_tool_calls(&tool_calls_accumulator);
                                        if !complete_tool_calls.is_empty() {
                                            yield Ok(StreamChunk::ToolCalls(complete_tool_calls));
                                        }
                                    }
                                    continue;
                                }

                                // Parse JSON data
                                match serde_json::from_str::<Value>(data) {
                                    Ok(json) => {
                                        if let Some(choices) = json["choices"].as_array() {
                                            if choices.is_empty() {
                                                continue;
                                            }

                                            let delta = &choices[0]["delta"];
                                            let finish_reason = choices[0]["finish_reason"].as_str();

                                            // Yield content chunks
                                            if let Some(content) = delta["content"].as_str() {
                                                if !content.is_empty() {
                                                    yield Ok(StreamChunk::Content(content.to_string()));
                                                }
                                            }

                                            // Accumulate tool call chunks
                                            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                                                for tc in tool_calls {
                                                    if let Some(index) = tc["index"].as_u64() {
                                                        let index = index as usize;

                                                        // Initialize accumulator if needed
                                                        let acc = tool_calls_accumulator.entry(index).or_insert_with(|| ToolCallAccumulator {
                                                            id: None,
                                                            name: None,
                                                            arguments: String::new(),
                                                        });

                                                        // First chunk has id
                                                        if let Some(id) = tc["id"].as_str() {
                                                            acc.id = Some(id.to_string());
                                                        }

                                                        // First chunk has function name
                                                        if let Some(name) = tc["function"]["name"].as_str() {
                                                            acc.name = Some(name.to_string());
                                                        }

                                                        // All chunks may have argument fragments
                                                        if let Some(args) = tc["function"]["arguments"].as_str() {
                                                            acc.arguments.push_str(args);
                                                        }
                                                    }
                                                }
                                            }

                                            // When stream completes with tool_calls, yield accumulated tool calls
                                            if finish_reason == Some("tool_calls") && !tool_calls_accumulator.is_empty() {
                                                let complete_tool_calls = build_complete_tool_calls(&tool_calls_accumulator);
                                                if !complete_tool_calls.is_empty() {
                                                    yield Ok(StreamChunk::ToolCalls(complete_tool_calls));
                                                }
                                                tool_calls_accumulator.clear();
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse streaming chunk: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(e.into());
                        return;
                    }
                }
            }
        })
    }
}

/// Accumulator for streaming tool calls.
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

/// Build complete tool calls from accumulators.
fn build_complete_tool_calls(
    accumulators: &HashMap<usize, ToolCallAccumulator>,
) -> Vec<LlmToolCall> {
    let mut indices: Vec<_> = accumulators.keys().collect();
    indices.sort();

    indices
        .iter()
        .filter_map(|&&index| {
            let acc = accumulators.get(&index)?;
            let name = acc.name.clone()?;

            // Parse arguments
            let arguments: HashMap<String, Value> =
                serde_json::from_str(&acc.arguments).unwrap_or_default();

            Some(LlmToolCall {
                id: acc.id.clone(),
                name,
                arguments,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_config_default() {
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("OPENAI_API_ENDPOINT");
        let config = OpenAIConfig::default();
        assert_eq!(config.api_key, "");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert!(config.timeout.is_none());
    }

    #[test]
    fn test_openai_config_from_env() {
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
        let chunks = gateway.chunk_text("Hello world", 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn test_chunk_text_long() {
        let gateway = OpenAIGateway::new();
        let long_text = "a".repeat(50000);
        let chunks = gateway.chunk_text(&long_text, 100);
        assert!(chunks.len() > 1);
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
    fn test_build_complete_tool_calls() {
        let mut accumulators = HashMap::new();
        accumulators.insert(
            0,
            ToolCallAccumulator {
                id: Some("call_123".to_string()),
                name: Some("get_weather".to_string()),
                arguments: r#"{"location": "NYC"}"#.to_string(),
            },
        );
        accumulators.insert(
            1,
            ToolCallAccumulator {
                id: Some("call_456".to_string()),
                name: Some("search".to_string()),
                arguments: r#"{"query": "test"}"#.to_string(),
            },
        );

        let result = build_complete_tool_calls(&accumulators);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, Some("call_123".to_string()));
        assert_eq!(result[0].name, "get_weather");
        assert_eq!(result[1].id, Some("call_456".to_string()));
        assert_eq!(result[1].name, "search");
    }

    #[test]
    fn test_build_complete_tool_calls_missing_name() {
        let mut accumulators = HashMap::new();
        accumulators.insert(
            0,
            ToolCallAccumulator {
                id: Some("call_123".to_string()),
                name: None, // Missing name
                arguments: r#"{}"#.to_string(),
            },
        );

        let result = build_complete_tool_calls(&accumulators);
        assert!(result.is_empty()); // Should be filtered out
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
        assert!(!supports_tools); // o1 doesn't support tools
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
        assert_eq!(embeddings.len(), 4);
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
