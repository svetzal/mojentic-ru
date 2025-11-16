//! Streaming Example - Demonstrates streaming text generation with tool calling
//!
//! This example shows how generate_stream() works with tools:
//! 1. Streams content as it arrives
//! 2. Detects tool calls in the stream
//! 3. Executes tools and recursively streams the LLM's response
//! 4. Provides seamless user experience with continuous streaming
//!
//! The example uses SimpleDateTool to resolve relative date expressions like
//! "three days from now" or "next week" in a streaming story.
//!
//! Run with: cargo run --example streaming

use futures::stream::StreamExt;
use mojentic::llm::broker::LlmBroker;
use mojentic::llm::gateways::ollama::OllamaGateway;
use mojentic::llm::models::LlmMessage;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use mojentic::llm::tools::LlmTool;
use std::sync::Arc;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up logging
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    println!("Streaming response with tool calling enabled...\n");

    // Create broker with Ollama
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Create date resolution tool
    let date_tool = SimpleDateTool;

    // Prepare tools for broker
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(date_tool)];

    // Stream a story that references relative dates
    // This will trigger the date resolution tool
    let messages = vec![LlmMessage::user(
        "Tell me a short story about a dragon. In your story, reference several dates \
         relative to today, like 'three days from now' or 'last week'. Keep it brief.",
    )];

    let mut stream = broker.generate_stream(&messages, Some(&tools), None, None);

    // Print chunks as they arrive
    // Tool calls will be executed transparently during the stream
    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => print!("{}", chunk),
            Err(e) => eprintln!("\nError: {}", e),
        }
    }

    println!("\n\nDone!");
    Ok(())
}
