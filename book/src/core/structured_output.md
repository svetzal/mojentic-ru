# Tutorial: Extracting Structured Data

## Why Use Structured Output?

LLMs are great at generating text, but sometimes you need data in a machine-readable format like JSON. Structured output allows you to define a schema (using Rust structs with Serde) and force the LLM to return data that matches that schema.

This is essential for:
- Data extraction from unstructured text
- Building API integrations
- Populating databases
- ensuring reliable downstream processing

## Getting Started

Let's build an example that extracts user information from a natural language description.

### 1. Define Your Data Schema

We use `serde` to define the structure we want.

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct UserInfo {
    name: String,
    age: u32,
    interests: Vec<String>,
}
```

### 2. Initialize the Broker

```rust
use mojentic::prelude::*;
use std::sync::Arc;

let gateway = Arc::new(OllamaGateway::new());
let broker = LlmBroker::new("qwen3:32b", gateway, None);
```

### 3. Generate Structured Data

Use `broker.generate_structured` to request the data.

```rust
let text = "John Doe is a 30-year-old software engineer who loves hiking and reading.";

let user_info: UserInfo = broker.generate_structured(text).await?;

println!("{:?}", user_info);
// UserInfo {
//   name: "John Doe",
//   age: 30,
//   interests: ["hiking", "reading"]
// }
```

## How It Works

1.  **Schema Definition**: Mojentic uses `schemars` to convert your Rust struct into a JSON schema that the LLM can understand.
2.  **Prompt Engineering**: The broker automatically appends instructions to the prompt, telling the LLM to output JSON matching the schema.
3.  **Validation**: When the response comes back, Mojentic parses the JSON and deserializes it into your struct, performing validation (e.g., ensuring `age` is a number).

## Advanced: Nested Schemas

You can also use nested structs for more complex data.

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Address {
    street: String,
    city: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct UserProfile {
    name: String,
    address: Address,
}
```

## Summary

Structured output turns unstructured text into reliable data structures. By defining Rust structs, you can integrate LLM outputs directly into your application's logic with type safety and validation.
