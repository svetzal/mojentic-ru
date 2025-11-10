use crate::error::Result;
use crate::llm::gateway::{CompletionConfig, LlmGateway};
use crate::llm::models::{LlmGatewayResponse, LlmMessage, MessageRole};
use crate::llm::tools::LlmTool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// Main interface for LLM interactions
pub struct LlmBroker {
    model: String,
    gateway: Arc<dyn LlmGateway>,
}

impl LlmBroker {
    /// Create a new LLM broker
    pub fn new(model: impl Into<String>, gateway: Arc<dyn LlmGateway>) -> Self {
        Self {
            model: model.into(),
            gateway,
        }
    }

    /// Generate text response from LLM
    pub async fn generate(
        &self,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: Option<CompletionConfig>,
    ) -> Result<String> {
        let config = config.unwrap_or_default();
        let current_messages = messages.to_vec();

        // Make initial LLM call
        let response = self
            .gateway
            .complete(&self.model, &current_messages, tools, &config)
            .await?;

        // Handle tool calls if present
        if !response.tool_calls.is_empty() {
            if let Some(tools) = tools {
                return Box::pin(self.handle_tool_calls(
                    current_messages,
                    response,
                    tools,
                    &config,
                ))
                .await;
            }
        }

        Ok(response.content.unwrap_or_default())
    }

    fn handle_tool_calls<'a>(
        &'a self,
        mut messages: Vec<LlmMessage>,
        response: LlmGatewayResponse,
        tools: &'a [Box<dyn LlmTool>],
        config: &'a CompletionConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            info!("Tool calls requested: {}", response.tool_calls.len());

            for tool_call in &response.tool_calls {
                // Find matching tool
                if let Some(tool) = tools.iter().find(|t| t.matches(&tool_call.name)) {
                    info!("Executing tool: {}", tool_call.name);

                    let output = tool.run(&tool_call.arguments)?;

                    // Add tool call and response to messages
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: None,
                        tool_calls: Some(vec![tool_call.clone()]),
                        image_paths: None,
                    });
                    messages.push(LlmMessage {
                        role: MessageRole::Tool,
                        content: Some(serde_json::to_string(&output)?),
                        tool_calls: Some(vec![tool_call.clone()]),
                        image_paths: None,
                    });

                    // Recursively call generate with updated messages
                    return self
                        .generate(&messages, Some(tools), Some(config.clone()))
                        .await;
                } else {
                    warn!("Tool not found: {}", tool_call.name);
                }
            }

            Ok(response.content.unwrap_or_default())
        })
    }

    /// Generate structured object response from LLM
    pub async fn generate_object<T>(
        &self,
        messages: &[LlmMessage],
        config: Option<CompletionConfig>,
    ) -> Result<T>
    where
        T: for<'de> Deserialize<'de> + Serialize + schemars::JsonSchema + Send,
    {
        let config = config.unwrap_or_default();

        // Generate JSON schema for the type
        let schema = serde_json::to_value(schemars::schema_for!(T))?;

        // Call the gateway with the schema
        let json_response = self
            .gateway
            .complete_json(&self.model, messages, schema, &config)
            .await?;

        // Deserialize the JSON into the target type
        let object: T = serde_json::from_value(json_response)?;

        Ok(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::models::LlmToolCall;
    use crate::llm::tools::{FunctionDescriptor, ToolDescriptor};
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::collections::HashMap;

    // Mock gateway for testing
    struct MockGateway {
        responses: Vec<LlmGatewayResponse>,
        call_count: std::sync::Mutex<usize>,
    }

    impl MockGateway {
        fn new(responses: Vec<LlmGatewayResponse>) -> Self {
            Self {
                responses,
                call_count: std::sync::Mutex::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmGateway for MockGateway {
        async fn complete(
            &self,
            _model: &str,
            _messages: &[LlmMessage],
            _tools: Option<&[Box<dyn LlmTool>]>,
            _config: &CompletionConfig,
        ) -> Result<LlmGatewayResponse> {
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;

            if idx < self.responses.len() {
                Ok(self.responses[idx].clone())
            } else {
                Ok(LlmGatewayResponse {
                    content: Some("default response".to_string()),
                    object: None,
                    tool_calls: vec![],
                })
            }
        }

        async fn complete_json(
            &self,
            _model: &str,
            _messages: &[LlmMessage],
            _schema: Value,
            _config: &CompletionConfig,
        ) -> Result<Value> {
            Ok(serde_json::json!({"test": "value"}))
        }

        async fn get_available_models(&self) -> Result<Vec<String>> {
            Ok(vec!["test-model".to_string()])
        }

        async fn calculate_embeddings(
            &self,
            _text: &str,
            _model: Option<&str>,
        ) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }
    }

    // Mock tool for testing
    struct MockTool {
        name: String,
        result: Value,
    }

    impl LlmTool for MockTool {
        fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
            Ok(self.result.clone())
        }

        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                r#type: "function".to_string(),
                function: FunctionDescriptor {
                    name: self.name.clone(),
                    description: "A mock tool".to_string(),
                    parameters: serde_json::json!({}),
                },
            }
        }
    }

    #[tokio::test]
    async fn test_broker_new() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway);
        assert_eq!(broker.model, "test-model");
    }

    #[tokio::test]
    async fn test_broker_new_string_conversion() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new(String::from("my-model"), gateway);
        assert_eq!(broker.model, "my-model");
    }

    #[tokio::test]
    async fn test_generate_simple_response() {
        let response = LlmGatewayResponse {
            content: Some("Hello, World!".to_string()),
            object: None,
            tool_calls: vec![],
        };

        let gateway = Arc::new(MockGateway::new(vec![response]));
        let broker = LlmBroker::new("test-model", gateway);

        let messages = vec![LlmMessage::user("Hi")];
        let result = broker.generate(&messages, None, None).await.unwrap();

        assert_eq!(result, "Hello, World!");
    }

    #[tokio::test]
    async fn test_generate_with_custom_config() {
        let response = LlmGatewayResponse {
            content: Some("Response".to_string()),
            object: None,
            tool_calls: vec![],
        };

        let gateway = Arc::new(MockGateway::new(vec![response]));
        let broker = LlmBroker::new("test-model", gateway);

        let config = CompletionConfig {
            temperature: 0.5,
            num_ctx: 2048,
            max_tokens: 100,
            num_predict: Some(50),
        };

        let messages = vec![LlmMessage::user("Hi")];
        let result = broker
            .generate(&messages, None, Some(config))
            .await
            .unwrap();

        assert_eq!(result, "Response");
    }

    #[tokio::test]
    async fn test_generate_empty_response_content() {
        let response = LlmGatewayResponse {
            content: None,
            object: None,
            tool_calls: vec![],
        };

        let gateway = Arc::new(MockGateway::new(vec![response]));
        let broker = LlmBroker::new("test-model", gateway);

        let messages = vec![LlmMessage::user("Hi")];
        let result = broker.generate(&messages, None, None).await.unwrap();

        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn test_generate_with_tool_call() {
        let tool_call = LlmToolCall {
            id: Some("call_1".to_string()),
            name: "test_tool".to_string(),
            arguments: HashMap::new(),
        };

        let first_response = LlmGatewayResponse {
            content: None,
            object: None,
            tool_calls: vec![tool_call],
        };

        let second_response = LlmGatewayResponse {
            content: Some("After tool execution".to_string()),
            object: None,
            tool_calls: vec![],
        };

        let gateway = Arc::new(MockGateway::new(vec![first_response, second_response]));
        let broker = LlmBroker::new("test-model", gateway);

        let tool = MockTool {
            name: "test_tool".to_string(),
            result: serde_json::json!({"result": "success"}),
        };

        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(tool)];

        let messages = vec![LlmMessage::user("Use the tool")];
        let result = broker
            .generate(&messages, Some(&tools), None)
            .await
            .unwrap();

        assert_eq!(result, "After tool execution");
    }

    #[tokio::test]
    async fn test_generate_with_tool_call_no_tools_provided() {
        let tool_call = LlmToolCall {
            id: Some("call_1".to_string()),
            name: "test_tool".to_string(),
            arguments: HashMap::new(),
        };

        let response = LlmGatewayResponse {
            content: Some("fallback".to_string()),
            object: None,
            tool_calls: vec![tool_call],
        };

        let gateway = Arc::new(MockGateway::new(vec![response]));
        let broker = LlmBroker::new("test-model", gateway);

        let messages = vec![LlmMessage::user("Use the tool")];
        let result = broker.generate(&messages, None, None).await.unwrap();

        assert_eq!(result, "fallback");
    }

    #[tokio::test]
    async fn test_generate_object() {
        #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
        struct TestObject {
            test: String,
        }

        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway);

        let messages = vec![LlmMessage::user("Generate object")];
        let result: TestObject = broker.generate_object(&messages, None).await.unwrap();

        assert_eq!(result.test, "value");
    }

    #[tokio::test]
    async fn test_generate_object_with_config() {
        #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
        struct TestData {
            test: String,
        }

        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway);

        let config = CompletionConfig {
            temperature: 0.1,
            num_ctx: 1024,
            max_tokens: 50,
            num_predict: None,
        };

        let messages = vec![LlmMessage::user("Generate")];
        let result: TestData = broker
            .generate_object(&messages, Some(config))
            .await
            .unwrap();

        assert_eq!(result.test, "value");
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let response = LlmGatewayResponse {
            content: Some("Response to conversation".to_string()),
            object: None,
            tool_calls: vec![],
        };

        let gateway = Arc::new(MockGateway::new(vec![response]));
        let broker = LlmBroker::new("test-model", gateway);

        let messages = vec![
            LlmMessage::system("You are helpful"),
            LlmMessage::user("First message"),
            LlmMessage::assistant("First response"),
            LlmMessage::user("Second message"),
        ];

        let result = broker.generate(&messages, None, None).await.unwrap();
        assert_eq!(result, "Response to conversation");
    }
}
