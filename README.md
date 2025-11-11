# Mojentic - Rust LLM Integration Framework

Mojentic is a modern LLM integration framework for Rust that provides a clean abstraction over multiple LLM providers with tool support, structured output generation, and an event-driven agent system.

## Features

- **Multi-Provider Support**: Works with OpenAI, Ollama, and Anthropic (in development)
- **Type-Safe**: Leverages Rust's type system for safe LLM interactions
- **Async-First**: Built on Tokio for efficient async operations
- **Tool System**: Extensible tool calling support for LLMs
- **Structured Output**: Generate type-safe structured data from LLMs
- **Local LLMs**: Full support for Ollama local models

## Quick Start

Add Mojentic to your `Cargo.toml`:

```toml
[dependencies]
mojentic = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

### Basic Usage

```rust
use mojentic::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));

    let messages = vec![
        LlmMessage::user("What is the capital of France?"),
    ];

    let response = broker.generate(&messages, None, None).await?;
    println!("Response: {}", response);

    Ok(())
}
```

## Prerequisites

To use Mojentic with local models, you need to have Ollama installed and running:

1. Install Ollama from [ollama.ai](https://ollama.ai)
2. Pull a model: `ollama pull qwen3:32b`
3. Verify it's running: `ollama list`

## Examples

The project includes several examples to get you started:

### Simple Text Generation
```bash
cargo run --example simple_llm
```

### Structured Output (Type-Safe)
```bash
cargo run --example structured_output
```

### Tool Usage
```bash
cargo run --example tool_usage
```

## Architecture

Mojentic is structured in layers:

### Layer 1: LLM Integration (Current Focus)
- `LlmBroker` - Main interface for LLM interactions
- `LlmGateway` trait - Abstract interface for LLM providers
- Gateway implementations: `OllamaGateway` (OpenAI and Anthropic coming soon)
- Message models and adapters
- Tool system

### Layer 2: Agent System (Future)
- Event-driven agent coordination
- Async event processing
- Router and dispatcher

## Development Status

This is an early-stage implementation focusing on:
- ✅ Core types and error handling
- ✅ LlmGateway trait
- ✅ Ollama gateway implementation
- ✅ LlmBroker with tool support
- 🚧 OpenAI gateway (planned)
- 🚧 Anthropic gateway (planned)
- 🚧 Agent system (planned)

## License

MIT
