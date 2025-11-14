# Chat Sessions

`ChatSession` provides a high-level abstraction for managing multi-turn conversations with LLMs. It automatically handles conversation history, context window management, and token counting.

## Key Features

- **Automatic History Management**: Maintains conversation history automatically
- **Context Window Trimming**: Removes oldest messages when token limit is exceeded
- **System Prompt Preservation**: Always keeps the system prompt (index 0) intact
- **Token Counting**: Uses `TokenizerGateway` for accurate token tracking
- **Builder Pattern**: Flexible configuration with sensible defaults

## Basic Usage

The simplest way to create a chat session:

```rust
use mojentic::llm::{ChatSession, LlmBroker};
use mojentic::llm::gateways::OllamaGateway;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create broker
    let gateway = Arc::new(OllamaGateway::default());
    let broker = LlmBroker::new("qwen3:32b", gateway);

    // Create session with defaults
    let mut session = ChatSession::new(broker);

    // Send messages
    let response = session.send("What is Rust?").await?;
    println!("{}", response);

    // Continue the conversation
    let response = session.send("Tell me more about ownership.").await?;
    println!("{}", response);

    Ok(())
}
```

## Custom Configuration

Use the builder pattern for custom configuration:

```rust
use mojentic::llm::ChatSession;

let session = ChatSession::builder(broker)
    .system_prompt("You are a helpful coding assistant specializing in Rust.")
    .temperature(0.7)
    .max_context(16384)  // 16k tokens
    .build();
```

### Configuration Options

- **`system_prompt`**: The system message (default: "You are a helpful assistant.")
- **`temperature`**: Sampling temperature (default: 1.0)
- **`max_context`**: Maximum tokens in context window (default: 32768)
- **`tokenizer_gateway`**: Custom tokenizer (default: cl100k_base)
- **`tools`**: Tools available to the LLM (default: None)

## Context Window Management

When the total token count exceeds `max_context`, `ChatSession` automatically removes the oldest messages:

```rust
let mut session = ChatSession::builder(broker)
    .max_context(2048)  // Small context window
    .build();

// Add many messages
for i in 0..100 {
    session.send(&format!("Message {}", i)).await?;
}

// Only recent messages are kept
println!("Messages in history: {}", session.messages().len());
println!("Total tokens: {}", session.total_tokens());
```

**Important**: The system prompt (index 0) is always preserved, even when trimming.

## Message History

Access the conversation history:

```rust
// Get all messages
for msg in session.messages() {
    println!("{:?}: {}", msg.role(), msg.content().unwrap_or(""));
}

// Check token usage
println!("Total tokens: {}", session.total_tokens());
```

## SizedLlmMessage

Internally, `ChatSession` uses `SizedLlmMessage`, which extends `LlmMessage` with token count metadata:

```rust
pub struct SizedLlmMessage {
    pub message: LlmMessage,
    pub token_length: usize,
}
```

This allows efficient context window management without recounting tokens repeatedly.

## Interactive Example

See the complete interactive chat example:

```bash
cargo run --example chat_session
```

This example demonstrates:
- Reading user input from stdin
- Maintaining conversation context
- Displaying token usage
- Graceful exit handling

## Best Practices

1. **Choose appropriate context size**: Balance between conversation length and performance
2. **Monitor token usage**: Use `session.total_tokens()` to track usage
3. **Set clear system prompts**: Guide the LLM's behavior from the start
4. **Adjust temperature**: Lower for deterministic responses, higher for creativity
5. **Handle errors gracefully**: Network and API errors can occur

## Error Handling

```rust
match session.send("Hello").await {
    Ok(response) => println!("Response: {}", response),
    Err(e) => eprintln!("Error: {}", e),
}
```

Common errors:
- Network failures connecting to Ollama
- Model not available
- Context window exceeded before trimming
- Tool execution failures (when using tools)

## Next Steps

- Learn about [Chat Sessions with Tools](chat_sessions_with_tools.md)
- Explore [Tool Usage](tool_usage.md)
- Read about [Building Tools](building_tools.md)
