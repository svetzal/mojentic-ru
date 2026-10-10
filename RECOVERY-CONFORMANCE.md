# Rust completion recovery conformance

This worktree implements the approved `TRANSIENT-RECOVERY-2026-10.md` contract
from baseline `484f4dd`, preserving native reasoning prototype `67791b7`.
Foundry owns finalization: no finalized correction commit, tag, release, or harness
pin exists yet. Do not treat the baseline commit as the implementation revision.
No sibling repository, parity matrix, dependency manifest, release file, Rust
edition, feature default, runtime, or CI configuration was changed. Cargo.lock
was absent at entry and generated from the existing manifest for verification;
it remains ignored under the repository's library lockfile policy. There is no
MSRV declaration or toolchain pin; CI uses stable. This increment does not set or
raise an MSRV. A separate declared-MSRV policy decision is recommended for this library.

## Public integration

All three adapters expose `with_recovery(RecoveryPolicy) -> Result<RecoveryGateway<_>>`.
The wrapper exposes:

- `complete_with_recovery(..., &RecoveryCall)` for ordinary completions;
- `complete_json_with_recovery(..., schema, ..., &RecoveryCall)` for structured
  completions, returning `LlmGatewayResponse<Value>`;
- `complete_stream_with_recovery(..., &RecoveryCall)` for reasoning, content,
  inspectable tool fragments, terminal completion, or structured failure.

Ordinary/structured success returns `Recovered<T>` with `value` and `report`.
The report includes final attempt identity/progress and failures from prior
attempts. Failures return `RecoveryError` with outcome, final failure, and one
history entry per failed actual attempt, including the final failed send.
Admission/backoff cancellation retains the prior attempt evidence without
inventing a new wire attempt. Pre-dispatch cancellation has no attempt identity
or history entry. Provider and operation are typed, as are status, Retry-After,
acceptance, progress, classification, IDs, and remote termination evidence.

`RecoveryGateway` implements `LlmGateway`, so an existing `LlmBroker` or
`ChatSession` can use it directly. These paths propagate
`MojenticError::Recovery(Box<RecoveryError>)` without flattening metadata or
restarting a tool round. Exhaustive downstream `MojenticError` matches need the
new variant. This increment is not a published release. The original gateways
retain their legacy errors and behavior. The raw `complete_stream_events` and
broker `generate_stream_events` deliberately delegate to their original
single-send implementation; opt-in streaming recovery uses the APIs above.

See [the migration and harness examples](book/src/core/recovery.md) and the
compiled rustdoc example on `RecoveryGateway`. Explicit cancellation uses
`RecoveryCall.cancellation`; drop cancels a recovery stream's child worker.
Brokers/sessions retain their existing cancellation surface (stream drop).

## Safety and policy

The default maximum is one attempt. A positive `NonZeroU32` prevents zero-attempt
policies. Default eligibility is transport failure and HTTP 429/500/502/503/504;
callers select categories/statuses. Protocol/provider-response errors and
400/401/403 remain ineligible. Unsupported options fail before dispatch.
Eligibility never overrides observed/delivered semantic output safety.

The endpoint, headers, model, tools, schemas, controls, image bytes, messages,
and native reasoning are encoded once before sending, then cloned byte-for-byte.
OpenAI's prototype capability restrictions and native-history omission remain
unchanged; this increment does not add unsupported preservation. Hidden reqwest
retries and redirects are disabled. Completion whole-response timeouts from
provider configuration are cleared only on this opt-in path. There is no total
active-generation timeout.

Backoff is full jitter in `[0, min(ceiling, base * 2^(n-1))]` with overflow-safe
saturating doubling. `Timing` injects monotonic and wall clocks, sleeping, and
jitter. Numeric Retry-After and HTTP dates (IMF-fixdate, RFC850, asctime) are
parsed; past dates have zero remaining delay. Invalid/absent values remain
explicit. A valid minimum is never shortened. Ceiling/budget/deadline refusal
prevents sending. Duration budgets begin at the first failure; absolute limits
apply to admission/backoff and retry start, never to active generation.

Configured asynchronous admission runs before every retry. Its future may remain
pending until explicit allow/reject; cancellation/expiration closes it locally.
Without a hook, ambiguous local-provider execution returns `AdmissionRequired`.
These adapters never claim remote termination from a socket close, model unload,
HTTP timeout, or model list. They never execute tools, reset tool depth, or restart
an agent. Logical and per-attempt UUIDs provide correlation, not idempotency.

Streams count all semantic evidence parsed from a received chunk before exposing
its events. Delivered progress is acknowledged at the public stream boundary.
Any observed content/reasoning/tool fragment prevents replay, even if undelivered.
Incomplete frames at EOF or transport failure fail closed rather than being
classified as empty output. Successful HTTP partial JSON bodies also fail closed.
Only completed keepalive frames affect raw progress alone. Native tool arguments
must parse as objects before completed calls are deliverable; partial calls never
reach broker execution. Terminal stream proof is required before completion.

A background owner closes locally owned HTTP resources promptly when cancelled,
including while a consumer is paused. Final cancellation retains available
headers, raw bytes, semantic progress, and original causes privately. Final
history progress is reconciled with delivered buffered events. No progress
counter is evidence that remote inference stopped.

Default Display/Debug/serialization and typed lifecycle events omit payloads,
credentials, tool arguments, arbitrary header strings, and cause text. Metadata
uses recognized error-code vocabulary and UUID-shaped request IDs, preserving a
validated ID verbatim. IDs whose spelling occurs in outbound headers or the
immutable body are omitted as possible credential/prompt echoes. Other strings
are omitted, not sanitized heuristically.
Explicit `inspect_cause`, `inspect_response_bytes`, and
`inspect_response_headers` expose potentially sensitive evidence. Original
transport/capture causes remain typed; JSON/UTF-8 causes retain their source chain.
Raw `WireObserver` capture is opt-in and caller-owned. It receives pre-dispatch
requests, response headers, and received body chunks, including failures and
partial bodies. A pre-dispatch capture can precede cancellation or fail before
any send; use reports/history for actual wire accounting. Capture failures are
terminal, preserve their causes, and cannot trigger a resend.

## Provider capabilities

| Completion adapter | Exact active request cancel/status | Idempotency | Remote termination proof on failure |
| --- | --- | --- | --- |
| Ollama `/api/chat` | Unsupported by adapter | Unknown, no fabricated header | Never asserted |
| oMLX `/v1/chat/completions` | Unsupported by adapter | Unknown, no fabricated header | Never asserted |
| OpenAI `/chat/completions` | Unsupported by adapter | Unknown, no fabricated header | Never asserted |

Current provider sources inspected October 10, 2026:
[Ollama chat API](https://docs.ollama.com/api/chat),
[oMLX documentation](https://github.com/jundot/omlx), and
[OpenAI chat completions API](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create).
The table describes implemented adapter facilities, not an assertion that all
upstream products lack cancellation/status/idempotency. Stored completion
retrieval or model-level management does not establish exact active request
termination. Embeddings and realtime voice are outside this contract.

## Executed public HTTP assertions

All new integration tests use loopback scripted HTTP or mockito, with synthetic
fixture credentials only. No live inference, Generation45 restart, or model
management operation was used. `tests/recovery.rs` exercises public APIs:

| Contract evidence | Executed assertions |
| --- | --- |
| 503 → success | Ordinary and streaming on all adapters: exact sends, byte-identical payload, stable logical/distinct attempt IDs, history and ordered lifecycle |
| Structured requests | All adapters: parsed object and frozen schema; Ollama/oMLX native reasoning; existing OpenAI restrictions preserved |
| Tools/schema/history/controls | Combined immutable request on all adapters; successful streamed native reasoning/tool broker history retained |
| 429 | All adapters and ordinary/structured/streaming APIs: numeric/IMF date floor within limits, ceiling and budget refusal, exact lifecycle/attempt IDs/history/frozen payload; existing tests cover all three date forms, past/invalid values |
| Persistent 504 | All adapters: bounded sends, final numeric status, one-based identities, complete bounded history |
| Local ambiguity | Missing/rejected/pending/explicitly allowed admission; pending does not dispatch or consume an attempt |
| Cancellation | Active ordinary HTTP, paused streaming consumer, pending admission, backoff, pre-dispatch: prompt closure/no resend and retained evidence |
| Partial interruption | Content, reasoning, tool fragments on every adapter; newline-free and truncated framing; partial HTTP200 JSON body; no replay or false success |
| Keepalive | All adapters: valid raw-only frame → EOF → admitted recovery; exact raw history, zero failed-attempt semantics, immutable resend and ordered lifecycle |
| Permanent/protocol/options | 400/401/403, including truncated bodies across all adapters and ordinary/structured/stream APIs despite selected statuses and explicit admission: exactly one send; malformed JSON and unsupported reasoning remain terminal |
| Privacy/causes | Secret sentinel absent from safe events/errors/serialization; raw evidence inspectable; typed capture cause retained; unsafe metadata omitted |
| Broker/session | All adapters: broker ordinary/structured/streaming and session ordinary/streaming retain exact HTTP/protocol/transport/interruption evidence; no partial assistant commit, incomplete tool execution or repeated completed-tool execution |
| Limits | Absolute deadline refuses only retry; admitted generation completes after recovery budget; legacy provider timeout does not become active timeout |
| Capture failure | Request/header/body capture errors: no resend, original cause retained, failed actual attempt in history/lifecycle; all adapters retain observed content/reasoning/tool evidence on Body capture failure with zero delivery |
| Compatibility/capabilities | All adapters: recovery-wrapped public raw broker events retain exact legacy 503 error and one send with no recovery observer calls; native prototype preserved; original regression suite |

`.foundry/proof.json` contains actual seed assertion evidence and final check
results. The seed proof is not itself whole-contract parity certification.
The preserved implementation was imported from
`foundry-task/mojentic-ru-33b3e315aafb` at
`725bde7de01a493ca1c87ad65ee9b433f9901556` onto base `484f4dd` as a working-tree
patch. No commits or task refs were changed. Earlier gate results on that ref
are historical; only checks recorded for this worktree count below.

The preserved ref records a prior RED permanent-body regression:
a truncated HTTP400 was resent until exhaustion. The correction retains HTTP
classification after non-success body transport errors and makes 400/401/403
permanently ineligible independently of category. The original typed transport
cause, headers and received body remain privately inspectable. The regression
covers 27 public adapter/operation/status combinations and asserts one actual send,
one history entry, safe lifecycle, no admission or backoff, and no secret leakage.

The preserved ref also corrects a received-progress gap found by earlier
independent review on Body capture failure. Accounting now parses received semantic evidence before calling the wire
hook and before delivering output. A failing hook remains terminal. Public tests
cover ordinary/structured output and separate streaming content/reasoning/tool
frames on all adapters, retaining observed progress with no semantic delivery.
Broker/session tests assert exact raw/header/progress/identity/history retention.
The reviewer found no further production blocker in the reviewed contract paths;
this does not exhaust every input combination or certify six-port parity.

## Independent-review remainder on bc8150a

This work extends the public conformance measurements on
`bc8150a381a540e6320cd4fd91a8bbecadb9a735`. It changes tests and evidence only;
no runtime, dependency, package version or release behavior changed.
The task started clean and synced with `origin/main`. No prior
`.foundry/proof.json` was present in this checkout; the new record retains
baseline/final source fingerprints and captured logs without inventing prior proof.

Seven added matrix tests measure 144 cases across all three maintained adapters
through public APIs. Retry-After floors and cancellation in admission/backoff cover ordinary,
structured and streaming completion. An injected pending sleeper makes backoff
cancellation deterministic. Pending admission also stays unsent until explicit
allow. Truncated successful bodies cover ordinary and structured completion;
keepalive recovery uses provider-valid empty messages or SSE comment frames.

Broker failure assertions cover ordinary, structured and streaming APIs. Session
assertions cover ordinary and streaming; `ChatSession` has no structured API.
HTTP bodies/headers match synthetic evidence exactly. Protocol causes retain the
original `serde_json::Error`, including classification and location. Body transport
causes retain `reqwest::Error` and the precise nested HTTP truncation evidence.
Final/history causes share object identity. Streaming transport tests before
semantic output disable transport eligibility explicitly to measure terminal
cause propagation; this policy does not test default transparent transport retry.
Interruption tests retain exact observed/delivered content, reasoning and tool
fragment counters, without completing tools or committing partial replies.
A separate completed-tool/follow-up-failure matrix proves one execution and a
new logical completion identity through both broker and session.

Initial failing runs exposed test compilation/assertion/fixture mistakes, not a
runtime contract defect. Those logs are retained as measurement-development RED;
GREEN runs validate corrected public assertions. No production RED/GREEN fix
was necessary. Read-only independent Codex review inspected the whole contract,
including raw event delegation and native reasoning compatibility. Its final
minor progress-counter assertion gap was closed. Foundry still owns final review
and Git finalization; the base SHA is the reviewed landed implementation, and
there is no newly landed SHA for this uncommitted measurement extension.

## Cancellation correction

Public regression tests first reproduced both reviewed defects: terminal-only
Ollama output returned completion after the body wire hook cancelled the call;
ordinary active-body cancellation emitted
`AttemptStarted, Cancelled, AttemptFailed, Cancelled`.
The engine now checks cancellation before terminal stream success, including
chunks with no semantic events. Attempt helpers return provisional errors;
recovery finalization records the failed send before emitting one `Cancelled`.
No `AttemptSucceeded` or `Completed` is allowed in these cancellation paths.

Tests exercise all three adapters through public ordinary, structured and
streaming APIs, asserting one send, one terminal event, exact ordered lifecycle,
identity/history, privately inspectable headers/raw evidence, observed semantic
progress, zero delivery when cancelled in the body hook, and safe summaries.
The paused-consumer test now covers all adapters and retains delivered content
alongside received reasoning/tool evidence while closing local resources.
Independent review corrected the OpenAI/oMLX terminal-only fixtures to include
safe finish proof before `[DONE]`; bare `[DONE]` was not valid terminal proof.
These assertions cover terminal-only chunks as well as content, reasoning and
tool chunks. They do not exhaust every concurrent cancellation interleaving.

## Validation and boundaries

Current gate outcomes and complete captured log paths are recorded in
`.foundry/proof.json`. Coverage retains the unchanged 56% floor. Deny and audit
are unfiltered; no advisory suppression or dependency manifest change is part
of this work. The current gates passed:

| Gate | Current outcome |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed |
| `cargo test --verbose --all-features` | Passed: 648 unit/integration tests, 19 doctests; 24 existing ignored doctests |
| `cargo test` | Passed with default features: same counts |
| `cargo build --verbose --all-features` | Passed |
| `cargo doc --no-deps --all-features` | Passed |
| `cargo deny check` | Passed unfiltered; existing duplicate dependency warnings |
| `cargo audit` | Passed unfiltered |
| `cargo tarpaulin --all-features --fail-under 56` | Passed: 69.47% (3263/4697 lines); unchanged 56% floor |
| `mdbook build book` | Exit 0, HTML generated; existing `truncate-long-entries` configuration diagnostic remains |
| Markdown lint | Passed with MD013 disabled for existing long tables/URLs |

Independent read-only Codex review found no remaining blocker in the inspected
contract paths. Its separate public matrix run passed all 7 matching tests.
The full logs, RED failures, GREEN assertion cases and source fingerprints are
retained in `.foundry/proof.json`. No claim of exhaustive concurrent cancellation
or every possible input combination follows from these checks.

There is no claim of six-port parity or live-provider efficacy. A fresh registered
harness experiment, finalized implementation commit, and harness lockfile pin
remain the controller's integration work. Never restart the frozen experiment.
The recovery suite contains 49 tests, many iterating all three adapters and
multiple failure modes. Foundry owns review and ref finalization; this worktree
intentionally remains uncommitted and has no reviewed landed SHA yet.

## Recovery stream telemetry prerequisite

The additive public events are `Progress { identity: Identity, progress:
StreamProgress }` and `Metrics { identity: Identity, metrics: StreamMetrics }`.
They map unchanged to compatibility `StreamChunk` variants. Ollama reuses its
existing provider helpers: UTF-8 byte counts, pre-frame accumulated tool count,
actual final durations/counts, optional metrics, and throughput calculation.
Each attempt has its own frame index. Metadata is nonsemantic and does not
change retry admission, terminal proof, or interaction accounting.
OpenAI/oMLX retain their original lack of these telemetry events; no equivalent
Ollama counters are invented. See the migration section in
[recovery documentation](book/src/core/recovery.md#decoded-stream-progress-and-metrics).

The assertion-bearing RED measured zero decoded progress events in the recovery
compatibility stream versus two required frames. GREEN proves both frame events
and final metrics. A frozen HTTP script compares original Ollama, public recovery,
and compatibility values/order, Unicode output, tools, identity, immutable native
reasoning payloads and actual metrics. Further tests measure frame resets after
503 or safely admitted keepalive-only failure, absent metrics, unsupported SSE
telemetry, and paused keepalive cancellation with no semantic delivery.
A second RED exposed completion finalized behind buffered terminal metadata.
The worker now reserves delivery capacity before advancing the provider stream;
GREEN proves paused terminal-progress cancellation ends with `AttemptFailed`
then one `Cancelled`, without `AttemptSucceeded` or completed tools.
The earlier adapter cancellation/capture/interruption and Foundry remainder
matrices remain intact; only metadata-aware consumption assertions were added.
Full capture logs and source fingerprints are in `.foundry/proof.json`.
Foundry owns independent review and finalization; no reviewed landed SHA exists
for these uncommitted changes. The coordinator must wait for that finalized
revision before repinning Harness or launching fresh real inference.
