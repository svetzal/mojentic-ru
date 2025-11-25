//! Event definitions for the ReAct pattern.
//!
//! This module defines all event types used to coordinate the ReAct loop,
//! including thinking, decisioning, tool calls, completion, and failure events.

use crate::event::Event;
use crate::llm::tools::LlmTool;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

use super::models::{CurrentContext, NextAction};

/// Event to trigger the thinking/planning phase.
///
/// This event initiates the planning process where the agent creates
/// or refines a plan for answering the user's query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeThinking {
    pub source: String,
    pub correlation_id: Option<String>,
    /// The current context as we work through our response.
    pub context: CurrentContext,
}

impl Event for InvokeThinking {
    fn source(&self) -> &str {
        &self.source
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

/// Event to trigger the decision-making phase.
///
/// This event initiates the decision process where the agent evaluates
/// the current plan and history to decide on the next action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeDecisioning {
    pub source: String,
    pub correlation_id: Option<String>,
    /// The current context as we work through our response.
    pub context: CurrentContext,
}

impl Event for InvokeDecisioning {
    fn source(&self) -> &str {
        &self.source
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

/// Event to trigger a tool invocation.
///
/// This event carries the information needed to execute a specific tool
/// with given arguments, along with the reasoning behind the decision.
/// Note: Cannot be cloned/serialized due to Arc<dyn LlmTool>.
pub struct InvokeToolCall {
    pub source: String,
    pub correlation_id: Option<String>,
    /// The current context as we work through our response.
    pub context: CurrentContext,
    /// The reasoning behind the decision.
    pub thought: String,
    /// The next action type.
    pub action: NextAction,
    /// The tool instance to invoke.
    pub tool: Arc<dyn LlmTool>,
    /// Arguments to pass to the tool.
    pub tool_arguments: std::collections::HashMap<String, serde_json::Value>,
}

impl std::fmt::Debug for InvokeToolCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InvokeToolCall")
            .field("source", &self.source)
            .field("correlation_id", &self.correlation_id)
            .field("context", &self.context)
            .field("thought", &self.thought)
            .field("action", &self.action)
            .field("tool", &self.tool.descriptor().function.name)
            .field("tool_arguments", &self.tool_arguments)
            .finish()
    }
}

impl Event for InvokeToolCall {
    fn source(&self) -> &str {
        &self.source
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        // We can't truly clone the tool, but we can clone the Arc
        Box::new(InvokeToolCall {
            source: self.source.clone(),
            correlation_id: self.correlation_id.clone(),
            context: self.context.clone(),
            thought: self.thought.clone(),
            action: self.action,
            tool: self.tool.clone(),
            tool_arguments: self.tool_arguments.clone(),
        })
    }
}

/// Event to trigger the completion and summarization phase.
///
/// This event indicates that the agent has gathered sufficient information
/// to answer the user's query and should generate a final response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishAndSummarize {
    pub source: String,
    pub correlation_id: Option<String>,
    /// The current context as we work through our response.
    pub context: CurrentContext,
    /// The reasoning behind the decision.
    pub thought: String,
}

impl Event for FinishAndSummarize {
    fn source(&self) -> &str {
        &self.source
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

/// Event to signal a failure in the ReAct loop.
///
/// This event captures errors or unrecoverable situations that prevent
/// the agent from continuing to process the user's query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureOccurred {
    pub source: String,
    pub correlation_id: Option<String>,
    /// The current context as we work through our response.
    pub context: CurrentContext,
    /// The reason for the failure.
    pub reason: String,
}

impl Event for FailureOccurred {
    fn source(&self) -> &str {
        &self.source
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invoke_thinking_event() {
        let mut event = InvokeThinking {
            source: "TestAgent".to_string(),
            correlation_id: None,
            context: CurrentContext::new("Test query"),
        };

        assert_eq!(event.source(), "TestAgent");
        assert_eq!(event.correlation_id(), None);

        event.set_correlation_id("test-123".to_string());
        assert_eq!(event.correlation_id(), Some("test-123"));
    }

    #[test]
    fn test_invoke_decisioning_event() {
        let event = InvokeDecisioning {
            source: "DecisionAgent".to_string(),
            correlation_id: Some("corr-456".to_string()),
            context: CurrentContext::new("Test query"),
        };

        assert_eq!(event.source(), "DecisionAgent");
        assert_eq!(event.correlation_id(), Some("corr-456"));
    }

    #[test]
    fn test_finish_and_summarize_event() {
        let event = FinishAndSummarize {
            source: "SummaryAgent".to_string(),
            correlation_id: Some("finish-789".to_string()),
            context: CurrentContext::new("Test query"),
            thought: "I have enough information".to_string(),
        };

        assert_eq!(event.source(), "SummaryAgent");
        assert_eq!(event.thought, "I have enough information");
    }

    #[test]
    fn test_failure_occurred_event() {
        let event = FailureOccurred {
            source: "ToolAgent".to_string(),
            correlation_id: Some("fail-101".to_string()),
            context: CurrentContext::new("Test query"),
            reason: "Tool execution failed".to_string(),
        };

        assert_eq!(event.source(), "ToolAgent");
        assert_eq!(event.reason, "Tool execution failed");
    }

    #[test]
    fn test_event_clone_box() {
        let event = InvokeThinking {
            source: "CloneTest".to_string(),
            correlation_id: Some("clone-123".to_string()),
            context: CurrentContext::new("Clone test query"),
        };

        let cloned = event.clone_box();
        assert_eq!(cloned.source(), "CloneTest");
        assert_eq!(cloned.correlation_id(), Some("clone-123"));
    }
}
