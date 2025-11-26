//! List available models from Ollama gateway
//!
//! This example demonstrates how to query the Ollama server for available models.
//!
//! # Usage
//! ```bash
//! cargo run --example list_models
//! ```
//!
//! # Requirements
//! - Ollama running locally (default: http://localhost:11434)
//! - At least one model pulled (e.g., `ollama pull qwen3:32b`)

use mojentic::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create Ollama gateway
    let gateway = OllamaGateway::new();

    println!("Ollama Models:");
    println!();

    // Get available models
    match gateway.get_available_models().await {
        Ok(models) => {
            if models.is_empty() {
                println!("No models found.");
                println!();
                println!("Pull a model with:");
                println!("  ollama pull qwen3:32b");
            } else {
                for model in models {
                    println!("- {}", model);
                }
            }
        }
        Err(e) => {
            eprintln!("Error fetching models: {}", e);
            eprintln!();
            eprintln!("Make sure Ollama is running:");
            eprintln!("  ollama serve");
            eprintln!();
            eprintln!("And that you have at least one model pulled:");
            eprintln!("  ollama pull qwen3:32b");
        }
    }

    Ok(())
}
