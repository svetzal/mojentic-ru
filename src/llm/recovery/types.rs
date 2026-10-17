//! Safe failure metadata. Payloads and original causes require explicit inspection.
use serde::Serialize;
use std::{error::Error, fmt, sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Ollama,
    Omlx,
    OpenAi,
}
impl Provider {
    pub(crate) fn local(self) -> bool {
        self != Self::OpenAi
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Ordinary,
    Structured,
    Streaming,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Transport,
    Http,
    ProviderResponse,
    Protocol,
    Cancellation,
    ClientTimeout,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Connecting,
    Sending,
    AwaitingHeaders,
    Streaming,
    Decoding,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Acceptance {
    Yes,
    No,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Supported,
    Unsupported,
    Unknown,
}
/// Endpoint facilities declared by a recovery adapter, not permission to retry.
/// Inspect `RecoveryGateway::capability_evidence` for documentation and limitations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// Legacy alias for `remote_request_cancellation`; omitted from serialization.
    #[serde(skip)]
    pub request_cancel: Support,
    /// Local cancellation; this never proves remote termination.
    pub local_request_cancellation: Support,
    /// Cancellation of an active remote request.
    pub remote_request_cancellation: Support,
    /// Status of an in-flight or ambiguously accepted request.
    pub request_status: Support,
    /// Deduplicated replay of this endpoint's request.
    pub idempotency: Support,
    /// Proof that one exact wire attempt stopped remotely.
    pub exact_attempt_termination: Support,
}
impl Default for Capabilities {
    fn default() -> Self {
        Self {
            request_cancel: Support::Unsupported,
            local_request_cancellation: Support::Unknown,
            remote_request_cancellation: Support::Unsupported,
            request_status: Support::Unsupported,
            idempotency: Support::Unknown,
            exact_attempt_termination: Support::Unknown,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SemanticProgress {
    pub reasoning_bytes: u64,
    pub content_bytes: u64,
    pub tool_fragments: u64,
    pub completed_tool_calls: u64,
}
impl SemanticProgress {
    pub fn any(self) -> bool {
        self != Self::default()
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub headers_received: bool,
    pub raw_bytes: u64,
    pub observed: SemanticProgress,
    pub delivered: SemanticProgress,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub logical_request_id: Uuid,
    pub attempt_id: Uuid,
    pub wire_attempt: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryAfter {
    Absent,
    Invalid,
    Seconds(u64),
    Date {
        unix_seconds: i64,
        remaining: Duration,
    },
}
impl RetryAfter {
    pub(crate) fn delay(&self) -> Option<Duration> {
        match self {
            Self::Seconds(s) => Some(Duration::from_secs(*s)),
            Self::Date { remaining, .. } => Some(*remaining),
            _ => None,
        }
    }
}
/// Only recognized provider codes and request-ID formats are retained by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidatedMetadata(String);
impl ValidatedMetadata {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn code(value: &str) -> Option<Self> {
        matches!(
            value,
            "rate_limit_exceeded"
                | "server_error"
                | "invalid_request_error"
                | "invalid_api_key"
                | "model_not_found"
                | "insufficient_quota"
                | "overloaded_error"
        )
        .then(|| Self(value.into()))
    }
    pub(crate) fn request_id(value: &str) -> Option<Self> {
        // UUIDs only: arbitrary provider strings can carry echoed credentials.
        Uuid::parse_str(value.strip_prefix("req_").unwrap_or(value))
            .ok()
            .map(|_| Self(value.to_owned()))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Transient,
    Permanent,
    Malformed,
    SemanticOutput,
    PartialResponse,
    Cancelled,
    InvalidConfiguration,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Classification {
    pub eligible: bool,
    pub reason: Reason,
}
#[derive(Clone, Serialize)]
pub struct Failure {
    pub provider: Provider,
    pub operation: Operation,
    pub category: Category,
    pub http_status: Option<u16>,
    pub provider_code: Option<ValidatedMetadata>,
    pub retry_after: RetryAfter,
    pub provider_request_id: Option<ValidatedMetadata>,
    pub phase: Option<Phase>,
    pub acceptance: Acceptance,
    pub progress: Progress,
    pub classification: Classification,
    pub identity: Option<Identity>,
    /// No adapter has evidence of remote termination for an exact request.
    pub remote_termination_confirmed: bool,
    #[serde(skip)]
    response_bytes: Arc<[u8]>,
    #[serde(skip)]
    response_headers: Option<Arc<reqwest::header::HeaderMap>>,
    #[serde(skip)]
    cause: Option<Arc<dyn Error + Send + Sync>>,
}
impl Failure {
    /// Explicit opt-in to potentially sensitive provider/transport evidence.
    pub fn inspect_cause(&self) -> Option<&(dyn Error + Send + Sync + 'static)> {
        self.cause.as_deref()
    }
    pub fn inspect_response_bytes(&self) -> &[u8] {
        &self.response_bytes
    }
    pub fn inspect_response_headers(&self) -> Option<&reqwest::header::HeaderMap> {
        self.response_headers.as_deref()
    }
    pub(crate) fn set_response_bytes(&mut self, bytes: &[u8]) {
        self.response_bytes = Arc::from(bytes);
    }
    pub(crate) fn set_response_headers(&mut self, headers: &reqwest::header::HeaderMap) {
        self.response_headers = Some(Arc::new(headers.clone()));
    }
    pub(crate) fn with_boxed_cause(mut self, cause: Box<dyn Error + Send + Sync>) -> Self {
        self.cause = Some(Arc::from(cause));
        self
    }
    pub(crate) fn with_cause(mut self, cause: impl Error + Send + Sync + 'static) -> Self {
        self.cause = Some(Arc::new(cause));
        self
    }
    pub(crate) fn new(provider: Provider, operation: Operation, category: Category) -> Self {
        Self {
            provider,
            operation,
            category,
            http_status: None,
            provider_code: None,
            retry_after: RetryAfter::Absent,
            provider_request_id: None,
            phase: None,
            acceptance: Acceptance::Unknown,
            progress: Progress::default(),
            classification: Classification {
                eligible: false,
                reason: Reason::Permanent,
            },
            identity: None,
            remote_termination_confirmed: false,
            response_bytes: Arc::from([]),
            response_headers: None,
            cause: None,
        }
    }
}
impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Failure")
            .field("provider", &self.provider)
            .field("operation", &self.operation)
            .field("category", &self.category)
            .field("http_status", &self.http_status)
            .field("classification", &self.classification)
            .field("identity", &self.identity)
            .field("progress", &self.progress)
            .finish()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Failed,
    Exhausted,
    Interrupted,
    Cancelled,
    AdmissionRequired,
    AdmissionRejected,
    AdmissionUnresolved,
    DelayCeiling,
    BudgetExpired,
    ObserverFailed,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryError {
    pub outcome: Outcome,
    pub failure: Box<Failure>,
    pub history: Vec<Failure>,
}
impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "completion recovery {:?}: {:?} ({:?}, HTTP {:?})",
            self.outcome,
            self.failure.classification.reason,
            self.failure.provider,
            self.failure.http_status
        )
    }
}
impl Error for RecoveryError {}
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryReport {
    pub identity: Identity,
    pub progress: Progress,
    pub history: Vec<Failure>,
}
/// Success and its exact wire accounting; response payload is intentionally not serializable here.
pub struct Recovered<T> {
    pub value: T,
    pub report: RecoveryReport,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    AttemptStarted,
    AttemptSucceeded,
    AttemptFailed,
    AdmissionPending,
    AdmissionAllowed,
    AdmissionRejected,
    AdmissionUnresolved,
    AdmissionRequired,
    DelayScheduled,
    RetryStarted,
    Exhausted,
    Interrupted,
    Cancelled,
}
#[derive(Debug, Clone, Serialize)]
pub struct LifecycleEvent {
    pub transition: Transition,
    pub identity: Option<Identity>,
    pub phase: Option<Phase>,
    pub progress: Progress,
    pub failure: Option<Failure>,
    pub delay: Option<Duration>,
}
