use crate::error::{MojenticError, Result};
use crate::llm::gateway::{CompletionConfig, LlmGateway};
use crate::llm::models::{LlmGatewayResponse, LlmMessage, LlmToolCall, MessageRole};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info};

/// Configuration for connecting to Ollama server
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub host: String,
    pub timeout: Option<std::time::Duration>,
    pub headers: HashMap<String, String>,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: std::env::var("OLLAMA_HOST")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            timeout: None,
            headers: HashMap::new(),
        }
    }
}

/// Gateway for Ollama local LLM service
///
/// This gateway provides access to local LLM models through Ollama,
/// supporting text generation, structured output, tool calling, and embeddings.
pub struct OllamaGateway {
    client: Client,
    config: OllamaConfig,
}

impl OllamaGateway {
    /// Create a new Ollama gateway with default configuration
    pub fn new() -> Self {
        Self::with_config(OllamaConfig::default())
    }

    /// Create a new Ollama gateway with custom configuration
    pub fn with_config(config: OllamaConfig) -> Self {
        let mut client_builder = Client::builder();

        if let Some(timeout) = config.timeout {
            client_builder = client_builder.timeout(timeout);
        }

        let client = client_builder.build().unwrap();

        Self { client, config }
    }

    /// Create gateway with custom host
    pub fn with_host(host: impl Into<String>) -> Self {
        Self::with_config(OllamaConfig {
            host: host.into(),
            ..Default::default()
        })
    }

    /// Pull a model from Ollama library
    pub async fn pull_model(&self, model: &str) -> Result<()> {
        info!("Pulling Ollama model: {}", model);

        let response = self
            .client
            .post(format!("{}/api/pull", self.config.host))
            .json(&serde_json::json!({
                "name": model
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(MojenticError::GatewayError(format!(
                "Failed to pull model {}: {}",
                model,
                response.status()
            )));
        }

        Ok(())
    }
}

impl Default for OllamaGateway {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmGateway for OllamaGateway {
    async fn complete(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse> {
        info!("Delegating to Ollama for completion");
        debug!("Model: {}, Message count: {}", model, messages.len());

        let ollama_messages = adapt_messages_to_ollama(messages)?;
        let options = extract_ollama_options(config);

        let mut body = serde_json::json!({
            "model": model,
            "messages": ollama_messages,
            "options": options,
            "stream": false
        });

        // Add tools if provided
        if let Some(tools) = tools {
            let tool_defs: Vec<_> = tools.iter().map(|t| t.descriptor()).collect();
            body["tools"] = serde_json::to_value(tool_defs)?;
        }

        // Make API request
        let response = self
            .client
            .post(format!("{}/api/chat", self.config.host))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(MojenticError::GatewayError(format!(
                "Ollama API error: {}",
                response.status()
            )));
        }

        let response_body: Value = response.json().await?;

        // Parse content
        let content = response_body["message"]["content"].as_str().map(String::from);

        // Parse tool calls if present
        let tool_calls = if let Some(calls) = response_body["message"]["tool_calls"].as_array() {
            calls
                .iter()
                .filter_map(|call| {
                    let name = call["function"]["name"].as_str()?.to_string();
                    let args = call["function"]["arguments"].as_object()?;

                    let arguments: HashMap<String, Value> =
                        args.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

                    Some(LlmToolCall {
                        id: call["id"].as_str().map(String::from),
                        name,
                        arguments,
                    })
                })
                .collect()
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
        info!("Requesting structured output from Ollama");

        let ollama_messages = adapt_messages_to_ollama(messages)?;
        let options = extract_ollama_options(config);

        let body = serde_json::json!({
            "model": model,
            "messages": ollama_messages,
            "options": options,
            "format": schema,
            "stream": false
        });

        let response = self
            .client
            .post(format!("{}/api/chat", self.config.host))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(MojenticError::GatewayError(format!(
                "Ollama API error: {}",
                response.status()
            )));
        }

        let response_body: Value = response.json().await?;
        let content = response_body["message"]["content"]
            .as_str()
            .ok_or_else(|| MojenticError::GatewayError("No content in response".to_string()))?;

        // Parse the JSON response
        let json_value: Value = serde_json::from_str(content)?;

        Ok(json_value)
    }

    async fn get_available_models(&self) -> Result<Vec<String>> {
        debug!("Fetching available Ollama models");

        let response = self.client.get(format!("{}/api/tags", self.config.host)).send().await?;

        if !response.status().is_success() {
            return Err(MojenticError::GatewayError(format!(
                "Failed to get models: {}",
                response.status()
            )));
        }

        let body: Value = response.json().await?;

        let models = body["models"]
            .as_array()
            .ok_or_else(|| MojenticError::GatewayError("Invalid response format".to_string()))?
            .iter()
            .filter_map(|m| m["name"].as_str().map(String::from))
            .collect::<Vec<_>>();

        Ok(models)
    }

    async fn calculate_embeddings(&self, text: &str, model: Option<&str>) -> Result<Vec<f32>> {
        let model = model.unwrap_or("mxbai-embed-large");
        debug!("Calculating embeddings with model: {}", model);

        let body = serde_json::json!({
            "model": model,
            "prompt": text
        });

        let response = self
            .client
            .post(format!("{}/api/embeddings", self.config.host))
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

        let embeddings = response_body["embedding"]
            .as_array()
            .ok_or_else(|| MojenticError::GatewayError("Invalid embeddings response".to_string()))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        Ok(embeddings)
    }
}

// Message adapter for Ollama format
fn adapt_messages_to_ollama(messages: &[LlmMessage]) -> Result<Vec<Value>> {
    messages
        .iter()
        .map(|msg| {
            let mut ollama_msg = serde_json::json!({
                "role": match msg.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                },
                "content": msg.content.as_deref().unwrap_or("")
            });

            // Add images for user messages - Ollama requires base64-encoded images
            if let Some(image_paths) = &msg.image_paths {
                let encoded_images: Result<Vec<String>> = image_paths
                    .iter()
                    .map(|path| {
                        std::fs::read(path)
                            .map_err(|e| {
                                MojenticError::GatewayError(format!(
                                    "Failed to read image file {}: {}",
                                    path, e
                                ))
                            })
                            .map(|bytes| base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes))
                    })
                    .collect();

                ollama_msg["images"] = serde_json::to_value(encoded_images?)?;
            }

            // Add tool calls for assistant messages
            if let Some(tool_calls) = &msg.tool_calls {
                let calls: Vec<_> = tool_calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments
                            }
                        })
                    })
                    .collect();
                ollama_msg["tool_calls"] = serde_json::to_value(calls)?;
            }

            Ok(ollama_msg)
        })
        .collect()
}

// Extract Ollama-specific options from config
fn extract_ollama_options(config: &CompletionConfig) -> Value {
    let mut options = serde_json::json!({
        "temperature": config.temperature,
        "num_ctx": config.num_ctx,
    });

    if let Some(num_predict) = config.num_predict {
        if num_predict > 0 {
            options["num_predict"] = serde_json::json!(num_predict);
        }
    } else if config.max_tokens > 0 {
        options["num_predict"] = serde_json::json!(config.max_tokens);
    }

    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_config_default() {
        std::env::remove_var("OLLAMA_HOST");
        let config = OllamaConfig::default();
        assert_eq!(config.host, "http://localhost:11434");
        assert!(config.timeout.is_none());
        assert!(config.headers.is_empty());
    }

    #[test]
    fn test_ollama_config_from_env() {
        std::env::set_var("OLLAMA_HOST", "http://custom:8080");
        let config = OllamaConfig::default();
        assert_eq!(config.host, "http://custom:8080");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    fn test_ollama_config_custom() {
        let mut headers = HashMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());

        let config = OllamaConfig {
            host: "http://test:9999".to_string(),
            timeout: Some(std::time::Duration::from_secs(30)),
            headers,
        };

        assert_eq!(config.host, "http://test:9999");
        assert_eq!(config.timeout, Some(std::time::Duration::from_secs(30)));
        assert_eq!(config.headers.get("X-Custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_gateway_new() {
        let gateway = OllamaGateway::new();
        assert_eq!(gateway.config.host, "http://localhost:11434");
    }

    #[test]
    fn test_gateway_with_host() {
        let gateway = OllamaGateway::with_host("http://example.com:8080");
        assert_eq!(gateway.config.host, "http://example.com:8080");
    }

    #[test]
    fn test_gateway_with_config() {
        let config = OllamaConfig {
            host: "http://custom:5000".to_string(),
            timeout: Some(std::time::Duration::from_secs(60)),
            headers: HashMap::new(),
        };

        let gateway = OllamaGateway::with_config(config);
        assert_eq!(gateway.config.host, "http://custom:5000");
    }

    #[test]
    fn test_gateway_default() {
        let gateway = OllamaGateway::default();
        assert_eq!(gateway.config.host, "http://localhost:11434");
    }

    #[test]
    fn test_adapt_messages_to_ollama_simple() {
        let messages = vec![
            LlmMessage::system("You are helpful"),
            LlmMessage::user("Hello"),
            LlmMessage::assistant("Hi there"),
        ];

        let result = adapt_messages_to_ollama(&messages).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["role"], "system");
        assert_eq!(result[0]["content"], "You are helpful");
        assert_eq!(result[1]["role"], "user");
        assert_eq!(result[1]["content"], "Hello");
        assert_eq!(result[2]["role"], "assistant");
        assert_eq!(result[2]["content"], "Hi there");
    }

    #[test]
    fn test_adapt_messages_with_images() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create temporary test image files
        let mut temp_file1 = NamedTempFile::new().unwrap();
        let mut temp_file2 = NamedTempFile::new().unwrap();
        temp_file1.write_all(b"fake_image_data_1").unwrap();
        temp_file2.write_all(b"fake_image_data_2").unwrap();

        // Get paths as strings
        let path1 = temp_file1.path().to_string_lossy().to_string();
        let path2 = temp_file2.path().to_string_lossy().to_string();

        // Expected base64 encodings
        let expected_base64_1 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"fake_image_data_1",
        );
        let expected_base64_2 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"fake_image_data_2",
        );

        let messages = vec![LlmMessage::user("Describe this").with_images(vec![path1, path2])];

        let result = adapt_messages_to_ollama(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "user");
        // Images should be base64-encoded
        assert_eq!(result[0]["images"][0], expected_base64_1);
        assert_eq!(result[0]["images"][1], expected_base64_2);
    }

    #[test]
    fn test_adapt_messages_with_tool_calls() {
        let tool_call = LlmToolCall {
            id: Some("call_123".to_string()),
            name: "test_function".to_string(),
            arguments: {
                let mut map = HashMap::new();
                map.insert("arg1".to_string(), serde_json::json!("value1"));
                map
            },
        };

        let messages = vec![LlmMessage {
            role: MessageRole::Assistant,
            content: None,
            tool_calls: Some(vec![tool_call]),
            image_paths: None,
        }];

        let result = adapt_messages_to_ollama(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "assistant");
        assert_eq!(result[0]["tool_calls"][0]["type"], "function");
        assert_eq!(result[0]["tool_calls"][0]["function"]["name"], "test_function");
    }

    #[test]
    fn test_adapt_messages_empty_content() {
        let messages = vec![LlmMessage {
            role: MessageRole::User,
            content: None,
            tool_calls: None,
            image_paths: None,
        }];

        let result = adapt_messages_to_ollama(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["content"], "");
    }

    #[test]
    fn test_adapt_messages_tool_role() {
        let messages = vec![LlmMessage {
            role: MessageRole::Tool,
            content: Some("Tool result".to_string()),
            tool_calls: None,
            image_paths: None,
        }];

        let result = adapt_messages_to_ollama(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "tool");
        assert_eq!(result[0]["content"], "Tool result");
    }

    #[test]
    fn test_extract_ollama_options_basic() {
        let config = CompletionConfig {
            temperature: 0.7,
            num_ctx: 4096,
            max_tokens: 2048,
            num_predict: None,
        };

        let options = extract_ollama_options(&config);

        // Use as_f64 for floating point comparison
        assert!((options["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
        assert_eq!(options["num_ctx"], 4096);
        // max_tokens should be used as num_predict when num_predict is None
        assert_eq!(options["num_predict"], 2048);
    }

    #[test]
    fn test_extract_ollama_options_with_num_predict() {
        let config = CompletionConfig {
            temperature: 0.5,
            num_ctx: 2048,
            max_tokens: 1000,
            num_predict: Some(500),
        };

        let options = extract_ollama_options(&config);

        assert!((options["temperature"].as_f64().unwrap() - 0.5).abs() < 0.01);
        assert_eq!(options["num_ctx"], 2048);
        // num_predict takes precedence over max_tokens
        assert_eq!(options["num_predict"], 500);
    }

    #[test]
    fn test_extract_ollama_options_zero_num_predict() {
        let config = CompletionConfig {
            temperature: 1.0,
            num_ctx: 8192,
            max_tokens: 4096,
            num_predict: Some(0),
        };

        let options = extract_ollama_options(&config);

        assert!((options["temperature"].as_f64().unwrap() - 1.0).abs() < 0.01);
        assert_eq!(options["num_ctx"], 8192);
        // When num_predict is Some(0) (not > 0), num_predict field is not added
        // (the else-if branch only runs when num_predict is None)
        assert!(options.get("num_predict").is_none() || options["num_predict"].is_null());
    }

    #[test]
    fn test_extract_ollama_options_zero_max_tokens() {
        let config = CompletionConfig {
            temperature: 0.8,
            num_ctx: 1024,
            max_tokens: 0,
            num_predict: None,
        };

        let options = extract_ollama_options(&config);

        assert!((options["temperature"].as_f64().unwrap() - 0.8).abs() < 0.01);
        assert_eq!(options["num_ctx"], 1024);
        // When max_tokens is 0 and num_predict is None, num_predict field is not added
        // Check that it's either missing or null (not set in the options object)
        let num_predict = options.get("num_predict");
        assert!(num_predict.is_none() || num_predict.unwrap().is_null());
    }

    #[tokio::test]
    async fn test_pull_model_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/pull")
            .with_status(200)
            .with_body(r#"{"status":"success"}"#)
            .create();

        let gateway = OllamaGateway::with_host(server.url());
        let result = gateway.pull_model("llama2").await;

        mock.assert();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pull_model_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/pull").with_status(404).create();

        let gateway = OllamaGateway::with_host(server.url());
        let result = gateway.pull_model("nonexistent").await;

        mock.assert();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complete_simple() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_body(r#"{"message":{"role":"assistant","content":"Hello!"}}"#)
            .create();

        let gateway = OllamaGateway::with_host(server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();

        let result = gateway.complete("llama2", &messages, None, &config).await;

        mock.assert();
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, Some("Hello!".to_string()));
    }

    #[tokio::test]
    async fn test_complete_with_tools() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .match_body(mockito::Matcher::JsonString(
                r#"{"model":"llama2","messages":[{"role":"user","content":"Hi"}],"options":{"temperature":1.0,"num_ctx":32768,"num_predict":16384},"stream":false,"tools":[{"type":"function","function":{"name":"test_tool","description":"A test","parameters":{}}}]}"#.to_string()
            ))
            .with_status(200)
            .with_body(r#"{"message":{"role":"assistant","content":"Result"}}"#)
            .create();

        let gateway = OllamaGateway::with_host(server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();

        use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};

        struct MockTool;
        impl LlmTool for MockTool {
            fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
                Ok(serde_json::json!({}))
            }
            fn descriptor(&self) -> ToolDescriptor {
                ToolDescriptor {
                    r#type: "function".to_string(),
                    function: FunctionDescriptor {
                        name: "test_tool".to_string(),
                        description: "A test".to_string(),
                        parameters: serde_json::json!({}),
                    },
                }
            }
        }

        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(MockTool)];
        let result = gateway.complete("llama2", &messages, Some(&tools), &config).await;

        mock.assert();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_complete_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/chat").with_status(500).create();

        let gateway = OllamaGateway::with_host(server.url());
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();

        let result = gateway.complete("llama2", &messages, None, &config).await;

        mock.assert();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complete_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_body(r#"{"message":{"content":"{\"name\":\"test\",\"value\":42}"}}"#)
            .create();

        let gateway = OllamaGateway::with_host(server.url());
        let messages = vec![LlmMessage::user("Generate JSON")];
        let schema = serde_json::json!({"type": "object"});
        let config = CompletionConfig::default();

        let result = gateway.complete_json("llama2", &messages, schema, &config).await;

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
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_body(r#"{"models":[{"name":"llama2"},{"name":"mistral"}]}"#)
            .create();

        let gateway = OllamaGateway::with_host(server.url());
        let result = gateway.get_available_models().await;

        mock.assert();
        assert!(result.is_ok());
        let models = result.unwrap();
        assert_eq!(models.len(), 2);
        assert!(models.contains(&"llama2".to_string()));
        assert!(models.contains(&"mistral".to_string()));
    }

    #[tokio::test]
    async fn test_calculate_embeddings() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embeddings")
            .with_status(200)
            .with_body(r#"{"embedding":[0.1,0.2,0.3,0.4]}"#)
            .create();

        let gateway = OllamaGateway::with_host(server.url());
        let result = gateway.calculate_embeddings("test text", None).await;

        mock.assert();
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 4);
        assert_eq!(embeddings[0], 0.1);
        assert_eq!(embeddings[3], 0.4);
    }

    #[tokio::test]
    async fn test_calculate_embeddings_custom_model() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/embeddings")
            .match_body(mockito::Matcher::JsonString(
                r#"{"model":"custom-embed","prompt":"test"}"#.to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"embedding":[0.5,0.6]}"#)
            .create();

        let gateway = OllamaGateway::with_host(server.url());
        let result = gateway.calculate_embeddings("test", Some("custom-embed")).await;

        mock.assert();
        assert!(result.is_ok());
    }
}
