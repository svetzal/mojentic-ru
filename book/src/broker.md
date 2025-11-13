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
