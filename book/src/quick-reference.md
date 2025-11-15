# Quick Reference

## Installation

```toml
[dependencies]
mojentic = { path = "./mojentic-rust" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
schemars = "0.8"  # For structured output
```

## Basic Usage

### Import the prelude
```rust
use mojentic::prelude::*;
use std::sync::Arc;
```

### Create a broker
```rust
let gateway = OllamaGateway::new();
let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));
```

### Simple text generation
```rust
let messages = vec![LlmMessage::user("Hello, how are you?")];
let response = broker.generate(&messages, None, None).await?;
```

## Message Types

### User message
```rust
LlmMessage::user("Your message here")
```

### System message
```rust
LlmMessage::system("You are a helpful assistant")
```

### Assistant message
```rust
LlmMessage::assistant("Response from assistant")
```

### Message with images
```rust
LlmMessage::user("Describe this image")
    .with_images(vec!["path/to/image.jpg".to_string()])
```

## Structured Output

### Define your type
```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Person {
    name: String,
    age: u32,
    occupation: String,
}
```

### Generate typed response
```rust
let messages = vec![
    LlmMessage::user("Extract person info: John Doe, 30, software engineer")
];

let person: Person = broker.generate_object(&messages, None).await?;
```

## Configuration

### Default config
```rust
let config = CompletionConfig::default();
// temperature: 1.0
// num_ctx: 32768
// max_tokens: 16384
```

### Custom config
```rust
let config = CompletionConfig {
    temperature: 0.7,
    num_ctx: 16384,
    max_tokens: 8192,
    num_predict: Some(1000),
};

let response = broker.generate(&messages, None, Some(config)).await?;
```

## Gateway Configuration

### Ollama with custom host
```rust
let gateway = OllamaGateway::with_host("http://localhost:11434");
```

### Ollama with environment variable
```rust
// Set OLLAMA_HOST environment variable
std::env::set_var("OLLAMA_HOST", "http://custom-host:11434");
let gateway = OllamaGateway::new();
```

## Error Handling

```rust
use mojentic::Result;

async fn my_function() -> Result<String> {
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));

    let messages = vec![LlmMessage::user("Hello")];

    match broker.generate(&messages, None, None).await {
        Ok(response) => Ok(response),
        Err(e) => {
            eprintln!("Error: {}", e);
            Err(e)
        }
    }
}
```

## Common Patterns

### Multi-turn conversation
```rust
let mut messages = vec![
    LlmMessage::system("You are a helpful assistant"),
];

// First turn
messages.push(LlmMessage::user("What is Rust?"));
let response1 = broker.generate(&messages, None, None).await?;
messages.push(LlmMessage::assistant(response1));

// Second turn
messages.push(LlmMessage::user("What are its benefits?"));
let response2 = broker.generate(&messages, None, None).await?;
```

### Async parallel requests
```rust
use tokio::try_join;

let broker = Arc::new(broker);
let broker1 = broker.clone();
let broker2 = broker.clone();

let (response1, response2) = try_join!(
    async move {
        broker1.generate(
            &[LlmMessage::user("Question 1")],
            None,
            None
        ).await
    },
    async move {
        broker2.generate(
            &[LlmMessage::user("Question 2")],
            None,
            None
        ).await
    }
)?;
```

### Model management (Ollama)
```rust
let gateway = OllamaGateway::new();

// List available models
let models = gateway.get_available_models().await?;

// Pull a model
gateway.pull_model("qwen3:32b").await?;

// Calculate embeddings
let embedding = gateway.calculate_embeddings(
    "Some text to embed",
    Some("mxbai-embed-large")
).await?;
```

## Logging

Enable logging with tracing:

```rust
// In main.rs
use tracing_subscriber;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Your code here
}
```

Set log level via environment variable:
```bash
RUST_LOG=debug cargo run
RUST_LOG=info cargo run
RUST_LOG=mojentic=debug cargo run
```
