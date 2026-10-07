//! No model is loaded: exercise native history and controls at the HTTP boundary.
use async_trait::async_trait;
use futures::StreamExt;
use mojentic::llm::gateway::ReasoningEffort;
use mojentic::llm::gateways::{OllamaGateway, OmlxConfig, OmlxGateway};
use mojentic::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor, ToolRunCtx};
use mojentic::llm::{CompletionConfig, LlmBroker, LlmGateway, LlmMessage, ReasoningProfile};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};

#[derive(Clone)]
struct Inspect;
#[async_trait]
impl LlmTool for Inspect {
    async fn run(
        &self,
        _: &HashMap<String, Value>,
        _: &ToolRunCtx,
    ) -> mojentic::error::Result<Value> {
        Ok(json!({"receipt":"observed"}))
    }
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".into(),
            function: FunctionDescriptor {
                name: "inspect".into(),
                description: "Observe a fixture".into(),
                parameters: json!({"type":"object","properties":{}}),
            },
        }
    }
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

fn gateway(provider: &str, host: String) -> Arc<dyn LlmGateway> {
    if provider == "ollama" {
        Arc::new(OllamaGateway::with_host(host))
    } else {
        Arc::new(OmlxGateway::with_config(OmlxConfig {
            host,
            api_key: None,
            timeout: Some(Duration::from_secs(5)),
        }))
    }
}
fn path(provider: &str) -> &'static str {
    if provider == "ollama" {
        "/api/chat"
    } else {
        "/v1/chat/completions"
    }
}
fn reply(provider: &str, tools: bool, streaming: bool) -> String {
    let message = if tools {
        if provider == "ollama" {
            json!({"role":"assistant", "content":"Inspecting", "thinking":"plan-sentinel", "tool_calls":[{"id":"inspect-1","function":{"name":"inspect","arguments":{}}}]})
        } else {
            json!({"role":"assistant","content":"Inspecting", "reasoning_content":"plan-sentinel", "tool_calls":[{"index":0,"id":"inspect-1","type":"function","function":{"name":"inspect","arguments":"{}"}}]})
        }
    } else {
        json!({"role":"assistant","content":"DONE"})
    };
    if provider == "ollama" {
        if streaming {
            format!(
                "{}\n{}\n",
                json!({"message":message,"done":false}),
                json!({"message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"})
            )
        } else {
            format!("{}\n", json!({"message":message,"done":true,"done_reason":"stop"}))
        }
    } else if streaming {
        format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":message,"finish_reason":if tools {"tool_calls"} else {"stop"}}]})
        )
    } else {
        json!({"choices":[{"message":message,"finish_reason":if tools {"tool_calls"} else {"stop"}}]}).to_string()
    }
}

#[tokio::test]
async fn broker_retains_exact_reasoning_at_tool_boundaries_when_opted_in() {
    for provider in ["ollama", "omlx"] {
        for streaming in [false, true] {
            for preserve in [false, true] {
                let mut server = mockito::Server::new_async().await;
                let first = server
                    .mock("POST", path(provider))
                    .match_request(|r| {
                        serde_json::from_slice::<Value>(r.body().expect("body")).expect("json")
                            ["messages"]
                            .as_array()
                            .expect("messages")
                            .len()
                            == 1
                    })
                    .with_status(200)
                    .with_header(
                        "content-type",
                        if streaming && provider == "omlx" {
                            "text/event-stream"
                        } else {
                            "application/json"
                        },
                    )
                    .with_body(reply(provider, true, streaming))
                    .expect(1)
                    .create_async()
                    .await;
                let second = server
                    .mock("POST", path(provider))
                    .match_request(move |r| {
                        let v: Value =
                            serde_json::from_slice(r.body().expect("body")).expect("json");
                        let m = &v["messages"];
                        let field = if provider == "ollama" {
                            "thinking"
                        } else {
                            "reasoning_content"
                        };
                        m.as_array().expect("messages").len() == 3
                            && m[1]["content"] == "Inspecting"
                            && if preserve {
                                m[1][field] == "plan-sentinel"
                            } else {
                                m[1].get(field).is_none()
                            }
                            && m[2]["content"].as_str().is_some_and(|s| s.contains("observed"))
                            && (provider == "ollama" || m[2]["tool_call_id"] == "inspect-1")
                    })
                    .with_status(200)
                    .with_header(
                        "content-type",
                        if streaming && provider == "omlx" {
                            "text/event-stream"
                        } else {
                            "application/json"
                        },
                    )
                    .with_body(reply(provider, false, streaming))
                    .expect(1)
                    .create_async()
                    .await;
                let broker = LlmBroker::new("fixture-qwen", gateway(provider, server.url()), None);
                let cfg = CompletionConfig {
                    preserve_reasoning: preserve,
                    reasoning_profile: Some(ReasoningProfile::qwen3_8()),
                    reasoning_effort: Some(ReasoningEffort::Medium),
                    ..Default::default()
                };
                let messages = [LlmMessage::user("Inspect then finish")];
                let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(Inspect)];
                if streaming {
                    let mut stream =
                        broker.generate_stream(&messages, Some(&tools), Some(cfg), None);
                    let mut text = String::new();
                    while let Some(chunk) = stream.next().await {
                        text += &chunk.expect("stream");
                    }
                    assert_eq!(text, "InspectingDONE");
                } else {
                    assert_eq!(
                        broker
                            .generate(&messages, Some(&tools), Some(cfg), None)
                            .await
                            .expect("response"),
                        "DONE"
                    );
                }
                first.assert_async().await;
                second.assert_async().await;
            }
        }
    }
}

#[tokio::test]
async fn effort_is_faithful_for_ordinary_streaming_and_structured_requests() {
    for provider in ["ollama", "omlx"] {
        for effort in [
            ReasoningEffort::Disabled,
            ReasoningEffort::Enabled,
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
        ] {
            for mode in ["ordinary", "stream", "structured"] {
                let mut server = mockito::Server::new_async().await;
                let expected = match effort {
                    ReasoningEffort::Disabled => json!(false),
                    ReasoningEffort::Enabled => json!(true),
                    named => json!(named),
                };
                let field = if provider == "ollama" {
                    "think"
                } else if expected.is_boolean() {
                    "enable_thinking"
                } else {
                    "reasoning_effort"
                };
                let mut response = reply(provider, false, mode == "stream");
                if mode == "structured" {
                    response = if provider == "ollama" {
                        json!({"message":{"content":"{}"}}).to_string()
                    } else {
                        json!({"choices":[{"message":{"content":"{}"},"finish_reason":"stop"}]})
                            .to_string()
                    };
                }
                let mock = server
                    .mock("POST", path(provider))
                    .match_request(move |r| {
                        serde_json::from_slice::<Value>(r.body().expect("body")).expect("json")
                            [field]
                            == expected
                    })
                    .with_status(200)
                    .with_body(response)
                    .expect(1)
                    .create_async()
                    .await;
                let gateway = gateway(provider, server.url());
                let cfg = CompletionConfig {
                    reasoning_effort: Some(effort),
                    ..Default::default()
                };
                let messages = [LlmMessage::user("finish")];
                if mode == "stream" {
                    let mut stream = gateway.complete_stream("fixture-qwen", &messages, None, &cfg);
                    while let Some(chunk) = stream.next().await {
                        chunk.expect("chunk");
                    }
                } else if mode == "structured" {
                    gateway
                        .complete_json("fixture-qwen", &messages, json!({"type":"object"}), &cfg)
                        .await
                        .expect("json reply");
                } else {
                    gateway.complete("fixture-qwen", &messages, None, &cfg).await.expect("reply");
                }
                mock.assert_async().await;
            }
        }
    }
}

#[tokio::test]
async fn qwen_profile_rejects_high_before_http() {
    for provider in ["ollama", "omlx"] {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", path(provider)).expect(0).create_async().await;
        let gateway = gateway(provider, server.url());
        let cfg = CompletionConfig {
            reasoning_effort: Some(ReasoningEffort::High),
            reasoning_profile: Some(ReasoningProfile::qwen3_8()),
            ..Default::default()
        };
        let messages = [LlmMessage::user("finish")];
        let error = gateway
            .complete("fixture-qwen", &messages, None, &cfg)
            .await
            .expect_err("invalid effort");
        assert!(error.to_string().contains("unsupported by profile qwen3.8"));
        let mut stream = gateway.complete_stream("fixture-qwen", &messages, None, &cfg);
        assert!(stream.next().await.expect("error chunk").is_err());
        assert!(gateway.complete_json("fixture-qwen", &messages, json!({}), &cfg).await.is_err());
        mock.assert_async().await;
    }
}
