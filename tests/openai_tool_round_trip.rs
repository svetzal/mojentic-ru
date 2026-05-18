//! Integration test: OpenAI tool-calling HTTP round-trip via mockito.
//!
//! Scenario exercised (using shared cross-port fixtures):
//!   1. User asks "What's the weather in Paris?"
//!   2. LLM responds with a get_weather(location="Paris") tool call
//!   3. Broker executes the tool locally, returns tool-result.json
//!   4. LLM produces a final text response
//!
//! The test uses mockito to intercept HTTP at the boundary, verifying that
//! `LlmBroker` + `OpenAIGateway` together produce the correct final answer.
//!
//! Fixture files are byte-identical across all four mojentic ports (ts/py/ex/ru).
//! If you change a fixture, update all four ports.

use async_trait::async_trait;
use mojentic::error::Result;
use mojentic::llm::gateway::CompletionConfig;
use mojentic::llm::gateways::OpenAIGateway;
use mojentic::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use mojentic::llm::{LlmBroker, LlmMessage};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

const RESPONSE_1: &str = include_str!("fixtures/openai_tool_round_trip/response-1-tool-call.json");
const RESPONSE_2: &str = include_str!("fixtures/openai_tool_round_trip/response-2-final.json");
const TOOL_RESULT: &str = include_str!("fixtures/openai_tool_round_trip/tool-result.json");

// ---------------------------------------------------------------------------
// GetWeatherTool — records its invocation arguments for post-hoc assertions.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct GetWeatherTool {
    calls: Arc<std::sync::Mutex<Vec<HashMap<String, Value>>>>,
}

#[async_trait]
impl LlmTool for GetWeatherTool {
    async fn run(
        &self,
        args: &HashMap<String, Value>,
        _ctx: &mojentic::llm::tools::ToolRunCtx,
    ) -> Result<Value> {
        self.calls.lock().unwrap().push(args.clone());
        Ok(serde_json::from_str::<Value>(TOOL_RESULT).expect("valid tool-result.json"))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "get_weather".to_string(),
                description: "Get current weather for a location".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "location": {"type": "string"}
                    },
                    "required": ["location"]
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Test: canonical get_weather round-trip backed by shared fixture files
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_openai_tool_call_round_trip() {
    let mut server = mockito::Server::new_async().await;

    // Compute the expected serialized form of the tool result content.
    // The broker calls serde_json::to_string(&output) where output is parsed
    // from TOOL_RESULT, so we reproduce the same serialization here.
    let tool_result_value: Value =
        serde_json::from_str(TOOL_RESULT).expect("valid tool-result.json");
    let expected_tool_content = serde_json::to_string(&tool_result_value).unwrap();

    // The broker serializes tool arguments via serde_json::to_string(&tc.arguments)
    // where tc.arguments is a HashMap<String, Value> parsed from the fixture's
    // "{\"location\": \"Paris\"}" string. Reproduce the same canonical form.
    let expected_arguments = serde_json::to_string(&json!({"location": "Paris"})).unwrap();

    // First HTTP call: verify tools array contains get_weather, return fixture response.
    // PartialJson checks that the actual request body's JSON contains all specified fields.
    let first_mock = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::PartialJson(json!({
            "tools": [{"function": {"name": "get_weather"}}]
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(RESPONSE_1)
        .create();

    // Second HTTP call: verify the messages round-trip correctly.
    // - user message preserved
    // - assistant message has tool_calls with correct arguments JSON string
    // - tool message has tool_call_id and content as serialized JSON string
    //
    // Note: PartialJson on arrays checks that actual[i] partially matches expected[i]
    // (zip semantics). Since no system message is injected, messages are [user, assistant, tool].
    // The `arguments` value is a JSON *string* (not an object), proving correct serialization.
    let second_mock = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::PartialJson(json!({
            "messages": [
                {"role": "user", "content": "What's the weather in Paris?"},
                {
                    "role": "assistant",
                    "tool_calls": [{
                        "function": {
                            "name": "get_weather",
                            "arguments": expected_arguments
                        }
                    }]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_fixture_get_weather",
                    "content": expected_tool_content
                }
            ]
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(RESPONSE_2)
        .create();

    let gateway = Arc::new(OpenAIGateway::with_api_key_and_base_url("test-key", server.url()));
    // Use "gpt-4" which is known to support tools per the model registry.
    let broker = LlmBroker::new("gpt-4", gateway, None);

    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(GetWeatherTool {
        calls: calls.clone(),
    })];

    let messages = vec![LlmMessage::user("What's the weather in Paris?")];
    let result = broker.generate(&messages, Some(&tools), None, None).await.unwrap();

    // Assertion 1 & 3: both HTTP mocks were satisfied (body matching verifies request structure)
    first_mock.assert();
    second_mock.assert();

    // Assertion 2: get_weather was invoked exactly once with location="Paris"
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1, "get_weather should be called exactly once");
    assert_eq!(
        recorded[0].get("location"),
        Some(&json!("Paris")),
        "get_weather should be called with location=Paris"
    );

    // Assertion 4: final text response matches fixture
    assert_eq!(result, "It's currently 22\u{b0}C and sunny in Paris.");
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
        .with_body(RESPONSE_1)
        .expect_at_least(1)
        .create();

    let gateway = Arc::new(OpenAIGateway::with_api_key_and_base_url("test-key", server.url()));
    let broker = LlmBroker::new("gpt-4", gateway, None);

    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(GetWeatherTool {
        calls: calls.clone(),
    })];

    let messages = vec![LlmMessage::user("Loop forever")];
    let result = broker
        .generate(
            &messages,
            Some(&tools),
            Some(CompletionConfig {
                max_tool_iterations: 2,
                ..Default::default()
            }),
            None,
        )
        .await;

    looping_mock.assert();

    assert!(result.is_err(), "Expected an error but got Ok");
    let err = result.unwrap_err();
    assert!(
        matches!(err, mojentic::MojenticError::MaxToolIterationsExceeded { limit: 2 }),
        "Unexpected error variant: {:?}",
        err
    );
}
