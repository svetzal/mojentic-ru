use super::types::*;
use async_trait::async_trait;
use std::{
    collections::BTreeSet,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio_util::sync::CancellationToken;

/// Inject both monotonic and wall time, sleep, and full jitter for deterministic tests.
#[async_trait]
pub trait Timing: Send + Sync {
    fn monotonic(&self) -> Instant;
    fn wall(&self) -> SystemTime;
    fn jitter(&self, ceiling: Duration) -> Duration;
    async fn sleep(&self, delay: Duration);
}
pub struct SystemTiming;
#[async_trait]
impl Timing for SystemTiming {
    fn monotonic(&self) -> Instant {
        Instant::now()
    }
    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
    fn jitter(&self, ceiling: Duration) -> Duration {
        let random = UuidBytes::random();
        Duration::from_nanos(
            (ceiling.as_nanos().min(u64::MAX as u128) * random as u128 / u64::MAX as u128) as u64,
        )
    }
    async fn sleep(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }
}
struct UuidBytes;
impl UuidBytes {
    fn random() -> u64 {
        let bytes = *uuid::Uuid::new_v4().as_bytes();
        u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[9], bytes[10],
        ])
    }
}
#[derive(Debug, Clone)]
pub struct AdmissionContext {
    pub failure: Failure,
    pub next_attempt: u32,
    pub previous_identity: Identity,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionDecision {
    Allow,
    Reject,
    /// The hook completed without granting or rejecting permission. Never resend.
    Unresolved,
}
/// Only `Allow` grants permission. `Unresolved` completes without a decision and stops
/// recovery; a pending future stays pending until a decision, cancellation, or expiry.
#[async_trait]
pub trait Admission: Send + Sync {
    async fn decide(&self, context: AdmissionContext) -> AdmissionDecision;
}
pub trait LifecycleObserver: Send + Sync {
    fn observe(&self, event: &LifecycleEvent);
}
/// Wire capture is explicit and sensitive. The library never stores captures by default.
pub enum WireEvent<'a> {
    Request {
        identity: Identity,
        request: &'a reqwest::Request,
    },
    Headers {
        identity: Identity,
        status: u16,
        headers: &'a reqwest::header::HeaderMap,
    },
    Body {
        identity: Identity,
        bytes: &'a [u8],
    },
}
pub trait WireObserver: Send + Sync {
    /// A capture failure terminates recovery; it never causes a resend.
    fn observe(&self, event: WireEvent<'_>)
        -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
#[derive(Clone)]
pub struct RecoveryPolicy {
    pub max_attempts: NonZeroU32,
    pub base_delay: Duration,
    pub delay_ceiling: Duration,
    pub retryable_statuses: BTreeSet<u16>,
    /// Protocol and provider-response errors remain ineligible without a proven provider policy.
    pub retryable_categories: BTreeSet<Category>,
    pub budget: Option<Duration>,
    pub deadline: Option<Instant>,
    pub timing: Arc<dyn Timing>,
    pub admission: Option<Arc<dyn Admission>>,
    pub observer: Option<Arc<dyn LifecycleObserver>>,
    pub wire_observer: Option<Arc<dyn WireObserver>>,
}
impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: NonZeroU32::MIN,
            base_delay: Duration::from_millis(100),
            delay_ceiling: Duration::from_secs(30),
            retryable_statuses: [429, 500, 502, 503, 504].into_iter().collect(),
            retryable_categories: [Category::Transport, Category::Http].into_iter().collect(),
            budget: None,
            deadline: None,
            timing: Arc::new(SystemTiming),
            admission: None,
            observer: None,
            wire_observer: None,
        }
    }
}
/// Per-call cancellation, independent of remote inference termination.
#[derive(Clone, Default)]
pub struct RecoveryCall {
    pub cancellation: CancellationToken,
}

/// Invalid recovery options, rejected before a wrapper can dispatch a request.
/// Positive attempt counts and nonnegative finite delays are enforced by Rust's
/// `NonZeroU32` and `Duration` types. Base delays above the ceiling are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryPolicyError {
    /// Only HTTP error statuses can be selected for recovery.
    #[error("retryable status must be an HTTP error status: {status}")]
    InvalidStatus { status: u16 },
    /// No provider policy proves replay safe for this category.
    #[error("unsupported retryable category: {category:?}")]
    UnsupportedCategory { category: Category },
}
impl RecoveryPolicy {
    /// Validate options before creating a recovery gateway.
    ///
    /// # Errors
    /// Returns a concrete policy error for non-error HTTP statuses or categories
    /// without a supported recovery policy. Permanent statuses remain ineligible
    /// even when explicitly selected.
    pub fn validate(&self) -> Result<(), RecoveryPolicyError> {
        for &status in &self.retryable_statuses {
            if !(400..=599).contains(&status) {
                return Err(RecoveryPolicyError::InvalidStatus { status });
            }
        }
        for &category in &self.retryable_categories {
            if !matches!(category, Category::Transport | Category::Http | Category::ClientTimeout) {
                return Err(RecoveryPolicyError::UnsupportedCategory { category });
            }
        }
        Ok(())
    }
}
