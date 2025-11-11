# Getting Started with Mojentic

This guide will help you get up and running with Mojentic in your Rust project.

## Installation

Add Mojentic to your `Cargo.toml`:

```toml
[dependencies]
mojentic = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

## Prerequisites

Mojentic requires:

- Rust 1.70 or later
- A local or remote LLM service (Ollama, OpenAI, etc.)

### Setting up Ollama (Local LLMs)

1. Install Ollama from [ollama.ai](https://ollama.ai)
2. Pull a model:

```bash
ollama pull phi4:14b
```

3. Start the Ollama service (runs on `http://localhost:11434` by default)

## Your First LLM Call

Create a simple async application:

```rust
use mojentic::llm::{Broker, Message};
use mojentic::llm::gateways::OllamaGateway;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create gateway
    let gateway = OllamaGateway::default();

    // Create broker
    let broker = Broker::new("phi4:14b", gateway);

    // Create messages
    let messages = vec![Message::user("What is the capital of France?")];

    // Generate response
    match broker.generate(&messages, None).await {
        Ok(response) => {
            println!("AI: {}", response);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}
```

Run with:

```bash
cargo run
```

## Understanding the Broker

The `Broker` is your main interface to LLMs. It handles:

- Model selection
- Gateway routing
- Message formatting
- Tool execution
- Error handling
- Request correlation

### Basic Broker Usage

```rust
use mojentic::llm::{Broker, CompletionConfig};

// Simple creation
let broker = Broker::new("qwen3:14b", gateway);

// With correlation ID for tracking
let broker = Broker::with_correlation_id(
    "qwen3:14b",
    gateway,
    "request-123"
);

// With custom configuration
let config = CompletionConfig {
    temperature: 0.7,
    max_tokens: Some(1000),
    ..Default::default()
};

let response = broker
    .generate(&messages, Some(config))
    .await?;
```

## Working with Messages

Messages form the conversation context:

```rust
use mojentic::llm::Message;

let messages = vec![
    Message::system("You are a helpful assistant"),
    Message::user("What's the weather?"),
    Message::assistant("I don't have access to weather data"),
    Message::user("Can you tell me a joke instead?"),
];

let response = broker.generate(&messages, None).await?;
```

## Structured Output

Parse responses into strongly-typed structs:

```rust
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize, Serialize)]
struct Person {
    name: String,
    age: u32,
    hobbies: Vec<String>,
}

// Define schema
let schema = json!({
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "age": {"type": "integer"},
        "hobbies": {
            "type": "array",
            "items": {"type": "string"}
        }
    },
    "required": ["name", "age"]
});

let messages = vec![Message::user("Generate a person profile")];

let person: Person = broker
    .generate_object(&messages, &schema, None)
    .await?;

println!("{:#?}", person);
```

## Using Tools

Tools let LLMs perform actions:

```rust
use mojentic::llm::tools::DateResolver;

let messages = vec![Message::user("What date is next Friday?")];

// Tools implement the Tool trait
let tools: Vec<Box<dyn Tool>> = vec![
    Box::new(DateResolver),
];

let response = broker
    .generate_with_tools(&messages, &tools, None)
    .await?;
```

## Configuration Options

Customize LLM behavior:

```rust
use mojentic::llm::CompletionConfig;

// Creative writing
let config = CompletionConfig {
    temperature: 1.5,
    max_tokens: Some(2000),
    ..Default::default()
};

// Factual responses
let config = CompletionConfig {
    temperature: 0.1,
    max_tokens: Some(500),
    ..Default::default()
};

// Long context
let config = CompletionConfig {
    num_ctx: Some(32768),
    ..Default::default()
};

let response = broker
    .generate(&messages, Some(config))
    .await?;
```

## Error Handling

Mojentic uses the `thiserror` crate for error handling:

```rust
use mojentic::error::Error;

match broker.generate(&messages, None).await {
    Ok(response) => {
        // Handle success
        process_response(&response);
    }
    Err(Error::Timeout) => {
        // Handle timeout
        eprintln!("Request timed out");
    }
    Err(Error::HttpError(status)) => {
        // Handle HTTP errors
        eprintln!("HTTP error: {}", status);
    }
    Err(Error::GatewayError(msg)) => {
        // Handle gateway errors
        eprintln!("Gateway error: {}", msg);
    }
    Err(e) => {
        // Handle other errors
        eprintln!("Error: {}", e);
    }
}
```

## Async Runtime

Mojentic is built on Tokio for async operations:

```rust
// Single runtime
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Your async code here
    Ok(())
}

// Or use tokio::spawn for concurrent tasks
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handle1 = tokio::spawn(async {
        // Task 1
    });

    let handle2 = tokio::spawn(async {
        // Task 2
    });

    let _ = tokio::try_join!(handle1, handle2)?;
    Ok(())
}
```

## Environment Configuration

Configure via environment variables:

```bash
# Ollama
export OLLAMA_HOST=http://localhost:11434
export OLLAMA_TIMEOUT=300000  # 5 minutes in ms

# OpenAI (planned)
export OPENAI_API_KEY=your-api-key
```

Or in code:

```rust
use mojentic::llm::gateways::OllamaGateway;

let gateway = OllamaGateway::new(
    "http://localhost:11434",
    std::time::Duration::from_secs(300)
);

let broker = Broker::new("qwen3:14b", gateway);
```

## Complete Example

```rust
use mojentic::llm::{Broker, Message, CompletionConfig};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let gateway = OllamaGateway::default();
    let broker = Broker::with_correlation_id(
        "qwen3:32b",
        gateway,
        "request-001"
    );

    // Configure
    let config = CompletionConfig {
        temperature: 0.7,
        max_tokens: Some(500),
        ..Default::default()
    };

    // Create conversation
    let messages = vec![
        Message::system("You are a helpful Rust expert"),
        Message::user("Explain ownership in Rust"),
    ];

    // Generate
    match broker.generate(&messages, Some(config)).await {
        Ok(response) => {
            println!("Response:\n{}", response);
        }
        Err(Error::Timeout) => {
            eprintln!("Request timed out, try again");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
```

## Next Steps

- [Broker Guide](./broker.md) - Deep dive into the Broker
- [Tool Usage](./tool-usage.md) - Building custom tools
- [Structured Output](./structured-output.md) - Working with schemas
- [Error Handling](./error-handling.md) - Comprehensive error handling
- [Testing](./testing.md) - Testing your LLM integrations
