# Model context windows

Use provider metadata to learn the advertised capacity before deciding your
application's token budget. Lookup does not configure a completion or load a
model. Unknown capacities stay unknown rather than acquiring a guessed limit.

```rust,no_run
use mojentic::llm::{CompletionConfig, LlmGateway, gateways::OllamaGateway};

# async fn example() -> mojentic::error::Result<()> {
let gateway = OllamaGateway::new();
let window = gateway.get_model_context_window("qwen3.8:27b-mlx").await?;
if let Some(window) = &window {
    println!("{} tokens from {}", window.max_context_tokens, window.field);
}
let config = CompletionConfig { num_ctx: Some(8192), ..Default::default() };
let effective = OllamaGateway::effective_context_window(window.as_ref(), &config);
if let Some(tokens) = effective {
    assert!(tokens <= 8192);
}
# Ok(())
# }
```

`Ok(Some(ModelContextWindow))` contains a positive `max_context_tokens`, its
`field` provenance, and optional Ollama `model_num_ctx`. `Ok(None)` means the
provider did not advertise a window for that model. Transport failures,
non-success statuses (including an unknown-model Ollama 404), malformed JSON
and invalid advertised windows return the existing gateway errors.

| Gateway | Lookup | Provenance |
| --- | --- | --- |
| oMLX | `GET /v1/models` at the configured host, with configured bearer key; exact `id` selection | `max_model_len` |
| Ollama | `POST /api/show` with `{"model": model}`, configured host, headers and timeout | `model_info.<architecture>.context_length` |
| OpenAI | Exact entry in the bundled model registry; no HTTP request | `registry` |

OpenAI's completion capability inference for unknown model names is unchanged;
metadata never uses that inference to guess a context window. Third-party
`LlmGateway` implementations inherit `Ok(None)` without needing a new method.
`RecoveryGateway` delegates metadata lookup to its wrapped adapter; lookup does
not opt into completion retries or change admission behavior.

Ollama's effective-window helper first requires an advertised window. It clamps
an explicit `config.num_ctx` to that maximum, otherwise clamps the model's
`num_ctx` parameter, otherwise returns the maximum. If no window is known it
returns `None`, even when configuration sets `num_ctx`. The helper is pure and
does not modify the configuration or later HTTP payloads.

Without either `num_ctx`, the result is an upper bound: the client cannot see
the server's `OLLAMA_CONTEXT_LENGTH` or built-in default. Applications needing
an exact loaded context can inspect `GET /api/ps` after their first request.
Budgeting, pressure bands and eviction remain application policy.

This implements section 1 of
[MODEL-METADATA-2026-10.md](https://github.com/svetzal/mojentic-unify/blob/4b08dcd1d87f520a4c70928c33a2bc84c228093d/MODEL-METADATA-2026-10.md).
Provider evidence is the coordinating
[Ollama 0.40.5 captures](https://github.com/svetzal/mojentic-unify/tree/4b08dcd1d87f520a4c70928c33a2bc84c228093d/fixtures/ollama)
and [oMLX models capture](https://github.com/svetzal/mojentic-unify/blob/4b08dcd1d87f520a4c70928c33a2bc84c228093d/fixtures/omlx/models.json).
The local executable contract is `tests/contracts/model_metadata_section1_v1.json`;
`cargo test --all-features --test model_metadata` verifies the public adapters
against it. The same public tests run in a fresh consumer of the packaged crate.
External dependent adoption follows coordinated publication under Stacey’s library
self-validation rule; no live provider verification is claimed.

Client first-token and request-duration measurements are described in
[Streaming](streaming.md#client-timing). Lookup itself does not start a stream clock.
