# Image Analysis

Vision-capable models can analyze images by passing image file paths along with text prompts. The framework automatically handles reading image files and encoding them as Base64 for transmission to the LLM.

## Basic Usage

Attach images to a message using the `with_images()` method:

```rust
use mojentic::prelude::*;

let message = LlmMessage::user("Describe this image")
    .with_images(vec!["/path/to/image.jpg".to_string()]);
```

You can attach multiple images to a single message:

```rust
let message = LlmMessage::user("Compare these images")
    .with_images(vec![
        "/path/to/image1.jpg".to_string(),
        "/path/to/image2.jpg".to_string(),
    ]);
```

## Complete Example

```rust
use mojentic::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Create gateway and broker with a vision-capable model
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("llava:latest", Arc::new(gateway), None);

    // Create a message with image
    let message = LlmMessage::user("What's in this image?")
        .with_images(vec!["path/to/image.jpg".to_string()]);

    // Generate response
    let response = broker.generate(&[message], None, None, None).await?;
    println!("{}", response);

    Ok(())
}
```

## Vision-Capable Models

Common vision-capable models available through Ollama:

- `llava:latest` - General-purpose vision model
- `bakllava:latest` - BakLLaVA vision model
- `qwen3-vl:30b` - Qwen3 vision-language model
- `gemma3:27b` - Gemma 3 with vision support

Pull a model before using:
```bash
ollama pull llava:latest
```

## How It Works

When you attach images to a message:

1. **File Reading**: The gateway reads the image file from the specified path
2. **Base64 Encoding**: The image bytes are encoded as Base64 using the `base64` crate
3. **API Transmission**: The encoded image is included in the `images` field of the Ollama API request
4. **Model Processing**: The vision-capable model analyzes both the text prompt and image(s)

## Error Handling

Image processing can fail if:
- The image file doesn't exist or isn't readable
- The file path is invalid
- The model doesn't support vision

Always handle errors when working with images:

```rust
match broker.generate(&[message], None, None, None).await {
    Ok(response) => println!("Response: {}", response),
    Err(e) => eprintln!("Error analyzing image: {}", e),
}
```

## Supported Image Formats

The framework reads raw image bytes and passes them to the model. Supported formats depend on the specific model being used. Most vision models support common formats like JPEG and PNG.

## See Also

- [Examples](../examples/README.md) - See `image_analysis.rs` for a working example
- [LlmMessage API](messages.md) - Full message construction API
- [Ollama Gateway](../gateways/ollama.md) - Gateway-specific details
