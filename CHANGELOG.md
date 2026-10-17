# Changelog

All notable changes to the Mojentic Rust implementation will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Recovery adapters expose five endpoint-specific capability values and
  `capability_evidence()` with provider citations, inspection dates and limits.
  Exact-attempt termination remains unknown for all three adapters.
- Opt-in transient completion recovery for Ollama, oMLX and OpenAI ordinary,
  structured and streaming requests. Immutable request replay, bounded jittered
  backoff and Retry-After floors, asynchronous admission, cancellation and
  recovery budgets, structured failure/history and per-attempt identities,
  safe lifecycle observations and explicit private wire inspection.
- Recovery gateways work with brokers and chat sessions without restarting tool
  loops. Ollama recovery streaming reports provider progress and metrics while
  retaining terminal evidence and refusing replay after semantic output.
- Native assistant reasoning history, opt-in broker retention, explicit model
  reasoning profiles, and Enabled/XHigh effort controls (the Rust prototype).

### Fixed

- Recovery-enabled ordinary and structured length-terminated responses now fail
  with `CompletionDecodeError::Length` before parsing truncated JSON or tools.
  Streaming length failures retain the same concrete cause through the gateway,
  broker and session; incomplete tools never execute and length never replays.
- Decoding failures expose their concrete `CompletionDecodeError` or original
  `serde_json::Error` through `Failure::inspect_cause()`. Capture callbacks retain
  their original error type. Safe summaries still omit private causes and bodies.

- A completed admission hook without a decision now stops recovery without a
  resend or tool execution. `AdmissionDecision::Unresolved` produces the distinct
  `Outcome::AdmissionUnresolved` and `Transition::AdmissionUnresolved`, preserving
  the initial failure/history and identity. Pending hooks still await explicit
  permission, cancellation, or expiry.
- Preserve named Ollama effort values rather than collapsing them to true.
- Apply reasoning controls to structured Ollama requests as well as normal and
  streaming calls. Preserve native history through oMLX and Ollama tool rounds.

### Breaking changes

- Invalid recovery category/status selections now fail during `with_recovery`
  or `RecoveryGateway::new`, before any entrypoint can dispatch HTTP, with
  `MojenticError::InvalidRecoveryPolicy(RecoveryPolicyError)`. Select HTTP error
  statuses (400–599) and supported Transport/Http/ClientTimeout categories.
  Explicit permanent-status selections remain valid but cannot authorize retry.
  `NonZeroU32`/`Duration` already exclude zero attempts and negative/nonfinite delays.
- Migration: include `InvalidRecoveryPolicy` in exhaustive `MojenticError` matches.
  Recovery callers must handle length termination as failure instead of consuming
  an incomplete ordinary/structured reply. Match `CompletionDecodeError::Length`
  by downcasting `error.failure.inspect_cause()`; inspect `RecoveryError.history`
  for the same original cause. Downcast serde errors directly from
  `inspect_cause()` rather than walking the former private wrapper's `source()`.
  Legacy gateways without recovery retain their
  previous length behavior. Raw event-stream APIs remain legacy single-send paths.

- Recovery capability serialization replaces `request_cancel` with
  `remote_request_cancellation`, and adds `local_request_cancellation` and
  `exact_attempt_termination`. Rust retains `request_cancel` as a legacy remote
  cancellation alias; capability struct literals need the new fields.
  Serialized capabilities use the five explicit contract names. Ollama/OpenAI
  idempotency is now `Unsupported` from inspected endpoint documentation; oMLX idempotency remains `Unknown`.
- Post-2.1.0 public message/config fields and reasoning enum variants require
  literal and exhaustive-match migrations. `MojenticError::Recovery` adds a
  structured error variant; recovery admission/outcome/lifecycle enums now also
  distinguish completed unresolved admission from rejection.

### Migration

- Migrate serialized `request_cancel` key reads to `remote_request_cancellation`;
  existing Rust field reads remain supported as a remote cancellation alias;
  use `local_request_cancellation` for local stream cancellation and inspect
  `capability_evidence()` for each declaration's endpoint, citation, date and
  basis. Add the three new fields to capability literals or use
  `..Default::default()` for unbound declarations. Obtain provider-specific
  values from the actual recovery adapter. Local cancellation does not prove
  remote termination or authorize replay; retry/admission behavior is unchanged.
- Use message constructors or initialize `reasoning_content`; initialize new
  completion config fields with `..Default::default()`. Select a reasoning
  profile explicitly and opt into `preserve_reasoning` to retain native reasoning
  during broker tool rounds. Enabled and XHigh are distinct effort controls.
- Update exhaustive error and recovery enum matches. Return `Allow` only after
  granting permission, `Reject` for rejection, or `Unresolved` for a completed
  absence of decision; await a pending decision as before. Handle
  `admission_unresolved` separately in serialized lifecycle/outcome consumers.
- Wrap an existing gateway with `with_recovery(RecoveryPolicy)` to opt in; defaults
  keep one attempt. See the recovery book page for reports, privacy and local
  provider admission rules. No version bump or release is assigned by this change.

## [2.1.0] - 2026-09-30

### Added

- Add a single-response broker API for caller-owned context and native tool requests.
- Support explicit unlimited tool rounds while retaining finite defaults.
- The unlimited builder uses `usize::MAX` as a symbolic value and bypasses the iteration check; existing numeric configuration remains compatible.
- The OpenAI gateway now sends `CompletionConfig.response_format` in streaming and non-streaming chat requests (`{"type": "text"}`, `{"type": "json_object"}`, or `{"type": "json_schema", ...}`). Previously it ignored the setting. Ollama already forwarded `format` in both paths; tests now cover every value for streaming requests. The format records what was requested; callers still validate content.
- `LlmGatewayResponse` has a new `evidence: ResponseEvidence` field with the provider-reported `usage`, `provider_model`, `finish_reason` and `metadata`. The OpenAI and Ollama gateways fill it for ordinary and structured responses. Unknown values stay `None`; usage is never estimated. `LlmGatewayResponse` now implements `Default`. Code that builds it with a struct literal must add `..Default::default()` (or the new field).
- `LlmResponseTracerEvent` carries the same evidence, serialized as flat `usage`, `provider_model`, `finish_reason` and `metadata` fields. The broker records it for `generate_response`, `generate` and `generate_object`. `model` stays the configured request model. New `TracerSystem::record_llm_response_with_evidence`; `record_llm_response` is unchanged and records unknown evidence.
- New provided method `LlmGateway::complete_json_response` returns the structured object with its evidence. `generate_object` uses it. The default implementation wraps `complete_json`, so existing gateway implementations keep working.
- New provided method `TracerEvent::as_any` lets event-store callbacks downcast built-in events to read typed fields such as the evidence.
- New `LlmBroker::generate_stream_events` streams one turn as `StreamEvent::Content` values and ends with exactly one terminal event: `StreamEvent::Completed(ResponseEvidence)` or `StreamEvent::Error(StreamEventError)`. Success needs OpenAI `finish_reason: "stop"` plus `data: [DONE]`, or Ollama `done: true` with `done_reason: "stop"`. Other outcomes are `IncompleteCompletion` (with evidence), `IncompleteStream` (with partial evidence), `ProviderError` (error frames and non-success HTTP status), `UnexpectedToolCalls`, `InvalidStreamEvent`, `StreamEventsUnsupported` and `RequestFailed`. One request, no tools, no retry; dropping the stream cancels the request. The tracer records the call when the request starts and the response with its evidence at the terminal event. OpenAI and Ollama support it; other gateways yield `StreamEventsUnsupported` through the new provided method `LlmGateway::complete_stream_events`. OpenAI stream-events requests set `stream_options.include_usage`. Existing `generate_stream` and `StreamChunk` behaviour is unchanged.
- New `OmlxGateway` and `OmlxConfig` (`mojentic::llm::gateways`) for [oMLX](https://github.com/jundot/omlx), an LLM server for Apple Silicon. Configuration comes from explicit fields, then `OMLX_HOST` (default `http://localhost:8000`; the gateway adds `/v1`), `OMLX_API_KEY` (bearer header only when set) and `OMLX_TIMEOUT` (milliseconds, default ten minutes, for non-streaming requests including model load). Chat requests send the configuration unadapted: `max_tokens` always, `top_p`/`top_k` when set, `reasoning_effort` low/medium/high as strings (`ReasoningEffort::Disabled` sends `enable_thinking: false` instead), never `max_completion_tokens`, `num_ctx` or `num_predict`, and no OpenAI model registry. `reasoning_content` maps to `thinking` (and to `StreamChunk::Thinking` in `complete_stream`). Structured output sends a `json_schema` response format; when oMLX reports it could not enforce a JSON format in a `Warning` header, the value goes into response metadata under `response_format_warning` (`omlx::RESPONSE_FORMAT_WARNING`) with a logged warning. Both streaming APIs drop oMLX keep-alive frames (`model: "keepalive"`) before parsing; `generate_stream_events` follows the OpenAI completion rules. New `OmlxGateway::load_model` and `unload_model`. `calculate_embeddings` needs a model (a missing or blank model is `InvalidArgument` before any request) and sends the text in one request. Provider errors carry the HTTP status and body. See the "oMLX Gateway" book page and `cargo run --example omlx`.

### Fixed

- Stream events reject malformed OpenAI choice, finish-reason and tool-call fields, and malformed Ollama message, done and tool-call fields. Ollama retains evidence from earlier frames when later frames omit it.
- oMLX omits authorization for explicit blank API keys and rejects blank load/unload model ids before sending a request. Both streaming APIs omit the whole-response timeout so long replies can finish; dropping either stream cancels its request. Non-streaming requests retain the configured timeout.
- OpenAI user images preserve HTTP(S) URLs and data URIs instead of treating them as local filenames and dropping them. Local files keep their existing Base64 encoding.
- OpenAI embeddings split long text into at most 8191 cl100k_base tokens per part and weight each vector by its part token count before unit normalization. Previously character estimates determined the parts, and vector dimensions produced an equal-weight mean. Single-part vectors remain unchanged.
- Legacy streaming (`complete_stream`) in the OpenAI gateway, and in the oMLX gateway that shares its transport, lost a whole network chunk when the chunk ended inside a multi-byte UTF-8 character, dropping content, reasoning or tool-call data. The transport now buffers bytes and decodes each line once it is complete. A line that is not valid UTF-8 is logged and skipped.

## [1.5.0] - 2026-05-21

### Added

- `RealtimeVoiceBroker` and OpenAI Realtime gateway over WebSocket for low-latency voice interactions
- `LlmTool` trait and `ToolRunner` abstraction with serial execution default, enabling async tool calling patterns decoupled from the broker
- `LlmBroker` is now wired through `ToolRunner` for unified tool dispatch
- OpenAI model registry now recognizes the GPT-5.4 and GPT-5.5 reasoning families (`gpt-5.4`, `gpt-5.4-mini`, `gpt-5.4-nano`, `gpt-5.5`, `gpt-5.5-pro`, plus dated snapshots). Each is registered explicitly with its true 1.05M-token context window (400K for `mini`/`nano`) and 128K output cap, rather than falling through to substring pattern matching, which inferred wrong token limits. Pattern mappings for `gpt-5.3`/`gpt-5.4`/`gpt-5.5` were also added so future unlisted variants resolve to the reasoning type.
- mdBook documentation chapters for Realtime voice and Parallel Tools patterns

### Fixed

- Race condition in `AsyncDispatcher::dispatch()` / `wait_for_empty_queue()` — concurrent dispatches could cause spurious "queue empty" signals before all in-flight handlers completed

### Changed

- Upgraded `tokio-tungstenite` from 0.24 to 0.29

## [1.4.0] - 2026-05-11

### Added

- `max_tool_iterations` field on `CompletionConfig` (default `10`) — bounds recursive tool-calling in both `generate` and `generate_stream`; returns `MojenticError::MaxToolIterationsExceeded` when the limit is reached. Previously both paths could recurse without bound on repeated tool-call responses.
- Integration test exercising the OpenAI tool-calling HTTP round-trip (assistant tool-call → tool result → final response), backed by a shared fixture set used across all four mojentic ports.

### Changed

- `SimpleRecursiveAgent` completion detection requires a strict whole-string match for `DONE`/`FAIL` (case-insensitive, trimmed); responses that merely contain those words as substrings no longer trigger completion.

### Fixed

- `AsyncDispatcher::wait_for_empty_queue` could return while agent handlers were still in flight; it now waits for both the queue to drain and all in-flight handlers to complete.
- Asynchronous event-handler errors and panics in `SimpleRecursiveAgent` were swallowed, leaving `solve` to hang until its timeout; handler errors now surface via `MojenticError::HandlerError`.

## [1.3.0] - 2026-04-11

### Changed

- `TokenizerGateway::encode()` now returns `Vec<u32>` instead of `Vec<usize>` (aligns with tiktoken-rs 0.11 API)
- `TokenizerGateway::decode()` now accepts `&[u32]` instead of `&[usize]`
- `WebSearchTool` uses regex-based HTML parsing instead of `scraper` crate for lighter dependencies
- Updated `reqwest` from 0.12 to 0.13
- Updated `schemars` from 0.8 to 1.0
- Updated `thiserror` from 1.0 to 2.0
- Updated `tiktoken-rs` from 0.5 to 0.11

### Removed

- `dotenv` dependency (was unused)
- `scraper` dependency (replaced with regex-based parsing in `WebSearchTool`)
- `urlencoding` dependency (replaced with `percent-encoding`)

## [1.2.0] - 2026-02-05

### Added

- `ReasoningEffort` enum (`Low`, `Medium`, `High`) for controlling model thinking depth
- `reasoning_effort` field on `CompletionConfig` for reasoning effort control
  - Ollama gateway: maps to `think: true` parameter for extended thinking
  - OpenAI gateway: maps to `reasoning_effort` API parameter for reasoning models
- `thinking` field on `LlmGatewayResponse` for model reasoning traces (populated by Ollama)

## [1.1.0] - 2026-02-05

### Added

- API endpoint support flags on `ModelCapabilities`: `supports_chat_api`, `supports_completions_api`, `supports_responses_api`
  - Indicates which OpenAI API endpoints each model supports (Chat, Completions, Responses)
  - Populated for all registered models based on endpoint audit data
- New models: `babbage-002`, `davinci-002`, `gpt-5.1-codex-mini`, `codex-mini-latest`

### Fixed

- `gpt-3.5-turbo-instruct` models now correctly flagged as completions-only (not chat-capable)

## [1.0.1] - 2026-02-01

### Added

- `ChatSession::send_stream()` method for streaming responses with automatic conversation history management
  - Returns `Pin<Box<dyn Stream<Item = Result<String>>>>` yielding chunks in real-time
  - Automatically records user message and assembled assistant response in conversation history
  - Supports tool calling through broker's recursive streaming
  - Respects context window limits

## [1.0.0] - 2025-11-27

### 🎉 First Stable Release

This release marks the first stable version of Mojentic for Rust, released simultaneously across all four language implementations (Python, Elixir, Rust, and TypeScript) with full feature parity.

### Highlights

- **Complete LLM Integration Layer**: Broker, OpenAI + Ollama gateways, structured output, tool calling, streaming with recursive tool execution, image analysis, tokenizer, embeddings
- **Full Tracer System**: Event recording, correlation tracking, event filtering, broker/tool integration
- **Complete Agent System**: Base agents, async agents, event system, dispatcher, router, aggregators, iterative solver, recursive agent, ReAct pattern, shared working memory
- **Comprehensive Tool Suite**: DateResolver, File tools (8 tools), Task manager, Tell user, Ask user, Web search, Current datetime, Tool wrapper (broker as tool)
- **24 Examples**: Full example suite demonstrating all major features
- **Type Safety**: Compile-time guarantees with Rust's type system

### Added

#### Layer 1: LLM Integration

- `LlmBroker` - Main interface for LLM interactions with recursive tool calling
- `LlmGateway` trait - Abstract interface for LLM providers
- `OllamaGateway` - Full Ollama implementation with async streaming
- `OpenAiGateway` - OpenAI gateway implementation
- `ChatSession` - Conversational session management
- `TokenizerGateway` - Token counting with tiktoken-rs
- `EmbeddingsGateway` - Vector embeddings support
- Async streaming with `Pin<Box<dyn Stream>>`

#### Layer 2: Tracer System

- `TracerSystem` - Event recording with thread-safe access
- `EventStore` - Event persistence and querying
- `TracerEvent` variants for LLM calls, responses, and tools
- Correlation ID tracking with `Arc` sharing

#### Layer 3: Agent System

- `BaseLlmAgent` - LLM-enabled agent foundation
- `AsyncLlmAgent` - Async agent with tokio runtime
- `AsyncAggregatorAgent` - Result aggregation
- `IterativeProblemSolver` - Multi-step reasoning
- `SimpleRecursiveAgent` - Self-recursive processing
- `AsyncDispatcher` - Event routing
- `Router` - Event-to-agent routing
- `SharedWorkingMemory` - Agent context sharing with `RwLock`
- ReAct pattern implementation

#### Tools

- `DateResolverTool` - Natural language date parsing
- `CurrentDatetimeTool` - Current time access
- `ToolWrapper` - Agent as tool delegation
- File tools: Read, Write, List, Exists, Delete, Move, Copy, Append
- `TaskManagerTool` - Ephemeral task management
- `TellUserTool` / `AskUserTool` - User interaction
- `WebSearchTool` - Organic web search

#### Infrastructure

- 365 tests passing
- Zero clippy warnings
- rustdoc + mdBook documentation
- cargo-deny security auditing
- Comprehensive error types with `thiserror`
