use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Tool for getting the current date and time
///
/// This tool returns the current datetime with optional formatting.
/// It's useful when the LLM needs to know the current time or date.
///
/// # Examples
///
/// ```ignore
/// use mojentic::llm::tools::current_datetime_tool::CurrentDatetimeTool;
///
/// let tool = CurrentDatetimeTool;
/// let args = HashMap::new();
///
/// let result = tool.run(&args, &crate::llm::tools::ToolRunCtx::default()).await?;
/// // result contains current_datetime, timestamp, and timezone
/// ```
#[derive(Clone)]
pub struct CurrentDatetimeTool;

impl CurrentDatetimeTool {
    /// Creates a new CurrentDatetimeTool instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for CurrentDatetimeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmTool for CurrentDatetimeTool {
    async fn run(
        &self,
        args: &HashMap<String, Value>,
        _ctx: &crate::llm::tools::ToolRunCtx,
    ) -> Result<Value> {
        let format_string = args
            .get("format_string")
            .and_then(|v| v.as_str())
            .unwrap_or("%Y-%m-%d %H:%M:%S");

        let now = super::datetime::now();
        let formatted_time = super::datetime::format(&now, format_string)?;
        let timestamp = now.timestamp().as_second();
        let timezone = super::datetime::format(&now, "%Z")?;

        Ok(json!({
            "current_datetime": formatted_time,
            "timestamp": timestamp,
            "timezone": timezone
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "get_current_datetime".to_string(),
                description: "Get the current date and time. Useful when you need to know the current time or date.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "format_string": {
                            "type": "string",
                            "description": "Format string for the datetime (e.g., '%Y-%m-%d %H:%M:%S', '%A, %B %d, %Y'). Default is ISO format."
                        }
                    },
                    "required": []
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
        let tool = CurrentDatetimeTool::new();
        let descriptor = tool.descriptor();

        assert_eq!(descriptor.r#type, "function");
        assert_eq!(descriptor.function.name, "get_current_datetime");
        assert!(descriptor.function.description.contains("current date and time"));
    }

    #[test]
    fn test_tool_matches() {
        let tool = CurrentDatetimeTool::new();
        assert!(tool.matches("get_current_datetime"));
        assert!(!tool.matches("other_tool"));
    }
}
