use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Tool for displaying messages to the user without expecting a response
///
/// This tool allows the LLM to send important intermediate information to the user
/// as it works on completing their request. It's useful for providing status updates,
/// progress information, or other important messages during long-running operations.
///
/// # Examples
///
/// ```
/// use mojentic::llm::tools::tell_user_tool::TellUserTool;
/// use mojentic::llm::tools::LlmTool;
/// use std::collections::HashMap;
/// use serde_json::json;
///
/// let tool = TellUserTool;
/// let mut args = HashMap::new();
/// args.insert("message".to_string(), json!("Processing your request..."));
///
/// let result = tool.run(&args).unwrap();
/// // Prints to stdout:
/// //
/// //
/// //
/// // MESSAGE FROM ASSISTANT:
/// // Processing your request...
/// //
/// // Returns: "Message delivered to user."
/// ```
pub struct TellUserTool;

impl TellUserTool {
    /// Creates a new TellUserTool instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for TellUserTool {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmTool for TellUserTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");

        println!("\n\n\nMESSAGE FROM ASSISTANT:\n{}", message);

        Ok(json!("Message delivered to user."))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "tell_user".to_string(),
                description: "Display a message to the user without expecting a response. Use this to send important intermediate information to the user as you work on completing their request.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "The important message you want to display to the user."
                        }
                    },
                    "required": ["message"]
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor() {
        let tool = TellUserTool::new();
        let descriptor = tool.descriptor();

        assert_eq!(descriptor.r#type, "function");
        assert_eq!(descriptor.function.name, "tell_user");
        assert!(descriptor.function.description.contains("Display a message to the user"));

        // Verify parameters structure
        let params = &descriptor.function.parameters;
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["message"].is_object());
        assert_eq!(params["properties"]["message"]["type"], "string");
        assert!(params["required"].as_array().unwrap().contains(&json!("message")));
    }

    #[test]
    fn test_run_with_message() {
        let tool = TellUserTool::new();
        let mut args = HashMap::new();
        args.insert("message".to_string(), json!("This is an important update"));

        let result = tool.run(&args).unwrap();

        assert_eq!(result, json!("Message delivered to user."));
    }

    #[test]
    fn test_run_without_message() {
        let tool = TellUserTool::new();
        let args = HashMap::new();

        let result = tool.run(&args).unwrap();

        assert_eq!(result, json!("Message delivered to user."));
    }

    #[test]
    fn test_tool_matches() {
        let tool = TellUserTool::new();
        assert!(tool.matches("tell_user"));
        assert!(!tool.matches("other_tool"));
    }

    #[test]
    fn test_multiline_message() {
        let tool = TellUserTool::new();
        let mut args = HashMap::new();
        args.insert("message".to_string(), json!("Line 1\nLine 2\nLine 3"));

        let result = tool.run(&args).unwrap();

        assert_eq!(result, json!("Message delivered to user."));
    }

    #[test]
    fn test_default_implementation() {
        let tool = TellUserTool;
        let descriptor = tool.descriptor();

        assert_eq!(descriptor.function.name, "tell_user");
    }
}
