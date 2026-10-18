# Rust coordinated 3.0.0 parity handoff

This record is limited to mojentic-ru. The coordinating PARITY.md and other
repositories remain owner-managed; no cross-port readiness is inferred.
See CONFORMANCE-3.0.0.md for binding document hashes, the decision-to-test map,
current source identities, package evidence and final verification results.

| Release part | Rust applicability and evidence |
| --- | --- |
| 1. Transient recovery and October 12 amendment | Ollama/oMLX/OpenAI public HTTP gateway/broker/session matrices; exact capabilities and cited provider facilities; admission item 2 preserved |
| 2. Native reasoning | Exact six efforts, caller profiles, separate history, opt-in retention, structured/stream/tool parity and explicit OpenAI restrictions |
| 3. Context metadata, timing, request fidelity | Versioned public contracts; all maintained streaming boundaries; one retry clock and fresh tool-continuation clocks; packaged-consumer execution |
| 4. Kotlin iOS removal | Inapplicable to Rust; Kotlin verification belongs to its authorized task |
| 5. Audit fixes and packaging | Negative num_predict cast removed with the separate field; crate allowlist excludes Foundry state; fresh extracted-artifact consumer verifies public APIs |

Rust local readiness is verified: all required gates passed, coverage is
71.35% against the unchanged 56% floor, and the fresh packaged consumer
passed 151 public tests. The final source and contract hashes are in the
conformance-linked audit records.
Stacey's library self-validation rule removes unreleased dependent/provider/other
port snapshots as publication prerequisites. External tested commits/results are
not claimed. Coordinated release approval still requires all six ports ready.
Registry adoption and post-publication install verification remain held, including
microgentic's dependency switch and removal of its local metadata/timing modules,
and the Elixir dependent migrations recorded by the initiative. This task neither
changes those repositories nor dispatches their work. Benchmark efficacy and
stopped experiment reruns are outside library compatibility evidence.

The earlier unexplained coverage failure remains low severity under Stacey's
binding reconciliation. Renewed passing evidence permits progress; no further
reproduction is requested. Keep the 56% floor and the existing coverage engine.
