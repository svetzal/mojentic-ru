//! Simple recursive agent using event-driven architecture.
//!
//! This module provides a declarative, event-driven agent that recursively attempts
//! to solve problems using available tools. The agent continues iterating until it
//! succeeds, fails explicitly, or reaches the maximum number of iterations.
//!
//! # Architecture
//!
//! The agent uses three main components:
//!
//! 1. **GoalState** - Tracks the problem-solving state through iterations
//! 2. **EventEmitter** - Manages event subscriptions and async dispatch
//! 3. **SimpleRecursiveAgent** - Orchestrates the problem-solving process
//!
//! # Events
//!
//! The agent emits the following events during problem-solving:
//!
//! - `GoalSubmittedEvent` - When a problem is submitted
//! - `IterationCompletedEvent` - After each iteration completes
//! - `GoalAchievedEvent` - When the goal is successfully achieved
//! - `GoalFailedEvent` - When the goal explicitly fails
//! - `TimeoutEvent` - When the process times out
//!
//! # Examples
//!
//! ```ignore
//! use mojentic::agents::SimpleRecursiveAgent;
//! use mojentic::llm::{LlmBroker, LlmGateway};
//! use mojentic::llm::gateways::OllamaGateway;
//! use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let gateway = Arc::new(OllamaGateway::default());
//!     let broker = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
//!
//!     let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];
//!
//!     let agent = SimpleRecursiveAgent::builder(broker)
//!         .tools(tools)
//!         .max_iterations(5)
//!         .build();
//!
//!     // Subscribe to events
//!     agent.emitter.subscribe(|event: IterationCompletedEvent| {
//!         println!("Iteration {}: {}", event.state.iteration, event.response);
//!     });
//!
//!     let result = agent.solve("What's the date next Friday?").await?;
//!     println!("Result: {}", result);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Completion Indicators
//!
//! The agent monitors responses for these keywords (case-insensitive, word boundaries):
//! - "DONE" - Task completed successfully
//! - "FAIL" - Task cannot be completed

use crate::error::Result;
use crate::llm::chat_session::ChatSession;
use crate::llm::tools::LlmTool;
use crate::llm::LlmBroker;
use regex::Regex;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};
use tracing::warn;

/// Represents the state of a problem-solving process.
#[derive(Debug, Clone)]
pub struct GoalState {
    /// The problem or goal to solve
    pub goal: String,
    /// Current iteration count
    pub iteration: usize,
    /// Maximum allowed iterations
    pub max_iterations: usize,
    /// The solution, if found
    pub solution: Option<String>,
    /// Whether the problem-solving process is complete
    pub is_complete: bool,
}

impl GoalState {
    /// Create a new goal state
    pub fn new(goal: impl Into<String>, max_iterations: usize) -> Self {
        Self {
            goal: goal.into(),
            iteration: 0,
            max_iterations,
            solution: None,
            is_complete: false,
        }
    }
}

/// Base trait for solver events.
pub trait SolverEvent: Send + Sync + std::fmt::Debug {
    /// Get the current state
    fn state(&self) -> &GoalState;
}

/// Event triggered when a goal is submitted for solving.
#[derive(Debug, Clone)]
pub struct GoalSubmittedEvent {
    pub state: GoalState,
}

impl SolverEvent for GoalSubmittedEvent {
    fn state(&self) -> &GoalState {
        &self.state
    }
}

/// Event triggered when an iteration of the problem-solving process is completed.
#[derive(Debug, Clone)]
pub struct IterationCompletedEvent {
    pub state: GoalState,
    /// The response from the LLM for this iteration
    pub response: String,
}

impl SolverEvent for IterationCompletedEvent {
    fn state(&self) -> &GoalState {
        &self.state
    }
}

/// Event triggered when a goal is successfully achieved.
#[derive(Debug, Clone)]
pub struct GoalAchievedEvent {
    pub state: GoalState,
}

impl SolverEvent for GoalAchievedEvent {
    fn state(&self) -> &GoalState {
        &self.state
    }
}

/// Event triggered when a goal cannot be solved.
#[derive(Debug, Clone)]
pub struct GoalFailedEvent {
    pub state: GoalState,
}

impl SolverEvent for GoalFailedEvent {
    fn state(&self) -> &GoalState {
        &self.state
    }
}

/// Event triggered when the problem-solving process times out.
#[derive(Debug, Clone)]
pub struct TimeoutEvent {
    pub state: GoalState,
}

impl SolverEvent for TimeoutEvent {
    fn state(&self) -> &GoalState {
        &self.state
    }
}

/// Union type of all solver events
#[derive(Debug, Clone)]
pub enum AnySolverEvent {
    GoalSubmitted(GoalSubmittedEvent),
    IterationCompleted(IterationCompletedEvent),
    GoalAchieved(GoalAchievedEvent),
    GoalFailed(GoalFailedEvent),
    Timeout(TimeoutEvent),
}

impl AnySolverEvent {
    /// Get the state from any event variant
    pub fn state(&self) -> &GoalState {
        match self {
            AnySolverEvent::GoalSubmitted(e) => &e.state,
            AnySolverEvent::IterationCompleted(e) => &e.state,
            AnySolverEvent::GoalAchieved(e) => &e.state,
            AnySolverEvent::GoalFailed(e) => &e.state,
            AnySolverEvent::Timeout(e) => &e.state,
        }
    }
}

/// Event handler callback type
type EventCallback = Arc<dyn Fn(AnySolverEvent) + Send + Sync>;

/// A simple event emitter that allows subscribing to and emitting events.
///
/// This implementation uses async channels to dispatch events to subscribers
/// asynchronously without blocking the emitter.
pub struct EventEmitter {
    subscribers: Arc<Mutex<Vec<EventCallback>>>,
}

impl EventEmitter {
    /// Create a new event emitter
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Subscribe to events with a callback function.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// emitter.subscribe(|event: AnySolverEvent| {
    ///     println!("Event received: {:?}", event);
    /// });
    /// ```
    pub async fn subscribe<F>(&self, callback: F)
    where
        F: Fn(AnySolverEvent) + Send + Sync + 'static,
    {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(Arc::new(callback));
    }

    /// Emit an event to all subscribers asynchronously.
    ///
    /// Events are dispatched to subscribers without blocking the emitter.
    pub async fn emit(&self, event: AnySolverEvent) {
        let subscribers = self.subscribers.lock().await.clone();

        for callback in subscribers {
            let event = event.clone();
            let callback = callback.clone();

            // Spawn a task to call the callback asynchronously
            tokio::spawn(async move {
                callback(event);
            });
        }
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

/// An agent that recursively attempts to solve a problem using available tools.
///
/// This agent uses an event-driven approach to manage the problem-solving process.
/// It will continue attempting to solve the problem until it either succeeds,
/// fails explicitly, or reaches the maximum number of iterations.
pub struct SimpleRecursiveAgent {
    broker: Arc<LlmBroker>,
    tools: Vec<Box<dyn LlmTool>>,
    max_iterations: usize,
    system_prompt: String,
    /// The event emitter used to manage events
    pub emitter: Arc<EventEmitter>,
}

impl SimpleRecursiveAgent {
    /// Create a new SimpleRecursiveAgent with default settings.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for generating responses
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mojentic::agents::SimpleRecursiveAgent;
    /// use mojentic::llm::LlmBroker;
    /// use std::sync::Arc;
    ///
    /// let broker = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
    /// let agent = SimpleRecursiveAgent::new(broker);
    /// ```
    pub fn new(broker: Arc<LlmBroker>) -> Self {
        Self::builder(broker).build()
    }

    /// Create a SimpleRecursiveAgent builder for custom configuration.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for generating responses
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mojentic::agents::SimpleRecursiveAgent;
    ///
    /// let agent = SimpleRecursiveAgent::builder(broker)
    ///     .max_iterations(10)
    ///     .system_prompt("You are a specialized assistant.")
    ///     .tools(vec![Box::new(SimpleDateTool)])
    ///     .build();
    /// ```
    pub fn builder(broker: Arc<LlmBroker>) -> SimpleRecursiveAgentBuilder {
        SimpleRecursiveAgentBuilder::new(broker)
    }

    /// Solve a problem asynchronously.
    ///
    /// This method runs the event-driven problem-solving process with a 300-second timeout.
    /// The agent will continue iterating until:
    /// - The task is completed successfully ("DONE")
    /// - The task fails explicitly ("FAIL")
    /// - The maximum number of iterations is reached
    /// - The process times out (300 seconds)
    ///
    /// # Arguments
    ///
    /// * `problem` - The problem or request to be solved
    ///
    /// # Returns
    ///
    /// The solution to the problem
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let solution = agent.solve("Calculate the factorial of 5").await?;
    /// println!("Solution: {}", solution);
    /// ```
    pub async fn solve(&self, problem: impl Into<String>) -> Result<String> {
        let problem = problem.into();

        // Create a channel to receive the solution
        let (solution_tx, mut solution_rx) = mpsc::channel::<String>(1);

        // Create the initial goal state
        let state = GoalState::new(problem.clone(), self.max_iterations);

        // Clone what we need for the async task
        let solution_tx_clone = solution_tx.clone();

        // Subscribe to completion events
        let emitter = self.emitter.clone();
        emitter
            .subscribe(move |event: AnySolverEvent| match &event {
                AnySolverEvent::GoalAchieved(_)
                | AnySolverEvent::GoalFailed(_)
                | AnySolverEvent::Timeout(_) => {
                    if let Some(solution) = &event.state().solution {
                        let _ = solution_tx_clone.try_send(solution.clone());
                    }
                }
                _ => {}
            })
            .await;

        // Start the solving process
        self.emitter
            .emit(AnySolverEvent::GoalSubmitted(GoalSubmittedEvent {
                state: state.clone(),
            }))
            .await;

        // Spawn a task to handle the problem submission
        let agent = self.clone_for_handler();
        tokio::spawn(async move {
            agent.handle_goal_submitted(state).await;
        });

        // Wait for solution or timeout (300 seconds)
        match timeout(Duration::from_secs(300), solution_rx.recv()).await {
            Ok(Some(solution)) => Ok(solution),
            Ok(None) => {
                let timeout_message =
                    "Timeout: Could not solve the problem within 300 seconds.".to_string();
                let mut timeout_state = GoalState::new(problem, self.max_iterations);
                timeout_state.solution = Some(timeout_message.clone());
                timeout_state.is_complete = true;

                self.emitter
                    .emit(AnySolverEvent::Timeout(TimeoutEvent {
                        state: timeout_state,
                    }))
                    .await;

                Ok(timeout_message)
            }
            Err(_) => {
                let timeout_message =
                    "Timeout: Could not solve the problem within 300 seconds.".to_string();
                let mut timeout_state = GoalState::new(problem, self.max_iterations);
                timeout_state.solution = Some(timeout_message.clone());
                timeout_state.is_complete = true;

                self.emitter
                    .emit(AnySolverEvent::Timeout(TimeoutEvent {
                        state: timeout_state,
                    }))
                    .await;

                Ok(timeout_message)
            }
        }
    }

    /// Handle a goal submitted event
    fn handle_goal_submitted(
        &self,
        state: GoalState,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.process_iteration(state).await;
        })
    }

    /// Handle an iteration completed event
    fn handle_iteration_completed(
        &self,
        mut state: GoalState,
        response: String,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let response_lower = response.to_lowercase();

            // Create regex patterns for word boundary matching
            let done_pattern = Regex::new(r"\bdone\b").unwrap();
            let fail_pattern = Regex::new(r"\bfail\b").unwrap();

            // Check if the task failed
            if fail_pattern.is_match(&response_lower) {
                state.solution = Some(format!(
                    "Failed to solve after {} iterations:\n{}",
                    state.iteration, response
                ));
                state.is_complete = true;

                self.emitter.emit(AnySolverEvent::GoalFailed(GoalFailedEvent { state })).await;
                return;
            }

            // Check if the task succeeded
            if done_pattern.is_match(&response_lower) {
                state.solution = Some(response);
                state.is_complete = true;

                self.emitter
                    .emit(AnySolverEvent::GoalAchieved(GoalAchievedEvent { state }))
                    .await;
                return;
            }

            // Check if we've reached max iterations
            if state.iteration >= state.max_iterations {
                state.solution = Some(format!(
                    "Best solution after {} iterations:\n{}",
                    state.max_iterations, response
                ));
                state.is_complete = true;

                self.emitter
                    .emit(AnySolverEvent::GoalAchieved(GoalAchievedEvent { state }))
                    .await;
                return;
            }

            // Continue with next iteration
            self.process_iteration(state).await;
        })
    }

    /// Process a single iteration of the problem-solving process
    fn process_iteration(
        &self,
        mut state: GoalState,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            // Increment iteration counter
            state.iteration += 1;

            // Generate prompt for this iteration
            let prompt = format!(
                "Given the user request:\n\
             {}\n\
             \n\
             Use the tools at your disposal to act on their request.\n\
             You may wish to create a step-by-step plan for more complicated requests.\n\
             \n\
             If you cannot provide an answer, say only \"FAIL\".\n\
             If you have the answer, say only \"DONE\".",
                state.goal
            );

            // Generate response asynchronously
            match self.generate_response(&prompt).await {
                Ok(response) => {
                    self.emitter
                        .emit(AnySolverEvent::IterationCompleted(IterationCompletedEvent {
                            state: state.clone(),
                            response: response.clone(),
                        }))
                        .await;

                    // Handle the completed iteration
                    self.handle_iteration_completed(state, response).await;
                }
                Err(e) => {
                    warn!("Error generating response: {}", e);
                    let mut error_state = state;
                    error_state.solution = Some(format!("Error: {}", e));
                    error_state.is_complete = true;

                    self.emitter
                        .emit(AnySolverEvent::GoalFailed(GoalFailedEvent { state: error_state }))
                        .await;
                }
            }
        })
    }

    /// Generate a response using a ChatSession
    async fn generate_response(&self, prompt: &str) -> Result<String> {
        // Create a chat session for this request
        let broker = Arc::clone(&self.broker);
        let mut chat = ChatSession::builder((*broker).clone())
            .system_prompt(&self.system_prompt)
            .tools(self.tools.iter().map(|t| t.clone_box()).collect())
            .build();

        chat.send(prompt).await
    }

    /// Clone the agent for use in async handlers
    ///
    /// This creates a shallow clone suitable for spawned tasks
    fn clone_for_handler(&self) -> Self {
        Self {
            broker: self.broker.clone(),
            tools: self.tools.iter().map(|t| t.clone_box()).collect(),
            max_iterations: self.max_iterations,
            system_prompt: self.system_prompt.clone(),
            emitter: self.emitter.clone(),
        }
    }
}

/// Builder for constructing a `SimpleRecursiveAgent` with custom configuration.
pub struct SimpleRecursiveAgentBuilder {
    broker: Arc<LlmBroker>,
    tools: Vec<Box<dyn LlmTool>>,
    max_iterations: usize,
    system_prompt: Option<String>,
}

impl SimpleRecursiveAgentBuilder {
    /// Create a new builder
    fn new(broker: Arc<LlmBroker>) -> Self {
        Self {
            broker,
            tools: Vec::new(),
            max_iterations: 5,
            system_prompt: None,
        }
    }

    /// Set the tools available to the agent
    pub fn tools(mut self, tools: Vec<Box<dyn LlmTool>>) -> Self {
        self.tools = tools;
        self
    }

    /// Set the maximum number of iterations (default: 5)
    pub fn max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Set a custom system prompt
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Build the agent
    pub fn build(self) -> SimpleRecursiveAgent {
        let system_prompt = self.system_prompt.unwrap_or_else(|| {
            "You are a problem-solving assistant that can solve complex problems step by step. \
             You analyze problems, break them down into smaller parts, and solve them systematically. \
             If you cannot solve a problem completely in one step, you make progress and identify what to do next."
                .to_string()
        });

        SimpleRecursiveAgent {
            broker: self.broker,
            tools: self.tools,
            max_iterations: self.max_iterations,
            system_prompt,
            emitter: Arc::new(EventEmitter::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::gateway::{CompletionConfig, LlmGateway, StreamChunk};
    use crate::llm::models::{LlmGatewayResponse, LlmMessage};
    use crate::llm::tools::{FunctionDescriptor, ToolDescriptor};
    use futures::stream::{self, Stream};
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Mock gateway for testing
    struct MockGateway {
        responses: Vec<String>,
        call_count: Arc<AtomicUsize>,
    }

    impl MockGateway {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses,
                call_count: Arc::new(AtomicUsize::new(0)),
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
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);

            let content = if idx < self.responses.len() {
                self.responses[idx].clone()
            } else {
                "default response".to_string()
            };

            Ok(LlmGatewayResponse {
                content: Some(content),
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
    async fn test_goal_state_creation() {
        let state = GoalState::new("Test problem", 5);

        assert_eq!(state.goal, "Test problem");
        assert_eq!(state.iteration, 0);
        assert_eq!(state.max_iterations, 5);
        assert_eq!(state.solution, None);
        assert!(!state.is_complete);
    }

    #[tokio::test]
    async fn test_event_emitter_subscribe_and_emit() {
        let emitter = EventEmitter::new();
        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        emitter
            .subscribe(move |_event: AnySolverEvent| {
                let received = received_clone.clone();
                tokio::spawn(async move {
                    *received.lock().await = true;
                });
            })
            .await;

        let state = GoalState::new("Test", 5);
        emitter.emit(AnySolverEvent::GoalSubmitted(GoalSubmittedEvent { state })).await;

        // Give the async task time to execute
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(*received.lock().await);
    }

    #[tokio::test]
    async fn test_builder_default_settings() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        assert_eq!(agent.max_iterations, 5);
        assert_eq!(agent.tools.len(), 0);
    }

    #[tokio::test]
    async fn test_builder_custom_max_iterations() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::builder(broker).max_iterations(10).build();

        assert_eq!(agent.max_iterations, 10);
    }

    #[tokio::test]
    async fn test_builder_with_tools() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));

        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(MockTool {
            name: "test_tool".to_string(),
        })];

        let agent = SimpleRecursiveAgent::builder(broker).tools(tools).build();

        assert_eq!(agent.tools.len(), 1);
    }

    #[tokio::test]
    async fn test_builder_custom_system_prompt() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::builder(broker).system_prompt("Custom prompt").build();

        assert_eq!(agent.system_prompt, "Custom prompt");
    }

    #[tokio::test]
    async fn test_solve_completes_with_done() {
        let gateway = Arc::new(MockGateway::new(vec!["DONE".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let result = agent.solve("Test problem").await.unwrap();

        assert_eq!(result, "DONE");
    }

    #[tokio::test]
    async fn test_solve_fails_with_fail() {
        let gateway = Arc::new(MockGateway::new(vec!["FAIL".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let result = agent.solve("Impossible problem").await.unwrap();

        assert!(result.contains("Failed to solve after 1 iterations"));
        assert!(result.contains("FAIL"));
    }

    #[tokio::test]
    async fn test_solve_case_insensitive_done() {
        let gateway = Arc::new(MockGateway::new(vec!["done".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let result = agent.solve("Test problem").await.unwrap();

        assert_eq!(result, "done");
    }

    #[tokio::test]
    async fn test_solve_case_insensitive_fail() {
        let gateway = Arc::new(MockGateway::new(vec!["fail".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let result = agent.solve("Test problem").await.unwrap();

        assert!(result.contains("Failed to solve"));
        assert!(result.contains("fail"));
    }

    #[tokio::test]
    async fn test_solve_word_boundary_done() {
        let gateway = Arc::new(MockGateway::new(vec!["I'm DONE with this task".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let result = agent.solve("Test problem").await.unwrap();

        assert_eq!(result, "I'm DONE with this task");
    }

    #[tokio::test]
    async fn test_solve_word_boundary_fail() {
        let gateway = Arc::new(MockGateway::new(vec!["This will FAIL".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let result = agent.solve("Test problem").await.unwrap();

        assert!(result.contains("Failed to solve"));
    }

    #[tokio::test]
    async fn test_solve_stops_at_max_iterations() {
        let gateway = Arc::new(MockGateway::new(vec![
            "Step 1".to_string(),
            "Step 2".to_string(),
            "Step 3".to_string(),
        ]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::builder(broker).max_iterations(3).build();

        let result = agent.solve("Long problem").await.unwrap();

        assert!(result.contains("Best solution after 3 iterations"));
        assert!(result.contains("Step 3"));
    }

    #[tokio::test]
    async fn test_solve_multiple_iterations_before_done() {
        let gateway = Arc::new(MockGateway::new(vec![
            "Working...".to_string(),
            "Still working...".to_string(),
            "DONE".to_string(),
        ]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::builder(broker).max_iterations(5).build();

        let result = agent.solve("Complex problem").await.unwrap();

        assert_eq!(result, "DONE");
    }

    #[tokio::test]
    async fn test_event_emission_during_solve() {
        let gateway = Arc::new(MockGateway::new(vec!["DONE".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let goal_submitted = Arc::new(Mutex::new(false));
        let iteration_completed = Arc::new(Mutex::new(false));
        let goal_achieved = Arc::new(Mutex::new(false));

        let goal_submitted_clone = goal_submitted.clone();
        let iteration_completed_clone = iteration_completed.clone();
        let goal_achieved_clone = goal_achieved.clone();

        agent
            .emitter
            .subscribe(move |event: AnySolverEvent| {
                let gs = goal_submitted_clone.clone();
                let ic = iteration_completed_clone.clone();
                let ga = goal_achieved_clone.clone();

                tokio::spawn(async move {
                    match event {
                        AnySolverEvent::GoalSubmitted(_) => *gs.lock().await = true,
                        AnySolverEvent::IterationCompleted(_) => *ic.lock().await = true,
                        AnySolverEvent::GoalAchieved(_) => *ga.lock().await = true,
                        _ => {}
                    }
                });
            })
            .await;

        let _result = agent.solve("Test problem").await.unwrap();

        // Give async tasks time to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(*goal_submitted.lock().await, "GoalSubmitted event not fired");
        assert!(*iteration_completed.lock().await, "IterationCompleted event not fired");
        assert!(*goal_achieved.lock().await, "GoalAchieved event not fired");
    }

    #[tokio::test]
    async fn test_event_emission_on_failure() {
        let gateway = Arc::new(MockGateway::new(vec!["FAIL".to_string()]));
        let broker = Arc::new(LlmBroker::new("test-model", gateway, None));
        let agent = SimpleRecursiveAgent::new(broker);

        let goal_failed = Arc::new(Mutex::new(false));
        let goal_failed_clone = goal_failed.clone();

        agent
            .emitter
            .subscribe(move |event: AnySolverEvent| {
                let gf = goal_failed_clone.clone();
                tokio::spawn(async move {
                    if matches!(event, AnySolverEvent::GoalFailed(_)) {
                        *gf.lock().await = true;
                    }
                });
            })
            .await;

        let _result = agent.solve("Test problem").await.unwrap();

        // Give async tasks time to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(*goal_failed.lock().await, "GoalFailed event not fired");
    }
}
