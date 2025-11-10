use mojentic::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct Sentiment {
    label: String,
    confidence: f32,
    reasoning: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create Ollama gateway
    let gateway = OllamaGateway::new();

    // Create broker with a local model
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));

    // Create a message asking for sentiment analysis
    let messages = vec![LlmMessage::user(
        "Analyze the sentiment of this text: 'I absolutely love this product! It exceeded all my expectations.'",
    )];

    // Generate a structured response
    println!("Generating structured sentiment analysis...");
    let sentiment: Sentiment = broker.generate_object(&messages, None).await?;

    println!("\nSentiment Analysis:");
    println!("  Label: {}", sentiment.label);
    println!("  Confidence: {:.2}", sentiment.confidence);
    println!("  Reasoning: {}", sentiment.reasoning);

    Ok(())
}
