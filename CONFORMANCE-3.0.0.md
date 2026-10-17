# Rust 3.0.0 scoped conformance

## Current correction: typed errors (amendment item 8)

This correction starts at `3bf99ad21759c87ae27d8130b2d7de6c5557a7d5` and remains
uncommitted for Foundry finalization. The capability correction below is a
historical record, not the current source identity or current gate results.
Only item 8 is advanced here. Publication holds and all other unverified
initiative decisions remain in force. Stacey's renewed-passing-evidence coverage
reconciliation remains binding: the historical unexplained failure is low
severity and no further reproduction is requested.

Executable contract: **typed-errors-v1**, Rust artifact
`tests/contracts/typed_errors_v1.json`, SHA-256
`48d957ccdabc9fa606d9e1fa32f7969c7f0c61259b6072c832eb0778e3450aee`.
The verifier is `tests/recovery.rs`, reusing the existing loopback HTTP fixture,
public gateway wrappers, broker and session. It runs in both the required test
and unchanged tarpaulin gates. The artifact supplies provider response bodies,
invalid policy selections and capture stages. This Rust-specific executable set
is distinct from the Elixir set of the same logical name; matching names do not
establish shared executable verification.

Verification command: `cargo test --all-features --test recovery typed_errors_v1`.
The matrix covers all three maintained adapters, three operations through gateway
and broker, and the session's two exposed operations: length, capture at three
stages, original JSON errors and typed invalid frames. Additional cases reject
invalid construction before HTTP; exercise optional fields; and preserve legacy
non-recovery length success. This Rust correction introduces v1;
no wire protocol version is changed. The detailed mapping is in RECOVERY-CONFORMANCE.md.

Rust provider/consumer boundary wiring is exercised locally against these exact
scripted cases. These fixtures have **not** been verified against external provider
implementations and must not be treated as doubles derived from jointly verified
contracts. OpenAI/Ollama/oMLX provider commits, verification commands and results:
**unavailable / not run**. External Rust consumer commit, command and results against
this exact artifact and verifier: **unavailable / not run**. microgentic remains
initiative work; its reported `e71b387` and retained pin `4ca1ed2` were not tested
here. Elixir's typed-errors-v1 cases were inspected as reference only, not rerun.
Missing paired verification is unfinished work for initiative coordination in
separately authorized repositories. No sibling checkout was changed, and no
cross-repository compatibility, consumer migration or release readiness is claimed.

Rust 2021, implicit single-package resolver, Tokio, dependency manifest, default
features, stable CI toolchain, formatting edition, coverage engine and 56% floor
are unchanged. No declared MSRV or toolchain pin exists; declaring an MSRV remains
a separate policy recommendation. Cargo.lock was absent at entry and is generated
only as an ignored verification artifact. No version/ref/publication changes were
made. The verified initiative brief and authoritative contract hashes are recorded
in RECOVERY-CONFORMANCE.md.

The tested Rust implementation is the baseline above plus this uncommitted source
bundle, SHA-256 `cd7d3c365cac50dda6a44eecc493bd5587e91731080f4b825f467eca48cbb69c`:

```sh
cat src/error.rs src/llm/recovery/{adapter,engine,frames,policy,types}.rs tests/recovery.rs tests/contracts/typed_errors_v1.json | sha256sum
```

The complete executable set (fixture followed by verifier) has SHA-256
`8eaeff4048e2d2ecabd50354b9895abe966ba4f4499e74abc61b7c873578c57d`:

```sh
cat tests/contracts/typed_errors_v1.json tests/recovery.rs | sha256sum
```

Verification uses Rust/Cargo 1.99.0, cargo-tarpaulin 0.37.2, the existing writable
Cargo cache, and the generated ignored Cargo.lock SHA-256
`390c599335be1aed77a5ff897c5f053631e31611d89ac66e2b06cd056e29034d`.
The unchanged capability artifact retains SHA-256
`40e8da683ce4bfcfebf9892ca51a53a701488d463977f021922cd6317203aa76`.

### Final verification for item 8

All seven discovered `.hone-gates.json` gates and the additional default-feature
and applicable CI documentation checks exited 0 on the source bundle above.
Verbose builds/tests and full diagnostics were captured through `foundry capture`.
The standalone versioned verifier also exited 0 on that same final source.

| Exact command | Exit | Result |
| --- | --- | --- |
| `cargo fmt --check` | 0 | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Passed; zero Clippy warnings |
| `cargo test --verbose --all-features` | 0 | 683 passed, including 19 doctests; 24 pre-existing ignored doctests |
| `cargo build --verbose --all-features` | 0 | Passed |
| `cargo tarpaulin --all-features --fail-under 56` | 0 | 69.96%, 3331/4761 lines; unchanged 56% floor |
| `cargo deny check` | 0 | Advisories, bans, licenses and sources passed; existing duplicate-version warnings |
| `cargo audit` | 0 | Passed; no vulnerabilities reported |
| `cargo test` | 0 | 683 passed; 24 pre-existing ignored doctests |
| `cargo doc --no-deps --all-features` | 0 | Passed with two existing warnings (connect link and dyn HTML tag) |
| `mdbook build book` | 0 | Exit 0 and HTML emitted; existing truncate-long-entries configuration diagnostic |
| `cargo test --all-features --test recovery typed_errors_v1` | 0 | 4 tests passed; includes the 144-case boundary matrix, 24 invalid policies, optional fields, legacy length and session proof |
| `cargo test --all-features --test recovery` | 0 | All 65 recovery tests passed |

The proof-first session probe exited 101 on the original implementation because
it accepted a length-terminated reply as success. The final corrected probe exits
0 and asserts the concrete length cause, exact received body, retained history,
observed but undelivered content, user-turn retention and one HTTP request.

The initial full test run exposed three assertions for the former private wrapper
and string length cause. They were migrated to direct serde downcasts and the
concrete length variant, retaining their decoder details, telemetry, history and
follow-up tool assertions. The subsequent complete gate sweep passed. No gate,
assertion scope, timeout, dependency, advisory policy or coverage floor was weakened.
This does not diagnose or reproduce the unrelated historical coverage failure.

Documentation checks are not warning-free: rustdoc retains its existing unresolved
`connect` link and unclosed `dyn` tag, and mdBook reports its existing unsupported
`truncate-long-entries` search configuration before writing HTML and exiting 0.
Generated mdBook placeholder source pages were removed after checking. These
existing diagnostics remain outside this correction.

Paired external verification remains unavailable as recorded above. The local
passes establish Rust regression evidence only; all publication and consumer
migration holds remain in force.


## Historical item 3 correction record


This increment implements October 12 recovery amendment item 3, **Capabilities**,
for existing Ollama, oMLX and OpenAI recovery adapters, preserving completed item 2.
It is not whole-initiative certification. Package version remains 2.1.0. Publication
holds remain until all six ports are ready; Foundry owns Git finalization.

## Inputs and tested source identity

- Starting/tested commit: `6b5895be44fd9f81196d2004900f2059bc71c3ce`, plus
  the uncommitted capability implementation and tests identified below.
- Initiative brief SHA-256, verified before work:
  `ea6a0efa56bfb3cd74b5ce728f80cc33c6fbd853114bec94b8aabe79f5d28e6e`.
- Binding root `TRANSIENT-RECOVERY-2026-10.md` SHA-256:
  `a53a3a4eb118fa4687dd703d6ce918dd40dd6cb4eda796abc6c429a6e932e7a3`.
  Read with root `RELEASE-3.0.0.md` from the coordinator checkout at
  `/home/svetzal/Work/Projects/Personal/mojentic-unify`; the isolated worktree's
  `../RELEASE-3.0.0.md` is absent. No root contract was edited.
- Reference Elixir source inspected at `ddb2217c8245b041a20fd25f34f5aa6fbfaee2ce`:
  `lib/mojentic/llm/recovery/capabilities.ex`,
  `test/contracts/recovery_capabilities_v1.json` and its executable verification
  `test/mojentic/llm/recovery_capabilities_test.exs`. This is inspection, not a
  new Elixir test result or a consumer verification.
- Reused executable declaration contract **recovery-capabilities-v1**:
  [tests/contracts/recovery_capabilities_v1.json](tests/contracts/recovery_capabilities_v1.json),
  SHA-256 `40e8da683ce4bfcfebf9892ca51a53a701488d463977f021922cd6317203aa76`.
  The bytes match the reference exactly. Original inspection dates stay
  2026-10-16; endpoint documentation was independently rechecked 2026-10-17.
- Changed production/test source aggregate SHA-256:
  `8193c5cfaa3217a66ba4614e4a02f852283f3fcea3f8c9935dddd1181ab1067e`.
  Reproduce from the repository root with:

  ```sh
  sha256sum src/llm/recovery/adapter.rs src/llm/recovery/capabilities.rs src/llm/recovery/mod.rs src/llm/recovery/types.rs tests/recovery.rs tests/contracts/recovery_capabilities_v1.json | sha256sum
  ```

Rust 2021, implicit single-package resolver, Tokio, dependencies and feature
semantics/defaults are unchanged. No workspace, toolchain pin or declared MSRV
exists; CI selects stable. A declared library MSRV remains a separate policy
recommendation. Cargo.lock was absent at entry and remains ignored by the library
policy; the generated verification lock SHA-256 is
`390c599335be1aed77a5ff897c5f053631e31611d89ac66e2b06cd056e29034d`.
Cargo.toml SHA-256 remains
`febc01c4edffd21752b9d3778158466f00caa19e3c8da8474cb9aea8a0bb7d21`.
Verification uses Rust/Cargo 1.99.0 and cargo-tarpaulin 0.37.2 with the existing engine and 56% floor.

## Item 3 executable mapping

| Decision | Executable evidence |
| --- | --- |
| Every maintained recovery adapter exposes five canonical capability declarations | `capability_contract_public_adapters_v1` calls each actual public adapter's `with_recovery`, then `capabilities()` |
| Values are exact per endpoint, including oMLX unknown local cancellation/idempotency | Same test compares the complete values object with v1 reference cases, not a count or schema |
| Every value has exact endpoint, citation, inspection date and explanatory evidence | Same test compares the complete `capability_evidence()` object for all fifteen cases; validates required HTTPS citations, dates and nonempty bases |
| Correct provider selected even when values match | Same test starts with OpenAI: hardcoding Ollama evidence fails on endpoint/citation/basis although all five values match |
| Exact-attempt termination stays unknown | Exact v1 equality plus typed `Support::Unknown` assertion through each adapter |
| Request fidelity survives without shared-default assumptions | `immutable_reasoning_tools_schema_and_sampling_all_adapters` keeps payload equality, schema, sampling, native reasoning and history assertions, selecting `thinking`/`reasoning_content` explicitly |
| Request/retry/cancellation/admission behavior remains unchanged | Existing full `tests/recovery.rs` suite, including item 2 matrices below |

Proof-first acceptance:
`cargo test --all-features --test recovery capability_contract_public_adapters_v1 -- --exact`.
The original three-field default failed with exit **101**. After implementing five
provider-specific declarations it passed with exit **0**. The strengthened exact
evidence test then rejected deliberate hardcoded Ollama provider selection with
exit **101** on OpenAI evidence. Restoring selection from the actual inner adapter
passed the identical command with exit **0**. This proves semantic sensitivity at
the public adapter boundary; no capability mocks or marker toggle are used.

The v1 set is the supported executable declaration contract. The old unreleased three-key
Rust serialized capability object is intentionally replaced in this coordinated major
initiative; it is not a second supported contract version. CHANGELOG and the book
explain literal/key migration and the idempotency declaration change. The Rust
`request_cancel` field is retained as a remote cancellation alias, verified by
the same public-adapter test, and omitted from serialization. All five
evidence records are required; none is optional. Existing completion request,
response, error and optional-field cases remain in the recovery suite unchanged.
No new Anthropic adapter, remote cancel/status request or idempotency header is
introduced. Values describe inspected endpoint documentation, not a live provider
certification or permission to resend.

## Preserved item 2 mapping

| Decision / invariant | Named executable evidence |
| --- | --- |
| Completed absence of permission stops after one request with distinct outcome and terminal lifecycle reason | `admission_unresolved_http_matrix` |
| Failed HTTP status, body/cause, history, logical/attempt identity and wire attempt 1 survive; no admission-allowed, delay or retry event follows | `admission_unresolved_http_matrix` |
| Safe Debug/Display/serialized errors and lifecycle omit sensitive payload; new stable serialized outcome/reason | `admission_unresolved_http_matrix` |
| Broker/session preserve typed error and history, retain the user turn, deliver no semantic output and execute zero tools | `admission_unresolved_broker_session_http_matrix` |
| Explicit Allow retries immutable payload once with a new attempt ID; explicit Reject stops distinctly | `admission_explicit_decisions_http_matrix` |
| Pending is still pending until explicit Allow | `review_explicit_pending_admission_all_adapters` |
| Pending hook cancellation remains prompt; no later send | `review_pending_and_backoff_cancel_matrix` |
| Pending hook budget expiry terminates without Allow/RetryStarted or an invented attempt | `admission_pending_budget_http_matrix` |
| Absent hook preserves ambiguous-local refusal and remote retry rules | `admission_absent_local_remote_http_matrix` |


## Boundary coordination gap

Rust provider verification uses the shared v1 bytes and actual public adapters:

```sh
cargo test --all-features --test recovery capability_contract_public_adapters_v1 -- --exact
cargo test --all-features --test recovery
cargo test --verbose --all-features
```

The all-feature test gate executes the contract probe automatically; no separate
CI contract runner is necessary. The full recovery suite still tests HTTP wiring,
request fidelity, errors, optional fields and broker/session journeys. Scripted
HTTP responses are not live provider verification and do not establish a paired
consumer agreement.

**Missing paired consumer verification is unfinished initiative work.** No consumer
commit was tested, and no consumer command/result against this v1 hash is available.
The initiative records microgentic at `e71b387`, retaining Mojentic Git pin `4ca1ed2`;
neither consumer revision was verified here. Coordination must provide this same
contract/hash to the consumer, execute it in that repository's required gates on
an identified commit, and report its command/results alongside the Rust identity
above. Elixir's reference test is a port reference, not Rust's consumer. Its tests
were not rerun in this Rust-only correction. No sibling checkout was modified.
No cross-repository compatibility, package migration or release readiness is claimed.

## Coverage reconciliation: Stacey's binding decision

Stacey's reconciliation permits progress on renewed passing evidence. The earlier
unexplained `Test failed during run` remains **low severity**; **further reproduction
is not requested**. Later passing runs do not diagnose that historical failure.
The prior request to obtain/reproduce it is superseded by this binding decision.
The coverage engine, assertions, timeouts, test scope and 56% floor remain intact.
No speculative runtime/harness correction, suppression or weakened check was made.

## Remaining initiative decisions

| Scope | Status |
| --- | --- |
| Recovery amendment item 2 | Preserved; renewed regression suite |
| Recovery amendment item 3 | Exact values/evidence verified through all three maintained public recovery adapters |
| Recovery items 1 and 4–8, 500/502 cases, and other recovery decisions | No renewed whole-contract certification; historical map remains in RECOVERY-CONFORMANCE.md |
| Native reasoning, context-window/first-token support, other audit fixes and packaging | Unverified by this increment |
| Paired consumers and coordinated publication | Missing; publication holds remain |

## Renewed verification

Commands run using `/home/svetzal/.cargo/bin` on PATH and writable
`CARGO_HOME=/tmp/mojentic-c2-cargo` for dependency/advisory caches. Verbose
builds/tests are captured by `foundry capture`. No dependency or advisory policy
was changed. Results below refer to the source identity above; prior conformance
results are not substituted for current execution.

| Exact command | Exit | Result |
| --- | --- | --- |
| `cargo fmt --check` | 0 | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Passed |
| `cargo test --verbose --all-features` | 0 | 679 passed; 24 existing ignored doctests |
| `cargo build --verbose --all-features` | 0 | Passed |
| `cargo tarpaulin --all-features --fail-under 56` | 0 | 69.86% coverage, 3306/4732 lines covered; unchanged 56% floor |
| `cargo deny check` | 0 | Advisories, bans, licenses and sources passed; existing duplicate-version warnings |
| `cargo audit` | 0 | Passed; no vulnerabilities reported |
| `cargo test` | 0 | 679 passed; 24 existing ignored doctests |
| `cargo test --all-features --test recovery` | 0 | 61 recovery tests passed |
| `cargo doc --no-deps --all-features` | 0 | Passed; existing unresolved `connect` link and unclosed `dyn` tag warnings |
| `mdbook build book` | 0 | HTML emitted (exit 0); existing `truncate-long-entries` setting emits a configuration error |

The targeted public-adapter v1 contract probe also passed with exit **0** on
the final source. The final all-feature test gate executed **679** passing tests
(including 19 doctests), with 24 pre-existing ignored doctests. All **61** recovery
tests passed. The first sweep measured 69.84% (3305/4732); the final serialized
sweep measured 69.86% (3306/4732). Both used the exact required tarpaulin command
and unchanged 56% floor; neither diagnoses the historical unexplained failure.

All eleven discovered gates/applicable CI checks above exited 0. The final
source was frozen throughout that sweep. No advisory suppression, dependency,
coverage-engine, assertion, timeout, gate or CI change was made. mdBook-generated
placeholder pages were removed after verification. Consumer verification remains
missing as described above.

The installed mdBook logs `Failed to deserialize output.html` for the existing
`truncate-long-entries` search setting, then writes HTML and exits 0. This
configuration diagnostic and the two existing rustdoc warnings remain outside
the capability correction; documentation checks are not claimed warning-free.
