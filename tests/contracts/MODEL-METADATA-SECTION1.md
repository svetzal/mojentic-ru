# Model metadata section 1 executable contract

Version: `model-metadata-section1-v1`. SHA-256 of `model_metadata_section1_v1.json`:
`617c0df913416e8cbfa5873256d427857c1f998103b9b04a4e2b3a456d07ac07`.

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

Provider-side executable verification and microgentic consumer verification
against this exact set/hash are missing. No external tested commit, command or
result exists from this run. Captures and fixture-derived doubles establish
local conformance only. The initiative must supply this artifact and verifier
semantics to both sides and record their tested commits and required-gate
results; missing paired verification remains unfinished work. No external
repository was changed, and publication and consumer-migration holds remain.
