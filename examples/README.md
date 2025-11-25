# Mojentic Rust Examples

This directory contains example programs demonstrating various features of the Mojentic Rust library.

## Prerequisites

Before running these examples, ensure you have:

1. **Rust installed** (version 1.70 or later)
2. **Ollama running locally** at `http://localhost:11434`
3. **At least one model pulled**, for example:
   ```bash
   ollama pull phi4:14b
   ollama pull qwen2.5:3b
   ```

## Available Examples

### Level 1: Basic LLM Usage ✅

All Level 1 examples are complete and ready to use.

#### `simple_llm.rs`
Basic text generation with a local LLM model.

```bash
cargo run --example simple_llm
```

Demonstrates:
- Creating a broker with Ollama gateway
- Sending a single user message
- Receiving and displaying a response

#### `list_models.rs`
List all available models from the Ollama gateway (if implemented).

```bash
cargo run --example list_models
```

Demonstrates:
- Querying available models
- Error handling for gateway connection issues

#### `structured_output.rs`
Generate structured JSON output using a schema.

```bash
cargo run --example structured_output
```

Demonstrates:
- Defining a JSON schema
- Getting structured data from the LLM
- Parsing and using the structured response

#### `tool_usage.rs`
Use tools (functions) that the LLM can call.

```bash
cargo run --example tool_usage
```

Demonstrates:
- Defining and implementing tools
- LLM making tool calls
- Executing tool functions
- Sending tool results back to the LLM

#### `broker_as_tool.rs`
Wrap agents as tools for delegation patterns.

```bash
cargo run --example broker_as_tool
```

Demonstrates:
- Creating specialist agents with specific behaviors and tools
- Wrapping agents as tools using ToolWrapper
- Building coordinator agents that delegate to specialists
- Multi-agent collaboration patterns

### Level 2: Advanced LLM Features ⚠️

#### `image_analysis.rs` ✅
Analyze images using vision-capable models.

```bash
cargo run --example image_analysis
```

Demonstrates:
- Multimodal messages with images
- Base64 encoding of image files
- Using vision-capable models (qwen3-vl, gemma3, llava)
- Extracting text from images

**Requirements**: Vision-capable model such as `qwen3-vl:30b`, `gemma3:27b`, or `llava:latest`

#### `working_memory.rs` ✅
Shared context and memory between agents.

```bash
cargo run --example working_memory
```

Demonstrates:
- SharedWorkingMemory for maintaining state across agent interactions
- Deep merging of JSON memory updates
- RequestAgent that learns from conversations
- Event-driven architecture with custom events
- AsyncDispatcher and Router coordination

**Requirements**: Any capable model such as `qwen2.5:14b`, `deepseek-r1:70b`, or similar

## Configuration

### Environment Variables

You can customize behavior using these environment variables:

- `OLLAMA_HOST` - Ollama server URL (default: `http://localhost:11434`)
- `RUST_LOG` - Logging level (e.g., `debug`, `info`, `warn`, `error`)

Example:
```bash
export OLLAMA_HOST=http://localhost:11434
export RUST_LOG=debug
cargo run --example simple_llm
```

## Troubleshooting

### "Connection refused" or gateway errors

Make sure Ollama is running:
```bash
ollama serve
```

### Model not found

Pull the model first:
```bash
ollama pull phi4:14b
```

### Timeout errors

Large models may take longer to respond. The default timeout is configured in the gateway, but you can adjust it if needed by modifying the `OllamaConfig`.

## Building and Testing

Build all examples:
```bash
cargo build --examples
```

Run tests:
```bash
cargo test
```

Run tests with output:
```bash
cargo test -- --nocapture
```

## Next Steps

After running these examples, check out:

- [Main README](../README.md) - Library overview and installation
- [API Documentation](https://docs.rs/mojentic) - Online documentation
- [Test Suite](../src/) - More examples in the test code

## Coming Soon

The following examples are planned for future implementation:

### Level 2: Advanced LLM Features (Remaining)
- `streaming.rs` - Streaming responses (Ollama partial support)
- `chat_session.rs` - Interactive chat sessions
- `broker_examples.rs` - Comprehensive broker feature tests

### Level 3: Tool System Extensions
- `file_tool.rs` - File operations
- `task_manager.rs` - Task management

### Level 4: Tracing & Observability
- `tracer_demo.rs` - Event tracing and debugging

See [PARITY.md](../../PARITY.md) for the complete feature roadmap across all language ports.
