use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Message role in LLM conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool call from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

/// Message in LLM conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    #[serde(default = "default_role")]
    pub role: MessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_paths: Option<Vec<String>>,
}

fn default_role() -> MessageRole {
    MessageRole::User
}

/// Response from LLM gateway
#[derive(Debug, Clone)]
pub struct LlmGatewayResponse<T = ()> {
    pub content: Option<String>,
    pub object: Option<T>,
    pub tool_calls: Vec<LlmToolCall>,
}

impl LlmMessage {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: Some(content.into()),
            tool_calls: None,
            image_paths: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: Some(content.into()),
            tool_calls: None,
            image_paths: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: Some(content.into()),
            tool_calls: None,
            image_paths: None,
        }
    }

    /// Add image paths to this message
    pub fn with_images(mut self, paths: Vec<String>) -> Self {
        self.image_paths = Some(paths);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_role_serialization() {
        assert_eq!(serde_json::to_string(&MessageRole::System).unwrap(), "\"system\"");
        assert_eq!(serde_json::to_string(&MessageRole::User).unwrap(), "\"user\"");
        assert_eq!(serde_json::to_string(&MessageRole::Assistant).unwrap(), "\"assistant\"");
        assert_eq!(serde_json::to_string(&MessageRole::Tool).unwrap(), "\"tool\"");
    }

    #[test]
    fn test_message_role_deserialization() {
        assert_eq!(serde_json::from_str::<MessageRole>("\"system\"").unwrap(), MessageRole::System);
        assert_eq!(serde_json::from_str::<MessageRole>("\"user\"").unwrap(), MessageRole::User);
        assert_eq!(
            serde_json::from_str::<MessageRole>("\"assistant\"").unwrap(),
            MessageRole::Assistant
        );
        assert_eq!(serde_json::from_str::<MessageRole>("\"tool\"").unwrap(), MessageRole::Tool);
    }

    #[test]
    fn test_user_message() {
        let msg = LlmMessage::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, Some("Hello".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.image_paths.is_none());
    }

    #[test]
    fn test_system_message() {
        let msg = LlmMessage::system("You are a helpful assistant");
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, Some("You are a helpful assistant".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.image_paths.is_none());
    }

    #[test]
    fn test_assistant_message() {
        let msg = LlmMessage::assistant("I can help with that");
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, Some("I can help with that".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.image_paths.is_none());
    }

    #[test]
    fn test_message_with_images() {
        let msg = LlmMessage::user("Describe this image")
            .with_images(vec!["/path/to/image.jpg".to_string()]);
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, Some("Describe this image".to_string()));
        assert_eq!(msg.image_paths, Some(vec!["/path/to/image.jpg".to_string()]));
    }

    #[test]
    fn test_llm_tool_call_serialization() {
        let mut args = HashMap::new();
        args.insert("key".to_string(), serde_json::json!("value"));

        let tool_call = LlmToolCall {
            id: Some("call_123".to_string()),
            name: "test_tool".to_string(),
            arguments: args,
        };

        let json = serde_json::to_string(&tool_call).unwrap();
        assert!(json.contains("test_tool"));
        assert!(json.contains("call_123"));
    }

    #[test]
    fn test_llm_tool_call_without_id() {
        let tool_call = LlmToolCall {
            id: None,
            name: "test_tool".to_string(),
            arguments: HashMap::new(),
        };

        let json = serde_json::to_string(&tool_call).unwrap();
        // id should be omitted when None
        assert!(!json.contains("\"id\""));
        assert!(json.contains("test_tool"));
    }

    #[test]
    fn test_llm_message_serialization() {
        let msg = LlmMessage::user("test content");
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"test content\""));
    }

    #[test]
    fn test_llm_message_deserialization() {
        let json = r#"{"role":"assistant","content":"response"}"#;
        let msg: LlmMessage = serde_json::from_str(json).unwrap();

        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, Some("response".to_string()));
    }

    #[test]
    fn test_llm_message_default_role() {
        let json = r#"{"content":"test"}"#;
        let msg: LlmMessage = serde_json::from_str(json).unwrap();

        // Should default to User role
        assert_eq!(msg.role, MessageRole::User);
    }
}
