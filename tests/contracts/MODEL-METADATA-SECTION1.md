# Model metadata section 1 executable contract

Version: `model-metadata-section1-v1`. SHA-256 of `model_metadata_section1_v1.json`:
`cd7b798a9b21fd339c749a14702b1ac53bb774dde9427f52db26a91dc4803e52`.

Normative source: MODEL-METADATA-2026-10.md section 1, coordinating repository
`svetzal/mojentic-unify` at `4b08dcd1d87f520a4c70928c33a2bc84c228093d`.
Specification SHA-256: `a085da1c0d110f9d587a24ace6ac2854b653e1ac28905b3829d94d665b2c906e`.
No applicable section-1 executable set was found in that checkout; this local
set reuses the existing JSON-contract integration-test mechanism. It does not
change the coordinating contract's decisions or claim external agreement.

Provider citations:

- [Ollama captures and request descriptions](https://github.com/svetzal/mojentic-unify/tree/4b08dcd1d87f520a4c70928c33a2bc84c228093d/fixtures/ollama):
  Ollama 0.40.5, captured 2026-10-15. Local `tests/fixtures/ollama/` copies are
  byte-identical. Both architecture prefixes and the unknown-model 404 body
  are used directly in this executable set.
- [oMLX model capture](https://github.com/svetzal/mojentic-unify/blob/4b08dcd1d87f520a4c70928c33a2bc84c228093d/fixtures/omlx/models.json):
  oMLX 0.7.0rc1, captured 2026-09-29. The pre-existing local copy and provenance
  README remain unchanged.
- OpenAI windows use exact existing Rust registry values at base commit
  `50387e33ba46e5b67737f70b6918bd6581b1b96b`; registry contents are unchanged.
  This set does not certify those values against a live OpenAI provider.

Run `cargo test --all-features --test model_metadata`. Ordinary `cargo test`,
the all-feature CI test gate and tarpaulin all execute the same integration
verifier. `section1_contract_reuses_coordinating_captures` asserts byte equality
between the embedded responses and copied provider captures. Other cases are
explicit section-1 edge inputs, not additional provider captures. HTTP doubles
read the versioned artifact; exact public API outcomes, methods, paths, auth,
model selection, optional values, errors and completion payloads are asserted.
The completion-preservation cases reuse `request-fidelity-v1` response bodies
and characterize the existing direct/wrapped `stream` field difference.

Supported compatibility: the new 3.0.0 operation defaults to `Ok(None)` on an
existing third-party trait implementation. Existing 2.1-style explicit
completion controls and 3.0 unset controls are preserved before and after
lookup. This is not a 2.1 provider wire-protocol version claim; section 1 does
not specify multiple metadata protocol versions.

Stacey's library self-validation rule supersedes external paired-verification
prerequisites for publication. The library's public integration tests and fresh
packaged consumer execute this same contract. No live provider or external
consumer verification is claimed; dependents verify when adopting the released
package. See CONFORMANCE-3.0.0.md for current whole-release results.

## Cycle 8: OpenAI request observation correction

Tested baseline: `552cf7738ebcfbe1f721ea10ab00511ece5282e4` plus the
uncommitted `tests/model_metadata.rs` correction in worktree
`mojentic-3-0-ru-c8-3f29a7`. Verifier SHA-256:
`1cc51637a0638015d77f35078759c35a700aca22108f0fb2b67209460adcc056`.
The v1 artifact and hash above are unchanged; no fixture or runtime change is
needed. All seven `.hone-gates.json` commands and the four additional applicable
CI/repository checks passed (exit 0), including 706 all-feature tests and 70.70%
coverage against the unchanged 56% floor. See `CONFORMANCE-3.0.0.md` for exact
commands, results and existing documentation diagnostics.

`section1_registry_exact_lookup_is_request_free` invokes the real public
`OpenAIGateway` and `RecoveryGateway` operations for each of the four v1 registry
cases: two exact known names, an unknown variant and an unknown name. It asserts
exact metadata (including optional fields) or `None` for each operation. Each
case/adapter pair has its own local TCP observer. The observer accepts any
connection, records its request line without filtering method or path, returns
HTTP 503, drains pending connections on shutdown and asserts no requests.
Lookup success at an unreachable host is no longer used as request evidence.

`section1_request_observer_rejects_ignored_post_failure` sends an actual POST to
`/ignored-failure?probe=1`, deliberately discards the HTTP failure, then requires
the same zero-request assertion to panic with that exact request line. This
negative control runs in the ordinary test, CI and coverage gates. Before the
full suite, a temporary mutation of the real OpenAI metadata implementation
sent that failing POST and ignored its error: the registry verifier rejected it
(exit 101). Restoring the implementation byte-for-byte made
`cargo test --all-features --test model_metadata` pass (exit 0, nine tests).
This proves observation of a request through the real gateway boundary rather
than a manually changed count. The temporary mutation is not part of the change.

This is local library conformance. External provider and microgentic commits
were not tested here. Stacey's library self-validation rule supersedes the
historical demand for their verification before publication. A fresh packaged
consumer now verifies this identical v1 contract in this repository's readiness
gates; external dependent adoption follows the coordinated release.

Current correction: the observer is an injected in-process `HttpTransport`.
It records every encoded method/path before supplying HTTP 503. The registry
lookup assertions and ignored-POST negative control retain their exact metadata
and request-line assertions. The cycle-8 TCP mechanism above is historical and
is superseded; current verification opens no connections.
