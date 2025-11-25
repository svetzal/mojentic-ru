//! Solver Chat Session Example
//!
//! This example demonstrates using ChatSession with the IterativeProblemSolver
//! wrapped as a tool. This creates a powerful pattern where the chat session
//! can delegate complex multi-step problems to the solver agent.
//!
//! Run with: cargo run --example solver_chat_session

use mojentic::agents::IterativeProblemSolver;
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use mojentic::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use mojentic::llm::{ChatSession, LlmBroker};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;

/// A tool that wraps the IterativeProblemSolver agent
///
/// This allows the solver to be used as a tool within a ChatSession,
/// enabling the chat assistant to delegate complex problems to the solver.
struct IterativeProblemSolverTool {
    broker: Arc<LlmBroker>,
    tools: Vec<Box<dyn LlmTool>>,
}

impl IterativeProblemSolverTool {
    /// Create a new IterativeProblemSolverTool
    ///
    /// # Arguments
    ///
    /// * `broker` - The LLM broker to use for the solver
    /// * `tools` - The tools available to the solver
    fn new(broker: Arc<LlmBroker>, tools: Vec<Box<dyn LlmTool>>) -> Self {
        Self { broker, tools }
    }
}

impl Clone for IterativeProblemSolverTool {
    fn clone(&self) -> Self {
        Self {
            broker: self.broker.clone(),
            tools: self.tools.iter().map(|t| t.clone_box()).collect(),
        }
    }
}

impl LlmTool for IterativeProblemSolverTool {
    fn run(&self, args: &HashMap<String, Value>) -> mojentic::error::Result<Value> {
        let problem_to_solve =
            args.get("problem_to_solve").and_then(|v| v.as_str()).ok_or_else(|| {
                mojentic::error::MojenticError::ToolError(
                    "Missing required argument: problem_to_solve".to_string(),
                )
            })?;

        // Clone tools for the solver
        let solver_tools: Vec<Box<dyn LlmTool>> =
            self.tools.iter().map(|t| t.clone_box()).collect();

        // Create and run the solver
        let runtime = tokio::runtime::Handle::current();
        let broker_clone = (*self.broker).clone();

        let result = runtime.block_on(async move {
            let mut solver = IterativeProblemSolver::builder(broker_clone)
                .tools(solver_tools)
                .max_iterations(5)
                .build();

            solver.solve(problem_to_solve).await
        })?;

        Ok(json!({
            "solution": result
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "iterative_problem_solver".to_string(),
                description:
                    "Iteratively solve a complex multi-step problem using available tools."
                        .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "problem_to_solve": {
                            "type": "string",
                            "description": "The problem or request to be solved."
                        }
                    },
                    "required": ["problem_to_solve"],
                    "additionalProperties": false
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with WARN level to reduce noise
    tracing_subscriber::fmt().with_max_level(tracing::Level::WARN).init();

    println!("Solver Chat Session Example");
    println!("============================\n");
    println!("This example shows how to wrap IterativeProblemSolver as a tool");
    println!("within a ChatSession, enabling the assistant to delegate complex");
    println!("problems to the solver agent.\n");

    // Create LLM broker with Ollama
    let gateway = Arc::new(OllamaGateway::default());

    // Try qwq first, fallback to qwen3:32b if not available
    let model = "qwq";
    let broker = Arc::new(LlmBroker::new(model, gateway, None));

    // Create the solver tool with SimpleDateTool as the inner tool
    let solver_tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];
    let solver_tool = IterativeProblemSolverTool::new(broker.clone(), solver_tools);

    // Create chat session with the solver tool
    let chat_tools: Vec<Box<dyn LlmTool>> = vec![Box::new(solver_tool)];

    let mut session = ChatSession::builder((*broker).clone())
        .system_prompt(
            "You are a helpful assistant with access to an iterative problem solver. \
             When faced with complex multi-step problems or questions that require \
             reasoning and tool usage, use the iterative_problem_solver tool to \
             delegate the work to a specialized solver agent.",
        )
        .tools(chat_tools)
        .build();

    println!("Model: {}", model);
    println!("\nAsk me questions! Try complex queries like:");
    println!("  - What will the date be 5 days from now?");
    println!("  - Can you tell me what day it is tomorrow?");
    println!("  - What's the date next Friday?");
    println!("\nType your messages and press Enter. Send empty message to exit.\n");

    loop {
        // Get user input
        print!("Query: ");
        io::stdout().flush()?;

        let mut query = String::new();
        io::stdin().read_line(&mut query)?;
        let query = query.trim();

        // Exit on empty input
        if query.is_empty() {
            println!("\nGoodbye!");
            break;
        }

        // Send message and get response
        match session.send(query).await {
            Ok(response) => {
                println!("{}\n", response);
            }
            Err(e) => {
                eprintln!("Error: {}\n", e);
            }
        }
    }

    Ok(())
}
