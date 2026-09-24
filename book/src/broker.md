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

### Structured output in streaming requests

`CompletionConfig.response_format` asks the provider for a response format. The
OpenAI and Ollama gateways send it in streaming and non-streaming requests alike.

| `response_format` | OpenAI request | Ollama request |
| ----------------- | -------------- | -------------- |
| `None` | unchanged | unchanged |
| `Some(ResponseFormat::Text)` | `response_format: {"type": "text"}` | no `format` field |
| `Some(ResponseFormat::JsonObject { schema: None })` | `response_format: {"type": "json_object"}` | `format: "json"` |
| `Some(ResponseFormat::JsonObject { schema: Some(s) })` | `response_format: {"type": "json_schema", "json_schema": {"name": "response", "schema": s}}` | `format: s` |

```rust
use mojentic::llm::gateway::{CompletionConfig, ResponseFormat};

let config = CompletionConfig {
    response_format: Some(ResponseFormat::JsonObject {
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"]
        })),
    }),
    ..Default::default()
};
let mut stream = broker.generate_stream(&messages, None, Some(config), None);
```

The format records what you asked for. It does not prove that the provider
enforced it. Parse and validate the streamed content before you use it.

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

## Caller-owned context and native responses

Use `broker.generate_response(&messages, Some(&tools), Some(config), None).await` to receive one native gateway response without
executing tools, extending history or making a follow-up request. Assemble the
complete message array before each call. The broker traces the supplied request
and returned response; it does not read repository guidance or apply a context policy.

The existing convenience completion method still executes tools and follows up.
Choose a serial or parallel runner according to the tools' effects. Parallel
execution does not make dependent edits safe.

Set `CompletionConfig::default().with_unlimited_tool_iterations()` to disable the tool-round limit.
Existing finite defaults remain unchanged. Concurrency controls simultaneous
work; it is not a task budget or a loop detector.

The unlimited builder uses `usize::MAX` as a symbolic value and bypasses the iteration check; existing numeric configuration remains compatible.

Native responses preserve the fields supplied by the gateway. Missing provider
usage or termination evidence must remain unknown; configured model names and
text length are not substitutes for reported metadata.
