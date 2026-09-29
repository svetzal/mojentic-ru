# oMLX fixtures

Raw responses from a live oMLX server, used by `tests/omlx_gateway.rs`.

These files are byte-identical copies of `fixtures/omlx/` in the mojentic
monorepo (commit `63409fa`, "docs: add oMLX gateway contract and live
fixtures"). The contract they support is `OMLX-2026-09.md` at the monorepo
root. This port's CI cannot see the monorepo, so the copies live here. If the
monorepo fixtures change, copy them again. Do not edit them by hand.

Captured on 2026-09-29 from oMLX 0.7.0rc1 (Homebrew, macOS, Apple Silicon)
serving `Qwen3.8-27B-MLX-8bit`. The files are unedited. Response headers are
not included.

| File | Request |
| ---- | ------- |
| `chat_thinking.json` | Plain chat, model default (thinking on) |
| `chat_thinking_disabled.json` | Same, with `enable_thinking: false` |
| `chat_tool_call.json` | One tool offered; the model calls it |
| `chat_after_tool_result.json` | The tool result sent back as a `tool` message |
| `chat_json_schema.json` | `response_format` `json_schema`, grammar enforced |
| `chat_length.json` | `max_tokens: 5`; truncated during thinking |
| `stream_thinking.sse` | Streamed plain chat, `include_usage` |
| `stream_tool_call.sse` | Streamed tool call |
| `stream_length.sse` | Streamed, `max_tokens: 5` |
| `models.json` | `GET /v1/models` |
| `model_load.json`, `model_unload.json` | Load and unload |
| `error_model_not_loaded.json` | Unload of a model that is not loaded (400) |
| `error_model_not_found.json` | Chat with an unknown model (404) |
| `error_not_embedding_model.json` | Embeddings with a chat model (400) |

Every stream starts with a keep-alive frame whose `model` is `keepalive`. The
gateway drops those frames before the shared OpenAI stream parser sees them.
