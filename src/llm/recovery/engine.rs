use super::{
    adapter::{RecoveryGateway, RecoveryProvider},
    frames::Frames,
    *,
};
use crate::llm::{models::LlmGatewayResponse, stream_events::LineBuffer};
use futures::{Stream, StreamExt};
use std::future::Future;
use std::{
    pin::Pin,
    time::{Duration, Instant, UNIX_EPOCH},
};
use uuid::Uuid;

/// Semantic output for one logical completion. Completed and Failed are terminal.
pub enum RecoveryStreamEvent {
    Content(String),
    Reasoning(String),
    /// Raw provider tool fragment, for explicit inspection only; never execute it.
    ToolFragment(serde_json::Value),
    Completed(Box<Recovered<LlmGatewayResponse>>),
    Failed(RecoveryError),
}
#[derive(Debug)]
pub(crate) struct PrivateEvidence {
    pub bytes: Vec<u8>,
    original: Option<std::sync::Arc<dyn std::error::Error + Send + Sync>>,
}
impl PrivateEvidence {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            original: None,
        }
    }
    pub fn from_error(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            bytes: error.to_string().into_bytes(),
            original: Some(std::sync::Arc::new(error)),
        }
    }
}
impl std::fmt::Display for PrivateEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.bytes))
    }
}
impl std::error::Error for PrivateEvidence {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.original.as_deref().map(|e| e as &(dyn std::error::Error + 'static))
    }
}
struct Attempt<'a> {
    policy: &'a RecoveryPolicy,
    provider: Provider,
    operation: Operation,
    logical: Uuid,
    number: u32,
    current: Failure,
    history: Vec<Failure>,
    first_failure: Option<Instant>,
    sent_current: bool,
    raw: Vec<u8>,
    delivered: std::sync::Arc<std::sync::Mutex<SemanticProgress>>,
}
impl<'a> Attempt<'a> {
    fn new(policy: &'a RecoveryPolicy, provider: Provider, operation: Operation) -> Self {
        Self {
            policy,
            provider,
            operation,
            logical: Uuid::new_v4(),
            number: 0,
            current: Failure::new(provider, operation, Category::Transport),
            history: vec![],
            first_failure: None,
            sent_current: false,
            raw: Vec::new(),
            delivered: Default::default(),
        }
    }
    fn emit(&self, transition: Transition, delay: Option<Duration>, failed: bool) {
        if let Some(observer) = &self.policy.observer {
            observer.observe(&LifecycleEvent {
                transition,
                identity: self.current.identity.clone(),
                phase: self.current.phase,
                progress: self.progress(),
                failure: failed.then(|| self.current.clone()),
                delay,
            });
        }
    }
    fn progress(&self) -> Progress {
        let mut progress = self.current.progress.clone();
        progress.delivered = *self.delivered.lock().unwrap_or_else(|e| e.into_inner());
        progress
    }
    fn classify(&mut self) {
        self.current.progress = self.progress();
        let f = &mut self.current;
        f.classification = if f.category == Category::Cancellation {
            Classification {
                eligible: false,
                reason: Reason::Cancelled,
            }
        } else if matches!(f.http_status, Some(400 | 401 | 403)) {
            // A body transport error cannot undo a known permanent HTTP rejection.
            Classification {
                eligible: false,
                reason: Reason::Permanent,
            }
        } else if f.progress.observed.any() || f.progress.delivered.any() {
            Classification {
                eligible: false,
                reason: Reason::SemanticOutput,
            }
        } else if self.operation != Operation::Streaming
            && f.http_status.is_some_and(|s| (200..300).contains(&s))
            && f.progress.raw_bytes > 0
        {
            // An incomplete JSON body may already contain inference output. Fail closed.
            Classification {
                eligible: false,
                reason: Reason::PartialResponse,
            }
        } else {
            let eligible = match f.category {
                Category::Http => {
                    self.policy.retryable_categories.contains(&Category::Http)
                        && f.http_status
                            .is_some_and(|s| self.policy.retryable_statuses.contains(&s))
                        && !matches!(f.http_status, Some(400 | 401 | 403))
                }
                Category::Transport => {
                    self.policy.retryable_categories.contains(&Category::Transport)
                }
                Category::ClientTimeout => {
                    self.policy.retryable_categories.contains(&Category::ClientTimeout)
                }
                _ => false,
            };
            Classification {
                eligible,
                reason: if eligible {
                    Reason::Transient
                } else if f.category == Category::Protocol {
                    Reason::Malformed
                } else {
                    Reason::Permanent
                },
            }
        };
    }
    fn failure(
        &mut self,
        category: Category,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) {
        self.current.set_response_bytes(&self.raw);
        self.current.category = category;
        self.current = self.current.clone().with_cause(cause);
        self.classify();
    }
    fn cancel(&mut self) {
        self.current.set_response_bytes(&self.raw);
        self.current.category = Category::Cancellation;
        self.classify();
    }
    fn end(&self, outcome: Outcome) -> RecoveryError {
        match outcome {
            Outcome::Cancelled => self.emit(Transition::Cancelled, None, true),
            Outcome::Interrupted => self.emit(Transition::Interrupted, None, true),
            Outcome::Exhausted => self.emit(Transition::Exhausted, None, true),
            _ => {}
        }
        RecoveryError {
            outcome,
            failure: Box::new(self.current.clone()),
            history: self.history.clone(),
        }
    }
    fn expires(&self) -> Option<Instant> {
        let budget = self
            .first_failure
            .zip(self.policy.budget)
            .and_then(|(start, budget)| start.checked_add(budget));
        match (self.policy.deadline, budget) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
    fn expired(&self) -> bool {
        self.expires().is_some_and(|end| self.policy.timing.monotonic() >= end)
    }
    async fn retry(&mut self, call: &RecoveryCall) -> Result<(), RecoveryError> {
        self.classify();
        if self.sent_current {
            self.history.push(self.current.clone());
            self.emit(Transition::AttemptFailed, None, true);
            self.sent_current = false;
        }
        if call.cancellation.is_cancelled() || self.current.category == Category::Cancellation {
            self.cancel();
            return Err(self.end(Outcome::Cancelled));
        }
        if self.current.progress.observed.any() || self.current.progress.delivered.any() {
            return Err(self.end(Outcome::Interrupted));
        }
        if !self.current.classification.eligible {
            return Err(self.end(Outcome::Failed));
        }
        if self.number >= self.policy.max_attempts.get() {
            return Err(self.end(Outcome::Exhausted));
        }
        self.first_failure.get_or_insert_with(|| self.policy.timing.monotonic());
        if self.expired() {
            return Err(self.end(Outcome::BudgetExpired));
        }
        let mut ceiling = self.policy.base_delay.min(self.policy.delay_ceiling);
        for _ in 1..self.number {
            if ceiling >= self.policy.delay_ceiling || ceiling.is_zero() {
                break;
            }
            ceiling = ceiling.saturating_mul(2).min(self.policy.delay_ceiling);
        }
        let floor = self.current.retry_after.delay().unwrap_or_default();
        if floor > self.policy.delay_ceiling {
            return Err(self.end(Outcome::DelayCeiling));
        }
        let delay = self.policy.timing.jitter(ceiling).min(ceiling).max(floor);
        if self.expires().is_some_and(|end| {
            self.policy
                .timing
                .monotonic()
                .checked_add(delay)
                .is_none_or(|start| start >= end)
        }) {
            return Err(self.end(Outcome::BudgetExpired));
        }
        if let Some(admission) = &self.policy.admission {
            self.emit(Transition::AdmissionPending, None, true);
            let context = AdmissionContext {
                failure: self.current.clone(),
                next_attempt: self.number + 1,
                previous_identity: self
                    .current
                    .identity
                    .clone()
                    .ok_or_else(|| self.end(Outcome::Failed))?,
            };
            let deadline_delay = self
                .expires()
                .map(|end| end.saturating_duration_since(self.policy.timing.monotonic()));
            let expiry = async {
                if let Some(delay) = deadline_delay {
                    self.policy.timing.sleep(delay).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            let decision = tokio::select! { biased;
                _ = call.cancellation.cancelled() => { self.cancel(); return Err(self.end(Outcome::Cancelled)); },
                decision = admission.decide(context) => decision,
                _ = expiry => return Err(self.end(Outcome::BudgetExpired)),
            };
            if decision == AdmissionDecision::Reject {
                self.emit(Transition::AdmissionRejected, None, true);
                return Err(self.end(Outcome::AdmissionRejected));
            }
            self.emit(Transition::AdmissionAllowed, None, true);
        } else if self.provider.local()
            && self.current.acceptance != Acceptance::No
            && !self.current.remote_termination_confirmed
        {
            self.emit(Transition::AdmissionRequired, None, true);
            return Err(self.end(Outcome::AdmissionRequired));
        }
        if self.expired() {
            return Err(self.end(Outcome::BudgetExpired));
        }
        if self.expires().is_some_and(|end| {
            self.policy
                .timing
                .monotonic()
                .checked_add(delay)
                .is_none_or(|start| start >= end)
        }) {
            return Err(self.end(Outcome::BudgetExpired));
        }
        self.emit(Transition::DelayScheduled, Some(delay), true);
        tokio::select! { biased;
            _ = call.cancellation.cancelled() => { self.cancel(); return Err(self.end(Outcome::Cancelled)); },
            _ = self.policy.timing.sleep(delay) => {},
        }
        if call.cancellation.is_cancelled() {
            self.cancel();
            return Err(self.end(Outcome::Cancelled));
        }
        if self.expired() {
            return Err(self.end(Outcome::BudgetExpired));
        }
        Ok(())
    }
    fn wire<'b>(
        &mut self,
        event: impl FnOnce(Identity) -> WireEvent<'b>,
    ) -> Result<(), RecoveryError> {
        if let (Some(observer), Some(id)) = (&self.policy.wire_observer, &self.current.identity) {
            if let Err(cause) = observer.observe(event(id.clone())) {
                self.current.category = Category::Protocol;
                self.current = self.current.clone().with_boxed_cause(cause);
                self.current.set_response_bytes(&self.raw);
                self.classify();
                if self.sent_current {
                    self.history.push(self.current.clone());
                    self.emit(Transition::AttemptFailed, None, true);
                    self.sent_current = false;
                } else {
                    self.current.identity = None;
                }
                return Err(self.end(Outcome::ObserverFailed));
            }
        }
        Ok(())
    }
    async fn send(
        &mut self,
        client: &reqwest::Client,
        request: &reqwest::Request,
        call: &RecoveryCall,
    ) -> Result<reqwest::Response, RecoveryError> {
        if call.cancellation.is_cancelled() {
            self.cancel();
            return Err(self.end(Outcome::Cancelled));
        }
        if self.number > 0 && self.expired() {
            return Err(self.end(Outcome::BudgetExpired));
        }
        let request = request.try_clone().ok_or_else(|| self.end(Outcome::Failed))?;
        self.sent_current = false;
        self.raw.clear();
        self.current = Failure::new(self.provider, self.operation, Category::Transport);
        self.current.identity = Some(Identity {
            logical_request_id: self.logical,
            attempt_id: Uuid::new_v4(),
            wire_attempt: self.number + 1,
        });
        self.wire(|id| WireEvent::Request {
            identity: id,
            request: &request,
        })?;
        if call.cancellation.is_cancelled() {
            self.current.identity = None;
            self.cancel();
            return Err(self.end(Outcome::Cancelled));
        }
        if self.number > 0 && self.expired() {
            self.current.identity = None;
            return Err(self.end(Outcome::BudgetExpired));
        }
        self.current.phase = None;
        if self.number > 0 {
            self.emit(Transition::RetryStarted, None, false);
        }
        self.emit(Transition::AttemptStarted, None, false);
        if call.cancellation.is_cancelled() {
            self.current.identity = None;
            self.cancel();
            return Err(self.end(Outcome::Cancelled));
        }
        if self.number > 0 && self.expired() {
            self.current.identity = None;
            return Err(self.end(Outcome::BudgetExpired));
        }
        let metadata_request = request.try_clone();
        let mut execution = Box::pin(client.execute(request));
        let mut dispatched = false;
        let response = tokio::select! { biased;
            _ = call.cancellation.cancelled() => {
                if !dispatched { self.current.identity = None; }
                else { self.number += 1; self.sent_current = true; }
                self.cancel(); return Err(self.end(Outcome::Cancelled));
            },
            response = std::future::poll_fn(|cx| { dispatched = true; execution.as_mut().poll(cx) }) => response,
        };
        self.number += 1;
        self.sent_current = true;
        match response {
            Ok(response) => {
                self.current.set_response_headers(response.headers());
                self.current.http_status = Some(response.status().as_u16());
                self.current.progress.headers_received = true;
                self.current.phase = Some(Phase::Streaming);
                self.current.retry_after = parse_retry_after(
                    response.headers().get("retry-after"),
                    self.policy.timing.wall(),
                );
                self.current.provider_request_id = response
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .and_then(ValidatedMetadata::request_id)
                    .filter(|id| {
                        metadata_request
                            .as_ref()
                            .is_some_and(|request| !echoes_request(id.as_str(), request))
                    });
                self.wire(|id| WireEvent::Headers {
                    identity: id,
                    status: response.status().as_u16(),
                    headers: response.headers(),
                })?;
                Ok(response)
            }
            Err(cause) => {
                if cause.is_connect() {
                    self.current.phase = Some(Phase::Connecting);
                }
                let category = if cause.is_timeout() {
                    Category::ClientTimeout
                } else {
                    Category::Transport
                };
                self.failure(category, cause);
                Err(self.end(Outcome::Failed))
            }
        }
    }
    fn report(&self) -> RecoveryReport {
        RecoveryReport {
            identity: self.current.identity.clone().unwrap_or(Identity {
                logical_request_id: self.logical,
                attempt_id: Uuid::nil(),
                wire_attempt: self.number,
            }),
            progress: self.progress(),
            history: self.history.clone(),
        }
    }
    fn decode_failure(&mut self, cause: PrivateEvidence) {
        let body = serde_json::from_slice::<serde_json::Value>(&cause.bytes).ok();
        let category = if body.as_ref().is_some_and(|body| !body["error"].is_null()) {
            Category::ProviderResponse
        } else {
            Category::Protocol
        };
        self.current.provider_code =
            body.and_then(|body| body["error"]["code"].as_str().and_then(ValidatedMetadata::code));
        self.failure(category, cause);
    }
    fn http_failure(&mut self, bytes: Vec<u8>) {
        self.current.provider_code = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v["error"]["code"].as_str().and_then(ValidatedMetadata::code));
        self.failure(Category::Http, PrivateEvidence::new(bytes));
    }
}
// A syntactically valid request ID can still be an echoed credential or prompt.
// Prefer omission when its spelling occurs in the immutable request.
fn echoes_request(value: &str, request: &reqwest::Request) -> bool {
    let candidates = [value, value.strip_prefix("req_").unwrap_or(value)];
    candidates.iter().any(|candidate| {
        let bytes = candidate.as_bytes();
        let contains = |source: &[u8]| source.windows(bytes.len()).any(|part| part == bytes);
        request.headers().values().any(|header| contains(header.as_bytes()))
            || request.body().and_then(|body| body.as_bytes()).is_some_and(contains)
    })
}
fn parse_retry_after(
    value: Option<&reqwest::header::HeaderValue>,
    wall: std::time::SystemTime,
) -> RetryAfter {
    let Some(value) = value else {
        return RetryAfter::Absent;
    };
    let Ok(value) = value.to_str() else {
        return RetryAfter::Invalid;
    };
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value.parse().map(RetryAfter::Seconds).unwrap_or(RetryAfter::Invalid);
    }
    let value = value.trim();
    // Resolve obsolete two-digit years against the observed wall clock before
    // validating the weekday. Chrono's fixed %y pivot is not HTTP's moving pivot.
    let obsolete = || {
        use chrono::Datelike;
        let (weekday, rest) = value.split_once(", ")?;
        let (date, time) = rest.split_once(' ')?;
        let (day_month, short_year) = date.rsplit_once('-')?;
        if short_year.len() != 2 || !short_year.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let horizon = chrono::DateTime::<chrono::Utc>::from(wall).year().saturating_add(50);
        let mut year = horizon.div_euclid(100) * 100 + short_year.parse::<i32>().ok()?;
        if year > horizon {
            year -= 100;
        }
        let expanded = format!("{weekday}, {day_month}-{year:04} {time}");
        chrono::NaiveDateTime::parse_from_str(&expanded, "%A, %d-%b-%Y %H:%M:%S GMT").ok()
    };
    let parsed = chrono::NaiveDateTime::parse_from_str(value, "%a, %d %b %Y %H:%M:%S GMT")
        .ok()
        .or_else(obsolete)
        .or_else(|| chrono::NaiveDateTime::parse_from_str(value, "%a %b %e %H:%M:%S %Y").ok());
    match parsed {
        Some(date) => {
            let seconds = date.and_utc().timestamp();
            let remaining = if seconds < 0 {
                Duration::ZERO
            } else {
                UNIX_EPOCH
                    .checked_add(Duration::from_secs(seconds as u64))
                    .and_then(|date| date.duration_since(wall).ok())
                    .unwrap_or_default()
            };
            RetryAfter::Date {
                unix_seconds: seconds,
                remaining,
            }
        }
        None => RetryAfter::Invalid,
    }
}
async fn body(
    state: &mut Attempt<'_>,
    response: reqwest::Response,
    call: &RecoveryCall,
) -> Result<Vec<u8>, RecoveryError> {
    let mut bytes = response.bytes_stream();
    let mut collected = Vec::new();
    loop {
        let chunk = tokio::select! { biased;
            _ = call.cancellation.cancelled() => { state.cancel(); state.current = state.current.clone().with_cause(PrivateEvidence::new(collected)); return Err(state.end(Outcome::Cancelled)); },
            chunk = bytes.next() => chunk,
        };
        match chunk {
            Some(Ok(chunk)) => {
                state.current.progress.raw_bytes =
                    state.current.progress.raw_bytes.saturating_add(chunk.len() as u64);
                state.raw.extend_from_slice(&chunk);
                collected.extend_from_slice(&chunk);
                if state.operation != Operation::Streaming
                    && state.current.http_status.is_some_and(|s| (200..300).contains(&s))
                {
                    if let Ok(response) = super::frames::decode(state.provider, &collected, false) {
                        super::frames::response_progress(
                            &response,
                            &mut state.current.progress.observed,
                        );
                    }
                }
                // Capture failure still retains semantic evidence already received.
                state.wire(|id| WireEvent::Body {
                    identity: id,
                    bytes: &chunk,
                })?;
            }
            Some(Err(cause)) => {
                // Once headers establish an HTTP failure, its status governs eligibility.
                // Preserve the body transport cause privately without reclassifying it.
                let category =
                    if state.current.http_status.is_some_and(|s| !(200..300).contains(&s)) {
                        Category::Http
                    } else {
                        Category::Transport
                    };
                state.failure(category, cause);
                return Err(state.end(Outcome::Failed));
            }
            None => return Ok(collected),
        }
    }
}
pub(super) async fn complete<G: RecoveryProvider>(
    gateway: &RecoveryGateway<G>,
    request: reqwest::Request,
    operation: Operation,
    call: &RecoveryCall,
) -> Result<Recovered<LlmGatewayResponse<serde_json::Value>>, RecoveryError> {
    let mut state = Attempt::new(&gateway.policy, gateway.inner.recovery_provider(), operation);
    let structured_output = operation == Operation::Structured
        || request
            .body()
            .and_then(|body| body.as_bytes())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
            .is_some_and(|body| {
                matches!(
                    body["response_format"]["type"].as_str(),
                    Some("json_object" | "json_schema")
                )
            });
    loop {
        let result = async {
            let response = state.send(&gateway.client, &request, call).await?;
            let success = response.status().is_success();
            let collected = body(&mut state, response, call).await?;
            if !success {
                state.http_failure(collected);
                return Err(state.end(Outcome::Failed));
            }
            state.current.phase = Some(Phase::Decoding);
            match super::frames::decode(
                state.provider,
                &collected,
                operation == Operation::Structured,
            ) {
                Ok(mut response) => {
                    if state.provider == Provider::Omlx && structured_output {
                        if let Some(warning) = state
                            .current
                            .inspect_response_headers()
                            .and_then(crate::llm::gateways::omlx::format_warning_headers)
                        {
                            response.evidence.metadata.insert(
                                crate::llm::gateways::omlx::RESPONSE_FORMAT_WARNING.into(),
                                serde_json::Value::String(warning),
                            );
                        }
                    }
                    super::frames::response_progress(
                        &response,
                        &mut state.current.progress.observed,
                    );
                    *state.delivered.lock().unwrap_or_else(|e| e.into_inner()) =
                        state.current.progress.observed;
                    Ok(response)
                }
                Err(cause) => {
                    state.decode_failure(cause);
                    Err(state.end(Outcome::Failed))
                }
            }
        }
        .await;
        match result {
            Ok(value) => {
                state.emit(Transition::AttemptSucceeded, None, false);
                return Ok(Recovered {
                    value,
                    report: state.report(),
                });
            }
            Err(error) if error.outcome == Outcome::ObserverFailed => return Err(error),
            Err(_) => state.retry(call).await?,
        }
    }
}
pub(super) fn stream<G: RecoveryProvider>(
    gateway: &RecoveryGateway<G>,
    request: reqwest::Request,
    call: &RecoveryCall,
) -> Pin<Box<dyn Stream<Item = RecoveryStreamEvent> + Send>> {
    let owner = StreamOwner {
        client: gateway.client.clone(),
        policy: gateway.policy.clone(),
        provider: gateway.inner.recovery_provider(),
    };
    let call = RecoveryCall {
        cancellation: call.cancellation.child_token(),
    };
    Box::pin(async_stream::stream! {
        let _guard = call.cancellation.clone().drop_guard();
        let delivered = std::sync::Arc::new(std::sync::Mutex::new(SemanticProgress::default()));
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let worker_call = call.clone();
        let worker_delivered = delivered.clone();
        tokio::spawn(async move {
            let mut source = stream_owned(owner, request, worker_call.clone(), worker_delivered);
            while let Some(event) = source.next().await {
                let terminal = matches!(event, RecoveryStreamEvent::Completed(_) | RecoveryStreamEvent::Failed(_));
                if terminal { let _ = sender.send(event).await; break; }
                tokio::select! { biased;
                    _ = worker_call.cancellation.cancelled() => {},
                    result = sender.send(event) => { if result.is_err() { break; } },
                }
            }
        });
        while let Some(mut event) = receiver.recv().await {
            if call.cancellation.is_cancelled() && !matches!(event, RecoveryStreamEvent::Failed(_) | RecoveryStreamEvent::Completed(_)) { continue; }
            {
                let mut delivered = delivered.lock().unwrap_or_else(|e| e.into_inner());
                super::frames::delivered(&event, &mut delivered);
                match &mut event {
                    RecoveryStreamEvent::Failed(error) => { error.failure.progress.delivered = *delivered;
                        if let Some(last) = error.history.last_mut().filter(|last| last.identity == error.failure.identity) { last.progress.delivered = *delivered; }
                    },
                    RecoveryStreamEvent::Completed(result) => { delivered.completed_tool_calls = result.value.tool_calls.len() as u64; result.report.progress.delivered = *delivered; },
                    _ => {},
                }
            }
            yield event;
        }
    })
}
struct StreamOwner {
    client: reqwest::Client,
    policy: RecoveryPolicy,
    provider: Provider,
}
fn stream_owned(
    gateway: StreamOwner,
    request: reqwest::Request,
    call: RecoveryCall,
    delivered: std::sync::Arc<std::sync::Mutex<SemanticProgress>>,
) -> Pin<Box<dyn Stream<Item = RecoveryStreamEvent> + Send>> {
    Box::pin(async_stream::stream! {
        let mut state = Attempt::new(&gateway.policy, gateway.provider, Operation::Streaming);
        state.delivered = delivered;
        loop {
            let response = match state.send(&gateway.client, &request, &call).await {
                Ok(response) => response,
                Err(error) => { if error.outcome == Outcome::ObserverFailed { yield RecoveryStreamEvent::Failed(error); break; } match state.retry(&call).await { Ok(()) => continue, Err(error) => { yield RecoveryStreamEvent::Failed(error); break; } } },
            };
            if !response.status().is_success() {
                match body(&mut state, response, &call).await { Ok(bytes) => state.http_failure(bytes), Err(error) if error.outcome == Outcome::ObserverFailed => { yield RecoveryStreamEvent::Failed(error); break; }, Err(_) => {} }
            } else {
                let mut bytes = response.bytes_stream();
                let mut lines = LineBuffer::default();
                let mut frames = Frames::new(state.provider);
                let mut failed = false;
                let mut raw_evidence = Vec::new();
                while !frames.terminal {
                    let chunk = tokio::select! { biased;
                        _ = call.cancellation.cancelled() => { state.cancel(); failed = true; None },
                        chunk = bytes.next() => chunk,
                    };
                    let Some(chunk) = chunk else { if !failed { state.failure(if lines.has_pending() { Category::Protocol } else { Category::Transport }, PrivateEvidence::new(b"stream ended without terminal proof".to_vec())); } break; };
                    let chunk = match chunk { Ok(chunk) => chunk, Err(cause) => { state.failure(if lines.has_pending() { Category::Protocol } else { Category::Transport }, cause); break; } };
                    state.current.progress.raw_bytes = state.current.progress.raw_bytes.saturating_add(chunk.len() as u64);
                    raw_evidence.extend_from_slice(&chunk);
                    state.raw.extend_from_slice(&chunk);
                    // Account for all semantic evidence in a received chunk before yielding any part.
                    let mut output = Vec::new();
                    for line in lines.push(&chunk) {
                        match frames.parse(&line, &mut state.current.progress.observed) {
                            Ok(events) => output.extend(events),
                            Err(cause) => { state.decode_failure(cause); failed = true; break; },
                        }
                    }
                    if let Err(error) = state.wire(|id| WireEvent::Body { identity: id, bytes: &chunk }) { yield RecoveryStreamEvent::Failed(error); return; }
                    for event in output {
                        if call.cancellation.is_cancelled() { state.cancel(); failed = true; break; }
                        yield event;
                    }
                    if failed { break; }
                }
                drop(bytes);
                if state.current.category == Category::Cancellation { state.current = state.current.clone().with_cause(PrivateEvidence::new(raw_evidence)); }
                if frames.terminal && !failed {
                    match frames.response() {
                        Ok(value) => { state.emit(Transition::AttemptSucceeded, None, false); yield RecoveryStreamEvent::Completed(Box::new(Recovered { value, report: state.report() })); return; },
                        Err(cause) => state.decode_failure(cause),
                    }
                }
            }
            match state.retry(&call).await { Ok(()) => {}, Err(error) => { yield RecoveryStreamEvent::Failed(error); break; } }
        }
    })
}
