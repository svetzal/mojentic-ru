# Parallel Tool Execution

Mojentic factors tool-batch execution behind a `ToolRunner` trait so
brokers don't grow their own concurrency code:

| Runner | Behaviour | Default for |
|---|---|---|
| `SerialToolRunner` | Sequential, in input order. | `LlmBroker` (backward-compatible) |
| `ParallelToolRunner` | `FuturesUnordered` + `Semaphore`, default `max_concurrency = 4`. Output order preserved. | `RealtimeVoiceBroker` |

`LlmBroker::new` wires `SerialToolRunner` by default; opt in to
parallel fan-out with `LlmBroker::with_tool_runner`:

```rust,ignore
use mojentic::llm::{LlmBroker, tools::ParallelToolRunner};
use std::sync::Arc;

let broker = LlmBroker::with_tool_runner(
    "qwen3:32b",
    gateway,
    None,
    Arc::new(ParallelToolRunner::default()),
);
```

## Cancellation

`LlmTool::run` accepts a `&ToolRunCtx` carrying a
`tokio_util::sync::CancellationToken`. Long-running async tools can
observe it between work units and abort early:

```rust,ignore
async fn run(&self, args: &HashMap<String, Value>, ctx: &ToolRunCtx) -> Result<Value> {
    for chunk in big_input(args) {
        if ctx.cancel.is_cancelled() {
            return Err(MojenticError::Cancelled);
        }
        process(chunk).await?;
    }
    Ok(json!({ "done": true }))
}
```

Tools that ignore the context continue to work unchanged. The
`SerialToolRunner` and `ParallelToolRunner` themselves surface a
cancelled batch as `ToolCallOutcome { ok: false, error: Some("Cancelled"), .. }`
without invoking the tool.

## Batch tracer event

`LlmBroker` and `RealtimeVoiceBroker` both emit a
`ToolBatchTracerEvent` alongside the per-call `ToolCallTracerEvent`s
whenever a batch executes more than one tool, so observers can measure
parallelism gains.
