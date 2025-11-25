//! Iterative problem solver agent that uses tools to break down and solve complex problems.
//!
//! This agent uses a chat-based approach to iteratively work on solving a problem,
//! continuing until it succeeds, fails explicitly, or reaches the maximum number of iterations.

use crate::error::Result;
use crate::llm::chat_session::ChatSession;
use crate::llm::tools::LlmTool;
use crate::llm::LlmBroker;
use tracing::{info, warn};

/// An agent that iteratively attempts to solve a problem using available tools.
///
/// The solver uses a chat-based approach to break down and solve complex problems.
/// It will continue attempting to solve the problem until it either succeeds,
/// fails explicitly, or reaches the maximum number of iterations.
///
/// # Examples
///
/// ```ignore
/// use mojentic::agents::IterativeProblemSolver;
/// use mojentic::llm::{LlmBroker, LlmGateway};
/// use mojentic::llm::gateways::OllamaGateway;
/// use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let gateway = Arc::new(OllamaGateway::default());
///     let broker = LlmBroker::new("qwen3:32b", gateway, None);
///
///     let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];
///
///     let solver = IterativeProblemSolver::builder(broker)
///         .tools(tools)
///         .max_iterations(5)
///         .build();
///
///     let result = solver.solve("What's the date next Friday?").await?;
///     println!("Result: {}", result);
///
///     Ok(())
/// }
/// ```
pub struct IterativeProblemSolver {
    chat: ChatSession,
    max_iterations: usize,
}

impl IterativeProblemSolver {
    /// Create a new problem solver with default settings.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for generating responses
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mojentic::agents::IterativeProblemSolver;
    /// use mojentic::llm::LlmBroker;
    ///
    /// let solver = IterativeProblemSolver::new(broker);
    /// ```
    pub fn new(broker: LlmBroker) -> Self {
        Self::builder(broker).build()
    }

    /// Create a problem solver builder for custom configuration.
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for generating responses
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mojentic::agents::IterativeProblemSolver;
    ///
    /// let solver = IterativeProblemSolver::builder(broker)
    ///     .max_iterations(10)
    ///     .system_prompt("You are a specialized problem solver.")
    ///     .tools(vec![Box::new(SimpleDateTool)])
    ///     .build();
    /// ```
    pub fn builder(broker: LlmBroker) -> IterativeProblemSolverBuilder {
        IterativeProblemSolverBuilder::new(broker)
    }

    /// Execute the problem-solving process.
    ///
    /// This method runs the iterative problem-solving process, continuing until one of
    /// these conditions is met:
    /// - The task is completed successfully (response contains "DONE")
    /// - The task fails explicitly (response contains "FAIL")
    /// - The maximum number of iterations is reached
    ///
    /// After completion, the agent requests a summary of the final result.
    ///
    /// # Arguments
    ///
    /// * `problem` - The problem or request to be solved
    ///
    /// # Returns
    ///
    /// A summary of the final result, excluding the process details
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let result = solver.solve("Calculate the date 7 days from now").await?;
    /// println!("Solution: {}", result);
    /// ```
    pub async fn solve(&mut self, problem: &str) -> Result<String> {
        let mut iterations_remaining = self.max_iterations;

        loop {
            let result = self.step(problem).await?;

            // Check for explicit failure
            if result.to_lowercase().contains("fail") {
                info!(user_request = problem, result = result.as_str(), "Task failed");
                break;
            }

            // Check for successful completion
            if result.to_lowercase().contains("done") {
                info!(user_request = problem, result = result.as_str(), "Task completed");
                break;
            }

            iterations_remaining -= 1;
            if iterations_remaining == 0 {
                warn!(
                    max_iterations = self.max_iterations,
                    user_request = problem,
                    result = result.as_str(),
                    "Max iterations reached"
                );
                break;
            }
        }

        // Request final summary
        let summary = self
            .chat
            .send(
                "Summarize the final result, and only the final result, \
                 without commenting on the process by which you achieved it.",
            )
            .await?;

        Ok(summary)
    }

    /// Execute a single problem-solving step.
    ///
    /// This method sends a prompt to the chat session asking it to work on the user's request
    /// using available tools. The response should indicate success ("DONE") or failure ("FAIL").
    ///
    /// # Arguments
    ///
    /// * `problem` - The problem or request to be solved
    ///
    /// # Returns
    ///
    /// The response from the chat session, indicating the step's outcome
    async fn step(&mut self, problem: &str) -> Result<String> {
        let prompt = format!(
            "Given the user request:\n\
             {}\n\
             \n\
             Use the tools at your disposal to act on their request. \
             You may wish to create a step-by-step plan for more complicated requests.\n\
             \n\
             If you cannot provide an answer, say only \"FAIL\".\n\
             If you have the answer, say only \"DONE\".",
            problem
        );

        self.chat.send(&prompt).await
    }
}

/// Builder for constructing an `IterativeProblemSolver` with custom configuration.
pub struct IterativeProblemSolverBuilder {
    broker: LlmBroker,
    tools: Option<Vec<Box<dyn LlmTool>>>,
    max_iterations: usize,
    system_prompt: Option<String>,
}

impl IterativeProblemSolverBuilder {
    /// Create a new builder
    fn new(broker: LlmBroker) -> Self {
        Self {
            broker,
            tools: None,
            max_iterations: 3,
            system_prompt: None,
        }
    }

    /// Set the tools available to the problem solver
    pub fn tools(mut self, tools: Vec<Box<dyn LlmTool>>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the maximum number of iterations (default: 3)
    pub fn max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Set a custom system prompt
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Build the problem solver
    pub fn build(self) -> IterativeProblemSolver {
        let system_prompt = self.system_prompt.unwrap_or_else(|| {
            "You are a problem-solving assistant that can solve complex problems step by step. \
             You analyze problems, break them down into smaller parts, and solve them systematically. \
             If you cannot solve a problem completely in one step, you make progress and identify what to do next."
                .to_string()
        });

        let mut chat_builder = ChatSession::builder(self.broker).system_prompt(system_prompt);

        if let Some(tools) = self.tools {
            chat_builder = chat_builder.tools(tools);
        }

        IterativeProblemSolver {
            chat: chat_builder.build(),
            max_iterations: self.max_iterations,
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
    use std::sync::{Arc, Mutex};

    // Mock gateway for testing
    struct MockGateway {
        responses: Vec<String>,
        call_count: Arc<Mutex<usize>>,
    }

    impl MockGateway {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses,
                call_count: Arc::new(Mutex::new(0)),
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
    async fn test_builder_default_settings() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let solver = IterativeProblemSolver::new(broker);

        assert_eq!(solver.max_iterations, 3);
    }

    #[tokio::test]
    async fn test_builder_custom_max_iterations() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let solver = IterativeProblemSolver::builder(broker).max_iterations(5).build();

        assert_eq!(solver.max_iterations, 5);
    }

    #[tokio::test]
    async fn test_builder_with_tools() {
        let gateway = Arc::new(MockGateway::new(vec![]));
        let broker = LlmBroker::new("test-model", gateway, None);

        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(MockTool {
            name: "test_tool".to_string(),
        })];

        let _solver = IterativeProblemSolver::builder(broker).tools(tools).build();

        // If this compiles and runs, the builder pattern works
    }

    #[tokio::test]
    async fn test_solve_completes_with_done() {
        let gateway = Arc::new(MockGateway::new(vec![
            "Working on it...".to_string(),
            "DONE".to_string(),
            "The answer is 42".to_string(),
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::new(broker);

        let result = solver.solve("Test problem").await.unwrap();

        assert_eq!(result, "The answer is 42");
    }

    #[tokio::test]
    async fn test_solve_fails_with_fail() {
        let gateway = Arc::new(MockGateway::new(vec![
            "Trying...".to_string(),
            "FAIL".to_string(),
            "Could not solve the problem".to_string(),
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::new(broker);

        let result = solver.solve("Impossible problem").await.unwrap();

        assert_eq!(result, "Could not solve the problem");
    }

    #[tokio::test]
    async fn test_solve_stops_at_max_iterations() {
        let gateway = Arc::new(MockGateway::new(vec![
            "Step 1".to_string(),
            "Step 2".to_string(),
            "Step 3".to_string(),
            "Final summary".to_string(),
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::builder(broker).max_iterations(3).build();

        let result = solver.solve("Long problem").await.unwrap();

        // Should have called the gateway 4 times: 3 iterations + 1 summary
        assert_eq!(result, "Final summary");
    }

    #[tokio::test]
    async fn test_solve_case_insensitive_done() {
        let gateway = Arc::new(MockGateway::new(vec![
            "done".to_string(),                 // lowercase "done"
            "The task is complete".to_string(), // summary
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::new(broker);

        let result = solver.solve("Test problem").await.unwrap();

        assert_eq!(result, "The task is complete");
    }

    #[tokio::test]
    async fn test_solve_case_insensitive_fail() {
        let gateway = Arc::new(MockGateway::new(vec![
            "fail".to_string(),                    // lowercase "fail"
            "Unable to complete task".to_string(), // summary
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::new(broker);

        let result = solver.solve("Test problem").await.unwrap();

        assert_eq!(result, "Unable to complete task");
    }

    #[tokio::test]
    async fn test_custom_system_prompt() {
        let gateway =
            Arc::new(MockGateway::new(vec!["DONE".to_string(), "Custom response".to_string()]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::builder(broker)
            .system_prompt("Custom system prompt for testing")
            .build();

        let result = solver.solve("Test problem").await.unwrap();

        assert_eq!(result, "Custom response");
    }

    #[tokio::test]
    async fn test_step_method() {
        let gateway = Arc::new(MockGateway::new(vec!["Step response".to_string()]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::new(broker);

        let result = solver.step("Test problem").await.unwrap();

        assert_eq!(result, "Step response");
    }

    #[tokio::test]
    async fn test_multiple_iterations_before_done() {
        let gateway = Arc::new(MockGateway::new(vec![
            "Working...".to_string(),
            "Still working...".to_string(),
            "Almost there...".to_string(),
            "DONE".to_string(),
            "Completed successfully".to_string(),
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::builder(broker).max_iterations(5).build();

        let result = solver.solve("Complex problem").await.unwrap();

        assert_eq!(result, "Completed successfully");
    }

    #[tokio::test]
    async fn test_done_substring_detection() {
        let gateway = Arc::new(MockGateway::new(vec![
            "I'm DONE with this task".to_string(), // Contains "DONE"
            "Task completed".to_string(),
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::new(broker);

        let result = solver.solve("Test problem").await.unwrap();

        assert_eq!(result, "Task completed");
    }

    #[tokio::test]
    async fn test_fail_substring_detection() {
        let gateway = Arc::new(MockGateway::new(vec![
            "This will FAIL".to_string(), // Contains "FAIL"
            "Failed to complete".to_string(),
        ]));
        let broker = LlmBroker::new("test-model", gateway, None);
        let mut solver = IterativeProblemSolver::new(broker);

        let result = solver.solve("Test problem").await.unwrap();

        assert_eq!(result, "Failed to complete");
    }
}
