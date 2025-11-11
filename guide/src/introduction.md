# Introduction to Mojentic for Rust

Mojentic is a comprehensive LLM integration framework designed for building reliable, type-safe AI-powered applications. The Rust implementation leverages Rust's strong type system, zero-cost abstractions, and memory safety guarantees.

## Philosophy

The Rust port of Mojentic follows Rust's core principles:

- **Type Safety**: Make invalid states unrepresentable
- **Zero-Cost Abstractions**: High-level features without runtime overhead
- **Memory Safety**: No segfaults, no data races
- **Explicit Error Handling**: Result types for all fallible operations
- **Fearless Concurrency**: Safe concurrent operations with async/await

## Core Features

### Layer 1: LLM Integration

- **LLM Broker**: Type-safe interface for LLM interactions
- **Multiple Gateways**: Support for Ollama (OpenAI and Anthropic planned)
- **Tool Calling**: Recursive tool execution with strongly-typed arguments
- **Structured Output**: Schema-based JSON parsing with serde
- **Message History**: Conversation context management
- **Streaming**: Real-time response streaming (Ollama gateway)
- **Async Operations**: Built on Tokio for high-performance async I/O

### Type Safety

Mojentic uses Rust's type system to prevent errors at compile time:

```rust
// Message roles are enums, not strings
let message = Message::user("Hello");

// Tool calls are strongly typed
let tool_call = ToolCall {
    id: "call-1".to_string(),
    name: "my_tool".to_string(),
    arguments: serde_json::json!({"param": "value"}),
};

// Results are explicit
match broker.generate(&messages, None).await {
    Ok(response) => println!("Success: {}", response),
    Err(e) => eprintln!("Error: {}", e),
}
```

## Quick Example

```rust
use mojentic::llm::{Broker, Message};
use mojentic::llm::gateways::OllamaGateway;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a broker
    let gateway = OllamaGateway::default();
    let broker = Broker::new("qwen3:14b", gateway);

    // Generate text
    let messages = vec![Message::user("What is Rust?")];
    let response = broker.generate(&messages, None).await?;

    println!("{}", response);
    Ok(())
}
```

## Architecture

Mojentic is organized into layers:

1. **Layer 1**: Core LLM integration (Broker, Gateways, Tools, Messages)
2. **Layer 2**: Tracer system for observability (planned)
3. **Layer 3**: Agent system for complex workflows (planned)

## Error Handling

All fallible operations return `Result<T, E>`:

```rust
use mojentic::error::Error;

match broker.generate(&messages, None).await {
    Ok(response) => {
        // Handle success
    }
    Err(Error::Timeout) => {
        // Handle timeout
    }
    Err(Error::HttpError(status)) => {
        // Handle HTTP errors
    }
    Err(e) => {
        // Handle other errors
        eprintln!("Error: {}", e);
    }
}
```

## Performance

Rust implementation benefits:

- **Zero-cost abstractions**: No runtime overhead
- **Efficient async I/O**: Built on Tokio runtime
- **Memory efficiency**: Stack allocation where possible
- **Compile-time optimization**: Aggressive inlining and optimization

## Next Steps

- [Getting Started](./getting-started.md) - Installation and basic usage
- [Broker Guide](./broker.md) - Understanding the LLM Broker
- [Tool Usage](./tool-usage.md) - Building and using tools
- [Structured Output](./structured-output.md) - Working with schemas
