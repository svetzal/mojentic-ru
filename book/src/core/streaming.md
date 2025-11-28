# Streaming Responses

Streaming allows you to receive LLM responses chunk-by-chunk as they are generated, improving perceived latency for users.

## Basic Streaming

Use `broker.generate_stream` to get a stream of chunks:

```rust
use mojentic::prelude::*;
use futures::StreamExt;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);
    
    let messages = vec![LlmMessage::user("Tell me a story.")];

    let mut stream = broker.generate_stream(&messages, None, None).await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => print!("{}", chunk),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
    
    Ok(())
}
```

## Streaming with Tools

Mojentic supports streaming even when tools are involved. The broker will pause streaming to execute tools and then resume streaming the final response.

```rust
use mojentic::tools::DateResolver;

let tools: Vec<Arc<dyn LlmTool>> = vec![Arc::new(DateResolver::new())];
let mut stream = broker.generate_stream(&messages, Some(tools), None).await?;

// The stream will contain text chunks.
// Tool execution happens transparently in the background.
while let Some(result) = stream.next().await {
    // ...
}
```
