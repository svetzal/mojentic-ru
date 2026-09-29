//! oMLX Example - one chat turn against a local oMLX server
//!
//! The gateway reads `OMLX_HOST` (default `http://localhost:8000`, without
//! `/v1`), `OMLX_API_KEY` (sent as a bearer token when set) and
//! `OMLX_TIMEOUT` (milliseconds, default ten minutes).
//!
//! Pass a model id as the first argument, or the example uses the first model
//! the server lists.
//!
//! Run with: cargo run --example omlx -- Qwen3.8-27B-MLX-8bit

use mojentic::llm::gateway::CompletionConfig;
use mojentic::llm::gateways::OmlxGateway;
use mojentic::llm::{LlmGateway, LlmMessage};

#[tokio::main]
async fn main() -> mojentic::Result<()> {
    let gateway = OmlxGateway::new();

    let model = match std::env::args().nth(1) {
        Some(model) => model,
        None => {
            let models = gateway.get_available_models().await?;
            let Some(model) = models.into_iter().next() else {
                eprintln!("The oMLX server lists no models.");
                return Ok(());
            };
            model
        }
    };
    println!("Model: {model}\n");

    let messages = vec![LlmMessage::user(
        "Name three rivers in Canada, one per line.",
    )];
    let response = gateway.complete(&model, &messages, None, &CompletionConfig::default()).await?;

    if let Some(thinking) = &response.thinking {
        println!("Thinking:\n{thinking}\n");
    }
    println!("Content:\n{}\n", response.content.as_deref().unwrap_or(""));

    let evidence = &response.evidence;
    println!("Finish reason: {:?}", evidence.finish_reason);
    println!("Provider model: {:?}", evidence.provider_model);
    println!("Usage: {:?}", evidence.usage);
    if evidence.finish_reason.as_deref() != Some("stop") {
        println!("The model did not finish; the content is not a complete answer.");
    }
    Ok(())
}
