# Ollama fixtures

Raw responses from a live Ollama server, for the gateway tests in every port.
The contract that uses them is `MODEL-METADATA-2026-10.md` at the monorepo
root.

Captured on 2026-10-15 from Ollama 0.40.5 (macOS, Apple Silicon). The files are
unedited. Response headers are not included.

| File | Request | What it shows |
| ---- | ------- | ------------- |
| `show_context_length.json` | `POST /api/show {"model":"qwen3.8:27b-mlx"}` | `model_info."qwen3_5.context_length": 262144`; `parameters` has no `num_ctx` line |
| `show_num_ctx_parameter.json` | `POST /api/show {"model":"qwen3.8-flash-next:125b-mlx"}` | `model_info."qwen4_exp.context_length": 262144`; `parameters` has a `num_ctx 262144` line |
| `error_model_not_found.json` | `POST /api/show {"model":"no-such-model:1b"}` | HTTP 404 body |

The `context_length` key carries an architecture prefix that differs between
models (`qwen3_5`, `qwen4_exp`). Match the `.context_length` suffix, not a
fixed key.
