# Extending Mojentic

This guide shows how to extend the Mojentic framework with new gateways and custom functionality.

## Adding a New LLM Gateway

To add support for a new LLM provider, implement the `LlmGateway` trait:

### 1. Create the Gateway File

```rust
// src/llm/gateways/openai.rs
use crate::error::{MojenticError, Result};
use crate::llm::gateway::{CompletionConfig, LlmGateway};
use crate::llm::models::{LlmGatewayResponse, LlmMessage};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use serde_json::Value;

pub struct OpenAiGateway {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAiGateway {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }
}

#[async_trait]
impl LlmGateway for OpenAiGateway {
    async fn complete(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse> {
        // Implement OpenAI completion API call
        todo!()
    }

    async fn complete_json(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<Value> {
        // Implement OpenAI structured output
        todo!()
    }

    async fn get_available_models(&self) -> Result<Vec<String>> {
        // Implement model listing
        todo!()
    }

    async fn calculate_embeddings(
        &self,
        text: &str,
        model: Option<&str>,
    ) -> Result<Vec<f32>> {
        // Implement embeddings API
        todo!()
    }
}
```

### 2. Export the Gateway

```rust
// src/llm/gateways/mod.rs
pub mod ollama;
pub mod openai;  // Add this line

pub use ollama::{OllamaConfig, OllamaGateway};
pub use openai::OpenAiGateway;  // Add this line
```

### 3. Use the Gateway

```rust
use mojentic::prelude::*;
use mojentic::llm::gateways::OpenAiGateway;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let gateway = OpenAiGateway::new("your-api-key".to_string());
    let broker = LlmBroker::new("gpt-4", Arc::new(gateway));

    // Use as normal
    let messages = vec![LlmMessage::user("Hello!")];
    let response = broker.generate(&messages, None, None).await?;

    Ok(())
}
```

## Creating Custom Message Types

You can create helper functions for specific message types:

```rust
use mojentic::llm::models::{LlmMessage, MessageRole};

impl LlmMessage {
    /// Create a message with an image
    pub fn user_with_image(content: impl Into<String>, image_path: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: Some(content.into()),
            tool_calls: None,
            image_paths: Some(vec![image_path.into()]),
        }
    }

    /// Create a system message with specific formatting
    pub fn system_instruction(instruction: impl Into<String>) -> Self {
        let content = format!("SYSTEM INSTRUCTION: {}", instruction.into());
        Self::system(content)
    }
}
```

## Implementing Structured Output Types

Define your own structured output types with JSON Schema:

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ProductReview {
    rating: u8,
    pros: Vec<String>,
    cons: Vec<String>,
    recommendation: bool,
    summary: String,
}

#[tokio::main]
async fn main() -> mojentic::Result<()> {
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));

    let messages = vec![
        LlmMessage::user(
            "Review this product: A wireless mouse with RGB lighting, \
             3200 DPI, 6 buttons, and 40-hour battery life."
        )
    ];

    let review: ProductReview = broker.generate_object(&messages, None).await?;

    println!("Rating: {}/5", review.rating);
    println!("Pros: {:?}", review.pros);
    println!("Cons: {:?}", review.cons);
    println!("Recommended: {}", review.recommendation);

    Ok(())
}
```

## Advanced: Custom Gateway Configuration

Create configuration structs for your gateways:

```rust
#[derive(Debug, Clone)]
pub struct CustomGatewayConfig {
    pub endpoint: String,
    pub timeout: std::time::Duration,
    pub retry_attempts: u32,
    pub custom_headers: HashMap<String, String>,
}

impl Default for CustomGatewayConfig {
    fn default() -> Self {
        Self {
            endpoint: std::env::var("CUSTOM_ENDPOINT")
                .unwrap_or_else(|_| "https://api.example.com".to_string()),
            timeout: std::time::Duration::from_secs(30),
            retry_attempts: 3,
            custom_headers: HashMap::new(),
        }
    }
}

pub struct CustomGateway {
    client: reqwest::Client,
    config: CustomGatewayConfig,
}

impl CustomGateway {
    pub fn new() -> Self {
        Self::with_config(CustomGatewayConfig::default())
    }

    pub fn with_config(config: CustomGatewayConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .unwrap();

        Self { client, config }
    }
}
```

## Testing Your Extensions

Create tests for your gateways and tools:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_models() {
        let gateway = OllamaGateway::new();
        let models = gateway.get_available_models().await.unwrap();
        assert!(!models.is_empty());
    }
}
```

## Best Practices

1. **Error Handling**: Always use proper error types, never panic in library code
2. **Async**: All I/O operations should be async
3. **Documentation**: Add rustdoc comments to all public APIs
4. **Testing**: Write tests for your implementations
5. **Configuration**: Support environment variables for API keys and endpoints
6. **Type Safety**: Leverage Rust's type system for compile-time guarantees

## Need Help?

- Check the existing implementations in `src/llm/gateways/ollama.rs`
- Review the tool example in `examples/tool_usage.rs`
- See the [Building Tools](../core/building_tools.md) guide for tool development
