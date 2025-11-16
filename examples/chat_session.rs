//! Basic interactive chat session example
//!
//! This example demonstrates how to use ChatSession for an interactive
//! conversation with an LLM. The session maintains conversation history
//! and automatically manages the context window.
//!
//! Run with: cargo run --example chat_session

use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::{ChatSession, LlmBroker};
use std::io::{self, Write};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt::init();

    // Create LLM broker with Ollama
    let gateway = Arc::new(OllamaGateway::default());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Create chat session with default settings
    let mut session = ChatSession::new(broker);

    println!("Chat Session Example");
    println!("===================");
    println!("Type your messages and press Enter. Send empty message to exit.\n");

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
