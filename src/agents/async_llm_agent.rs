//! Async LLM-powered agent implementation.
//!
//! This module provides an agent that uses an LLM to generate responses to events.
//! It supports system prompts (behaviour), structured output via response models,
//! and tool calling.

use crate::agents::BaseAsyncAgent;
use crate::event::Event;
use crate::llm::{LlmBroker, LlmMessage, LlmTool};
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// An async agent powered by an LLM.
///
/// This agent uses an LLM broker to generate responses. It can be configured
/// with a system prompt (behaviour), optional tools, and a response model for
/// structured output.
///
/// # Examples
///
/// ```ignore
/// use mojentic::agents::AsyncLlmAgent;
/// use mojentic::llm::LlmBroker;
///
/// let broker = Arc::new(LlmBroker::new("model-name", gateway, None));
/// let agent = AsyncLlmAgent::new(
///     broker,
///     "You are a helpful assistant.",
///     None, // tools
/// );
/// ```
pub struct AsyncLlmAgent {
    broker: Arc<LlmBroker>,
    behaviour: String,
    tools: Vec<Box<dyn LlmTool>>,
}

impl AsyncLlmAgent {
    /// Create a new AsyncLlmAgent.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for generating responses
    /// * `behaviour` - System prompt defining the agent's personality and behavior
    /// * `tools` - Optional tools available to the LLM
    pub fn new(
        broker: Arc<LlmBroker>,
        behaviour: impl Into<String>,
        tools: Option<Vec<Box<dyn LlmTool>>>,
    ) -> Self {
        Self {
            broker,
            behaviour: behaviour.into(),
            tools: tools.unwrap_or_default(),
        }
    }

    /// Add a tool to the agent.
    ///
    /// # Arguments
    ///
    /// * `tool` - The tool to add
    pub fn add_tool(&mut self, tool: Box<dyn LlmTool>) {
        self.tools.push(tool);
    }

    /// Generate a text response using the LLM.
    ///
    /// # Arguments
    ///
    /// * `content` - The user message content
    /// * `correlation_id` - Optional correlation ID for tracing
    ///
    /// # Returns
    ///
    /// The generated text response
    pub async fn generate_response(
        &self,
        content: &str,
        correlation_id: Option<String>,
    ) -> Result<String> {
        let messages = vec![
            LlmMessage::system(&self.behaviour),
            LlmMessage::user(content),
        ];

        let tools = if self.tools.is_empty() {
            None
        } else {
            Some(self.tools.as_slice())
        };

        self.broker.generate(&messages, tools, None, correlation_id).await
    }

    /// Generate a structured object response using the LLM.
    ///
    /// # Arguments
    ///
    /// * `content` - The user message content
    /// * `correlation_id` - Optional correlation ID for tracing
    ///
    /// # Returns
    ///
    /// The generated structured object
    pub async fn generate_object<T>(
        &self,
        content: &str,
        correlation_id: Option<String>,
    ) -> Result<T>
    where
        T: for<'de> Deserialize<'de> + Serialize + schemars::JsonSchema + Send,
    {
        let messages = vec![
            LlmMessage::system(&self.behaviour),
            LlmMessage::user(content),
        ];

        self.broker.generate_object(&messages, None, correlation_id).await
    }
}

#[async_trait]
impl BaseAsyncAgent for AsyncLlmAgent {
    async fn receive_event_async(&self, _event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Default implementation returns no events
        // Subclasses should override this to handle specific event types
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::gateway::{CompletionConfig, StreamChunk};
    use crate::llm::{LlmGateway, LlmGatewayResponse};
    use futures::stream::{self, Stream};
    use serde_json::Value;
    use std::collections::HashMap;
    use std::pin::Pin;

    // Mock gateway for testing
    struct MockGateway {
        response: String,
    }

    impl MockGateway {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
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
            Ok(LlmGatewayResponse {
                content: Some(self.response.clone()),
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
            Ok(serde_json::json!({
                "message": self.response,
                "confidence": 0.95
            }))
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
            Box::pin(stream::iter(vec![Ok(StreamChunk::Content(self.response.clone()))]))
        }
    }

    #[tokio::test]
    async fn test_new_agent() {
        let gateway = Arc::new(MockGateway::new("test response"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = AsyncLlmAgent::new(broker, "You are helpful", None);

        assert_eq!(agent.behaviour, "You are helpful");
        assert_eq!(agent.tools.len(), 0);
    }

    #[tokio::test]
    async fn test_new_agent_with_tools() {
        use crate::llm::tools::{FunctionDescriptor, ToolDescriptor};

        struct MockTool;
        impl LlmTool for MockTool {
            fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
                Ok(serde_json::json!({"result": "ok"}))
            }
            fn descriptor(&self) -> ToolDescriptor {
                ToolDescriptor {
                    r#type: "function".to_string(),
                    function: FunctionDescriptor {
                        name: "mock_tool".to_string(),
                        description: "A mock tool".to_string(),
                        parameters: serde_json::json!({}),
                    },
                }
            }
        }

        let gateway = Arc::new(MockGateway::new("test"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(MockTool)];
        let agent = AsyncLlmAgent::new(broker, "You are helpful", Some(tools));

        assert_eq!(agent.tools.len(), 1);
    }

    #[tokio::test]
    async fn test_add_tool() {
        use crate::llm::tools::{FunctionDescriptor, ToolDescriptor};

        struct MockTool;
        impl LlmTool for MockTool {
            fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
                Ok(serde_json::json!({"result": "ok"}))
            }
            fn descriptor(&self) -> ToolDescriptor {
                ToolDescriptor {
                    r#type: "function".to_string(),
                    function: FunctionDescriptor {
                        name: "mock_tool".to_string(),
                        description: "A mock tool".to_string(),
                        parameters: serde_json::json!({}),
                    },
                }
            }
        }

        let gateway = Arc::new(MockGateway::new("test"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let mut agent = AsyncLlmAgent::new(broker, "You are helpful", None);

        assert_eq!(agent.tools.len(), 0);
        agent.add_tool(Box::new(MockTool));
        assert_eq!(agent.tools.len(), 1);
    }

    #[tokio::test]
    async fn test_generate_response() {
        let gateway = Arc::new(MockGateway::new("Hello from LLM"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = AsyncLlmAgent::new(broker, "You are helpful", None);

        let response = agent.generate_response("Test message", None).await.unwrap();

        assert_eq!(response, "Hello from LLM");
    }

    #[tokio::test]
    async fn test_generate_response_with_correlation_id() {
        let gateway = Arc::new(MockGateway::new("Response"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = AsyncLlmAgent::new(broker, "You are helpful", None);

        let response = agent
            .generate_response("Test", Some("correlation-123".to_string()))
            .await
            .unwrap();

        assert_eq!(response, "Response");
    }

    #[tokio::test]
    async fn test_generate_object() {
        #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
        struct TestResponse {
            message: String,
            confidence: f64,
        }

        let gateway = Arc::new(MockGateway::new("Test message"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = AsyncLlmAgent::new(broker, "You are helpful", None);

        let response: TestResponse = agent.generate_object("Generate object", None).await.unwrap();

        assert_eq!(response.message, "Test message");
        assert_eq!(response.confidence, 0.95);
    }

    #[tokio::test]
    async fn test_receive_event_async_default() {
        use crate::event::Event;
        use serde::{Deserialize, Serialize};
        use std::any::Any;

        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct TestEvent {
            source: String,
            correlation_id: Option<String>,
        }

        impl Event for TestEvent {
            fn source(&self) -> &str {
                &self.source
            }
            fn correlation_id(&self) -> Option<&str> {
                self.correlation_id.as_deref()
            }
            fn set_correlation_id(&mut self, id: String) {
                self.correlation_id = Some(id);
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn clone_box(&self) -> Box<dyn Event> {
                Box::new(self.clone())
            }
        }

        let gateway = Arc::new(MockGateway::new("test"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = AsyncLlmAgent::new(broker, "You are helpful", None);

        let event = Box::new(TestEvent {
            source: "Test".to_string(),
            correlation_id: None,
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event).await.unwrap();
        assert_eq!(result.len(), 0); // Default implementation returns empty
    }

    #[tokio::test]
    async fn test_agent_with_custom_behaviour() {
        let gateway = Arc::new(MockGateway::new("Custom response"));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent =
            AsyncLlmAgent::new(broker, "You are a specialized agent with custom behavior", None);

        assert_eq!(agent.behaviour, "You are a specialized agent with custom behavior");

        let response = agent.generate_response("Test", None).await.unwrap();
        assert_eq!(response, "Custom response");
    }
}
