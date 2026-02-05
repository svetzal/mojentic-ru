use crate::error::Result;
use crate::llm::broker::LlmBroker;
use crate::llm::models::{LlmMessage, MessageRole};
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Wraps an agent (broker + tools + behaviour) as an LlmTool
///
/// This allows agents to be used as tools by other agents (delegation pattern).
/// The tool's descriptor has a single "input" parameter (string).
/// When run, it creates initial messages from the agent's behaviour, appends the input,
/// and calls the agent's broker.
pub struct ToolWrapper {
    broker: Arc<LlmBroker>,
    tools: Vec<Box<dyn LlmTool>>,
    behaviour: String,
    name: String,
    description: String,
}

impl ToolWrapper {
    /// Create a new ToolWrapper
    ///
    /// # Arguments
    /// * `broker` - The LLM broker for this agent
    /// * `tools` - The tools available to this agent
    /// * `behaviour` - The system message defining the agent's behaviour
    /// * `name` - The name of this tool (how other agents will call it)
    /// * `description` - Description of what this agent/tool does
    pub fn new(
        broker: Arc<LlmBroker>,
        tools: Vec<Box<dyn LlmTool>>,
        behaviour: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            broker,
            tools,
            behaviour: behaviour.into(),
            name: name.into(),
            description: description.into(),
        }
    }

    /// Create initial messages with the agent's behaviour
    fn create_initial_messages(&self) -> Vec<LlmMessage> {
        vec![LlmMessage {
            role: MessageRole::System,
            content: Some(self.behaviour.clone()),
            tool_calls: None,
            image_paths: None,
        }]
    }
}

impl LlmTool for ToolWrapper {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        // Extract input from arguments
        let input = args.get("input").and_then(|v| v.as_str()).ok_or_else(|| {
            crate::error::MojenticError::ToolError("Missing 'input' parameter".to_string())
        })?;

        // Create initial messages with behaviour
        let mut messages = self.create_initial_messages();

        // Append the user input
        messages.push(LlmMessage {
            role: MessageRole::User,
            content: Some(input.to_string()),
            tool_calls: None,
            image_paths: None,
        });

        // Call the broker with the messages and tools
        // We need to handle the async call in a way that works with the sync trait
        let response = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.broker.generate(&messages, Some(&self.tools), None, None).await
            })
        })?;

        Ok(json!(response))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Instructions for this agent."
                        }
                    },
                    "required": ["input"],
                    "additionalProperties": false
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(ToolWrapper {
            broker: self.broker.clone(),
            tools: self.tools.iter().map(|t| t.clone_box()).collect(),
            behaviour: self.behaviour.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::gateway::{CompletionConfig, LlmGateway, StreamChunk};
    use crate::llm::models::LlmGatewayResponse;
    use futures::stream::{self, Stream};
    use std::pin::Pin;

    // Mock gateway for testing
    struct MockGateway {
        expected_behaviour: String,
        response: String,
    }

    impl MockGateway {
        fn new(expected_behaviour: String, response: String) -> Self {
            Self {
                expected_behaviour,
                response,
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmGateway for MockGateway {
        async fn complete(
            &self,
            _model: &str,
            messages: &[LlmMessage],
            _tools: Option<&[Box<dyn LlmTool>]>,
            _config: &CompletionConfig,
        ) -> Result<LlmGatewayResponse> {
            // Verify that the first message is the system message with behaviour
            assert!(messages.len() >= 2, "Expected at least 2 messages (system + user)");
            assert_eq!(messages[0].role, MessageRole::System, "First message should be system");
            assert_eq!(
                messages[0].content.as_ref().unwrap(),
                &self.expected_behaviour,
                "System message should match behaviour"
            );

            Ok(LlmGatewayResponse {
                content: Some(self.response.clone()),
                object: None,
                tool_calls: vec![],
                thinking: None,
            })
        }

        async fn complete_json(
            &self,
            _model: &str,
            _messages: &[LlmMessage],
            _schema: Value,
            _config: &CompletionConfig,
        ) -> Result<Value> {
            Ok(json!({}))
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
            Box::pin(stream::iter(vec![]))
        }
    }

    #[tokio::test]
    async fn test_tool_wrapper_descriptor() {
        let gateway = Arc::new(MockGateway::new(
            "You are a test agent".to_string(),
            "test response".to_string(),
        ));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let tools: Vec<Box<dyn LlmTool>> = vec![];

        let wrapper = ToolWrapper::new(
            broker,
            tools,
            "You are a test agent",
            "test_agent",
            "A test agent for unit testing",
        );

        let descriptor = wrapper.descriptor();

        assert_eq!(descriptor.r#type, "function");
        assert_eq!(descriptor.function.name, "test_agent");
        assert_eq!(descriptor.function.description, "A test agent for unit testing");

        let params = descriptor.function.parameters;
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["input"].is_object());
        assert_eq!(params["properties"]["input"]["type"], "string");
        assert_eq!(params["properties"]["input"]["description"], "Instructions for this agent.");
        assert_eq!(params["required"], json!(["input"]));
        assert_eq!(params["additionalProperties"], false);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tool_wrapper_execution() {
        let gateway = Arc::new(MockGateway::new(
            "You are a helpful assistant".to_string(),
            "I can help with that!".to_string(),
        ));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let tools: Vec<Box<dyn LlmTool>> = vec![];

        let wrapper = ToolWrapper::new(
            broker,
            tools,
            "You are a helpful assistant",
            "assistant",
            "A helpful assistant",
        );

        let mut args = HashMap::new();
        args.insert("input".to_string(), json!("Help me with something"));

        let result = wrapper.run(&args).unwrap();

        // Result should be a JSON string value
        assert_eq!(result, json!("I can help with that!"));
    }

    #[tokio::test]
    async fn test_tool_wrapper_missing_input() {
        let gateway =
            Arc::new(MockGateway::new("You are a test agent".to_string(), "test".to_string()));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let tools: Vec<Box<dyn LlmTool>> = vec![];

        let wrapper =
            ToolWrapper::new(broker, tools, "You are a test agent", "test_agent", "A test agent");

        let args = HashMap::new();
        let result = wrapper.run(&args);

        assert!(result.is_err());
        match result {
            Err(crate::error::MojenticError::ToolError(message)) => {
                assert_eq!(message, "Missing 'input' parameter");
            }
            _ => panic!("Expected ToolError"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tool_wrapper_with_tools() {
        // Mock tool for the wrapped agent
        struct MockTool;

        impl LlmTool for MockTool {
            fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
                Ok(json!({"result": "tool executed"}))
            }

            fn descriptor(&self) -> ToolDescriptor {
                ToolDescriptor {
                    r#type: "function".to_string(),
                    function: FunctionDescriptor {
                        name: "mock_tool".to_string(),
                        description: "A mock tool".to_string(),
                        parameters: json!({}),
                    },
                }
            }

            fn clone_box(&self) -> Box<dyn LlmTool> {
                Box::new(MockTool)
            }
        }

        let gateway = Arc::new(MockGateway::new(
            "You are an agent with tools".to_string(),
            "Task completed using tools".to_string(),
        ));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(MockTool)];

        let wrapper = ToolWrapper::new(
            broker,
            tools,
            "You are an agent with tools",
            "tool_agent",
            "An agent that has access to tools",
        );

        let mut args = HashMap::new();
        args.insert("input".to_string(), json!("Use your tools"));

        let result = wrapper.run(&args).unwrap();

        assert_eq!(result, json!("Task completed using tools"));
    }

    #[tokio::test]
    async fn test_tool_wrapper_matches() {
        let gateway = Arc::new(MockGateway::new("test".to_string(), "test".to_string()));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let tools: Vec<Box<dyn LlmTool>> = vec![];

        let wrapper =
            ToolWrapper::new(broker, tools, "You are a test agent", "my_agent", "A test agent");

        assert!(wrapper.matches("my_agent"));
        assert!(!wrapper.matches("other_agent"));
    }
}
