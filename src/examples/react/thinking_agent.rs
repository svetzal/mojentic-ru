//! Planning agent for the ReAct pattern.
//!
//! This agent creates structured plans for solving user queries.

use crate::agents::BaseAsyncAgent;
use crate::event::Event;
use crate::llm::tools::simple_date_tool::SimpleDateTool;
use crate::llm::tools::LlmTool;
use crate::llm::{LlmBroker, LlmMessage, MessageRole};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

use super::events::{FailureOccurred, InvokeDecisioning, InvokeThinking};
use super::formatters::{format_available_tools, format_current_context};
use super::models::{Plan, ThoughtActionObservation};

/// Agent responsible for creating plans in the ReAct loop.
///
/// This agent analyzes the user query and available tools to create
/// a step-by-step plan for answering the query.
pub struct ThinkingAgent {
    llm: Arc<LlmBroker>,
    tools: Vec<Box<dyn LlmTool>>,
}

impl ThinkingAgent {
    /// Initialize the thinking agent.
    ///
    /// # Arguments
    ///
    /// * `llm` - The LLM broker to use for generating plans.
    pub fn new(llm: Arc<LlmBroker>) -> Self {
        Self {
            llm,
            tools: vec![Box::new(SimpleDateTool)],
        }
    }

    /// Generate the prompt for the planning LLM.
    fn prompt(&self, event: &InvokeThinking) -> String {
        let tools_list: Vec<&dyn LlmTool> = self.tools.iter().map(|t| t.as_ref()).collect();

        format!(
            "You are to solve a problem by reasoning and acting on the information you have. Here is the current context:

{}
{}

Your Instructions:
Given our context and what we've done so far, and the tools available, create a step-by-step plan to answer the query.
Each step should be concrete and actionable. Consider which tools you'll need to use.",
            format_current_context(&event.context),
            format_available_tools(&tools_list)
        )
    }
}

#[async_trait]
impl BaseAsyncAgent for ThinkingAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Downcast to InvokeThinking
        let thinking_event = match event.as_any().downcast_ref::<InvokeThinking>() {
            Some(e) => e,
            None => return Ok(vec![]),
        };

        let prompt = self.prompt(thinking_event);
        println!("\n{}\n{}\n{}\n", "=".repeat(80), prompt, "=".repeat(80));

        // Generate plan using structured output
        let plan = match self
            .llm
            .generate_object::<Plan>(
                &[LlmMessage {
                    role: MessageRole::User,
                    content: Some(prompt),
                    tool_calls: None,
                    image_paths: None,
                }],
                None,
                thinking_event.correlation_id.clone(),
            )
            .await
        {
            Ok(p) => p,
            Err(e) => {
                return Ok(vec![Box::new(FailureOccurred {
                    source: "ThinkingAgent".to_string(),
                    correlation_id: thinking_event.correlation_id.clone(),
                    context: thinking_event.context.clone(),
                    reason: format!("Error during planning: {}", e),
                }) as Box<dyn Event>]);
            }
        };

        println!("\n{}\nPlan: {:?}\n{}\n", "=".repeat(80), plan, "=".repeat(80));

        // Update context with new plan
        let mut updated_context = thinking_event.context.clone();
        updated_context.plan = plan.clone();

        // Add planning step to history
        updated_context.history.push(ThoughtActionObservation {
            thought: "I need to create a plan to solve this query.".to_string(),
            action: "Created a step-by-step plan.".to_string(),
            observation: format!("Plan has {} steps.", plan.steps.len()),
        });

        Ok(vec![Box::new(InvokeDecisioning {
            source: "ThinkingAgent".to_string(),
            correlation_id: thinking_event.correlation_id.clone(),
            context: updated_context,
        }) as Box<dyn Event>])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::gateways::OllamaGateway;

    use super::super::models::CurrentContext;

    #[test]
    fn test_thinking_agent_prompt_generation() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = ThinkingAgent::new(llm);

        let context = CurrentContext::new("What is the date tomorrow?");
        let event = InvokeThinking {
            source: "TestSource".to_string(),
            correlation_id: Some("test-123".to_string()),
            context,
        };

        let prompt = agent.prompt(&event);

        assert!(prompt.contains("What is the date tomorrow?"));
        assert!(prompt.contains("create a step-by-step plan"));
        assert!(prompt.contains("resolve_date"));
    }

    #[test]
    fn test_thinking_agent_with_existing_plan() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = ThinkingAgent::new(llm);

        let mut context = CurrentContext::new("What day is it?");
        context.plan = Plan {
            steps: vec!["Get current date".to_string()],
        };

        let event = InvokeThinking {
            source: "TestSource".to_string(),
            correlation_id: Some("test-456".to_string()),
            context,
        };

        let prompt = agent.prompt(&event);

        assert!(prompt.contains("Current plan:"));
        assert!(prompt.contains("Get current date"));
    }

    #[tokio::test]
    async fn test_thinking_agent_ignores_wrong_event_type() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = ThinkingAgent::new(llm);

        let wrong_event = Box::new(InvokeDecisioning {
            source: "Wrong".to_string(),
            correlation_id: None,
            context: CurrentContext::new("Test"),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(wrong_event).await.unwrap();
        assert!(result.is_empty());
    }
}
