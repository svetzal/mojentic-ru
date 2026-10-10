# Recovering one completion request

Transient completion recovery is opt-in for Ollama, oMLX, and OpenAI. It preserves
one encoded HTTP request across attempts. A retry never starts the broker tool
loop again. Embeddings, realtime voice, model management, and the original
`generate_stream_events` API keep their existing behavior.

Wrap the gateway before giving it to a broker or chat session:

```rust,no_run
use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::recovery::{RecoveryPolicy, RecoveryCall, Outcome};
use mojentic::llm::gateway::CompletionConfig;
use std::{num::NonZeroU32, sync::Arc, time::Duration};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let policy = RecoveryPolicy {
    max_attempts: NonZeroU32::new(3).expect("positive attempt limit"),
    budget: Some(Duration::from_secs(20)),
    ..Default::default()
};
let gateway = OllamaGateway::new().with_recovery(policy)?;
let call = RecoveryCall::default();
let messages = [LlmMessage::user("Explain ownership")];
let result = gateway.complete_with_recovery(
    "qwen3", &messages, None, &CompletionConfig::default(), &call,
).await;
match result {
    Ok(result) => {
        println!("wire attempts: {}", result.report.identity.wire_attempt);
        // Consume result.value as the existing LlmGatewayResponse.
    }
    Err(error) if error.outcome == Outcome::AdmissionRequired => {
        // The harness must establish ownership/termination and explicitly admit
        // recovery. No second request was dispatched.
    }
    Err(error) => return Err(error.into()),
}
let broker = LlmBroker::new("qwen3", Arc::new(gateway), None);
// broker.generate(...) and ChatSession propagate MojenticError::Recovery.
# Ok(())
# }
```

The default policy allows only one attempt. Setting a larger maximum selects
eligibility; it does not authorize an ambiguous local inference resend. Implement
`Admission::decide(AdmissionContext)` to await your harness's checks and return
`Allow` or `Reject`. The context includes the previous failure, logical and
attempt identity, next wire number, and observed/delivered progress. A pending
future stays pending and is cancelled when the call is cancelled or its recovery
budget expires. Every retry invokes the hook when configured, including OpenAI.

`complete_json_with_recovery` returns a parsed object and the same report.
`complete_stream_with_recovery` returns content, reasoning, tool fragments,
nonsemantic progress/metrics when supported, and exactly one `Completed` or
`Failed` event. Fragments are inspection data; execute
only complete calls after terminal success. Wrapping the gateway also enables
recovery on `LlmGateway::complete_stream`, including broker/session streaming.
Native thinking remains available to broker-managed tool rounds when
`preserve_reasoning` is enabled. Raw `complete_stream_events` and
`generate_stream_events` deliberately remain the original single-send APIs.

Any observed reasoning, content, or tool fragment prevents replay, including
output buffered before delivery. A partial successful HTTP JSON body also
prevents replay conservatively. Incomplete stream frames fail closed. Complete
keepalive frames affect raw bytes and decoded frame progress only. Cancellation yields `Cancelled`, retains
received evidence, and closes owned response resources even if a streaming
consumer pauses. Dropping a recovery stream cancels its owned worker. Closing a
socket proves local cleanup; it does not prove remote inference terminated.
Cancellation from a body wire observer also wins over terminal-only stream
completion. An active cancelled send emits `AttemptStarted`, `AttemptFailed`,
then one `Cancelled`; it emits neither `AttemptSucceeded` nor `Completed`.
The failure retains both observed and delivered progress with attempt history.

Recovery budgets start at the first failure. Absolute deadlines and budgets
limit admission, waiting, and the start of a retry; they never impose a timeout
on active generation. Provider configuration whole-response timeouts are omitted
on recovery completion requests. Cancel with `call.cancellation.cancel()`.
Retries use exponential full jitter; numeric and HTTP-date Retry-After values
set a minimum. An excessive minimum is refused rather than shortened. Inject
`Timing` to control monotonic/wall clocks, sleeping, and jitter in harness tests.

Safe errors and lifecycle events omit response text, tool arguments, credentials,
and original cause formatting. `inspect_cause`, `inspect_response_bytes`, and
`inspect_response_headers` explicitly expose potentially sensitive evidence.
`LifecycleObserver` receives typed safe metadata. Request IDs are validated and
omitted when their spelling echoes outbound credentials or prompt text. `WireObserver` is a separate
opt-in HTTP-boundary hook receiving raw requests, response headers, and each
received body chunk, including partial/failing responses. Its request callback
runs before dispatch; cancellation or capture failure can still prevent that
send. Use reports/history for actual attempt accounting. Capture errors retain semantic progress already parsed from received bytes, stop
recovery before delivery, and never cause another inference request. Callers own capture storage
and must apply their credential/payload policy.

| Adapter | Remote request cancellation | Remote request status | Idempotency |
| --- | --- | --- | --- |
| Ollama | Unsupported by this adapter | Unsupported by this adapter | Unknown; no invented header |
| oMLX | Unsupported by this adapter | Unsupported by this adapter | Unknown; no invented header |
| OpenAI chat completions | Unsupported by this adapter | Unsupported for active requests | Unknown; no invented header |

Model unload/list operations are aggregate model operations, not termination
proof for this request. Logical/attempt UUIDs are correlation IDs only.

Existing gateways are unchanged when used without this wrapper. The wrapper's
`MojenticError::Recovery` retains the structured failure for broker/session
callers; update exhaustive matches on `MojenticError` for the new variant. Direct
recovery methods return `RecoveryError` and `Recovered<T>` without flattening
metadata into a legacy error string. See `RECOVERY-CONFORMANCE.md` in the
repository for assertions, limitations, and validation evidence.

Known HTTP 400/401/403 responses remain permanent even when their bodies are
truncated. A body transport error preserves the HTTP status and private original
cause; selecting those statuses or allowing admission cannot authorize a resend.

## Decoded stream progress and metrics

Recovery streams now expose `RecoveryStreamEvent::Progress { identity, progress }`
and `RecoveryStreamEvent::Metrics { identity, metrics }`. Add these arms to
exhaustive event matches when migrating. `identity` correlates each event to the
logical request, attempt ID, and one-based wire attempt. Ollama frame indices
start at one again on each attempt, including after a safely admitted retry.

The values are the same `StreamProgress` and `StreamMetrics` used by the original
`OllamaGateway::complete_stream`. Progress precedes semantic output from each
valid NDJSON frame, including empty keepalives and the terminal frame. Its
accumulated tool count describes calls accumulated **before** that frame.
The existing `content_chars` and `thinking_chars` fields measure UTF-8 byte
lengths; their names and values are preserved for compatibility. Terminal metrics
follow terminal progress; completed tool calls are delivered only after provider
terminal proof. Recovery's `LlmGateway::complete_stream` maps these events to
`StreamChunk::Progress` and `StreamChunk::Metrics`, preserving the values.

A structurally valid Ollama terminal frame with an unsuccessful finish reason
(for example `done_reason="length"`) still emits exact `Progress`, then `Metrics`,
then `Failed`. Compatibility streams emit the same metadata followed by the
recovery error. Telemetry describes what the provider reported, rather than
certifying a successful reply. Semantic fields in that rejected frame count as
observed evidence, but are withheld from delivery; no completed tools or retry
follow. Earlier delivered semantic output remains accounted for. Malformed
fields do not produce telemetry from unvalidated data. Cancellation and capture
failure can stop delivery before metadata reaches the consumer while retaining
received semantic evidence in the failure report.

Ollama durations/counts come directly from provider fields; absent fields and
undefined throughput remain `None`. OpenAI and oMLX do not expose these Ollama
frame counters or final duration metrics through their existing compatibility
streams, so recovery emits neither event for them. Provider response evidence
remains available on completion. Do not infer missing telemetry or fabricate
local/cloud equivalents.

Progress and metrics are nonsemantic metadata. They do not increment delivered
content, reasoning, tool fragments, completed tools, or model/tool interactions.
Keepalives can inform a harness's progress/stall classification, but never prove
remote termination or authorize another attempt. Wire body observation remains
raw transport evidence and is not a substitute for decoded frame progress.
Cancellation and capture failures retain the existing observed/delivered
semantic counters; metadata never changes replay safety or admission requirements.
