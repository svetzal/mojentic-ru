# Rust 3.0.0 readiness assessment

This assessment covers RELEASE-3.0.0.md parts 1–5 for the maintained Ollama,
oMLX and OpenAI adapters. It starts at `ac81c44fff024d9675417d4da6d1f24134cc4eac`
and includes delivered trunk `8ac38ea134af7816c6fdd02e07a420ed3be1a2f4`
(the current HEAD, with this uncommitted correction). The first is an ancestor
of the second. The delivered 8ac38ea delta consists solely of ten execution-log
files (2,475 added lines); no source, manifest or CI change was dropped.
Foundry owns finalization; no ref, version or dependency was changed.
[Cycles 1–8](audit/readiness-cycles-1-8.md) preserve earlier implementation and
verification records. Their external-verification blockers are superseded.

The binding coordinating checkout inspected was `mojentic-unify` at
`ef9339c6a9682ed5f060bd690b738f6b8eea1d2d`. File SHA-256 identities:

| Binding document | SHA-256 |
| --- | --- |
| RELEASE-3.0.0.md / initiative brief | `ea6a0efa56bfb3cd74b5ce728f80cc33c6fbd853114bec94b8aabe79f5d28e6e` |
| TRANSIENT-RECOVERY-2026-10.md, authoritative October 12 amendment | `a53a3a4eb118fa4687dd703d6ce918dd40dd6cb4eda796abc6c429a6e932e7a3` |
| REASONING-PROTOTYPE-2026-10.md | `0a40f3ad45c593bc779411021e45db569e73d3ecce86c99b201e7f8ebb656809` |
| MODEL-METADATA-2026-10.md, including October 17 timing decisions | `d9945dca45ce09e2163843263a75b5e93028b00ee1baf191fc3ff13396830e9c` |

The Foundry-owned initiative brief was read and its supplied hash verified.
The sibling-relative release path is absent in this isolated checkout; the
binding originals above were read from the coordinating checkout without editing
it. The older local recovery copy is historical; the root amendment governs.

## Distribution boundary and contract identities

Stacey's library self-validation rule governs this assessment. The provider is
this Rust library, verified by its production implementations; consumers are its
public-API integration suites and a fresh consumer of the extracted crate. Both
execute the same versioned fixtures and assertions. HTTP doubles derive their
responses from those artifacts and existing coordinating captures. They establish
local library conformance, without claiming live server verification or benchmark
efficacy. No running service protocol or external repository was changed.

External providers, dependents and another port's executable snapshot are not
prerequisites before publication. No external tested commit or command/result is
claimed. Dependent registry adoption and fresh post-publication installation
remain separate, owner-held initiative work. Earlier demands for microgentic or
other-port verification against unreleased Rust are superseded, including those
in historical section-1 and recovery records.

| Executable contract | Artifact / verifier | SHA-256 |
| --- | --- | --- |
| model-metadata-section1-v1 | tests/contracts/model_metadata_section1_v1.json | `617c0df913416e8cbfa5873256d427857c1f998103b9b04a4e2b3a456d07ac07` |
| request-fidelity-v1 | tests/contracts/request_fidelity_v1.json | `6f02088311f50707b9c12154d888f844928c23d359043cb2e5d5654bf7e5eaea` |
| recovery-capabilities-v1 | tests/contracts/recovery_capabilities_v1.json | `40e8da683ce4bfcfebf9892ca51a53a701488d463977f021922cd6317203aa76` |
| recovery-status-selection-v1 | tests/contracts/recovery_status_selection_v1.json | `8250dd928817d3634eb239f90dbeee908398f8fcbba8623ba2913278fba19b30` |
| typed-errors-v1 | tests/contracts/typed_errors_v1.json | `48d957ccdabc9fa606d9e1fa32f7969c7f0c61259b6072c832eb0778e3450aee` |
| rust-stream-timing-v1 | tests/contracts/stream_timing_v1.json | `83bc48949b4a81eb0730b3c0f1f5d20bf9d3360eead3bfc504b0adcc53364755` |
| Rust native-reasoning public executable set | tests/reasoning_contract.rs, versioned by tested source hash | See audit/package-readiness-3.0.0.json |

The timing set adapts the binding TypeScript `ts-stream-first-token-v1` semantics
(content/thinking/arguments, exact clock, retries and continuations) to Rust's
existing JSON contract mechanism. That reference's `src/llm/first-token-http.test.ts`
was inspected locally; no execution of another port is a prerequisite. Provider
citations and capture versions remain in tests/fixtures/*/README.md,
tests/contracts/MODEL-METADATA-SECTION1.md and recovery capability evidence.
No unsupported wire protocol version is invented. Compatibility includes old
serialized metrics, existing third-party gateways, legacy error causes, former
2.1 explicit numeric defaults, and new 3.0 unset controls.

## Part 1: transient recovery, all binding decisions

The following are named public consumer tests in `tests/recovery.rs`; each runs
through production HTTP adapters, including wrapper/broker/session paths where
applicable. Existing deterministic timing, admission and wire-observation fakes
remain unchanged. See RECOVERY-CONFORMANCE.md for the detailed historical map.

| Decision | Public executable evidence |
| --- | --- |
| Recover one completion, default one attempt, bounded history and typed failures | `public_503_success_all_adapters`, `bounded_errors_privacy_and_disabled`, `permanent_and_bounded_errors_all_adapters` |
| Exact provider/operation/status/phase/acceptance/progress/identity/history and retained original causes | `review_broker_session_failure_evidence_matrix`, `provider_response_category_codes_and_causes_all_adapters`, `public_broker_and_session_preserve_structured_failure_and_history` |
| Configurable category/status; 429/500/502/503/504; permanent 400/401/403; no protocol catch-all | `recovery_status_selection_v1`, `truncated_permanent_http_bodies_never_retry_all_public_completion_apis`, `partial_http200_body_and_malformed_protocol_never_replayed` |
| Full jitter, exponential ceiling, overflow safety, delay floors, recovery budget/deadline excluding active generation | `budgets_retry_after_and_exact_exponential_delays`, `review_retry_after_numeric_date_ceiling_budget_matrix`, `recovery_budget_and_provider_timeout_do_not_cut_off_admitted_generation`, `absolute_deadline_refuses_retry_without_timing_out_initial_request` |
| Immutable model/messages/images/tools/schema/controls/native history on replay, no hidden retries/redirects | `immutable_reasoning_tools_schema_and_sampling_all_adapters`, `native_history_tool_schema_combination_is_frozen_all_adapters`, `streaming_503_success_preserves_payload_and_accounting_all_adapters` |
| Async admission before each resend; local ambiguity without a hook; remote eligibility without hook | `admission_absent_local_remote_http_matrix`, `admission_explicit_decisions_http_matrix`, `review_explicit_pending_admission_all_adapters` |
| Amendment 2: completed hook without decision has distinct outcome; pending never grants permission | `admission_unresolved_http_matrix`, `admission_unresolved_broker_session_http_matrix`, `admission_pending_budget_http_matrix`, `pending_is_not_permission_then_explicit_allow_dispatches` |
| Amendment 3: five exact facilities, endpoint-specific citations/date, exact termination unknown | `capability_contract_public_adapters_v1` |
| Amendment 4: allowlisted codes, validated request IDs, credential/payload echoes omitted; typed private inspection | `uuid_shaped_credential_or_prompt_echo_is_omitted_from_safe_metadata`, `provider_response_category_codes_and_causes_all_adapters`, `typed_errors_v1_boundaries` |
| Amendment 5: seconds, IMF-fixdate, RFC850 and asctime, past/invalid dates and RFC850 century | `retry_after_floors_dates_invalid_and_refusals`, `rfc850_century_is_resolved_before_weekday_and_floor_validation`, `review_retry_after_numeric_date_ceiling_budget_matrix` |
| Amendment 6: closed provider and operation types across all maintained adapters | `typed_errors_v1_boundaries`, `recovery_status_selection_v1` |
| Amendment 7: oMLX keepalive is raw-only, no model or semantic metadata, recoverable before tokens | `review_keepalive_and_partial_http200_matrix`, `keepalive_only_recovers_raw_without_semantic_progress`, `recovery_metadata_paused_keepalive_cancellation_has_no_semantic_delivery` |
| Amendment 8: typed policy/capture/length errors across entrypoints | `typed_errors_v1_invalid_policy_prevents_all_entrypoints`, `typed_errors_v1_boundaries`, `typed_errors_v1_session_length_proof`, `typed_errors_v1_optional_fields_and_legacy_length` |
| Amendment 9: partial reasoning/content/tool interruption at broker/session, caller eligibility and safe lifecycle fields, 500/502 | `review_broker_session_failure_evidence_matrix`, `session_interruption_never_commits_partial_assistant_or_executes_tools`, `recovery_status_selection_v1` |
| Observed versus delivered semantics, truncated/newline-free frames, no replay after semantics, one terminal outcome | `partial_reasoning_content_and_tools_all_adapters_never_replay`, `no_newline_semantic_or_truncated_frames_never_replay`, `body_capture_failure_retains_observed_semantics_without_delivery_all_adapters` |
| Cancellation at predispatch, active HTTP, paused stream, admission and backoff; no phantom attempt or remote-termination claim | `cancellation_before_dispatch_has_no_phantom_wire_attempt`, `cancellation_active_ordinary_retains_headers_raw_and_no_resend`, `cancellation_retains_received_semantics_and_closes_when_consumer_pauses`, `review_pending_and_backoff_cancel_matrix`, `recovery_metadata_terminal_progress_cancellation_never_completes`, `recovery_rejected_terminal_metadata_cancellation_wins` |
| Safe lifecycle order, opt-in raw per-attempt request/header/body capture; capture failure is terminal | `capture_failure_is_terminal_accounted_and_preserves_original_cause`, `predispatch_capture_failure_has_zero_wire_attempts`, `cancellation_active_body_all_adapters_has_one_ordered_terminal_lifecycle` |
| No repeated completed tool, reset tool depth, agent restart or partial assistant commit | `public_broker_recovery_never_reexecutes_tool`, `review_completed_tool_not_reexecuted_after_followup_failure`, `session_interruption_never_commits_partial_assistant_or_executes_tools` |
| Legacy single-send event API, original provider progress/metrics including rejected terminal frames | `review_raw_broker_stream_events_remain_single_send_legacy`, `recovery_metadata_frozen_ollama_matches_original_and_compatibility`, `recovery_rejected_terminal_telemetry_raw_and_compatibility`, `recovery_rejected_terminal_capture_retains_evidence_without_delivery`, `recovery_terminal_invalid_fields_do_not_fabricate_telemetry` |
| Safe Debug/Display/serialization omit bodies/causes; explicit inspection retains them | `bounded_errors_privacy_and_disabled`, `typed_errors_v1_boundaries`, `body_capture_failure_retains_observed_semantics_without_delivery_all_adapters` |

The event API places client timing immediately before the terminal event to
preserve existing error variants and original causes; collect this metrics event
when an error has no provider evidence container.

No new recovery policy was invented. The timing correction preserves original
raw provider telemetry and cancellation checkpoints. Final metrics live in
`RecoveryReport.stream_metrics` / `RecoveryError.stream_metrics`; raw usage is
explicitly inspected rather than added to default safe summaries.

## Part 2: native reasoning

| Decision | Public executable evidence |
| --- | --- |
| Native reasoning separate from content/tool results; correct Ollama/oMLX wire names | `broker_retains_exact_reasoning_at_tool_boundaries_when_opted_in` in tests/reasoning_contract.rs; `native_history_tool_schema_combination_is_frozen_all_adapters` |
| Disabled/Enabled/Low/Medium/High/XHigh exact booleans and named values; ordinary/structured/stream parity | `effort_is_faithful_for_ordinary_streaming_and_structured_requests`; `native_reasoning_mappings_keep_request_fidelity` in tests/request_fidelity.rs |
| Caller-selected profiles, Qwen3.8 exact efforts, unsupported controls fail before send | `qwen_profile_rejects_high_before_http`, `unsupported_reasoning_rejected_before_send_and_capabilities_truthful`; profile validation tests additionally cover absent effort and history capability |
| Opt-in completed tool-turn retention; cancellation/error never fabricates history; oMLX preserve_thinking | `broker_retains_exact_reasoning_at_tool_boundaries_when_opted_in`, `native_reasoning_streamed_tool_turn_is_retained_and_tool_runs_once`, `session_interruption_never_commits_partial_assistant_or_executes_tools` |
| OpenAI omits unsupported history, rejects new unsupported controls, preserves unrelated behavior | `unsupported_reasoning_rejected_before_send_and_capabilities_truthful`, `dropped_openai_reasoning_warns_at_http_boundary`, `openai_model_restrictions_preserve_selected_output_limits` |

## Part 3: model metadata and request fidelity

| Decision | Public executable evidence |
| --- | --- |
| Context lookup optional trait operation; configured host/auth; exact model selection and source field | tests/model_metadata.rs: `section1_configured_authenticated_boundary`, `section1_http_contract_exact_outcomes_direct_and_wrapped` |
| Positive window or None; missing entry/field/model_info; architecture prefixes; malformed values/body/status/transport | `section1_http_contract_exact_outcomes_direct_and_wrapped`, `section1_transport_failure_direct_and_wrapped` |
| OpenAI registry lookup is exact and request-free; observer rejects ignored POST failure | `section1_registry_exact_lookup_is_request_free`, `section1_request_observer_rejects_ignored_post_failure` |
| Effective Ollama explicit/model num_ctx, maximum clamping, absent metadata; lookup has no completion side effect | `section1_effective_precedence_and_clamping`, `section1_lookup_preserves_subsequent_completion_payload` |
| Third-party gateway compatibility | `section1_existing_third_party_gateway_is_source_compatible` |
| Monotonic dispatch timing, unpolled construction, frame precedence, first-only, arguments before complete call, keepalive exclusion, empty streams, exact successful/error durations, retained first timing | tests/stream_timing.rs: `section2_exact_clocks_all_public_http_streams`, `section2_native_recovery_terminal_success_and_failure_retain_timing` |
| One clock across admitted retries; new clock for each tool continuation | `section2_retry_keeps_first_dispatch_clock`, `section2_broker_session_tool_continuations_start_fresh_clocks` (all three providers, direct/recovery, broker/session; exact 25/60 ms followed by 7/30 ms) |
| Provider metrics unchanged, optional old metric shape readable | `first_frame_precedence_crosses_public_http_boundary`, `section2_old_metric_documents_remain_readable`, `section2_retry_does_not_relabel_prior_attempt_provider_usage`; frozen recovery telemetry tests above; section-2 matrix checks oMLX usage units |
| Optional controls, max_tokens only, exact provider mapping, zero values, old explicit defaults and new unset values, no client controls leaked | tests/request_fidelity.rs: `unset_controls_cross_http_boundary`, `artifact_cases_through_broker_all_http_entrypoints`, `artifact_cases_through_session_send_and_stream`, `artifact_cases_recovery_replays_bytes_and_real_attempt_identities`, `artifact_cases_recovery_through_broker_and_session` |
| Tool continuations preserve controls/history and temperature 0 versus absent; trace migration | `artifact_tool_continuations_preserve_session_controls_and_history`, `session_default_and_temperature_builder_preserve_absence_and_zero`, `trace_temperature_reads_old_numeric_and_new_absent_fields` |
| Fixed OpenAI temperature/output limits, warn on dropped reasoning, exact native mappings | `openai_model_restrictions_preserve_selected_output_limits`, `dropped_openai_reasoning_warns_at_http_boundary`, `native_reasoning_mappings_keep_request_fidelity` |
| Optional/error response behavior and supported old/new fields | `artifact_error_and_optional_response_fields_through_broker`, `typed_errors_v1_optional_fields_and_legacy_length` |

No Anthropic adapter exists here; its required max_tokens fallback is inapplicable.
Nonstream duration is optional in the timing contract and remains unimplemented.
No contract ambiguity requiring a new owner policy was found in these decisions.

## Parts 4–5 and package readiness

Part 4's Kotlin iOS target removal is inapplicable to Rust. Part 5's Python
assistant-message bug and TypeScript temperature-zero bug are owned by those
ports. Rust's negative num_predict cast is eliminated by removing the separate
field; typed `Option<usize>` max_tokens is the sole output limit and the
request-fidelity matrix verifies all maintained wire mappings. Version, Rust
2021/formatting, Tokio, feature defaults, dependencies and the 56% coverage floor
are preserved. No workspace/resolver, MSRV or toolchain pin is declared; CI uses
stable and existing registry/Git/target caches. A declared library MSRV remains
a separate recommendation, not a change made by this task.

Cargo.toml now explicitly includes implementation, executable contracts, examples
and user documentation, excluding tracked Foundry execution artifacts. Run
`python3 scripts/verify_package.py`. It runs `cargo package --allow-dirty`, verifies
archive contents, extracts it outside the checkout, creates a fresh consumer with
a dependency pointing only to that extracted crate, verifies resolution with
`cargo metadata --format-version=1`, then runs
`cargo test --verbose --all-features` on copied packaged public tests. No source
checkout path or independently authored mock is substituted. The generated
`audit/package-readiness-3.0.0.json` records archive SHA-256, base commit,
resolved artifact path, exact contract and tested-source hashes, commands and
actual result. Audit JSON records are detached source-checkout evidence, excluded
from the archive to avoid self-referential artifact hashes. This is a prepublication artifact at unchanged version 2.1.0;
registry adoption and post-publication installation remain held.

## Verification and certification

Current commands/results and exact source/test hashes are recorded in
`audit/readiness-gates-3.0.0.json`; packaged consumer evidence is recorded above.
The behavioral negative control reset the timing origin on each actual retry:
the public HTTP probe rejected 35/75 ms instead of 25/60 ms (exit 101), then
passed with one logical dispatch origin (exit 0). This tests behavior rather
than a marker or event count. The initial missing-field probe also rejected
null first-token kind through real Ollama HTTP and passed after implementation.

**Rust library readiness is established for binding parts 1–5**, subject to
Foundry finalization and coordinated release approval. All seven required gates,
the final retry proof and fresh packaged consumer passed on the frozen executable
source. All default/no-default/docs checks were rerun after the
compatibility correction as part of the same final frozen-source sweep. The delivered trunk and cycles 1–8 are preserved.

Commands ran from this worktree using `/home/svetzal/.cargo/bin` on PATH,
`CARGO_HOME=/tmp/mojentic-c2-cargo` and the default `target/` directory. Builds and
tests were captured with `foundry capture`; complete execution logs and the
behavioral proof remain archived execution evidence rather than shipped files.

| Exact command | Exit | Final result |
| --- | --- | --- |
| `cargo fmt --check` | 0 | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Passed without warnings |
| `cargo test --verbose --all-features` | 0 | 713 passed, including 20 doctests; 24 pre-existing ignored doctests |
| `cargo build --verbose --all-features` | 0 | Passed |
| `cargo tarpaulin --all-features --fail-under 56` | 0 | 71.35%, 3563/4994 lines; unchanged 56% floor |
| `cargo deny check` | 0 | Advisories, bans, licenses and sources passed; existing duplicate-version warnings |
| `cargo audit` | 0 | Passed; no vulnerabilities reported |
| `cargo test --all-features --test stream_timing section2_retry_keeps_first_dispatch_clock` | 0 | Exact logical-clock probe passed on final source |
| `cargo test` | 0 | 713 passed, 24 existing ignored doctests |
| `cargo test --no-default-features` | 0 | 713 passed, 24 existing ignored doctests |
| `cargo doc --no-deps --all-features` | 0 | Generated API docs; two pre-existing rustdoc warnings |
| `mdbook build book` | 0 | Generated HTML; pre-existing search-setting diagnostic |
| `python3 scripts/verify_package.py` | 0 | Crate verification and 151 public tests in fresh extracted-artifact consumer |

Final tested archive SHA-256:
`5f8bd4efdafd2b931d7a7c8bdca67213a47d159e2ed70dc64551a2d8f74ffa7b`.
Its source and executable contracts are byte-identical to the frozen provider
snapshot. `audit/package-readiness-3.0.0.json` identifies the exact extracted
library path, consumer, source hashes, contracts, command results and archive.
The archive contains no Foundry state; two mdBook-created placeholder source
pages were removed before this final package. The conformance/result prose was
finalized after execution; executable source, fixtures and verifiers are unchanged.

Documentation checks are not claimed warning-free: rustdoc retains the existing
unresolved `connect` link and unclosed `dyn` HTML tag. mdBook logs an unknown
`truncate-long-entries` search setting, writes HTML and exits 0. Neither warning
nor diagnostic was suppressed or relabeled as a clean diagnostic run. No runtime,
dependency, advisory policy, test assertion, timeout, coverage engine or threshold
was weakened to pass verification.

This certifies local Rust library readiness, not readiness of all six ports.
**Coordinated release approval is required.** Publication, registry adoption and
post-publication fresh-install verification remain held. No release, dependency
migration, stopped benchmark rerun or cross-repository modification occurred.
Stacey's coverage reconciliation remains binding: renewed passing evidence
permits progress; the historical unexplained failure remains low severity and
further reproduction was not requested or attempted.
