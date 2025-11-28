# Example: Web Search

The `WebSearchTool` demonstrates how to integrate external APIs into your agent's toolset. This example implementation supports multiple providers like Tavily and Serper.

## Configuration

The web search tool typically requires an API key for a search provider (e.g., Tavily, Serper).

```rust
use std::env;

env::set_var("TAVILY_API_KEY", "your-api-key");
```

## Usage

```rust
use mojentic::prelude::*;
use mojentic::tools::WebSearchTool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize broker
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Register the tool
    let tools: Vec<Arc<dyn LlmTool>> = vec![
        Arc::new(WebSearchTool::new(SearchProvider::Tavily)),
    ];

    // Ask a question requiring up-to-date info
    let messages = vec![
        LlmMessage::user("What is the current stock price of Apple?"),
    ];

    let response = broker.generate(&messages, Some(tools), None).await?;
    println!("{}", response);
    
    Ok(())
}
```

## Supported Providers

- **Tavily**: Optimized for LLM agents
- **Serper**: Google Search API
- **DuckDuckGo**: Privacy-focused search (no API key required for basic usage)
