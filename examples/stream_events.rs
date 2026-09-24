//! Stream Events Example - one streamed turn with terminal completion evidence
//!
//! `generate_stream_events` yields content as it arrives and ends with exactly
//! one terminal event. `Completed` carries the provider's finish reason, usage
//! and model. An error means the output is incomplete: the content already
//! printed is evidence of what arrived, not a finished answer.
//!
//! The small `num_predict` limit makes a `length` finish likely, so you can
//! see the incomplete-completion error. Raise it to see a clean completion.
//!
//! Run with: cargo run --example stream_events

use futures::stream::StreamExt;
use mojentic::llm::gateway::CompletionConfig;
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::{LlmBroker, LlmMessage, StreamEvent, StreamEventError};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let broker = LlmBroker::new("qwen3:32b", Arc::new(OllamaGateway::new()), None);
    let messages = vec![LlmMessage::user(
        "Describe a river delta in three sentences.",
    )];
    let config = CompletionConfig {
        num_predict: Some(40),
        ..Default::default()
    };

    let mut events = broker.generate_stream_events(&messages, Some(config), None);
    while let Some(event) = events.next().await {
        match event {
            StreamEvent::Content(text) => print!("{text}"),
            StreamEvent::Completed(evidence) => {
                println!("\n\nCompleted by {:?}", evidence.provider_model);
                println!("Usage: {:?}", evidence.usage);
            }
            StreamEvent::Error(StreamEventError::IncompleteCompletion(evidence)) => {
                println!("\n\nIncomplete: finish reason {:?}", evidence.finish_reason);
                println!("Usage: {:?}", evidence.usage);
            }
            StreamEvent::Error(error) => eprintln!("\n\nStream failed: {error}"),
            _ => {}
        }
    }
}
