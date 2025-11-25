use crate::error::Result;
use serde_json::Value;
use std::collections::HashMap;

/// Descriptor for tool function parameters
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDescriptor {
    pub r#type: String,
    pub function: FunctionDescriptor,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionDescriptor {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Trait for LLM tools
pub trait LlmTool: Send + Sync {
    /// Execute the tool with given arguments
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value>;

    /// Get tool descriptor for LLM
    fn descriptor(&self) -> ToolDescriptor;

    /// Check if this tool matches the given name
    fn matches(&self, name: &str) -> bool {
        self.descriptor().function.name == name
    }

    /// Clone the tool into a Box
    ///
    /// This method is required to support cloning trait objects.
    /// Implementations should return `Box::new(self.clone())`.
    fn clone_box(&self) -> Box<dyn LlmTool>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_descriptor_serialization() {
        let descriptor = ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "test_tool".to_string(),
                description: "A test tool".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "arg1": {"type": "string"}
                    }
                }),
            },
        };

        let json = serde_json::to_string(&descriptor).unwrap();
        assert!(json.contains("test_tool"));
        assert!(json.contains("A test tool"));
        assert!(json.contains("function"));
    }

    #[test]
    fn test_tool_descriptor_deserialization() {
        let json = r#"{
            "type": "function",
            "function": {
                "name": "calculator",
                "description": "Perform calculations",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "expression": {"type": "string"}
                    }
                }
            }
        }"#;

        let descriptor: ToolDescriptor = serde_json::from_str(json).unwrap();
        assert_eq!(descriptor.r#type, "function");
        assert_eq!(descriptor.function.name, "calculator");
        assert_eq!(descriptor.function.description, "Perform calculations");
    }

    #[test]
    fn test_function_descriptor_clone() {
        let desc1 = FunctionDescriptor {
            name: "test".to_string(),
            description: "desc".to_string(),
            parameters: json!({"type": "object"}),
        };

        let desc2 = desc1.clone();
        assert_eq!(desc1.name, desc2.name);
        assert_eq!(desc1.description, desc2.description);
    }

    struct MockTool;

    impl LlmTool for MockTool {
        fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
            Ok(json!("result"))
        }

        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                r#type: "function".to_string(),
                function: FunctionDescriptor {
                    name: "mock_tool".to_string(),
                    description: "A mock tool".to_string(),
                    parameters: json!({}),
                },
            }
        }

        fn clone_box(&self) -> Box<dyn LlmTool> {
            Box::new(MockTool)
        }
    }

    #[test]
    fn test_tool_matches() {
        let tool = MockTool;
        assert!(tool.matches("mock_tool"));
        assert!(!tool.matches("other_tool"));
    }

    #[test]
    fn test_tool_run() {
        let tool = MockTool;
        let args = HashMap::new();
        let result = tool.run(&args).unwrap();
        assert_eq!(result, json!("result"));
    }
}
