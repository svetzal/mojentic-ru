# The Basics

A quick orientation to the core types and flows.

- Create messages (system, user, assistant)
- Configure a completion
- Send to a gateway via the broker

```rust
use mojentic::llm::{Broker, CompletionConfig, Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let broker = Broker::new()?;
    let cfg = CompletionConfig::default();
    let msgs = vec![
        Message::system("You are concise"),
        Message::user("Explain Rust lifetimes in one line"),
    ];
    let out = broker.chat(cfg, msgs).await?;
    println!("{}", out.text());
    Ok(())
}
```
