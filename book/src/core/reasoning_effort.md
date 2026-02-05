# Reasoning Effort Control

Some LLM models support extended thinking modes that allow them to reason more deeply about complex problems before responding. Mojentic provides reasoning effort control through the `CompletionConfig`.

## Overview

The `reasoning_effort` field allows you to control how much computational effort the model should spend on reasoning:

- **Low**: Quick responses with minimal reasoning
- **Medium**: Balanced approach (default reasoning depth)
- **High**: Extended thinking for complex problems

## Usage

```rust
use mojentic::llm::gateway::{CompletionConfig, ReasoningEffort};
use mojentic::llm::broker::LlmBroker;
use mojentic::llm::models::LlmMessage;
use mojentic::llm::gateways::ollama::OllamaGateway;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Configure with high reasoning effort
    let config = CompletionConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        ..Default::default()
    };

    let messages = vec![
        LlmMessage::system("You are a helpful assistant."),
        LlmMessage::user("Explain quantum entanglement in simple terms."),
    ];

    let response = broker.generate(&messages, None, Some(config), None).await?;
    println!("Response: {}", response);

    Ok(())
}
```

## Provider-Specific Behavior

### Ollama

When `reasoning_effort` is set (any value), Ollama receives the `think: true` parameter, which enables extended thinking mode. The model's internal reasoning is captured in the `thinking` field of the response:

```rust
let response = broker.generate(&messages, None, Some(config), None).await?;

// For Ollama, check the gateway response directly for thinking field
// The thinking content shows the model's internal reasoning process
```

### OpenAI

For OpenAI reasoning models (like o1), the `reasoning_effort` parameter maps directly to the API's `reasoning_effort` field:

- `ReasoningEffort::Low` → `"low"`
- `ReasoningEffort::Medium` → `"medium"`
- `ReasoningEffort::High` → `"high"`

For non-reasoning OpenAI models, the parameter is ignored and a warning is logged.

## When to Use Reasoning Effort

Use higher reasoning effort for:

- Complex mathematical problems
- Multi-step logical reasoning
- Code generation requiring architectural decisions
- Nuanced ethical or philosophical questions
- Tasks requiring careful consideration of trade-offs

Use lower reasoning effort for:

- Simple factual questions
- Quick classifications
- Routine formatting tasks
- When response speed is critical

## Performance Considerations

Higher reasoning effort typically means:

- Longer response times
- Increased token usage
- More thoughtful, accurate responses
- Better handling of edge cases

The trade-off between speed and quality should guide your choice of reasoning effort level.
