# Rust completion recovery conformance

## Current correction: October 12 amendment item 8

Source baseline: `3bf99ad21759c87ae27d8130b2d7de6c5557a7d5` plus the uncommitted
Foundry correction. This section supersedes historical item-8 assumptions below;
it does not recertify unrelated initiative decisions. The authoritative root
contract at mojentic-unify revision `4b08dcd1d87f520a4c70928c33a2bc84c228093d`
has SHA-256 `a53a3a4eb118fa4687dd703d6ce918dd40dd6cb4eda796abc6c429a6e932e7a3`.
The local October 9 copy predates that amendment. The supplied initiative brief
was read and its SHA-256 verified as
`ea6a0efa56bfb3cd74b5ce728f80cc33c6fbd853114bec94b8aabe79f5d28e6e`.

| Item 8 behavior | Executable evidence in tests/recovery.rs |
| --- | --- |
| Invalid policies cannot construct any recovery entrypoint or dispatch HTTP | `typed_errors_v1_invalid_policy_prevents_all_entrypoints`, all three public `with_recovery` adapters; concrete enum and error-source downcast |
| Concrete length cause crosses actual HTTP, gateway, broker and session | `typed_errors_v1_session_length_proof`, `typed_errors_v1_boundaries` |
| Structured truncated JSON retains length, ordinary completed tool calls do not execute, stream tool fragments do not execute | Same versioned boundary matrix, all maintained adapters |
| Original capture errors survive request/header/body failures; original serde and structural failures remain inspectable | Same matrix; direct downcasts to `ContractCaptureError`, `serde_json::Error` and `CompletionDecodeError` |
| Exactly one terminal failure, retained cause identity/history, user turn retained, zero resends/admissions/tool execution, private summaries | Same matrix, ordinary/structured/streaming gateway and broker; ordinary/streaming session |
| Existing optional response fields and legacy length success remain supported | `typed_errors_v1_optional_fields_and_legacy_length` |

Policy validation rejects non-error HTTP statuses and unsupported retry categories.
Rust's existing NonZeroU32/Duration types already enforce positive attempt limits
and nonnegative finite delays. Base delays above ceilings, zero budgets and
permanent-status selection remain valid; the latter cannot authorize retries.
The wrapper owns an immutable validated policy, so construction protects every
recovery entrypoint, including legacy delegated streams on that wrapper.
Length failures preserve received progress without treating it as delivered output.
Default telemetry/privacy, admission, request fidelity, reasoning and capability
semantics are preserved. `recovery-capabilities-v1` is unchanged.

The proof first observed ChatSession accepting `partial-secret` as a successful
reply despite `finish_reason: length` (exit 101). The corrected same HTTP/session
probe rejects it, retains the user turn, never replays, and downcasts the cause to
`CompletionDecodeError::Length` (exit 0). Full current gate and contract identities
are recorded in CONFORMANCE-3.0.0.md. Paired external verification remains missing;
no compatibility or coordinated release readiness is claimed.

## Historical recovery implementation record


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

October 12 amendment item 3 is advanced by `recovery-capabilities-v1` in
`tests/contracts/recovery_capabilities_v1.json`, SHA-256
`40e8da683ce4bfcfebf9892ca51a53a701488d463977f021922cd6317203aa76`.
`capability_contract_public_adapters_v1` verifies all five exact values and
complete endpoint-specific evidence through the actual Ollama, oMLX and OpenAI
public recovery adapters. The set is copied byte-for-byte from Elixir's reference,
not an independently authored mock. Original inspection: 2026-10-16;
provider documentation rechecked: 2026-10-17.

| Adapter | Local cancellation | Remote cancellation | Request status | Idempotency | Exact-attempt termination |
| --- | --- | --- | --- | --- | --- |
| Ollama | Supported | Unsupported | Unsupported | Unsupported | Unknown |
| oMLX | Unknown | Unsupported | Unsupported | Unknown | Unknown |
| OpenAI | Supported | Unsupported | Unsupported | Unsupported | Unknown |

Values are derived from the same production evidence returned by
`capability_evidence()`, selected by the inner adapter's typed provider.
Every declaration includes a canonical endpoint, provider documentation URL,
inspection date and basis. The recovery book page links those sources and
explains endpoint limitations. No Anthropic recovery adapter exists.
Local cancellation, stored-result retrieval and model management do not prove
exact remote termination. Embeddings/realtime remain outside this contract.
A deliberate wrong-provider selection failed exact evidence verification even
though OpenAI/Ollama values are identical; restoring provider selection passed.
`immutable_reasoning_tools_schema_and_sampling_all_adapters` retains byte-identical
retry payload and native reasoning assertions using explicit provider field
names, without consulting capability defaults. Requests, retry, cancellation and
admission logic are unchanged. See CONFORMANCE-3.0.0.md for current source identity,
gate evidence and the missing paired consumer verification.

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
The recovery suite contains 55 tests, many iterating all three adapters and
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

## Rejected terminal telemetry correction

This continuation imports only implementation commit
`638c41e16f5d8b07b163c211137cfeeb81bc9b8f` from
`foundry-task/mojentic-ru-35342b89ceaa` as a patch on reviewed base
`a76124ad` (the full base SHA is recorded in proof). Its earlier recovery matrix
and paused-terminal cancellation correction are preserved. Git refs are unchanged.

Assertion-bearing HTTP RED tests reproduced dropped telemetry independently on
raw recovery and compatibility streams for valid Ollama `done=true`,
`done_reason="length"`, `eval_count=12`, `eval_duration=6000000000`. The decoder
now returns validated telemetry alongside the original finish-validation error.
GREEN compares every progress/metric field to legacy Ollama and asserts exact
terminal `Progress -> Metrics -> Failed` ordering, identity, one attempt per
call, identical request payloads, and no completed tool delivery or retry.
Earlier Unicode content/reasoning/tool fragments retain delivered evidence;
semantic fields inside the rejected frame remain observed-only. Malformed text,
tool arguments, done flag and finish reason emit no fabricated telemetry.
OpenAI/oMLX length termination remains a failure without invented Ollama metrics.
Capture failures and cancellation at either terminal metadata boundary preserve
evidence, prevent completion/retry, and retain ordered terminal lifecycle events.

Current scoped gate results, tested source identity and coverage reconciliation
are recorded in [CONFORMANCE-3.0.0.md](CONFORMANCE-3.0.0.md). Historical gate
counts and scratch-evidence references above are prior-stage context only;
Foundry scratch is not durable repository evidence. No live inference or Harness
writes are part of this work. Foundry must finalize a reviewed commit before supplying a landed SHA.


## October 12 amendment item 2 correction

See [CONFORMANCE-3.0.0.md](CONFORMANCE-3.0.0.md) for the scoped current mapping,
executable set identity, tested base/correction distinction, gate results and
missing consumer verification. `admission_unresolved_http_matrix` and
`admission_unresolved_broker_session_http_matrix` prove completed unresolved
admission stops with `AdmissionUnresolved`, retaining the first HTTP failure and
attempt identity without resend, tool execution, AdmissionAllowed or RetryStarted.
Explicit permission/rejection and pending cancellation/budget regressions are
mapped there. Earlier whole-contract statements above are historical evidence,
not verification of untouched 3.0.0 initiative decisions.
