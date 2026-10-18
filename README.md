# Mojentic

[![Crates.io](https://img.shields.io/crates/v/mojentic.svg)](https://crates.io/crates/mojentic)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)

A modern LLM integration framework for Rust with full feature parity across Python, Elixir, and TypeScript implementations.

Mojentic provides a clean abstraction over multiple LLM providers with tool support, structured output generation, streaming, and a complete event-driven agent system.

## 🚀 Features

- **🔌 Multi-Provider Support**: OpenAI and Ollama gateways
- **🔒 Type-Safe**: Leverages Rust's type system for safe LLM interactions
- **⚡ Async-First**: Built on Tokio for efficient async operations
- **🛠️ Tool System**: Extensible tool calling with automatic recursive execution
- **📊 Structured Output**: Generate type-safe structured data with serde
- **🌊 Streaming**: Async streaming with `Pin<Box<dyn Stream>>`
- **🔍 Tracer System**: Complete observability for debugging and monitoring
- **🤖 Agent System**: Event-driven multi-agent coordination with ReAct pattern
- **📦 24 Examples**: Comprehensive examples demonstrating all features

## 📦 Installation

Add Mojentic to your `Cargo.toml`:

```toml
[dependencies]
mojentic = "3.0"
tokio = { version = "1", features = ["full"] }
```

## 🔧 Prerequisites

To use Mojentic with local models, you need Ollama installed and running:

1. Install Ollama from [ollama.ai](https://ollama.ai)
2. Pull a model: `ollama pull qwen3:32b`
3. Verify it's running: `ollama list`

## 🎯 Quick Start

```rust
use mojentic::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    let messages = vec![
        LlmMessage::user("What is the capital of France?"),
    ];

    let response = broker.generate(&messages, None, None).await?;
    println!("Response: {}", response);

    Ok(())
}
```

## 📚 Documentation

- **User Guide**: [mdBook documentation](https://svetzal.github.io/mojentic-ru/)
- **API Reference**: `cargo doc --no-deps --all-features --open`

Build documentation locally:

```bash
# Build API docs
cargo doc --no-deps --all-features

# Build the mdBook
mdbook build book
open book/book/index.html
```

## 🏗️ Architecture

Mojentic is structured in three layers:

### Layer 1: LLM Integration

- `LlmBroker` - Main interface for LLM interactions
- `LlmGateway` trait - Abstract interface for LLM providers
- `OllamaGateway` / `OpenAiGateway` - Provider implementations
- `ChatSession` - Conversational session management
- `TokenizerGateway` - Token counting with tiktoken-rs
- `EmbeddingsGateway` - Vector embeddings
- Comprehensive tool system with 10+ built-in tools

### Layer 2: Tracer System

- `TracerSystem` - Thread-safe event recording
- `EventStore` - Event persistence and querying
- Correlation ID tracking with `Arc` sharing
- LLM call, response, and tool events

### Layer 3: Agent System

- `AsyncDispatcher` - Event routing
- `Router` - Event-to-agent routing
- `BaseLlmAgent` / `AsyncLlmAgent` - LLM-powered agents
- `IterativeProblemSolver` - Multi-step reasoning
- `SimpleRecursiveAgent` - Self-recursive processing
- `SharedWorkingMemory` - Agent context sharing with `RwLock`
- ReAct pattern implementation

## 🧪 Examples

```bash
cargo run --example simple_llm
cargo run --example structured_output
cargo run --example tool_usage
cargo run --example streaming
cargo run --example chat_session
cargo run --example tracer_demo
cargo run --example async_llm
cargo run --example iterative_solver
```

## 🔧 Development

```bash
# Build
cargo build

# Test
cargo test

# Format
cargo fmt

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Security audit
cargo deny check
```

## 📄 License

MIT License - see [LICENSE](LICENSE)

## Credits

Mojentic is a [Mojility](https://mojility.com) product by Stacey Vetzal.
