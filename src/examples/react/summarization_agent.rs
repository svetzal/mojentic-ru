//! Summarization agent for the ReAct pattern.
//!
//! This agent generates the final answer based on accumulated context.

use crate::agents::BaseAsyncAgent;
use crate::event::Event;
use crate::llm::{LlmBroker, LlmMessage, MessageRole};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

use super::events::{FailureOccurred, FinishAndSummarize};
use super::formatters::format_current_context;

/// Agent responsible for generating the final answer.
///
/// This agent reviews the context, plan, and history to synthesize
/// a complete answer to the user's original query.
pub struct SummarizationAgent {
    llm: Arc<LlmBroker>,
}

impl SummarizationAgent {
    /// Initialize the summarization agent.
    ///
    /// # Arguments
    ///
    /// * `llm` - The LLM broker to use for generating summaries.
    pub fn new(llm: Arc<LlmBroker>) -> Self {
        Self { llm }
    }

    /// Generate the prompt for the summarization LLM.
    fn prompt(&self, event: &FinishAndSummarize) -> String {
        format!(
            "Based on the following context, provide a clear and concise answer to the user's query.

{}

Your task:
Review what we've learned and provide a direct answer to: \"{}\"

Be specific and use the information gathered during our process.",
            format_current_context(&event.context),
            event.context.user_query
        )
    }
}

#[async_trait]
impl BaseAsyncAgent for SummarizationAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Downcast to FinishAndSummarize
        let finish_event = match event.as_any().downcast_ref::<FinishAndSummarize>() {
            Some(e) => e,
            None => return Ok(vec![]),
        };

        let prompt = self.prompt(finish_event);
        println!("\n{}\n{}\n{}\n", "=".repeat(80), prompt, "=".repeat(80));

        // Generate final response
        let response = match self
            .llm
            .generate(
                &[LlmMessage {
                    role: MessageRole::User,
                    content: Some(prompt),
                    tool_calls: None,
                    image_paths: None,
                }],
                None,
                None,
                finish_event.correlation_id.clone(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(vec![Box::new(FailureOccurred {
                    source: "SummarizationAgent".to_string(),
                    correlation_id: finish_event.correlation_id.clone(),
                    context: finish_event.context.clone(),
                    reason: format!("Error during summarization: {}", e),
                }) as Box<dyn Event>]);
            }
        };

        println!("\n{}", "=".repeat(80));
        println!("FINAL ANSWER:");
        println!("{}", "=".repeat(80));
        println!("{}", response);
        println!("{}\n", "=".repeat(80));

        // This is a terminal event - return empty list to stop the loop
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::gateways::OllamaGateway;

    use super::super::models::{CurrentContext, Plan, ThoughtActionObservation};

    #[test]
    fn test_summarization_agent_prompt_generation() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = SummarizationAgent::new(llm);

        let mut context = CurrentContext::new("What is the date tomorrow?");
        context.history.push(ThoughtActionObservation {
            thought: "Need to resolve date".to_string(),
            action: "Called resolve_date".to_string(),
            observation: "2025-11-30".to_string(),
        });

        let event = FinishAndSummarize {
            source: "TestSource".to_string(),
            correlation_id: Some("test-123".to_string()),
            context,
            thought: "I have the information needed".to_string(),
        };

        let prompt = agent.prompt(&event);

        assert!(prompt.contains("What is the date tomorrow?"));
        assert!(prompt.contains("provide a direct answer"));
        assert!(prompt.contains("2025-11-30"));
    }

    #[test]
    fn test_summarization_agent_with_plan() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = SummarizationAgent::new(llm);

        let mut context = CurrentContext::new("What day is next Friday?");
        context.plan = Plan {
            steps: vec!["Resolve date for next Friday".to_string()],
        };
        context.history.push(ThoughtActionObservation {
            thought: "Calculate next Friday".to_string(),
            action: "Called resolve_date".to_string(),
            observation: "2025-11-29".to_string(),
        });

        let event = FinishAndSummarize {
            source: "TestSource".to_string(),
            correlation_id: Some("test-456".to_string()),
            context,
            thought: "Ready to summarize".to_string(),
        };

        let prompt = agent.prompt(&event);

        assert!(prompt.contains("What day is next Friday?"));
        assert!(prompt.contains("Current plan:"));
        assert!(prompt.contains("2025-11-29"));
    }

    #[tokio::test]
    async fn test_summarization_agent_ignores_wrong_event_type() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = SummarizationAgent::new(llm);

        use super::super::events::InvokeThinking;

        let wrong_event = Box::new(InvokeThinking {
            source: "Wrong".to_string(),
            correlation_id: None,
            context: CurrentContext::new("Test"),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(wrong_event).await.unwrap();
        assert!(result.is_empty());
    }
}
