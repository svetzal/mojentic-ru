# Getting Started

This section helps you install and begin using Mojentic in Rust.

## Install

Add the dependency (placeholder until published):

```toml
[dependencies]
mojentic = { path = "../" }
```

## Minimal Example

```rust
use mojentic::{Broker, Message};

fn main() {
    let mut broker = Broker::default();
    broker.emit(Message::text("hello world"));
}
```

## Next Steps
- Learn the LLM broker → [Broker](broker.md)
- Dive into Core concepts → [Layer 1 overview](core/README.md)
