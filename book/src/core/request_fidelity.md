# Request controls and migration to 3.0

Mojentic sends sampling, context and output controls only when you choose them.
`CompletionConfig::default()` leaves `temperature`, `num_ctx`, and `max_tokens`
unset (`None`), so your model and server settings apply. Setting zero temperature
requests zero; it does not mean “use the default”. `top_p` and `top_k` remain
optional. Provider support determines which controls can be sent.

| Config control | Ollama | OpenAI | oMLX |
| --- | --- | --- | --- |
| `temperature` | `options.temperature` | `temperature`, adapted to model restrictions | `temperature` |
| `num_ctx` | `options.num_ctx` | omitted | omitted |
| `max_tokens` | `options.num_predict` | `max_tokens`, or `max_completion_tokens` for registry reasoning models | `max_tokens` |
| `top_p` | `options.top_p` | `top_p` | `top_p` |
| `top_k` | `options.top_k` | omitted | `top_k` |

Every supported field in this table is omitted when unset. There is no separate
`num_predict` config field and no negative sentinel. Use `max_tokens: None` to
leave the output limit to the provider. oMLX does not require `max_tokens`.
Explicit zero limits are forwarded; the provider decides whether to accept them.

OpenAI models with a single allowed temperature rewrite an explicit temperature
to that allowed value. Models that accept no temperature omit it. An unset
value remains absent in both cases. Reasoning effort on a non-reasoning model
is omitted with a warning. Named native reasoning mappings, explicit profile
validation, and restrictions on boolean enablement and reasoning history are
unchanged. See [reasoning controls](reasoning_effort.md).

## Migrating from 2.1

Wrap numeric values in `Some(...)` and handle `None` when reading config fields.
To retain the former library defaults explicitly:

```rust
use mojentic::llm::gateway::CompletionConfig;

let config = CompletionConfig {
    temperature: Some(1.0),
    num_ctx: Some(32768),
    max_tokens: Some(16384),
    ..Default::default()
};
```

Without these settings, Ollama uses model/server precedence, which may produce a
smaller context and unlimited output. OpenAI no longer receives the library's
16,384 output cap; reasoning completions may run longer and cost more. Each
provider retains its own limits.

Use a full config in a session to carry the same intent through ordinary turns,
streaming and automatic tool continuations:

```rust,no_run
use mojentic::llm::{ChatSession, LlmBroker, gateway::CompletionConfig, gateways::OllamaGateway};
use std::sync::Arc;

let broker = LlmBroker::new("qwen3", Arc::new(OllamaGateway::new()), None);
let session = ChatSession::builder(broker)
    .max_context(32768) // local history budget only
    .completion_config(CompletionConfig {
        temperature: Some(0.0),
        num_ctx: Some(131072),
        max_tokens: Some(8192),
        ..Default::default()
    })
    .build();
```

`.temperature(value)` remains available and changes only that config field.
Builder calls apply in order: `.completion_config(...)` replaces the config.
A session with no explicit config sends no provider controls. `max_context`
trims local history and never sets `num_ctx`.

Recovery freezes and replays the encoded request, including absent controls.
Raw event streams retain their single-send behavior even on a recovery wrapper;
see [completion recovery](recovery.md). Tracer call events retain caller intent
as `Option<f64>`: an unset temperature is absent from serialized events. Readers
of older numeric trace events remain supported.

## Provider references and verification limits

[Ollama chat](https://docs.ollama.com/api/chat) accepts runtime options;
[its parameter reference](https://docs.ollama.com/modelfile#valid-parameters-and-values)
describes `num_ctx`, `num_predict` and sampling controls.
[OpenAI Chat Completions](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
documents the optional token-limit and sampling fields, including reasoning-token
accounting for `max_completion_tokens`.
[oMLX's API implementation](https://github.com/jundot/omlx/blob/53ab0e66d75fe46e3cfa7ed7a7762d3bbcf21e9c/omlx/server.py)
is the pinned provider source reference; omission behavior follows the coordinated
October 2026 model-metadata contract and its recorded no-limit server capture.

The shared `request-fidelity-v1` tests exercise the production HTTP adapters and
broker/session paths using artifact-derived responses. Local verification does
not establish external provider or microgentic agreement. Those counterpart
commands and source identities remain required initiative coordination; see
`CONFORMANCE-3.0.0.md` in the repository.
