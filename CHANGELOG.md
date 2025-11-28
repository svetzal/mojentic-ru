# Changelog

All notable changes to the Mojentic Rust implementation will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-11-27

### 🎉 First Stable Release

This release marks the first stable version of Mojentic for Rust, released simultaneously across all four language implementations (Python, Elixir, Rust, and TypeScript) with full feature parity.

### Highlights

- **Complete LLM Integration Layer**: Broker, OpenAI + Ollama gateways, structured output, tool calling, streaming with recursive tool execution, image analysis, tokenizer, embeddings
- **Full Tracer System**: Event recording, correlation tracking, event filtering, broker/tool integration
- **Complete Agent System**: Base agents, async agents, event system, dispatcher, router, aggregators, iterative solver, recursive agent, ReAct pattern, shared working memory
- **Comprehensive Tool Suite**: DateResolver, File tools (8 tools), Task manager, Tell user, Ask user, Web search, Current datetime, Tool wrapper (broker as tool)
- **24 Examples**: Full example suite demonstrating all major features
- **Type Safety**: Compile-time guarantees with Rust's type system

### Added

#### Layer 1: LLM Integration
- `LlmBroker` - Main interface for LLM interactions with recursive tool calling
- `LlmGateway` trait - Abstract interface for LLM providers
- `OllamaGateway` - Full Ollama implementation with async streaming
- `OpenAiGateway` - OpenAI gateway implementation
- `ChatSession` - Conversational session management
- `TokenizerGateway` - Token counting with tiktoken-rs
- `EmbeddingsGateway` - Vector embeddings support
- Async streaming with `Pin<Box<dyn Stream>>`

#### Layer 2: Tracer System
- `TracerSystem` - Event recording with thread-safe access
- `EventStore` - Event persistence and querying
- `TracerEvent` variants for LLM calls, responses, and tools
- Correlation ID tracking with `Arc` sharing

#### Layer 3: Agent System
- `BaseLlmAgent` - LLM-enabled agent foundation
- `AsyncLlmAgent` - Async agent with tokio runtime
- `AsyncAggregatorAgent` - Result aggregation
- `IterativeProblemSolver` - Multi-step reasoning
- `SimpleRecursiveAgent` - Self-recursive processing
- `AsyncDispatcher` - Event routing
- `Router` - Event-to-agent routing
- `SharedWorkingMemory` - Agent context sharing with `RwLock`
- ReAct pattern implementation

#### Tools
- `DateResolverTool` - Natural language date parsing
- `CurrentDatetimeTool` - Current time access
- `ToolWrapper` - Agent as tool delegation
- File tools: Read, Write, List, Exists, Delete, Move, Copy, Append
- `TaskManagerTool` - Ephemeral task management
- `TellUserTool` / `AskUserTool` - User interaction
- `WebSearchTool` - Organic web search

#### Infrastructure
- 365 tests passing
- Zero clippy warnings
- rustdoc + mdBook documentation
- cargo-deny security auditing
- Comprehensive error types with `thiserror`
