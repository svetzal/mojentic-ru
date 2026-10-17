use super::{engine, *};
use crate::{
    error::{MojenticError, Result},
    llm::{
        gateway::{CompletionConfig, LlmGateway, StreamChunk},
        models::{LlmGatewayResponse, LlmMessage},
        stream_events::{StreamEventError, StreamEventStream},
        tools::LlmTool,
    },
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde_json::Value;
use std::pin::Pin;

/// Implemented by completion adapters that can freeze their encoded HTTP request.
#[doc(hidden)]
pub trait RecoveryProvider: LlmGateway {
    fn recovery_provider(&self) -> Provider;
    fn prepare_recovery(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
        schema: Option<Value>,
        stream: bool,
    ) -> Result<reqwest::Request>;
}
/// One-request recovery adapter. Tool execution stays in the existing broker/session.
///
/// ```no_run
/// use mojentic::llm::{LlmMessage, gateway::CompletionConfig, gateways::OllamaGateway};
/// use mojentic::llm::recovery::{RecoveryCall, RecoveryPolicy};
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let gateway = OllamaGateway::new().with_recovery(RecoveryPolicy::default())?;
/// let call = RecoveryCall::default();
/// let result = gateway.complete_with_recovery(
///     "qwen3", &[LlmMessage::user("Explain ownership")], None,
///     &CompletionConfig::default(), &call,
/// ).await?;
/// assert_eq!(result.report.identity.wire_attempt, 1);
/// # Ok(())
/// # }
/// ```
pub struct RecoveryGateway<G> {
    pub(crate) inner: G,
    pub(crate) policy: RecoveryPolicy,
    pub(crate) client: reqwest::Client,
}
impl<G: RecoveryProvider> RecoveryGateway<G> {
    pub fn new(inner: G, policy: RecoveryPolicy) -> Result<Self> {
        policy.validate()?;
        let client = reqwest::Client::builder()
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            inner,
            policy,
            client,
        })
    }
    /// Endpoint-specific facilities, with no change to retry or admission policy.
    pub fn capabilities(&self) -> Capabilities {
        self.capability_evidence().values()
    }
    /// Provider documentation, inspection dates and limitations for each facility.
    /// Local cancellation is never evidence of exact remote attempt termination.
    pub fn capability_evidence(&self) -> CapabilityEvidenceSet {
        CapabilityEvidenceSet::for_provider(self.inner.recovery_provider())
    }
    /// Full metadata for an ordinary completion. No tool is executed by this method.
    pub async fn complete_with_recovery(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
        call: &RecoveryCall,
    ) -> std::result::Result<Recovered<LlmGatewayResponse>, RecoveryError> {
        let request = self.prepare(model, messages, tools, config, None, Operation::Ordinary)?;
        engine::complete(self, request, Operation::Ordinary, call)
            .await
            .map(|r| Recovered {
                value: LlmGatewayResponse {
                    content: r.value.content,
                    thinking: r.value.thinking,
                    tool_calls: r.value.tool_calls,
                    evidence: r.value.evidence,
                    object: None,
                },
                report: r.report,
            })
    }
    pub async fn complete_json_with_recovery(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
        call: &RecoveryCall,
    ) -> std::result::Result<Recovered<LlmGatewayResponse<Value>>, RecoveryError> {
        let request =
            self.prepare(model, messages, None, config, Some(schema), Operation::Structured)?;
        engine::complete(self, request, Operation::Structured, call).await
    }
    /// Reasoning and tool fragments are observable, but never executable until completion.
    pub fn complete_stream_with_recovery<'a>(
        &'a self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
        call: &'a RecoveryCall,
    ) -> Pin<Box<dyn Stream<Item = RecoveryStreamEvent> + Send + 'a>> {
        match self.prepare(model, messages, tools, config, None, Operation::Streaming) {
            Ok(request) => engine::stream(self, request, call),
            Err(error) => {
                Box::pin(futures::stream::once(async { RecoveryStreamEvent::Failed(error) }))
            }
        }
    }
    fn prepare(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
        schema: Option<Value>,
        operation: Operation,
    ) -> std::result::Result<reqwest::Request, RecoveryError> {
        self.inner
            .prepare_recovery(
                model,
                messages,
                tools,
                config,
                schema,
                operation == Operation::Streaming,
            )
            .map_err(|cause| {
                let mut failure =
                    Failure::new(self.inner.recovery_provider(), operation, Category::Protocol)
                        .with_cause(cause);
                failure.acceptance = Acceptance::No;
                failure.classification.reason = Reason::InvalidConfiguration;
                RecoveryError {
                    outcome: Outcome::Failed,
                    failure: Box::new(failure),
                    history: vec![],
                }
            })
    }
}
fn legacy(error: RecoveryError) -> MojenticError {
    MojenticError::Recovery(Box::new(error))
}
#[async_trait]
impl<G: RecoveryProvider> LlmGateway for RecoveryGateway<G> {
    async fn complete(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse> {
        self.complete_with_recovery(model, messages, tools, config, &RecoveryCall::default())
            .await
            .map(|r| r.value)
            .map_err(legacy)
    }
    async fn complete_json(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<Value> {
        Ok(self
            .complete_json_response(model, messages, schema, config)
            .await?
            .object
            .unwrap_or(Value::Null))
    }
    async fn complete_json_response(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse<Value>> {
        self.complete_json_with_recovery(model, messages, schema, config, &RecoveryCall::default())
            .await
            .map(|r| r.value)
            .map_err(legacy)
    }
    async fn get_available_models(&self) -> Result<Vec<String>> {
        self.inner.get_available_models().await
    }
    async fn calculate_embeddings(&self, text: &str, model: Option<&str>) -> Result<Vec<f32>> {
        self.inner.calculate_embeddings(text, model).await
    }
    fn complete_stream<'a>(
        &'a self,
        model: &'a str,
        messages: &'a [LlmMessage],
        tools: Option<&'a [Box<dyn LlmTool>]>,
        config: &'a CompletionConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
        Box::pin(async_stream::stream! {
            let call = RecoveryCall::default();
            let mut stream = self.complete_stream_with_recovery(model, messages, tools, config, &call);
            while let Some(event) = stream.next().await {
                match event {
                    RecoveryStreamEvent::Progress { progress, .. } => yield Ok(StreamChunk::Progress(progress)),
                    RecoveryStreamEvent::Metrics { metrics, .. } => yield Ok(StreamChunk::Metrics(metrics)),
                    RecoveryStreamEvent::Content(text) => yield Ok(StreamChunk::Content(text)),
                    RecoveryStreamEvent::Reasoning(text) => yield Ok(StreamChunk::Thinking(text)),
                    RecoveryStreamEvent::Completed(response) => { if !response.value.tool_calls.is_empty() { yield Ok(StreamChunk::ToolCalls(response.value.tool_calls)); } },
                    RecoveryStreamEvent::Failed(error) => { yield Err(legacy(error)); break; },
                    _ => {}
                }
            }
        })
    }
    fn complete_stream_events<'a>(
        &'a self,
        model: &'a str,
        messages: &'a [LlmMessage],
        config: &'a CompletionConfig,
    ) -> std::result::Result<StreamEventStream<'a>, StreamEventError> {
        self.inner.complete_stream_events(model, messages, config)
    }
}
