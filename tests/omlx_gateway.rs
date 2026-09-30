//! Integration tests for the oMLX gateway, over HTTP with mockito.
//!
//! The responses are live captures from an oMLX server (see
//! `fixtures/omlx/README.md`). No test talks to a real server.

use async_trait::async_trait;
use futures::stream::StreamExt;
use mockito::Matcher;
use mojentic::error::{MojenticError, Result};
use mojentic::llm::gateway::{CompletionConfig, ReasoningEffort, ResponseFormat, StreamChunk};
use mojentic::llm::gateways::omlx::RESPONSE_FORMAT_WARNING;
use mojentic::llm::gateways::{OmlxConfig, OmlxGateway};
use mojentic::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor, ToolRunCtx};
use mojentic::llm::{LlmBroker, LlmGateway, LlmMessage, StreamEvent, StreamEventError};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const CHAT_THINKING: &str = include_str!("fixtures/omlx/chat_thinking.json");
const CHAT_THINKING_DISABLED: &str = include_str!("fixtures/omlx/chat_thinking_disabled.json");
const CHAT_TOOL_CALL: &str = include_str!("fixtures/omlx/chat_tool_call.json");
const CHAT_AFTER_TOOL_RESULT: &str = include_str!("fixtures/omlx/chat_after_tool_result.json");
const CHAT_JSON_SCHEMA: &str = include_str!("fixtures/omlx/chat_json_schema.json");
const CHAT_LENGTH: &str = include_str!("fixtures/omlx/chat_length.json");
const STREAM_THINKING: &str = include_str!("fixtures/omlx/stream_thinking.sse");
const STREAM_TOOL_CALL: &str = include_str!("fixtures/omlx/stream_tool_call.sse");
const STREAM_LENGTH: &str = include_str!("fixtures/omlx/stream_length.sse");
const MODELS: &str = include_str!("fixtures/omlx/models.json");
const MODEL_LOAD: &str = include_str!("fixtures/omlx/model_load.json");
const MODEL_UNLOAD: &str = include_str!("fixtures/omlx/model_unload.json");
const ERROR_MODEL_NOT_LOADED: &str = include_str!("fixtures/omlx/error_model_not_loaded.json");
const ERROR_MODEL_NOT_FOUND: &str = include_str!("fixtures/omlx/error_model_not_found.json");
const ERROR_NOT_EMBEDDING_MODEL: &str =
    include_str!("fixtures/omlx/error_not_embedding_model.json");

const MODEL: &str = "Qwen3.8-27B-MLX-8bit";
const CHAT_PATH: &str = "/v1/chat/completions";
const KEEPALIVE_FRAME: &str = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"keepalive\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n";

/// Tests that change `OMLX_*` variables hold this lock.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// A gateway for `url` built without reading the environment.
fn gateway(url: String) -> OmlxGateway {
    OmlxGateway::with_config(OmlxConfig {
        host: url,
        api_key: None,
        timeout: None,
    })
}

fn user(text: &str) -> Vec<LlmMessage> {
    vec![LlmMessage::user(text)]
}

/// Serve `body` for one chat request and capture the JSON body the gateway sent.
async fn chat_server(body: &'static str) -> (mockito::ServerGuard, Arc<Mutex<Option<Value>>>) {
    let captured = Arc::new(Mutex::new(None::<Value>));
    let sink = captured.clone();
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", CHAT_PATH)
        .match_request(move |request| {
            *sink.lock().unwrap() =
                serde_json::from_slice(request.body().expect("request body")).ok();
            true
        })
        .with_status(200)
        .with_header("content-type", content_type(body))
        .with_body(body)
        .create_async()
        .await;
    (server, captured)
}

/// The content type oMLX sends for `body`: an event stream or JSON.
fn content_type(body: &str) -> &'static str {
    if body.starts_with("data:") {
        "text/event-stream"
    } else {
        "application/json"
    }
}

fn sent(captured: &Arc<Mutex<Option<Value>>>) -> Value {
    captured.lock().unwrap().take().expect("the gateway sent a JSON body")
}

// ---------------------------------------------------------------------------
// 1. Configuration
// ---------------------------------------------------------------------------

mod configuration {
    use super::*;

    fn clear_env() {
        std::env::remove_var("OMLX_HOST");
        std::env::remove_var("OMLX_API_KEY");
        std::env::remove_var("OMLX_TIMEOUT");
    }

    #[test]
    fn defaults_to_localhost_8000_without_a_key_and_a_ten_minute_timeout() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();

        let config = OmlxConfig::default();

        assert_eq!(config.host, "http://localhost:8000");
        assert_eq!(config.api_key, None);
        assert_eq!(config.timeout, Some(std::time::Duration::from_millis(600_000)));
    }

    #[test]
    fn reads_host_key_and_millisecond_timeout_from_the_environment() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("OMLX_HOST", "http://studio.local:9000");
        std::env::set_var("OMLX_API_KEY", "env-key");
        std::env::set_var("OMLX_TIMEOUT", "90000");

        let config = OmlxConfig::default();
        clear_env();

        assert_eq!(config.host, "http://studio.local:9000");
        assert_eq!(config.api_key.as_deref(), Some("env-key"));
        assert_eq!(config.timeout, Some(std::time::Duration::from_secs(90)));
    }

    #[test]
    fn an_empty_key_or_unreadable_timeout_falls_back_to_the_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("OMLX_API_KEY", "");
        std::env::set_var("OMLX_TIMEOUT", "soon");

        let config = OmlxConfig::default();
        clear_env();

        assert_eq!(config.api_key, None);
        assert_eq!(config.timeout, Some(std::time::Duration::from_millis(600_000)));
    }

    #[tokio::test]
    async fn new_uses_the_environment_host_and_key_under_v1() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .match_header("authorization", "Bearer env-key")
            .with_status(200)
            .with_body(MODELS)
            .create_async()
            .await;

        let gateway = {
            let _lock = ENV_LOCK.lock().unwrap();
            clear_env();
            std::env::set_var("OMLX_HOST", server.url());
            std::env::set_var("OMLX_API_KEY", "env-key");
            let gateway = OmlxGateway::new();
            clear_env();
            gateway
        };
        gateway.get_available_models().await.unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn explicit_values_take_precedence_over_the_environment() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .match_header("authorization", "Bearer explicit-key")
            .with_status(200)
            .with_body(MODELS)
            .create_async()
            .await;

        let gateway = {
            let _lock = ENV_LOCK.lock().unwrap();
            clear_env();
            std::env::set_var("OMLX_HOST", "http://127.0.0.1:1");
            std::env::set_var("OMLX_API_KEY", "env-key");
            let gateway = OmlxGateway::with_config(OmlxConfig {
                host: server.url(),
                api_key: Some("explicit-key".into()),
                ..Default::default()
            });
            clear_env();
            gateway
        };
        gateway.get_available_models().await.unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn sends_no_authorization_header_without_a_key() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .match_header("authorization", Matcher::Missing)
            .with_status(200)
            .with_body(MODELS)
            .create_async()
            .await;

        gateway(server.url()).get_available_models().await.unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn a_trailing_slash_on_the_host_is_ignored() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_body(MODELS)
            .create_async()
            .await;

        gateway(format!("{}/", server.url())).get_available_models().await.unwrap();

        mock.assert_async().await;
    }
}

// ---------------------------------------------------------------------------
// 2. Request body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn explicit_blank_keys_send_no_authorization_header() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/v1/models")
        .match_header("authorization", Matcher::Missing)
        .with_header("content-type", "application/json")
        .with_body(MODELS)
        .expect(3)
        .create_async()
        .await;
    for key in ["", " ", "\t"] {
        OmlxGateway::with_config(OmlxConfig {
            host: server.url(),
            api_key: Some(key.to_string()),
            timeout: None,
        })
        .get_available_models()
        .await
        .unwrap();
    }
    mock.assert_async().await;
}

mod request_body {
    use super::*;

    #[tokio::test]
    async fn forwards_every_setting_without_per_model_adaptation() {
        let (server, captured) = chat_server(CHAT_THINKING).await;
        // "o1" and "o3" would make the OpenAI registry treat this as a reasoning model.
        let model = "o1-o3-local-distill";
        let config = CompletionConfig {
            temperature: 0.5,
            max_tokens: 256,
            top_p: Some(0.25),
            top_k: Some(20),
            num_ctx: 4096,
            num_predict: Some(99),
            reasoning_effort: Some(ReasoningEffort::Low),
            ..Default::default()
        };

        gateway(server.url()).complete(model, &user("Hi"), None, &config).await.unwrap();

        let body = sent(&captured);
        assert_eq!(body["model"], model);
        assert_eq!(body["messages"], json!([{"role": "user", "content": "Hi"}]));
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["max_tokens"], 256);
        assert_eq!(body["top_p"], 0.25);
        assert_eq!(body["top_k"], 20);
        assert_eq!(body["reasoning_effort"], "low");
        for absent in [
            "max_completion_tokens",
            "num_ctx",
            "num_predict",
            "options",
            "stream",
            "response_format",
            "tools",
        ] {
            assert!(body.get(absent).is_none(), "{absent} should not be sent: {body}");
        }
    }

    #[tokio::test]
    async fn omits_unset_optional_settings() {
        let (server, captured) = chat_server(CHAT_THINKING).await;

        gateway(server.url())
            .complete(MODEL, &user("Hi"), None, &CompletionConfig::default())
            .await
            .unwrap();

        let body = sent(&captured);
        assert_eq!(body["max_tokens"], 16384);
        for absent in ["top_p", "top_k", "reasoning_effort", "enable_thinking"] {
            assert!(body.get(absent).is_none(), "{absent} should not be sent: {body}");
        }
    }

    #[tokio::test]
    async fn always_sends_max_tokens_from_the_config() {
        let (server, captured) = chat_server(CHAT_LENGTH).await;
        let config = CompletionConfig {
            max_tokens: 0,
            num_predict: Some(7),
            ..Default::default()
        };

        gateway(server.url()).complete(MODEL, &user("Hi"), None, &config).await.unwrap();

        assert_eq!(sent(&captured)["max_tokens"], 0);
    }

    #[tokio::test]
    async fn forwards_low_medium_and_high_reasoning_effort_as_strings() {
        for (effort, sent_value) in [
            (ReasoningEffort::Low, "low"),
            (ReasoningEffort::Medium, "medium"),
            (ReasoningEffort::High, "high"),
        ] {
            let (server, captured) = chat_server(CHAT_THINKING).await;
            let config = CompletionConfig {
                reasoning_effort: Some(effort),
                ..Default::default()
            };

            gateway(server.url()).complete(MODEL, &user("Hi"), None, &config).await.unwrap();

            let body = sent(&captured);
            assert_eq!(body["reasoning_effort"], sent_value);
            assert!(body.get("enable_thinking").is_none(), "{body}");
        }
    }

    #[tokio::test]
    async fn disabled_reasoning_effort_turns_thinking_off() {
        let (server, captured) = chat_server(CHAT_THINKING_DISABLED).await;
        let config = CompletionConfig {
            reasoning_effort: Some(ReasoningEffort::Disabled),
            ..Default::default()
        };

        let response =
            gateway(server.url()).complete(MODEL, &user("Hi"), None, &config).await.unwrap();

        let body = sent(&captured);
        assert_eq!(body["enable_thinking"], false);
        assert!(body.get("reasoning_effort").is_none(), "{body}");
        assert_eq!(response.thinking, None);
    }

    #[tokio::test]
    async fn disabled_reasoning_effort_turns_thinking_off_when_streaming() {
        let (server, captured) = chat_server(STREAM_THINKING).await;
        let config = CompletionConfig {
            reasoning_effort: Some(ReasoningEffort::Disabled),
            ..Default::default()
        };
        let gateway = gateway(server.url());
        let messages = user("Hi");

        let _: Vec<_> = gateway
            .complete_stream_events(MODEL, &messages, &config)
            .expect("supported")
            .collect()
            .await;

        let body = sent(&captured);
        assert_eq!(body["enable_thinking"], false);
        assert!(body.get("reasoning_effort").is_none(), "{body}");
    }

    #[tokio::test]
    async fn sends_tools_for_any_model_name() {
        let (server, captured) = chat_server(CHAT_TOOL_CALL).await;
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(ResolveDateTool::default())];

        gateway(server.url())
            .complete("o1-mini-local", &user("Date?"), Some(&tools), &CompletionConfig::default())
            .await
            .unwrap();

        let body = sent(&captured);
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "resolve_date");
    }

    #[tokio::test]
    async fn forwards_each_configured_response_format() {
        let schema = json!({"type": "object", "properties": {"n": {"type": "integer"}}});
        for (format, expected) in [
            (ResponseFormat::Text, json!({"type": "text"})),
            (ResponseFormat::JsonObject { schema: None }, json!({"type": "json_object"})),
            (
                ResponseFormat::JsonObject {
                    schema: Some(schema.clone()),
                },
                json!({"type": "json_schema", "json_schema": {"name": "response", "schema": schema}}),
            ),
        ] {
            let (server, captured) = chat_server(CHAT_JSON_SCHEMA).await;
            let config = CompletionConfig {
                response_format: Some(format),
                ..Default::default()
            };

            gateway(server.url()).complete(MODEL, &user("Hi"), None, &config).await.unwrap();

            assert_eq!(sent(&captured)["response_format"], expected);
        }
    }
}

// ---------------------------------------------------------------------------
// 3, 6. Response mapping
// ---------------------------------------------------------------------------

mod response_mapping {
    use super::*;

    #[tokio::test]
    async fn reasoning_content_becomes_thinking_and_usage_is_kept_whole() {
        let (server, _) = chat_server(CHAT_THINKING).await;

        let response = gateway(server.url())
            .complete(MODEL, &user("Reply with exactly: hello"), None, &CompletionConfig::default())
            .await
            .unwrap();

        assert_eq!(response.content.as_deref(), Some("hello"));
        assert_eq!(
            response.thinking.as_deref(),
            Some(
                "We need to reply exactly: hello. User said \"Reply with exactly: hello\". Need final \"hello\". Ensure no extra."
            )
        );
        let fixture: Value = serde_json::from_str(CHAT_THINKING).unwrap();
        assert_eq!(response.evidence.usage.as_ref(), Some(&fixture["usage"]));
        assert_eq!(response.evidence.usage.as_ref().unwrap()["model_load_duration"], 8.78);
        assert_eq!(response.evidence.provider_model.as_deref(), Some(MODEL));
        assert_eq!(response.evidence.finish_reason.as_deref(), Some("stop"));
        assert_eq!(response.evidence.metadata["id"], "chatcmpl-8c2b3fa6");
    }

    #[tokio::test]
    async fn a_response_without_reasoning_content_has_no_thinking() {
        let (server, _) = chat_server(CHAT_THINKING_DISABLED).await;

        let response = gateway(server.url())
            .complete(MODEL, &user("Reply with exactly: hello"), None, &CompletionConfig::default())
            .await
            .unwrap();

        assert_eq!(response.content.as_deref(), Some("hello"));
        assert_eq!(response.thinking, None);
    }

    #[tokio::test]
    async fn truncation_during_thinking_keeps_the_fields_as_reported() {
        let (server, _) = chat_server(CHAT_LENGTH).await;
        let config = CompletionConfig {
            max_tokens: 5,
            ..Default::default()
        };

        let response =
            gateway(server.url()).complete(MODEL, &user("Hi"), None, &config).await.unwrap();

        assert_eq!(response.evidence.finish_reason.as_deref(), Some("length"));
        assert_eq!(response.content.as_deref(), Some("We need to respond to"));
        assert_eq!(response.thinking, None);
    }

    #[tokio::test]
    async fn an_error_status_is_a_gateway_error_with_status_and_body() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", CHAT_PATH)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(ERROR_MODEL_NOT_FOUND)
            .create_async()
            .await;

        let error = gateway(server.url())
            .complete("nope", &user("Hi"), None, &CompletionConfig::default())
            .await
            .unwrap_err();

        let MojenticError::GatewayError(message) = error else {
            panic!("expected a gateway error, got {error:?}");
        };
        assert!(message.contains("404"), "{message}");
        assert!(message.contains(ERROR_MODEL_NOT_FOUND), "{message}");
    }
}

// ---------------------------------------------------------------------------
// 4. Tools
// ---------------------------------------------------------------------------

/// A `resolve_date` tool that records its arguments.
#[derive(Clone, Default)]
struct ResolveDateTool {
    calls: Arc<Mutex<Vec<HashMap<String, Value>>>>,
}

#[async_trait]
impl LlmTool for ResolveDateTool {
    async fn run(&self, args: &HashMap<String, Value>, _ctx: &ToolRunCtx) -> Result<Value> {
        self.calls.lock().unwrap().push(args.clone());
        Ok(json!("2026-09-29"))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "resolve_date".to_string(),
                description: "Resolve a relative date".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {"relative": {"type": "string"}},
                    "required": ["relative"]
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

mod tools {
    use super::*;

    #[tokio::test]
    async fn parses_a_tool_call_and_its_reasoning() {
        let (server, _) = chat_server(CHAT_TOOL_CALL).await;
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(ResolveDateTool::default())];

        let response = gateway(server.url())
            .complete(
                MODEL,
                &user("What is today's date?"),
                Some(&tools),
                &CompletionConfig::default(),
            )
            .await
            .unwrap();

        assert_eq!(response.tool_calls.len(), 1);
        let call = &response.tool_calls[0];
        assert_eq!(call.id.as_deref(), Some("call_bd4d55c2"));
        assert_eq!(call.name, "resolve_date");
        assert_eq!(call.arguments["relative"], "today");
        assert_eq!(response.content, None);
        assert!(response.thinking.unwrap().contains("resolve_date tool"));
        assert_eq!(response.evidence.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[tokio::test]
    async fn round_trips_a_tool_result_through_the_broker() {
        let mut server = mockito::Server::new_async().await;
        let first = server
            .mock("POST", CHAT_PATH)
            .match_body(Matcher::PartialJson(json!({"messages": [{"role": "user"}]})))
            .with_status(200)
            .with_body(CHAT_TOOL_CALL)
            .create_async()
            .await;
        let second = server
            .mock("POST", CHAT_PATH)
            .match_body(Matcher::PartialJson(json!({
                "messages": [
                    {"role": "user", "content": "What is today's date?"},
                    {"role": "assistant", "tool_calls": [{
                        "id": "call_bd4d55c2",
                        "function": {"name": "resolve_date", "arguments": "{\"relative\":\"today\"}"}
                    }]},
                    {"role": "tool", "tool_call_id": "call_bd4d55c2", "content": "\"2026-09-29\""}
                ]
            })))
            .with_status(200)
            .with_body(CHAT_AFTER_TOOL_RESULT)
            .create_async()
            .await;
        let tool = ResolveDateTool::default();
        let calls = tool.calls.clone();
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(tool)];
        let broker = LlmBroker::new(MODEL, Arc::new(gateway(server.url())), None);

        let answer = broker
            .generate(&user("What is today's date?"), Some(&tools), None, None)
            .await
            .unwrap();

        first.assert_async().await;
        second.assert_async().await;
        assert_eq!(calls.lock().unwrap()[0]["relative"], "today");
        assert_eq!(answer, "Today's date is **September 29, 2026** (2026-09-29).");
    }
}

// ---------------------------------------------------------------------------
// 5. Structured output
// ---------------------------------------------------------------------------

mod structured_output {
    use super::*;

    fn person_schema() -> Value {
        json!({
            "type": "object",
            "properties": {"name": {"type": "string"}, "age": {"type": "integer"}},
            "required": ["name", "age"]
        })
    }

    async fn structured_server(warning: Option<&str>) -> (mockito::ServerGuard, mockito::Mock) {
        let mut server = mockito::Server::new_async().await;
        let mut mock = server.mock("POST", CHAT_PATH).match_body(Matcher::PartialJson(json!({
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "response", "schema": person_schema()}
            }
        })));
        if let Some(warning) = warning {
            mock = mock.with_header("warning", warning);
        }
        let mock = mock.with_status(200).with_body(CHAT_JSON_SCHEMA).create_async().await;
        (server, mock)
    }

    #[tokio::test]
    async fn sends_a_json_schema_response_format_and_parses_the_object() {
        let (server, mock) = structured_server(None).await;

        let response = gateway(server.url())
            .complete_json_response(
                MODEL,
                &user("Ada, 36"),
                person_schema(),
                &CompletionConfig::default(),
            )
            .await
            .unwrap();

        mock.assert_async().await;
        assert_eq!(response.object, Some(json!({"name": "Ada", "age": 36})));
        assert_eq!(response.content.as_deref(), Some("{\"name\": \"Ada\", \"age\": 36}"));
        assert_eq!(response.evidence.provider_model.as_deref(), Some(MODEL));
        assert!(!response.evidence.metadata.contains_key(RESPONSE_FORMAT_WARNING));
    }

    #[tokio::test]
    async fn records_the_warning_header_in_metadata() {
        let warning = "299 - \"json_schema could not be compiled; using prompt instructions\"";
        let (server, _) = structured_server(Some(warning)).await;

        let response = gateway(server.url())
            .complete_json_response(
                MODEL,
                &user("Ada, 36"),
                person_schema(),
                &CompletionConfig::default(),
            )
            .await
            .unwrap();

        assert_eq!(response.evidence.metadata[RESPONSE_FORMAT_WARNING], warning);
        assert_eq!(response.object, Some(json!({"name": "Ada", "age": 36})));
    }

    #[tokio::test]
    async fn joins_several_warning_headers() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", CHAT_PATH)
            .with_header("warning", "299 - \"first\"")
            .with_header("warning", "299 - \"second\"")
            .with_status(200)
            .with_body(CHAT_JSON_SCHEMA)
            .create_async()
            .await;

        let response = gateway(server.url())
            .complete_json_response(
                MODEL,
                &user("Ada, 36"),
                person_schema(),
                &CompletionConfig::default(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.evidence.metadata[RESPONSE_FORMAT_WARNING],
            "299 - \"first\", 299 - \"second\""
        );
    }

    #[tokio::test]
    async fn complete_json_returns_the_object() {
        let (server, _) = structured_server(None).await;

        let object = gateway(server.url())
            .complete_json(MODEL, &user("Ada, 36"), person_schema(), &CompletionConfig::default())
            .await
            .unwrap();

        assert_eq!(object, json!({"name": "Ada", "age": 36}));
    }

    async fn complete_with_warning(format: ResponseFormat) -> HashMap<String, Value> {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", CHAT_PATH)
            .with_header("warning", "299 - \"grammar unavailable\"")
            .with_status(200)
            .with_body(CHAT_JSON_SCHEMA)
            .create_async()
            .await;
        let config = CompletionConfig {
            response_format: Some(format),
            ..Default::default()
        };

        gateway(server.url())
            .complete(MODEL, &user("Hi"), None, &config)
            .await
            .unwrap()
            .evidence
            .metadata
    }

    #[tokio::test]
    async fn a_configured_json_format_records_the_warning_on_complete() {
        let metadata = complete_with_warning(ResponseFormat::JsonObject { schema: None }).await;

        assert_eq!(metadata[RESPONSE_FORMAT_WARNING], "299 - \"grammar unavailable\"");
    }

    #[tokio::test]
    async fn a_configured_json_schema_format_records_the_warning_on_complete() {
        let metadata = complete_with_warning(ResponseFormat::JsonObject {
            schema: Some(person_schema()),
        })
        .await;

        assert_eq!(metadata[RESPONSE_FORMAT_WARNING], "299 - \"grammar unavailable\"");
    }

    #[tokio::test]
    async fn a_text_request_does_not_record_a_warning() {
        let metadata = complete_with_warning(ResponseFormat::Text).await;

        assert!(!metadata.contains_key(RESPONSE_FORMAT_WARNING));
    }

    #[tokio::test]
    async fn a_request_without_a_format_does_not_record_a_warning() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", CHAT_PATH)
            .with_header("warning", "299 - \"unrelated\"")
            .with_status(200)
            .with_body(CHAT_THINKING)
            .create_async()
            .await;

        let response = gateway(server.url())
            .complete(MODEL, &user("Hi"), None, &CompletionConfig::default())
            .await
            .unwrap();

        assert!(!response.evidence.metadata.contains_key(RESPONSE_FORMAT_WARNING));
    }
}

// ---------------------------------------------------------------------------
// 7. Events API
// ---------------------------------------------------------------------------

mod stream_events {
    use super::*;

    async fn events_for(body: &'static str) -> (Vec<StreamEvent>, Value) {
        let (server, captured) = chat_server(body).await;
        let gateway = gateway(server.url());
        let messages = user("Hi");
        let config = CompletionConfig::default();
        let events: Vec<_> = gateway
            .complete_stream_events(MODEL, &messages, &config)
            .expect("oMLX supports stream events")
            .collect()
            .await;
        (events, sent(&captured))
    }

    #[tokio::test]
    async fn requests_a_stream_with_usage_and_no_tools() {
        let (_, body) = events_for(STREAM_THINKING).await;

        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"], json!({"include_usage": true}));
        assert!(body.get("tools").is_none());
        assert!(body.get("max_completion_tokens").is_none());
    }

    #[tokio::test]
    async fn a_thinking_stream_completes_with_content_only() {
        let (events, _) = events_for(STREAM_THINKING).await;

        let [StreamEvent::Content(text), StreamEvent::Completed(evidence)] = events.as_slice()
        else {
            panic!("expected one content event then completed, got {events:?}");
        };
        assert_eq!(text, "\n\nhello");
        assert_eq!(evidence.provider_model.as_deref(), Some(MODEL));
        assert_eq!(evidence.finish_reason.as_deref(), Some("stop"));
        assert_eq!(evidence.usage.as_ref().unwrap()["generation_tokens_per_second"], 15.22);
        assert_eq!(evidence.metadata["created"], 1790679817);
    }

    #[tokio::test]
    async fn a_length_stream_is_an_incomplete_completion() {
        let (events, _) = events_for(STREAM_LENGTH).await;

        let [StreamEvent::Error(StreamEventError::IncompleteCompletion(evidence))] =
            events.as_slice()
        else {
            panic!("expected only an incomplete completion, got {events:?}");
        };
        assert_eq!(evidence.finish_reason.as_deref(), Some("length"));
        assert_eq!(evidence.provider_model.as_deref(), Some(MODEL));
        assert_eq!(evidence.usage.as_ref().unwrap()["completion_tokens"], 5);
    }

    #[tokio::test]
    async fn a_tool_call_stream_is_unexpected_tool_calls() {
        let (events, _) = events_for(STREAM_TOOL_CALL).await;

        assert!(
            matches!(
                events.as_slice(),
                [StreamEvent::Content(text), StreamEvent::Error(StreamEventError::UnexpectedToolCalls)]
                    if text == "\n\n"
            ),
            "{events:?}"
        );
    }

    #[tokio::test]
    async fn a_keepalive_only_stream_is_incomplete_with_no_provider_model() {
        let (events, _) = events_for(KEEPALIVE_FRAME).await;

        assert!(
            matches!(
                events.as_slice(),
                [StreamEvent::Error(StreamEventError::IncompleteStream(None))]
            ),
            "{events:?}"
        );
    }

    #[tokio::test]
    async fn an_error_status_is_a_provider_error_with_the_body() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", CHAT_PATH)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(ERROR_MODEL_NOT_FOUND)
            .create_async()
            .await;
        let gateway = gateway(server.url());
        let messages = user("Hi");
        let config = CompletionConfig::default();

        let events: Vec<_> = gateway
            .complete_stream_events("nope", &messages, &config)
            .expect("supported")
            .collect()
            .await;

        assert!(
            matches!(
                events.as_slice(),
                [StreamEvent::Error(StreamEventError::ProviderError { status: Some(404), error })]
                    if error["type"] == "not_found_error"
            ),
            "{events:?}"
        );
    }

    #[tokio::test]
    async fn the_broker_streams_events_through_the_gateway() {
        let (server, _) = chat_server(STREAM_THINKING).await;
        let broker = LlmBroker::new(MODEL, Arc::new(gateway(server.url())), None);
        let messages = user("Reply with exactly: hello");

        let events: Vec<_> = broker.generate_stream_events(&messages, None, None).collect().await;

        assert!(matches!(events.last(), Some(StreamEvent::Completed(_))), "{events:?}");
    }
}

// ---------------------------------------------------------------------------
// 8. Legacy streaming
// ---------------------------------------------------------------------------

mod legacy_streaming {
    use super::*;

    async fn chunks_for(body: &'static str) -> (Vec<StreamChunk>, Value) {
        let (server, captured) = chat_server(body).await;
        let gateway = gateway(server.url());
        let messages = user("Hi");
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(ResolveDateTool::default())];
        let config = CompletionConfig::default();
        let items: Vec<_> =
            gateway.complete_stream(MODEL, &messages, Some(&tools), &config).collect().await;
        let chunks = items.into_iter().map(|item| item.expect("no stream error")).collect();
        (chunks, sent(&captured))
    }

    fn thinking(chunks: &[StreamChunk]) -> String {
        chunks
            .iter()
            .filter_map(|chunk| match chunk {
                StreamChunk::Thinking(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn content(chunks: &[StreamChunk]) -> String {
        chunks
            .iter()
            .filter_map(|chunk| match chunk {
                StreamChunk::Content(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn sends_tools_and_a_stream_flag_without_usage_options() {
        let (_, body) = chunks_for(STREAM_THINKING).await;

        assert_eq!(body["stream"], true);
        assert_eq!(body["tools"][0]["function"]["name"], "resolve_date");
        assert!(body.get("stream_options").is_none());
    }

    #[tokio::test]
    async fn reasoning_deltas_become_thinking_chunks() {
        let (chunks, _) = chunks_for(STREAM_THINKING).await;

        assert_eq!(
            thinking(&chunks),
            "\nWe need to reply exactly: hello. User said \"Reply with exactly: hello\". Need final \"hello\". Ensure no extra.\n"
        );
        assert_eq!(content(&chunks), "\n\nhello");
    }

    #[tokio::test]
    async fn a_streamed_tool_call_yields_one_complete_tool_call() {
        let (chunks, _) = chunks_for(STREAM_TOOL_CALL).await;

        let tool_calls: Vec<_> = chunks
            .iter()
            .filter_map(|chunk| match chunk {
                StreamChunk::ToolCalls(calls) => Some(calls),
                _ => None,
            })
            .collect();
        let [calls] = tool_calls.as_slice() else {
            panic!("expected one tool-calls chunk, got {chunks:?}");
        };
        assert_eq!(content(&chunks), "\n\n");
        assert!(
            matches!(
                &chunks[chunks.len() - 2..],
                [StreamChunk::Content(_), StreamChunk::ToolCalls(_)]
            ),
            "content comes before the tool call: {chunks:?}"
        );
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id.as_deref(), Some("call_659d0e77"));
        assert_eq!(calls[0].name, "resolve_date");
        assert_eq!(calls[0].arguments["relative"], "today");
    }

    #[tokio::test]
    async fn keepalive_frames_are_dropped() {
        // A keep-alive frame never carries content; this one does, to prove
        // the gateway drops the whole frame.
        const BODY: &str = concat!(
            "data: {\"model\":\"keepalive\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ping\",\"reasoning_content\":\"ping\"}}]}\n\n",
            "data: {\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"}}]}\n\n",
            "data: [DONE]\n\n",
        );

        let (chunks, _) = chunks_for(BODY).await;

        assert!(
            matches!(chunks.as_slice(), [StreamChunk::Content(text)] if text == "hi"),
            "{chunks:?}"
        );
    }

    #[tokio::test]
    async fn an_error_status_is_a_gateway_error_with_the_body() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", CHAT_PATH)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(ERROR_MODEL_NOT_FOUND)
            .create_async()
            .await;
        let gateway = gateway(server.url());
        let messages = user("Hi");
        let config = CompletionConfig::default();

        let items: Vec<_> =
            gateway.complete_stream("nope", &messages, None, &config).collect().await;

        assert!(
            matches!(
                items.as_slice(),
                [Err(MojenticError::GatewayError(message))]
                    if message.contains("404") && message.contains("not_found_error")
            ),
            "{items:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 9. Models
// ---------------------------------------------------------------------------

mod models {
    use super::*;

    #[tokio::test]
    async fn lists_model_ids_sorted() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"object":"list","data":[{"id":"b-model"},{"id":"a-model"}]}"#)
            .create_async()
            .await;

        let models = gateway(server.url()).get_available_models().await.unwrap();

        assert_eq!(models, ["a-model", "b-model"]);
    }

    #[tokio::test]
    /// mockito is a real HTTP server on a local port, so this drives the
    /// gateway's real reqwest client through an `application/json` response,
    /// the way oMLX serves it.
    async fn lists_the_captured_models_from_a_real_json_response() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MODELS)
            .create_async()
            .await;

        let models = gateway(server.url()).get_available_models().await.unwrap();

        assert_eq!(models, [MODEL]);
    }

    #[tokio::test]
    async fn loads_and_unloads_a_model() {
        let mut server = mockito::Server::new_async().await;
        let load = server
            .mock("POST", format!("/v1/models/{MODEL}/load").as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MODEL_LOAD)
            .create_async()
            .await;
        let unload = server
            .mock("POST", format!("/v1/models/{MODEL}/unload").as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MODEL_UNLOAD)
            .create_async()
            .await;
        let gateway = gateway(server.url());

        gateway.load_model(MODEL).await.unwrap();
        gateway.unload_model(MODEL).await.unwrap();

        load.assert_async().await;
        unload.assert_async().await;
    }

    #[tokio::test]
    async fn encodes_the_model_id_as_one_path_segment() {
        let mut server = mockito::Server::new_async().await;
        let load = server
            .mock("POST", "/v1/models/my%20model%3Fv%3D2/load")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MODEL_LOAD)
            .create_async()
            .await;

        gateway(server.url()).load_model("my model?v=2").await.unwrap();

        load.assert_async().await;
    }

    #[tokio::test]
    async fn unloading_a_model_that_is_not_loaded_is_a_provider_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", format!("/v1/models/{MODEL}/unload").as_str())
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(ERROR_MODEL_NOT_LOADED)
            .create_async()
            .await;

        let error = gateway(server.url()).unload_model(MODEL).await.unwrap_err();

        let MojenticError::GatewayError(message) = error else {
            panic!("expected a gateway error, got {error:?}");
        };
        assert!(message.contains("400"), "{message}");
        assert!(message.contains(ERROR_MODEL_NOT_LOADED), "{message}");
    }
}

// ---------------------------------------------------------------------------
// 10. Embeddings
// ---------------------------------------------------------------------------

mod embeddings {
    use super::*;

    #[tokio::test]
    async fn sends_one_request_with_the_whole_text() {
        let long_text = "word ".repeat(20_000);
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/embeddings")
            .match_body(Matcher::Json(json!({"model": "embed-model", "input": long_text})))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"object":"list","data":[{"object":"embedding","index":0,"embedding":[0.25,-0.5,1.0]}],"model":"embed-model"}"#)
            .expect(1)
            .create_async()
            .await;

        let embedding = gateway(server.url())
            .calculate_embeddings(&long_text, Some("embed-model"))
            .await
            .unwrap();

        mock.assert_async().await;
        assert_eq!(embedding, [0.25, -0.5, 1.0]);
    }

    #[tokio::test]
    async fn a_missing_or_empty_model_is_rejected_before_any_request() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", Matcher::Any).expect(0).create_async().await;
        let gateway = gateway(server.url());

        for model in [None, Some(""), Some("  ")] {
            let error = gateway.calculate_embeddings("text", model).await.unwrap_err();
            assert!(matches!(error, MojenticError::InvalidArgument(_)), "{model:?}: {error:?}");
        }

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn a_chat_model_is_a_provider_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/v1/embeddings")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(ERROR_NOT_EMBEDDING_MODEL)
            .create_async()
            .await;

        let error = gateway(server.url())
            .calculate_embeddings("text", Some(MODEL))
            .await
            .unwrap_err();

        let MojenticError::GatewayError(message) = error else {
            panic!("expected a gateway error, got {error:?}");
        };
        assert!(message.contains("400"), "{message}");
        assert!(message.contains("is not an embedding model"), "{message}");
    }
}
