# Rust 3.0.0 scoped conformance

This is the start of the 3.0.0 conformance map, not a release certification.
This correction verifies October 12 recovery amendment item 2, **Admission
without a decision**, only. The remaining initiative decisions require their own
verification. The package remains 2.1.0; no tag or publication is authorized here.

## Inputs and implementation identity

- Tested source commit: `94627c66ce622fd69ab4a463900d2950c3a79709`
  (the preserved admission-unresolved implementation and tests), plus the
  documentation reconciliation in this worktree. Production and test sources
  remain byte-identical to that commit. Foundry owns final Git finalization.
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

The tested Git trees are `src` = `cef0f94d872586287234508147eb956d494b2b39`
and `tests` = `719c68bd30730b8d7e5499a4d949669da3b4cf60`. The ignored,
locally resolved Cargo.lock SHA-256 is
`34b4edeec0b17bc03bc1075e581c6268e9bc1e8c07a5a431c67b4769556af6f4`;
Cargo.toml SHA-256 is
`febc01c4edffd21752b9d3778158466f00caa19e3c8da8474cb9aea8a0bb7d21`.
There is no workspace/resolver override or toolchain pin; CI selects stable.
Verification uses Rust/Cargo 1.99.0 and cargo-tarpaulin 0.37.2.

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
on the preserved source commit above, with the executable set/hash above. The full
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


## Coverage failure reconciliation (2026-10-16)

The prior quality table described a passing capture on an earlier worktree and
an initial rustup-proxy setup failure. Neither explains the subsequently reported
`Test failed during run`. Those results are historical, not renewed evidence for
this correction. The preserved scratch logs available at entry contain no such
failure diagnostic. A failing executable, test name and diagnostic for that run
have not yet been identified. Do not treat a later passing run as its diagnosis
or claim that the required coverage correction is complete.

Before documentation changes, the exact required coverage command was exercised
on preserved commit `94627c66ce622fd69ab4a463900d2950c3a79709`. Fresh results and
post-documentation checks are recorded below. No timeout, assertion, test,
coverage engine, coverage floor or gate configuration was relaxed. No speculative
runtime or harness change was made without a demonstrated failure.

A separate sensitivity probe temporarily changed only the engine's unresolved
branch to emit `AdmissionAllowed` and continue. Running
`cargo test --all-features --test recovery admission_unresolved_http_matrix -- --exact`
exited **101** at `tests/recovery.rs:3711`: `unresolved admission must stop`,
with a successful `RecoveryReport` on wire attempt **2**. Restoring the preserved
branch byte-for-byte made the same command exit **0** (one matrix test, all nine
adapter/operation cells). This establishes that the real HTTP test rejects an
unsafe resend; it does **not** reproduce the reported coverage failure. Existing
assertions retain stable `admission_unresolved`, original failure identity/history,
one wire request and zero broker/session tool execution.

## Renewed quality verification

On the unchanged production/test source identity above:

| Exact command | Exit | Result |
| --- | --- | --- |
| `cargo fmt --check` | 0 | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Passed |
| `cargo test --verbose --all-features` | 0 | 678 passed; 24 existing ignored doctests |
| `cargo build --verbose --all-features` | 0 | Passed |
| `cargo tarpaulin --all-features --fail-under 56` | 0 | Final serialized run: 69.71%, 3282/4708 lines; floor remains 56% |
| `cargo test --all-features --test recovery` | 0 | All 60 recovery tests passed |
| `cargo test` | 0 | Passed, including doctests |
| `cargo doc --no-deps --all-features` | 0 | Passed with existing broken `connect` link and unclosed `dyn` tag warnings |
| `mdbook build book` | 0 | HTML emitted; installed mdBook reports unsupported existing `truncate-long-entries` search setting |
| `cargo deny check` | 0 | Advisories, bans, licenses and sources passed; existing duplicate-version warnings |
| `cargo audit` | 0 | No vulnerabilities reported |

Security commands were run with `CARGO_HOME=/tmp/mojentic-c2-cargo` and
`/home/svetzal/.cargo/bin` prepended to PATH. This provides a writable advisory
cache and selects the installed rustup proxy. The initial deny attempt exited 1
because the default advisory lock path was read-only; the first audit fetch
exited 1 on a network timeout, and the unchanged command passed on retry.
No advisory suppression or dependency change was made. mdBook's generated
placeholder source pages were removed after checking the build.

Two additional initial coverage captures reached completed coverage summaries
(69.71%, 3282/4708 lines) without a failing test diagnostic. They overlapped, so
they are not independent serialized reproductions. The initial captured exit-0
run measured 69.69% (3281/4708 lines). After the documentation correction, the
entire seven-gate set was rerun serially: all exited 0; the final coverage result
is in the table above. Both exit-0 runs used the exact required command. The
one-line variation does not affect the unchanged 56% floor. All recovery matrices passed;
none establishes the cause of the earlier reported failed run.

**Unfinished:** obtain the failing coverage capture or reproduce its failing
executable/test and diagnostic, then correct only its demonstrated cause and
rerun the gates. Passing captures and the admission sensitivity probe do not
close that requirement. Consumer-side verification also remains missing; no
paired compatibility or release readiness is claimed.
