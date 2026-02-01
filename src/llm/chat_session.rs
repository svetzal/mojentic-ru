//! Chat session management with context window tracking.
//!
//! This module provides a chat session abstraction that manages conversation history
//! and automatically handles context window limits using token counting.

use crate::error::Result;
use crate::llm::broker::LlmBroker;
use crate::llm::gateway::CompletionConfig;
use crate::llm::gateways::TokenizerGateway;
use crate::llm::models::{LlmMessage, MessageRole};
use crate::llm::tools::LlmTool;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// An LLM message with token count metadata.
///
/// This extends the standard `LlmMessage` with token length information
/// for context window management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizedLlmMessage {
    #[serde(flatten)]
    pub message: LlmMessage,
    pub token_length: usize,
}

impl SizedLlmMessage {
    /// Create a new sized message
    pub fn new(message: LlmMessage, token_length: usize) -> Self {
        Self {
            message,
            token_length,
        }
    }

    /// Get the role of the message
    pub fn role(&self) -> MessageRole {
        self.message.role
    }

    /// Get the content of the message
    pub fn content(&self) -> Option<&str> {
        self.message.content.as_deref()
    }
}

/// A chat session that manages conversation history with context window limits.
///
/// `ChatSession` maintains a list of messages and automatically trims old messages
/// when the total token count exceeds the configured maximum context size. The system
/// prompt (first message) is always preserved.
///
/// # Examples
///
/// ```ignore
/// use mojentic::llm::{ChatSession, LlmBroker};
/// use mojentic::llm::gateways::OllamaGateway;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let gateway = Arc::new(OllamaGateway::default());
///     let broker = LlmBroker::new("qwen3:32b", gateway);
///     let mut session = ChatSession::new(broker);
///
///     let response = session.send("What is Rust?").await?;
///     println!("Response: {}", response);
///
///     Ok(())
/// }
/// ```
pub struct ChatSession {
    broker: LlmBroker,
    messages: Vec<SizedLlmMessage>,
    tools: Option<Vec<Box<dyn LlmTool>>>,
    max_context: usize,
    tokenizer_gateway: TokenizerGateway,
    temperature: f32,
}

impl ChatSession {
    /// Create a new chat session with default settings.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for generating responses
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mojentic::llm::{ChatSession, LlmBroker};
    /// use mojentic::llm::gateways::OllamaGateway;
    /// use std::sync::Arc;
    ///
    /// let gateway = Arc::new(OllamaGateway::default());
    /// let broker = LlmBroker::new("qwen3:32b", gateway);
    /// let session = ChatSession::new(broker);
    /// ```
    pub fn new(broker: LlmBroker) -> Self {
        Self::builder(broker).build()
    }

    /// Create a chat session builder for custom configuration.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for generating responses
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mojentic::llm::ChatSession;
    ///
    /// let session = ChatSession::builder(broker)
    ///     .system_prompt("You are a helpful coding assistant.")
    ///     .temperature(0.7)
    ///     .max_context(16384)
    ///     .build();
    /// ```
    pub fn builder(broker: LlmBroker) -> ChatSessionBuilder {
        ChatSessionBuilder::new(broker)
    }

    /// Send a message to the LLM and get a response.
    ///
    /// This method:
    /// 1. Adds the user message to the conversation history
    /// 2. Generates a response using the LLM
    /// 3. Adds the assistant's response to the history
    /// 4. Automatically trims old messages if context window is exceeded
    ///
    /// # Arguments
    ///
    /// * `query` - The user's message
    ///
    /// # Returns
    ///
    /// The LLM's response as a string
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let response = session.send("What is 2 + 2?").await?;
    /// println!("Answer: {}", response);
    /// ```
    pub async fn send(&mut self, query: &str) -> Result<String> {
        // Add user message
        self.insert_message(LlmMessage::user(query));

        // Generate response
        let messages: Vec<LlmMessage> = self.messages.iter().map(|m| m.message.clone()).collect();
        let config = CompletionConfig {
            temperature: self.temperature,
            ..Default::default()
        };

        let response = self
            .broker
            .generate(&messages, self.tools.as_deref(), Some(config), None)
            .await?;

        // Ensure all messages in history have token counts
        self.ensure_all_messages_are_sized();

        // Add assistant response
        self.insert_message(LlmMessage::assistant(&response));

        Ok(response)
    }

    /// Send a message to the LLM and get a streaming response.
    ///
    /// This method:
    /// 1. Adds the user message to the conversation history
    /// 2. Streams the response from the LLM, yielding chunks as they arrive
    /// 3. After the stream is fully consumed, adds the assembled response to history
    /// 4. Automatically trims old messages if context window is exceeded
    ///
    /// # Arguments
    ///
    /// * `query` - The user's message
    ///
    /// # Returns
    ///
    /// A stream of string chunks from the LLM response
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use futures::stream::StreamExt;
    ///
    /// let mut stream = session.send_stream("Tell me a story");
    /// while let Some(result) = stream.next().await {
    ///     print!("{}", result?);
    /// }
    /// ```
    pub fn send_stream<'a>(
        &'a mut self,
        query: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<String>> + 'a>> {
        // Add user message
        self.insert_message(LlmMessage::user(query));

        // Clone messages for the broker call
        let messages: Vec<LlmMessage> = self.messages.iter().map(|m| m.message.clone()).collect();
        let config = CompletionConfig {
            temperature: self.temperature,
            ..Default::default()
        };

        Box::pin(async_stream::stream! {
            let mut accumulated = Vec::new();
            let tools_ref = self.tools.as_deref();
            let mut inner_stream = self.broker.generate_stream(&messages, tools_ref, Some(config), None);

            while let Some(result) = inner_stream.next().await {
                match &result {
                    Ok(chunk) => {
                        accumulated.push(chunk.clone());
                        yield result;
                    }
                    Err(_) => {
                        yield result;
                        return;
                    }
                }
            }

            // Stream consumed — finalize
            drop(inner_stream);
            self.ensure_all_messages_are_sized();
            let full_response = accumulated.join("");
            self.insert_message(LlmMessage::assistant(&full_response));
        })
    }

    /// Insert a message into the conversation history.
    ///
    /// If the total token count exceeds `max_context`, the oldest messages
    /// are removed until the total is under the limit. The system prompt
    /// (index 0) is always preserved.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to add
    pub fn insert_message(&mut self, message: LlmMessage) {
        let sized_message = self.build_sized_message(message);
        self.messages.push(sized_message);

        // Trim messages if over context limit
        let mut total_length: usize = self.messages.iter().map(|m| m.token_length).sum();

        while total_length > self.max_context && self.messages.len() > 1 {
            // Remove the oldest message (index 1 to preserve system prompt at 0)
            let removed = self.messages.remove(1);
            total_length -= removed.token_length;
        }
    }

    /// Get the current conversation history
    pub fn messages(&self) -> &[SizedLlmMessage] {
        &self.messages
    }

    /// Get the total token count of the current conversation
    pub fn total_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.token_length).sum()
    }

    /// Build a sized message from a regular message
    fn build_sized_message(&self, message: LlmMessage) -> SizedLlmMessage {
        let token_length = if let Some(content) = &message.content {
            self.tokenizer_gateway.encode(content).len()
        } else {
            0
        };

        SizedLlmMessage::new(message, token_length)
    }

    /// Ensure all messages in history have token counts
    fn ensure_all_messages_are_sized(&mut self) {
        for i in 0..self.messages.len() {
            if self.messages[i].token_length == 0 && self.messages[i].message.content.is_some() {
                let content = self.messages[i].message.content.clone().unwrap();
                let token_length = self.tokenizer_gateway.encode(&content).len();
                self.messages[i].token_length = token_length;
            }
        }
    }
}

/// Builder for constructing a `ChatSession` with custom configuration.
pub struct ChatSessionBuilder {
    broker: LlmBroker,
    system_prompt: String,
    tools: Option<Vec<Box<dyn LlmTool>>>,
    max_context: usize,
    tokenizer_gateway: Option<TokenizerGateway>,
    temperature: f32,
}

impl ChatSessionBuilder {
    /// Create a new builder
    fn new(broker: LlmBroker) -> Self {
        Self {
            broker,
            system_prompt: "You are a helpful assistant.".to_string(),
            tools: None,
            max_context: 32768,
            tokenizer_gateway: None,
            temperature: 1.0,
        }
    }

    /// Set the system prompt (default: "You are a helpful assistant.")
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Set the tools available to the LLM
    pub fn tools(mut self, tools: Vec<Box<dyn LlmTool>>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the maximum context window in tokens (default: 32768)
    pub fn max_context(mut self, max_context: usize) -> Self {
        self.max_context = max_context;
        self
    }

    /// Set a custom tokenizer gateway (default: cl100k_base)
    pub fn tokenizer_gateway(mut self, gateway: TokenizerGateway) -> Self {
        self.tokenizer_gateway = Some(gateway);
        self
    }

    /// Set the temperature for generation (default: 1.0)
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Build the chat session
    pub fn build(self) -> ChatSession {
        let tokenizer_gateway = self.tokenizer_gateway.unwrap_or_default();
        let system_message = LlmMessage::system(&self.system_prompt);
        let token_length = tokenizer_gateway.encode(&self.system_prompt).len();

        ChatSession {
            broker: self.broker,
            messages: vec![SizedLlmMessage::new(system_message, token_length)],
            tools: self.tools,
            max_context: self.max_context,
            tokenizer_gateway,
            temperature: self.temperature,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::gateway::{LlmGateway, StreamChunk};
    use crate::llm::models::LlmGatewayResponse;
    use crate::llm::tools::{FunctionDescriptor, ToolDescriptor};
    use futures::stream::{self, Stream};
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    // Mock gateway for testing
    struct MockGateway {
        responses: Vec<String>,
        call_count: Mutex<usize>,
    }

    impl MockGateway {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses,
                call_count: Mutex::new(0),
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

            let content = if idx < self.responses.len() {
                self.responses[idx].clone()
            } else {
                "default response".to_string()
            };

            Ok(LlmGatewayResponse {
                content: Some(content),
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
            Ok(json!({}))
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
            Box::pin(stream::iter(vec![Ok(StreamChunk::Content("test".to_string()))]))
        }
    }

    // Mock tool for testing
    #[derive(Clone)]
    struct MockTool {
        name: String,
    }

    impl LlmTool for MockTool {
        fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
            Ok(json!({"result": "success"}))
        }

        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                r#type: "function".to_string(),
                function: FunctionDescriptor {
                    name: self.name.clone(),
                    description: "A mock tool".to_string(),
                    parameters: json!({}),
                },
            }
        }

        fn clone_box(&self) -> Box<dyn LlmTool> {
            Box::new(self.clone())
        }
    }

    #[tokio::test]
    async fn test_new_session_has_system_message() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let session = ChatSession::new(broker);

        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role(), MessageRole::System);
        assert_eq!(session.messages[0].content(), Some("You are a helpful assistant."));
    }

    #[tokio::test]
    async fn test_builder_custom_system_prompt() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let session = ChatSession::builder(broker).system_prompt("Custom system prompt").build();

        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content(), Some("Custom system prompt"));
    }

    #[tokio::test]
    async fn test_builder_custom_temperature() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let session = ChatSession::builder(broker).temperature(0.5).build();

        assert_eq!(session.temperature, 0.5);
    }

    #[tokio::test]
    async fn test_builder_custom_max_context() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let session = ChatSession::builder(broker).max_context(16384).build();

        assert_eq!(session.max_context, 16384);
    }

    #[tokio::test]
    async fn test_send_adds_messages_to_history() {
        let gateway = Arc::new(MockGateway::new(vec!["Hello, World!".to_string()]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        let response = session.send("Hi").await.unwrap();

        assert_eq!(response, "Hello, World!");
        // Should have: system, user, assistant
        assert_eq!(session.messages.len(), 3);
        assert_eq!(session.messages[1].role(), MessageRole::User);
        assert_eq!(session.messages[1].content(), Some("Hi"));
        assert_eq!(session.messages[2].role(), MessageRole::Assistant);
        assert_eq!(session.messages[2].content(), Some("Hello, World!"));
    }

    #[tokio::test]
    async fn test_send_multiple_turns() {
        let gateway = Arc::new(MockGateway::new(vec![
            "First response".to_string(),
            "Second response".to_string(),
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        session.send("First query").await.unwrap();
        session.send("Second query").await.unwrap();

        // Should have: system, user1, assistant1, user2, assistant2
        assert_eq!(session.messages.len(), 5);
        assert_eq!(session.messages[3].content(), Some("Second query"));
        assert_eq!(session.messages[4].content(), Some("Second response"));
    }

    #[tokio::test]
    async fn test_insert_message_calculates_token_length() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        session.insert_message(LlmMessage::user("Hello"));

        assert_eq!(session.messages.len(), 2);
        assert!(session.messages[1].token_length > 0);
    }

    #[tokio::test]
    async fn test_context_window_trimming() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);

        // Create session with very small context window
        let mut session = ChatSession::builder(broker).max_context(50).build();

        // Add several messages with longer content to force trimming
        for i in 0..10 {
            session.insert_message(LlmMessage::user(format!(
                "This is a longer message number {} with more content to increase token count",
                i
            )));
        }

        // Should have trimmed old messages
        assert!(session.messages.len() < 11); // Less than 1 system + 10 user messages

        // System prompt should still be first
        assert_eq!(session.messages[0].role(), MessageRole::System);

        // Total tokens should be under limit
        assert!(session.total_tokens() <= 50);
    }

    #[tokio::test]
    async fn test_context_window_preserves_system_prompt() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);

        let mut session = ChatSession::builder(broker)
            .system_prompt("Important system prompt")
            .max_context(50)
            .build();

        // Add many messages to force trimming
        for i in 0..20 {
            session.insert_message(LlmMessage::user(format!("Message {}", i)));
        }

        // System prompt should still be first
        assert_eq!(session.messages[0].role(), MessageRole::System);
        assert_eq!(session.messages[0].content(), Some("Important system prompt"));
    }

    #[tokio::test]
    async fn test_total_tokens() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        let initial_tokens = session.total_tokens();
        assert!(initial_tokens > 0); // System prompt has tokens

        session.insert_message(LlmMessage::user("Hello"));

        assert!(session.total_tokens() > initial_tokens);
    }

    #[tokio::test]
    async fn test_messages_accessor() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        session.insert_message(LlmMessage::user("Test"));

        let messages = session.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role(), MessageRole::System);
        assert_eq!(messages[1].role(), MessageRole::User);
    }

    #[tokio::test]
    async fn test_builder_with_tools() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);

        let tool: Box<dyn LlmTool> = Box::new(MockTool {
            name: "test_tool".to_string(),
        });

        let session = ChatSession::builder(broker).tools(vec![tool]).build();

        assert!(session.tools.is_some());
        assert_eq!(session.tools.as_ref().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_sized_message_creation() {
        let message = LlmMessage::user("Test content");
        let sized = SizedLlmMessage::new(message, 5);

        assert_eq!(sized.token_length, 5);
        assert_eq!(sized.role(), MessageRole::User);
        assert_eq!(sized.content(), Some("Test content"));
    }

    // Streaming mock gateway
    struct StreamingMockGateway {
        stream_chunks: Vec<Vec<String>>,
        call_count: Mutex<usize>,
    }

    impl StreamingMockGateway {
        fn new(stream_chunks: Vec<Vec<String>>) -> Self {
            Self {
                stream_chunks,
                call_count: Mutex::new(0),
            }
        }
    }

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
                content: Some("default".to_string()),
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
            Ok(json!({}))
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
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;

            let chunks = if idx < self.stream_chunks.len() {
                self.stream_chunks[idx].clone()
            } else {
                vec!["default".to_string()]
            };

            Box::pin(stream::iter(
                chunks.into_iter().map(|c| Ok(StreamChunk::Content(c))).collect::<Vec<_>>(),
            ))
        }
    }

    #[tokio::test]
    async fn test_send_stream_yields_content_chunks() {
        let gateway = Arc::new(StreamingMockGateway::new(vec![vec![
            "Hello".to_string(),
            " world".to_string(),
        ]]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        let mut chunks = Vec::new();
        let mut stream = session.send_stream("Hi");
        while let Some(result) = stream.next().await {
            chunks.push(result.unwrap());
        }

        assert_eq!(chunks, vec!["Hello", " world"]);
    }

    #[tokio::test]
    async fn test_send_stream_grows_message_history() {
        let gateway = Arc::new(StreamingMockGateway::new(vec![vec!["Response".to_string()]]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        {
            let mut stream = session.send_stream("Hi");
            while stream.next().await.is_some() {}
        }

        // system + user + assistant
        assert_eq!(session.messages.len(), 3);
    }

    #[tokio::test]
    async fn test_send_stream_records_assembled_response() {
        let gateway = Arc::new(StreamingMockGateway::new(vec![vec![
            "Hello".to_string(),
            " world".to_string(),
        ]]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        {
            let mut stream = session.send_stream("Hi");
            while stream.next().await.is_some() {}
        }

        assert_eq!(session.messages[2].content(), Some("Hello world"));
        assert_eq!(session.messages[2].role(), MessageRole::Assistant);
    }

    #[tokio::test]
    async fn test_send_stream_records_user_message() {
        let gateway = Arc::new(StreamingMockGateway::new(vec![vec!["Response".to_string()]]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        {
            let mut stream = session.send_stream("My question");
            while stream.next().await.is_some() {}
        }

        assert_eq!(session.messages[1].role(), MessageRole::User);
        assert_eq!(session.messages[1].content(), Some("My question"));
    }

    #[tokio::test]
    async fn test_send_stream_respects_context_capacity() {
        let gateway = Arc::new(StreamingMockGateway::new(vec![
            vec!["This is a longer response to consume tokens in the context window".to_string()],
            vec!["Another longer response that also consumes many tokens in context".to_string()],
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::builder(broker).max_context(50).build();

        {
            let mut stream = session.send_stream("First longer query message with extra words");
            while stream.next().await.is_some() {}
        }
        {
            let mut stream = session.send_stream("Second longer query message with extra words");
            while stream.next().await.is_some() {}
        }

        // System prompt should still be first
        assert_eq!(session.messages[0].role(), MessageRole::System);
        // Total tokens should be under limit
        assert!(session.total_tokens() <= 50);
    }

    #[tokio::test]
    async fn test_message_with_no_content_has_zero_tokens() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut session = ChatSession::new(broker);

        let message = LlmMessage {
            role: MessageRole::Assistant,
            content: None,
            tool_calls: None,
            image_paths: None,
        };

        session.insert_message(message);

        // Should have system + the message with no content
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[1].token_length, 0);
    }
}
