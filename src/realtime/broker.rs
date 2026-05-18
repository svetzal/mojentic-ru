//! Realtime voice broker — sibling to [`crate::llm::broker::LlmBroker`].

use crate::llm::tools::{ParallelToolRunner, ToolRunner};
use crate::realtime::config::RealtimeVoiceConfig;
use crate::realtime::gateway::RealtimeVoiceGateway;
use crate::realtime::session::RealtimeSession;
use crate::realtime::transport::TransportError;
use crate::tracer::TracerSystem;
use std::sync::Arc;
use uuid::Uuid;

/// Long-lived broker that opens duplex realtime sessions against a
/// gateway. Reusable across many concurrent sessions; the
/// [`RealtimeSession`] returned by [`connect`] owns the socket
/// lifetime.
pub struct RealtimeVoiceBroker {
    model: String,
    gateway: Arc<dyn RealtimeVoiceGateway>,
    config: RealtimeVoiceConfig,
    tracer: Option<Arc<TracerSystem>>,
    tool_runner: Arc<dyn ToolRunner>,
}

impl RealtimeVoiceBroker {
    pub fn new(
        model: impl Into<String>,
        gateway: Arc<dyn RealtimeVoiceGateway>,
        config: RealtimeVoiceConfig,
        tracer: Option<Arc<TracerSystem>>,
        tool_runner: Option<Arc<dyn ToolRunner>>,
    ) -> Self {
        Self {
            model: model.into(),
            gateway,
            config,
            tracer,
            tool_runner: tool_runner.unwrap_or_else(|| Arc::new(ParallelToolRunner::default())),
        }
    }

    /// Open a new realtime session.
    ///
    /// Returns the session handle ready to drive: text in / events out
    /// flow through it. The initial ``session.update`` is sent in the
    /// background by the session driver before the first event lands.
    pub async fn connect(&self) -> Result<RealtimeSession, TransportError> {
        let correlation_id = Uuid::new_v4().to_string();
        let gateway_session =
            self.gateway.open(&self.model, &self.config, Some(&correlation_id)).await?;

        Ok(RealtimeSession::run(
            gateway_session,
            self.config.clone(),
            self.tool_runner.clone(),
            self.tracer.clone(),
            correlation_id,
        )
        .await)
    }
}
