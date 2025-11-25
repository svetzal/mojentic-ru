//! Decision-making agent for the ReAct pattern.
//!
//! This agent evaluates the current context and decides on the next action to take.

use crate::agents::BaseAsyncAgent;
use crate::event::Event;
use crate::llm::tools::simple_date_tool::SimpleDateTool;
use crate::llm::tools::LlmTool;
use crate::llm::{LlmBroker, LlmMessage, MessageRole};
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::events::{
    FailureOccurred, FinishAndSummarize, InvokeDecisioning, InvokeThinking, InvokeToolCall,
};
use super::formatters::{format_available_tools, format_current_context};
use super::models::NextAction;

/// Structured response from the decisioning agent.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DecisionResponse {
    /// The reasoning behind the decision
    pub thought: String,
    /// What should happen next: PLAN, ACT, or FINISH
    pub next_action: NextAction,
    /// Name of tool to use if next_action is ACT
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Arguments for the tool if next_action is ACT
    #[serde(default)]
    pub tool_arguments: HashMap<String, serde_json::Value>,
}

/// Agent responsible for deciding the next action in the ReAct loop.
///
/// This agent evaluates the current context, plan, and history to determine
/// whether to continue planning, take an action, or finish and summarize.
pub struct DecisioningAgent {
    llm: Arc<LlmBroker>,
    tools: Vec<Arc<dyn LlmTool>>,
}

impl DecisioningAgent {
    /// Maximum iterations before failing
    const MAX_ITERATIONS: usize = 10;

    /// Initialize the decisioning agent.
    ///
    /// # Arguments
    ///
    /// * `llm` - The LLM broker to use for generating decisions.
    pub fn new(llm: Arc<LlmBroker>) -> Self {
        Self {
            llm,
            tools: vec![Arc::new(SimpleDateTool)],
        }
    }

    /// Generate the prompt for the decision-making LLM.
    fn prompt(&self, event: &InvokeDecisioning) -> String {
        let tools_list: Vec<&dyn LlmTool> = self.tools.iter().map(|t| t.as_ref()).collect();

        format!(
            "You are to solve a problem by reasoning and acting on the information you have. Here is the current context:

{}
{}

Your Instructions:
Review the current plan and history. Decide what to do next:

1. PLAN - If the plan is incomplete or needs refinement
2. ACT - If you should take an action using one of the available tools
3. FINISH - If you have enough information to answer the user's query

If you choose ACT, specify which tool to use and what arguments to pass.
Think carefully about whether each step in the plan has been completed.",
            format_current_context(&event.context),
            format_available_tools(&tools_list)
        )
    }
}

#[async_trait]
impl BaseAsyncAgent for DecisioningAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Downcast to InvokeDecisioning
        let decisioning_event = match event.as_any().downcast_ref::<InvokeDecisioning>() {
            Some(e) => e,
            None => return Ok(vec![]),
        };

        // Check iteration limit
        if decisioning_event.context.iteration >= Self::MAX_ITERATIONS {
            return Ok(vec![Box::new(FailureOccurred {
                source: "DecisioningAgent".to_string(),
                correlation_id: decisioning_event.correlation_id.clone(),
                context: decisioning_event.context.clone(),
                reason: format!("Maximum iterations ({}) exceeded", Self::MAX_ITERATIONS),
            }) as Box<dyn Event>]);
        }

        // Increment iteration counter
        let mut updated_context = decisioning_event.context.clone();
        updated_context.iteration += 1;

        let prompt = self.prompt(decisioning_event);
        println!("\n{}\n{}\n{}\n", "=".repeat(80), prompt, "=".repeat(80));

        // Generate decision using structured output
        let decision = match self
            .llm
            .generate_object::<DecisionResponse>(
                &[LlmMessage {
                    role: MessageRole::User,
                    content: Some(prompt),
                    tool_calls: None,
                    image_paths: None,
                }],
                None,
                decisioning_event.correlation_id.clone(),
            )
            .await
        {
            Ok(d) => d,
            Err(e) => {
                return Ok(vec![Box::new(FailureOccurred {
                    source: "DecisioningAgent".to_string(),
                    correlation_id: decisioning_event.correlation_id.clone(),
                    context: updated_context,
                    reason: format!("Error during decision making: {}", e),
                }) as Box<dyn Event>]);
            }
        };

        println!("\n{}\nDecision: {:?}\n{}\n", "=".repeat(80), decision, "=".repeat(80));

        // Route based on decision
        match decision.next_action {
            NextAction::Finish => Ok(vec![Box::new(FinishAndSummarize {
                source: "DecisioningAgent".to_string(),
                correlation_id: decisioning_event.correlation_id.clone(),
                context: updated_context,
                thought: decision.thought,
            }) as Box<dyn Event>]),

            NextAction::Act => {
                let tool_name = match decision.tool_name {
                    Some(name) => name,
                    None => {
                        return Ok(vec![Box::new(FailureOccurred {
                            source: "DecisioningAgent".to_string(),
                            correlation_id: decisioning_event.correlation_id.clone(),
                            context: updated_context,
                            reason: "ACT decision made but no tool specified".to_string(),
                        }) as Box<dyn Event>]);
                    }
                };

                // Find the requested tool
                let tool =
                    match self.tools.iter().find(|t| t.descriptor().function.name == tool_name) {
                        Some(t) => t.clone(),
                        None => {
                            return Ok(vec![Box::new(FailureOccurred {
                                source: "DecisioningAgent".to_string(),
                                correlation_id: decisioning_event.correlation_id.clone(),
                                context: updated_context,
                                reason: format!("Tool '{}' not found", tool_name),
                            }) as Box<dyn Event>]);
                        }
                    };

                Ok(vec![Box::new(InvokeToolCall {
                    source: "DecisioningAgent".to_string(),
                    correlation_id: decisioning_event.correlation_id.clone(),
                    context: updated_context,
                    thought: decision.thought,
                    action: NextAction::Act,
                    tool,
                    tool_arguments: decision.tool_arguments,
                }) as Box<dyn Event>])
            }

            NextAction::Plan => Ok(vec![Box::new(InvokeThinking {
                source: "DecisioningAgent".to_string(),
                correlation_id: decisioning_event.correlation_id.clone(),
                context: updated_context,
            }) as Box<dyn Event>]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::gateways::OllamaGateway;

    use super::super::models::{CurrentContext, Plan};

    #[test]
    fn test_decisioning_agent_prompt_generation() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = DecisioningAgent::new(llm);

        let context = CurrentContext::new("What is the date tomorrow?");
        let event = InvokeDecisioning {
            source: "TestSource".to_string(),
            correlation_id: Some("test-123".to_string()),
            context,
        };

        let prompt = agent.prompt(&event);

        assert!(prompt.contains("What is the date tomorrow?"));
        assert!(prompt.contains("Decide what to do next"));
        assert!(prompt.contains("PLAN"));
        assert!(prompt.contains("ACT"));
        assert!(prompt.contains("FINISH"));
    }

    #[test]
    fn test_decisioning_agent_with_plan() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = DecisioningAgent::new(llm);

        let mut context = CurrentContext::new("What day is it?");
        context.plan = Plan {
            steps: vec!["Call resolve_date tool".to_string()],
        };

        let event = InvokeDecisioning {
            source: "TestSource".to_string(),
            correlation_id: Some("test-456".to_string()),
            context,
        };

        let prompt = agent.prompt(&event);

        assert!(prompt.contains("Current plan:"));
        assert!(prompt.contains("Call resolve_date tool"));
    }

    #[tokio::test]
    async fn test_decisioning_agent_max_iterations() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = DecisioningAgent::new(llm);

        let mut context = CurrentContext::new("Test query");
        context.iteration = DecisioningAgent::MAX_ITERATIONS;

        let event = Box::new(InvokeDecisioning {
            source: "TestSource".to_string(),
            correlation_id: Some("test-max".to_string()),
            context,
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event).await.unwrap();
        assert_eq!(result.len(), 1);

        let failure = result[0].as_any().downcast_ref::<FailureOccurred>().unwrap();
        assert!(failure.reason.contains("Maximum iterations"));
    }

    #[tokio::test]
    async fn test_decisioning_agent_ignores_wrong_event_type() {
        let gateway = Arc::new(OllamaGateway::new());
        let llm = Arc::new(LlmBroker::new("qwen3:32b", gateway, None));
        let agent = DecisioningAgent::new(llm);

        let wrong_event = Box::new(InvokeThinking {
            source: "Wrong".to_string(),
            correlation_id: None,
            context: CurrentContext::new("Test"),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(wrong_event).await.unwrap();
        assert!(result.is_empty());
    }
}
