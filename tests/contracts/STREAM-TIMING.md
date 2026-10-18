# rust-stream-timing-v1

Normative source: MODEL-METADATA-2026-10.md section 2 at coordinating
`mojentic-unify` revision `ef9339c6a9682ed5f060bd690b738f6b8eea1d2d`, including
Stacey's October 17 precedence/retry/continuation decisions. Specification SHA-256:
`d9945dca45ce09e2163843263a75b5e93028b00ee1baf191fc3ff13396830e9c`.
Artifact: `stream_timing_v1.json`, SHA-256
`c794e476216781941bf0cf52d2904a1e974ed72cc495d937d29944c51a77462b`.

The existing JSON contract mechanism is reused. Cases are explicit synthetic
section-2 specifications, using the wire forms already verified by the captured
Ollama and oMLX fixtures in tests/fixtures. They are not new live-provider captures.
The named TypeScript reference `ts-stream-first-token-v1` was inspected for its
clock and HTTP semantics; library readiness does not require executing it.

Run `cargo test --all-features --test stream_timing`. The ordinary test gate,
all-feature CI test gate, tarpaulin and `python3 scripts/verify_package.py` execute
these public consumer assertions against the actual Rust provider implementations.
The fresh consumer copies this identical set out of the crate archive; its
artifact/source hashes and results appear in audit/package-readiness-3.0.0.json.
No external provider/dependent commit was tested or is claimed as a prerequisite.

`section2_exact_clocks_all_public_http_streams` derives HTTP doubles from this
artifact for Ollama, oMLX and OpenAI, using direct legacy/event APIs and recovery
compatibility streams. It asserts exact first-token kind, 25 ms first-token time,
60 ms terminal duration, absent first values for empty streams, thinking/content
precedence within a frame, first-only semantics, argument-fragment detection (tool headers alone are excluded),
keepalive exclusion, HTTP 401 and interrupted failures, and oMLX usage preservation.
`section2_native_recovery_terminal_success_and_failure_retain_timing` verifies the
same cases at native recovery terminal reports/errors, including explicit raw
usage inspection and omission of private sentinels from safe summaries.

`section2_retry_keeps_first_dispatch_clock` uses in-process 503 then success responses, with
explicit admission for local providers. An intentionally reset origin produces
35/75 ms and fails the expected 25/60 ms. The corrected clock spans wire attempts.
`section2_broker_session_tool_continuations_start_fresh_clocks` verifies exact
25/60 ms tool-call then 7/30 ms content measurements in public trace evidence,
including HTTP tool-result wiring, for direct and recovery adapters. It exercises
broker and session text journeys rather than counting private events.

`section2_old_metric_documents_remain_readable` covers the old serialized shape
and preserves rejection of malformed records missing the required provider field.
`first_frame_precedence_crosses_public_http_boundary` also runs with the default
monotonic clock and preserves exact provider nanosecond metrics. A clock is
sampled at dispatch, the first semantic frame, and terminal outcome; constructing
an unpolled stream never samples it. Provider metrics remain independent of client
measurements. Nonstream duration is optional and outside this executable set.

`section2_retry_does_not_relabel_prior_attempt_provider_usage` proves that
usage-only interrupted attempts can recover without carrying their provider usage
into a successful later attempt. Client timing still spans both dispatches.

Current verification uses injected `HttpTransport` responses. Interrupted cases
yield their contract frame, wait for semantic observation, then yield an I/O
`UnexpectedEof`. The public API must retain an HTTP body error and exact timing,
including trace evidence before broker/session error delivery. No socket or
HTTP server is involved. The v1 decision set and frames are unchanged; only the
fixture-delivery description changed, so the artifact hash above is renewed.
