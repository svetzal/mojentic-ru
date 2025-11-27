//! Adapter for converting LLM messages to OpenAI format.

use crate::error::Result;
use crate::llm::models::{LlmMessage, LlmToolCall, MessageRole};
use base64::Engine;
use serde_json::Value;
use std::path::Path;
use tracing::warn;

/// OpenAI message format.
#[derive(Debug, Clone)]
pub struct OpenAIMessage {
    pub role: String,
    pub content: OpenAIContent,
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    pub tool_call_id: Option<String>,
}

/// OpenAI content format (text or multimodal).
#[derive(Debug, Clone)]
pub enum OpenAIContent {
    Text(String),
    Parts(Vec<OpenAIContentPart>),
}

/// A part of multimodal content.
#[derive(Debug, Clone)]
pub enum OpenAIContentPart {
    Text { text: String },
    ImageUrl { url: String },
}

/// OpenAI tool call format.
#[derive(Debug, Clone)]
pub struct OpenAIToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAIToolCallFunction,
}

/// OpenAI tool call function.
#[derive(Debug, Clone)]
pub struct OpenAIToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Determine image type from file extension.
fn get_image_type(file_path: &str) -> &'static str {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => "jpeg",
        "png" => "png",
        "gif" => "gif",
        "webp" => "webp",
        _ => "jpeg", // Default to jpeg for unknown types
    }
}

/// Read and encode an image file as base64.
fn encode_image_as_base64(file_path: &str) -> Result<String> {
    let bytes = std::fs::read(file_path)?;
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let image_type = get_image_type(file_path);
    Ok(format!("data:image/{};base64,{}", image_type, base64_data))
}

/// Adapt LLM messages to OpenAI format.
pub fn adapt_messages_to_openai(messages: &[LlmMessage]) -> Result<Vec<Value>> {
    let mut result = Vec::new();

    for msg in messages {
        let openai_msg = match msg.role {
            MessageRole::System => {
                serde_json::json!({
                    "role": "system",
                    "content": msg.content.as_deref().unwrap_or("")
                })
            }
            MessageRole::User => {
                // Check for images
                if let Some(ref image_paths) = msg.image_paths {
                    if !image_paths.is_empty() {
                        let mut content_parts = Vec::new();

                        // Add text content
                        if let Some(ref text) = msg.content {
                            if !text.is_empty() {
                                content_parts.push(serde_json::json!({
                                    "type": "text",
                                    "text": text
                                }));
                            }
                        }

                        // Add images
                        for path in image_paths {
                            match encode_image_as_base64(path) {
                                Ok(data_url) => {
                                    content_parts.push(serde_json::json!({
                                        "type": "image_url",
                                        "image_url": {
                                            "url": data_url
                                        }
                                    }));
                                }
                                Err(e) => {
                                    warn!(path = path, error = %e, "Failed to encode image");
                                }
                            }
                        }

                        serde_json::json!({
                            "role": "user",
                            "content": content_parts
                        })
                    } else {
                        serde_json::json!({
                            "role": "user",
                            "content": msg.content.as_deref().unwrap_or("")
                        })
                    }
                } else {
                    serde_json::json!({
                        "role": "user",
                        "content": msg.content.as_deref().unwrap_or("")
                    })
                }
            }
            MessageRole::Assistant => {
                let mut assistant_msg = serde_json::json!({
                    "role": "assistant"
                });

                if let Some(ref content) = msg.content {
                    assistant_msg["content"] = serde_json::json!(content);
                }

                // Add tool calls if present
                if let Some(ref tool_calls) = msg.tool_calls {
                    let formatted_calls: Vec<Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id.as_deref().unwrap_or(""),
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": serde_json::to_string(&tc.arguments).unwrap_or_default()
                                }
                            })
                        })
                        .collect();
                    assistant_msg["tool_calls"] = serde_json::json!(formatted_calls);
                }

                assistant_msg
            }
            MessageRole::Tool => {
                // Tool messages need tool_call_id - use the first tool call id if available
                let tool_call_id = msg
                    .tool_calls
                    .as_ref()
                    .and_then(|tcs| tcs.first())
                    .and_then(|tc| tc.id.clone())
                    .unwrap_or_default();

                serde_json::json!({
                    "role": "tool",
                    "content": msg.content.as_deref().unwrap_or(""),
                    "tool_call_id": tool_call_id
                })
            }
        };

        result.push(openai_msg);
    }

    Ok(result)
}

/// Convert tool calls from OpenAI format to internal format.
pub fn convert_tool_calls(tool_calls: &[Value]) -> Vec<LlmToolCall> {
    tool_calls
        .iter()
        .filter_map(|tc| {
            let id = tc["id"].as_str().map(String::from);
            let name = tc["function"]["name"].as_str()?.to_string();
            let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");

            // Parse arguments as JSON object
            let arguments: std::collections::HashMap<String, Value> =
                serde_json::from_str(args_str).unwrap_or_default();

            Some(LlmToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_get_image_type_jpg() {
        assert_eq!(get_image_type("/path/to/image.jpg"), "jpeg");
        assert_eq!(get_image_type("/path/to/image.jpeg"), "jpeg");
    }

    #[test]
    fn test_get_image_type_png() {
        assert_eq!(get_image_type("/path/to/image.png"), "png");
    }

    #[test]
    fn test_get_image_type_gif() {
        assert_eq!(get_image_type("/path/to/image.gif"), "gif");
    }

    #[test]
    fn test_get_image_type_webp() {
        assert_eq!(get_image_type("/path/to/image.webp"), "webp");
    }

    #[test]
    fn test_get_image_type_unknown() {
        assert_eq!(get_image_type("/path/to/image.unknown"), "jpeg");
    }

    #[test]
    fn test_adapt_system_message() {
        let messages = vec![LlmMessage::system("You are helpful")];

        let result = adapt_messages_to_openai(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "system");
        assert_eq!(result[0]["content"], "You are helpful");
    }

    #[test]
    fn test_adapt_user_message() {
        let messages = vec![LlmMessage::user("Hello")];

        let result = adapt_messages_to_openai(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[0]["content"], "Hello");
    }

    #[test]
    fn test_adapt_assistant_message() {
        let messages = vec![LlmMessage::assistant("Hi there")];

        let result = adapt_messages_to_openai(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "assistant");
        assert_eq!(result[0]["content"], "Hi there");
    }

    #[test]
    fn test_adapt_user_message_with_images() {
        // Create a temporary image file
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"fake image data").unwrap();
        let path = temp_file.path().to_string_lossy().to_string();

        let messages =
            vec![LlmMessage::user("Describe this image").with_images(vec![path.clone()])];

        let result = adapt_messages_to_openai(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "user");

        let content = &result[0]["content"];
        assert!(content.is_array());

        let parts = content.as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "Describe this image");
        assert_eq!(parts[1]["type"], "image_url");
        assert!(parts[1]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn test_adapt_assistant_with_tool_calls() {
        let tool_call = LlmToolCall {
            id: Some("call_123".to_string()),
            name: "get_weather".to_string(),
            arguments: {
                let mut map = HashMap::new();
                map.insert("location".to_string(), serde_json::json!("NYC"));
                map
            },
        };

        let messages = vec![LlmMessage {
            role: MessageRole::Assistant,
            content: None,
            tool_calls: Some(vec![tool_call]),
            image_paths: None,
        }];

        let result = adapt_messages_to_openai(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "assistant");

        let tool_calls = &result[0]["tool_calls"];
        assert!(tool_calls.is_array());

        let calls = tool_calls.as_array().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["id"], "call_123");
        assert_eq!(calls[0]["type"], "function");
        assert_eq!(calls[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn test_adapt_tool_message() {
        let messages = vec![LlmMessage {
            role: MessageRole::Tool,
            content: Some("Weather result: 72F".to_string()),
            tool_calls: Some(vec![LlmToolCall {
                id: Some("call_123".to_string()),
                name: "get_weather".to_string(),
                arguments: HashMap::new(),
            }]),
            image_paths: None,
        }];

        let result = adapt_messages_to_openai(&messages).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["role"], "tool");
        assert_eq!(result[0]["content"], "Weather result: 72F");
        assert_eq!(result[0]["tool_call_id"], "call_123");
    }

    #[test]
    fn test_convert_tool_calls() {
        let tool_calls = vec![serde_json::json!({
            "id": "call_abc",
            "type": "function",
            "function": {
                "name": "search",
                "arguments": "{\"query\": \"test\"}"
            }
        })];

        let result = convert_tool_calls(&tool_calls);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, Some("call_abc".to_string()));
        assert_eq!(result[0].name, "search");
        assert_eq!(result[0].arguments.get("query"), Some(&serde_json::json!("test")));
    }

    #[test]
    fn test_convert_tool_calls_empty_args() {
        let tool_calls = vec![serde_json::json!({
            "id": "call_xyz",
            "type": "function",
            "function": {
                "name": "no_args_tool",
                "arguments": "{}"
            }
        })];

        let result = convert_tool_calls(&tool_calls);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "no_args_tool");
        assert!(result[0].arguments.is_empty());
    }
}
