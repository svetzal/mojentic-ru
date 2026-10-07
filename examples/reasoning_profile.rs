//! Validate explicit model controls without loading a model.
use mojentic::llm::gateway::ReasoningEffort;
use mojentic::llm::{CompletionConfig, LlmMessage, ReasoningProfile};

fn main() -> mojentic::error::Result<()> {
    let config = CompletionConfig {
        reasoning_profile: Some(ReasoningProfile::qwen3_8()),
        reasoning_effort: Some(ReasoningEffort::Medium),
        preserve_reasoning: true,
        ..Default::default()
    };
    config.validate_reasoning()?;
    let history = LlmMessage::assistant("Inspecting the source.")
        .with_reasoning("Read the relevant function before editing it.");
    println!("{}", serde_json::to_string_pretty(&history)?);
    Ok(())
}
