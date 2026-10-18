//! Executable request-fidelity-v1 through production HTTP adapters.
#[path = "support/mod.rs"]
pub mod boundary;
use mojentic::llm::{
    gateway::{CompletionConfig, LlmGateway},
    gateways::OllamaGateway,
    LlmMessage,
};
use serde_json::{json, Value};

fn contract() -> Value {
    serde_json::from_str(include_str!("contracts/request_fidelity_v1.json"))
        .expect("versioned contract")
}

#[tokio::test]
async fn unset_controls_cross_http_boundary() {
    let artifact = contract();
    let mut server = boundary::Fixture::new_async().await;
    let request = server
        .mock("POST", "/api/chat")
        .match_body(boundary::Matcher::Json(json!({
            "model":"contract-model", "messages":[{"role":"user","content":"question"}],
            "options":{}, "stream":false
        })))
        .with_body(artifact["responses"]["ollama-complete"].as_str().expect("contract response"))
        .create_async()
        .await;
    let response = OllamaGateway::with_host(server.url())
        .with_transport(server.transport())
        .complete(
            "contract-model",
            &[LlmMessage::user("question")],
            None,
            &CompletionConfig::default(),
        )
        .await
        .expect("unset request must match the HTTP contract");
    assert_eq!(response.content.as_deref(), Some("answer"));
    request.assert_async().await;
}

use async_trait::async_trait;
use futures::StreamExt;
use mojentic::{
    error::Result,
    llm::{
        gateway::ReasoningEffort,
        gateways::{OmlxConfig, OmlxGateway, OpenAIGateway},
        recovery::*,
        stream_events::StreamEvent,
        tools::{FunctionDescriptor, LlmTool, ToolDescriptor, ToolRunCtx},
        ChatSession, LlmBroker,
    },
    tracer::{EventStore, LlmCallTracerEvent, TracerSystem},
};
use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, Debug)]
enum ProviderKind {
    Ollama,
    Omlx,
    OpenAi,
}
const PROVIDERS: [ProviderKind; 3] = [
    ProviderKind::Ollama,
    ProviderKind::Omlx,
    ProviderKind::OpenAi,
];
impl ProviderKind {
    fn path(self) -> &'static str {
        match self {
            Self::Ollama => "/api/chat",
            Self::Omlx => "/v1/chat/completions",
            Self::OpenAi => "/chat/completions",
        }
    }
    fn response(self, entry: &str) -> String {
        let family = if matches!(self, Self::Ollama) {
            "ollama"
        } else {
            "openai"
        };
        contract()["responses"][format!("{family}-{entry}")]
            .as_str()
            .expect("artifact response")
            .into()
    }
    fn gateway(self, url: &str) -> Arc<dyn LlmGateway> {
        match self {
            Self::Ollama => Arc::new({
                let transport = boundary::transport_for(url);
                OllamaGateway::with_host(url).with_transport(transport)
            }),
            Self::Omlx => Arc::new({
                let transport = boundary::transport_for(url);
                OmlxGateway::with_config(OmlxConfig {
                    host: url.into(),
                    api_key: None,
                    ..Default::default()
                })
                .with_transport(transport)
            }),
            Self::OpenAi => Arc::new({
                let transport = boundary::transport_for(url);
                OpenAIGateway::with_api_key_and_base_url("contract-key", url)
                    .with_transport(transport)
            }),
        }
    }
}

// Read caller intent directly from the shared executable cases, including JSON null.
fn config(case: &Value) -> CompletionConfig {
    let c = &case["config"];
    CompletionConfig {
        temperature: c["temperature"].as_f64().map(|n| n as f32),
        num_ctx: c["num_ctx"].as_u64().map(|n| n as usize),
        max_tokens: c["max_tokens"].as_u64().map(|n| n as usize),
        top_p: c["top_p"].as_f64().map(|n| n as f32),
        top_k: c["top_k"].as_u64().map(|n| n as u32),
        ..Default::default()
    }
}
fn cases() -> Vec<Value> {
    let mut cases = contract()["cases"].as_array().expect("contract cases").clone();
    // Supported migration from 2.1's implicit defaults to explicit 3.0 controls.
    cases.push(json!({"name":"explicit-former-defaults", "config":{
        "temperature":1.0, "num_ctx":32768, "max_tokens":16384
    }}));
    cases
}
fn assert_controls(provider: ProviderKind, body: &Value, case: &Value, reasoning: bool) {
    let mut expected = serde_json::Map::new();
    let c = &case["config"];
    for name in ["temperature", "num_ctx", "max_tokens", "top_p", "top_k"] {
        if c[name].is_null() {
            continue;
        }
        if !matches!(provider, ProviderKind::Ollama) && name == "num_ctx" {
            continue;
        }
        if matches!(provider, ProviderKind::OpenAi) && name == "top_k" {
            continue;
        }
        let wire = match (provider, name, reasoning) {
            (ProviderKind::Ollama, "max_tokens", _) => "num_predict",
            (ProviderKind::OpenAi, "max_tokens", true) => "max_completion_tokens",
            _ => name,
        };
        expected.insert(wire.into(), c[name].clone());
    }
    let source = if matches!(provider, ProviderKind::Ollama) {
        &body["options"]
    } else {
        body
    };
    let actual: serde_json::Map<String, Value> = source
        .as_object()
        .expect("request object")
        .iter()
        .filter(|(key, _)| {
            [
                "temperature",
                "num_ctx",
                "max_tokens",
                "num_predict",
                "max_completion_tokens",
                "top_p",
                "top_k",
            ]
            .contains(&key.as_str())
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (key, value) in &expected {
        if let Some(number) = value.as_f64() {
            assert!(
                (actual[key].as_f64().expect("numeric control") - number).abs() < 0.000001,
                "{provider:?} {}: {key}: {body}",
                case["name"]
            );
        }
    }
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "{provider:?} {}: {body}",
        case["name"]
    );
    if matches!(provider, ProviderKind::Ollama) {
        assert_eq!(source.as_object().expect("options").len(), expected.len());
        for key in [
            "temperature",
            "num_ctx",
            "num_predict",
            "max_tokens",
            "top_p",
            "top_k",
        ] {
            assert!(body.get(key).is_none(), "misplaced {key}: {body}");
        }
    } else {
        assert!(body.get("options").is_none());
    }
    for key in [
        "max_tool_iterations",
        "reasoning_profile",
        "preserve_reasoning",
        "recovery",
    ] {
        assert!(body.get(key).is_none(), "client control leaked: {body}");
    }
}

type Bodies = Arc<Mutex<Vec<Vec<u8>>>>;
async fn stub(
    server: &mut boundary::FixtureGuard,
    provider: ProviderKind,
    status: usize,
    response: String,
    bodies: &Bodies,
) -> boundary::Mock {
    let sink = bodies.clone();
    server
        .mock("POST", provider.path())
        .with_status(status)
        .with_body_from_request(move |request| {
            sink.lock()
                .expect("captured bodies")
                .push(request.body().expect("HTTP body").to_vec());
            response.as_bytes().to_vec()
        })
        .expect(1)
        .create_async()
        .await
}
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct Answer {
    answer: String,
}
type Temperatures = Arc<Mutex<Vec<Option<f64>>>>;
fn tracer() -> (Arc<TracerSystem>, Temperatures) {
    let temperatures = Arc::new(Mutex::new(Vec::new()));
    let capture = temperatures.clone();
    let store = Arc::new(EventStore::new(Some(Arc::new(move |event| {
        if let Some(call) = event.as_any().and_then(|e| e.downcast_ref::<LlmCallTracerEvent>()) {
            capture.lock().expect("trace temperatures").push(call.temperature);
            let wire = serde_json::to_value(call).expect("trace JSON");
            assert_eq!(wire.get("temperature").is_some(), call.temperature.is_some());
        }
    }))));
    (Arc::new(TracerSystem::new(Some(store), true)), temperatures)
}

#[tokio::test]
async fn artifact_cases_through_broker_all_http_entrypoints() {
    assert_eq!(contract()["version"], "request-fidelity-v1");
    for provider in PROVIDERS {
        for case in cases() {
            for entry in ["complete", "object", "legacy", "events"] {
                let mut server = boundary::Fixture::new_async().await;
                let bodies = Bodies::default();
                let request =
                    stub(&mut server, provider, 200, provider.response(entry), &bodies).await;
                let (trace, temperatures) = tracer();
                let broker = LlmBroker::new("gpt-4o", provider.gateway(&server.url()), Some(trace));
                let messages = [LlmMessage::user("question")];
                let cfg = config(&case);
                match entry {
                    "complete" => {
                        let response = broker
                            .generate_response(&messages, None, Some(cfg.clone()), None)
                            .await
                            .expect("complete");
                        assert_eq!(response.content.as_deref(), Some("answer"));
                        // The artifact omits model and usage; do not manufacture evidence.
                        assert!(response.evidence.usage.is_none());
                        assert!(response.evidence.provider_model.is_none());
                    }
                    "object" => {
                        let response: Answer = broker
                            .generate_object(&messages, Some(cfg.clone()), None)
                            .await
                            .expect("structured");
                        assert_eq!(response.answer, "answer");
                    }
                    "legacy" => {
                        let chunks: Vec<_> = broker
                            .generate_stream(&messages, None, Some(cfg.clone()), None)
                            .collect()
                            .await;
                        assert_eq!(
                            chunks
                                .into_iter()
                                .collect::<Result<Vec<_>>>()
                                .expect("stream")
                                .concat(),
                            "answer"
                        );
                    }
                    "events" => {
                        let events: Vec<_> = broker
                            .generate_stream_events(&messages, Some(cfg.clone()), None)
                            .collect()
                            .await;
                        assert!(
                            matches!(events.last(), Some(StreamEvent::Completed(_))),
                            "{provider:?}: {events:?}"
                        );
                        assert_eq!(
                            events
                                .iter()
                                .filter_map(|e| if let StreamEvent::Content(s) = e {
                                    Some(s.as_str())
                                } else {
                                    None
                                })
                                .collect::<String>(),
                            "answer"
                        );
                    }
                    _ => unreachable!(),
                }
                request.assert_async().await;
                let body: Value = serde_json::from_slice(&bodies.lock().expect("bodies")[0])
                    .expect("JSON request");
                assert_controls(provider, &body, &case, false);
                assert_eq!(
                    *temperatures.lock().expect("temperatures"),
                    vec![cfg.temperature.map(f64::from)],
                    "{entry}"
                );
            }
        }
    }
}

#[tokio::test]
async fn artifact_cases_through_session_send_and_stream() {
    for provider in PROVIDERS {
        for case in cases() {
            for stream in [false, true] {
                let mut server = boundary::Fixture::new_async().await;
                let bodies = Bodies::default();
                let request = stub(
                    &mut server,
                    provider,
                    200,
                    provider.response(if stream { "legacy" } else { "complete" }),
                    &bodies,
                )
                .await;
                let broker = LlmBroker::new("gpt-4o", provider.gateway(&server.url()), None);
                let mut session = ChatSession::builder(broker)
                    .max_context(2000)
                    .completion_config(config(&case))
                    .build();
                let answer = if stream {
                    session
                        .send_stream("question")
                        .collect::<Vec<_>>()
                        .await
                        .into_iter()
                        .collect::<Result<Vec<_>>>()
                        .expect("session stream")
                        .concat()
                } else {
                    session.send("question").await.expect("session send")
                };
                assert_eq!(answer, "answer");
                request.assert_async().await;
                let body: Value = serde_json::from_slice(&bodies.lock().expect("bodies")[0])
                    .expect("JSON request");
                assert_controls(provider, &body, &case, false);
            }
        }
    }
}

#[derive(Default)]
struct Attempts(Mutex<Vec<(Identity, Vec<u8>)>>);
impl WireObserver for Attempts {
    fn observe(
        &self,
        event: WireEvent<'_>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let WireEvent::Request { identity, request } = event {
            self.0.lock().expect("attempts").push((
                identity,
                request.body().and_then(|b| b.as_bytes()).expect("encoded request").to_vec(),
            ));
        }
        Ok(())
    }
}
struct Allow;
#[async_trait]
impl Admission for Allow {
    async fn decide(&self, _: AdmissionContext) -> AdmissionDecision {
        AdmissionDecision::Allow
    }
}
async fn recovery_cases<G: RecoveryProvider>(
    inner: G,
    provider: ProviderKind,
    server: &mut boundary::FixtureGuard,
    case: &Value,
    entry: &str,
) {
    let bodies = Bodies::default();
    let failure = contract()["retryable_error"].clone();
    let reject = stub(
        server,
        provider,
        failure["status"].as_u64().expect("status") as usize,
        failure["body"].as_str().expect("body").into(),
        &bodies,
    )
    .await;
    let accept = stub(server, provider, 200, provider.response(entry), &bodies).await;
    let attempts = Arc::new(Attempts::default());
    let gateway = RecoveryGateway::new(
        inner,
        RecoveryPolicy {
            max_attempts: NonZeroU32::new(2).expect("two attempts"),
            base_delay: Duration::ZERO,
            admission: Some(Arc::new(Allow)),
            wire_observer: Some(attempts.clone()),
            ..Default::default()
        },
    )
    .expect("recovery gateway");
    let messages = [LlmMessage::user("question")];
    let cfg = config(case);
    let call = RecoveryCall::default();
    match entry {
        "complete" => {
            let result = gateway
                .complete_with_recovery("gpt-4o", &messages, None, &cfg, &call)
                .await
                .expect("recovered complete");
            assert_eq!(result.value.content.as_deref(), Some("answer"));
            assert_eq!(result.report.identity.wire_attempt, 2);
        }
        "object" => {
            let result = gateway
                .complete_json_with_recovery(
                    "gpt-4o",
                    &messages,
                    json!({"type":"object"}),
                    &cfg,
                    &call,
                )
                .await
                .expect("recovered object");
            assert_eq!(result.value.object.expect("object")["answer"], "answer");
        }
        "events" => {
            let events: Vec<_> = gateway
                .complete_stream_with_recovery("gpt-4o", &messages, None, &cfg, &call)
                .collect()
                .await;
            assert_eq!(
                events
                    .iter()
                    .filter_map(|e| if let RecoveryStreamEvent::Content(s) = e {
                        Some(s.as_str())
                    } else {
                        None
                    })
                    .collect::<String>(),
                "answer"
            );
            let Some(RecoveryStreamEvent::Completed(result)) = events.last() else {
                panic!("recovery must terminate successfully")
            };
            assert_eq!(result.report.identity.wire_attempt, 2);
            assert_eq!(result.value.content.as_deref(), Some("answer"));
        }
        _ => unreachable!(),
    }
    reject.assert_async().await;
    accept.assert_async().await;
    assert_replay(&bodies, &attempts, provider, case);
}
fn assert_replay(bodies: &Bodies, attempts: &Attempts, provider: ProviderKind, case: &Value) {
    let sent = bodies.lock().expect("actual HTTP bodies");
    let observed = attempts.0.lock().expect("attempt identities");
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], sent[1], "byte-identical HTTP replay");
    assert_eq!(observed.len(), 2);
    for i in 0..2 {
        assert_eq!(sent[i], observed[i].1);
    }
    assert_eq!(observed[0].0.logical_request_id, observed[1].0.logical_request_id);
    assert_ne!(observed[0].0.attempt_id, observed[1].0.attempt_id);
    assert_eq!(observed[0].0.wire_attempt, 1);
    assert_eq!(observed[1].0.wire_attempt, 2);
    assert_controls(provider, &serde_json::from_slice(&sent[0]).expect("JSON"), case, false);
}
#[tokio::test]
async fn artifact_cases_recovery_replays_bytes_and_real_attempt_identities() {
    for provider in PROVIDERS {
        for case in cases() {
            for entry in ["complete", "object", "events"] {
                let mut server = boundary::Fixture::new_async().await;
                match provider {
                    ProviderKind::Ollama => {
                        recovery_cases(
                            OllamaGateway::with_host(server.url())
                                .with_transport(server.transport()),
                            provider,
                            &mut server,
                            &case,
                            entry,
                        )
                        .await
                    }
                    ProviderKind::Omlx => {
                        recovery_cases(
                            OmlxGateway::with_config(OmlxConfig {
                                host: server.url(),
                                api_key: None,
                                ..Default::default()
                            })
                            .with_transport(server.transport()),
                            provider,
                            &mut server,
                            &case,
                            entry,
                        )
                        .await
                    }
                    ProviderKind::OpenAi => {
                        recovery_cases(
                            OpenAIGateway::with_api_key_and_base_url("contract-key", server.url())
                                .with_transport(server.transport()),
                            provider,
                            &mut server,
                            &case,
                            entry,
                        )
                        .await
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct ContractTool(Arc<Mutex<Vec<Value>>>);
#[async_trait]
impl LlmTool for ContractTool {
    async fn run(&self, args: &HashMap<String, Value>, _: &ToolRunCtx) -> Result<Value> {
        let expected: Value = serde_json::from_str(
            contract()["tool_call"]["function"]["arguments"].as_str().expect("arguments"),
        )
        .expect("contract arguments");
        assert_eq!(serde_json::to_value(args).expect("tool arguments"), expected);
        self.0.lock().expect("tool runs").push(expected.clone());
        Ok(expected)
    }
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".into(),
            function: FunctionDescriptor {
                name: contract()["tool_call"]["function"]["name"]
                    .as_str()
                    .expect("tool name")
                    .into(),
                description: "Echo the contract argument".into(),
                parameters: json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}),
            },
        }
    }
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}
fn tool_response(provider: ProviderKind, stream: bool) -> String {
    let mut call = contract()["tool_call"].clone();
    if matches!(provider, ProviderKind::Ollama) {
        call["function"]["arguments"] =
            serde_json::from_str(call["function"]["arguments"].as_str().expect("arguments"))
                .expect("argument object");
    }
    if !stream {
        let mut response: Value =
            serde_json::from_str(&provider.response("complete")).expect("complete template");
        let message = if matches!(provider, ProviderKind::Ollama) {
            &mut response["message"]
        } else {
            &mut response["choices"][0]["message"]
        };
        message["content"] = json!("");
        message["tool_calls"] = json!([call]);
        response.to_string()
    } else if matches!(provider, ProviderKind::Ollama) {
        let template = provider.response("legacy");
        let mut lines = template.lines();
        let mut frame: Value =
            serde_json::from_str(lines.next().expect("first frame")).expect("frame");
        frame["message"]["content"] = json!("");
        frame["message"]["tool_calls"] = json!([call]);
        format!("{frame}\n{}\n", lines.collect::<Vec<_>>().join("\n"))
    } else {
        call["index"] = json!(0);
        let template = provider.response("legacy");
        let mut frames = template.splitn(2, "\n\n");
        let mut frame: Value =
            serde_json::from_str(frames.next().expect("first frame").trim_start_matches("data: "))
                .expect("frame");
        frame["choices"][0]["delta"] = json!({"tool_calls":[call]});
        format!("data: {frame}\n\n{}", frames.next().expect("remaining frames"))
    }
}
#[tokio::test]
async fn artifact_tool_continuations_preserve_session_controls_and_history() {
    for provider in PROVIDERS {
        for case in cases() {
            for stream in [false, true] {
                let mut server = boundary::Fixture::new_async().await;
                let bodies = Bodies::default();
                let first =
                    stub(&mut server, provider, 200, tool_response(provider, stream), &bodies)
                        .await;
                let second = stub(
                    &mut server,
                    provider,
                    200,
                    provider.response(if stream { "legacy" } else { "complete" }),
                    &bodies,
                )
                .await;
                let runs = Arc::new(Mutex::new(Vec::new()));
                let broker = LlmBroker::new("gpt-4o", provider.gateway(&server.url()), None);
                let mut session = ChatSession::builder(broker)
                    .completion_config(config(&case))
                    .tools(vec![Box::new(ContractTool(runs.clone()))])
                    .build();
                let answer = if stream {
                    session
                        .send_stream("question")
                        .collect::<Vec<_>>()
                        .await
                        .into_iter()
                        .collect::<Result<Vec<_>>>()
                        .expect("tool stream")
                        .concat()
                } else {
                    session.send("question").await.expect("tool send")
                };
                assert_eq!(answer, "answer");
                first.assert_async().await;
                second.assert_async().await;
                assert_eq!(
                    runs.lock().expect("tool runs").as_slice(),
                    &[json!({"value":"next-π"})]
                );
                let sent = bodies.lock().expect("bodies");
                assert_eq!(sent.len(), 2);
                let first: Value = serde_json::from_slice(&sent[0]).expect("first request");
                let second: Value = serde_json::from_slice(&sent[1]).expect("continuation request");
                assert_controls(provider, &first, &case, false);
                assert_controls(provider, &second, &case, false);
                assert_eq!(first["tools"], second["tools"]);
                let history = second["messages"].as_array().expect("history");
                let assistant =
                    history.iter().find(|m| m["role"] == "assistant").expect("assistant call");
                assert_eq!(
                    assistant["tool_calls"][0]["function"]["name"],
                    contract()["tool_call"]["function"]["name"]
                );
                let tool = history.iter().find(|m| m["role"] == "tool").expect("tool result");
                assert!(tool["content"].as_str().expect("tool result text").contains("next-π"));
                if !matches!(provider, ProviderKind::Ollama) {
                    assert_eq!(tool["tool_call_id"], contract()["tool_call"]["id"]);
                }
            }
        }
    }
}

#[tokio::test]
async fn openai_model_restrictions_preserve_selected_output_limits() {
    for (model, temperature, expected_temperature, effort, expected_effort) in [
        ("o3", None, None, None, None),
        ("o3", Some(0.0), Some(1.0), Some(ReasoningEffort::XHigh), Some("xhigh")),
        ("gpt-4o-search-preview", Some(0.0), None, None, None),
        ("gpt-4o", Some(0.0), Some(0.0), Some(ReasoningEffort::High), None),
    ] {
        let mut server = boundary::Fixture::new_async().await;
        let bodies = Bodies::default();
        let request = stub(
            &mut server,
            ProviderKind::OpenAi,
            200,
            ProviderKind::OpenAi.response("complete"),
            &bodies,
        )
        .await;
        let config = CompletionConfig {
            temperature,
            max_tokens: Some(73),
            reasoning_effort: effort,
            ..Default::default()
        };
        let broker = LlmBroker::new(model, ProviderKind::OpenAi.gateway(&server.url()), None);
        broker
            .generate_response(&[LlmMessage::user("question")], None, Some(config), None)
            .await
            .expect("restricted model");
        request.assert_async().await;
        let body: Value =
            serde_json::from_slice(&bodies.lock().expect("bodies")[0]).expect("request");
        assert_eq!(body.get("temperature").and_then(Value::as_f64), expected_temperature);
        assert_eq!(body.get("reasoning_effort").and_then(Value::as_str), expected_effort);
        let (selected, absent) = if model == "o3" {
            ("max_completion_tokens", "max_tokens")
        } else {
            ("max_tokens", "max_completion_tokens")
        };
        assert_eq!(body[selected], 73);
        assert!(body.get(absent).is_none());
    }
}

#[tokio::test]
async fn native_reasoning_mappings_keep_request_fidelity() {
    for provider in [ProviderKind::Ollama, ProviderKind::Omlx] {
        for effort in [
            ReasoningEffort::Disabled,
            ReasoningEffort::Enabled,
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
        ] {
            let case = contract()["cases"]
                .as_array()
                .expect("cases")
                .iter()
                .find(|c| c["name"] == "all")
                .expect("combined settings")
                .clone();
            let mut cfg = config(&case);
            cfg.reasoning_effort = Some(effort);
            let mut server = boundary::Fixture::new_async().await;
            let bodies = Bodies::default();
            let request =
                stub(&mut server, provider, 200, provider.response("complete"), &bodies).await;
            provider
                .gateway(&server.url())
                .complete("contract-model", &[LlmMessage::user("question")], None, &cfg)
                .await
                .expect("native reasoning");
            request.assert_async().await;
            let body: Value =
                serde_json::from_slice(&bodies.lock().expect("bodies")[0]).expect("request");
            assert_controls(provider, &body, &case, false);
            let expected = match effort {
                ReasoningEffort::Disabled => json!(false),
                ReasoningEffort::Enabled => json!(true),
                named => serde_json::to_value(named).expect("named effort"),
            };
            let field = if matches!(provider, ProviderKind::Ollama) {
                "think"
            } else if matches!(effort, ReasoningEffort::Disabled | ReasoningEffort::Enabled) {
                "enable_thinking"
            } else {
                "reasoning_effort"
            };
            assert_eq!(body[field], expected);
        }
    }
}

#[tokio::test]
async fn artifact_error_and_optional_response_fields_through_broker() {
    for provider in PROVIDERS {
        let case = contract()["cases"][0].clone();
        for status in [
            400,
            contract()["retryable_error"]["status"].as_u64().expect("status") as usize,
        ] {
            let mut server = boundary::Fixture::new_async().await;
            let bodies = Bodies::default();
            let request = stub(
                &mut server,
                provider,
                status,
                contract()["retryable_error"]["body"].as_str().expect("error body").into(),
                &bodies,
            )
            .await;
            let broker = LlmBroker::new("gpt-4o", provider.gateway(&server.url()), None);
            let error = broker
                .generate_response(&[LlmMessage::user("question")], None, Some(config(&case)), None)
                .await
                .expect_err("HTTP error");
            assert!(error.to_string().contains(&status.to_string()));
            request.assert_async().await;
            let sent = bodies.lock().expect("bodies");
            assert_eq!(sent.len(), 1, "ordinary request must not gain automatic recovery");
            assert_controls(
                provider,
                &serde_json::from_slice(&sent[0]).expect("request"),
                &case,
                false,
            );
        }
        // Derive an older minimal response from v1 by removing optional finish evidence.
        let mut response: Value =
            serde_json::from_str(&provider.response("complete")).expect("artifact response");
        if matches!(provider, ProviderKind::Ollama) {
            response.as_object_mut().expect("response").remove("done_reason");
        } else {
            response["choices"][0].as_object_mut().expect("choice").remove("finish_reason");
        }
        let mut server = boundary::Fixture::new_async().await;
        let bodies = Bodies::default();
        let request = stub(&mut server, provider, 200, response.to_string(), &bodies).await;
        let broker = LlmBroker::new("gpt-4o", provider.gateway(&server.url()), None);
        let result = broker
            .generate_response(&[LlmMessage::user("question")], None, None, None)
            .await
            .expect("optional evidence");
        assert_eq!(result.content.as_deref(), Some("answer"));
        assert!(result.evidence.finish_reason.is_none());
        request.assert_async().await;
    }
}

#[tokio::test]
async fn session_default_and_temperature_builder_preserve_absence_and_zero() {
    for temperature in [None, Some(0.0)] {
        let mut server = boundary::Fixture::new_async().await;
        let bodies = Bodies::default();
        let request = stub(
            &mut server,
            ProviderKind::Ollama,
            200,
            ProviderKind::Ollama.response("complete"),
            &bodies,
        )
        .await;
        let broker = LlmBroker::new("gpt-4o", ProviderKind::Ollama.gateway(&server.url()), None);
        let mut session = match temperature {
            None => ChatSession::new(broker),
            Some(value) => ChatSession::builder(broker).temperature(value).build(),
        };
        assert_eq!(session.send("question").await.expect("session"), "answer");
        request.assert_async().await;
        let body: Value =
            serde_json::from_slice(&bodies.lock().expect("bodies")[0]).expect("request");
        let case = json!({"name":"session builder", "config":{"temperature":temperature}});
        assert_controls(ProviderKind::Ollama, &body, &case, false);
    }
}

#[derive(Clone, Default)]
struct WarningLog(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for WarningLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("warning log").extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[tokio::test]
async fn dropped_openai_reasoning_warns_at_http_boundary() {
    use tracing::instrument::WithSubscriber;
    for effort in [ReasoningEffort::High, ReasoningEffort::Disabled] {
        let log = WarningLog::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || writer.clone())
            .finish();
        async {
            let mut server = boundary::Fixture::new_async().await;
            let bodies = Bodies::default();
            let request = stub(
                &mut server,
                ProviderKind::OpenAi,
                200,
                ProviderKind::OpenAi.response("complete"),
                &bodies,
            )
            .await;
            let cfg = CompletionConfig {
                reasoning_effort: Some(effort),
                ..Default::default()
            };
            ProviderKind::OpenAi
                .gateway(&server.url())
                .complete("gpt-4o", &[LlmMessage::user("question")], None, &cfg)
                .await
                .expect("dropped reasoning");
            request.assert_async().await;
            let body: Value =
                serde_json::from_slice(&bodies.lock().expect("bodies")[0]).expect("request");
            assert!(body.get("reasoning_effort").is_none());
        }
        .with_subscriber(subscriber)
        .await;
        assert!(String::from_utf8(log.0.lock().expect("warning log").clone())
            .expect("UTF-8 log")
            .contains("reasoning_effort specified but model is not a reasoning model"));
    }
}

#[tokio::test]
async fn artifact_cases_recovery_through_broker_and_session() {
    for provider in PROVIDERS {
        for case in cases() {
            for entry in ["complete", "object", "legacy"] {
                let mut server = boundary::Fixture::new_async().await;
                let bodies = Bodies::default();
                let failure = contract()["retryable_error"].clone();
                let rejected = stub(
                    &mut server,
                    provider,
                    failure["status"].as_u64().expect("status") as usize,
                    failure["body"].as_str().expect("error body").into(),
                    &bodies,
                )
                .await;
                let accepted =
                    stub(&mut server, provider, 200, provider.response(entry), &bodies).await;
                let attempts = Arc::new(Attempts::default());
                let policy = RecoveryPolicy {
                    max_attempts: NonZeroU32::new(2).expect("attempts"),
                    base_delay: Duration::ZERO,
                    admission: Some(Arc::new(Allow)),
                    wire_observer: Some(attempts.clone()),
                    ..Default::default()
                };
                let gateway: Arc<dyn LlmGateway> = match provider {
                    ProviderKind::Ollama => Arc::new(
                        OllamaGateway::with_host(server.url())
                            .with_transport(server.transport())
                            .with_recovery(policy)
                            .expect("gateway"),
                    ),
                    ProviderKind::Omlx => Arc::new(
                        OmlxGateway::with_config(OmlxConfig {
                            host: server.url(),
                            api_key: None,
                            ..Default::default()
                        })
                        .with_transport(server.transport())
                        .with_recovery(policy)
                        .expect("gateway"),
                    ),
                    ProviderKind::OpenAi => Arc::new(
                        OpenAIGateway::with_api_key_and_base_url("contract-key", server.url())
                            .with_transport(server.transport())
                            .with_recovery(policy)
                            .expect("gateway"),
                    ),
                };
                let broker = LlmBroker::new("gpt-4o", gateway, None);
                if entry == "object" {
                    let answer: Answer = broker
                        .generate_object(&[LlmMessage::user("question")], Some(config(&case)), None)
                        .await
                        .expect("recovered broker object");
                    assert_eq!(answer.answer, "answer");
                } else {
                    let mut session =
                        ChatSession::builder(broker).completion_config(config(&case)).build();
                    let answer = if entry == "legacy" {
                        session
                            .send_stream("question")
                            .collect::<Vec<_>>()
                            .await
                            .into_iter()
                            .collect::<Result<Vec<_>>>()
                            .expect("recovered session stream")
                            .concat()
                    } else {
                        session.send("question").await.expect("recovered session")
                    };
                    assert_eq!(answer, "answer");
                }
                rejected.assert_async().await;
                accepted.assert_async().await;
                assert_replay(&bodies, &attempts, provider, &case);
            }
        }
    }
}

#[test]
fn trace_temperature_reads_old_numeric_and_new_absent_fields() {
    let legacy = json!({"timestamp":0.0,"correlation_id":"contract","source":"broker",
        "model":"contract-model","messages":[],"temperature":0.0,"tools":null});
    let event: LlmCallTracerEvent =
        serde_json::from_value(legacy.clone()).expect("old numeric trace");
    assert_eq!(event.temperature, Some(0.0));
    let mut absent = legacy;
    absent.as_object_mut().expect("event").remove("temperature");
    let event: LlmCallTracerEvent = serde_json::from_value(absent).expect("new absent trace");
    assert_eq!(event.temperature, None);
    assert!(serde_json::to_value(event)
        .expect("serialized trace")
        .get("temperature")
        .is_none());
}
