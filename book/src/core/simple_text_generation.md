# Simple Text Generation

The minimal path from prompt to text.

```rust
use mojentic::llm::{Broker, CompletionConfig, Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let resp = Broker::new()?
        .chat(CompletionConfig::default(), [Message::user("Haiku about Rust")])
        .await?;
    println!("{}", resp.text());
    Ok(())
}
```
