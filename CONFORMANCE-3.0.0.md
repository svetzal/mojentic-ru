# Rust 3.0.0 scoped conformance

This is the start of the 3.0.0 conformance map, not a release certification.
This correction verifies October 12 recovery amendment item 2, **Admission
without a decision**, only. The remaining initiative decisions require their own
verification. The package remains 2.1.0; no tag or publication is authorized here.

## Inputs and implementation identity

- Tested base commit: `009012452e9cb55691c475f831b9fb3fa748da3a`, plus the
  uncommitted correction in this worktree. Foundry will assign the final commit;
  the base alone does not contain this correction.
- Initiative brief SHA-256 (verified before work):
  `ea6a0efa56bfb3cd74b5ce728f80cc33c6fbd853114bec94b8aabe79f5d28e6e`.
- Local recovery contract: `TRANSIENT-RECOVERY-2026-10.md`, SHA-256
  `be317890d22c02f69cda2272aae3c59dc5531c35866ff120b780f852141d9cd0`.
  This snapshot describes the earlier recovery contract; amendment item 2 is
  supplied by the authorized correction plan and initiative brief. The coordinator
  reference is `../RELEASE-3.0.0.md`; that sibling path is absent in this isolated
  worktree. No binding contract text was rewritten here.
- Executable Rust recovery set: existing `tests/recovery.rs` HTTP harness,
  scoped addition **admission-unresolved-v1**, file SHA-256
  `9fda675e67f7ccadb2ea335505d7cf74725c30b220c002dbf59b9519ae055c37`.
  This identifies executable cases, not a shared producer/consumer certification.
  Existing Allow/Reject hooks keep their signatures. Completed absence is the
  new `AdmissionDecision::Unresolved`; serialized outcome and transition are
  `admission_unresolved`. Existing absent optional metadata and private evidence
  remain governed by the unchanged recovery suite.

## Item 2 executable mapping

All matrix tests below dispatch through public adapters over actual loopback
TCP/HTTP for Ollama, oMLX, and OpenAI. Ordinary, structured and recovery streaming
are covered; session propagation covers its ordinary/streaming APIs, and
structured propagation uses the broker. These are adapter implementation tests
against scripted HTTP responses, not live upstream provider verification.

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

Run the executable set with:

```sh
cargo test --all-features --test recovery
```

The proof-first probe `cargo test --all-features --test recovery
admission_unresolved_http_matrix -- --exact` failed with exit **101** before the
engine correction: the HTTP adapter returned success on wire attempt **2** after
an unresolved hook. The same probe passed with exit **0** after the correction,
covering all nine provider/operation cells and retained failure semantics.

## Remaining initiative decisions

| Scope | Status in this correction |
| --- | --- |
| Recovery amendment item 2 | Verified by the named tests above |
| Recovery amendment items 1 and 3–8, including capability evidence and all 500/502 cases | Unverified here; existing tests are not renewed whole-contract certification |
| Other recovery decisions: eligibility, transport phases, partial output, termination evidence, retry floors, observers, immutable payloads and active-generation limits | Unverified here beyond item 2 regression assertions; historical map remains in RECOVERY-CONFORMANCE.md |
| Native reasoning parity and migration completeness | Unverified here; existing implementation documented in Unreleased |
| Context-window and first-token initiative decisions | Unverified here |
| Other audit fixes, packaging, consumers, coordinated release | Unverified here; outside this correction |

## Boundary coordination gap

No shared, versioned executable cross-repository contract mechanism was found.
This change reuses the Rust HTTP conformance harness rather than introducing
independent gateway mocks. The scripted HTTP peer does **not** establish agreement
with another repository or with live upstream servers.

Rust provider-side verification is `cargo test --all-features --test recovery`
on the base plus this correction, with the executable set/hash above. The full
all-feature test gate also executes it. Consumer-side verification is **missing**:
no consumer commit was tested and no consumer verification command/result is
available. In particular, the initiative records microgentic at `e71b387`, still
pinned to Mojentic `4ca1ed2`; neither revision was tested in this correction.
Coordinate a shared executable set and its consumption/verification in the
consumer's required gates as separate initiative work. Publication hold remains;
this document makes no cross-repository compatibility claim. No sibling
repository was edited.

## Quality verification

Results are recorded below after running the discovered gates. Cargo.lock was
absent and ignored at entry; verification resolves the unchanged manifest into
an ignored local lockfile. Rust 2021, Tokio, feature defaults and dependencies
are unchanged. CI selects stable and declares no MSRV; declaring an MSRV remains
a separate library policy recommendation.


All discovered `.hone-gates.json` gates and additional repository checks passed
on the source snapshot identified above using Rust/Cargo 1.99.0 (stable):

| Command | Exit | Result |
| --- | --- | --- |
| `cargo fmt --check` | 0 | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Passed |
| `cargo test --verbose --all-features` | 0 | 678 passed; 24 existing ignored doctests |
| `cargo build --verbose --all-features` | 0 | Passed |
| `cargo tarpaulin --all-features --fail-under 56` | 0 | 69.69%, 3281/4708 lines; threshold 56% |
| `cargo deny check` | 0 | Passed; duplicate base64/syn version warnings, no advisory suppression |
| `cargo audit` | 0 | 1329 advisories loaded, 267 locked packages scanned; no vulnerabilities reported |
| `cargo test` | 0 | Passed, including doctests |
| `cargo doc --no-deps --all-features` | 0 | Passed; existing broken `connect` link in realtime/broker.rs and unclosed `dyn` HTML tag in examples/react/events.rs |

The first coverage invocation exited 1 before testing because a direct Cargo
binary could not process Tarpaulin's `+stable` invocation. Selecting the installed
rustup proxy fixed the environment; the entire nine-check set was rerun and
passed without changing gate configuration. Initial probe setup also required
a writable scratch Cargo cache instead of the read-only home cache. No coverage
threshold, advisory policy, dependency declaration, or source invariant changed.


Dedicated Rust-side executable-set verification:
`cargo test --all-features --test recovery` exited **0**, with **60 passed**,
0 failed, on `009012452e9cb55691c475f831b9fb3fa748da3a` plus this uncommitted
correction and the `admission-unresolved-v1` file hash recorded above.
Consumer-side verification remains missing, as described in the coordination gap.
