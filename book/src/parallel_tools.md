# Parallel Tool Execution

Mojentic factors tool-batch execution behind a `ToolRunner` trait so
brokers don't grow their own concurrency code:

| Runner | Behaviour | Default for |
|---|---|---|
| `SerialToolRunner` | Sequential, in input order. | `LlmBroker` (backward-compatible) |
| `ParallelToolRunner` | `FuturesUnordered` + `Semaphore`, default `max_concurrency = 4`. Output order preserved. | `RealtimeVoiceBroker` |

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

Tools that ignore the context continue to work unchanged.

## Batch tracer event

The realtime broker (and any caller that drives `ParallelToolRunner`)
emits a `ToolBatchTracerEvent` alongside the per-call
`ToolCallTracerEvent`s, so observers can measure parallelism gains.
