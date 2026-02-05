# Using LLMs (Broker)

Mojentic’s LLM broker routes completion requests to pluggable gateways (e.g., Ollama). It provides a unified API for chat and text completions.

## Quick chat example

```rust
use mojentic::llm::{Broker, CompletionConfig, Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let broker = Broker::new()?;
    let cfg = CompletionConfig::default();

    let resp = broker.chat(
        cfg,
        [
            Message::system("You are a helpful assistant"),
            Message::user("Say hi in one sentence"),
        ],
    ).await?;

    println!("{}", resp.text());
    Ok(())
}
```

## Gateways
- Ollama: local models for fast iteration.
- HTTP-based gateways: add your own by implementing the `Gateway` trait.

## Structured output
Use typed schemas to parse the model output into structs. See [Structured Output](core/structured_output.md).

## Configuration

Use `CompletionConfig` to control generation parameters:

```rust
use mojentic::llm::gateway::{CompletionConfig, ReasoningEffort};

let config = CompletionConfig {
    temperature: 0.3,
    reasoning_effort: Some(ReasoningEffort::High),
    ..Default::default()
};
```

### Available Parameters

- **temperature** (`f64`): Controls randomness. Default: 1.0
- **num_ctx** (`u32`): Context window size in tokens. Default: 32768
- **max_tokens** (`u32`): Maximum tokens to generate. Default: 16384
- **num_predict** (`i32`): Tokens to predict (-1 = no limit). Default: -1
- **reasoning_effort** (`Option<ReasoningEffort>`): Extended thinking level — `Low`, `Medium`, `High`, or `None`. Default: None

### Reasoning Effort

Control how much the model thinks before responding:

```rust
use mojentic::llm::gateway::{CompletionConfig, ReasoningEffort};

// Deep reasoning for complex problems
let config = CompletionConfig {
    reasoning_effort: Some(ReasoningEffort::High),
    temperature: 0.1,
    ..Default::default()
};

// Quick responses
let config = CompletionConfig {
    reasoning_effort: Some(ReasoningEffort::Low),
    ..Default::default()
};
```

- **Ollama**: Maps to `think: true` parameter for extended thinking. The model's reasoning trace is available in `LlmGatewayResponse.thinking`.
- **OpenAI**: Maps to `reasoning_effort` API parameter for reasoning models (o1, o3 series). Ignored with a warning for non-reasoning models.

For full details, see [Reasoning Effort Control](core/reasoning_effort.md).
