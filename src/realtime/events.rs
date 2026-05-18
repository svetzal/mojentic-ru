//! Vendor-neutral event union for the realtime subsystem.

use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub extras: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone)]
pub struct RealtimeItem {
    pub id: String,
    pub item_type: String,
    pub role: Option<String>,
    pub text: Option<String>,
    pub transcript: Option<String>,
    pub name: Option<String>,
    pub args: Option<HashMap<String, Value>>,
    pub output: Option<Value>,
    pub call_id: Option<String>,
}

/// Vendor-neutral event types emitted by [`crate::realtime::RealtimeSession`].
#[derive(Debug, Clone)]
pub enum RealtimeEvent {
    // Session lifecycle
    SessionOpened {
        session_id: String,
    },
    SessionUpdated {
        config: HashMap<String, Value>,
    },
    SessionClosed {
        reason: SessionCloseReason,
    },

    // User speech
    UserSpeechStarted {
        at_ms: u64,
    },
    UserSpeechStopped {
        at_ms: u64,
    },
    UserTranscriptDelta {
        item_id: String,
        delta: String,
    },
    UserTranscript {
        item_id: String,
        text: String,
    },

    // Assistant output
    AssistantTurnStarted {
        turn_id: String,
    },
    AssistantTextDelta {
        turn_id: String,
        delta: String,
    },
    AssistantText {
        turn_id: String,
        text: String,
    },
    AssistantTranscriptDelta {
        turn_id: String,
        delta: String,
    },
    AssistantTranscript {
        turn_id: String,
        text: String,
    },
    AssistantAudioDelta {
        turn_id: String,
        pcm: Vec<i16>,
    },
    AssistantTurnCompleted {
        turn_id: String,
        usage: Option<TokenUsage>,
    },

    // Tool calls
    ToolCallStarted {
        turn_id: String,
        call_id: String,
        name: String,
    },
    ToolCallArgsDelta {
        call_id: String,
        delta: String,
    },
    ToolCallDispatched {
        call_id: String,
        name: String,
        args: HashMap<String, Value>,
    },
    ToolCallCompleted {
        call_id: String,
        name: String,
        result: Value,
    },
    ToolCallFailed {
        call_id: String,
        name: String,
        error: String,
    },
    ToolBatchSubmitted {
        turn_id: String,
        call_ids: Vec<String>,
    },

    // Control
    Interrupted {
        turn_id: String,
        reason: InterruptReason,
    },
    RateLimited {
        reset_ms: u64,
        details: HashMap<String, Value>,
    },
    Error {
        error: String,
        recoverable: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCloseReason {
    Client,
    Server,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptReason {
    BargeIn,
    Manual,
    Error,
}
