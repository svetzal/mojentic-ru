# Streaming Responses

Streaming gives you a model's response in pieces as the provider generates it.
Mojentic has two streaming APIs:

- `generate_stream` streams text and runs tools between rounds. Use it for
  chat-style output where tools can take part.
- `generate_stream_events` streams one turn with no tools and ends with
  evidence of how the turn finished. Use it when truncated output must not be
  mistaken for a complete answer.

## Streaming with tools

`broker.generate_stream` returns a stream of `Result<String>` chunks:

```rust
use futures::StreamExt;
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::{LlmBroker, LlmMessage};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let broker = LlmBroker::new("qwen3:32b", Arc::new(OllamaGateway::new()), None);
    let messages = vec![LlmMessage::user("Tell me a story.")];

    let mut stream = broker.generate_stream(&messages, None, None, None);
    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => print!("{}", chunk),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
```

Pass tools to let the model call them. The broker stops streaming to run the
tools, then streams the model's follow-up response:

```rust
let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];
let mut stream = broker.generate_stream(&messages, Some(&tools), None, None);
```

`generate_stream` does not tell you why the provider stopped. A response cut
off at the token limit looks the same as a finished one.

## Single-turn streaming with terminal completion evidence

`broker.generate_stream_events(&messages, config, correlation_id)` returns a
stream of `StreamEvent` values:

| Event | Meaning |
| ----- | ------- |
| `StreamEvent::Content(text)` | Visible assistant content, in order |
| `StreamEvent::Completed(evidence)` | Terminal success |
| `StreamEvent::Error(error)` | Terminal failure |

Every stream ends with exactly one terminal event, and nothing follows it.
`Completed` carries a `ResponseEvidence` with the finish reason, usage (`None`
when the provider did not report it), provider model and provider metadata
(for example Ollama's `total_duration` and `eval_duration`).

```rust
use futures::StreamExt;
use mojentic::llm::{StreamEvent, StreamEventError};

let mut events = broker.generate_stream_events(&messages, None, None);
while let Some(event) = events.next().await {
    match event {
        StreamEvent::Content(text) => print!("{text}"),
        StreamEvent::Completed(evidence) => println!("\nusage: {:?}", evidence.usage),
        StreamEvent::Error(StreamEventError::IncompleteCompletion(evidence)) => {
            eprintln!("\ncut off: {:?}", evidence.finish_reason)
        }
        StreamEvent::Error(error) => eprintln!("\nfailed: {error}"),
        _ => {}
    }
}
```

Content that arrived before an error is evidence of what the provider sent. It
is not a result. Mojentic does not continue, retry or estimate anything. What
to do with incomplete output is your decision.

### When a turn is complete

- **OpenAI-compatible**: the stream needs a `finish_reason` of `stop` and then
  the `data: [DONE]` marker.
- **Ollama**: the final frame needs `done: true` and a `done_reason` of `stop`.
  Ollama servers too old to send `done_reason` always end with
  `IncompleteCompletion`, so they cannot use this API.

### Errors

| `StreamEventError` | Cause |
| ------------------ | ----- |
| `IncompleteCompletion(evidence)` | The terminal marker arrived with a finish reason other than `stop`, such as `length`. Carries finish reason, usage, provider model and metadata |
| `IncompleteStream(evidence)` | The stream ended without a terminal marker. Carries the evidence that arrived before the end, or `None` |
| `ProviderError { status, error }` | The provider sent an error frame, or answered with a non-success HTTP status (`status` is set) |
| `UnexpectedToolCalls` | The provider streamed a native tool call. This API sends no tools and runs none |
| `InvalidStreamEvent(reason)` | A frame could not be read |
| `StreamEventsUnsupported` | The gateway does not support this API. No request was sent |
| `RequestFailed(error)` | Building the request, connecting, or reading the body failed |

### Behaviour

- One HTTP request. Tool iterations are forced to zero. No retry, no
  continuation, no recursion.
- Dropping the stream, or breaking out of the loop that reads it, cancels the
  request.
- The OpenAI and Ollama gateways support this API. Other gateways yield a
  single `StreamEventsUnsupported` error. A custom gateway opts in by
  implementing `LlmGateway::complete_stream_events`.
- `CompletionConfig.response_format` is forwarded as described in
  [Using LLMs](../broker.md#structured-output-in-streaming-requests).
- OpenAI requests set `stream_options: {"include_usage": true}` so the
  provider reports usage.

### Tracing

With a tracer, the broker records the LLM call when the request starts. When
the terminal event arrives, success or failure, it records the response: the
content received so far and the evidence (`usage`, `provider_model`,
`finish_reason`, `metadata`). If you stop reading before the terminal event,
the call is traced and no response is. That is not an error. An unsupported
gateway records nothing.

## Provider-specific stream detail

The gateways' lower-level `complete_stream` also yields `StreamChunk::Thinking`,
`StreamChunk::Progress` and `StreamChunk::Metrics` for Ollama. The events API
does not expose these. Thinking text is not assistant content.
