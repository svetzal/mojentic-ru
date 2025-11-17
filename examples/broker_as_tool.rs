//! Broker as Tool Example
//!
//! This example demonstrates wrapping agents (broker + tools + behavior) as tools
//! that can be used by other agents. This enables agent delegation patterns where
//! a coordinator agent can delegate work to specialist agents.
//!
//! Run with: cargo run --example broker_as_tool

use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use mojentic::llm::{LlmBroker, LlmMessage, LlmTool, ToolWrapper};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt::init();

    println!("Broker as Tool Example - Agent Delegation");
    println!("==========================================\n");

    // Create the Ollama gateway
    let gateway = Arc::new(OllamaGateway::default());

    // Create a temporal specialist agent
    // This agent specializes in resolving dates and temporal information
    let temporal_broker = Arc::new(LlmBroker::new("qwen2.5:7b", gateway.clone(), None));
    let temporal_tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];
    let temporal_behaviour = "You are a temporal specialist who helps resolve dates and time-related questions. \
                              When asked about dates, use the resolve_date tool to provide accurate date information.";

    // Wrap the temporal specialist as a tool
    let temporal_tool = ToolWrapper::new(
        temporal_broker,
        temporal_tools,
        temporal_behaviour,
        "temporal_specialist",
        "A specialist agent that resolves dates and temporal information. \
         Handles questions like 'what day is tomorrow', 'date in 3 days', etc.",
    );

    // Create a coordinator agent with the temporal specialist as a tool
    let coordinator_broker = LlmBroker::new("qwen2.5:14b", gateway, None);
    let coordinator_tools: Vec<Box<dyn LlmTool>> = vec![Box::new(temporal_tool)];

    let coordinator_behaviour = "You are a coordinator who can delegate tasks to specialist agents. \
                                 When users ask about dates or temporal information, use the temporal_specialist tool.";

    // Create the initial messages for the coordinator
    let mut messages = vec![LlmMessage::system(coordinator_behaviour)];

    // Test 1: Simple date query
    println!("Test 1: Simple date query");
    println!("-------------------------");
    let query1 = "What day is tomorrow?";
    println!("User: {}", query1);

    messages.push(LlmMessage::user(query1));

    let response1 = coordinator_broker
        .generate(&messages, Some(&coordinator_tools), None, None)
        .await?;

    println!("Coordinator: {}\n", response1);

    // Test 2: More complex temporal query
    println!("Test 2: Complex temporal query");
    println!("-------------------------------");
    let query2 = "What will the date be 5 days from now?";
    println!("User: {}", query2);

    // Start fresh conversation for clarity
    messages = vec![LlmMessage::system(coordinator_behaviour)];
    messages.push(LlmMessage::user(query2));

    let response2 = coordinator_broker
        .generate(&messages, Some(&coordinator_tools), None, None)
        .await?;

    println!("Coordinator: {}\n", response2);

    // Test 3: Show that coordinator delegates appropriately
    println!("Test 3: Direct question (no delegation needed)");
    println!("----------------------------------------------");
    let query3 = "What is 2 + 2?";
    println!("User: {}", query3);

    messages = vec![LlmMessage::system(coordinator_behaviour)];
    messages.push(LlmMessage::user(query3));

    let response3 = coordinator_broker
        .generate(&messages, Some(&coordinator_tools), None, None)
        .await?;

    println!("Coordinator: {}\n", response3);

    println!("\nExample completed successfully!");
    println!("\nKey takeaways:");
    println!("- ToolWrapper allows wrapping agents as tools");
    println!("- Coordinator can delegate to specialist agents");
    println!("- Each agent has its own broker, tools, and behavior");
    println!("- The pattern enables clean separation of concerns");

    Ok(())
}
