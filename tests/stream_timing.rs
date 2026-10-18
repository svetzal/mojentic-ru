//! Public HTTP acceptance for MODEL-METADATA section 2.
#[path = "support/mod.rs"]
pub mod boundary;
use futures::StreamExt;
use mojentic::llm::{
    gateway::{CompletionConfig, LlmGateway, StreamChunk},
    gateways::OllamaGateway,
    LlmMessage,
};

#[tokio::test]
async fn first_frame_precedence_crosses_public_http_boundary() {
    let mut server = boundary::Fixture::new_async().await;
    let request = server.mock("POST", "/api/chat")
        .with_body(concat!(
            "{\"message\":{\"content\":\"answer\",\"thinking\":\"reason\"},\"done\":false}\n",
            "{\"message\":{},\"done\":true,\"done_reason\":\"stop\",\"eval_count\":2,\"total_duration\":900}\n"
        )).create_async().await;
    let gateway = OllamaGateway::with_host(server.url()).with_transport(server.transport());
    let messages = [LlmMessage::user("question")];
    let config = CompletionConfig::default();
    let events: Vec<_> = gateway.complete_stream("m", &messages, None, &config).collect().await;
    let metrics = events
        .into_iter()
        .filter_map(|e| match e.expect("stream succeeds") {
            StreamChunk::Metrics(m) => Some(serde_json::to_value(m).expect("metrics serialize")),
            _ => None,
        })
        .next_back()
        .expect("terminal metrics");
    assert_eq!(metrics["first_token_kind"], "thinking");
    assert!(metrics["time_to_first_token_ms"].is_number());
    assert!(metrics["request_duration_ms"].is_number());
    assert_eq!(metrics["total_duration_ns"], 900);
    request.assert_async().await;
}

use mojentic::llm::{
    gateways::{OmlxConfig, OmlxGateway, OpenAIGateway},
    recovery::RecoveryPolicy,
    stream_timing::StreamClock,
    StreamEvent,
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

struct ScriptClock(Mutex<VecDeque<u64>>);
impl ScriptClock {
    fn new(values: &[u64]) -> Arc<Self> {
        Arc::new(Self(Mutex::new(values.iter().copied().collect())))
    }
    fn remaining(&self) -> usize {
        self.0.lock().expect("clock").len()
    }
}
impl StreamClock for ScriptClock {
    fn now(&self) -> Duration {
        Duration::from_millis(
            self.0
                .lock()
                .expect("clock")
                .pop_front()
                .expect("only dispatch, first token and terminal sample the clock"),
        )
    }
}
fn artifact() -> Value {
    serde_json::from_str(include_str!("contracts/stream_timing_v1.json")).expect("timing contract")
}
fn path(provider: &str) -> &'static str {
    match provider {
        "ollama" => "/api/chat",
        "omlx" => "/v1/chat/completions",
        _ => "/chat/completions",
    }
}
struct AllowRetry;
#[async_trait::async_trait]
impl mojentic::llm::recovery::Admission for AllowRetry {
    async fn decide(
        &self,
        _: mojentic::llm::recovery::AdmissionContext,
    ) -> mojentic::llm::recovery::AdmissionDecision {
        mojentic::llm::recovery::AdmissionDecision::Allow
    }
}
fn gateway(
    provider: &str,
    url: &str,
    clock: Arc<dyn StreamClock>,
    recovery: bool,
    attempts: u32,
) -> Arc<dyn LlmGateway> {
    let policy = RecoveryPolicy {
        max_attempts: std::num::NonZeroU32::new(attempts).expect("positive attempts"),
        base_delay: Duration::ZERO,
        admission: Some(Arc::new(AllowRetry)),
        ..Default::default()
    };
    match provider {
        "ollama" => {
            let g = {
                let transport = boundary::transport_for(url);
                OllamaGateway::with_host(url).with_transport(transport)
            }
            .with_stream_clock(clock);
            if recovery {
                Arc::new(g.with_recovery(policy).expect("policy"))
            } else {
                Arc::new(g)
            }
        }
        "omlx" => {
            let g = {
                let transport = boundary::transport_for(url);
                OmlxGateway::with_config(OmlxConfig {
                    host: url.into(),
                    api_key: None,
                    ..Default::default()
                })
                .with_transport(transport)
            }
            .with_stream_clock(clock);
            if recovery {
                Arc::new(g.with_recovery(policy).expect("policy"))
            } else {
                Arc::new(g)
            }
        }
        _ => {
            let g = {
                let transport = boundary::transport_for(url);
                OpenAIGateway::with_api_key_and_base_url("test-key", url).with_transport(transport)
            }
            .with_stream_clock(clock);
            if recovery {
                Arc::new(g.with_recovery(policy).expect("policy"))
            } else {
                Arc::new(g)
            }
        }
    }
}
fn assert_timing(metrics: &Value, case: &Value) {
    for field in [
        "time_to_first_token_ms",
        "first_token_kind",
        "request_duration_ms",
    ] {
        assert_eq!(metrics[field], case[field], "{}: {field}: {metrics}", case["name"]);
    }
}

#[tokio::test]
async fn section2_exact_clocks_all_public_http_streams() {
    let contract = artifact();
    assert_eq!(contract["version"], "rust-stream-timing-v1");
    for provider in ["ollama", "omlx", "openai"] {
        for case in contract["providers"][provider].as_array().expect("cases") {
            for entry in ["legacy", "events", "recovery"] {
                let mut server = boundary::Fixture::new_async().await;
                let request = server
                    .mock("POST", path(provider))
                    .with_status(case["status"].as_u64().unwrap_or(200) as usize)
                    .match_body(boundary::Matcher::PartialJson(
                        serde_json::json!({"model":"gpt-4o","stream":true}),
                    ))
                    .with_body(case["body"].as_str().expect("contract wire body"))
                    .create_async()
                    .await;
                let clock = ScriptClock::new(if case["first_token_kind"].is_null() {
                    &[100, 160]
                } else {
                    &[100, 125, 160]
                });
                let gateway =
                    gateway(provider, &server.url(), clock.clone(), entry == "recovery", 1);
                let messages = [LlmMessage::user("question")];
                let config = CompletionConfig::default();
                let mut metrics = Vec::new();
                if entry == "events" {
                    let stream = gateway
                        .complete_stream_events("gpt-4o", &messages, &config)
                        .expect("event API");
                    assert_eq!(
                        clock.remaining(),
                        if case["first_token_kind"].is_null() {
                            2
                        } else {
                            3
                        },
                        "construction must not dispatch"
                    );
                    let events: Vec<_> = stream.collect().await;
                    for event in &events {
                        if let StreamEvent::Metrics(m) = event {
                            metrics.push(serde_json::to_value(m).expect("metrics"));
                        }
                    }
                    assert_eq!(
                        matches!(events.last(), Some(StreamEvent::Completed(_))),
                        case["event_success"].as_bool().expect("success"),
                        "{provider}: {case}"
                    );
                    if let Some(StreamEvent::Completed(evidence)) = events.last() {
                        assert_timing(
                            &serde_json::to_value(&evidence.metadata).expect("evidence"),
                            case,
                        );
                    }
                } else {
                    let stream = gateway.complete_stream("gpt-4o", &messages, None, &config);
                    assert_eq!(
                        clock.remaining(),
                        if case["first_token_kind"].is_null() {
                            2
                        } else {
                            3
                        }
                    );
                    let events: Vec<_> = stream.collect().await;
                    for event in &events {
                        if let Ok(StreamChunk::Metrics(m)) = event {
                            metrics.push(serde_json::to_value(m).expect("metrics"));
                        }
                    }
                    if entry == "recovery" {
                        assert_eq!(
                            events.iter().all(Result::is_ok),
                            case["recovery_success"].as_bool().expect("success"),
                            "{provider}: {case}"
                        );
                    }
                }
                assert_eq!(metrics.len(), 1, "{provider} {entry}: {metrics:?}");
                assert_timing(&metrics[0], case);
                if provider == "omlx" && case["recovery_success"] == true && entry != "events" {
                    assert_eq!(metrics[0]["provider_usage"], contract["provider_usage"]);
                }
                assert_eq!(clock.remaining(), 0, "all clock samples used");
                request.assert_async().await;
            }
        }
    }
}

#[tokio::test]
async fn section2_retry_keeps_first_dispatch_clock() {
    struct RetryClock(Mutex<VecDeque<u64>>);
    impl StreamClock for RetryClock {
        fn now(&self) -> Duration {
            Duration::from_millis(self.0.lock().expect("clock").pop_front().unwrap_or(200))
        }
    }
    for provider in ["ollama", "omlx", "openai"] {
        let mut server = boundary::Fixture::new_async().await;
        let rejected = server
            .mock("POST", path(provider))
            .with_status(503)
            .with_body("busy")
            .expect(1)
            .create_async()
            .await;
        let contract = artifact();
        let case = &contract["providers"][provider][3];
        let accepted = server
            .mock("POST", path(provider))
            .with_body(case["body"].as_str().expect("body"))
            .expect(1)
            .create_async()
            .await;
        let clock = Arc::new(RetryClock(Mutex::new([100, 125, 160].into_iter().collect())));
        let gateway = gateway(provider, &server.url(), clock.clone(), true, 2);
        let events: Vec<_> = gateway
            .complete_stream("gpt-4o", &[], None, &CompletionConfig::default())
            .collect()
            .await;
        assert!(
            events.iter().all(Result::is_ok),
            "{provider}: admitted HTTP retries succeed: {events:?}"
        );
        let metrics = events
            .iter()
            .find_map(|event| match event {
                Ok(StreamChunk::Metrics(m)) => Some(serde_json::to_value(m).expect("metrics")),
                _ => None,
            })
            .expect("terminal metrics");
        assert_timing(&metrics, case);
        assert!(clock.0.lock().expect("clock").is_empty());
        rejected.assert_async().await;
        accepted.assert_async().await;
    }
}

#[test]
fn section2_old_metric_documents_remain_readable() {
    assert!(
        serde_json::from_str::<mojentic::llm::gateway::StreamMetrics>("{}").is_err(),
        "the existing provider field must remain required"
    );
    let metrics: mojentic::llm::gateway::StreamMetrics =
        serde_json::from_str(r#"{"provider":"ollama","eval_count":2}"#).expect("old metric shape");
    assert_eq!(metrics.eval_count, Some(2));
    assert_eq!(metrics.first_token_kind, None);
    assert_eq!(metrics.request_duration_ms, None);
}

#[derive(Clone)]
struct Inspect;
#[async_trait::async_trait]
impl mojentic::llm::tools::LlmTool for Inspect {
    async fn run(
        &self,
        _: &std::collections::HashMap<String, Value>,
        _: &mojentic::llm::tools::ToolRunCtx,
    ) -> mojentic::error::Result<Value> {
        Ok(serde_json::json!({"receipt":"observed"}))
    }
    fn descriptor(&self) -> mojentic::llm::tools::ToolDescriptor {
        mojentic::llm::tools::ToolDescriptor {
            r#type: "function".into(),
            function: mojentic::llm::tools::FunctionDescriptor {
                name: "inspect".into(),
                description: "Observe contract".into(),
                parameters: serde_json::json!({"type":"object","properties":{}}),
            },
        }
    }
    fn clone_box(&self) -> Box<dyn mojentic::llm::tools::LlmTool> {
        Box::new(self.clone())
    }
}

#[tokio::test]
async fn section2_broker_session_tool_continuations_start_fresh_clocks() {
    use mojentic::{
        llm::{ChatSession, LlmBroker},
        tracer::{EventStore, LlmResponseTracerEvent, TracerSystem},
    };
    for provider in ["ollama", "omlx", "openai"] {
        for recovery in [false, true] {
            for session_entry in [false, true] {
                let contract = artifact();
                let mut server = boundary::Fixture::new_async().await;
                let tool = contract["tool_continuation"][provider]
                    .as_str()
                    .expect("versioned tool response");
                let first = server
                    .mock("POST", path(provider))
                    .with_body(tool)
                    .expect(1)
                    .create_async()
                    .await;
                let second = server
                    .mock("POST", path(provider))
                    .match_body(boundary::Matcher::Regex("observed".into()))
                    .with_body(
                        contract["providers"][provider][3]["body"].as_str().expect("final body"),
                    )
                    .expect(1)
                    .create_async()
                    .await;
                let clock = ScriptClock::new(&[100, 125, 160, 1000, 1007, 1030]);
                let gateway = gateway(provider, &server.url(), clock.clone(), recovery, 1);
                let evidence = Arc::new(Mutex::new(Vec::<Value>::new()));
                let captured = evidence.clone();
                let store = Arc::new(EventStore::new(Some(Arc::new(move |event| {
                    if let Some(response) = event
                        .as_any()
                        .and_then(|event| event.downcast_ref::<LlmResponseTracerEvent>())
                    {
                        captured.lock().expect("trace").push(
                            serde_json::to_value(&response.evidence.metadata).expect("metadata"),
                        );
                    }
                }))));
                let trace = Arc::new(TracerSystem::new(Some(store), true));
                let broker = LlmBroker::new("gpt-4o", gateway, Some(trace));
                let tools: Vec<Box<dyn mojentic::llm::tools::LlmTool>> = vec![Box::new(Inspect)];
                let answer = if session_entry {
                    let mut session = ChatSession::builder(broker).tools(tools).build();
                    session
                        .send_stream("question")
                        .collect::<Vec<_>>()
                        .await
                        .into_iter()
                        .collect::<mojentic::error::Result<Vec<_>>>()
                        .expect("session stream")
                        .concat()
                } else {
                    broker
                        .generate_stream(&[LlmMessage::user("question")], Some(&tools), None, None)
                        .collect::<Vec<_>>()
                        .await
                        .into_iter()
                        .collect::<mojentic::error::Result<Vec<_>>>()
                        .expect("broker stream")
                        .concat()
                };
                assert_eq!(answer, "answer");
                {
                    let observed = evidence.lock().expect("evidence");
                    assert_eq!(observed.len(), 2);
                    assert_timing(
                        &observed[0],
                        &serde_json::json!({"name":"tool turn","time_to_first_token_ms":25,"first_token_kind":"tool_call","request_duration_ms":60}),
                    );
                    assert_timing(
                        &observed[1],
                        &serde_json::json!({"name":"continuation","time_to_first_token_ms":7,"first_token_kind":"content","request_duration_ms":30}),
                    );
                    assert_eq!(clock.remaining(), 0);
                }
                first.assert_async().await;
                second.assert_async().await;
            }
        }
    }
}

async fn native_recovery_case<G: mojentic::llm::recovery::RecoveryProvider>(
    inner: G,
    case: &Value,
) {
    use mojentic::llm::recovery::{RecoveryCall, RecoveryGateway, RecoveryStreamEvent};
    let gateway = RecoveryGateway::new(inner, RecoveryPolicy::default()).expect("gateway");
    let call = RecoveryCall::default();
    let events: Vec<_> = gateway
        .complete_stream_with_recovery("gpt-4o", &[], None, &CompletionConfig::default(), &call)
        .collect()
        .await;
    let metrics = match events.last().expect("terminal") {
        RecoveryStreamEvent::Completed(result) => result.report.stream_metrics.as_ref(),
        RecoveryStreamEvent::Failed(error) => error.stream_metrics.as_ref(),
        _ => panic!("expected terminal"),
    }
    .expect("terminal stream timing");
    assert_timing(&serde_json::to_value(metrics).expect("metrics"), case);
    assert_eq!(
        matches!(events.last(), Some(RecoveryStreamEvent::Completed(_))),
        case["recovery_success"].as_bool().expect("success")
    );
    if let Some(RecoveryStreamEvent::Failed(error)) = events.last() {
        if let Some(secret) = case["private_usage"].as_str() {
            assert_eq!(
                error
                    .stream_metrics
                    .as_ref()
                    .expect("timing")
                    .provider_usage
                    .as_ref()
                    .expect("raw usage")["private_detail"],
                secret
            );
            assert!(!format!("{error:?}").contains(secret));
            assert!(!error.to_string().contains(secret));
            assert!(!serde_json::to_string(error).expect("safe error").contains(secret));
        }
    }
    if let Some(RecoveryStreamEvent::Completed(result)) = events.last() {
        assert_timing(
            &serde_json::to_value(&result.value.evidence.metadata).expect("evidence"),
            case,
        );
    }
}
#[tokio::test]
async fn section2_native_recovery_terminal_success_and_failure_retain_timing() {
    let contract = artifact();
    for provider in ["ollama", "omlx", "openai"] {
        for case in contract["providers"][provider].as_array().expect("cases") {
            let mut server = boundary::Fixture::new_async().await;
            let request = server
                .mock("POST", path(provider))
                .with_status(case["status"].as_u64().unwrap_or(200) as usize)
                .with_body(case["body"].as_str().expect("body"))
                .expect(1)
                .create_async()
                .await;
            let clock = ScriptClock::new(if case["first_token_kind"].is_null() {
                &[100, 160]
            } else {
                &[100, 125, 160]
            });
            match provider {
                "ollama" => {
                    native_recovery_case(
                        OllamaGateway::with_host(server.url())
                            .with_transport(server.transport())
                            .with_stream_clock(clock.clone()),
                        case,
                    )
                    .await
                }
                "omlx" => {
                    native_recovery_case(
                        OmlxGateway::with_config(OmlxConfig {
                            host: server.url(),
                            api_key: None,
                            ..Default::default()
                        })
                        .with_transport(server.transport())
                        .with_stream_clock(clock.clone()),
                        case,
                    )
                    .await
                }
                _ => {
                    native_recovery_case(
                        OpenAIGateway::with_api_key_and_base_url("test-key", server.url())
                            .with_transport(server.transport())
                            .with_stream_clock(clock.clone()),
                        case,
                    )
                    .await
                }
            }
            assert_eq!(clock.remaining(), 0);
            request.assert_async().await;
        }
    }
}

#[tokio::test]
async fn section2_retry_does_not_relabel_prior_attempt_provider_usage() {
    let contract = artifact();
    for provider in ["ollama", "omlx", "openai"] {
        let mut server = boundary::Fixture::new_async().await;
        let case = &contract["retry_provider_usage"][provider];
        let first = server
            .mock("POST", path(provider))
            .with_body(case["failed_body"].as_str().expect("usage-only interrupted attempt"))
            .expect(1)
            .create_async()
            .await;
        let second = server
            .mock("POST", path(provider))
            .with_body(case["successful_body"].as_str().expect("successful attempt without usage"))
            .expect(1)
            .create_async()
            .await;
        let clock = ScriptClock::new(&[100, 125, 160]);
        let gateway = gateway(provider, &server.url(), clock.clone(), true, 2);
        let events: Vec<_> = gateway
            .complete_stream("gpt-4o", &[], None, &CompletionConfig::default())
            .collect()
            .await;
        assert!(events.iter().all(Result::is_ok), "{events:?}");
        let metrics = events
            .iter()
            .find_map(|event| match event {
                Ok(StreamChunk::Metrics(m)) => Some(serde_json::to_value(m).expect("metrics")),
                _ => None,
            })
            .expect("terminal metrics");
        assert_eq!(
            metrics["provider_usage"],
            Value::Null,
            "a successful attempt must not inherit failed-attempt usage"
        );
        assert_timing(&metrics, &contract["providers"][provider][3]);
        assert_eq!(clock.remaining(), 0);
        first.assert_async().await;
        second.assert_async().await;
    }
}

// Interrupt an in-process response body only after the semantic frame has been observed.
struct InterruptedClock {
    script: Arc<ScriptClock>,
    observed: Arc<tokio::sync::Notify>,
}
impl StreamClock for InterruptedClock {
    fn now(&self) -> Duration {
        let now = self.script.now();
        if self.script.remaining() == 1 {
            self.observed.notify_one();
        }
        now
    }
}

async fn verify_transport_interruption(entry: &str) {
    use mojentic::{
        llm::{ChatSession, LlmBroker},
        tracer::{EventStore, LlmResponseTracerEvent, TracerSystem},
    };
    let contract = artifact();
    for provider in ["ollama", "omlx", "openai"] {
        for case in contract["transport_interruption"]["providers"][provider]
            .as_array()
            .expect("interruption cases")
        {
            let observed = Arc::new(tokio::sync::Notify::new());
            let script = ScriptClock::new(&[
                contract["clock"]["dispatch"].as_u64().expect("dispatch"),
                contract["clock"]["first"].as_u64().expect("first"),
                contract["clock"]["terminal"].as_u64().expect("terminal"),
            ]);
            let clock = Arc::new(InterruptedClock {
                script: script.clone(),
                observed: observed.clone(),
            });
            let server = boundary::Fixture::new();
            let url = server.url();
            let body = case["body"].as_str().expect("contract frame").to_owned();
            let expected_path = path(provider).to_string();
            server.respond(move |request| {
                assert_eq!(request.method(), "POST");
                assert_eq!(request.url().path(), expected_path);
                let payload: Value = serde_json::from_slice(request.body().expect("body").as_bytes().expect("bytes")).expect("JSON request");
                assert_eq!(payload["stream"], true);
                assert_eq!(payload["model"], "gpt-4o");
                assert_eq!(payload["messages"].as_array().expect("messages").last().expect("query")["content"], "question");
                let body = body.clone();
                let observed = observed.clone();
                let chunks = async_stream::stream! {
                    yield Ok::<_, std::io::Error>(body);
                    observed.notified().await;
                    yield Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "contract transport interruption"));
                };
                http::Response::new(reqwest::Body::wrap_stream(chunks)).into()
            });
            let gateway = gateway(provider, &url, clock, false, 1);
            let evidence = Arc::new(Mutex::new(Vec::<Value>::new()));
            let captured = evidence.clone();
            let store = Arc::new(EventStore::new(Some(Arc::new(move |event| {
                if let Some(response) =
                    event.as_any().and_then(|e| e.downcast_ref::<LlmResponseTracerEvent>())
                {
                    captured
                        .lock()
                        .expect("trace")
                        .push(serde_json::to_value(&response.evidence.metadata).expect("metadata"));
                }
            }))));
            let broker = LlmBroker::new(
                "gpt-4o",
                gateway.clone(),
                Some(Arc::new(TracerSystem::new(Some(store), true))),
            );
            let check_error = |item: &mojentic::error::Result<String>| {
                if item.is_err() {
                    let retained = evidence.lock().expect("evidence at error delivery");
                    assert_eq!(
                        retained.len(),
                        1,
                        "{provider}/{entry}: evidence before error delivery"
                    );
                    assert_timing(&retained[0], case);
                }
            };
            let messages = [LlmMessage::user("question")];
            let metrics = if entry == "gateway" {
                let events: Vec<_> = gateway
                    .complete_stream("gpt-4o", &messages, None, &CompletionConfig::default())
                    .collect()
                    .await;
                assert_transport_error(events.last().expect("terminal"));
                assert!(matches!(events[events.len() - 2], Ok(StreamChunk::Metrics(_))));
                let text: Vec<_> = events
                    .iter()
                    .filter_map(|event| match event {
                        Ok(StreamChunk::Content(text)) => Some(text),
                        _ => None,
                    })
                    .collect();
                assert_eq!(serde_json::to_value(text).expect("gateway text"), case["text_chunks"]);
                events
                    .iter()
                    .find_map(|item| match item {
                        Ok(StreamChunk::Metrics(m)) => {
                            Some(serde_json::to_value(m).expect("metrics"))
                        }
                        _ => None,
                    })
                    .expect("gateway timing")
            } else {
                let events = if entry == "session" {
                    let mut session = ChatSession::builder(broker).build();
                    session.insert_message(LlmMessage::assistant("previous answer"));
                    let before = session.messages().to_vec();
                    let events = session
                        .send_stream("question")
                        .inspect(check_error)
                        .collect::<Vec<_>>()
                        .await;
                    assert_eq!(session.messages().len(), before.len() + 1);
                    for (old, retained) in before.iter().zip(session.messages()) {
                        assert_eq!(
                            serde_json::to_value(&old.message).expect("old"),
                            serde_json::to_value(&retained.message).expect("retained")
                        );
                    }
                    assert_eq!(
                        serde_json::to_value(&session.messages().last().expect("user").message)
                            .expect("user"),
                        serde_json::to_value(LlmMessage::user("question")).expect("expected user")
                    );
                    events
                } else {
                    broker
                        .generate_stream(&messages, None, None, None)
                        .inspect(check_error)
                        .collect::<Vec<_>>()
                        .await
                };
                let text: Vec<_> = events.iter().filter_map(|event| event.as_ref().ok()).collect();
                assert_eq!(serde_json::to_value(text).expect("text"), case["text_chunks"]);
                assert_transport_error(events.last().expect("terminal"));
                let retained = evidence.lock().expect("evidence");
                assert_eq!(retained.len(), 1, "{provider}/{entry}: terminal response evidence");
                assert_timing(&retained[0]["stream_metrics"], case);
                retained[0].clone()
            };
            assert_timing(&metrics, case);
            assert_eq!(script.remaining(), 0);
            assert_eq!(server.requests().len(), 1);
        }
    }
}

fn assert_transport_error<T: std::fmt::Debug>(terminal: &mojentic::error::Result<T>) {
    match terminal {
        Err(mojentic::error::MojenticError::HttpError(error)) => {
            use std::error::Error;
            assert!(error.is_body() || error.is_decode(), "HTTP body interruption: {error:?}");
            let mut cause = error.source();
            let mut unexpected_eof = false;
            while let Some(source) = cause {
                unexpected_eof |= source
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::UnexpectedEof);
                cause = source.source();
            }
            assert!(unexpected_eof, "original transport EOF cause retained: {error:?}");
        }
        other => panic!("expected actual HTTP body error, got {other:?}"),
    }
}

#[tokio::test]
async fn section2_transport_interruption_gateway() {
    verify_transport_interruption("gateway").await;
}

#[tokio::test]
async fn section2_transport_interruption_broker() {
    verify_transport_interruption("broker").await;
}

#[tokio::test]
async fn section2_transport_interruption_session() {
    verify_transport_interruption("session").await;
}
