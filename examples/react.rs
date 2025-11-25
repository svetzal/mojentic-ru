//! ReAct Pattern Example
//!
//! This example demonstrates a Reasoning and Acting (ReAct) loop where agents
//! iteratively plan, decide, act, and summarize to answer user queries.
//!
//! The ReAct pattern consists of:
//! 1. Thinking Agent - Creates plans
//! 2. Decisioning Agent - Decides next actions
//! 3. Tool Call Agent - Executes tools
//! 4. Summarization Agent - Generates final answers
//!
//! # Usage
//!
//! ```bash
//! cargo run --example react
//! ```
//!
//! # Requirements
//!
//! - Ollama running locally on http://localhost:11434
//! - A model like qwen3:32b pulled and available

use mojentic::async_dispatcher::AsyncDispatcher;
use mojentic::examples::react::{
    CurrentContext, DecisioningAgent, FailureOccurred, FinishAndSummarize, InvokeDecisioning,
    InvokeThinking, InvokeToolCall, SummarizationAgent, ThinkingAgent, ToolCallAgent,
};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::LlmBroker;
use mojentic::router::Router;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mojentic=info".parse().unwrap())
                .add_directive("react=info".parse().unwrap()),
        )
        .init();

    println!("\n{}", "=".repeat(80));
    println!("Starting ReAct Pattern Example");
    println!("{}", "=".repeat(80));

    // Initialize LLM broker with Ollama gateway
    let gateway = Arc::new(OllamaGateway::new());
    let llm = Arc::new(LlmBroker::new("qwen3:8b", gateway, None));
    // Alternative models (uncomment if available):
    // let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
    // let llm = Arc::new(LlmBroker::new("deepseek-r1:70b", gateway, None));

    // Create agents
    let thinking_agent = Arc::new(ThinkingAgent::new(llm.clone()));
    let decisioning_agent = Arc::new(DecisioningAgent::new(llm.clone()));
    let tool_call_agent = Arc::new(ToolCallAgent::new());
    let summarization_agent = Arc::new(SummarizationAgent::new(llm.clone()));

    // Configure router - maps event types to agent handlers
    let mut router = Router::new();
    router.add_route::<InvokeThinking>(thinking_agent);
    router.add_route::<InvokeDecisioning>(decisioning_agent);
    router.add_route::<InvokeToolCall>(tool_call_agent);
    router.add_route::<FinishAndSummarize>(summarization_agent.clone());
    router.add_route::<FailureOccurred>(summarization_agent); // Handle failures with same agent

    // Create and start dispatcher
    let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
    dispatcher.start().await?;

    // Create initial context with user query
    let initial_context = CurrentContext::new("What is the date next Friday?");

    println!("User Query: {}", initial_context.user_query);
    println!("{}\n", "=".repeat(80));

    // Create and dispatch initial thinking event
    let initial_event = Box::new(InvokeThinking {
        source: "main".to_string(),
        correlation_id: None,
        context: initial_context,
    }) as Box<dyn mojentic::event::Event>;

    dispatcher.dispatch(initial_event);

    // Wait for the event queue to become empty (processing complete)
    println!("Processing events...\n");
    let completed = dispatcher.wait_for_empty_queue(Some(Duration::from_secs(120))).await?;

    if !completed {
        eprintln!("Warning: Processing timed out after 120 seconds");
    }

    // Stop the dispatcher
    dispatcher.stop().await?;

    println!("\n{}", "=".repeat(80));
    println!("ReAct Pattern Example Complete");
    println!("{}\n", "=".repeat(80));

    Ok(())
}
