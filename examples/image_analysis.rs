/// Image Analysis Example
///
/// This example demonstrates multimodal capabilities - analyzing images
/// with vision-capable LLM models.
///
/// Usage:
///   cargo run --example image_analysis
///
/// Requirements:
///   - Ollama running locally (default: http://localhost:11434)
///   - A vision-capable model pulled (e.g., ollama pull qwen3-vl:30b)
///   - Test image at examples/images/flash_rom.jpg
use mojentic::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get the absolute path to the image
    let mut image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    image_path.push("examples/images/flash_rom.jpg");

    // Check if image exists
    if !image_path.exists() {
        eprintln!("Error: Image not found at {:?}", image_path);
        eprintln!("\nMake sure the test image exists:");
        eprintln!("  examples/images/flash_rom.jpg");
        std::process::exit(1);
    }

    println!("Analyzing image with vision model...");
    println!("Image: {:?}", image_path);
    println!();

    // Create gateway and broker with a vision-capable model
    // Options: llava:latest, bakllava:latest, gemma3:27b, qwen3-vl:30b, etc.
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3-vl:30b", Arc::new(gateway), None);

    // Create a message with image
    let message = LlmMessage::user(
        "This is a Flash ROM chip on an adapter board. Extract the text on top of the chip.",
    )
    .with_images(vec![image_path.to_string_lossy().to_string()]);

    // Generate response
    match broker.generate(&[message], None, None, None).await {
        Ok(response) => {
            println!("Response:");
            println!("{}", response);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!();
            eprintln!("Make sure you have a vision-capable model installed:");
            eprintln!("  ollama pull qwen3-vl:30b");
            eprintln!();
            eprintln!("Other vision models to try:");
            eprintln!("  ollama pull llava:latest");
            eprintln!("  ollama pull bakllava:latest");
            eprintln!("  ollama pull gemma3:27b");
            std::process::exit(1);
        }
    }

    Ok(())
}
