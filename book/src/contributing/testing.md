# Testing

Run the repository gates from `.hone-gates.json`: formatting, strict all-target
Clippy, all-feature tests/build and tarpaulin with the existing 56% floor. Also
run `cargo deny check` and `cargo audit`; report their actual results.

Tests use the public API with in-process transports and gateway fakes. They do
not open sockets or start servers. `tests/support/mod.rs` matches encoded HTTP
requests against each suite's contracts and supplies captured or contract-derived
responses. Streaming fixtures can split UTF-8 bytes, remain pending until dropped,
or fail with `UnexpectedEof`. Assertions cover requests and terminal behavior,
not fixture call counts alone.

`HttpTransport` is the execution boundary for Ollama, oMLX, OpenAI and web search.
Use `with_transport(Arc<dyn HttpTransport>)` to inject it; gateway recovery inherits
the same transport. Production construction continues to execute with reqwest.
Use `with_stream_clock` for exact semantic timing and `RecoveryPolicy::timing`
for retry time, sleeps and jitter. Tests needing Tokio timers use a paused clock;
the existing tokio-test dependency already enables test-util.

The contract/decision maps live in `CONFORMANCE-3.0.0.md` and
`RECOVERY-CONFORMANCE.md`. Preserve assertions for errors, optional fields,
legacy explicit controls, unset controls, cancellation and tool continuations.

After dependencies are cached, `python3 scripts/verify_package.py` packages
offline, extracts a fresh artifact and runs the identical public integration
tests in a fresh consumer with `CARGO_NET_OFFLINE=true`. It records contract,
archive and implementation hashes and the resolved extracted dependency in
`audit/package-readiness-3.0.0.json`. This verifies the Rust distribution boundary;
external providers and dependent repositories retain separate verification gaps.
