//! Chat session with tool integration example
//!
//! This example demonstrates using ChatSession with tool calling.
//! The SimpleDateTool allows the LLM to resolve relative date expressions
//! like "tomorrow" or "3 days from now" into absolute dates.
//!
//! Run with: cargo run --example chat_session_with_tool

use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use mojentic::llm::{ChatSession, LlmBroker};
use std::io::{self, Write};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt::init();

    // Create LLM broker with Ollama
    let gateway = Arc::new(OllamaGateway::default());
    let broker = LlmBroker::new("qwen3:32b", gateway);

    // Create chat session with SimpleDateTool
    let tools: Vec<Box<dyn mojentic::llm::LlmTool>> = vec![Box::new(SimpleDateTool)];

    let mut session = ChatSession::builder(broker)
        .system_prompt(
            "You are a helpful assistant. When users ask about dates, \
             use the resolve_date tool to convert relative dates to absolute dates.",
        )
        .tools(tools)
        .build();

    println!("Chat Session with Tool Example");
    println!("==============================");
    println!("Ask me about dates! Try questions like:");
    println!("  - What is tomorrow's date?");
    println!("  - What day is 3 days from now?");
    println!("  - Tell me the date for next week");
    println!("\nType your messages and press Enter. Send empty message to exit.\n");

    loop {
        // Get user input
        print!("You: ");
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
        print!("Assistant: ");
        io::stdout().flush()?;

        match session.send(query).await {
            Ok(response) => {
                println!("{}\n", response);
            }
            Err(e) => {
                eprintln!("Error: {}\n", e);
            }
        }

        // Display token usage
        println!("(Total tokens: {})\n", session.total_tokens());
    }

    Ok(())
}
