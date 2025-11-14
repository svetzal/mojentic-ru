//! Embeddings Example
//!
//! This example demonstrates how to generate embeddings using the Ollama gateway.
//! Embeddings are vector representations of text that capture semantic meaning,
//! useful for similarity search, clustering, and other NLP tasks.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example embeddings
//! ```
//!
//! # Requirements
//!
//! - Ollama must be running locally (http://localhost:11434)
//! - The mxbai-embed-large model must be available
//!   Pull it with: `ollama pull mxbai-embed-large`

use mojentic::llm::gateway::LlmGateway;
use mojentic::llm::gateways::OllamaGateway;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging to see gateway operations
    tracing_subscriber::fmt::init();

    // Create Ollama gateway
    let gateway = OllamaGateway::new();

    // Generate embeddings for a simple text
    println!("Generating embeddings for: 'Hello, world!'");
    let embeddings =
        gateway.calculate_embeddings("Hello, world!", Some("mxbai-embed-large")).await?;

    // Print the embedding dimensions
    println!("Embedding dimensions: {}", embeddings.len());

    // Optionally, print the first few values to show it's a real vector
    if embeddings.len() >= 5 {
        println!(
            "First 5 values: [{:.4}, {:.4}, {:.4}, {:.4}, {:.4}]",
            embeddings[0], embeddings[1], embeddings[2], embeddings[3], embeddings[4]
        );
    }

    Ok(())
}
