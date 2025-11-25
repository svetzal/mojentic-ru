//! Working memory example demonstrating shared context between agents.
//!
//! For comprehensive documentation on the working memory pattern, see:
//! book/src/agents/working_memory.md
//!
//! This example shows how to use SharedWorkingMemory to maintain state across
//! multiple agent interactions. It demonstrates:
//! - SharedWorkingMemory for shared context
//! - RequestAgent that uses memory to answer questions and learns new information
//! - Event-driven architecture with custom events
//! - AsyncDispatcher and Router for event coordination

use async_trait::async_trait;
use mojentic::agents::BaseAsyncAgent;
use mojentic::async_dispatcher::AsyncDispatcher;
use mojentic::context::SharedWorkingMemory;
use mojentic::event::{Event, TerminateEvent};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::router::Router;
use mojentic::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Event Definitions
// ============================================================================

/// Event representing a user request/question
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RequestEvent {
    source: String,
    correlation_id: Option<String>,
    text: String,
}

impl Event for RequestEvent {
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

/// Event representing an agent response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResponseEvent {
    source: String,
    correlation_id: Option<String>,
    text: String,
    memory: Value,
}

impl Event for ResponseEvent {
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

// ============================================================================
// Response Model for Structured LLM Output
// ============================================================================

/// Response model for the LLM - includes text and updated memory
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ResponseModel {
    /// The text response to the user
    text: String,
    /// Updated working memory with any new information learned
    memory: Value,
}

// ============================================================================
// RequestAgent - Handles user requests with memory
// ============================================================================

/// Agent that processes user requests using shared working memory.
///
/// This agent answers questions based on what it remembers and learns new
/// information from the user, storing it in the shared working memory.
struct RequestAgent {
    broker: Arc<LlmBroker>,
    memory: SharedWorkingMemory,
    behaviour: String,
    instructions: String,
}

impl RequestAgent {
    /// Create a new RequestAgent.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker for generating responses
    /// * `memory` - Shared working memory
    fn new(broker: Arc<LlmBroker>, memory: SharedWorkingMemory) -> Self {
        Self {
            broker,
            memory,
            behaviour: "You are a helpful assistant, and you like to make note of new things that you learn.".to_string(),
            instructions: "Answer the user's question, use what you know, and what you remember.".to_string(),
        }
    }

    /// Generate a response with memory context.
    async fn generate_response(
        &self,
        content: &str,
        correlation_id: Option<String>,
    ) -> Result<ResponseModel> {
        let current_memory = self.memory.get_working_memory();

        let messages = vec![
            LlmMessage::system(&self.behaviour),
            LlmMessage::system(format!(
                "This is what you remember:\n{}\n\nRemember anything new you learn by storing it to your working memory in your response.",
                serde_json::to_string_pretty(&current_memory).unwrap()
            )),
            LlmMessage::user(&self.instructions),
            LlmMessage::user(content),
        ];

        let response: ResponseModel =
            self.broker.generate_object(&messages, None, correlation_id).await?;

        // Merge the updated memory
        self.memory.merge_to_working_memory(response.memory.clone());

        Ok(response)
    }
}

#[async_trait]
impl BaseAsyncAgent for RequestAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Downcast to RequestEvent
        if let Some(request_event) = event.as_any().downcast_ref::<RequestEvent>() {
            let response = self
                .generate_response(
                    &request_event.text,
                    event.correlation_id().map(|s| s.to_string()),
                )
                .await?;

            return Ok(vec![Box::new(ResponseEvent {
                source: "RequestAgent".to_string(),
                correlation_id: event.correlation_id().map(|s| s.to_string()),
                text: response.text,
                memory: self.memory.get_working_memory(),
            }) as Box<dyn Event>]);
        }

        Ok(vec![])
    }
}

// ============================================================================
// OutputAgent - Displays responses and terminates
// ============================================================================

/// Agent that outputs responses and terminates the dispatcher.
struct OutputAgent;

#[async_trait]
impl BaseAsyncAgent for OutputAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        if let Some(request_event) = event.as_any().downcast_ref::<RequestEvent>() {
            println!("\n📥 Request: {}", request_event.text);
        } else if let Some(response_event) = event.as_any().downcast_ref::<ResponseEvent>() {
            println!("\n📤 Response: {}", response_event.text);
            println!("\n🧠 Memory State:");
            println!("{}", serde_json::to_string_pretty(&response_event.memory).unwrap());

            // Terminate after displaying response
            return Ok(vec![Box::new(TerminateEvent::new("OutputAgent")) as Box<dyn Event>]);
        }

        Ok(vec![])
    }
}

// ============================================================================
// Main Example
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for better debugging
    tracing_subscriber::fmt::init();

    println!("🚀 Working Memory Example");
    println!("═══════════════════════════════════════════════════════════════");

    // Create shared working memory with initial user data
    let memory = SharedWorkingMemory::new(json!({
        "User": {
            "name": "Stacey",
            "age": 56,
        }
    }));

    println!("\n📝 Initial Memory:");
    println!("{}", serde_json::to_string_pretty(&memory.get_working_memory()).unwrap());

    // Create LLM broker with Ollama gateway
    let gateway = OllamaGateway::default();
    let broker = Arc::new(LlmBroker::new(
        // "deepseek-r1:70b",
        "qwen2.5:14b",
        Arc::new(gateway),
        None,
    ));

    // Create agents
    let request_agent = Arc::new(RequestAgent::new(broker.clone(), memory.clone()));
    let output_agent = Arc::new(OutputAgent);

    // Setup router
    let mut router = Router::new();
    router.add_route::<RequestEvent>(request_agent.clone());
    router.add_route::<RequestEvent>(output_agent.clone());
    router.add_route::<ResponseEvent>(output_agent.clone());

    // Create and start dispatcher
    let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
    dispatcher.start().await?;

    // Dispatch initial request
    let request = Box::new(RequestEvent {
        source: "User".to_string(),
        correlation_id: None,
        text: "What is my name, and how old am I? And, did you know I have a dog named Boomer, and two cats named Spot and Beau?".to_string(),
    }) as Box<dyn Event>;

    dispatcher.dispatch(request);

    // Wait for processing to complete
    println!("\n⏳ Processing request...\n");
    dispatcher.wait_for_empty_queue(Some(Duration::from_secs(60))).await?;

    // Stop dispatcher
    dispatcher.stop().await?;

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("✅ Example completed successfully");

    Ok(())
}
