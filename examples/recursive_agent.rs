//! Example: Using the SimpleRecursiveAgent
//!
//! For comprehensive documentation on the SimpleRecursiveAgent pattern, see:
//! book/src/agents/sra.md
//!
//! This example demonstrates how to create and use a SimpleRecursiveAgent to solve
//! problems asynchronously, including event handling and concurrent problem-solving.
//!
//! Run with: cargo run --example recursive_agent
//!
//! Note: Requires Ollama to be running with the qwen3:32b model

use mojentic::agents::simple_recursive_agent::{
    AnySolverEvent, GoalAchievedEvent, GoalFailedEvent, GoalSubmittedEvent,
    IterationCompletedEvent, SimpleRecursiveAgent,
};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use mojentic::llm::tools::LlmTool;
use mojentic::llm::LlmBroker;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n{}", "=".repeat(80));
    println!("SIMPLE RECURSIVE AGENT - EXAMPLE");
    println!("{}\n", "=".repeat(80));

    // Initialize the LLM broker with Ollama
    let gateway = Arc::new(OllamaGateway::default());
    let broker = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));

    // Example 1: Basic usage
    example_1_basic_usage(broker.clone()).await?;

    // Example 2: With event handling
    example_2_event_handling(broker.clone()).await?;

    // Example 3: With tools
    example_3_with_tools(broker.clone()).await?;

    // Example 4: Running multiple problems concurrently
    example_4_concurrent_solving(broker.clone()).await?;

    // Example 5: Custom system prompt
    example_5_custom_system_prompt(broker.clone()).await?;

    println!("{}", "=".repeat(80));
    println!("All examples completed!");
    println!("{}", "=".repeat(80));

    Ok(())
}

/// Example 1: Basic usage
async fn example_1_basic_usage(broker: Arc<LlmBroker>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Example 1: Basic Usage\n");

    let agent = SimpleRecursiveAgent::builder(broker).max_iterations(3).build();

    let problem = "What is the capital of France?";
    println!("Problem: {}", problem);

    let solution = agent.solve(problem).await?;
    println!("Solution: {}\n", solution);

    Ok(())
}

/// Example 2: With event handling
async fn example_2_event_handling(
    broker: Arc<LlmBroker>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Example 2: With Event Handling\n");

    let agent = SimpleRecursiveAgent::builder(broker).max_iterations(3).build();

    let problem = "What are the three primary colors?";
    println!("Problem: {}", problem);

    // Set up event handlers for monitoring the solution process
    let iteration_count = Arc::new(Mutex::new(0));
    let iteration_count_clone = iteration_count.clone();

    agent
        .emitter
        .subscribe(move |event: AnySolverEvent| {
            let iteration_count = iteration_count_clone.clone();
            tokio::spawn(async move {
                match event {
                    AnySolverEvent::GoalSubmitted(GoalSubmittedEvent { state }) => {
                        println!("  🎯 Goal submitted: {}", state.goal);
                    }
                    AnySolverEvent::IterationCompleted(IterationCompletedEvent {
                        state,
                        response: _,
                    }) => {
                        *iteration_count.lock().await += 1;
                        println!("  🔄 Iteration {} completed", state.iteration);
                    }
                    AnySolverEvent::GoalAchieved(GoalAchievedEvent { state }) => {
                        println!("  ✅ Problem solved after {} iterations", state.iteration);
                    }
                    AnySolverEvent::GoalFailed(GoalFailedEvent { state }) => {
                        println!("  ❌ Problem failed after {} iterations", state.iteration);
                    }
                    AnySolverEvent::Timeout(_) => {
                        println!("  ⏱️  Problem timed out");
                    }
                }
            });
        })
        .await;

    let solution = agent.solve(problem).await?;

    // Give async event handlers time to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("Solution: {}\n", solution);

    Ok(())
}

/// Example 3: With tools
async fn example_3_with_tools(broker: Arc<LlmBroker>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Example 3: With Tools\n");

    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];

    let agent = SimpleRecursiveAgent::builder(broker).tools(tools).max_iterations(5).build();

    let problem = "What's the date next Friday?";
    println!("Problem: {}", problem);

    let solution = agent.solve(problem).await?;
    println!("Solution: {}\n", solution);

    Ok(())
}

/// Example 4: Running multiple problems concurrently
async fn example_4_concurrent_solving(
    broker: Arc<LlmBroker>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Example 4: Concurrent Problem Solving\n");
    println!("Running multiple problems concurrently...");

    let agent = Arc::new(SimpleRecursiveAgent::builder(broker).max_iterations(3).build());

    let problems = vec![
        "What is the Pythagorean theorem?",
        "Explain the concept of recursion in programming.",
    ];

    // Create tasks for all problems and run them concurrently
    let mut tasks = Vec::new();

    for problem in problems {
        let agent = agent.clone();
        let task = tokio::spawn(async move {
            println!("\nStarted solving: {}", problem);
            let solution = agent.solve(problem).await.unwrap();
            println!("\nSolution for '{}':\n{}", problem, solution);
            solution
        });
        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        task.await?;
    }

    println!("\nAll concurrent problems have been solved!");

    Ok(())
}

/// Example 5: Custom system prompt
async fn example_5_custom_system_prompt(
    broker: Arc<LlmBroker>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n{}", "=".repeat(80));
    println!("Example 5: Custom System Prompt\n");

    let custom_prompt = "You are a concise assistant that provides brief, factual answers. \
                         Always respond in exactly one sentence.";

    let agent = SimpleRecursiveAgent::builder(broker)
        .max_iterations(3)
        .system_prompt(custom_prompt)
        .build();

    let problem = "What is Rust?";
    println!("Problem: {}", problem);

    let solution = agent.solve(problem).await?;
    println!("Solution: {}\n", solution);

    Ok(())
}
