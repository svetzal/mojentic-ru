# oMLX Gateway

[oMLX](https://github.com/jundot/omlx) is an LLM server for Apple Silicon.
`OmlxGateway` connects Mojentic to it.

oMLX uses the OpenAI chat completions protocol, but do not use
`OpenAIGateway` for it. The OpenAI gateway changes requests for model names
that it does not know, it drops the reasoning trace, and it splits embedding
input with the OpenAI tokenizer. `OmlxGateway` sends your settings as you set
them.

## Quick start

```rust,no_run
use mojentic::llm::gateways::OmlxGateway;
use mojentic::llm::{LlmBroker, LlmMessage};
use std::sync::Arc;

#[tokio::main]
async fn main() -> mojentic::Result<()> {
    let gateway = Arc::new(OmlxGateway::new());
    let broker = LlmBroker::new("Qwen3.8-27B-MLX-8bit", gateway, None);

    let messages = vec![LlmMessage::user("Name three rivers in Canada.")];
    let reply = broker.generate(&messages, None, None, None).await?;
    println!("{reply}");
    Ok(())
}
```

The `omlx` example runs one turn and shows the thinking and the evidence:

```bash
cargo run --example omlx -- Qwen3.8-27B-MLX-8bit
```

## Configuration

| Field | Environment variable | Default |
| ----- | -------------------- | ------- |
| `host` | `OMLX_HOST` | `http://localhost:8000` |
| `api_key` | `OMLX_API_KEY` | none |
| `timeout` | `OMLX_TIMEOUT`, in milliseconds | 600000 (10 minutes) |

- Give the host without `/v1`. The gateway adds `/v1` to each path.
- When you set an API key, the gateway sends `Authorization: Bearer <key>`.
  When you do not, it sends no authorization header. An empty `OMLX_API_KEY`
  counts as no key. An explicit empty or whitespace-only key also sends no
  authorization header and does not fall back to the environment.
- The timeout bounds non-streaming requests, including model load. Streaming
  requests omit this whole-response timeout so long replies can finish. Drop
  the stream to cancel its request.
  When `OMLX_TIMEOUT` is not a number, the gateway logs a warning and uses the
  default. Set `timeout: None` for no timeout.

`OmlxConfig::default()` reads the environment. A field that you set yourself
has priority. The environment has priority above the defaults:

```rust,no_run
use mojentic::llm::gateways::{OmlxConfig, OmlxGateway};
use std::time::Duration;

let gateway = OmlxGateway::with_config(OmlxConfig {
    host: "http://studio.local:8000".to_string(),
    timeout: Some(Duration::from_secs(1800)),
    ..Default::default()
});
```

## Request settings

The gateway sends these `CompletionConfig` fields to oMLX:

| Field | Sent as |
| ----- | ------- |
| `temperature` | `temperature` |
| `max_tokens` | `max_tokens`, always. Never `max_completion_tokens` |
| `top_p`, `top_k` | `top_p`, `top_k`, when set |
| `reasoning_effort` | `reasoning_effort` or `enable_thinking`, when set. See below |
| `response_format` | `response_format`, the same as the OpenAI gateway |

The gateway does not send `num_ctx` or `num_predict`. oMLX sets the context
length for each model.

### Reasoning effort

| `reasoning_effort` | Sent as |
| ------------------ | ------- |
| not set | nothing. The model uses its default. Qwen 3 models think by default |
| `Low`, `Medium`, `High` | `reasoning_effort: "low"`, `"medium"` or `"high"` |
| `Disabled` | `enable_thinking: false`, and no `reasoning_effort` |

oMLX gives `reasoning_effort` to the model's chat template, so the effect of
`low`, `medium` and `high` depends on the model. `enable_thinking: false`
turns thinking off.

## Thinking

oMLX reports the model's reasoning trace as `reasoning_content`. The gateway
puts it in `LlmGatewayResponse::thinking`. When the model does not think,
`thinking` is `None`.

In the streaming APIs:

- `complete_stream` yields reasoning deltas as `StreamChunk::Thinking`.
- `generate_stream_events` yields no event for reasoning. It has no thinking
  event.

### Truncation during thinking

When `max_tokens` stops the model while it thinks, oMLX puts the partial
reasoning in `content`, sets no `reasoning_content`, and reports the finish
reason `length`. A streamed response keeps the partial reasoning as thinking.
The gateway does not move text between the fields.

When the finish reason in `response.evidence` is not `stop`, `content` is not
an answer. Read the finish reason before you use `content`.

## Structured output

`generate_object`, `complete_json` and `complete_json_response` send
`response_format: {"type": "json_schema", "json_schema": {"name": "response",
"schema": ...}}`. The gateway parses the content as JSON. You must still
validate the object.

When oMLX cannot compile a grammar for the schema, it does not enforce the
format. It adds instructions to the prompt and sends a `Warning` response
header. When you asked for structured output (the object APIs, or a
`ResponseFormat::JsonObject` with or without a schema), the gateway:

- puts the header value in `response.evidence.metadata` with the key
  `response_format_warning` (the constant `omlx::RESPONSE_FORMAT_WARNING`).
  It joins several `Warning` headers with `, `.
- logs a warning.

The gateway does not retry and does not fail. The warning is evidence. For a
text format or no format, the gateway ignores the header.

## Streaming

oMLX starts each stream with a keep-alive frame whose `model` is
`keepalive`, and sends more of them during a long prefill. The gateway drops
these frames before it parses the stream, so `keepalive` never shows as the
provider model.

`generate_stream_events` uses the OpenAI completion rules. It asks for usage
with `stream_options.include_usage`. A clean finish needs the finish reason
`stop` and the `data: [DONE]` marker. See
[Streaming](streaming.md#single-turn-streaming-with-terminal-completion-evidence).

`complete_stream` yields content, thinking and tool calls. oMLX sends all of
a tool call in one delta. The gateway yields it as one
`StreamChunk::ToolCalls`.

## Models

```rust,no_run
use mojentic::llm::gateways::OmlxGateway;
use mojentic::llm::LlmGateway;

# async fn run() -> mojentic::Result<()> {
let gateway = OmlxGateway::new();
let models = gateway.get_available_models().await?; // sorted ids
gateway.load_model(&models[0]).await?;
gateway.unload_model(&models[0]).await?;
# Ok(())
# }
```

- Blank model ids for load or unload fail before a request is sent.
- `load_model` blocks until the model is in memory. A chat request loads its
  model when necessary, so use `load_model` only to warm up a model early.
- `unload_model` for a model that is not loaded fails with the 400 error
  from oMLX.
- oMLX downloads models only through its admin dashboard. The gateway has no
  pull operation.

## Embeddings

```rust,no_run
# use mojentic::llm::gateways::OmlxGateway;
# use mojentic::llm::LlmGateway;
# async fn run() -> mojentic::Result<()> {
let gateway = OmlxGateway::new();
let embedding = gateway.calculate_embeddings("A river delta", Some("my-embedding-model")).await?;
# Ok(())
# }
```

- You must give a model. oMLX has no default embedding model. Without a model,
  the gateway returns `MojenticError::InvalidArgument` and sends no request.
- The gateway sends the text in one request. It does not split the text.
- A chat model fails with a 400 error from oMLX.

## Errors

oMLX errors use the OpenAI shape. The gateway reports each one as
`MojenticError::GatewayError`, with the HTTP status and the response body. In
`generate_stream_events`, the error is `StreamEventError::ProviderError`, with
the status and the body's `error` value.

| Status | Type | Cause |
| ------ | ---- | ----- |
| 401 | `authentication_error` | The API key is missing or wrong |
| 404 | `not_found_error` | The model is unknown. The message lists the available models |
| 400 | `invalid_request_error` | A chat model was used for embeddings, or an unload of a model that is not loaded |
