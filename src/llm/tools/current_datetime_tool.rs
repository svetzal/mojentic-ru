use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use chrono::Local;
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
/// let result = tool.run(&args)?;
/// // result contains current_datetime, timestamp, and timezone
/// ```
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

impl LlmTool for CurrentDatetimeTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let format_string = args
            .get("format_string")
            .and_then(|v| v.as_str())
            .unwrap_or("%Y-%m-%d %H:%M:%S");

        let now = Local::now();
        let formatted_time = now.format(format_string).to_string();
        let timestamp = now.timestamp();
        let timezone = format!("{}", now.offset());

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
    fn test_run_with_default_format() {
        let tool = CurrentDatetimeTool::new();
        let args = HashMap::new();

        let result = tool.run(&args).unwrap();

        assert!(result.is_object());
        assert!(result.get("current_datetime").is_some());
        assert!(result.get("timestamp").is_some());
        assert!(result.get("timezone").is_some());

        // Check format matches default "%Y-%m-%d %H:%M:%S"
        let datetime_str = result.get("current_datetime").unwrap().as_str().unwrap();
        let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$").unwrap();
        assert!(re.is_match(datetime_str));
    }

    #[test]
    fn test_run_with_custom_format() {
        let tool = CurrentDatetimeTool::new();
        let mut args = HashMap::new();
        args.insert("format_string".to_string(), json!("%Y-%m-%d"));

        let result = tool.run(&args).unwrap();

        let datetime_str = result.get("current_datetime").unwrap().as_str().unwrap();
        let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
        assert!(re.is_match(datetime_str));
    }

    #[test]
    fn test_timestamp_is_reasonable() {
        let tool = CurrentDatetimeTool::new();
        let args = HashMap::new();

        let result = tool.run(&args).unwrap();
        let timestamp = result.get("timestamp").unwrap().as_i64().unwrap();

        // Timestamp should be reasonable (after 2020, before 2030)
        assert!(timestamp > 1_577_836_800);
        assert!(timestamp < 1_893_456_000);
    }

    #[test]
    fn test_timezone_is_present() {
        let tool = CurrentDatetimeTool::new();
        let args = HashMap::new();

        let result = tool.run(&args).unwrap();
        let timezone = result.get("timezone").unwrap().as_str().unwrap();

        assert!(!timezone.is_empty());
    }

    #[test]
    fn test_tool_matches() {
        let tool = CurrentDatetimeTool::new();
        assert!(tool.matches("get_current_datetime"));
        assert!(!tool.matches("other_tool"));
    }

    #[test]
    fn test_format_with_day_name() {
        let tool = CurrentDatetimeTool::new();
        let mut args = HashMap::new();
        args.insert("format_string".to_string(), json!("%A"));

        let result = tool.run(&args).unwrap();
        let day_name = result.get("current_datetime").unwrap().as_str().unwrap();

        let valid_days = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ];
        assert!(valid_days.contains(&day_name));
    }

    #[test]
    fn test_format_with_month_name() {
        let tool = CurrentDatetimeTool::new();
        let mut args = HashMap::new();
        args.insert("format_string".to_string(), json!("%B"));

        let result = tool.run(&args).unwrap();
        let month_name = result.get("current_datetime").unwrap().as_str().unwrap();

        let valid_months = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        assert!(valid_months.contains(&month_name));
    }
}
