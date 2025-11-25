//! Formatting utilities for the ReAct pattern implementation.
//!
//! This module provides helper functions for formatting context and tool information
//! into human-readable strings for LLM prompts.

use crate::llm::tools::LlmTool;

use super::models::CurrentContext;

/// Format the current context into a readable string.
///
/// # Arguments
///
/// * `context` - The current context containing query, plan, and history.
///
/// # Returns
///
/// A formatted multi-line string describing the current context.
pub fn format_current_context(context: &CurrentContext) -> String {
    let user_query = format!(
        "The user has asked us to answer the following query:\n> {}\n",
        context.user_query
    );

    let plan = if context.plan.steps.is_empty() {
        "You have not yet made a plan.\n".to_string()
    } else {
        let mut output = "Current plan:\n".to_string();
        for step in &context.plan.steps {
            output.push_str(&format!("- {}\n", step));
        }
        output
    };

    let history = if context.history.is_empty() {
        "No steps have yet been taken.\n".to_string()
    } else {
        let mut output = "What's been done so far:\n".to_string();
        for (i, step) in context.history.iter().enumerate() {
            output.push_str(&format!(
                "{}.\n    Thought: {}\n    Action: {}\n    Observation: {}\n",
                i + 1,
                step.thought,
                step.action,
                step.observation
            ));
        }
        output
    };

    format!("Current Context:\n{}{}{}\n", user_query, plan, history)
}

/// Format the available tools into a readable list.
///
/// # Arguments
///
/// * `tools` - A slice of tool references with descriptor dictionaries.
///
/// # Returns
///
/// A formatted string listing available tools and their descriptions.
pub fn format_available_tools(tools: &[&dyn LlmTool]) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let mut output = "Tools available:\n".to_string();

    for tool in tools {
        let descriptor = tool.descriptor();
        output.push_str(&format!(
            "- {}: {}\n",
            descriptor.function.name, descriptor.function.description
        ));

        // Add parameter information
        if let Some(params) = descriptor.function.parameters.as_object() {
            if let Some(properties) = params.get("properties").and_then(|p| p.as_object()) {
                output.push_str("  Parameters:\n");

                let required: Vec<String> = params
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                    })
                    .unwrap_or_default();

                for (param_name, param_info) in properties {
                    let param_desc =
                        param_info.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    let is_required = required.contains(param_name);
                    let req_str = if is_required {
                        " (required)"
                    } else {
                        " (optional)"
                    };
                    output.push_str(&format!("    - {}{}: {}\n", param_name, req_str, param_desc));
                }
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::tools::simple_date_tool::SimpleDateTool;
    use crate::llm::tools::LlmTool;

    use super::super::models::{Plan, ThoughtActionObservation};

    #[test]
    fn test_format_current_context_empty() {
        let context = CurrentContext::new("What is the weather?");
        let formatted = format_current_context(&context);

        assert!(formatted.contains("What is the weather?"));
        assert!(formatted.contains("You have not yet made a plan"));
        assert!(formatted.contains("No steps have yet been taken"));
    }

    #[test]
    fn test_format_current_context_with_plan() {
        let mut context = CurrentContext::new("Calculate total");
        context.plan = Plan {
            steps: vec![
                "Step 1: Get data".to_string(),
                "Step 2: Sum values".to_string(),
            ],
        };

        let formatted = format_current_context(&context);

        assert!(formatted.contains("Current plan:"));
        assert!(formatted.contains("Step 1: Get data"));
        assert!(formatted.contains("Step 2: Sum values"));
    }

    #[test]
    fn test_format_current_context_with_history() {
        let mut context = CurrentContext::new("Get date");
        context.history.push(ThoughtActionObservation {
            thought: "Need to resolve date".to_string(),
            action: "Called resolve_date".to_string(),
            observation: "2025-11-29".to_string(),
        });

        let formatted = format_current_context(&context);

        assert!(formatted.contains("What's been done so far:"));
        assert!(formatted.contains("Need to resolve date"));
        assert!(formatted.contains("Called resolve_date"));
        assert!(formatted.contains("2025-11-29"));
    }

    #[test]
    fn test_format_available_tools_empty() {
        let tools: Vec<&dyn LlmTool> = vec![];
        let formatted = format_available_tools(&tools);

        assert_eq!(formatted, "");
    }

    #[test]
    fn test_format_available_tools_with_tools() {
        let date_tool = SimpleDateTool;
        let tools: Vec<&dyn LlmTool> = vec![&date_tool];

        let formatted = format_available_tools(&tools);

        assert!(formatted.contains("Tools available:"));
        assert!(formatted.contains("resolve_date"));
        assert!(formatted.contains("relative date"));
        assert!(formatted.contains("Parameters:"));
        assert!(formatted.contains("relative_date"));
        assert!(formatted.contains("(required)"));
    }

    #[test]
    fn test_format_available_tools_multiple() {
        let date_tool = SimpleDateTool;
        let tools: Vec<&dyn LlmTool> = vec![&date_tool];

        let formatted = format_available_tools(&tools);

        // Should list each tool with its description and parameters
        let tool_count = formatted.matches("resolve_date").count();
        assert_eq!(tool_count, 1);
    }
}
