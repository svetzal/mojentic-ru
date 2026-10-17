//! Public HTTP-boundary conformance; synthetic payloads and loopback only.
use async_trait::async_trait;
use futures::StreamExt;
use mojentic::{
    error::Result,
    llm::{
        gateway::CompletionConfig,
        gateways::{OllamaGateway, OmlxConfig, OmlxGateway, OpenAIGateway},
        recovery::*,
        tools::{FunctionDescriptor, LlmTool, ToolDescriptor},
        LlmBroker, LlmMessage,
    },
};
use serde_json::{json, Value};
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

#[derive(Default)]
struct Capture {
    events: Mutex<Vec<LifecycleEvent>>,
    requests: Mutex<Vec<(Identity, Vec<u8>)>>,
}
impl LifecycleObserver for Capture {
    fn observe(&self, event: &LifecycleEvent) {
        self.events.lock().expect("events").push(event.clone());
    }
}
impl WireObserver for Capture {
    fn observe(
        &self,
        event: WireEvent<'_>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let WireEvent::Request { identity, request } = event {
            self.requests.lock().expect("requests").push((
                identity,
                request.body().and_then(|b| b.as_bytes()).expect("encoded body").to_vec(),
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
struct Reject;
#[async_trait]
impl Admission for Reject {
    async fn decide(&self, _: AdmissionContext) -> AdmissionDecision {
        AdmissionDecision::Reject
    }
}
struct Pending;
#[async_trait]
impl Admission for Pending {
    async fn decide(&self, _: AdmissionContext) -> AdmissionDecision {
        std::future::pending().await
    }
}
struct Clock {
    wall_seconds: u64,
    start: Instant,
    elapsed: Mutex<Duration>,
    sleeps: Mutex<Vec<Duration>>,
}
impl Default for Clock {
    fn default() -> Self {
        Self {
            wall_seconds: 1_000_000,
            start: Instant::now(),
            elapsed: Mutex::new(Duration::ZERO),
            sleeps: Mutex::new(vec![]),
        }
    }
}
#[async_trait]
impl Timing for Clock {
    fn monotonic(&self) -> Instant {
        self.start + *self.elapsed.lock().expect("clock")
    }
    fn wall(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(self.wall_seconds)
    }
    fn jitter(&self, ceiling: Duration) -> Duration {
        ceiling / 2
    }
    async fn sleep(&self, delay: Duration) {
        self.sleeps.lock().expect("sleep").push(delay);
        *self.elapsed.lock().expect("clock") += delay;
    }
}
fn policy(attempts: u32, capture: &Arc<Capture>) -> RecoveryPolicy {
    RecoveryPolicy {
        max_attempts: NonZeroU32::new(attempts).expect("attempts"),
        base_delay: Duration::ZERO,
        admission: Some(Arc::new(Allow)),
        observer: Some(capture.clone()),
        wire_observer: Some(capture.clone()),
        ..Default::default()
    }
}
fn omlx(url: &str) -> OmlxGateway {
    OmlxGateway::with_config(OmlxConfig {
        host: url.into(),
        api_key: Some("synthetic-secret".into()),
        ..Default::default()
    })
}
fn response(provider: Provider, content: &str) -> Value {
    if provider == Provider::Ollama {
        json!({"message":{"content":content},"done":true,"done_reason":"stop"})
    } else {
        json!({"choices":[{"message":{"content":content},"finish_reason":"stop"}]})
    }
}
fn path(provider: Provider) -> &'static str {
    match provider {
        Provider::Ollama => "/api/chat",
        Provider::Omlx => "/v1/chat/completions",
        Provider::OpenAi => "/chat/completions",
    }
}
async fn seed<G: RecoveryProvider>(
    inner: G,
    provider: Provider,
    server: &mut mockito::ServerGuard,
) {
    let capture = Arc::new(Capture::default());
    let fail = server
        .mock("POST", path(provider))
        .with_status(503)
        .with_body("synthetic-secret body")
        .expect(1)
        .create_async()
        .await;
    let success = server
        .mock("POST", path(provider))
        .with_status(200)
        .with_body(response(provider, "done").to_string())
        .expect(1)
        .create_async()
        .await;
    let gateway = RecoveryGateway::new(inner, policy(2, &capture)).expect("gateway");
    let result = gateway
        .complete_with_recovery(
            "qwen3",
            &[LlmMessage::user("synthetic-secret prompt")],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .await
        .expect("recover");
    assert_eq!(result.value.content.as_deref(), Some("done"));
    assert_eq!(result.report.identity.wire_attempt, 2);
    assert_eq!(result.report.history.len(), 1);
    let requests = capture.requests.lock().expect("requests").clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].1, requests[1].1);
    assert_eq!(requests[0].0.logical_request_id, requests[1].0.logical_request_id);
    assert_ne!(requests[0].0.attempt_id, requests[1].0.attempt_id);
    let events = capture.events.lock().expect("events").clone();
    assert_eq!(
        events.iter().map(|e| e.transition).collect::<Vec<_>>(),
        vec![
            Transition::AttemptStarted,
            Transition::AttemptFailed,
            Transition::AdmissionPending,
            Transition::AdmissionAllowed,
            Transition::DelayScheduled,
            Transition::RetryStarted,
            Transition::AttemptStarted,
            Transition::AttemptSucceeded
        ]
    );
    assert!(!serde_json::to_string(&*events)
        .expect("safe events")
        .contains("synthetic-secret"));
    fail.assert_async().await;
    success.assert_async().await;
}
#[tokio::test]
async fn public_503_success_all_adapters() {
    let mut server = mockito::Server::new_async().await;
    seed(OllamaGateway::with_host(server.url()), Provider::Ollama, &mut server).await;
    let mut server = mockito::Server::new_async().await;
    seed(omlx(&server.url()), Provider::Omlx, &mut server).await;
    let mut server = mockito::Server::new_async().await;
    seed(
        OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url()),
        Provider::OpenAi,
        &mut server,
    )
    .await;
}
#[derive(Clone)]
struct CountTool(Arc<AtomicUsize>);
#[async_trait]
impl LlmTool for CountTool {
    async fn run(
        &self,
        _: &std::collections::HashMap<String, Value>,
        _: &mojentic::llm::tools::ToolRunCtx,
    ) -> Result<Value> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"ok":true}))
    }
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".into(),
            function: FunctionDescriptor {
                name: "count".into(),
                description: "count once".into(),
                parameters: json!({"type":"object"}),
            },
        }
    }
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}
#[tokio::test]
async fn public_broker_recovery_never_reexecutes_tool() {
    let mut server = mockito::Server::new_async().await;
    let tool=server.mock("POST","/chat/completions").match_body(mockito::Matcher::PartialJson(json!({"messages":[{"role":"user","content":"tool"}]}))).with_body(json!({"choices":[{"message":{"tool_calls":[{"id":"call1","function":{"name":"count","arguments":"{}"}}]}}]}).to_string()).expect(1).create_async().await;
    let fail = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex("tool_call_id".into()))
        .with_status(503)
        .expect(1)
        .create_async()
        .await;
    let done = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex("tool_call_id".into()))
        .with_body(response(Provider::OpenAi, "done").to_string())
        .expect(1)
        .create_async()
        .await;
    let capture = Arc::new(Capture::default());
    let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url())
        .with_recovery(policy(2, &capture))
        .expect("gateway");
    let count = Arc::new(AtomicUsize::new(0));
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(CountTool(count.clone()))];
    let broker = LlmBroker::new("gpt-4o", Arc::new(gateway), None);
    assert_eq!(
        broker
            .generate(&[LlmMessage::user("tool")], Some(&tools), None, None)
            .await
            .expect("broker"),
        "done"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let requests = capture.requests.lock().expect("requests").clone();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].1, requests[2].1);
    assert_eq!(requests[1].0.logical_request_id, requests[2].0.logical_request_id);
    assert_ne!(requests[0].0.logical_request_id, requests[1].0.logical_request_id);
    tool.assert_async().await;
    fail.assert_async().await;
    done.assert_async().await;
}
#[tokio::test]
async fn bounded_errors_privacy_and_disabled() {
    for (status, attempts, expected) in [
        (504, 3, Outcome::Exhausted),
        (503, 1, Outcome::Exhausted),
        (400, 3, Outcome::Failed),
        (401, 3, Outcome::Failed),
    ] {
        let mut server = mockito::Server::new_async().await;
        let capture = Arc::new(Capture::default());
        let sends = if status == 400 || status == 401 {
            1
        } else {
            attempts
        };
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(status)
            .with_header("x-request-id", "synthetic-secret")
            .with_body(r#"{"error":{"message":"synthetic-secret","code":"synthetic-secret"}}"#)
            .expect(sends as usize)
            .create_async()
            .await;
        let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url())
            .with_recovery(policy(attempts, &capture))
            .expect("gateway");
        let error = gateway
            .complete_with_recovery(
                "gpt-4o",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await
            .err()
            .expect("failure");
        assert_eq!(error.outcome, expected);
        assert_eq!(error.history.len(), sends as usize);
        assert_eq!(error.failure.http_status, Some(status as u16));
        assert_eq!(error.failure.identity.as_ref().expect("identity").wire_attempt, sends);
        assert!(error
            .failure
            .inspect_cause()
            .expect("cause")
            .to_string()
            .contains("synthetic-secret"));
        assert!(!format!(
            "{error:?} {error} {}",
            serde_json::to_string(&error).expect("serialize")
        )
        .contains("synthetic-secret"));
        mock.assert_async().await;
    }
}
#[tokio::test]
async fn retry_after_floors_dates_invalid_and_refusals() {
    for (header, expected, outcome) in [
        ("2", Duration::from_secs(2), None),
        ("Mon, 12 Jan 1970 13:46:42 GMT", Duration::from_secs(2), None),
        ("Monday, 12-Jan-70 13:46:42 GMT", Duration::from_secs(2), None),
        ("Mon Jan 12 13:46:42 1970", Duration::from_secs(2), None),
        ("Mon, 12 Jan 1970 13:46:39 GMT", Duration::from_secs(1), None),
        ("bad", Duration::from_secs(1), None),
        ("31", Duration::ZERO, Some(Outcome::DelayCeiling)),
    ] {
        let mut server = mockito::Server::new_async().await;
        let capture = Arc::new(Capture::default());
        let clock = Arc::new(Clock::default());
        let fail = server
            .mock("POST", "/chat/completions")
            .with_status(429)
            .with_header("retry-after", header)
            .expect(1)
            .create_async()
            .await;
        let done = server
            .mock("POST", "/chat/completions")
            .with_body(response(Provider::OpenAi, "ok").to_string())
            .expect(if outcome.is_some() { 0 } else { 1 })
            .create_async()
            .await;
        let mut p = policy(2, &capture);
        p.timing = clock.clone();
        p.base_delay = Duration::from_secs(2);
        let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url())
            .with_recovery(p)
            .expect("gateway");
        let result = gateway
            .complete_with_recovery(
                "gpt-4o",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await;
        if let Some(outcome) = outcome {
            assert_eq!(result.err().expect("refused").outcome, outcome);
            assert!(clock.sleeps.lock().expect("sleep").is_empty());
        } else {
            assert!(result.is_ok());
            assert_eq!(*clock.sleeps.lock().expect("sleep"), vec![expected]);
        }
        fail.assert_async().await;
        done.assert_async().await;
    }
}
#[tokio::test]
async fn local_ambiguity_requires_admission_rejection_preserves_failure() {
    for admission in [None, Some(Arc::new(Reject) as Arc<dyn Admission>)] {
        let mut server = mockito::Server::new_async().await;
        let capture = Arc::new(Capture::default());
        let mock = server.mock("POST", "/api/chat").with_status(504).expect(1).create_async().await;
        let expected = if admission.is_none() {
            Outcome::AdmissionRequired
        } else {
            Outcome::AdmissionRejected
        };
        let mut p = policy(3, &capture);
        p.admission = admission;
        let gateway = OllamaGateway::with_host(server.url()).with_recovery(p).expect("gateway");
        let error = gateway
            .complete_with_recovery(
                "qwen3",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await
            .err()
            .expect("failure");
        assert_eq!(error.outcome, expected);
        assert_eq!(error.history.len(), 1);
        assert_eq!(error.failure.acceptance, Acceptance::Unknown);
        mock.assert_async().await;
    }
}
#[tokio::test]
async fn cancellation_pending_admission_and_backoff() {
    for pending in [true, false] {
        let mut server = mockito::Server::new_async().await;
        let capture = Arc::new(Capture::default());
        let mock = server.mock("POST", "/api/chat").with_status(503).expect(1).create_async().await;
        let mut p = policy(3, &capture);
        if pending {
            p.admission = Some(Arc::new(Pending));
        } else {
            p.base_delay = Duration::from_secs(10);
            p.timing = Arc::new(SystemTiming);
        }
        let gateway = OllamaGateway::with_host(server.url()).with_recovery(p).expect("gateway");
        let call = RecoveryCall::default();
        let cancel = call.cancellation.clone();
        let events = capture.clone();
        tokio::spawn(async move {
            loop {
                if events.events.lock().expect("events").iter().any(|e| {
                    e.transition
                        == if pending {
                            Transition::AdmissionPending
                        } else {
                            Transition::DelayScheduled
                        }
                }) {
                    cancel.cancel();
                    break;
                }
                tokio::task::yield_now().await;
            }
        });
        let error = tokio::time::timeout(
            Duration::from_secs(2),
            gateway.complete_with_recovery("qwen3", &[], None, &CompletionConfig::default(), &call),
        )
        .await
        .expect("prompt cancellation")
        .err()
        .expect("cancelled");
        assert_eq!(error.outcome, Outcome::Cancelled);
        assert_eq!(error.failure.http_status, Some(503));
        assert!(error.failure.progress.headers_received);
        assert_eq!(error.history.len(), 1);
        mock.assert_async().await;
    }
}

struct HttpFixture {
    url: String,
    sends: Arc<AtomicUsize>,
    bodies: Arc<Mutex<Vec<Vec<u8>>>>,
    closed: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn fixture(replies: Vec<(u16, String, bool, bool)>) -> HttpFixture {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("loopback");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let sends = Arc::new(AtomicUsize::new(0));
    let bodies = Arc::new(Mutex::new(vec![]));
    let closed = Arc::new(AtomicUsize::new(0));
    let state = (sends.clone(), bodies.clone(), closed.clone());
    let task = tokio::spawn(async move {
        for (status, body, partial, hang) in replies {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut bytes = vec![];
            let mut buf = [0u8; 4096];
            loop {
                let n = socket.read(&mut buf).await.expect("request");
                if n == 0 {
                    return;
                }
                bytes.extend_from_slice(&buf[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let length = head
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            if k.eq_ignore_ascii_case("content-length") {
                                v.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        state
                            .1
                            .lock()
                            .expect("bodies")
                            .push(bytes[end + 4..end + 4 + length].to_vec());
                        break;
                    }
                }
            }
            state.0.fetch_add(1, Ordering::SeqCst);
            let length = body.len() + if partial || hang { 100 } else { 0 };
            let head=format!("HTTP/1.1 {status} Fixture\r\nContent-Type: text/event-stream\r\nContent-Length: {length}\r\nX-Request-ID: req_550e8400-e29b-41d4-a716-446655440000\r\nRetry-After: 0\r\nConnection: close\r\n\r\n");
            socket.write_all(head.as_bytes()).await.expect("headers");
            socket.write_all(body.as_bytes()).await.expect("body");
            socket.flush().await.expect("flush");
            if hang {
                let mut buf = [0u8; 1];
                let _ = socket.read(&mut buf).await;
                state.2.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    HttpFixture {
        url,
        sends,
        bodies,
        closed,
        task,
    }
}
fn frame(provider: Provider, kind: &str) -> String {
    if provider == Provider::Ollama {
        let message = match kind {
            "reasoning" => json!({"thinking":"synthetic-secret reasoning"}),
            "content" => json!({"content":"partial"}),
            _ => json!({"tool_calls":[{"function":{"name":"count","arguments":{}}}]}),
        };
        format!("{}\n", json!({"message":message,"done":false}))
    } else {
        let delta = match kind {
            "reasoning" => json!({"reasoning_content":"synthetic-secret reasoning"}),
            "content" => json!({"content":"partial"}),
            _ => {
                json!({"tool_calls":[{"index":0,"id":"call1","function":{"name":"count","arguments":"{"}}]})
            }
        };
        format!("data: {}\n\n", json!({"choices":[{"delta":delta,"finish_reason":null}]}))
    }
}
fn final_stream(provider: Provider) -> String {
    if provider == Provider::Ollama {
        format!("{}\n", json!({"message":{"content":"done"},"done":true,"done_reason":"stop"}))
    } else {
        format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{"content":"done"},"finish_reason":"stop"}]})
        )
    }
}
async fn partial<G: RecoveryProvider>(
    gateway: G,
    provider: Provider,
    server: &HttpFixture,
    kind: &str,
) {
    let capture = Arc::new(Capture::default());
    let gateway = RecoveryGateway::new(gateway, policy(3, &capture)).expect("gateway");
    let call = RecoveryCall::default();
    let mut events = gateway.complete_stream_with_recovery(
        "qwen3",
        &[],
        None,
        &CompletionConfig::default(),
        &call,
    );
    let mut terminal = None;
    while let Some(event) = events.next().await {
        match event {
            RecoveryStreamEvent::Failed(error) => terminal = Some(error),
            RecoveryStreamEvent::Completed(_) => panic!("partial cannot complete"),
            _ => {}
        }
    }
    let error = terminal.expect("interrupted");
    assert_eq!(error.outcome, Outcome::Interrupted, "{provider:?} {kind}");
    assert!(error.failure.progress.observed.any());
    assert!(error.failure.progress.delivered.any());
    assert_eq!(error.failure.http_status, Some(200));
    assert!(error.failure.progress.raw_bytes > 0);
    assert_eq!(error.history.len(), 1);
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    assert!(!serde_json::to_string(&error).expect("safe").contains("synthetic-secret"));
}
#[tokio::test]
async fn partial_reasoning_content_and_tools_all_adapters_never_replay() {
    for kind in ["reasoning", "content", "tools"] {
        let server = fixture(vec![(200, frame(Provider::Ollama, kind), true, false)]).await;
        partial(OllamaGateway::with_host(&server.url), Provider::Ollama, &server, kind).await;
        let server = fixture(vec![(200, frame(Provider::Omlx, kind), true, false)]).await;
        partial(omlx(&server.url), Provider::Omlx, &server, kind).await;
        let server = fixture(vec![(200, frame(Provider::OpenAi, kind), true, false)]).await;
        partial(
            OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
            Provider::OpenAi,
            &server,
            kind,
        )
        .await;
    }
}
#[tokio::test]
async fn keepalive_only_recovers_raw_without_semantic_progress() {
    let server = fixture(vec![
        (200, "data: {\"model\":\"keepalive\"}\n\n".into(), false, false),
        (200, final_stream(Provider::Omlx), false, false),
    ])
    .await;
    let capture = Arc::new(Capture::default());
    let gateway = omlx(&server.url).with_recovery(policy(2, &capture)).expect("gateway");
    let call = RecoveryCall::default();
    let mut stream = gateway.complete_stream_with_recovery(
        "qwen3",
        &[],
        None,
        &CompletionConfig::default(),
        &call,
    );
    let mut result = None;
    while let Some(event) = stream.next().await {
        match event {
            RecoveryStreamEvent::Completed(r) => result = Some(r),
            RecoveryStreamEvent::Failed(e) => panic!("{e}"),
            _ => {}
        }
    }
    let result = result.expect("complete");
    assert_eq!(result.report.history.len(), 1);
    let progress = &result.report.history[0].progress;
    assert!(progress.raw_bytes > 0);
    assert!(!progress.observed.any());
    assert!(!progress.delivered.any());
    assert_eq!(server.sends.load(Ordering::SeqCst), 2);
    let bodies = server.bodies.lock().expect("bodies");
    assert_eq!(bodies[0], bodies[1]);
}
#[tokio::test]
async fn cancellation_retains_received_semantics_and_closes_when_consumer_pauses() {
    async fn check<G: RecoveryProvider>(inner: G, provider: Provider, server: &HttpFixture) {
        let body = format!(
            "{}{}{}",
            frame(provider, "content"),
            frame(provider, "reasoning"),
            frame(provider, "tools")
        );
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(3, &capture)).expect("gateway");
        let call = RecoveryCall::default();
        let mut stream = gateway.complete_stream_with_recovery(
            "qwen3",
            &[],
            None,
            &CompletionConfig::default(),
            &call,
        );
        if provider == Provider::Ollama {
            assert!(matches!(stream.next().await, Some(RecoveryStreamEvent::Progress { .. })));
        }
        assert!(matches!(stream.next().await, Some(RecoveryStreamEvent::Content(_))));
        call.cancellation.cancel();
        tokio::time::timeout(Duration::from_secs(2), async {
            while server.closed.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("socket closes without consumer polling");
        let error = loop {
            if let Some(RecoveryStreamEvent::Failed(error)) = stream.next().await {
                break error;
            }
        };
        assert_eq!(error.outcome, Outcome::Cancelled);
        let p = &error.failure.progress;
        assert_eq!(p.raw_bytes, body.len() as u64);
        assert_eq!(p.delivered.content_bytes, 7);
        assert_eq!(p.delivered.reasoning_bytes, 0);
        assert!(p.observed.reasoning_bytes > 0);
        assert_eq!(p.observed.tool_fragments, 1);
        assert_eq!(error.failure.http_status, Some(200));
        assert!(error.failure.provider_request_id.is_some());
        assert!(error
            .failure
            .inspect_cause()
            .expect("private evidence")
            .to_string()
            .contains("synthetic-secret"));
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert!(stream.next().await.is_none(), "one terminal stream event");
        assert_eq!(error.history.len(), 1);
        assert_eq!(error.history[0].identity, error.failure.identity);
        assert_eq!(error.history[0].progress, error.failure.progress);
        let events = capture.events.lock().expect("events");
        assert_eq!(
            events.iter().map(|e| e.transition).collect::<Vec<_>>(),
            vec![
                Transition::AttemptStarted,
                Transition::AttemptFailed,
                Transition::Cancelled,
            ]
        );
        assert_eq!(events.last().expect("cancelled").progress, error.failure.progress);
    }
    for provider in [Provider::Ollama, Provider::Omlx, Provider::OpenAi] {
        let body = format!(
            "{}{}{}",
            frame(provider, "content"),
            frame(provider, "reasoning"),
            frame(provider, "tools")
        );
        let server = fixture(vec![(200, body, true, true)]).await;
        match provider {
            Provider::Ollama => {
                check(OllamaGateway::with_host(&server.url), provider, &server).await
            }
            Provider::Omlx => check(omlx(&server.url), provider, &server).await,
            Provider::OpenAi => {
                check(
                    OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
                    provider,
                    &server,
                )
                .await
            }
        }
    }
}

#[tokio::test]
async fn cancellation_active_ordinary_retains_headers_raw_and_no_resend() {
    let server =
        fixture(vec![(200, "{\"message\":{\"content\":\"partial".into(), true, true)]).await;
    let capture = Arc::new(Capture::default());
    let gateway = OllamaGateway::with_host(&server.url)
        .with_recovery(policy(3, &capture))
        .expect("gateway");
    let call = RecoveryCall::default();
    let cancel = call.cancellation.clone();
    let s = server.sends.clone();
    tokio::spawn(async move {
        while s.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();
    });
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        gateway.complete_with_recovery("qwen3", &[], None, &CompletionConfig::default(), &call),
    )
    .await
    .expect("prompt")
    .err()
    .expect("cancel");
    assert_eq!(error.outcome, Outcome::Cancelled);
    assert_eq!(error.failure.http_status, Some(200));
    assert!(error.failure.progress.headers_received);
    assert!(error.failure.progress.raw_bytes > 0);
    assert_eq!(error.history.len(), 1);
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn partial_http200_body_and_malformed_protocol_never_replayed() {
    for (body, partial) in [
        ("{\"message\":{\"content\":\"half".to_string(), true),
        ("not JSON".to_string(), false),
    ] {
        let server = fixture(vec![(200, body, partial, false)]).await;
        let capture = Arc::new(Capture::default());
        let gateway = OllamaGateway::with_host(&server.url)
            .with_recovery(policy(3, &capture))
            .expect("gateway");
        let error = gateway
            .complete_with_recovery(
                "qwen3",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await
            .err()
            .expect("failure");
        assert_eq!(error.outcome, Outcome::Failed);
        assert!(!error.failure.classification.eligible);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn immutable_reasoning_tools_schema_and_sampling_all_adapters() {
    async fn check<G: RecoveryProvider>(inner: G, server: &HttpFixture, reasoning_field: &str) {
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(2, &capture)).expect("gateway");
        let mut message = LlmMessage::assistant("prior");
        message.reasoning_content = Some("native prior reasoning".into());
        let config = CompletionConfig {
            temperature: Some(0.5),
            top_p: Some(0.7),
            max_tokens: Some(123),
            reasoning_effort: Some(mojentic::llm::gateway::ReasoningEffort::Enabled),
            ..Default::default()
        };
        let result = gateway
            .complete_json_with_recovery(
                "qwen3",
                &[message, LlmMessage::user("next")],
                json!({"type":"object","properties":{"ok":{"type":"boolean"}}}),
                &config,
                &RecoveryCall::default(),
            )
            .await
            .expect("structured");
        assert_eq!(result.value.object, Some(json!({"ok":true})));
        let bodies = server.bodies.lock().expect("bodies");
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0], bodies[1]);
        let body: Value = serde_json::from_slice(&bodies[0]).expect("json");
        assert_eq!(body["messages"][0][reasoning_field], "native prior reasoning");
        assert_eq!(result.report.history.len(), 1);
    }
    let server = fixture(vec![
        (503, "fail".into(), false, false),
        (200, response(Provider::Ollama, "{\"ok\":true}").to_string(), false, false),
    ])
    .await;
    check(OllamaGateway::with_host(&server.url), &server, "thinking").await;
    let server = fixture(vec![
        (503, "fail".into(), false, false),
        (200, response(Provider::Omlx, "{\"ok\":true}").to_string(), false, false),
    ])
    .await;
    check(omlx(&server.url), &server, "reasoning_content").await;
    // OpenAI deliberately rejects native reasoning replay: preserve its prototype capability boundary.
    let server = fixture(vec![
        (503, "fail".into(), false, false),
        (200, response(Provider::OpenAi, "{\"ok\":true}").to_string(), false, false),
    ])
    .await;
    let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url)
        .with_recovery(policy(2, &Arc::new(Capture::default())))
        .expect("gateway");
    let result = gateway
        .complete_json_with_recovery(
            "gpt-4o",
            &[LlmMessage::user("next")],
            json!({"type":"object"}),
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .await
        .expect("structured");
    assert_eq!(result.value.object, Some(json!({"ok":true})));
    let bodies = server.bodies.lock().expect("bodies");
    assert_eq!(bodies[0], bodies[1]);
}

#[tokio::test]
async fn no_newline_semantic_or_truncated_frames_never_replay() {
    async fn check<G: RecoveryProvider>(inner: G, server: &HttpFixture) {
        let gateway =
            RecoveryGateway::new(inner, policy(3, &Arc::new(Capture::default()))).expect("gateway");
        let call = RecoveryCall::default();
        let mut stream = gateway.complete_stream_with_recovery(
            "qwen3",
            &[],
            None,
            &CompletionConfig::default(),
            &call,
        );
        let mut failure = None;
        while let Some(event) = stream.next().await {
            if let RecoveryStreamEvent::Failed(error) = event {
                failure = Some(error);
            }
        }
        let error = failure.expect("fail closed");
        assert!(!error.failure.classification.eligible);
        assert_eq!(error.history.len(), 1);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert!(!error.failure.inspect_response_bytes().is_empty());
    }
    for partial in [false, true] {
        for kind in ["content", "reasoning", "tools"] {
            let server = fixture(vec![(
                200,
                frame(Provider::Ollama, kind).trim_end().into(),
                partial,
                false,
            )])
            .await;
            check(OllamaGateway::with_host(&server.url), &server).await;
            let server =
                fixture(vec![(200, frame(Provider::Omlx, kind).trim_end().into(), partial, false)])
                    .await;
            check(omlx(&server.url), &server).await;
            let server = fixture(vec![(
                200,
                frame(Provider::OpenAi, kind).trim_end().into(),
                partial,
                false,
            )])
            .await;
            check(
                OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
                &server,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn provider_response_category_codes_and_causes_all_adapters() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &mut mockito::ServerGuard,
    ) {
        let mock = server
            .mock("POST", path(provider))
            .with_body(r#"{"error":{"code":"rate_limit_exceeded","message":"synthetic-secret"}}"#)
            .expect(1)
            .create_async()
            .await;
        let gateway =
            RecoveryGateway::new(inner, policy(3, &Arc::new(Capture::default()))).expect("gateway");
        let error = gateway
            .complete_with_recovery(
                "qwen3",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await
            .err()
            .expect("provider error");
        assert_eq!(error.failure.category, Category::ProviderResponse);
        assert_eq!(
            error.failure.provider_code.as_ref().expect("recognized code").as_str(),
            "rate_limit_exceeded"
        );
        assert!(!error.failure.classification.eligible);
        assert!(error
            .failure
            .inspect_cause()
            .expect("cause")
            .to_string()
            .contains("synthetic-secret"));
        assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
        mock.assert_async().await;
    }
    let mut server = mockito::Server::new_async().await;
    check(OllamaGateway::with_host(server.url()), Provider::Ollama, &mut server).await;
    let mut server = mockito::Server::new_async().await;
    check(omlx(&server.url()), Provider::Omlx, &mut server).await;
    let mut server = mockito::Server::new_async().await;
    check(
        OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url()),
        Provider::OpenAi,
        &mut server,
    )
    .await;
}

#[tokio::test]
async fn budgets_retry_after_and_exact_exponential_delays() {
    for budget in [Some(Duration::from_secs(2)), Some(Duration::ZERO), None] {
        let mut server = mockito::Server::new_async().await;
        let attempts = if budget.is_some() { 1 } else { 4 };
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(504)
            .with_header("retry-after", if budget.is_some() { "2" } else { "0" })
            .expect(attempts)
            .create_async()
            .await;
        let clock = Arc::new(Clock::default());
        let capture = Arc::new(Capture::default());
        let mut p = policy(4, &capture);
        p.budget = budget;
        p.base_delay = Duration::from_secs(1);
        p.timing = clock.clone();
        let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url())
            .with_recovery(p)
            .expect("gateway");
        let error = gateway
            .complete_with_recovery(
                "gpt-4o",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await
            .err()
            .expect("exhausted");
        assert_eq!(
            error.outcome,
            if budget.is_some() {
                Outcome::BudgetExpired
            } else {
                Outcome::Exhausted
            }
        );
        assert_eq!(error.history.len(), attempts);
        assert_eq!(
            *clock.sleeps.lock().expect("sleep"),
            if budget.is_some() {
                vec![]
            } else {
                vec![
                    Duration::from_millis(500),
                    Duration::from_secs(1),
                    Duration::from_secs(2),
                ]
            }
        );
        mock.assert_async().await;
    }
}

struct DecideLater {
    signal: tokio::sync::Notify,
}
#[async_trait]
impl Admission for DecideLater {
    async fn decide(&self, _: AdmissionContext) -> AdmissionDecision {
        self.signal.notified().await;
        AdmissionDecision::Allow
    }
}
#[tokio::test]
async fn pending_is_not_permission_then_explicit_allow_dispatches() {
    let server = fixture(vec![
        (504, "failure".into(), false, false),
        (200, response(Provider::Ollama, "ok").to_string(), false, false),
    ])
    .await;
    let capture = Arc::new(Capture::default());
    let admission = Arc::new(DecideLater {
        signal: tokio::sync::Notify::new(),
    });
    let mut p = policy(2, &capture);
    p.admission = Some(admission.clone());
    let gateway = OllamaGateway::with_host(&server.url).with_recovery(p).expect("gateway");
    let config = CompletionConfig::default();
    let call = RecoveryCall::default();
    let future = gateway.complete_with_recovery("qwen3", &[], None, &config, &call);
    tokio::pin!(future);
    tokio::select! {result=&mut future=>panic!("pending completed: {}",result.is_ok()),_ = async {loop {if capture.events.lock().expect("events").iter().any(|e|e.transition==Transition::AdmissionPending){break;}tokio::task::yield_now().await;}}=>{}}
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    admission.signal.notify_one();
    let result = future.await.expect("explicitly allowed");
    assert_eq!(result.report.identity.wire_attempt, 2);
}
struct CancelOnStart(tokio_util::sync::CancellationToken);
impl LifecycleObserver for CancelOnStart {
    fn observe(&self, event: &LifecycleEvent) {
        if event.transition == Transition::AttemptStarted {
            self.0.cancel();
        }
    }
}
#[tokio::test]
async fn cancellation_before_dispatch_has_no_phantom_wire_attempt() {
    let mut server = mockito::Server::new_async().await;
    let mock = server.mock("POST", "/api/chat").expect(0).create_async().await;
    let call = RecoveryCall::default();
    let p = RecoveryPolicy {
        observer: Some(Arc::new(CancelOnStart(call.cancellation.clone()))),
        ..Default::default()
    };
    let gateway = OllamaGateway::with_host(server.url()).with_recovery(p).expect("gateway");
    let error = gateway
        .complete_with_recovery("qwen3", &[], None, &CompletionConfig::default(), &call)
        .await
        .err()
        .expect("cancelled");
    assert_eq!(error.outcome, Outcome::Cancelled);
    assert!(error.history.is_empty());
    assert!(error.failure.identity.is_none());
    mock.assert_async().await;
}
struct BrokenCapture {
    at_request: bool,
}
impl WireObserver for BrokenCapture {
    fn observe(
        &self,
        event: WireEvent<'_>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if (self.at_request && matches!(event, WireEvent::Request { .. }))
            || (!self.at_request && matches!(event, WireEvent::Headers { .. }))
        {
            Err(Box::new(std::io::Error::other("synthetic-secret capture failure")))
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn capture_failure_is_terminal_accounted_and_preserves_original_cause() {
    let mut server = mockito::Server::new_async().await;
    let mock = server.mock("POST", "/api/chat").with_status(503).expect(1).create_async().await;
    let capture = Arc::new(Capture::default());
    let mut p = policy(3, &capture);
    p.wire_observer = Some(Arc::new(BrokenCapture { at_request: false }));
    let gateway = OllamaGateway::with_host(server.url()).with_recovery(p).expect("gateway");
    let error = gateway
        .complete_with_recovery(
            "qwen3",
            &[],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .await
        .err()
        .expect("capture failure");
    assert_eq!(error.outcome, Outcome::ObserverFailed);
    assert_eq!(error.history.len(), 1);
    assert!(error
        .failure
        .inspect_cause()
        .expect("cause")
        .downcast_ref::<std::io::Error>()
        .is_some());
    assert!(capture
        .events
        .lock()
        .expect("events")
        .iter()
        .any(|e| e.transition == Transition::AttemptFailed));
    assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
    mock.assert_async().await;
}

#[tokio::test]
async fn public_broker_and_session_preserve_structured_failure_and_history() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/api/chat")
        .with_status(504)
        .with_body("synthetic-secret")
        .expect(2)
        .create_async()
        .await;
    let gateway = OllamaGateway::with_host(server.url())
        .with_recovery(RecoveryPolicy::default())
        .expect("gateway");
    let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
    let error = broker
        .generate(&[LlmMessage::user("question")], None, None, None)
        .await
        .expect_err("failure");
    let mojentic::MojenticError::Recovery(error) = error else {
        panic!("structured error")
    };
    assert_eq!(error.failure.http_status, Some(504));
    assert_eq!(error.history.len(), 1);
    assert!(error.failure.progress.headers_received);
    assert_eq!(error.failure.progress.raw_bytes, "synthetic-secret".len() as u64);
    assert_eq!(error.history[0].progress, error.failure.progress);
    assert_eq!(error.history[0].identity, error.failure.identity);
    assert_eq!(error.failure.inspect_response_bytes(), b"synthetic-secret");
    assert!(error.failure.inspect_cause().is_some());
    let mut session = mojentic::llm::ChatSession::new(broker);
    let error = session.send("question").await.expect_err("session failure");
    let mojentic::MojenticError::Recovery(error) = error else {
        panic!("structured error")
    };
    assert_eq!(error.failure.http_status, Some(504));
    assert_eq!(error.history.len(), 1);
    assert!(error.failure.progress.headers_received);
    assert_eq!(error.failure.progress.raw_bytes, "synthetic-secret".len() as u64);
    assert_eq!(error.history[0].progress, error.failure.progress);
    assert_eq!(error.history[0].identity, error.failure.identity);
    assert_eq!(error.failure.inspect_response_bytes(), b"synthetic-secret");
    assert!(error.failure.inspect_cause().is_some());
    assert_eq!(
        session.messages().last().expect("user retained").role(),
        mojentic::llm::MessageRole::User
    );
    mock.assert_async().await;
}

#[tokio::test]
async fn native_reasoning_streamed_tool_turn_is_retained_and_tool_runs_once() {
    let body = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":{"reasoning_content":"native prior reasoning","tool_calls":[{"index":0,"id":"call1","function":{"name":"count","arguments":"{}"}}]},"finish_reason":null}]}),
        json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]})
    );
    let server = fixture(vec![
        (200, body, false, false),
        (503, "failure".into(), false, false),
        (200, final_stream(Provider::Omlx), false, false),
    ])
    .await;
    let capture = Arc::new(Capture::default());
    let gateway = omlx(&server.url).with_recovery(policy(2, &capture)).expect("gateway");
    let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
    let count = Arc::new(AtomicUsize::new(0));
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(CountTool(count.clone()))];
    let messages = [LlmMessage::user("tool")];
    let config = CompletionConfig {
        preserve_reasoning: true,
        max_tool_iterations: 2,
        ..Default::default()
    };
    let mut stream = broker.generate_stream(&messages, Some(&tools), Some(config), None);
    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        text.push_str(&chunk.expect("complete stream"));
    }
    assert_eq!(text, "done");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let bodies = server.bodies.lock().expect("bodies");
    assert_eq!(bodies.len(), 3);
    assert_eq!(bodies[1], bodies[2]);
    let second: Value = serde_json::from_slice(&bodies[1]).expect("request");
    assert_eq!(second["messages"][1]["reasoning_content"], "native prior reasoning");
}

#[tokio::test]
async fn streaming_503_success_preserves_payload_and_accounting_all_adapters() {
    async fn check<G: RecoveryProvider>(inner: G, server: &HttpFixture) {
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(2, &capture)).expect("gateway");
        let call = RecoveryCall::default();
        let mut stream = gateway.complete_stream_with_recovery(
            "qwen3",
            &[LlmMessage::user("synthetic prompt")],
            None,
            &CompletionConfig::default(),
            &call,
        );
        let mut text = String::new();
        let mut report = None;
        while let Some(event) = stream.next().await {
            match event {
                RecoveryStreamEvent::Content(s) => text.push_str(&s),
                RecoveryStreamEvent::Completed(r) => report = Some(r.report),
                RecoveryStreamEvent::Failed(e) => panic!("{e}"),
                _ => {}
            }
        }
        assert_eq!(text, "done");
        let report = report.expect("complete");
        assert_eq!(report.identity.wire_attempt, 2);
        assert_eq!(report.history.len(), 1);
        assert_eq!(report.history[0].http_status, Some(503));
        assert_eq!(report.progress.observed, report.progress.delivered);
        let bodies = server.bodies.lock().expect("bodies");
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0], bodies[1]);
    }
    let server = fixture(vec![
        (503, "failure".into(), false, false),
        (200, final_stream(Provider::Ollama), false, false),
    ])
    .await;
    check(OllamaGateway::with_host(&server.url), &server).await;
    let server = fixture(vec![
        (503, "failure".into(), false, false),
        (200, final_stream(Provider::Omlx), false, false),
    ])
    .await;
    check(omlx(&server.url), &server).await;
    let server = fixture(vec![
        (503, "failure".into(), false, false),
        (200, final_stream(Provider::OpenAi), false, false),
    ])
    .await;
    check(
        OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
        &server,
    )
    .await;
}

#[tokio::test]
async fn native_history_tool_schema_combination_is_frozen_all_adapters() {
    async fn check<G: RecoveryProvider>(inner: G, provider: Provider, server: &HttpFixture) {
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(2, &capture)).expect("gateway");
        let mut history = LlmMessage::assistant("previous");
        history.reasoning_content = Some("native trace".into());
        let messages = [history, LlmMessage::user("next")];
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(CountTool(Arc::new(AtomicUsize::new(0))))];
        let config = CompletionConfig {
            temperature: Some(0.5),
            top_p: Some(0.6),
            max_tokens: Some(123),
            response_format: Some(mojentic::llm::gateway::ResponseFormat::JsonObject {
                schema: Some(json!({"type":"object"})),
            }),
            ..Default::default()
        };
        gateway
            .complete_with_recovery(
                "gpt-4o",
                &messages,
                Some(&tools),
                &config,
                &RecoveryCall::default(),
            )
            .await
            .expect("recover");
        let bodies = server.bodies.lock().expect("bodies");
        assert_eq!(bodies[0], bodies[1]);
        let body: Value = serde_json::from_slice(&bodies[0]).expect("json");
        assert_eq!(body["tools"][0]["function"]["name"], "count");
        assert_eq!(body["messages"][0]["content"], "previous");
        match provider {
            Provider::Ollama => {
                assert_eq!(body["messages"][0]["thinking"], "native trace");
                assert_eq!(body["format"], json!({"type":"object"}));
            }
            Provider::Omlx => {
                assert_eq!(body["messages"][0]["reasoning_content"], "native trace");
                assert!(body["response_format"].is_object());
            }
            Provider::OpenAi => {
                assert!(body["messages"][0]["reasoning_content"].is_null());
                assert!(body["response_format"].is_object());
            }
        }
    }
    let server = fixture(vec![
        (503, "failure".into(), false, false),
        (200, response(Provider::Ollama, "{}").to_string(), false, false),
    ])
    .await;
    check(OllamaGateway::with_host(&server.url), Provider::Ollama, &server).await;
    let server = fixture(vec![
        (503, "failure".into(), false, false),
        (200, response(Provider::Omlx, "{}").to_string(), false, false),
    ])
    .await;
    check(omlx(&server.url), Provider::Omlx, &server).await;
    let server = fixture(vec![
        (503, "failure".into(), false, false),
        (200, response(Provider::OpenAi, "{}").to_string(), false, false),
    ])
    .await;
    check(
        OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
        Provider::OpenAi,
        &server,
    )
    .await;
}
#[tokio::test]
async fn session_interruption_never_commits_partial_assistant_or_executes_tools() {
    let server = fixture(vec![(
        200,
        format!("{}{}", frame(Provider::Omlx, "content"), frame(Provider::Omlx, "tools")),
        true,
        false,
    )])
    .await;
    let gateway = omlx(&server.url)
        .with_recovery(policy(3, &Arc::new(Capture::default())))
        .expect("gateway");
    let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
    let count = Arc::new(AtomicUsize::new(0));
    let mut session = mojentic::llm::ChatSession::builder(broker)
        .tools(vec![Box::new(CountTool(count.clone()))])
        .build();
    let mut stream = session.send_stream("question");
    let mut error = None;
    while let Some(chunk) = stream.next().await {
        if let Err(e) = chunk {
            error = Some(e);
        }
    }
    drop(stream);
    let mojentic::MojenticError::Recovery(error) = error.expect("interrupted") else {
        panic!("structured error")
    };
    assert_eq!(error.outcome, Outcome::Interrupted);
    assert!(error.failure.progress.observed.tool_fragments > 0);
    assert_eq!(error.history[0].progress.delivered, error.failure.progress.delivered);
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(
        session.messages().last().expect("user").role(),
        mojentic::llm::MessageRole::User
    );
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn permanent_and_bounded_errors_all_adapters() {
    async fn check<G: RecoveryProvider>(inner: G, status: u16, server: &HttpFixture) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(2, &capture);
        p.retryable_statuses.extend([400, 401]);
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let error = gateway
            .complete_with_recovery(
                "qwen3",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await
            .err()
            .expect("failure");
        let expected = if status == 504 { 2 } else { 1 };
        assert_eq!(server.sends.load(Ordering::SeqCst), expected);
        assert_eq!(error.history.len(), expected);
        assert_eq!(error.failure.http_status, Some(status));
        assert_eq!(
            error.outcome,
            if status == 504 {
                Outcome::Exhausted
            } else {
                Outcome::Failed
            }
        );
        for (n, failure) in error.history.iter().enumerate() {
            assert_eq!(failure.identity.as_ref().expect("id").wire_attempt, n as u32 + 1);
        }
    }
    for status in [400, 401, 504] {
        let replies = vec![
            (status, "synthetic-secret".into(), false, false);
            if status == 504 { 2 } else { 1 }
        ];
        let server = fixture(replies.clone()).await;
        check(OllamaGateway::with_host(&server.url), status, &server).await;
        let server = fixture(replies.clone()).await;
        check(omlx(&server.url), status, &server).await;
        let server = fixture(replies).await;
        check(
            OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
            status,
            &server,
        )
        .await;
    }
}

struct AdvanceDuringActive {
    clock: Arc<Clock>,
    headers: AtomicUsize,
}
impl WireObserver for AdvanceDuringActive {
    fn observe(
        &self,
        event: WireEvent<'_>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if matches!(event, WireEvent::Headers { .. })
            && self.headers.fetch_add(1, Ordering::SeqCst) == 1
        {
            *self.clock.elapsed.lock().expect("clock") = Duration::from_secs(10);
        }
        Ok(())
    }
}
#[tokio::test]
async fn recovery_budget_and_provider_timeout_do_not_cut_off_admitted_generation() {
    let server = fixture(vec![
        (503, "failure".into(), false, false),
        (200, response(Provider::Ollama, "done").to_string(), false, false),
    ])
    .await;
    let clock = Arc::new(Clock::default());
    let mut p = policy(2, &Arc::new(Capture::default()));
    p.timing = clock.clone();
    p.budget = Some(Duration::from_secs(1));
    p.wire_observer = Some(Arc::new(AdvanceDuringActive {
        clock: clock.clone(),
        headers: AtomicUsize::new(0),
    }));
    let gateway = OllamaGateway::with_config(mojentic::llm::gateways::OllamaConfig {
        host: server.url.clone(),
        timeout: Some(Duration::from_nanos(1)),
        ..Default::default()
    })
    .with_recovery(p)
    .expect("gateway");
    let result = gateway
        .complete_with_recovery(
            "qwen3",
            &[],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .await
        .expect("active request may finish after recovery budget");
    assert_eq!(result.value.content.as_deref(), Some("done"));
    assert_eq!(result.report.identity.wire_attempt, 2);
    assert_eq!(*clock.elapsed.lock().expect("clock"), Duration::from_secs(10));
}
#[tokio::test]
async fn absolute_deadline_refuses_retry_without_timing_out_initial_request() {
    let server = fixture(vec![(504, "failure".into(), false, false)]).await;
    let clock = Arc::new(Clock::default());
    let mut p = policy(3, &Arc::new(Capture::default()));
    p.deadline = Some(clock.start);
    p.timing = clock;
    let gateway = OllamaGateway::with_host(&server.url).with_recovery(p).expect("gateway");
    let error = gateway
        .complete_with_recovery(
            "qwen3",
            &[],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .await
        .err()
        .expect("deadline");
    assert_eq!(error.outcome, Outcome::BudgetExpired);
    assert_eq!(error.history.len(), 1);
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn unsupported_reasoning_rejected_before_send_and_capabilities_truthful() {
    let mut server = mockito::Server::new_async().await;
    let mock = server.mock("POST", "/chat/completions").expect(0).create_async().await;
    let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url())
        .with_recovery(RecoveryPolicy::default())
        .expect("gateway");
    let config = CompletionConfig {
        preserve_reasoning: true,
        ..Default::default()
    };
    let error = gateway
        .complete_with_recovery("gpt-4o", &[], None, &config, &RecoveryCall::default())
        .await
        .err()
        .expect("unsupported preservation");
    assert_eq!(error.failure.acceptance, Acceptance::No);
    assert_eq!(error.failure.classification.reason, Reason::InvalidConfiguration);
    assert!(error.failure.identity.is_none());
    assert!(error.history.is_empty());
    let capabilities = gateway.capabilities();
    assert_eq!(capabilities.remote_request_cancellation, Support::Unsupported);
    assert_eq!(capabilities.request_status, Support::Unsupported);
    assert_eq!(capabilities.idempotency, Support::Unsupported);
    mock.assert_async().await;
}

#[tokio::test]
async fn predispatch_capture_failure_has_zero_wire_attempts() {
    let mut server = mockito::Server::new_async().await;
    let mock = server.mock("POST", "/api/chat").expect(0).create_async().await;
    let p = RecoveryPolicy {
        wire_observer: Some(Arc::new(BrokenCapture { at_request: true })),
        ..Default::default()
    };
    let gateway = OllamaGateway::with_host(server.url()).with_recovery(p).expect("gateway");
    let error = gateway
        .complete_with_recovery(
            "qwen3",
            &[],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .await
        .err()
        .expect("capture failed");
    assert_eq!(error.outcome, Outcome::ObserverFailed);
    assert!(error.history.is_empty());
    assert!(error.failure.identity.is_none());
    assert!(error
        .failure
        .inspect_cause()
        .expect("original cause")
        .downcast_ref::<std::io::Error>()
        .is_some());
    mock.assert_async().await;
}

#[tokio::test]
async fn rfc850_century_is_resolved_before_weekday_and_floor_validation() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(429)
        .with_header("retry-after", "Wednesday, 06-Nov-75 00:00:00 GMT")
        .expect(1)
        .create_async()
        .await;
    let wall_seconds = chrono::DateTime::parse_from_rfc3339("2026-10-10T00:00:00Z")
        .expect("fixture clock")
        .timestamp() as u64;
    let clock = Arc::new(Clock {
        wall_seconds,
        ..Default::default()
    });
    let mut p = policy(2, &Arc::new(Capture::default()));
    p.timing = clock.clone();
    let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", server.url())
        .with_recovery(p)
        .expect("gateway");
    let error = gateway
        .complete_with_recovery(
            "gpt-4o",
            &[],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .await
        .err()
        .expect("future minimum refused");
    assert_eq!(error.outcome, Outcome::DelayCeiling);
    let RetryAfter::Date {
        unix_seconds,
        remaining,
    } = error.failure.retry_after
    else {
        panic!("valid HTTP date")
    };
    assert_eq!(
        chrono::DateTime::from_timestamp(unix_seconds, 0)
            .expect("date")
            .format("%Y")
            .to_string(),
        "2075"
    );
    assert!(remaining > Duration::from_secs(30));
    assert!(clock.sleeps.lock().expect("sleeps").is_empty());
    mock.assert_async().await;
}

#[tokio::test]
async fn recovery_preserves_omlx_structured_warning_evidence_without_logging_it() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_header("warning", "199 oMLX schema fallback")
        .with_body(response(Provider::Omlx, "{}").to_string())
        .expect(1)
        .create_async()
        .await;
    let gateway = omlx(&server.url()).with_recovery(RecoveryPolicy::default()).expect("gateway");
    let result = mojentic::llm::gateway::LlmGateway::complete_json_response(
        &gateway,
        "qwen3",
        &[],
        json!({"type":"object"}),
        &CompletionConfig::default(),
    )
    .await
    .expect("structured success");
    assert_eq!(
        result.evidence.metadata[mojentic::llm::gateways::omlx::RESPONSE_FORMAT_WARNING],
        "199 oMLX schema fallback"
    );
    mock.assert_async().await;
}

#[tokio::test]
async fn uuid_shaped_credential_or_prompt_echo_is_omitted_from_safe_metadata() {
    for credential in [
        "550e8400-e29b-41d4-a716-446655440000",
        "550E8400-E29B-41D4-A716-446655440000",
        "550e8400e29b41d4a716446655440000",
    ] {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .with_status(503)
            .with_header("x-request-id", &format!("req_{credential}"))
            .expect(1)
            .create_async()
            .await;
        let gateway = OpenAIGateway::with_api_key_and_base_url(credential, server.url())
            .with_recovery(RecoveryPolicy::default())
            .expect("gateway");
        let error = gateway
            .complete_with_recovery(
                "gpt-4o",
                &[LlmMessage::user(credential)],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .await
            .err()
            .expect("failure");
        assert!(error.failure.provider_request_id.is_none());
        assert!(!serde_json::to_string(&error).expect("safe metadata").contains(credential));
        assert!(error
            .failure
            .inspect_response_headers()
            .expect("explicit header inspection")
            .get("x-request-id")
            .is_some());
        mock.assert_async().await;
    }
}

#[tokio::test]
async fn truncated_permanent_http_bodies_never_retry_all_public_completion_apis() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        status: u16,
        operation: Operation,
        server: &HttpFixture,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        // Even explicit eligibility and admission cannot override permanent status.
        p.retryable_statuses.extend([400, 401, 403]);
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let messages = [LlmMessage::user("synthetic-secret prompt")];
        let config = CompletionConfig::default();
        let call = RecoveryCall::default();
        let error = match operation {
            Operation::Ordinary => gateway
                .complete_with_recovery("qwen3", &messages, None, &config, &call)
                .await
                .err()
                .expect("permanent ordinary failure"),
            Operation::Structured => gateway
                .complete_json_with_recovery(
                    "qwen3",
                    &messages,
                    json!({"type":"object"}),
                    &config,
                    &call,
                )
                .await
                .err()
                .expect("permanent structured failure"),
            Operation::Streaming => {
                let mut stream =
                    gateway.complete_stream_with_recovery("qwen3", &messages, None, &config, &call);
                let error = match stream.next().await.expect("terminal failure") {
                    RecoveryStreamEvent::Failed(error) => error,
                    _ => panic!("permanent HTTP error must not produce semantic output"),
                };
                assert!(stream.next().await.is_none());
                error
            }
        };
        assert_eq!(error.outcome, Outcome::Failed);
        assert_eq!(error.failure.category, Category::Http);
        assert_eq!(error.failure.http_status, Some(status));
        assert_eq!(error.failure.operation, operation);
        assert_eq!(error.failure.classification.reason, Reason::Permanent);
        assert!(!error.failure.classification.eligible);
        assert!(error.failure.progress.headers_received);
        assert!(error.failure.progress.raw_bytes > 0);
        assert!(!error.failure.progress.observed.any());
        assert!(!error.failure.progress.delivered.any());
        assert!(error
            .failure
            .inspect_cause()
            .expect("body transport cause")
            .downcast_ref::<reqwest::Error>()
            .is_some());
        assert_eq!(error.failure.inspect_response_bytes(), b"synthetic-secret truncated body");
        assert_eq!(error.history.len(), 1);
        assert_eq!(error.history[0].http_status, Some(status));
        assert_eq!(error.history[0].identity.as_ref().expect("identity").wire_attempt, 1);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert_eq!(capture.requests.lock().expect("requests").len(), 1);
        let events = capture.events.lock().expect("events");
        assert_eq!(
            events.iter().map(|e| e.transition).collect::<Vec<_>>(),
            vec![Transition::AttemptStarted, Transition::AttemptFailed,]
        );
        for safe in [
            format!("{error}"),
            format!("{error:?}"),
            serde_json::to_string(&error).expect("safe error"),
            serde_json::to_string(&*events).expect("safe events"),
        ] {
            assert!(!safe.contains("synthetic-secret"));
        }
    }
    for status in [400, 401, 403] {
        for operation in [
            Operation::Ordinary,
            Operation::Structured,
            Operation::Streaming,
        ] {
            // A second scripted response makes an unauthorized resend observable.
            let replies = vec![(status, "synthetic-secret truncated body".into(), true, false); 3];
            let server = fixture(replies.clone()).await;
            check(OllamaGateway::with_host(&server.url), status, operation, &server).await;
            let server = fixture(replies.clone()).await;
            check(omlx(&server.url), status, operation, &server).await;
            let server = fixture(replies).await;
            check(
                OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
                status,
                operation,
                &server,
            )
            .await;
        }
    }
}

struct RejectBodyCapture;
impl WireObserver for RejectBodyCapture {
    fn observe(
        &self,
        event: WireEvent<'_>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if matches!(event, WireEvent::Body { .. }) {
            Err(Box::new(std::io::Error::other("synthetic-secret body capture failure")))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn body_capture_failure_retains_observed_semantics_without_delivery_all_adapters() {
    async fn check<G: RecoveryProvider>(inner: G, operation: Operation, server: &HttpFixture) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        p.wire_observer = Some(Arc::new(RejectBodyCapture));
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let call = RecoveryCall::default();
        let config = CompletionConfig::default();
        let error = match operation {
            Operation::Ordinary => gateway
                .complete_with_recovery("qwen3", &[], None, &config, &call)
                .await
                .err()
                .expect("capture failure"),
            Operation::Structured => gateway
                .complete_json_with_recovery("qwen3", &[], json!({"type":"object"}), &config, &call)
                .await
                .err()
                .expect("capture failure"),
            Operation::Streaming => {
                let mut stream =
                    gateway.complete_stream_with_recovery("qwen3", &[], None, &config, &call);
                let error = match stream.next().await.expect("failure") {
                    RecoveryStreamEvent::Failed(error) => error,
                    _ => panic!("failed capture must not deliver semantic output"),
                };
                assert!(stream.next().await.is_none());
                error
            }
        };
        assert_eq!(error.outcome, Outcome::ObserverFailed);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert_eq!(error.history.len(), 1);
        assert_eq!(error.history[0].progress, error.failure.progress);
        assert!(error.failure.progress.headers_received);
        assert!(error.failure.progress.raw_bytes > 0);
        assert!(error.failure.progress.observed.any());
        assert!(!error.failure.progress.delivered.any());
        assert!(!error.failure.inspect_response_bytes().is_empty());
        assert!(error
            .failure
            .inspect_cause()
            .expect("original cause")
            .downcast_ref::<std::io::Error>()
            .is_some());
        let events = capture.events.lock().expect("events");
        assert_eq!(events.last().expect("failed event").transition, Transition::AttemptFailed);
        assert_eq!(events.last().expect("failed event").progress, error.failure.progress);
        assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
        assert!(!serde_json::to_string(&error).expect("safe error").contains("synthetic-secret"));
        assert!(!serde_json::to_string(&*events)
            .expect("safe events")
            .contains("synthetic-secret"));
    }
    for provider in [Provider::Ollama, Provider::Omlx, Provider::OpenAi] {
        for operation in [
            Operation::Ordinary,
            Operation::Structured,
            Operation::Streaming,
        ] {
            let bodies = if operation == Operation::Streaming {
                ["content", "reasoning", "tools"].map(|kind| frame(provider, kind)).to_vec()
            } else {
                vec![response(provider, r#"{"synthetic-secret":"received"}"#).to_string()]
            };
            for body in bodies {
                let server = fixture(vec![(200, body, false, false)]).await;
                match provider {
                    Provider::Ollama => {
                        check(OllamaGateway::with_host(&server.url), operation, &server).await
                    }
                    Provider::Omlx => check(omlx(&server.url), operation, &server).await,
                    Provider::OpenAi => {
                        check(
                            OpenAIGateway::with_api_key_and_base_url(
                                "synthetic-secret",
                                &server.url,
                            ),
                            operation,
                            &server,
                        )
                        .await
                    }
                }
            }
        }
    }
}

struct CancelOnBody(tokio_util::sync::CancellationToken);
impl WireObserver for CancelOnBody {
    fn observe(
        &self,
        event: WireEvent<'_>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if matches!(event, WireEvent::Body { .. }) {
            self.0.cancel();
        }
        Ok(())
    }
}

async fn cancelled_body<G: RecoveryProvider>(
    inner: G,
    operation: Operation,
    server: &HttpFixture,
    body: &str,
    semantic: bool,
) {
    let capture = Arc::new(Capture::default());
    let call = RecoveryCall::default();
    let mut p = policy(3, &capture);
    p.wire_observer = Some(Arc::new(CancelOnBody(call.cancellation.clone())));
    let gateway = RecoveryGateway::new(inner, p).expect("gateway");
    let config = CompletionConfig::default();
    let error = tokio::time::timeout(Duration::from_secs(2), async {
        match operation {
            Operation::Ordinary => gateway
                .complete_with_recovery("qwen3", &[], None, &config, &call)
                .await
                .err()
                .expect("cancelled ordinary request"),
            Operation::Structured => gateway
                .complete_json_with_recovery("qwen3", &[], json!({"type":"object"}), &config, &call)
                .await
                .err()
                .expect("cancelled structured request"),
            Operation::Streaming => {
                let mut stream =
                    gateway.complete_stream_with_recovery("qwen3", &[], None, &config, &call);
                let error = match stream.next().await.expect("one terminal event") {
                    RecoveryStreamEvent::Failed(error) => error,
                    _ => panic!("cancelled body must never complete or deliver output"),
                };
                assert!(stream.next().await.is_none(), "exactly one terminal event");
                error
            }
        }
    })
    .await
    .expect("prompt local cancellation");
    assert_eq!(error.outcome, Outcome::Cancelled);
    assert_eq!(error.failure.category, Category::Cancellation);
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    assert_eq!(error.history.len(), 1);
    assert_eq!(error.history[0].identity, error.failure.identity);
    assert_eq!(error.history[0].progress, error.failure.progress);
    assert_eq!(error.failure.identity.as_ref().expect("actual send").wire_attempt, 1);
    assert_eq!(error.failure.http_status, Some(200));
    assert!(error.failure.progress.headers_received);
    assert_eq!(error.failure.progress.raw_bytes, body.len() as u64);
    assert_eq!(error.failure.inspect_response_bytes(), body.as_bytes());
    assert!(error.failure.inspect_response_headers().is_some());
    assert_eq!(error.failure.progress.observed.any(), semantic);
    assert!(!error.failure.progress.delivered.any());
    let events = capture.events.lock().expect("events");
    assert_eq!(
        events.iter().map(|e| e.transition).collect::<Vec<_>>(),
        vec![
            Transition::AttemptStarted,
            Transition::AttemptFailed,
            Transition::Cancelled,
        ]
    );
    for event in events.iter().skip(1) {
        assert_eq!(event.identity, error.failure.identity);
        assert_eq!(event.progress, error.failure.progress);
    }
    assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
    assert!(!serde_json::to_string(&error).expect("safe error").contains("synthetic-secret"));
    assert!(!serde_json::to_string(&*events)
        .expect("safe events")
        .contains("synthetic-secret"));
}

#[tokio::test]
async fn cancellation_terminal_only_body_all_adapters_never_succeeds() {
    for provider in [Provider::Ollama, Provider::Omlx, Provider::OpenAi] {
        let body = if provider == Provider::Ollama {
            "{\"message\":{},\"done\":true,\"done_reason\":\"stop\"}\n".to_string()
        } else {
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
                .to_string()
        };
        let server = fixture(vec![(200, body.clone(), false, false)]).await;
        match provider {
            Provider::Ollama => {
                cancelled_body(
                    OllamaGateway::with_host(&server.url),
                    Operation::Streaming,
                    &server,
                    &body,
                    false,
                )
                .await
            }
            Provider::Omlx => {
                cancelled_body(omlx(&server.url), Operation::Streaming, &server, &body, false).await
            }
            Provider::OpenAi => {
                cancelled_body(
                    OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
                    Operation::Streaming,
                    &server,
                    &body,
                    false,
                )
                .await
            }
        }
    }
}

#[tokio::test]
async fn cancellation_active_body_all_adapters_has_one_ordered_terminal_lifecycle() {
    for provider in [Provider::Ollama, Provider::Omlx, Provider::OpenAi] {
        for operation in [
            Operation::Ordinary,
            Operation::Structured,
            Operation::Streaming,
        ] {
            let bodies = if operation == Operation::Streaming {
                ["content", "reasoning", "tools"].map(|kind| frame(provider, kind)).to_vec()
            } else {
                vec![response(provider, r#"{"synthetic-secret":"received"}"#).to_string()]
            };
            for body in bodies {
                let server = fixture(vec![(200, body.clone(), true, true)]).await;
                match provider {
                    Provider::Ollama => {
                        cancelled_body(
                            OllamaGateway::with_host(&server.url),
                            operation,
                            &server,
                            &body,
                            true,
                        )
                        .await
                    }
                    Provider::Omlx => {
                        cancelled_body(omlx(&server.url), operation, &server, &body, true).await
                    }
                    Provider::OpenAi => {
                        cancelled_body(
                            OpenAIGateway::with_api_key_and_base_url(
                                "synthetic-secret",
                                &server.url,
                            ),
                            operation,
                            &server,
                            &body,
                            true,
                        )
                        .await
                    }
                }
                tokio::time::timeout(Duration::from_secs(2), async {
                    while server.closed.load(Ordering::SeqCst) == 0 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("locally owned response socket closed promptly");
            }
        }
    }
}

// Every case enters a maintained adapter's public API, never the retry engine.
macro_rules! adapter_case {
    ($check:ident, $provider:expr, $url:expr $(, $arg:expr)*) => {
        match $provider {
            Provider::Ollama => $check(OllamaGateway::with_host($url), $provider $(, $arg)*).await,
            Provider::Omlx => $check(omlx($url), $provider $(, $arg)*).await,
            Provider::OpenAi => $check(OpenAIGateway::with_api_key_and_base_url(
                "synthetic-secret", $url), $provider $(, $arg)*).await,
        }
    };
}
const ADAPTERS: [Provider; 3] = [Provider::Ollama, Provider::Omlx, Provider::OpenAi];

fn transitions(capture: &Capture) -> Vec<Transition> {
    capture.events.lock().expect("events").iter().map(|e| e.transition).collect()
}
fn assert_single_failure(error: &RecoveryError, capture: &Capture, provider: Provider) {
    assert_eq!(error.failure.provider, provider);
    assert_eq!(error.history.len(), 1);
    let prior = &error.history[0];
    assert_eq!(prior.identity, error.failure.identity);
    assert_eq!(prior.progress, error.failure.progress);
    if error.outcome != Outcome::Cancelled {
        assert_eq!(prior.category, error.failure.category);
    }
    assert_eq!(prior.inspect_response_bytes(), error.failure.inspect_response_bytes());
    assert_eq!(prior.inspect_response_headers(), error.failure.inspect_response_headers());
    let cause = error.failure.inspect_cause().expect("original cause");
    let prior_cause = prior.inspect_cause().expect("history cause");
    assert!(std::ptr::eq(cause, prior_cause), "cause identity retained");
    let requests = capture.requests.lock().expect("requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(Some(&requests[0].0), error.failure.identity.as_ref());
    assert_eq!(requests[0].0.wire_attempt, 1);
    assert!(!error.failure.remote_termination_confirmed);
}
fn assert_resend(report: &RecoveryReport, capture: &Capture) {
    assert_eq!(report.history.len(), 1);
    let requests = capture.requests.lock().expect("requests");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].1, requests[1].1, "immutable encoded payload");
    assert_eq!(requests[0].0.logical_request_id, requests[1].0.logical_request_id);
    assert_ne!(requests[0].0.attempt_id, requests[1].0.attempt_id);
    assert_eq!(requests[0].0.wire_attempt, 1);
    assert_eq!(requests[1].0.wire_attempt, 2);
    assert_eq!(report.history[0].identity.as_ref(), Some(&requests[0].0));
    assert_eq!(report.identity, requests[1].0);
    assert_eq!(
        transitions(capture),
        vec![
            Transition::AttemptStarted,
            Transition::AttemptFailed,
            Transition::AdmissionPending,
            Transition::AdmissionAllowed,
            Transition::DelayScheduled,
            Transition::RetryStarted,
            Transition::AttemptStarted,
            Transition::AttemptSucceeded,
        ]
    );
}

// A deterministic sleeping boundary that only cancellation can release.
struct BackoffWait;
#[async_trait]
impl Timing for BackoffWait {
    fn monotonic(&self) -> Instant {
        Instant::now()
    }
    fn wall(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }
    fn jitter(&self, ceiling: Duration) -> Duration {
        ceiling / 2
    }
    async fn sleep(&self, _: Duration) {
        std::future::pending().await
    }
}

async fn reviewed_operation<G: RecoveryProvider>(
    gateway: &RecoveryGateway<G>,
    operation: Operation,
    call: &RecoveryCall,
) -> std::result::Result<RecoveryReport, RecoveryError> {
    let messages = [LlmMessage::user("question")];
    let config = CompletionConfig::default();
    match operation {
        Operation::Ordinary => gateway
            .complete_with_recovery("qwen3", &messages, None, &config, call)
            .await
            .map(|r| r.report),
        Operation::Structured => gateway
            .complete_json_with_recovery(
                "qwen3",
                &messages,
                json!({"type":"object"}),
                &config,
                call,
            )
            .await
            .map(|r| r.report),
        Operation::Streaming => {
            let mut stream =
                gateway.complete_stream_with_recovery("qwen3", &messages, None, &config, call);
            let mut terminal = None;
            while let Some(event) = stream.next().await {
                match event {
                    RecoveryStreamEvent::Completed(result) => {
                        assert!(terminal.is_none());
                        terminal = Some(Ok(result.report));
                    }
                    RecoveryStreamEvent::Failed(error) => {
                        assert!(terminal.is_none());
                        terminal = Some(Err(error));
                    }
                    _ => {
                        assert!(terminal.is_none(), "no output after terminal");
                    }
                }
            }
            terminal.expect("one terminal result")
        }
    }
}
const OPERATIONS: [Operation; 3] = [
    Operation::Ordinary,
    Operation::Structured,
    Operation::Streaming,
];

#[tokio::test]
async fn review_retry_after_numeric_date_ceiling_budget_matrix() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &mut mockito::ServerGuard,
        header: &str,
        limit: &str,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let clock = Arc::new(Clock::default());
        let mut p = policy(2, &capture);
        p.timing = clock.clone();
        p.base_delay = Duration::from_secs(2);
        p.delay_ceiling = Duration::from_secs(if limit == "ceiling" { 1 } else { 3 });
        p.budget = Some(Duration::from_secs(if limit == "budget" { 2 } else { 3 }));
        let fail = server
            .mock("POST", path(provider))
            .with_status(429)
            .with_header("retry-after", header)
            .with_body("synthetic-secret failure")
            .expect(1)
            .create_async()
            .await;
        let done = server
            .mock("POST", path(provider))
            .with_body(if operation == Operation::Streaming {
                final_stream(provider)
            } else {
                response(
                    provider,
                    if operation == Operation::Structured {
                        "{}"
                    } else {
                        "ok"
                    },
                )
                .to_string()
            })
            .expect(usize::from(limit == "within"))
            .create_async()
            .await;
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let result = reviewed_operation(&gateway, operation, &RecoveryCall::default()).await;
        if limit == "within" {
            let result = result.expect("floor fits ceiling and budget");
            assert_resend(&result, &capture);
            assert_eq!(*clock.sleeps.lock().expect("sleeps"), vec![Duration::from_secs(2)]);
            assert_eq!(result.history[0].inspect_response_bytes(), b"synthetic-secret failure");
            assert!(!result.history[0].progress.observed.any());
        } else {
            let error = result.expect_err("floor cannot be shortened");
            assert_single_failure(&error, &capture, provider);
            assert_eq!(
                error.outcome,
                if limit == "ceiling" {
                    Outcome::DelayCeiling
                } else {
                    Outcome::BudgetExpired
                }
            );
            assert!(clock.sleeps.lock().expect("sleeps").is_empty());
            assert_eq!(
                transitions(&capture),
                vec![Transition::AttemptStarted, Transition::AttemptFailed]
            );
        }
        {
            let events = capture.events.lock().expect("events");
            let failure = events[1].failure.as_ref().expect("failed attempt");
            assert_eq!(
                failure.retry_after,
                if header == "2" {
                    RetryAfter::Seconds(2)
                } else {
                    RetryAfter::Date {
                        unix_seconds: 1_000_002,
                        remaining: Duration::from_secs(2),
                    }
                }
            );
            assert_eq!(failure.progress.raw_bytes, 24);
            assert_eq!(
                failure.inspect_cause().expect("HTTP cause").to_string(),
                "synthetic-secret failure"
            );
            assert_eq!(failure.inspect_response_bytes(), b"synthetic-secret failure");
            assert_eq!(failure.inspect_response_headers().expect("headers")["retry-after"], header);
            assert!(!failure.progress.observed.any());
            assert!(!failure.progress.delivered.any());
        }
        fail.assert_async().await;
        done.assert_async().await;
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            for header in ["2", "Mon, 12 Jan 1970 13:46:42 GMT"] {
                for limit in ["within", "ceiling", "budget"] {
                    let mut server = mockito::Server::new_async().await;
                    let url = server.url();
                    adapter_case!(check, provider, &url, &mut server, header, limit, operation);
                }
            }
        }
    }
}

#[tokio::test]
async fn review_pending_and_backoff_cancel_matrix() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        pending: bool,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        if pending {
            p.admission = Some(Arc::new(Pending));
        } else {
            p.base_delay = Duration::from_secs(10);
            p.timing = Arc::new(BackoffWait);
        }
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let call = RecoveryCall::default();
        let future = reviewed_operation(&gateway, operation, &call);
        tokio::pin!(future);
        let wait_for = if pending {
            Transition::AdmissionPending
        } else {
            Transition::DelayScheduled
        };
        tokio::time::timeout(Duration::from_secs(2), async {
            tokio::select! {
                result = &mut future => panic!("wait completed before cancellation: {}", result.is_ok()),
                _ = async { while !transitions(&capture).contains(&wait_for) { tokio::task::yield_now().await; } } => {}
            }
        }).await.expect("entered wait");
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        call.cancellation.cancel();
        let error = tokio::time::timeout(Duration::from_secs(2), future)
            .await
            .expect("cancel promptly")
            .expect_err("cancelled");
        assert_single_failure(&error, &capture, provider);
        assert_eq!(error.outcome, Outcome::Cancelled);
        assert_eq!(error.failure.category, Category::Cancellation);
        assert_eq!(error.history[0].category, Category::Http);
        assert_eq!(error.failure.http_status, Some(503));
        assert_eq!(error.failure.inspect_response_bytes(), b"synthetic-secret failure");
        assert_eq!(
            error.failure.inspect_cause().expect("cause").to_string(),
            "synthetic-secret failure"
        );
        assert_eq!(error.failure.progress.raw_bytes, 24);
        assert_fixture_headers(&error.failure, "synthetic-secret failure", false);
        if !pending {
            assert_eq!(
                capture.events.lock().expect("events")[4].delay,
                Some(Duration::from_secs(5))
            );
        }
        assert!(!error.failure.progress.observed.any());
        assert!(!error.failure.progress.delivered.any());
        let mut expected = vec![
            Transition::AttemptStarted,
            Transition::AttemptFailed,
            Transition::AdmissionPending,
        ];
        if !pending {
            expected.extend([Transition::AdmissionAllowed, Transition::DelayScheduled]);
        }
        expected.push(Transition::Cancelled);
        assert_eq!(transitions(&capture), expected);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            for pending in [true, false] {
                let server =
                    fixture(vec![(503, "synthetic-secret failure".into(), false, false)]).await;
                adapter_case!(check, provider, &server.url, &server, pending, operation);
            }
        }
    }
}

#[tokio::test]
async fn review_keepalive_and_partial_http200_matrix() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        keepalive: bool,
        body: &str,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(2, &capture)).expect("gateway");
        let call = RecoveryCall::default();
        if keepalive {
            let mut stream = gateway.complete_stream_with_recovery(
                "qwen3",
                &[LlmMessage::user("question")],
                None,
                &CompletionConfig::default(),
                &call,
            );
            let mut completed = None;
            while let Some(event) = stream.next().await {
                match event {
                    RecoveryStreamEvent::Completed(result) => {
                        assert!(completed.is_none());
                        completed = Some(result);
                    }
                    RecoveryStreamEvent::Failed(error) => panic!("keepalive recovery: {error}"),
                    RecoveryStreamEvent::Content(text) => assert_eq!(text, "done"),
                    RecoveryStreamEvent::Progress { .. } | RecoveryStreamEvent::Metrics { .. } => {
                        assert_eq!(provider, Provider::Ollama);
                    }
                    _ => panic!("unexpected semantic event"),
                }
            }
            let result = completed.expect("one completion");
            assert_resend(&result.report, &capture);
            let prior = &result.report.history[0];
            assert_eq!(prior.progress.raw_bytes, body.len() as u64);
            assert!(!prior.progress.observed.any());
            assert!(!prior.progress.delivered.any());
            assert_eq!(prior.inspect_response_bytes(), body.as_bytes());
            assert_eq!(
                prior.inspect_cause().expect("EOF cause").to_string(),
                "stream ended without terminal proof"
            );
            assert_eq!(server.sends.load(Ordering::SeqCst), 2);
            let bodies = server.bodies.lock().expect("bodies");
            assert_eq!(bodies[0], bodies[1]);
        } else {
            let error = reviewed_operation(&gateway, operation, &call)
                .await
                .expect_err("partial HTTP200");
            assert_single_failure(&error, &capture, provider);
            assert_eq!(error.outcome, Outcome::Failed);
            assert!(!error.failure.classification.eligible);
            assert_eq!(error.failure.http_status, Some(200));
            assert_eq!(error.failure.progress.raw_bytes, body.len() as u64);
            assert_eq!(error.failure.inspect_response_bytes(), body.as_bytes());
            assert!(!error.failure.progress.delivered.any());
            assert_eq!(
                transitions(&capture),
                vec![Transition::AttemptStarted, Transition::AttemptFailed]
            );
            assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        }
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            if operation == Operation::Streaming {
                continue;
            }
            for keepalive in [true, false] {
                if keepalive && operation != Operation::Ordinary {
                    continue;
                }
                let body = if keepalive {
                    if provider == Provider::Ollama {
                        "{\"message\":{},\"done\":false}\n"
                    } else {
                        ": keepalive\n\n"
                    }
                } else if provider == Provider::Ollama {
                    "{\"message\":{\"content\":\"half"
                } else {
                    "{\"choices\":[{\"message\":{\"content\":\"half"
                };
                let replies = if keepalive {
                    vec![
                        (200, body.into(), false, false),
                        (200, final_stream(provider), false, false),
                    ]
                } else {
                    vec![(200, body.into(), true, false)]
                };
                let server = fixture(replies).await;
                adapter_case!(check, provider, &server.url, &server, keepalive, body, operation);
            }
        }
    }
}

fn assert_fixture_headers(failure: &Failure, body: &str, truncated: bool) {
    let mut expected = reqwest::header::HeaderMap::new();
    expected.insert("content-type", "text/event-stream".parse().expect("header"));
    expected.insert(
        "content-length",
        (body.len() + if truncated { 100 } else { 0 })
            .to_string()
            .parse()
            .expect("length"),
    );
    expected
        .insert("x-request-id", "req_550e8400-e29b-41d4-a716-446655440000".parse().expect("id"));
    expected.insert("retry-after", "0".parse().expect("floor"));
    expected.insert("connection", "close".parse().expect("connection"));
    assert_eq!(failure.inspect_response_headers(), Some(&expected));
}

#[tokio::test]
async fn review_broker_session_failure_evidence_matrix() {
    async fn check<G: RecoveryProvider + 'static>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        kind: &str,
        session_api: bool,
        body: &str,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        if operation == Operation::Streaming && kind == "transport" {
            p.retryable_categories.remove(&Category::Transport);
        }
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
        let count = Arc::new(AtomicUsize::new(0));
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(CountTool(count.clone()))];
        let streaming = operation == Operation::Streaming;
        let mut delivered = vec![];
        let error = if session_api {
            let mut session = mojentic::llm::ChatSession::builder(broker).tools(tools).build();
            let initial = session.messages().len();
            let error = if streaming {
                let mut stream = session.send_stream("question");
                let mut errors = vec![];
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => delivered.push(text),
                        Err(error) => errors.push(error),
                    }
                }
                assert_eq!(errors.len(), 1, "one terminal error");
                errors.pop().expect("failure")
            } else {
                session.send("question").await.expect_err("failure")
            };
            assert_eq!(session.messages().len(), initial + 1, "only user is committed");
            assert_eq!(
                session.messages().last().expect("user retained").role(),
                mojentic::llm::MessageRole::User
            );
            error
        } else if streaming {
            let messages = [LlmMessage::user("question")];
            let mut stream = broker.generate_stream(&messages, Some(&tools), None, None);
            let mut errors = vec![];
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(text) => delivered.push(text),
                    Err(error) => errors.push(error),
                }
            }
            assert_eq!(errors.len(), 1, "one terminal error");
            errors.pop().expect("failure")
        } else if operation == Operation::Structured {
            broker
                .generate_object::<Value>(&[LlmMessage::user("question")], None, None)
                .await
                .expect_err("structured failure")
        } else {
            broker
                .generate(&[LlmMessage::user("question")], Some(&tools), None, None)
                .await
                .expect_err("failure")
        };
        let mojentic::MojenticError::Recovery(error) = error else {
            panic!("structured error retained")
        };
        assert_single_failure(&error, &capture, provider);
        assert_eq!(error.failure.operation, operation);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert_eq!(count.load(Ordering::SeqCst), 0, "no incomplete tool execution");
        assert_eq!(error.failure.http_status, Some(if kind == "http" { 401 } else { 200 }));
        assert!(error.failure.progress.headers_received);
        assert_eq!(error.failure.retry_after, RetryAfter::Seconds(0));
        assert_eq!(error.failure.acceptance, Acceptance::Unknown);
        assert_eq!(
            error.failure.provider_request_id.as_ref().expect("validated ID").as_str(),
            "req_550e8400-e29b-41d4-a716-446655440000"
        );
        assert!(!error.failure.classification.eligible);
        assert_eq!(
            error.failure.classification.reason,
            match kind {
                "http" => Reason::Permanent,
                "interrupted" => Reason::SemanticOutput,
                _ if operation != Operation::Streaming => Reason::PartialResponse,
                "protocol" => Reason::Malformed,
                _ => Reason::Permanent,
            }
        );
        assert_eq!(error.failure.progress.raw_bytes, body.len() as u64);
        assert_eq!(error.failure.inspect_response_bytes(), body.as_bytes());
        assert_fixture_headers(&error.failure, body, kind == "transport" || kind == "interrupted");
        let cause = error.failure.inspect_cause().expect("inspectable original cause");
        match kind {
            "http" => {
                assert_eq!(error.failure.category, Category::Http);
                assert_eq!(cause.to_string(), body);
                assert_eq!(error.outcome, Outcome::Failed);
            }
            "protocol" => {
                assert_eq!(error.failure.category, Category::Protocol);
                let original =
                    cause.downcast_ref::<serde_json::Error>().expect("serde error retained");
                let expected =
                    serde_json::from_str::<Value>(body).expect_err("synthetic invalid JSON");
                assert_eq!(original.to_string(), expected.to_string());
                assert_eq!(original.classify(), expected.classify());
                assert_eq!(
                    (original.line(), original.column()),
                    (expected.line(), expected.column())
                );
                assert_eq!(cause.to_string(), expected.to_string());
                assert_eq!(error.outcome, Outcome::Failed);
            }
            "transport" | "interrupted" => {
                assert_eq!(error.failure.category, Category::Transport);
                let original =
                    cause.downcast_ref::<reqwest::Error>().expect("original body transport error");
                assert!(original.is_decode());
                assert_eq!(original.to_string(), "error decoding response body");
                let source = std::error::Error::source(original).expect("HTTP framing cause");
                assert_eq!(source.to_string(), "error reading a body from connection");
                assert_eq!(
                    source.source().expect("truncation evidence").to_string(),
                    "end of file before message length reached"
                );
                assert_eq!(
                    error.outcome,
                    if kind == "interrupted" {
                        Outcome::Interrupted
                    } else {
                        Outcome::Failed
                    }
                );
            }
            _ => panic!("unknown fixture"),
        }
        if kind == "interrupted" {
            assert_eq!(delivered, vec!["partial"]);
            assert_eq!(error.failure.progress.observed.content_bytes, 7);
            assert_eq!(error.failure.progress.delivered.content_bytes, 7);
            assert_eq!(error.failure.progress.observed.reasoning_bytes, 26);
            assert_eq!(error.failure.progress.delivered.reasoning_bytes, 26);
            assert_eq!(error.failure.progress.delivered.completed_tool_calls, 0);
            assert_eq!(error.failure.progress.observed.tool_fragments, 1);
            assert_eq!(error.failure.progress.observed.completed_tool_calls, 0);
            assert_eq!(error.failure.progress.delivered.tool_fragments, 1);
        } else {
            assert!(delivered.is_empty());
            assert!(!error.failure.progress.observed.any());
            assert!(!error.failure.progress.delivered.any());
        }
        let mut expected = vec![Transition::AttemptStarted, Transition::AttemptFailed];
        if kind == "interrupted" {
            expected.push(Transition::Interrupted);
        }
        assert_eq!(transitions(&capture), expected);
        assert!(!format!(
            "{error:?} {error} {}",
            serde_json::to_string(&error).expect("safe summary")
        )
        .contains("synthetic-secret"));
    }
    for provider in ADAPTERS {
        for session_api in [false, true] {
            for operation in OPERATIONS {
                if session_api && operation == Operation::Structured {
                    continue;
                }
                for kind in ["http", "protocol", "transport", "interrupted"] {
                    if kind == "interrupted" && operation != Operation::Streaming {
                        continue;
                    }
                    let body = match kind {
                        "http" => "synthetic-secret HTTP failure".into(),
                        "protocol" if operation == Operation::Streaming => {
                            if provider == Provider::Ollama {
                                "synthetic-secret not JSON\n"
                            } else {
                                "data: synthetic-secret not JSON\n\n"
                            }
                            .into()
                        }
                        "protocol" => "synthetic-secret not JSON".into(),
                        "transport" if operation == Operation::Streaming => {
                            if provider == Provider::Ollama {
                                "{\"message\":{},\"done\":false}\n"
                            } else {
                                ": keepalive\n\n"
                            }
                            .into()
                        }
                        "transport" => "{\"synthetic-secret\":".into(),
                        _ => format!(
                            "{}{}{}",
                            frame(provider, "content"),
                            frame(provider, "reasoning"),
                            frame(provider, "tools")
                        ),
                    };
                    let server = fixture(vec![(
                        if kind == "http" { 401 } else { 200 },
                        body.clone(),
                        kind == "transport" || kind == "interrupted",
                        false,
                    )])
                    .await;
                    adapter_case!(
                        check,
                        provider,
                        &server.url,
                        &server,
                        kind,
                        session_api,
                        &body,
                        operation
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn review_explicit_pending_admission_all_adapters() {
    async fn check<G: RecoveryProvider>(inner: G, _: Provider, server: &HttpFixture) {
        let capture = Arc::new(Capture::default());
        let admission = Arc::new(DecideLater {
            signal: tokio::sync::Notify::new(),
        });
        let mut p = policy(2, &capture);
        p.admission = Some(admission.clone());
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let call = RecoveryCall::default();
        let future = reviewed_operation(&gateway, Operation::Ordinary, &call);
        tokio::pin!(future);
        tokio::time::timeout(Duration::from_secs(2), async {
            tokio::select! {
                result = &mut future => panic!("pending cannot dispatch: {}", result.is_ok()),
                _ = async { while !transitions(&capture).contains(&Transition::AdmissionPending) { tokio::task::yield_now().await; } } => {}
            }
        }).await.expect("pending admission");
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert_eq!(
            transitions(&capture),
            vec![
                Transition::AttemptStarted,
                Transition::AttemptFailed,
                Transition::AdmissionPending
            ]
        );
        admission.signal.notify_one();
        let report = tokio::time::timeout(Duration::from_secs(2), future)
            .await
            .expect("allowed")
            .expect("success");
        assert_resend(&report, &capture);
        assert_eq!(server.sends.load(Ordering::SeqCst), 2);
    }
    for provider in ADAPTERS {
        let server = fixture(vec![
            (503, "failure".into(), false, false),
            (200, response(provider, "done").to_string(), false, false),
        ])
        .await;
        adapter_case!(check, provider, &server.url, &server);
    }
}

#[tokio::test]
async fn review_completed_tool_not_reexecuted_after_followup_failure() {
    async fn check<G: RecoveryProvider + 'static>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        session_api: bool,
    ) {
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(3, &capture)).expect("gateway");
        let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
        let count = Arc::new(AtomicUsize::new(0));
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(CountTool(count.clone()))];
        let error = if session_api {
            let mut session = mojentic::llm::ChatSession::builder(broker).tools(tools).build();
            let initial = session.messages().len();
            let error = session.send("tool").await.expect_err("followup protocol failure");
            assert_eq!(session.messages().len(), initial + 1);
            assert_eq!(
                session.messages().last().expect("user").role(),
                mojentic::llm::MessageRole::User
            );
            error
        } else {
            broker
                .generate(&[LlmMessage::user("tool")], Some(&tools), None, None)
                .await
                .expect_err("followup protocol failure")
        };
        let mojentic::MojenticError::Recovery(error) = error else {
            panic!("recovery error")
        };
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(server.sends.load(Ordering::SeqCst), 2);
        assert_eq!(error.outcome, Outcome::Failed);
        assert_eq!(error.failure.provider, provider);
        assert_eq!(error.failure.category, Category::Protocol);
        assert_eq!(error.history.len(), 1, "history belongs to failed followup, not tool turn");
        assert_eq!(error.history[0].identity, error.failure.identity);
        let requests = capture.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert_ne!(requests[0].0.logical_request_id, requests[1].0.logical_request_id);
        assert_eq!(requests[1].0.wire_attempt, 1);
        assert_eq!(error.failure.identity.as_ref(), Some(&requests[1].0));
        let body: Value = serde_json::from_slice(&requests[1].1).expect("followup payload");
        assert!(body["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .any(|m| m["role"] == "tool"));
        assert_eq!(error.failure.inspect_response_bytes(), b"synthetic-secret not JSON");
        assert_fixture_headers(&error.failure, "synthetic-secret not JSON", false);
        let cause = error.failure.inspect_cause().expect("original cause");
        let decoder = cause.downcast_ref::<serde_json::Error>().expect("serde cause");
        assert_eq!(
            decoder.to_string(),
            serde_json::from_str::<Value>("synthetic-secret not JSON")
                .expect_err("fixture")
                .to_string()
        );
        assert_eq!(
            transitions(&capture),
            vec![
                Transition::AttemptStarted,
                Transition::AttemptSucceeded,
                Transition::AttemptStarted,
                Transition::AttemptFailed
            ]
        );
    }
    for provider in ADAPTERS {
        for session_api in [false, true] {
            let tool_response = if provider == Provider::Ollama {
                json!({"message":{"tool_calls":[{"function":{"name":"count","arguments":{}}}]},"done":true,"done_reason":"stop"})
            } else {
                json!({"choices":[{"message":{"tool_calls":[{"id":"call1","function":{"name":"count","arguments":"{}"}}]},"finish_reason":"tool_calls"}]})
            };
            let server = fixture(vec![
                (200, tool_response.to_string(), false, false),
                (200, "synthetic-secret not JSON".into(), false, false),
            ])
            .await;
            adapter_case!(check, provider, &server.url, &server, session_api);
        }
    }
}

#[tokio::test]
async fn review_raw_broker_stream_events_remain_single_send_legacy() {
    async fn check<G: RecoveryProvider + 'static>(inner: G, _: Provider, server: &HttpFixture) {
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(3, &capture)).expect("gateway");
        let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
        let messages = [LlmMessage::user("question")];
        let events: Vec<_> = broker.generate_stream_events(&messages, None, None).collect().await;
        assert_eq!(events.len(), 1);
        let mojentic::llm::stream_events::StreamEvent::Error(error) = &events[0] else {
            panic!("legacy terminal error")
        };
        let mojentic::llm::stream_events::StreamEventError::ProviderError { status, error } = error
        else {
            panic!("legacy provider error retained")
        };
        assert_eq!(*status, Some(503));
        assert_eq!(*error, json!("synthetic-secret raw failure"));
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert!(
            capture.requests.lock().expect("requests").is_empty(),
            "raw API bypasses recovery"
        );
        assert!(transitions(&capture).is_empty());
    }
    for provider in ADAPTERS {
        let server =
            fixture(vec![(503, "synthetic-secret raw failure".into(), false, false)]).await;
        adapter_case!(check, provider, &server.url, &server);
    }
}

#[tokio::test]
async fn recovery_metadata_public_red_green() {
    use mojentic::llm::gateway::{LlmGateway, StreamChunk};
    let body = "{\"message\":{},\"done\":false}\n{\"message\":{},\"done\":true,\"done_reason\":\"stop\"}\n";
    let server = fixture(vec![(200, body.into(), false, false)]).await;
    let gateway =
        RecoveryGateway::new(OllamaGateway::with_host(&server.url), RecoveryPolicy::default())
            .expect("gateway");
    let config = CompletionConfig::default();
    let chunks: Vec<_> = gateway.complete_stream("qwen3", &[], None, &config).collect().await;
    assert_eq!(
        chunks.iter().filter(|c| matches!(c, Ok(StreamChunk::Progress(_)))).count(),
        2,
        "every valid NDJSON frame needs decoded progress, including keepalive"
    );
    assert_eq!(
        chunks.iter().filter(|c| matches!(c, Ok(StreamChunk::Metrics(_)))).count(),
        1,
        "terminal metrics must survive recovery compatibility mapping"
    );
}

fn frozen_ollama_telemetry() -> String {
    [
        json!({"done":false}),
        json!({"message":{"thinking":"思考🦀"},"done":false}),
        json!({"message":{"content":"hé🦀"},"done":false}),
        json!({"message":{"tool_calls":[{"id":"call-1","function":{"name":"count","arguments":{"value":"🦀"}}}]},"done":false}),
        json!({"message":{},"done":true,"done_reason":"stop","total_duration":9000000000_u64,"load_duration":1000000000_u64,"prompt_eval_count":7,"prompt_eval_duration":2000000000_u64,"eval_count":12,"eval_duration":6000000000_u64}),
    ].iter().map(|v| format!("{v}\n")).collect()
}

#[tokio::test]
async fn recovery_metadata_frozen_ollama_matches_original_and_compatibility() {
    use mojentic::llm::gateway::{LlmGateway, StreamChunk, StreamMetrics, StreamProgress};
    let body = frozen_ollama_telemetry();
    let server = fixture(vec![(200, body.clone(), false, false); 3]).await;
    let config = CompletionConfig {
        reasoning_effort: Some(mojentic::llm::gateway::ReasoningEffort::Enabled),
        ..Default::default()
    };
    let messages = [LlmMessage::user("question")];
    let inner = OllamaGateway::with_host(&server.url);
    let old: Vec<_> = inner.complete_stream("qwen3", &messages, None, &config).collect().await;
    let capture = Arc::new(Capture::default());
    let gateway = RecoveryGateway::new(inner, policy(1, &capture)).expect("gateway");
    let call = RecoveryCall::default();
    let events: Vec<_> = gateway
        .complete_stream_with_recovery("qwen3", &messages, None, &config, &call)
        .collect()
        .await;
    let mut progress = vec![];
    let mut metrics = vec![];
    let mut identities = vec![];
    let mut order = vec![];
    let mut completed = None;
    for event in events {
        match event {
            RecoveryStreamEvent::Progress {
                identity,
                progress: value,
            } => {
                identities.push(identity);
                progress.push(value);
                order.push("progress");
            }
            RecoveryStreamEvent::Metrics {
                identity,
                metrics: value,
            } => {
                identities.push(identity);
                metrics.push(value);
                order.push("metrics");
            }
            RecoveryStreamEvent::Reasoning(text) => {
                assert_eq!(text, "思考🦀");
                order.push("reasoning");
            }
            RecoveryStreamEvent::Content(text) => {
                assert_eq!(text, "hé🦀");
                order.push("content");
            }
            RecoveryStreamEvent::ToolFragment(value) => {
                assert_eq!(value["function"]["arguments"]["value"], "🦀");
                order.push("fragment");
            }
            RecoveryStreamEvent::Completed(result) => {
                assert!(completed.is_none());
                completed = Some(result);
                order.push("completed");
            }
            RecoveryStreamEvent::Failed(e) => panic!("{e}"),
        }
    }
    assert_eq!(
        order,
        [
            "progress",
            "progress",
            "reasoning",
            "progress",
            "content",
            "progress",
            "fragment",
            "progress",
            "metrics",
            "completed"
        ]
    );
    let result = completed.expect("terminal proof");
    assert!(identities.iter().all(|id| id == &result.report.identity));
    assert_eq!(result.value.tool_calls.len(), 1);
    assert_eq!(result.value.tool_calls[0].arguments["value"], "🦀");
    assert_eq!(result.value.thinking.as_deref(), Some("思考🦀"));
    assert_eq!(result.report.progress.raw_bytes, body.len() as u64);
    assert_eq!(result.report.progress.delivered.reasoning_bytes, "思考🦀".len() as u64);
    assert_eq!(result.report.progress.delivered.content_bytes, "hé🦀".len() as u64);
    assert_eq!(result.report.progress.delivered.tool_fragments, 1);
    assert_eq!(result.report.progress.delivered.completed_tool_calls, 1);
    let old_progress: Vec<_> = old
        .iter()
        .filter_map(|c| match c.as_ref().expect("old stream") {
            StreamChunk::Progress(p) => Some(p.clone()),
            _ => None,
        })
        .collect();
    let old_metrics: Vec<_> = old
        .iter()
        .filter_map(|c| match c.as_ref().expect("old stream") {
            StreamChunk::Metrics(m) => Some(m.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(progress, old_progress);
    assert_eq!(metrics, old_metrics);
    assert_eq!(
        progress[1],
        StreamProgress {
            provider: "ollama".into(),
            frame_index: 2,
            done: false,
            content_chars: 0,
            thinking_chars: "思考🦀".len(),
            tool_call_count: 0,
            accumulated_tool_call_count: 0
        }
    );
    assert_eq!(
        progress.iter().map(|p| p.accumulated_tool_call_count).collect::<Vec<_>>(),
        [0, 0, 0, 0, 1]
    );
    assert_eq!(
        metrics,
        [StreamMetrics {
            provider: "ollama".into(),
            total_duration_ns: Some(9000000000),
            load_duration_ns: Some(1000000000),
            prompt_eval_count: Some(7),
            prompt_eval_duration_ns: Some(2000000000),
            eval_count: Some(12),
            eval_duration_ns: Some(6000000000),
            tokens_per_second: Some(2.0)
        }]
    );
    let compatible: Vec<_> =
        gateway.complete_stream("qwen3", &messages, None, &config).collect().await;
    // Complete tool delivery follows terminal metrics once, just as in the original stream.
    assert_eq!(
        old.iter()
            .map(|c| format!("{:?}", c.as_ref().expect("old")))
            .collect::<Vec<_>>(),
        compatible
            .iter()
            .map(|c| format!("{:?}", c.as_ref().expect("compatible")))
            .collect::<Vec<_>>()
    );
    assert_eq!(server.sends.load(Ordering::SeqCst), 3);
    let bodies = server.bodies.lock().expect("payloads");
    assert_eq!(bodies[0], bodies[1]);
    assert_eq!(bodies[1], bodies[2]);
    assert_ne!(
        serde_json::from_slice::<Value>(&bodies[1]).expect("json")["think"],
        json!(false)
    );
    let requests = capture.requests.lock().expect("wire identity");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0, result.report.identity);
}

#[tokio::test]
async fn recovery_metadata_frame_accounting_after_503_and_keepalive_failure() {
    for keepalive in [false, true] {
        let prior = if keepalive {
            "{\"message\":{},\"done\":false}\n"
        } else {
            "unavailable"
        };
        let server = fixture(vec![
            (if keepalive { 200 } else { 503 }, prior.into(), keepalive, false),
            (200, frozen_ollama_telemetry(), false, false),
        ])
        .await;
        let capture = Arc::new(Capture::default());
        let gateway =
            RecoveryGateway::new(OllamaGateway::with_host(&server.url), policy(2, &capture))
                .expect("gateway");
        let call = RecoveryCall::default();
        let events: Vec<_> = gateway
            .complete_stream_with_recovery("qwen3", &[], None, &CompletionConfig::default(), &call)
            .collect()
            .await;
        let mut frames = vec![];
        let mut metrics = vec![];
        let mut completed = None;
        for event in events {
            match event {
                RecoveryStreamEvent::Progress { identity, progress } => {
                    frames.push((identity, progress))
                }
                RecoveryStreamEvent::Metrics {
                    identity,
                    metrics: value,
                } => metrics.push((identity, value)),
                RecoveryStreamEvent::Completed(result) => completed = Some(result),
                RecoveryStreamEvent::Failed(e) => panic!("{e}"),
                _ => {}
            }
        }
        let result = completed.expect("safe admitted recovery");
        assert_resend(&result.report, &capture);
        let prior_id = result.report.history[0].identity.as_ref().expect("first send");
        if keepalive {
            assert_eq!(&frames[0].0, prior_id);
            assert_eq!(frames[0].1.frame_index, 1);
            assert!(!frames[0].1.done);
        }
        let second = &frames[usize::from(keepalive)..];
        assert_eq!(second.len(), 5);
        for (index, (id, p)) in second.iter().enumerate() {
            assert_eq!(id, &result.report.identity);
            assert_eq!(p.frame_index, index + 1);
        }
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].0, result.report.identity);
        assert!(!result.report.history[0].progress.observed.any());
        assert!(!result.report.history[0].progress.delivered.any());
        assert_eq!(result.report.history[0].progress.raw_bytes, prior.len() as u64);
        assert_eq!(server.sends.load(Ordering::SeqCst), 2);
        let bodies = server.bodies.lock().expect("same immutable bytes");
        assert_eq!(bodies[0], bodies[1]);
    }
}

#[tokio::test]
async fn recovery_metadata_absent_counters_are_not_fabricated() {
    let server = fixture(vec![(
        200,
        "{\"message\":{},\"done\":true,\"done_reason\":\"stop\"}\n".into(),
        false,
        false,
    )])
    .await;
    let gateway =
        RecoveryGateway::new(OllamaGateway::with_host(&server.url), RecoveryPolicy::default())
            .expect("gateway");
    let call = RecoveryCall::default();
    let events: Vec<_> = gateway
        .complete_stream_with_recovery("qwen3", &[], None, &CompletionConfig::default(), &call)
        .collect()
        .await;
    assert_eq!(events.len(), 3);
    match &events[1] {
        RecoveryStreamEvent::Metrics { metrics, .. } => {
            assert_eq!(metrics.provider, "ollama");
            assert_eq!(
                (
                    metrics.total_duration_ns,
                    metrics.load_duration_ns,
                    metrics.prompt_eval_count,
                    metrics.prompt_eval_duration_ns,
                    metrics.eval_count,
                    metrics.eval_duration_ns,
                    metrics.tokens_per_second
                ),
                (None, None, None, None, None, None, None)
            );
        }
        _ => panic!("final metrics"),
    }
    match &events[2] {
        RecoveryStreamEvent::Completed(result) => {
            assert!(!result.report.progress.observed.any());
            assert!(!result.report.progress.delivered.any());
        }
        _ => panic!("completed"),
    }
}

#[tokio::test]
async fn recovery_metadata_unsupported_adapters_emit_no_ollama_telemetry() {
    use mojentic::llm::gateway::{LlmGateway, StreamChunk};
    async fn check<G: RecoveryProvider>(inner: G, server: &HttpFixture) {
        let gateway = RecoveryGateway::new(inner, RecoveryPolicy::default()).expect("gateway");
        let call = RecoveryCall::default();
        let config = CompletionConfig::default();
        let events: Vec<_> = gateway
            .complete_stream_with_recovery("m", &[], None, &config, &call)
            .collect()
            .await;
        assert!(matches!(events.last(), Some(RecoveryStreamEvent::Completed(_))));
        assert!(!events.iter().any(|e| matches!(
            e,
            RecoveryStreamEvent::Progress { .. } | RecoveryStreamEvent::Metrics { .. }
        )));
        let chunks: Vec<_> = gateway.complete_stream("m", &[], None, &config).collect().await;
        assert!(chunks.iter().all(|c| matches!(c, Ok(StreamChunk::Content(_)))));
        assert_eq!(server.sends.load(Ordering::SeqCst), 2);
    }
    for provider in [Provider::Omlx, Provider::OpenAi] {
        let server = fixture(vec![(200, final_stream(provider), false, false); 2]).await;
        match provider {
            Provider::Omlx => check(omlx(&server.url), &server).await,
            Provider::OpenAi => {
                check(
                    OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url),
                    &server,
                )
                .await
            }
            _ => unreachable!("SSE adapters"),
        }
    }
}

#[tokio::test]
async fn recovery_metadata_paused_keepalive_cancellation_has_no_semantic_delivery() {
    let body = "{\"message\":{},\"done\":false}\n";
    let server = fixture(vec![(200, body.into(), false, true)]).await;
    let capture = Arc::new(Capture::default());
    let gateway = RecoveryGateway::new(OllamaGateway::with_host(&server.url), policy(3, &capture))
        .expect("gateway");
    let call = RecoveryCall::default();
    let mut stream = gateway.complete_stream_with_recovery(
        "qwen3",
        &[],
        None,
        &CompletionConfig::default(),
        &call,
    );
    let identity = match stream.next().await.expect("decoded keepalive") {
        RecoveryStreamEvent::Progress { identity, progress } => {
            assert_eq!(progress.frame_index, 1);
            assert!(!progress.done);
            identity
        }
        _ => panic!("progress"),
    };
    call.cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.closed.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("paused consumer still closes socket");
    let error = match stream.next().await.expect("terminal failure") {
        RecoveryStreamEvent::Failed(e) => e,
        _ => panic!("cancelled"),
    };
    assert_eq!(error.outcome, Outcome::Cancelled);
    assert_eq!(error.failure.identity.as_ref(), Some(&identity));
    assert!(!error.failure.progress.observed.any());
    assert!(!error.failure.progress.delivered.any());
    assert_eq!(error.failure.progress.raw_bytes, body.len() as u64);
    assert_eq!(error.history.len(), 1);
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        transitions(&capture),
        [
            Transition::AttemptStarted,
            Transition::AttemptFailed,
            Transition::Cancelled
        ]
    );
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn recovery_metadata_terminal_progress_cancellation_never_completes() {
    let server = fixture(vec![(200,"{\"message\":{},\"done\":true,\"done_reason\":\"stop\",\"eval_count\":12,\"eval_duration\":6000000000}\n".into(),false,false)]).await;
    let capture = Arc::new(Capture::default());
    let gateway = RecoveryGateway::new(OllamaGateway::with_host(&server.url), policy(3, &capture))
        .expect("gateway");
    let call = RecoveryCall::default();
    let mut stream = gateway.complete_stream_with_recovery(
        "qwen3",
        &[],
        None,
        &CompletionConfig::default(),
        &call,
    );
    assert!(
        matches!(stream.next().await,Some(RecoveryStreamEvent::Progress {progress,..}) if progress.done)
    );
    // Allow the worker to fill its bounded metadata buffer while the consumer pauses.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    call.cancellation.cancel();
    let error = match stream.next().await.expect("terminal failure") {
        RecoveryStreamEvent::Failed(e) => e,
        _ => panic!("cancellation after terminal metadata must win"),
    };
    assert_eq!(error.outcome, Outcome::Cancelled);
    assert_eq!(
        transitions(&capture),
        [
            Transition::AttemptStarted,
            Transition::AttemptFailed,
            Transition::Cancelled
        ]
    );
    assert!(!error.failure.progress.observed.any());
    assert!(!error.failure.progress.delivered.any());
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn recovery_rejected_terminal_telemetry_raw_and_compatibility() {
    use mojentic::llm::gateway::{LlmGateway, StreamChunk};
    for semantic in [false, true] {
        let prefix = if semantic {
            "{\"message\":{\"thinking\":\"思🦀\",\"content\":\"hé\",\"tool_calls\":[{\"function\":{\"name\":\"count\",\"arguments\":{}}}]},\"done\":false}\n"
        } else {
            ""
        };
        let terminal = "{\"message\":{},\"done\":true,\"done_reason\":\"length\",\"eval_count\":12,\"eval_duration\":6000000000}\n";
        let body = format!("{prefix}{terminal}");
        let server = fixture(vec![(200, body.clone(), false, false); 3]).await;
        let config = CompletionConfig::default();
        let inner = OllamaGateway::with_host(&server.url);
        let old: Vec<_> = inner.complete_stream("qwen3", &[], None, &config).collect().await;
        let expected_progress: Vec<_> = old
            .iter()
            .filter_map(|c| match c {
                Ok(StreamChunk::Progress(p)) => Some(p.clone()),
                _ => None,
            })
            .collect();
        let expected_metrics: Vec<_> = old
            .iter()
            .filter_map(|c| match c {
                Ok(StreamChunk::Metrics(m)) => Some(m.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(expected_metrics[0].eval_count, Some(12));
        assert_eq!(expected_metrics[0].tokens_per_second, Some(2.0));
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(3, &capture)).expect("gateway");
        let call = RecoveryCall::default();
        let events: Vec<_> = gateway
            .complete_stream_with_recovery("qwen3", &[], None, &config, &call)
            .collect()
            .await;
        let mut progress = vec![];
        let mut metrics = vec![];
        let mut order = vec![];
        let mut failure = None;
        for event in events {
            match event {
                RecoveryStreamEvent::Progress {
                    identity,
                    progress: p,
                } => {
                    assert_eq!(identity.wire_attempt, 1);
                    progress.push(p);
                    order.push("progress");
                }
                RecoveryStreamEvent::Metrics { metrics: m, .. } => {
                    metrics.push(m);
                    order.push("metrics");
                }
                RecoveryStreamEvent::Failed(e) => {
                    failure = Some(e);
                    order.push("failed");
                }
                RecoveryStreamEvent::Completed(_) => {
                    panic!("length must never complete tools/reply")
                }
                _ => order.push("semantic"),
            }
        }
        assert_eq!(
            progress, expected_progress,
            "rejected finish must retain exact decoded progress"
        );
        assert_eq!(metrics, expected_metrics, "rejected finish must retain exact final metrics");
        assert_eq!(&order[order.len() - 3..], &["progress", "metrics", "failed"]);
        let error = failure.expect("failure");
        assert_eq!(error.failure.progress.observed.completed_tool_calls, 0);
        assert_eq!(error.failure.progress.delivered.completed_tool_calls, 0);
        assert_eq!(
            error.failure.progress.observed.content_bytes,
            if semantic { "hé".len() as u64 } else { 0 }
        );
        assert_eq!(error.failure.progress.observed, error.failure.progress.delivered);
        assert_eq!(error.failure.progress.raw_bytes, body.len() as u64);
        assert_eq!(error.history.len(), 1);
        assert_eq!(
            error
                .failure
                .inspect_cause()
                .expect("original cause")
                .downcast_ref::<CompletionDecodeError>(),
            Some(&CompletionDecodeError::Length)
        );
        assert_eq!(capture.requests.lock().expect("requests").len(), 1);
        let chunks: Vec<_> = gateway.complete_stream("qwen3", &[], None, &config).collect().await;
        assert_eq!(
            chunks
                .iter()
                .filter_map(|c| match c {
                    Ok(StreamChunk::Progress(p)) => Some(p.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            expected_progress
        );
        assert_eq!(
            chunks
                .iter()
                .filter_map(|c| match c {
                    Ok(StreamChunk::Metrics(m)) => Some(m.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            expected_metrics
        );
        assert!(matches!(chunks.last(), Some(Err(_))));
        assert!(!chunks.iter().any(|c| matches!(c, Ok(StreamChunk::ToolCalls(_)))));
        let requests = capture.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].1, requests[1].1);
        assert_eq!(server.sends.load(Ordering::SeqCst), 3);
    }
}

#[tokio::test]
async fn recovery_rejected_terminal_telemetry_compatibility_red_green() {
    use mojentic::llm::gateway::{LlmGateway, StreamChunk};
    let body = "{\"done\":true,\"done_reason\":\"length\",\"eval_count\":12,\"eval_duration\":6000000000}\n";
    let server = fixture(vec![(200, body.into(), false, false)]).await;
    let gateway =
        RecoveryGateway::new(OllamaGateway::with_host(&server.url), RecoveryPolicy::default())
            .expect("gateway");
    let chunks: Vec<_> = gateway
        .complete_stream("qwen3", &[], None, &CompletionConfig::default())
        .collect()
        .await;
    assert_eq!(chunks.len(), 3, "compatibility must emit Progress -> Metrics -> Err for length");
    assert!(matches!(&chunks[0], Ok(StreamChunk::Progress(p)) if p.done && p.frame_index == 1));
    assert!(
        matches!(&chunks[1], Ok(StreamChunk::Metrics(m)) if m.eval_count == Some(12) && m.eval_duration_ns == Some(6000000000) && m.tokens_per_second == Some(2.0))
    );
    assert!(chunks[2].is_err());
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn recovery_rejected_terminal_same_frame_semantics_are_observed_only() {
    let body = format!(
        "{}\n",
        json!({"message":{"content":"hé🦀","thinking":"思🦀","tool_calls":[{"function":{"name":"count","arguments":{}}}]},"done":true,"done_reason":"length","eval_count":12,"eval_duration":6000000000_u64})
    );
    let server = fixture(vec![(200, body.clone(), false, false)]).await;
    let gateway =
        RecoveryGateway::new(OllamaGateway::with_host(&server.url), RecoveryPolicy::default())
            .expect("gateway");
    let events: Vec<_> = gateway
        .complete_stream_with_recovery(
            "m",
            &[],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .collect()
        .await;
    assert_eq!(events.len(), 3);
    assert!(
        matches!(&events[0], RecoveryStreamEvent::Progress { progress, .. } if progress.done && progress.content_chars == "hé🦀".len() && progress.thinking_chars == "思🦀".len() && progress.tool_call_count == 1 && progress.accumulated_tool_call_count == 0)
    );
    assert!(
        matches!(&events[1], RecoveryStreamEvent::Metrics { metrics, .. } if metrics.eval_count == Some(12))
    );
    let RecoveryStreamEvent::Failed(error) = &events[2] else {
        panic!("failed")
    };
    assert_eq!(error.failure.progress.observed.content_bytes, "hé🦀".len() as u64);
    assert_eq!(error.failure.progress.observed.reasoning_bytes, "思🦀".len() as u64);
    assert_eq!(error.failure.progress.observed.tool_fragments, 1);
    assert_eq!(error.failure.progress.observed.completed_tool_calls, 0);
    assert!(!error.failure.progress.delivered.any());
    assert_eq!(error.failure.inspect_response_bytes(), body.as_bytes());
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn recovery_terminal_invalid_fields_do_not_fabricate_telemetry() {
    use mojentic::llm::gateway::{LlmGateway, StreamChunk};
    async fn check<G: RecoveryProvider>(inner: G, _: Provider, server: &HttpFixture) {
        let gateway = RecoveryGateway::new(inner, RecoveryPolicy::default()).expect("gateway");
        let events: Vec<_> = gateway
            .complete_stream_with_recovery(
                "m",
                &[],
                None,
                &CompletionConfig::default(),
                &RecoveryCall::default(),
            )
            .collect()
            .await;
        assert!(matches!(events.as_slice(), [RecoveryStreamEvent::Failed(_)]));
        let chunks: Vec<_> = gateway
            .complete_stream("m", &[], None, &CompletionConfig::default())
            .collect()
            .await;
        assert!(matches!(chunks.as_slice(), [Err(_)]));
        assert!(!chunks.iter().any(|c| matches!(c, Ok(StreamChunk::ToolCalls(_)))));
        assert_eq!(server.sends.load(Ordering::SeqCst), 2);
    }
    for body in [
        json!({"message":{"content":7},"done":true,"done_reason":"length","eval_count":12}),
        json!({"message":{"tool_calls":[{"function":{"name":"count","arguments":"partial"}}]},"done":true,"done_reason":"length"}),
        json!({"message":{},"done":"true","done_reason":"length","eval_count":12}),
        json!({"message":{},"done":true,"done_reason":7,"eval_count":12}),
    ] {
        let server = fixture(vec![(200, format!("{body}\n"), false, false); 2]).await;
        check(OllamaGateway::with_host(&server.url), Provider::Ollama, &server).await;
    }
    for provider in [Provider::Omlx, Provider::OpenAi] {
        let body = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}],\"usage\":{\"completion_tokens\":12}}\n\ndata: [DONE]\n\n";
        let server = fixture(vec![(200, body.into(), false, false); 2]).await;
        adapter_case!(check, provider, &server.url, &server);
    }
}

#[tokio::test]
async fn recovery_rejected_terminal_capture_retains_evidence_without_delivery() {
    let body = format!(
        "{}\n",
        json!({"message":{"content":"hé","thinking":"思","tool_calls":[{"function":{"name":"count","arguments":{}}}]},"done":true,"done_reason":"length","eval_count":12})
    );
    let server = fixture(vec![(200, body.clone(), false, false)]).await;
    let capture = Arc::new(Capture::default());
    let mut p = policy(3, &capture);
    p.wire_observer = Some(Arc::new(RejectBodyCapture));
    let gateway = RecoveryGateway::new(OllamaGateway::with_host(&server.url), p).expect("gateway");
    let events: Vec<_> = gateway
        .complete_stream_with_recovery(
            "m",
            &[],
            None,
            &CompletionConfig::default(),
            &RecoveryCall::default(),
        )
        .collect()
        .await;
    let [RecoveryStreamEvent::Failed(error)] = events.as_slice() else {
        panic!("capture must stop before delivery")
    };
    assert_eq!(error.outcome, Outcome::ObserverFailed);
    assert!(error
        .failure
        .inspect_cause()
        .expect("typed capture cause")
        .downcast_ref::<std::io::Error>()
        .is_some());
    assert_eq!(error.failure.progress.observed.content_bytes, "hé".len() as u64);
    assert_eq!(error.failure.progress.observed.reasoning_bytes, "思".len() as u64);
    assert_eq!(error.failure.progress.observed.tool_fragments, 1);
    assert_eq!(error.failure.progress.observed.completed_tool_calls, 0);
    assert!(!error.failure.progress.delivered.any());
    assert_eq!(error.failure.inspect_response_bytes(), body.as_bytes());
    assert!(error
        .failure
        .inspect_cause()
        .expect("capture cause")
        .to_string()
        .contains("body capture failure"));
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        transitions(&capture),
        vec![Transition::AttemptStarted, Transition::AttemptFailed]
    );
}

#[tokio::test]
async fn recovery_rejected_terminal_metadata_cancellation_wins() {
    for cancel_after_metrics in [false, true] {
        let body = "{\"done\":true,\"done_reason\":\"length\",\"eval_count\":12,\"eval_duration\":6000000000}\n";
        let server = fixture(vec![(200, body.into(), false, false)]).await;
        let capture = Arc::new(Capture::default());
        let gateway =
            RecoveryGateway::new(OllamaGateway::with_host(&server.url), policy(3, &capture))
                .expect("gateway");
        let call = RecoveryCall::default();
        let config = CompletionConfig::default();
        let mut stream = gateway.complete_stream_with_recovery("m", &[], None, &config, &call);
        assert!(
            matches!(stream.next().await, Some(RecoveryStreamEvent::Progress { progress, .. }) if progress.done)
        );
        if cancel_after_metrics {
            assert!(
                matches!(stream.next().await, Some(RecoveryStreamEvent::Metrics { metrics, .. }) if metrics.eval_count == Some(12))
            );
        }
        call.cancellation.cancel();
        let rest: Vec<_> = stream.collect().await;
        let [RecoveryStreamEvent::Failed(error)] = rest.as_slice() else {
            panic!("one cancellation failure")
        };
        assert_eq!(error.outcome, Outcome::Cancelled);
        assert!(!error.failure.progress.delivered.any());
        assert!(!error.failure.progress.observed.any());
        assert_eq!(error.failure.progress.raw_bytes, body.len() as u64);
        assert_eq!(
            transitions(&capture),
            vec![
                Transition::AttemptStarted,
                Transition::AttemptFailed,
                Transition::Cancelled
            ]
        );
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
    }
}

struct Unresolved;
#[async_trait]
impl Admission for Unresolved {
    async fn decide(&self, _: AdmissionContext) -> AdmissionDecision {
        AdmissionDecision::Unresolved
    }
}

#[tokio::test]
async fn admission_unresolved_http_matrix() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        p.admission = Some(Arc::new(Unresolved));
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let error = reviewed_operation(&gateway, operation, &RecoveryCall::default())
            .await
            .expect_err("unresolved admission must stop");
        assert_eq!(error.outcome, Outcome::AdmissionUnresolved);
        assert_single_failure(&error, &capture, provider);
        assert_eq!(error.failure.operation, operation);
        assert_eq!(error.failure.http_status, Some(503));
        assert_eq!(error.failure.inspect_response_bytes(), b"synthetic-secret failure");
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert_eq!(
            transitions(&capture),
            vec![
                Transition::AttemptStarted,
                Transition::AttemptFailed,
                Transition::AdmissionPending,
                Transition::AdmissionUnresolved,
            ]
        );
        let events = capture.events.lock().expect("events");
        for event in events.iter().skip(1) {
            assert_eq!(event.identity, error.failure.identity);
            assert_eq!(event.failure.as_ref().expect("failure").http_status, Some(503));
        }
        assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
        assert!(!serde_json::to_string(&error).expect("safe error").contains("synthetic-secret"));
        assert!(!serde_json::to_string(&*events)
            .expect("safe events")
            .contains("synthetic-secret"));
        assert_eq!(serde_json::to_value(error.outcome).expect("outcome"), "admission_unresolved");
        assert_eq!(
            serde_json::to_value(events.last().expect("terminal").transition).expect("reason"),
            "admission_unresolved"
        );
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            let server = fixture(vec![
                (503, "synthetic-secret failure".into(), false, false),
                (200, response(provider, "unexpected resend").to_string(), false, false),
            ])
            .await;
            adapter_case!(check, provider, &server.url, &server, operation);
        }
    }
}

#[tokio::test]
async fn admission_unresolved_broker_session_http_matrix() {
    async fn check<G: RecoveryProvider + 'static>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        session_api: bool,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        p.admission = Some(Arc::new(Unresolved));
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
        let count = Arc::new(AtomicUsize::new(0));
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(CountTool(count.clone()))];
        let streaming = operation == Operation::Streaming;
        let mut delivered = vec![];
        let error = if session_api {
            let mut session = mojentic::llm::ChatSession::builder(broker).tools(tools).build();
            let initial = session.messages().len();
            let error = if streaming {
                let mut stream = session.send_stream("question");
                let mut errors = vec![];
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => delivered.push(text),
                        Err(error) => errors.push(error),
                    }
                }
                assert_eq!(errors.len(), 1, "one terminal error");
                errors.pop().expect("failure")
            } else {
                session.send("question").await.expect_err("failure")
            };
            assert_eq!(session.messages().len(), initial + 1, "only user is committed");
            assert_eq!(
                session.messages().last().expect("user retained").role(),
                mojentic::llm::MessageRole::User
            );
            error
        } else if streaming {
            let messages = [LlmMessage::user("question")];
            let mut stream = broker.generate_stream(&messages, Some(&tools), None, None);
            let mut errors = vec![];
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(text) => delivered.push(text),
                    Err(error) => errors.push(error),
                }
            }
            assert_eq!(errors.len(), 1, "one terminal error");
            errors.pop().expect("failure")
        } else if operation == Operation::Structured {
            broker
                .generate_object::<Value>(&[LlmMessage::user("question")], None, None)
                .await
                .expect_err("structured failure")
        } else {
            broker
                .generate(&[LlmMessage::user("question")], Some(&tools), None, None)
                .await
                .expect_err("failure")
        };
        let mojentic::MojenticError::Recovery(error) = error else {
            panic!("structured error retained")
        };
        assert_single_failure(&error, &capture, provider);
        assert_eq!(error.failure.operation, operation);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert_eq!(count.load(Ordering::SeqCst), 0, "no incomplete tool execution");
        assert_eq!(error.outcome, Outcome::AdmissionUnresolved);
        assert_eq!(error.failure.http_status, Some(503));
        assert_eq!(error.failure.inspect_response_bytes(), b"synthetic-secret failure");
        assert!(delivered.is_empty(), "no semantic output on unresolved admission");
        assert_eq!(
            transitions(&capture),
            vec![
                Transition::AttemptStarted,
                Transition::AttemptFailed,
                Transition::AdmissionPending,
                Transition::AdmissionUnresolved,
            ]
        );
        assert!(!serde_json::to_string(&error).expect("safe error").contains("synthetic-secret"));
        assert!(!serde_json::to_string(&*capture.events.lock().expect("events"))
            .expect("safe events")
            .contains("synthetic-secret"));
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            for session_api in [false, true] {
                // ChatSession has ordinary and streaming APIs; structured is broker-only.
                if session_api && operation == Operation::Structured {
                    continue;
                }
                let server = fixture(vec![
                    (503, "synthetic-secret failure".into(), false, false),
                    (200, response(provider, "unexpected resend").to_string(), false, false),
                ])
                .await;
                adapter_case!(check, provider, &server.url, &server, session_api, operation);
            }
        }
    }
}

#[tokio::test]
async fn admission_explicit_decisions_http_matrix() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        operation: Operation,
        allow: bool,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        if !allow {
            p.admission = Some(Arc::new(Reject));
        }
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let result = reviewed_operation(&gateway, operation, &RecoveryCall::default()).await;
        if allow {
            assert_resend(&result.expect("explicit allow"), &capture);
            assert_eq!(server.sends.load(Ordering::SeqCst), 2);
        } else {
            let error = result.expect_err("explicit reject");
            assert_eq!(error.outcome, Outcome::AdmissionRejected);
            assert_single_failure(&error, &capture, provider);
            assert_eq!(server.sends.load(Ordering::SeqCst), 1);
            assert_eq!(
                transitions(&capture),
                vec![
                    Transition::AttemptStarted,
                    Transition::AttemptFailed,
                    Transition::AdmissionPending,
                    Transition::AdmissionRejected
                ]
            );
        }
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            for allow in [true, false] {
                let success = match operation {
                    Operation::Streaming => final_stream(provider),
                    Operation::Structured => response(provider, "{}").to_string(),
                    Operation::Ordinary => response(provider, "done").to_string(),
                };
                let server = fixture(vec![
                    (503, "synthetic-secret failure".into(), false, false),
                    (200, success, false, false),
                ])
                .await;
                adapter_case!(check, provider, &server.url, &server, operation, allow);
            }
        }
    }
}

#[tokio::test]
async fn admission_pending_budget_http_matrix() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        p.admission = Some(Arc::new(Pending));
        p.budget = Some(Duration::from_secs(1));
        p.timing = Arc::new(Clock::default());
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let error = tokio::time::timeout(
            Duration::from_secs(2),
            reviewed_operation(&gateway, operation, &RecoveryCall::default()),
        )
        .await
        .expect("pending hook budget expires")
        .expect_err("budget expired");
        assert_eq!(error.outcome, Outcome::BudgetExpired);
        assert_single_failure(&error, &capture, provider);
        assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        assert_eq!(
            transitions(&capture),
            vec![
                Transition::AttemptStarted,
                Transition::AttemptFailed,
                Transition::AdmissionPending
            ]
        );
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            let server =
                fixture(vec![(503, "synthetic-secret failure".into(), false, false)]).await;
            adapter_case!(check, provider, &server.url, &server, operation);
        }
    }
}

#[tokio::test]
async fn admission_absent_local_remote_http_matrix() {
    async fn check<G: RecoveryProvider>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        operation: Operation,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        p.admission = None;
        let gateway = RecoveryGateway::new(inner, p).expect("gateway");
        let result = reviewed_operation(&gateway, operation, &RecoveryCall::default()).await;
        if provider == Provider::OpenAi {
            let report = result.expect("remote no-hook retry");
            assert_eq!(report.history.len(), 1);
            assert_eq!(report.identity.wire_attempt, 2);
            assert_eq!(server.sends.load(Ordering::SeqCst), 2);
            assert!(transitions(&capture).contains(&Transition::RetryStarted));
        } else {
            let error = result.expect_err("ambiguous local retry requires permission");
            assert_eq!(error.outcome, Outcome::AdmissionRequired);
            assert_single_failure(&error, &capture, provider);
            assert_eq!(server.sends.load(Ordering::SeqCst), 1);
            assert_eq!(
                transitions(&capture),
                vec![
                    Transition::AttemptStarted,
                    Transition::AttemptFailed,
                    Transition::AdmissionRequired
                ]
            );
        }
        assert!(!transitions(&capture).contains(&Transition::AdmissionPending));
        assert!(!transitions(&capture).contains(&Transition::AdmissionAllowed));
    }
    for provider in ADAPTERS {
        for operation in OPERATIONS {
            let success = match operation {
                Operation::Streaming => final_stream(provider),
                Operation::Structured => response(provider, "{}").to_string(),
                Operation::Ordinary => response(provider, "done").to_string(),
            };
            let server = fixture(vec![
                (503, "synthetic-secret failure".into(), false, false),
                (200, success, false, false),
            ])
            .await;
            adapter_case!(check, provider, &server.url, &server, operation);
        }
    }
}

/// Executes the shared declarations against real public adapters, without a mock provider.
#[test]
fn capability_contract_public_adapters_v1() {
    fn check<G: RecoveryProvider>(gateway: RecoveryGateway<G>, provider: &str) {
        let contract: Value =
            serde_json::from_str(include_str!("contracts/recovery_capabilities_v1.json"))
                .expect("versioned reference contract");
        assert_eq!(contract["contract"], "recovery-capabilities-v1");
        let keys = [
            "local_request_cancellation",
            "remote_request_cancellation",
            "request_status",
            "idempotency",
            "exact_attempt_termination",
        ];
        let cases = contract["cases"].as_array().expect("contract cases");
        let mut expected_evidence = serde_json::Map::new();
        let mut expected_values = serde_json::Map::new();
        for key in keys {
            let entries: Vec<_> = cases
                .iter()
                .filter(|entry| entry["provider"] == provider && entry["capability"] == key)
                .collect();
            assert_eq!(entries.len(), 1, "one contract case for {provider}/{key}");
            let evidence = entries[0]["evidence"].clone();
            assert!(matches!(
                evidence["value"].as_str(),
                Some("supported" | "unsupported" | "unknown")
            ));
            assert!(evidence["documentation"].as_str().expect("citation").starts_with("https://"));
            assert!(!evidence["basis"].as_str().expect("basis").is_empty());
            assert!(chrono::NaiveDate::parse_from_str(
                evidence["inspected_on"].as_str().expect("date"),
                "%Y-%m-%d"
            )
            .is_ok());
            expected_values.insert(key.into(), evidence["value"].clone());
            expected_evidence.insert(key.into(), evidence);
        }
        assert_eq!(
            serde_json::to_value(gateway.capabilities()).expect("values"),
            Value::Object(expected_values),
            "{provider} values"
        );
        assert_eq!(
            serde_json::to_value(gateway.capability_evidence()).expect("evidence"),
            Value::Object(expected_evidence),
            "{provider} exact endpoint evidence"
        );
        assert_eq!(gateway.capabilities().exact_attempt_termination, Support::Unknown);
        assert_eq!(
            gateway.capabilities().request_cancel,
            gateway.capabilities().remote_request_cancellation
        );
    }
    // OpenAI and Ollama have identical values, but different endpoint evidence.
    check(
        OpenAIGateway::with_api_key("contract-test")
            .with_recovery(RecoveryPolicy::default())
            .expect("openai"),
        "openai",
    );
    check(
        OllamaGateway::new().with_recovery(RecoveryPolicy::default()).expect("ollama"),
        "ollama",
    );
    check(
        OmlxGateway::new().with_recovery(RecoveryPolicy::default()).expect("omlx"),
        "omlx",
    );
}

#[tokio::test]
async fn typed_errors_v1_session_length_proof() {
    let body =
        json!({"choices":[{"message":{"content":"partial-secret"},"finish_reason":"length"}]})
            .to_string();
    let server = fixture(vec![(200, body.clone(), false, false)]).await;
    let capture = Arc::new(Capture::default());
    let gateway = OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url)
        .with_recovery(policy(3, &capture))
        .expect("gateway");
    let broker = LlmBroker::new("fixture-model", Arc::new(gateway), None);
    let mut session = mojentic::llm::ChatSession::builder(broker).build();
    let initial = session.messages().len();
    let error = session.send("question").await.expect_err("length must terminate recovery");
    let mojentic::MojenticError::Recovery(error) = error else {
        panic!("typed recovery failure")
    };
    assert_eq!(error.failure.category, Category::Protocol);
    assert_eq!(error.failure.progress.observed.content_bytes, "partial-secret".len() as u64);
    assert_eq!(error.failure.progress.delivered, SemanticProgress::default());
    assert!(!error.failure.classification.eligible);
    assert_eq!(
        error
            .failure
            .inspect_cause()
            .expect("typed cause")
            .downcast_ref::<CompletionDecodeError>(),
        Some(&CompletionDecodeError::Length)
    );
    assert_eq!(error.failure.inspect_response_bytes(), body.as_bytes());
    assert_eq!(error.history.len(), 1);
    assert_eq!(session.messages().len(), initial + 1);
    assert_eq!(server.sends.load(Ordering::SeqCst), 1);
}

fn typed_errors_contract() -> Value {
    let contract: Value = serde_json::from_str(include_str!("contracts/typed_errors_v1.json"))
        .expect("executable contract");
    assert_eq!(contract["version"], "typed-errors-v1");
    assert_eq!(contract["port"], "rust");
    contract
}

#[derive(Debug, thiserror::Error)]
#[error("capture-secret at {0}")]
struct ContractCaptureError(&'static str);
struct ContractCapture {
    stage: &'static str,
    capture: Arc<Capture>,
}
impl WireObserver for ContractCapture {
    fn observe(
        &self,
        event: WireEvent<'_>,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stage = match &event {
            WireEvent::Request { .. } => "request",
            WireEvent::Headers { .. } => "headers",
            WireEvent::Body { .. } => "body",
        };
        WireObserver::observe(&*self.capture, event)?;
        if stage == self.stage {
            Err(Box::new(ContractCaptureError(self.stage)))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn typed_errors_v1_boundaries() {
    async fn check<G: RecoveryProvider + 'static>(
        inner: G,
        provider: Provider,
        server: &HttpFixture,
        operation: Operation,
        boundary: &str,
        scenario: &str,
        body: &str,
    ) {
        let capture = Arc::new(Capture::default());
        let mut p = policy(3, &capture);
        let stage = match scenario {
            "request" => Some("request"),
            "headers" => Some("headers"),
            "body" => Some("body"),
            _ => None,
        };
        if let Some(stage) = stage {
            p.wire_observer = Some(Arc::new(ContractCapture {
                stage,
                capture: capture.clone(),
            }));
        }
        let gateway = RecoveryGateway::new(inner, p).expect("valid policy");
        let count = Arc::new(AtomicUsize::new(0));
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(CountTool(count.clone()))];
        let messages = [LlmMessage::user("question")];
        let config = CompletionConfig::default();
        let call = RecoveryCall::default();
        let error = if boundary == "gateway" {
            let error = match operation {
                Operation::Ordinary => gateway
                    .complete_with_recovery("qwen3", &messages, Some(&tools), &config, &call)
                    .await
                    .err()
                    .expect("terminal error"),
                Operation::Structured => gateway
                    .complete_json_with_recovery(
                        "qwen3",
                        &messages,
                        json!({"type":"object"}),
                        &config,
                        &call,
                    )
                    .await
                    .err()
                    .expect("terminal error"),
                Operation::Streaming => {
                    let events: Vec<_> = gateway
                        .complete_stream_with_recovery(
                            "qwen3",
                            &messages,
                            Some(&tools),
                            &config,
                            &call,
                        )
                        .collect()
                        .await;
                    let mut errors = vec![];
                    for event in events {
                        match event {
                            RecoveryStreamEvent::Failed(error) => errors.push(error),
                            RecoveryStreamEvent::Completed(_) => panic!("incomplete completion"),
                            _ => {}
                        }
                    }
                    assert_eq!(errors.len(), 1);
                    errors.pop().expect("terminal error")
                }
            };
            mojentic::MojenticError::Recovery(Box::new(error))
        } else {
            let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
            if boundary == "session" {
                let mut session = mojentic::llm::ChatSession::builder(broker).tools(tools).build();
                let initial = session.messages().len();
                let error = if operation == Operation::Streaming {
                    let chunks: Vec<_> = session.send_stream("question").collect().await;
                    let mut errors: Vec<_> =
                        chunks.into_iter().filter_map(std::result::Result::err).collect();
                    assert_eq!(errors.len(), 1);
                    errors.pop().expect("terminal error")
                } else {
                    session.send("question").await.expect_err("terminal error")
                };
                assert_eq!(session.messages().len(), initial + 1);
                assert_eq!(
                    session.messages().last().expect("retained user").role(),
                    mojentic::llm::MessageRole::User
                );
                error
            } else {
                match operation {
                    Operation::Ordinary => broker
                        .generate(&messages, Some(&tools), None, None)
                        .await
                        .expect_err("terminal error"),
                    Operation::Structured => broker
                        .generate_object::<Value>(&messages, None, None)
                        .await
                        .expect_err("terminal error"),
                    Operation::Streaming => {
                        let chunks: Vec<_> = broker
                            .generate_stream(&messages, Some(&tools), None, None)
                            .collect()
                            .await;
                        let mut errors: Vec<_> =
                            chunks.into_iter().filter_map(std::result::Result::err).collect();
                        assert_eq!(errors.len(), 1);
                        errors.pop().expect("terminal error")
                    }
                }
            }
        };
        let mojentic::MojenticError::Recovery(error) = error else {
            panic!("recovery error preserved")
        };
        assert_eq!(error.failure.provider, provider);
        assert_eq!(error.failure.operation, operation);
        assert!(!error.failure.classification.eligible);
        assert_eq!(count.load(Ordering::SeqCst), 0, "no incomplete tools");
        let sent = usize::from(scenario != "request");
        assert_eq!(server.sends.load(Ordering::SeqCst), sent, "no resend");
        assert_eq!(error.history.len(), sent);
        if sent == 1 {
            assert_single_failure(&error, &capture, provider);
            let requests = server.bodies.lock().expect("request bodies");
            assert_eq!(requests[0], capture.requests.lock().expect("wire requests")[0].1);
            assert!(String::from_utf8_lossy(&requests[0]).contains("question"));
        } else {
            assert!(error.failure.identity.is_none());
        }
        let cause = error.failure.inspect_cause().expect("original typed cause");
        if let Some(stage) = stage {
            assert_eq!(error.outcome, Outcome::ObserverFailed);
            assert_eq!(
                cause.downcast_ref::<ContractCaptureError>().expect("original capture error").0,
                stage
            );
        } else if scenario == "length" {
            assert_eq!(
                cause.downcast_ref::<CompletionDecodeError>(),
                Some(&CompletionDecodeError::Length)
            );
        } else if scenario == "invalid_frame" {
            assert_eq!(
                cause.downcast_ref::<CompletionDecodeError>(),
                Some(&CompletionDecodeError::InvalidFrame {
                    reason: "invalid text field"
                })
            );
        } else {
            assert!(cause.downcast_ref::<serde_json::Error>().is_some(), "original serde error");
        }
        if stage.is_none() || stage == Some("body") {
            assert_eq!(error.failure.inspect_response_bytes(), body.as_bytes());
        }
        assert!(!transitions(&capture).iter().any(|t| matches!(
            t,
            Transition::AdmissionPending | Transition::RetryStarted | Transition::AttemptSucceeded
        )));
        for safe in [
            format!("{error:?}"),
            error.to_string(),
            serde_json::to_string(&error).expect("safe serialization"),
            serde_json::to_string(&*capture.events.lock().expect("events"))
                .expect("safe lifecycle"),
        ] {
            for secret in [
                "partial-secret",
                "capture-secret",
                "synthetic-secret",
                "question",
            ] {
                assert!(!safe.contains(secret), "private evidence leaked");
            }
        }
    }
    let contract = typed_errors_contract();
    for provider in ADAPTERS {
        let key = match provider {
            Provider::Ollama => "ollama",
            Provider::Omlx => "omlx",
            Provider::OpenAi => "openai",
        };
        for operation in OPERATIONS {
            let op = match operation {
                Operation::Ordinary => "ordinary",
                Operation::Structured => "structured",
                Operation::Streaming => "streaming",
            };
            for boundary in ["gateway", "broker", "session"] {
                if boundary == "session" && operation == Operation::Structured {
                    continue;
                }
                let mut scenarios = vec!["length", "decode", "invalid_frame"];
                scenarios.extend(
                    contract["capture_stages"]
                        .as_array()
                        .expect("capture stages")
                        .iter()
                        .map(|stage| stage.as_str().expect("stage")),
                );
                for scenario in scenarios {
                    let kind = if matches!(scenario, "request" | "headers" | "body") {
                        "capture"
                    } else {
                        scenario
                    };
                    let body =
                        contract["providers"][key][kind][op].as_str().expect("versioned response");
                    let server = fixture(vec![(200, body.into(), false, false)]).await;
                    adapter_case!(
                        check,
                        provider,
                        &server.url,
                        &server,
                        operation,
                        boundary,
                        scenario,
                        body
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn typed_errors_v1_invalid_policy_prevents_all_entrypoints() {
    let contract = typed_errors_contract();
    for provider in ADAPTERS {
        let server = fixture(vec![]).await;
        let mut cases = vec![];
        for status in contract["invalid_statuses"].as_array().expect("statuses") {
            let status = u16::try_from(status.as_u64().expect("status")).expect("u16");
            cases.push((
                RecoveryPolicy {
                    retryable_statuses: [status].into_iter().collect(),
                    ..Default::default()
                },
                RecoveryPolicyError::InvalidStatus { status },
            ));
        }
        for category in contract["unsupported_categories"].as_array().expect("categories") {
            let category = match category.as_str().expect("category") {
                "protocol" => Category::Protocol,
                "provider_response" => Category::ProviderResponse,
                "cancellation" => Category::Cancellation,
                _ => panic!("unknown contract category"),
            };
            cases.push((
                RecoveryPolicy {
                    retryable_categories: [category].into_iter().collect(),
                    ..Default::default()
                },
                RecoveryPolicyError::UnsupportedCategory { category },
            ));
        }
        for (policy, expected) in cases {
            assert_eq!(policy.validate(), Err(expected));
            let error = match provider {
                Provider::Ollama => {
                    OllamaGateway::with_host(&server.url).with_recovery(policy).err()
                }
                Provider::Omlx => omlx(&server.url).with_recovery(policy).err(),
                Provider::OpenAi => {
                    OpenAIGateway::with_api_key_and_base_url("synthetic-secret", &server.url)
                        .with_recovery(policy)
                        .err()
                }
            }
            .expect("cannot construct any recovery entrypoint with invalid policy");
            assert!(
                matches!(&error, mojentic::MojenticError::InvalidRecoveryPolicy(actual) if *actual == expected)
            );
            assert_eq!(
                std::error::Error::source(&error)
                    .expect("policy source")
                    .downcast_ref::<RecoveryPolicyError>(),
                Some(&expected)
            );
            assert_eq!(server.sends.load(Ordering::SeqCst), 0);
        }
    }
    // Existing delay clamping, zero budgets and permanent-status selection stay valid.
    RecoveryPolicy {
        base_delay: Duration::MAX,
        delay_ceiling: Duration::ZERO,
        budget: Some(Duration::ZERO),
        retryable_statuses: [400, 401, 403, 429, 500, 599].into_iter().collect(),
        ..Default::default()
    }
    .validate()
    .expect("representable limits are valid");
}

#[tokio::test]
async fn typed_errors_v1_optional_fields_and_legacy_length() {
    async fn check<G: RecoveryProvider>(inner: G, _: Provider, body: &str, legacy: bool) {
        let response = if legacy {
            inner
                .complete("qwen3", &[], None, &CompletionConfig::default())
                .await
                .expect("legacy length response")
        } else {
            let gateway = RecoveryGateway::new(inner, RecoveryPolicy::default()).expect("gateway");
            gateway
                .complete_with_recovery(
                    "qwen3",
                    &[],
                    None,
                    &CompletionConfig::default(),
                    &RecoveryCall::default(),
                )
                .await
                .unwrap_or_else(|error| panic!("optional fields may be absent: {error}"))
                .value
        };
        let value: Value = serde_json::from_str(body).expect("response fixture");
        if legacy {
            assert_eq!(response.evidence.finish_reason.as_deref(), Some("length"));
        } else {
            assert_eq!(response.content.as_deref(), Some("{}"));
            assert!(response.thinking.is_none());
            assert!(response.tool_calls.is_empty());
            assert!(value.is_object());
        }
    }
    let contract = typed_errors_contract();
    for provider in ADAPTERS {
        let key = match provider {
            Provider::Ollama => "ollama",
            Provider::Omlx => "omlx",
            Provider::OpenAi => "openai",
        };
        for legacy in [false, true] {
            let body = if legacy {
                &contract["providers"][key]["length"]["ordinary"]
            } else {
                &contract["providers"][key]["optional_success"]
            }
            .as_str()
            .expect("response");
            let server = fixture(vec![(200, body.into(), false, false)]).await;
            adapter_case!(check, provider, &server.url, body, legacy);
            assert_eq!(server.sends.load(Ordering::SeqCst), 1);
        }
    }
}

// recovery-status-selection-v1: the same HTTP conversation with only policy changed.
async fn status_selection_case<G: RecoveryProvider + 'static>(
    inner: G,
    provider: Provider,
    server: &HttpFixture,
    boundary: &str,
    operation: Operation,
    status: u16,
    selection: &str,
) -> Vec<u8> {
    let capture = Arc::new(Capture::default());
    let clock = Arc::new(Clock::default());
    let mut p = policy(2, &capture);
    p.base_delay = Duration::from_millis(20);
    p.timing = clock.clone();
    match selection {
        "exclude_status" => {
            p.retryable_statuses.remove(&status);
        }
        "exclude_category" => {
            p.retryable_categories.remove(&Category::Http);
        }
        "default" => {}
        _ => panic!("unknown contract selection"),
    }
    let eligible = selection == "default";
    let gateway = RecoveryGateway::new(inner, p).expect("policy");
    let result = if boundary == "gateway" {
        reviewed_operation(&gateway, operation, &RecoveryCall::default())
            .await
            .map(|_| ())
            .map_err(|e| mojentic::MojenticError::Recovery(Box::new(e)))
    } else {
        let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
        let messages = [LlmMessage::user("question")];
        if boundary == "session" {
            let mut session = mojentic::llm::ChatSession::new(broker);
            let initial = session.messages().len();
            let result = if operation == Operation::Streaming {
                let chunks: Vec<_> = session.send_stream("question").collect().await;
                chunks.into_iter().collect::<Result<Vec<_>>>().map(|v| v.concat())
            } else {
                session.send("question").await
            };
            assert_eq!(session.messages().len(), initial + if result.is_ok() { 2 } else { 1 });
            result.map(|text| assert_eq!(text, "done"))
        } else if operation == Operation::Streaming {
            let chunks: Vec<_> =
                broker.generate_stream(&messages, None, None, None).collect().await;
            chunks
                .into_iter()
                .collect::<Result<Vec<_>>>()
                .map(|v| assert_eq!(v.concat(), "done"))
        } else if operation == Operation::Structured {
            broker
                .generate_object::<Value>(&messages, None, None)
                .await
                .map(|v| assert_eq!(v, json!({})))
        } else {
            broker
                .generate(&messages, None, None, None)
                .await
                .map(|v| assert_eq!(v, "done"))
        }
    };
    if eligible {
        result.expect("selected HTTP failure must recover through public API");
    } else {
        let mojentic::MojenticError::Recovery(error) =
            result.expect_err("excluded failure must not resend")
        else {
            panic!("typed recovery failure")
        };
        assert_eq!(error.outcome, Outcome::Failed);
        assert_single_failure(&error, &capture, provider);
        assert_eq!(error.failure.http_status, Some(status));
        assert!(!error.failure.classification.eligible);
    }
    let requests = capture.requests.lock().expect("requests");
    let bodies = server.bodies.lock().expect("HTTP bodies");
    let attempts = if eligible { 2 } else { 1 };
    assert_eq!(server.sends.load(Ordering::SeqCst), attempts);
    assert_eq!(requests.len(), attempts);
    assert_eq!(bodies.len(), attempts);
    for (index, ((id, encoded), received)) in requests.iter().zip(bodies.iter()).enumerate() {
        assert_eq!(encoded, received, "observer agrees with actual HTTP request");
        assert_eq!(encoded, &requests[0].1, "retry payload is byte-identical");
        assert_eq!(id.logical_request_id, requests[0].0.logical_request_id);
        assert_eq!(id.wire_attempt, index as u32 + 1);
        if index > 0 {
            assert_ne!(id.attempt_id, requests[0].0.attempt_id);
        }
    }
    let payload: Value = serde_json::from_slice(&bodies[0]).expect("HTTP JSON request");
    assert_eq!(payload["model"], "qwen3");
    assert_eq!(payload["stream"], operation == Operation::Streaming);
    assert!(payload["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .any(|m| m["role"] == "user" && m["content"] == "question"));
    let events = capture.events.lock().expect("events");
    for (index, event) in events.iter().enumerate() {
        let attempt = usize::from(index >= 5);
        assert_eq!(event.identity.as_ref(), Some(&requests[attempt].0));
        if event.failure.is_none() {
            assert_eq!(event.delay, None);
            assert_eq!(
                event.phase,
                if event.transition == Transition::AttemptSucceeded {
                    Some(if operation == Operation::Streaming {
                        Phase::Streaming
                    } else {
                        Phase::Decoding
                    })
                } else {
                    None
                }
            );
        }
    }
    for event in events.iter().filter(|e| e.failure.is_some()) {
        let failure = event.failure.as_ref().expect("failure");
        assert_eq!(event.identity.as_ref(), Some(&requests[0].0));
        assert_eq!(event.phase, Some(Phase::Streaming));
        assert_eq!(failure.phase, event.phase);
        assert_eq!(failure.provider, provider);
        assert_eq!(failure.operation, operation);
        assert_eq!(failure.category, Category::Http);
        assert_eq!(failure.http_status, Some(status));
        assert_eq!(failure.acceptance, Acceptance::Unknown);
        assert_eq!(failure.classification.eligible, eligible);
        assert_eq!(
            failure.classification.reason,
            if eligible {
                Reason::Transient
            } else {
                Reason::Permanent
            }
        );
        assert_eq!(failure.inspect_response_bytes(), b"synthetic-secret failure");
        assert_eq!(failure.identity, event.identity);
        assert!(failure.progress.headers_received);
        assert!(!failure.progress.observed.any());
        assert!(!failure.progress.delivered.any());
        assert_eq!(
            event.delay,
            if event.transition == Transition::DelayScheduled {
                Some(Duration::from_millis(10))
            } else {
                None
            }
        );
    }
    assert_eq!(
        events.iter().map(|e| e.transition).collect::<Vec<_>>(),
        if eligible {
            vec![
                Transition::AttemptStarted,
                Transition::AttemptFailed,
                Transition::AdmissionPending,
                Transition::AdmissionAllowed,
                Transition::DelayScheduled,
                Transition::RetryStarted,
                Transition::AttemptStarted,
                Transition::AttemptSucceeded,
            ]
        } else {
            vec![Transition::AttemptStarted, Transition::AttemptFailed]
        }
    );
    assert_eq!(
        *clock.sleeps.lock().expect("sleeps"),
        if eligible {
            vec![Duration::from_millis(10)]
        } else {
            vec![]
        }
    );
    assert!(!serde_json::to_string(&*events)
        .expect("safe lifecycle")
        .contains("synthetic-secret"));
    bodies[0].clone()
}

#[tokio::test]
async fn recovery_status_selection_v1() {
    let contract: Value =
        serde_json::from_str(include_str!("contracts/recovery_status_selection_v1.json"))
            .expect("versioned executable cases");
    assert_eq!(contract["contract"], "recovery-status-selection-v1");
    assert_eq!(contract["version"], 1);
    for provider in ADAPTERS {
        for status in contract["statuses"].as_array().expect("statuses") {
            let status = u16::try_from(status.as_u64().expect("HTTP status")).expect("u16");
            for boundary in contract["boundaries"].as_array().expect("boundaries") {
                let boundary = boundary.as_str().expect("boundary");
                for operation in contract["operations"].as_array().expect("operations") {
                    let operation = match operation.as_str().expect("operation") {
                        "ordinary" => Operation::Ordinary,
                        "structured" => Operation::Structured,
                        "streaming" => Operation::Streaming,
                        _ => panic!("unknown contract operation"),
                    };
                    if boundary == "session" && operation == Operation::Structured {
                        continue;
                    }
                    let success = if operation == Operation::Streaming {
                        final_stream(provider)
                    } else {
                        response(
                            provider,
                            contract[if operation == Operation::Structured {
                                "structured_content"
                            } else {
                                "success_content"
                            }]
                            .as_str()
                            .expect("success"),
                        )
                        .to_string()
                    };
                    let mut paired_payload = None;
                    for selection in contract["selections"].as_array().expect("selections") {
                        let selection = selection.as_str().expect("selection");
                        let server = fixture(vec![
                            (
                                status,
                                contract["error_body"].as_str().expect("error").into(),
                                false,
                                false,
                            ),
                            (200, success.clone(), false, false),
                        ])
                        .await;
                        let payload = adapter_case!(
                            status_selection_case,
                            provider,
                            &server.url,
                            &server,
                            boundary,
                            operation,
                            status,
                            selection
                        );
                        if let Some(expected) = &paired_payload {
                            assert_eq!(
                                &payload, expected,
                                "changing only policy preserves HTTP input"
                            );
                        } else {
                            paired_payload = Some(payload);
                        }
                    }
                }
            }
        }
    }
}
