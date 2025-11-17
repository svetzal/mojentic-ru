//! Example demonstrating the IterativeProblemSolver agent
//!
//! This example shows how to use the IterativeProblemSolver to solve
//! date-related queries using the SimpleDateTool and AskUserTool.
//! The agent iteratively works on the problem until it succeeds, fails,
//! or reaches the maximum number of iterations.
//!
//! Run with: cargo run --example iterative_solver

use mojentic::agents::IterativeProblemSolver;
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::ask_user_tool::AskUserTool;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use mojentic::llm::{LlmBroker, LlmTool};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    println!("Iterative Problem Solver Example");
    println!("=================================\n");

    // Initialize the LLM broker with Ollama
    let gateway = Arc::new(OllamaGateway::default());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Define the tools available to the solver
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(AskUserTool::new()), Box::new(SimpleDateTool)];

    // Create the problem solver with the tools
    let mut solver = IterativeProblemSolver::builder(broker).tools(tools).max_iterations(5).build();

    // Define the user's request
    let user_request = "What's the date next Friday?";

    println!("User Request:");
    println!("{}\n", user_request);
    println!("Processing...\n");

    // Run the solver and get the result
    match solver.solve(user_request).await {
        Ok(result) => {
            println!("\n{}", "=".repeat(50));
            println!("Agent Response:");
            println!("{}", result);
            println!("{}", "=".repeat(50));
        }
        Err(e) => {
            eprintln!("\nError: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
