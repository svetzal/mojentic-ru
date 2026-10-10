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
    let body = format!(
        "{}{}{}",
        frame(Provider::Omlx, "content"),
        frame(Provider::Omlx, "reasoning"),
        frame(Provider::Omlx, "tools")
    );
    let server = fixture(vec![(200, body.clone(), true, true)]).await;
    let capture = Arc::new(Capture::default());
    let gateway = omlx(&server.url).with_recovery(policy(3, &capture)).expect("gateway");
    let call = RecoveryCall::default();
    let mut stream = gateway.complete_stream_with_recovery(
        "qwen3",
        &[],
        None,
        &CompletionConfig::default(),
        &call,
    );
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
    async fn check<G: RecoveryProvider>(inner: G, server: &HttpFixture) {
        let capture = Arc::new(Capture::default());
        let gateway = RecoveryGateway::new(inner, policy(2, &capture)).expect("gateway");
        let mut message = LlmMessage::assistant("prior");
        message.reasoning_content = Some("native prior reasoning".into());
        let config = CompletionConfig {
            temperature: 0.5,
            top_p: Some(0.7),
            max_tokens: 123,
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
        assert_eq!(
            body["messages"][0][if gateway.capabilities().request_status == Support::Unsupported
                && body["options"].is_object()
            {
                "thinking"
            } else {
                "reasoning_content"
            }],
            "native prior reasoning"
        );
        assert_eq!(result.report.history.len(), 1);
    }
    let server = fixture(vec![
        (503, "fail".into(), false, false),
        (200, response(Provider::Ollama, "{\"ok\":true}").to_string(), false, false),
    ])
    .await;
    check(OllamaGateway::with_host(&server.url), &server).await;
    let server = fixture(vec![
        (503, "fail".into(), false, false),
        (200, response(Provider::Omlx, "{\"ok\":true}").to_string(), false, false),
    ])
    .await;
    check(omlx(&server.url), &server).await;
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
            temperature: 0.5,
            top_p: Some(0.6),
            num_predict: Some(123),
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
    assert_eq!(capabilities.request_cancel, Support::Unsupported);
    assert_eq!(capabilities.request_status, Support::Unsupported);
    assert_eq!(capabilities.idempotency, Support::Unknown);
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
