//! Realtime voice subsystem.
//!
//! Sibling to [`crate::llm`] — exposes [`RealtimeVoiceBroker`] that opens
//! duplex voice + tool sessions against a realtime-capable provider
//! (currently OpenAI's Realtime API over WebSocket). Mirrors the Python
//! and TypeScript implementations so consumers see the same conceptual
//! API regardless of language.

pub mod broker;
pub mod codec;
pub mod config;
pub mod events;
pub mod gateway;
pub mod openai_gateway;
pub mod schemas;
pub mod session;
pub mod transport;

pub use broker::RealtimeVoiceBroker;
pub use codec::{decode_base64_pcm16, encode_base64_pcm16};
pub use config::{
    InterruptOutputPolicy, RealtimeAudioFormat, RealtimeModality, RealtimeToolChoice,
    RealtimeVoiceConfig, SemanticVadConfig, ServerVadConfig, TurnDetectionMode,
};
pub use events::{RealtimeEvent, RealtimeItem, TokenUsage};
pub use gateway::{
    ClientRealtimeEvent, RealtimeGatewaySession, RealtimeVoiceGateway, ServerRealtimeEvent,
};
pub use openai_gateway::{OpenAIRealtimeGateway, OpenAIRealtimeGatewayOptions};
pub use session::{build_session_update, RealtimeSession};
pub use transport::{RealtimeTransport, WebSocketTransport};
