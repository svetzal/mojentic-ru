//! Integration test: OpenAI tool-calling HTTP round-trip via mockito.
//!
//! Scenario exercised:
//!   1. User message → LLM responds with a `tool_calls` request
//!   2. Broker executes the tool locally
//!   3. Tool result is sent back to the LLM as a `tool` message
//!   4. LLM produces a final text response
//!
//! The test uses mockito to intercept HTTP at the boundary, verifying that
//! `LlmBroker` + `OpenAIGateway` together produce the correct final answer.

use mojentic::error::Result;
use mojentic::llm::gateways::OpenAIGateway;
use mojentic::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use mojentic::llm::{LlmBroker, LlmMessage};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// A minimal tool whose single purpose is returning a canned answer.
// ---------------------------------------------------------------------------

struct EchoTool {
    name: String,
    response: Value,
}

impl LlmTool for EchoTool {
    fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
        Ok(self.response.clone())
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: self.name.clone(),
                description: "Returns a canned response".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(EchoTool {
            name: self.name.clone(),
            response: self.response.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Helper: build a well-formed OpenAI chat-completions response that contains
// a single tool_call.
// ---------------------------------------------------------------------------
fn tool_call_response(tool_name: &str, call_id: &str) -> String {
    serde_json::json!({
        "id": "chatcmpl-abc123",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "finish_reason": "tool_calls",
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "arguments": "{}"
                    }
                }]
            }
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Helper: build an OpenAI response with final text content.
// ---------------------------------------------------------------------------
fn text_response(content: &str) -> String {
    serde_json::json!({
        "id": "chatcmpl-def456",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": {
                "role": "assistant",
                "content": content
            }
        }],
        "usage": {"prompt_tokens": 20, "completion_tokens": 10, "total_tokens": 30}
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Test: one tool call, then a final answer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_openai_tool_call_round_trip() {
    let mut server = mockito::Server::new_async().await;

    // First HTTP call: LLM requests tool "get_info"
    let first_mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(tool_call_response("get_info", "call_001"))
        .create();

    // Second HTTP call: LLM returns the final answer after receiving tool result
    let second_mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(text_response("The answer is 42."))
        .create();

    let gateway = Arc::new(OpenAIGateway::with_api_key_and_base_url("test-key", server.url()));
    let broker = LlmBroker::new("gpt-4", gateway, None);

    let echo_tool = EchoTool {
        name: "get_info".to_string(),
        response: json!({"value": 42}),
    };
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(echo_tool)];

    let messages = vec![LlmMessage::user("What is the answer?")];
    let result = broker.generate(&messages, Some(&tools), None, None).await.unwrap();

    first_mock.assert();
    second_mock.assert();

    assert_eq!(result, "The answer is 42.");
}

// ---------------------------------------------------------------------------
// Test: max_tool_iterations is respected even at the HTTP boundary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_openai_tool_call_max_iterations_exceeded() {
    let mut server = mockito::Server::new_async().await;

    // Every HTTP call returns another tool_call — the broker should stop at limit
    let looping_mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(tool_call_response("loop_tool", "call_loop"))
        .expect_at_least(1)
        .create();

    let gateway = Arc::new(OpenAIGateway::with_api_key_and_base_url("test-key", server.url()));
    let broker = LlmBroker::new("gpt-4", gateway, None).with_max_tool_iterations(2);

    let echo_tool = EchoTool {
        name: "loop_tool".to_string(),
        response: json!({"keep": "going"}),
    };
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(echo_tool)];

    let messages = vec![LlmMessage::user("Loop forever")];
    let result = broker.generate(&messages, Some(&tools), None, None).await;

    looping_mock.assert();

    assert!(result.is_err(), "Expected an error but got Ok");
    let err = result.unwrap_err();
    assert!(
        matches!(err, mojentic::MojenticError::MaxToolIterationsExceeded { limit: 2 }),
        "Unexpected error variant: {:?}",
        err
    );
}
