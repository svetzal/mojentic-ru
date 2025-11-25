use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, Write};

/// Tool for prompting the user for input or assistance
///
/// This tool allows the LLM to ask the user questions or request help when it
/// doesn't have enough information to proceed. The user's response is returned
/// as the tool's result.
///
/// # Examples
///
/// ```
/// use mojentic::llm::tools::ask_user_tool::AskUserTool;
/// use mojentic::llm::tools::LlmTool;
/// use std::collections::HashMap;
/// use serde_json::json;
///
/// let tool = AskUserTool::new();
/// let mut args = HashMap::new();
/// args.insert("user_request".to_string(), json!("What is your favorite color?"));
///
/// // This would prompt the user for input
/// // let result = tool.run(&args).unwrap();
/// ```
#[derive(Clone)]
pub struct AskUserTool;

impl AskUserTool {
    /// Creates a new AskUserTool instance
    pub fn new() -> Self {
        Self
    }

    /// Prompt the user with a message and read their response
    fn prompt_user(&self, request: &str) -> Result<String> {
        println!("\n\n\nI NEED YOUR HELP!\n{}", request);
        print!("Your response: ");
        io::stdout().flush().map_err(crate::error::MojenticError::IoError)?;

        let mut response = String::new();
        io::stdin()
            .read_line(&mut response)
            .map_err(crate::error::MojenticError::IoError)?;

        Ok(response.trim().to_string())
    }
}

impl Default for AskUserTool {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmTool for AskUserTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let user_request = args.get("user_request").and_then(|v| v.as_str()).ok_or_else(|| {
            crate::error::MojenticError::ToolError(
                "Missing required argument: user_request".to_string(),
            )
        })?;

        let response = self.prompt_user(user_request)?;
        Ok(json!(response))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "ask_user".to_string(),
                description: "If you do not know how to proceed, ask the user a question, or ask them for help or to do something for you.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "user_request": {
                            "type": "string",
                            "description": "The question you need the user to answer, or the task you need the user to do for you."
                        }
                    },
                    "required": ["user_request"]
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor() {
        let tool = AskUserTool::new();
        let descriptor = tool.descriptor();

        assert_eq!(descriptor.r#type, "function");
        assert_eq!(descriptor.function.name, "ask_user");
        assert!(descriptor.function.description.contains("ask the user a question"));

        // Verify parameters structure
        let params = &descriptor.function.parameters;
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["user_request"].is_object());
        assert_eq!(params["properties"]["user_request"]["type"], "string");
        assert!(params["required"].as_array().unwrap().contains(&json!("user_request")));
    }

    #[test]
    fn test_tool_matches() {
        let tool = AskUserTool::new();
        assert!(tool.matches("ask_user"));
        assert!(!tool.matches("other_tool"));
    }

    #[test]
    fn test_missing_argument() {
        let tool = AskUserTool::new();
        let args = HashMap::new();

        let result = tool.run(&args);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::error::MojenticError::ToolError(_)));
    }

    #[test]
    fn test_default_implementation() {
        let tool = AskUserTool;
        let descriptor = tool.descriptor();

        assert_eq!(descriptor.function.name, "ask_user");
    }
}
