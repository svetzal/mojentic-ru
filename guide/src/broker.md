# LLM Broker Guide

The `Broker` is the central interface for interacting with Large Language Models in Mojentic. It provides a consistent, type-safe API across different LLM providers.

## Overview

```
Your App → Broker → Gateway → LLM Provider
                ↓
            Tool Execution
```

## Creating a Broker

```rust
use mojentic::llm::{Broker, gateways::OllamaGateway};

// Basic broker
let gateway = OllamaGateway::default();
let broker = Broker::new("qwen3:32b", gateway);

// With correlation ID for request tracking
let broker = Broker::with_correlation_id(
    "qwen3:32b",
    gateway,
    "request-abc-123"
);
```

### Broker Structure

```rust
pub struct Broker<G: Gateway> {
    model: String,
    gateway: G,
    correlation_id: Option<String>,
}
```

The broker is generic over the gateway type, ensuring type safety at compile time.

## Text Generation

Basic text generation:

```rust
use mojentic::llm::Message;

let messages = vec![
    Message::user("Explain Rust's ownership in one sentence")
];

let response = broker.generate(&messages, None).await?;
println!("{}", response);
```

### With Configuration

```rust
use mojentic::llm::CompletionConfig;

let config = CompletionConfig {
    temperature: 0.3,  // Lower = more focused
    max_tokens: Some(500),
    ..Default::default()
};

let response = broker
    .generate(&messages, Some(config))
    .await?;
```

### With Tools

```rust
use mojentic::llm::Tool;

let tools: Vec<Box<dyn Tool>> = vec![
    Box::new(DateResolver),
    Box::new(CurrentDateTime),
];

let response = broker
    .generate_with_tools(&messages, &tools, Some(config))
    .await?;
```

## Structured Output

Generate responses conforming to JSON schemas:

```rust
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize, Serialize)]
struct Analysis {
    title: String,
    summary: String,
    keywords: Vec<String>,
}

let schema = json!({
    "type": "object",
    "properties": {
        "title": {"type": "string"},
        "summary": {"type": "string"},
        "keywords": {
            "type": "array",
            "items": {"type": "string"}
        }
    },
    "required": ["title", "summary"]
});

let messages = vec![
    Message::user("Analyze: Rust is a systems programming language")
];

let analysis: Analysis = broker
    .generate_object(&messages, &schema, None)
    .await?;

println!("{:#?}", analysis);
```

## Tool Execution Flow

When tools are provided, the Broker handles a recursive loop:

1. Send messages to LLM
2. If LLM requests tools:
   - Execute each tool concurrently
   - Add tool results to conversation
   - Return to step 1
3. Return final text response

```rust
// The broker automatically:
// 1. Calls LLM with the question
// 2. LLM responds with tool call request
// 3. Broker executes DateResolver::run(args)
// 4. Broker adds result to conversation
// 5. Calls LLM again with tool result
// 6. LLM responds with final answer

let messages = vec![Message::user("What's the date next Monday?")];
let tools: Vec<Box<dyn Tool>> = vec![Box::new(DateResolver)];

let response = broker
    .generate_with_tools(&messages, &tools, None)
    .await?;
```

## Message Management

```rust
use mojentic::llm::Message;

let messages = vec![
    Message::system("You are a helpful coding assistant"),
    Message::user("How do I read a file in Rust?"),
    Message::assistant("You can use std::fs::read_to_string()..."),
    Message::user("What about async?"),
];

let response = broker.generate(&messages, None).await?;
```

### Message Types

- `Message::system(content)` - Set LLM behavior
- `Message::user(content)` - User input  
- `Message::assistant(content)` - LLM responses (for history)
- Tool-related messages for tool execution flow

## Error Handling

The Broker returns `Result<T, Error>`:

```rust
use mojentic::error::Error;

match broker.generate(&messages, None).await {
    Ok(response) => {
        // Success
        println!("{}", response);
    }
    Err(Error::Timeout) => {
        // Request timed out
        eprintln!("Timeout");
    }
    Err(Error::InvalidResponse) => {
        // LLM returned invalid format
        eprintln!("Invalid response");
    }
    Err(Error::HttpError(429)) => {
        // Rate limited
        eprintln!("Rate limited");
    }
    Err(Error::HttpError(status)) => {
        // Other HTTP errors
        eprintln!("HTTP {}", status);
    }
    Err(Error::GatewayError(msg)) => {
        // Gateway-specific error
        eprintln!("Gateway: {}", msg);
    }
    Err(e) => {
        // Other errors
        eprintln!("Error: {}", e);
    }
}
```

### Error Recovery

```rust
async fn generate_with_retry(
    broker: &Broker<impl Gateway>,
    messages: &[Message],
    max_retries: u32
) -> Result<String, Error> {
    let mut attempts = 0;
    
    loop {
        match broker.generate(messages, None).await {
            Ok(response) => return Ok(response),
            Err(Error::Timeout) if attempts < max_retries => {
                attempts += 1;
                eprintln!("Timeout, retrying {}/{}...", attempts, max_retries);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Correlation IDs

Track requests across your system:

```rust
// Generate unique ID
let correlation_id = uuid::Uuid::new_v4().to_string();
let broker = Broker::with_correlation_id(
    "qwen3:32b",
    gateway,
    &correlation_id
);

// ID flows through all operations
let response = broker.generate(&messages, None).await?;

// Find logs/events by correlation_id
```

## Configuration Options

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

// Constrained generation
let config = CompletionConfig {
    temperature: 0.7,
    num_predict: Some(256),
    ..Default::default()
};
```

## Best Practices

### 1. Use System Messages

```rust
let messages = vec![
    Message::system(
        "You are a helpful assistant that provides concise answers. \
         Always format code examples with proper syntax highlighting."
    ),
    Message::user("How do I create a struct?"),
];
```

### 2. Handle Errors Properly

```rust
use mojentic::error::Error;

let result = broker.generate(&messages, None).await;

match result {
    Ok(response) => process(response),
    Err(Error::Timeout) => retry(),
    Err(Error::HttpError(429)) => backoff_and_retry(),
    Err(e) => {
        log::error!("LLM error: {}", e);
        return fallback_response();
    }
}
```

### 3. Manage Context Windows

```rust
fn keep_recent_messages(messages: &[Message], max: usize) -> Vec<Message> {
    if messages.len() <= max {
        messages.to_vec()
    } else {
        messages[messages.len() - max..].to_vec()
    }
}

let messages = keep_recent_messages(&conversation_history, 10);
```

### 4. Use Appropriate Timeouts

```rust
use std::time::Duration;

let gateway = OllamaGateway::new(
    "http://localhost:11434",
    Duration::from_secs(600)  // 10 minutes for large models
);
```

## Advanced Usage

### Concurrent Requests

```rust
use futures::future::try_join_all;

let tasks: Vec<_> = inputs
    .iter()
    .map(|input| {
        let messages = vec![Message::user(input)];
        broker.generate(&messages, None)
    })
    .collect();

let responses = try_join_all(tasks).await?;
```

### Multiple Models

```rust
// Different models for different tasks
let fast_broker = Broker::new("qwen3:7b", gateway.clone());
let smart_broker = Broker::new("qwen3:32b", gateway);

// Quick classification
let category = fast_broker.generate(&classify_msgs, None).await?;

// Deep analysis
let analysis = smart_broker.generate(&analysis_msgs, None).await?;
```

### Streaming Responses

```rust
use futures::StreamExt;

let mut stream = broker.generate_stream(&messages, None).await?;

while let Some(chunk) = stream.next().await {
    match chunk {
        Ok(text) => print!("{}", text),
        Err(e) => eprintln!("Stream error: {}", e),
    }
}
```

## Type Safety Benefits

Rust's type system prevents common errors:

```rust
// Cannot create invalid message roles
let msg = Message::user("text");  // ✓ Valid
// let msg = Message("invalid");  // ✗ Compile error

// Gateway trait ensures consistent API
fn process<G: Gateway>(broker: &Broker<G>) {
    // Works with any gateway implementation
}

// Result types force error handling
let response = broker.generate(&messages, None).await;
// Must handle Result - cannot ignore errors
```

## See Also

- [Getting Started](./getting-started.md)
- [Tool Usage](./tool-usage.md)
- [Structured Output](./structured-output.md)
- [Error Handling](./error-handling.md)
