use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use chrono::{Local, NaiveDate};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Simple date resolution tool that resolves relative date expressions to absolute dates
///
/// This tool takes text like "three days from now" or "next Tuesday" and resolves it
/// to an absolute date. It uses a simple heuristic approach for common patterns.
///
/// # Examples
///
/// ```ignore
/// use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
///
/// let tool = SimpleDateTool;
/// let args = HashMap::from([
///     ("relative_date".to_string(), json!("tomorrow"))
/// ]);
///
/// let result = tool.run(&args)?;
/// // result contains absolute date for tomorrow
/// ```
pub struct SimpleDateTool;

impl SimpleDateTool {
    /// Parse relative date expressions
    ///
    /// Supports common patterns like:
    /// - "today", "tomorrow", "yesterday"
    /// - "X days from now", "X days ago"
    /// - "next week", "last week"
    fn parse_relative_date(&self, relative_date: &str) -> Result<NaiveDate> {
        let today = Local::now().date_naive();
        let lower = relative_date.to_lowercase();

        // Simple pattern matching for common cases
        if lower.contains("today") {
            return Ok(today);
        }

        if lower.contains("tomorrow") {
            return Ok(today + chrono::Duration::days(1));
        }

        if lower.contains("yesterday") {
            return Ok(today - chrono::Duration::days(1));
        }

        // Parse "X days from now"
        if let Some(days) = self.extract_days_offset(&lower, "from now") {
            return Ok(today + chrono::Duration::days(days));
        }

        // Parse "X days ago"
        if let Some(days) = self.extract_days_offset(&lower, "ago") {
            return Ok(today - chrono::Duration::days(days));
        }

        // Parse "next week"
        if lower.contains("next week") {
            return Ok(today + chrono::Duration::weeks(1));
        }

        // Parse "last week"
        if lower.contains("last week") {
            return Ok(today - chrono::Duration::weeks(1));
        }

        // Default to today if we can't parse
        Ok(today)
    }

    /// Extract numeric offset from phrases like "3 days from now"
    fn extract_days_offset(&self, text: &str, suffix: &str) -> Option<i64> {
        if !text.contains(suffix) {
            return None;
        }

        // Extract number before "days"
        let words: Vec<&str> = text.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            if word.contains("day") && i > 0 {
                // Try to parse the previous word as a number
                if let Ok(num) = words[i - 1].parse::<i64>() {
                    return Some(num);
                }
                // Try named numbers
                return Some(match words[i - 1] {
                    "one" | "a" => 1,
                    "two" => 2,
                    "three" => 3,
                    "four" => 4,
                    "five" => 5,
                    "six" => 6,
                    "seven" => 7,
                    _ => 1,
                });
            }
        }

        None
    }
}

impl LlmTool for SimpleDateTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let relative_date =
            args.get("relative_date").and_then(|v| v.as_str()).ok_or_else(|| {
                crate::error::MojenticError::ToolError(
                    "Missing required argument: relative_date".to_string(),
                )
            })?;

        let resolved_date = self.parse_relative_date(relative_date)?;
        let formatted_date = resolved_date.format("%Y-%m-%d").to_string();

        Ok(json!({
            "relative_date": relative_date,
            "resolved_date": formatted_date,
            "summary": format!("The date '{}' is {}", relative_date, formatted_date)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "resolve_date".to_string(),
                description: "Resolves relative date expressions to absolute dates. \
                             Takes text like 'tomorrow', 'three days from now', or 'next week' \
                             and returns the absolute date in YYYY-MM-DD format."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "relative_date": {
                            "type": "string",
                            "description": "The relative date expression to resolve (e.g., 'tomorrow', '3 days from now', 'next week')"
                        }
                    },
                    "required": ["relative_date"]
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
        let tool = SimpleDateTool;
        let desc = tool.descriptor();

        assert_eq!(desc.r#type, "function");
        assert_eq!(desc.function.name, "resolve_date");
        assert!(desc.function.description.contains("relative date"));
    }

    #[test]
    fn test_resolve_today() {
        let tool = SimpleDateTool;
        let args = HashMap::from([("relative_date".to_string(), json!("today"))]);

        let result = tool.run(&args).unwrap();
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();

        assert_eq!(result["relative_date"], "today");
        assert_eq!(result["resolved_date"], today);
    }

    #[test]
    fn test_resolve_tomorrow() {
        let tool = SimpleDateTool;
        let args = HashMap::from([("relative_date".to_string(), json!("tomorrow"))]);

        let result = tool.run(&args).unwrap();
        let tomorrow = (Local::now().date_naive() + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        assert_eq!(result["relative_date"], "tomorrow");
        assert_eq!(result["resolved_date"], tomorrow);
    }

    #[test]
    fn test_resolve_days_from_now() {
        let tool = SimpleDateTool;
        let args = HashMap::from([("relative_date".to_string(), json!("3 days from now"))]);

        let result = tool.run(&args).unwrap();
        let expected = (Local::now().date_naive() + chrono::Duration::days(3))
            .format("%Y-%m-%d")
            .to_string();

        assert_eq!(result["resolved_date"], expected);
    }

    #[test]
    fn test_resolve_days_ago() {
        let tool = SimpleDateTool;
        let args = HashMap::from([("relative_date".to_string(), json!("2 days ago"))]);

        let result = tool.run(&args).unwrap();
        let expected = (Local::now().date_naive() - chrono::Duration::days(2))
            .format("%Y-%m-%d")
            .to_string();

        assert_eq!(result["resolved_date"], expected);
    }

    #[test]
    fn test_missing_argument() {
        let tool = SimpleDateTool;
        let args = HashMap::new();

        let result = tool.run(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_days_offset() {
        let tool = SimpleDateTool;

        assert_eq!(tool.extract_days_offset("3 days from now", "from now"), Some(3));
        assert_eq!(tool.extract_days_offset("five days from now", "from now"), Some(5));
        assert_eq!(tool.extract_days_offset("2 days ago", "ago"), Some(2));
        assert_eq!(tool.extract_days_offset("tomorrow", "from now"), None);
    }
}
