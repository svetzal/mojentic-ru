//! Data models for the ReAct pattern.
//!
//! This module defines the core data structures used throughout the ReAct
//! implementation, including actions, plans, observations, and context.

use serde::{Deserialize, Serialize};

/// Enumeration of possible next actions in the ReAct loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum NextAction {
    /// Create or refine a plan
    Plan,
    /// Execute a tool action
    Act,
    /// Complete and summarize the results
    Finish,
}

/// A single step in the ReAct loop capturing thought, action, and observation.
///
/// This model represents one iteration of the ReAct pattern where the agent:
/// 1. Thinks about what to do
/// 2. Takes an action
/// 3. Observes the result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtActionObservation {
    /// The thought process behind the action taken in the current context.
    pub thought: String,
    /// The action taken in the current context.
    pub action: String,
    /// The observation made after the action taken in the current context.
    pub observation: String,
}

/// A structured plan for solving a user query.
///
/// Contains a list of steps that outline how to approach answering the query.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct Plan {
    /// How to answer the query, step by step, each step outlining an action to take.
    #[serde(default)]
    pub steps: Vec<String>,
}

/// The complete context for a ReAct session.
///
/// This model tracks everything needed to maintain state throughout the
/// reasoning and acting loop, including the user's query, the plan,
/// the history of actions, and the iteration count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentContext {
    /// The user query to which we are responding.
    pub user_query: String,
    /// The current plan of action for the current context.
    #[serde(default)]
    pub plan: Plan,
    /// The history of actions taken and observations made in the current context.
    #[serde(default)]
    pub history: Vec<ThoughtActionObservation>,
    /// The number of iterations taken in the current context.
    #[serde(default)]
    pub iteration: usize,
}

impl CurrentContext {
    /// Create a new context with the given user query.
    pub fn new(user_query: impl Into<String>) -> Self {
        Self {
            user_query: user_query.into(),
            plan: Plan::default(),
            history: Vec::new(),
            iteration: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_action_serialization() {
        assert_eq!(serde_json::to_string(&NextAction::Plan).unwrap(), "\"PLAN\"");
        assert_eq!(serde_json::to_string(&NextAction::Act).unwrap(), "\"ACT\"");
        assert_eq!(serde_json::to_string(&NextAction::Finish).unwrap(), "\"FINISH\"");
    }

    #[test]
    fn test_thought_action_observation() {
        let tao = ThoughtActionObservation {
            thought: "I need to get the date".to_string(),
            action: "Called resolve_date".to_string(),
            observation: "2025-11-29".to_string(),
        };

        assert_eq!(tao.thought, "I need to get the date");
        assert_eq!(tao.action, "Called resolve_date");
        assert_eq!(tao.observation, "2025-11-29");
    }

    #[test]
    fn test_plan_default() {
        let plan = Plan::default();
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn test_plan_serialization() {
        let plan = Plan {
            steps: vec!["Step 1".to_string(), "Step 2".to_string()],
        };

        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("Step 1"));
        assert!(json.contains("Step 2"));

        let deserialized: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.steps.len(), 2);
    }

    #[test]
    fn test_current_context_new() {
        let ctx = CurrentContext::new("What is the date?");
        assert_eq!(ctx.user_query, "What is the date?");
        assert_eq!(ctx.iteration, 0);
        assert!(ctx.history.is_empty());
        assert!(ctx.plan.steps.is_empty());
    }

    #[test]
    fn test_current_context_with_history() {
        let mut ctx = CurrentContext::new("Test query");
        ctx.history.push(ThoughtActionObservation {
            thought: "Thinking".to_string(),
            action: "Acting".to_string(),
            observation: "Observing".to_string(),
        });
        ctx.iteration = 1;

        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.iteration, 1);
    }
}
