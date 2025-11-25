//! Tool execution agent for the ReAct pattern.
//!
//! This agent handles the actual execution of tools and captures the results.

use crate::agents::BaseAsyncAgent;
use crate::event::Event;
use crate::Result;
use async_trait::async_trait;

use super::events::{FailureOccurred, InvokeDecisioning, InvokeToolCall};
use super::models::ThoughtActionObservation;

/// Agent responsible for executing tool calls.
///
/// This agent receives tool call events, executes the specified tool,
/// and updates the context with the results before continuing to the
/// decisioning phase.
pub struct ToolCallAgent;

impl ToolCallAgent {
    /// Create a new tool call agent.
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolCallAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseAsyncAgent for ToolCallAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Downcast to InvokeToolCall
        let tool_call_event = match event.as_any().downcast_ref::<InvokeToolCall>() {
            Some(e) => e,
            None => return Ok(vec![]),
        };

        let tool = &tool_call_event.tool;
        let tool_name = tool.descriptor().function.name.clone();
        let arguments = &tool_call_event.tool_arguments;

        println!("\nExecuting tool: {}", tool_name);
        println!("Arguments: {:?}", arguments);

        // Execute the tool
        let result = match tool.run(arguments) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Tool execution error: {}", e);
                return Ok(vec![Box::new(FailureOccurred {
                    source: "ToolCallAgent".to_string(),
                    correlation_id: tool_call_event.correlation_id.clone(),
                    context: tool_call_event.context.clone(),
                    reason: format!("Tool execution failed: {}", e),
                }) as Box<dyn Event>]);
            }
        };

        println!("Result: {:?}", result);

        // Extract text content from result
        let result_text = if result.is_object() {
            // If it's an object, try to get a "summary" field, otherwise use the whole JSON
            result
                .get("summary")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| result.to_string())
        } else {
            result.to_string()
        };

        // Update context with observation
        let mut updated_context = tool_call_event.context.clone();
        updated_context.history.push(ThoughtActionObservation {
            thought: tool_call_event.thought.clone(),
            action: format!("Called {} with {:?}", tool_name, tool_call_event.tool_arguments),
            observation: result_text,
        });

        // Continue to decisioning
        Ok(vec![Box::new(InvokeDecisioning {
            source: "ToolCallAgent".to_string(),
            correlation_id: tool_call_event.correlation_id.clone(),
            context: updated_context,
        }) as Box<dyn Event>])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::tools::simple_date_tool::SimpleDateTool;
    use crate::llm::tools::LlmTool;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::super::events::InvokeToolCall;
    use super::super::models::{CurrentContext, NextAction};

    #[tokio::test]
    async fn test_tool_call_agent_successful_execution() {
        let agent = ToolCallAgent::new();
        let tool: Arc<dyn LlmTool> = Arc::new(SimpleDateTool);

        let mut args = HashMap::new();
        args.insert("relative_date".to_string(), json!("tomorrow"));

        let context = CurrentContext::new("What is the date tomorrow?");

        let event = Box::new(InvokeToolCall {
            source: "TestSource".to_string(),
            correlation_id: Some("test-123".to_string()),
            context,
            thought: "I need to resolve the date".to_string(),
            action: NextAction::Act,
            tool,
            tool_arguments: args,
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(event).await.unwrap();
        assert_eq!(result.len(), 1);

        // Should return InvokeDecisioning event
        let decisioning = result[0].as_any().downcast_ref::<InvokeDecisioning>();
        assert!(decisioning.is_some());

        let decisioning = decisioning.unwrap();
        assert_eq!(decisioning.context.history.len(), 1);
        assert!(decisioning.context.history[0].observation.contains("tomorrow"));
    }

    #[tokio::test]
    async fn test_tool_call_agent_ignores_wrong_event_type() {
        let agent = ToolCallAgent::new();

        let wrong_event = Box::new(InvokeDecisioning {
            source: "Wrong".to_string(),
            correlation_id: None,
            context: CurrentContext::new("Test"),
        }) as Box<dyn Event>;

        let result = agent.receive_event_async(wrong_event).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_tool_call_agent_default() {
        let _agent1 = ToolCallAgent::new();
        let _agent2 = ToolCallAgent;
    }
}
