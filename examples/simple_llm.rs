use mojentic::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create Ollama gateway
    let gateway = OllamaGateway::new();

    // Create broker with a local model
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));

    // Create a simple message
    let messages = vec![LlmMessage::user("Explain what Rust is in one sentence.")];

    // Generate a response
    println!("Generating response...");
    let response = broker.generate(&messages, None, None).await?;

    println!("\nResponse: {}", response);

    Ok(())
}
