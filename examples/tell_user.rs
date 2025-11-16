//! Tell User Tool Example
//!
//! This example demonstrates how to use the TellUserTool to display
//! intermediate messages to the user without expecting a response.

use mojentic::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> mojentic::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create gateway and broker
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway), None);

    // Create the TellUser tool
    let tell_user_tool = mojentic::llm::tools::tell_user_tool::TellUserTool::new();

    // User request
    let user_request = "Tell me about the benefits of exercise.";

    // Create messages with a system prompt encouraging tool usage
    let messages = vec![
        LlmMessage::system(
            "You are a helpful assistant. Use the tell_user tool to share important intermediate information with the user as you work on their request."
        ),
        LlmMessage::user(user_request),
    ];

    println!("User Request:");
    println!("{}", user_request);
    println!("\nProcessing...\n");

    // Generate response with the TellUser tool
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(tell_user_tool)];

    let response = broker.generate(&messages, Some(&tools), None, None).await?;

    println!("\nFinal Response:");
    println!("{}", response);

    Ok(())
}
