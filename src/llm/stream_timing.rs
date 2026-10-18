//! Client timing for a logical streamed completion.
use super::gateway::{StreamChunk, StreamMetrics};
use crate::error::Result;
use futures::{Stream, StreamExt};
use serde_json::Value;
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Monotonic time source for client stream measurements.
///
/// Implementations must return nondecreasing elapsed times from a fixed origin.
/// A custom clock permits deterministic measurements in embedded environments
/// and contract tests; normal gateways use [`Instant`].
pub trait StreamClock: Send + Sync {
    /// Elapsed time from this clock's fixed origin.
    fn now(&self) -> Duration;
}
struct MonotonicClock(Instant);
impl StreamClock for MonotonicClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}
pub(crate) fn system_clock() -> Arc<dyn StreamClock> {
    Arc::new(MonotonicClock(Instant::now()))
}

#[derive(Default)]
struct Measurement {
    start: Option<Duration>,
    first: Option<(u64, &'static str)>,
    duration: Option<u64>,
    usage: Option<Value>,
}
/// Shared only between a request driver and its output adapter.
#[derive(Clone)]
pub(crate) struct StreamTiming {
    clock: Arc<dyn StreamClock>,
    measurement: Arc<Mutex<Measurement>>,
}
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
impl StreamTiming {
    pub(crate) fn new(clock: Arc<dyn StreamClock>) -> Self {
        Self {
            clock,
            measurement: Default::default(),
        }
    }
    pub(crate) fn dispatch(&self) {
        let mut state = self.measurement.lock().unwrap_or_else(|e| e.into_inner());
        // Provider usage belongs to a wire attempt; the client origin belongs
        // to the logical completion and survives admitted retries.
        state.usage = None;
        if state.start.is_none() {
            state.start = Some(self.clock.now());
        }
    }
    pub(crate) fn observe_line(&self, line: &str) {
        let text = line.trim().strip_prefix("data:").unwrap_or(line).trim();
        if let Ok(frame) = serde_json::from_str::<Value>(text) {
            self.observe(&frame);
        }
    }
    fn observe(&self, frame: &Value) {
        if frame["model"] == "keepalive" {
            return;
        }
        let mut state = self.measurement.lock().unwrap_or_else(|e| e.into_inner());
        if frame["usage"].is_object() {
            state.usage = Some(frame["usage"].clone());
        }
        if state.first.is_some() {
            return;
        }
        let delta = if frame["message"].is_object() {
            &frame["message"]
        } else {
            &frame["choices"][0]["delta"]
        };
        let nonempty = |value: &Value| value.as_str().is_some_and(|text| !text.is_empty());
        let kind = if nonempty(&delta["thinking"]) || nonempty(&delta["reasoning_content"]) {
            Some("thinking")
        } else if nonempty(&delta["content"]) {
            Some("content")
        } else if delta["tool_calls"].as_array().is_some_and(|calls| {
            calls.iter().any(|call| {
                let function = &call["function"];
                nonempty(&function["arguments"]) || function["arguments"].is_object()
            })
        }) {
            Some("tool_call")
        } else {
            None
        };
        if let (Some(start), Some(kind)) = (state.start, kind) {
            state.first = Some((millis(self.clock.now().saturating_sub(start)), kind));
        }
    }
    pub(crate) fn finish(&self, mut metrics: StreamMetrics) -> StreamMetrics {
        let mut state = self.measurement.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(start) = state.start {
            let duration = *state
                .duration
                .get_or_insert_with(|| millis(self.clock.now().saturating_sub(start)));
            metrics.request_duration_ms = Some(duration);
        }
        metrics.time_to_first_token_ms = state.first.map(|(ms, _)| ms);
        metrics.first_token_kind = state.first.map(|(_, kind)| kind.to_string());
        metrics.provider_usage = state.usage.clone();
        metrics
    }
    pub(crate) fn legacy<'a>(
        self,
        mut source: Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>>,
        provider: &'static str,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
        Box::pin(async_stream::stream! {
            let mut reported = false;
            while let Some(item) = source.next().await {
                match item {
                    Ok(StreamChunk::Metrics(metrics)) => {
                        reported = true;
                        yield Ok(StreamChunk::Metrics(self.finish(metrics)));
                    }
                    Err(error) => {
                        let metrics = self.finish(StreamMetrics { provider: provider.into(), ..Default::default() });
                        if metrics.request_duration_ms.is_some() { yield Ok(StreamChunk::Metrics(metrics)); }
                        yield Err(error);
                        return;
                    }
                    item => yield item,
                }
            }
            if !reported {
                yield Ok(StreamChunk::Metrics(self.finish(StreamMetrics { provider: provider.into(), ..Default::default() })));
            }
        })
    }
}

/// Attach only client fields; provider evidence remains unchanged.
pub(crate) fn retain_timing(
    evidence: &mut super::models::ResponseEvidence,
    metrics: &StreamMetrics,
) {
    evidence
        .metadata
        .insert("time_to_first_token_ms".into(), metrics.time_to_first_token_ms.into());
    evidence
        .metadata
        .insert("first_token_kind".into(), metrics.first_token_kind.clone().into());
    evidence
        .metadata
        .insert("request_duration_ms".into(), metrics.request_duration_ms.into());
}
