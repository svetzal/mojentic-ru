# Embeddings

Embeddings allow you to convert text into vector representations, which are useful for semantic search, clustering, and similarity comparisons.

## Setup

You need an embedding model. Ollama supports models like `mxbai-embed-large` or `nomic-embed-text`.

```rust
use mojentic::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize gateway
    let gateway = Arc::new(EmbeddingsGateway::new("mxbai-embed-large"));
    
    Ok(())
}
```

## Generating Embeddings

```rust
let text = "The quick brown fox jumps over the lazy dog.";
let vector = gateway.embed(text).await?;

println!("{:?}", &vector[0..5]);
// => [0.123, -0.456, ...]
```

## Batch Processing

You can embed multiple texts at once:

```rust
let texts = vec!["Hello", "World"];
let vectors = gateway.embed_batch(texts).await?;
```

## Cosine Similarity

Mojentic provides utilities to calculate similarity between vectors:

```rust
use mojentic::utils::math::cosine_similarity;

let similarity = cosine_similarity(&vector1, &vector2);
```
