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
            self.clone_config(),
            self.tool_runner.clone(),
            self.tracer.clone(),
            correlation_id,
        )
        .await)
    }

    fn clone_config(&self) -> RealtimeVoiceConfig {
        RealtimeVoiceConfig {
            instructions: self.config.instructions.clone(),
            voice: self.config.voice.clone(),
            modalities: self.config.modalities.clone(),
            input_audio_format: self.config.input_audio_format,
            output_audio_format: self.config.output_audio_format,
            turn_detection: self.config.turn_detection.clone(),
            input_audio_transcription: self.config.input_audio_transcription.clone(),
            disable_input_audio_transcription: self.config.disable_input_audio_transcription,
            tools: self.config.tools.as_ref().map(|ts| ts.iter().map(|t| t.clone_box()).collect()),
            tool_choice: self.config.tool_choice.clone(),
            temperature: self.config.temperature,
            max_response_output_tokens: self.config.max_response_output_tokens,
            on_interrupt: self.config.on_interrupt,
            provider_extras: self.config.provider_extras.clone(),
        }
    }
}
