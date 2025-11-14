use crate::error::Result;
use crate::llm::gateway::{CompletionConfig, LlmGateway, StreamChunk};
use crate::llm::models::{LlmGatewayResponse, LlmMessage, MessageRole};
use crate::llm::tools::LlmTool;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
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
        let response =
            self.gateway.complete(&self.model, &current_messages, tools, &config).await?;

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
                    return self.generate(&messages, Some(tools), Some(config.clone())).await;
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
        let json_response =
            self.gateway.complete_json(&self.model, messages, schema, &config).await?;

        // Deserialize the JSON into the target type
        let object: T = serde_json::from_value(json_response)?;

        Ok(object)
    }

    /// Generate streaming text response from LLM
    ///
    /// Returns a stream that yields content chunks as they arrive. When tool calls
    /// are detected, the broker executes them and recursively streams the LLM's
    /// follow-up response.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use futures::stream::StreamExt;
    ///
    /// let broker = LlmBroker::new("qwen3:32b", gateway);
    /// let messages = vec![LlmMessage::user("Tell me a story")];
    ///
    /// let mut stream = broker.generate_stream(&messages, None, None);
    /// while let Some(result) = stream.next().await {
    ///     match result {
    ///         Ok(chunk) => print!("{}", chunk),
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// ```
    pub fn generate_stream<'a>(
        &'a self,
        messages: &'a [LlmMessage],
        tools: Option<&'a [Box<dyn LlmTool>]>,
        config: Option<CompletionConfig>,
    ) -> Pin<Box<dyn Stream<Item = Result<String>> + 'a>> {
        let config = config.unwrap_or_default();
        let current_messages = messages.to_vec();

        Box::pin(async_stream::stream! {
            let mut accumulated_content = String::new();
            let mut accumulated_tool_calls = Vec::new();

            // Stream from gateway
            let mut stream = self.gateway.complete_stream(
                &self.model,
                &current_messages,
                tools,
                &config,
            );

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(StreamChunk::Content(content)) => {
                        accumulated_content.push_str(&content);
                        yield Ok(content);
                    }
                    Ok(StreamChunk::ToolCalls(tool_calls)) => {
                        accumulated_tool_calls = tool_calls;
                    }
                    Err(e) => {
                        yield Err(e);
                        return;
                    }
                }
            }

            // Handle tool calls if present
            if !accumulated_tool_calls.is_empty() {
                if let Some(tools) = tools {
                    info!("Processing {} tool call(s) in stream", accumulated_tool_calls.len());

                    // Build new messages with tool results
                    let mut new_messages = current_messages.clone();

                    // Add assistant message with tool calls
                    new_messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: Some(accumulated_content),
                        tool_calls: Some(accumulated_tool_calls.clone()),
                        image_paths: None,
                    });

                    // Execute tools and add results
                    for tool_call in &accumulated_tool_calls {
                        if let Some(tool) = tools.iter().find(|t| t.matches(&tool_call.name)) {
                            info!("Executing tool: {}", tool_call.name);

                            match tool.run(&tool_call.arguments) {
                                Ok(output) => {
                                    let output_str = match serde_json::to_string(&output) {
                                        Ok(s) => s,
                                        Err(e) => {
                                            yield Err(e.into());
                                            return;
                                        }
                                    };

                                    new_messages.push(LlmMessage {
                                        role: MessageRole::Tool,
                                        content: Some(output_str),
                                        tool_calls: Some(vec![tool_call.clone()]),
                                        image_paths: None,
                                    });
                                }
                                Err(e) => {
                                    warn!("Tool execution failed: {}", e);
                                    yield Err(e);
                                    return;
                                }
                            }
                        } else {
                            warn!("Tool not found: {}", tool_call.name);
                        }
                    }

                    // Recursively stream with updated messages
                    let mut recursive_stream = self.generate_stream(&new_messages, Some(tools), Some(config.clone()));

                    while let Some(result) = recursive_stream.next().await {
                        yield result;
                    }
                } else {
                    warn!("LLM requested tool calls but no tools provided");
                }
            }
        })
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

        fn complete_stream<'a>(
            &'a self,
            _model: &'a str,
            _messages: &'a [LlmMessage],
            _tools: Option<&'a [Box<dyn LlmTool>]>,
            _config: &'a CompletionConfig,
        ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
            use futures::stream;
            Box::pin(stream::iter(vec![Ok(StreamChunk::Content("test".to_string()))]))
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
        let result = broker.generate(&messages, None, Some(config)).await.unwrap();

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
        let result = broker.generate(&messages, Some(&tools), None).await.unwrap();

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
        let result: TestData = broker.generate_object(&messages, Some(config)).await.unwrap();

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

    #[tokio::test]
    async fn test_generate_stream_basic() {
        use futures::stream;

        // Mock gateway that returns a simple stream
        struct StreamingMockGateway;

        #[async_trait::async_trait]
        impl LlmGateway for StreamingMockGateway {
            async fn complete(
                &self,
                _model: &str,
                _messages: &[LlmMessage],
                _tools: Option<&[Box<dyn LlmTool>]>,
                _config: &CompletionConfig,
            ) -> Result<LlmGatewayResponse> {
                Ok(LlmGatewayResponse {
                    content: Some("test".to_string()),
                    object: None,
                    tool_calls: vec![],
                })
            }

            async fn complete_json(
                &self,
                _model: &str,
                _messages: &[LlmMessage],
                _schema: Value,
                _config: &CompletionConfig,
            ) -> Result<Value> {
                Ok(serde_json::json!({}))
            }

            async fn get_available_models(&self) -> Result<Vec<String>> {
                Ok(vec![])
            }

            async fn calculate_embeddings(
                &self,
                _text: &str,
                _model: Option<&str>,
            ) -> Result<Vec<f32>> {
                Ok(vec![])
            }

            fn complete_stream<'a>(
                &'a self,
                _model: &'a str,
                _messages: &'a [LlmMessage],
                _tools: Option<&'a [Box<dyn LlmTool>]>,
                _config: &'a CompletionConfig,
            ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
                Box::pin(stream::iter(vec![
                    Ok(StreamChunk::Content("Hello".to_string())),
                    Ok(StreamChunk::Content(" ".to_string())),
                    Ok(StreamChunk::Content("World".to_string())),
                ]))
            }
        }

        let gateway = Arc::new(StreamingMockGateway);
        let broker = LlmBroker::new("test-model", gateway);
        let messages = vec![LlmMessage::user("Hello")];

        let mut stream = broker.generate_stream(&messages, None, None);
        let mut result = String::new();

        while let Some(chunk) = stream.next().await {
            result.push_str(&chunk.unwrap());
        }

        assert_eq!(result, "Hello World");
    }

    #[tokio::test]
    async fn test_generate_stream_with_tool_calls() {
        use futures::stream;

        // Mock gateway that returns tool calls
        struct ToolCallMockGateway {
            call_count: std::sync::Mutex<usize>,
        }

        impl ToolCallMockGateway {
            fn new() -> Self {
                Self {
                    call_count: std::sync::Mutex::new(0),
                }
            }
        }

        #[async_trait::async_trait]
        impl LlmGateway for ToolCallMockGateway {
            async fn complete(
                &self,
                _model: &str,
                _messages: &[LlmMessage],
                _tools: Option<&[Box<dyn LlmTool>]>,
                _config: &CompletionConfig,
            ) -> Result<LlmGatewayResponse> {
                Ok(LlmGatewayResponse {
                    content: Some("test".to_string()),
                    object: None,
                    tool_calls: vec![],
                })
            }

            async fn complete_json(
                &self,
                _model: &str,
                _messages: &[LlmMessage],
                _schema: Value,
                _config: &CompletionConfig,
            ) -> Result<Value> {
                Ok(serde_json::json!({}))
            }

            async fn get_available_models(&self) -> Result<Vec<String>> {
                Ok(vec![])
            }

            async fn calculate_embeddings(
                &self,
                _text: &str,
                _model: Option<&str>,
            ) -> Result<Vec<f32>> {
                Ok(vec![])
            }

            fn complete_stream<'a>(
                &'a self,
                _model: &'a str,
                _messages: &'a [LlmMessage],
                _tools: Option<&'a [Box<dyn LlmTool>]>,
                _config: &'a CompletionConfig,
            ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
                let mut count = self.call_count.lock().unwrap();
                let call_num = *count;
                *count += 1;

                if call_num == 0 {
                    // First call: return content with tool call
                    Box::pin(stream::iter(vec![
                        Ok(StreamChunk::Content("Initial ".to_string())),
                        Ok(StreamChunk::Content("response".to_string())),
                        Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
                            id: Some("call_1".to_string()),
                            name: "test_tool".to_string(),
                            arguments: HashMap::new(),
                        }])),
                    ]))
                } else {
                    // Second call (after tool execution): return final content
                    Box::pin(stream::iter(vec![
                        Ok(StreamChunk::Content("After ".to_string())),
                        Ok(StreamChunk::Content("tool".to_string())),
                    ]))
                }
            }
        }

        let gateway = Arc::new(ToolCallMockGateway::new());
        let broker = LlmBroker::new("test-model", gateway);

        let tool = MockTool {
            name: "test_tool".to_string(),
            result: serde_json::json!({"result": "success"}),
        };
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(tool)];

        let messages = vec![LlmMessage::user("Use the tool")];
        let mut stream = broker.generate_stream(&messages, Some(&tools), None);

        let mut result = String::new();
        while let Some(chunk) = stream.next().await {
            result.push_str(&chunk.unwrap());
        }

        // Should contain both initial response and post-tool response
        assert!(result.contains("Initial response"));
        assert!(result.contains("After tool"));
    }

    #[tokio::test]
    async fn test_generate_stream_without_tools() {
        use futures::stream;

        struct SimpleStreamGateway;

        #[async_trait::async_trait]
        impl LlmGateway for SimpleStreamGateway {
            async fn complete(
                &self,
                _model: &str,
                _messages: &[LlmMessage],
                _tools: Option<&[Box<dyn LlmTool>]>,
                _config: &CompletionConfig,
            ) -> Result<LlmGatewayResponse> {
                Ok(LlmGatewayResponse {
                    content: Some("test".to_string()),
                    object: None,
                    tool_calls: vec![],
                })
            }

            async fn complete_json(
                &self,
                _model: &str,
                _messages: &[LlmMessage],
                _schema: Value,
                _config: &CompletionConfig,
            ) -> Result<Value> {
                Ok(serde_json::json!({}))
            }

            async fn get_available_models(&self) -> Result<Vec<String>> {
                Ok(vec![])
            }

            async fn calculate_embeddings(
                &self,
                _text: &str,
                _model: Option<&str>,
            ) -> Result<Vec<f32>> {
                Ok(vec![])
            }

            fn complete_stream<'a>(
                &'a self,
                _model: &'a str,
                _messages: &'a [LlmMessage],
                _tools: Option<&'a [Box<dyn LlmTool>]>,
                _config: &'a CompletionConfig,
            ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
                // Simple stream with no tool calls
                Box::pin(stream::iter(vec![
                    Ok(StreamChunk::Content("Simple ".to_string())),
                    Ok(StreamChunk::Content("stream".to_string())),
                ]))
            }
        }

        let gateway = Arc::new(SimpleStreamGateway);
        let broker = LlmBroker::new("test-model", gateway);

        let messages = vec![LlmMessage::user("Test")];
        let mut stream = broker.generate_stream(&messages, None, None);

        let mut result = String::new();
        while let Some(chunk) = stream.next().await {
            result.push_str(&chunk.unwrap());
        }

        assert_eq!(result, "Simple stream");
    }
}
