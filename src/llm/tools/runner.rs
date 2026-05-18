//! Tool runner abstraction for executing batches of tool calls.
//!
//! Provides pluggable execution strategies (serial, parallel) so brokers can
//! stay independent of concurrency policy. Mirrors the Python and TypeScript
//! `ToolRunner` design.

use crate::llm::tools::tool::{LlmTool, ToolRunCtx};
use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

/// A single tool call to execute, identified by an opaque id.
///
/// The id is preserved on the matching [`ToolCallOutcome`] so callers can
/// pair calls and outcomes deterministically.
#[derive(Debug, Clone)]
pub struct ToolCallExecution {
    pub id: String,
    pub name: String,
    pub args: HashMap<String, Value>,
}

/// Outcome of executing a single tool call.
#[derive(Debug, Clone)]
pub struct ToolCallOutcome {
    pub id: String,
    pub name: String,
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Strategy for executing a batch of tool calls.
///
/// Implementations decide concurrency, ordering, and cancellation semantics.
/// Output order must match input order regardless of execution order.
#[async_trait]
pub trait ToolRunner: Send + Sync {
    async fn run_batch(
        &self,
        calls: &[ToolCallExecution],
        tools: &[Box<dyn LlmTool>],
        ctx: &ToolRunCtx,
    ) -> Vec<ToolCallOutcome>;
}

/// Executes tool calls one at a time in input order.
///
/// Default for [`crate::llm::broker::LlmBroker`] — predictable, stepwise
/// debugging and zero behaviour change for callers who don't opt in.
#[derive(Debug, Default, Clone)]
pub struct SerialToolRunner;

#[async_trait]
impl ToolRunner for SerialToolRunner {
    async fn run_batch(
        &self,
        calls: &[ToolCallExecution],
        tools: &[Box<dyn LlmTool>],
        ctx: &ToolRunCtx,
    ) -> Vec<ToolCallOutcome> {
        let mut outcomes = Vec::with_capacity(calls.len());
        for call in calls {
            if ctx.cancel.is_cancelled() {
                outcomes.push(aborted_outcome(call));
                continue;
            }
            outcomes.push(execute_one(call, tools, ctx).await);
        }
        outcomes
    }
}

/// Executes tool calls concurrently with a bounded fan-out.
///
/// `max_concurrency` defaults to 4 — high enough to win on typical realtime
/// turns, low enough that unbounded fan-out into rate-limited APIs doesn't
/// punish users.
#[derive(Debug, Clone)]
pub struct ParallelToolRunner {
    pub max_concurrency: usize,
}

impl Default for ParallelToolRunner {
    fn default() -> Self {
        Self { max_concurrency: 4 }
    }
}

impl ParallelToolRunner {
    pub fn new(max_concurrency: usize) -> Self {
        assert!(max_concurrency >= 1, "max_concurrency must be >= 1");
        Self { max_concurrency }
    }
}

#[async_trait]
impl ToolRunner for ParallelToolRunner {
    async fn run_batch(
        &self,
        calls: &[ToolCallExecution],
        tools: &[Box<dyn LlmTool>],
        ctx: &ToolRunCtx,
    ) -> Vec<ToolCallOutcome> {
        if calls.is_empty() {
            return Vec::new();
        }

        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let mut futures = FuturesUnordered::new();

        for (idx, call) in calls.iter().enumerate() {
            let permit_sem = Arc::clone(&semaphore);
            let cancel = ctx.cancel.clone();
            futures.push(async move {
                if cancel.is_cancelled() {
                    return (idx, aborted_outcome(call));
                }
                let _permit = permit_sem.acquire().await.expect("semaphore closed");
                if cancel.is_cancelled() {
                    return (idx, aborted_outcome(call));
                }
                (idx, execute_one(call, tools, ctx).await)
            });
        }

        let mut indexed: Vec<(usize, ToolCallOutcome)> = Vec::with_capacity(calls.len());
        while let Some(pair) = futures.next().await {
            indexed.push(pair);
        }
        indexed.sort_by_key(|(idx, _)| *idx);
        indexed.into_iter().map(|(_, o)| o).collect()
    }
}

fn aborted_outcome(call: &ToolCallExecution) -> ToolCallOutcome {
    ToolCallOutcome {
        id: call.id.clone(),
        name: call.name.clone(),
        ok: false,
        result: None,
        error: Some("Tool batch aborted".to_string()),
        duration_ms: 0,
    }
}

async fn execute_one(
    call: &ToolCallExecution,
    tools: &[Box<dyn LlmTool>],
    ctx: &ToolRunCtx,
) -> ToolCallOutcome {
    let start = Instant::now();
    let tool = tools.iter().find(|t| t.matches(&call.name));
    let outcome = match tool {
        None => ToolCallOutcome {
            id: call.id.clone(),
            name: call.name.clone(),
            ok: false,
            result: None,
            error: Some(format!("Tool {:?} not found", call.name)),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Some(tool) => match tool.run(&call.args, ctx).await {
            Ok(result) => ToolCallOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: true,
                result: Some(result),
                error: None,
                duration_ms: start.elapsed().as_millis() as u64,
            },
            Err(err) => ToolCallOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                result: None,
                error: Some(format!("{err}")),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        },
    };
    outcome
}

/// Convenience: build a fresh cancellation token for callers that don't have
/// one already (used by the sync broker code path).
pub fn fresh_cancel_token() -> CancellationToken {
    CancellationToken::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct EchoTool;

    #[async_trait]
    impl LlmTool for EchoTool {
        async fn run(
            &self,
            args: &HashMap<String, Value>,
            _ctx: &ToolRunCtx,
        ) -> crate::error::Result<Value> {
            Ok(json!({ "echo": args.get("value").cloned().unwrap_or_default() }))
        }

        fn descriptor(&self) -> ToolDescriptorPlaceholder {
            unimplemented!()
        }

        fn clone_box(&self) -> Box<dyn LlmTool> {
            Box::new(EchoTool)
        }
    }

    // Helper alias to avoid pulling the real descriptor type into the test;
    // the test only exercises run() and matches() through the runner.
    use crate::llm::tools::tool::ToolDescriptor as ToolDescriptorPlaceholder;

    impl EchoTool {
        fn boxed_descriptor() -> crate::llm::tools::tool::ToolDescriptor {
            crate::llm::tools::tool::ToolDescriptor {
                r#type: "function".to_string(),
                function: crate::llm::tools::tool::FunctionDescriptor {
                    name: "echo".to_string(),
                    description: "Echo".to_string(),
                    parameters: json!({}),
                },
            }
        }
    }

    // We re-implement descriptor() above as unimplemented to keep the impl
    // minimal; provide a working impl here for `matches()` to succeed.
    struct EchoToolReal;

    #[async_trait]
    impl LlmTool for EchoToolReal {
        async fn run(
            &self,
            args: &HashMap<String, Value>,
            _ctx: &ToolRunCtx,
        ) -> crate::error::Result<Value> {
            Ok(json!({ "echo": args.get("value").cloned().unwrap_or_default() }))
        }

        fn descriptor(&self) -> crate::llm::tools::tool::ToolDescriptor {
            EchoTool::boxed_descriptor()
        }

        fn clone_box(&self) -> Box<dyn LlmTool> {
            Box::new(EchoToolReal)
        }
    }

    fn exec(id: &str, name: &str, value: &str) -> ToolCallExecution {
        let mut args = HashMap::new();
        args.insert("value".to_string(), json!(value));
        ToolCallExecution {
            id: id.to_string(),
            name: name.to_string(),
            args,
        }
    }

    #[tokio::test]
    async fn serial_preserves_input_order() {
        let runner = SerialToolRunner;
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(EchoToolReal)];
        let calls = vec![
            exec("1", "echo", "a"),
            exec("2", "echo", "b"),
            exec("3", "echo", "c"),
        ];
        let ctx = ToolRunCtx::default();

        let outcomes = runner.run_batch(&calls, &tools, &ctx).await;

        assert_eq!(outcomes.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), vec!["1", "2", "3"]);
        assert!(outcomes.iter().all(|o| o.ok));
    }

    #[tokio::test]
    async fn parallel_preserves_output_order() {
        let runner = ParallelToolRunner::new(4);
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(EchoToolReal)];
        let calls = vec![
            exec("a", "echo", "1"),
            exec("b", "echo", "2"),
            exec("c", "echo", "3"),
        ];
        let ctx = ToolRunCtx::default();

        let outcomes = runner.run_batch(&calls, &tools, &ctx).await;

        assert_eq!(outcomes.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn unknown_tools_return_not_found() {
        let runner = SerialToolRunner;
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(EchoToolReal)];
        let calls = vec![exec("1", "missing", "x")];

        let outcomes = runner.run_batch(&calls, &tools, &ToolRunCtx::default()).await;

        assert!(!outcomes[0].ok);
        assert!(outcomes[0].error.as_ref().map(|s| s.contains("missing")).unwrap_or(false));
    }

    #[tokio::test]
    async fn pre_cancelled_batch_skips_dispatch() {
        let runner = ParallelToolRunner::new(2);
        let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(EchoToolReal)];
        let calls = vec![exec("1", "echo", "x"), exec("2", "echo", "y")];
        let ctx = ToolRunCtx::default();
        ctx.cancel.cancel();

        let outcomes = runner.run_batch(&calls, &tools, &ctx).await;

        assert!(outcomes.iter().all(|o| !o.ok));
        assert!(outcomes.iter().all(|o| o.error.as_deref() == Some("Tool batch aborted")));
    }
}
