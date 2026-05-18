//! Configuration for the realtime voice subsystem.

use crate::llm::tools::LlmTool;
use std::collections::HashMap;

pub type RealtimeVoice = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RealtimeModality {
    Audio,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeAudioFormat {
    Pcm16,
    G711Ulaw,
    G711Alaw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptOutputPolicy {
    Drop,
    Submit,
    SubmitCompletedOnly,
}

#[derive(Debug, Clone)]
pub enum RealtimeToolChoice {
    Auto,
    None,
    Required,
    Function(String),
}

#[derive(Debug, Clone, Default)]
pub struct ServerVadConfig {
    pub threshold: Option<f32>,
    pub prefix_padding_ms: Option<u32>,
    pub silence_duration_ms: Option<u32>,
    pub create_response: Option<bool>,
    pub interrupt_response: Option<bool>,
    pub idle_timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticVadConfig {
    pub eagerness: Option<String>,
    pub create_response: Option<bool>,
    pub interrupt_response: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum TurnDetectionMode {
    ServerVad,
    SemanticVad,
    None,
    ServerVadCustom(ServerVadConfig),
    SemanticVadCustom(SemanticVadConfig),
}

#[derive(Default)]
pub struct RealtimeVoiceConfig {
    pub instructions: Option<String>,
    pub voice: Option<RealtimeVoice>,
    pub modalities: Option<Vec<RealtimeModality>>,
    pub input_audio_format: Option<RealtimeAudioFormat>,
    pub output_audio_format: Option<RealtimeAudioFormat>,
    pub turn_detection: Option<TurnDetectionMode>,
    pub input_audio_transcription: Option<InputAudioTranscriptionConfig>,
    pub disable_input_audio_transcription: bool,
    pub tools: Option<Vec<Box<dyn LlmTool>>>,
    pub tool_choice: Option<RealtimeToolChoice>,
    pub temperature: Option<f32>,
    pub max_response_output_tokens: Option<u32>,
    pub on_interrupt: Option<InterruptOutputPolicy>,
    pub provider_extras: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct InputAudioTranscriptionConfig {
    pub model: String,
}

impl Default for InputAudioTranscriptionConfig {
    fn default() -> Self {
        Self {
            model: "whisper-1".to_string(),
        }
    }
}

pub struct Defaults {
    pub modalities: Vec<RealtimeModality>,
    pub input_audio_format: RealtimeAudioFormat,
    pub output_audio_format: RealtimeAudioFormat,
    pub tool_choice: RealtimeToolChoice,
    pub on_interrupt: InterruptOutputPolicy,
}

pub fn defaults() -> Defaults {
    Defaults {
        modalities: vec![RealtimeModality::Audio, RealtimeModality::Text],
        input_audio_format: RealtimeAudioFormat::Pcm16,
        output_audio_format: RealtimeAudioFormat::Pcm16,
        tool_choice: RealtimeToolChoice::Auto,
        on_interrupt: InterruptOutputPolicy::Drop,
    }
}

pub const REALTIME_DEFAULTS: &str = "Use defaults() to access default values at runtime.";
