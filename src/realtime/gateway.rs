//! Realtime voice gateway trait.

use crate::realtime::config::RealtimeVoiceConfig;
use crate::realtime::transport::TransportError;
use async_trait::async_trait;
use serde_json::Value;

pub type ClientRealtimeEvent = Value;
pub type ServerRealtimeEvent = Value;

#[async_trait]
pub trait RealtimeGatewaySession: Send + Sync {
    fn session_id(&self) -> &str;
    async fn send_event(&mut self, event: &Value) -> Result<(), TransportError>;
    async fn next_event(&mut self) -> Option<Value>;
    async fn close(&mut self) -> Result<(), TransportError>;
    fn is_closed(&self) -> bool;
}

#[async_trait]
pub trait RealtimeVoiceGateway: Send + Sync {
    async fn open(
        &self,
        model: &str,
        config: &RealtimeVoiceConfig,
        correlation_id: Option<&str>,
    ) -> Result<Box<dyn RealtimeGatewaySession>, TransportError>;
}
