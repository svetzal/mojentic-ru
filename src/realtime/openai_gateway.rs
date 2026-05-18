//! OpenAI Realtime API gateway.

use crate::realtime::config::RealtimeVoiceConfig;
use crate::realtime::gateway::{RealtimeGatewaySession, RealtimeVoiceGateway};
use crate::realtime::schemas::parse_server_event;
use crate::realtime::transport::{RealtimeTransport, TransportError, WebSocketTransport};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

const DEFAULT_URL: &str = "wss://api.openai.com/v1/realtime";

#[derive(Debug, Clone, Default)]
pub struct OpenAIRealtimeGatewayOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

pub struct OpenAIRealtimeGateway {
    api_key: String,
    base_url: String,
}

impl OpenAIRealtimeGateway {
    pub fn new(options: OpenAIRealtimeGatewayOptions) -> Result<Self, &'static str> {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or("OpenAIRealtimeGateway requires api_key or OPENAI_API_KEY env var")?;
        Ok(Self {
            api_key,
            base_url: options.base_url.unwrap_or_else(|| DEFAULT_URL.to_string()),
        })
    }
}

#[async_trait]
impl RealtimeVoiceGateway for OpenAIRealtimeGateway {
    async fn open(
        &self,
        model: &str,
        _config: &RealtimeVoiceConfig,
        correlation_id: Option<&str>,
    ) -> Result<Box<dyn RealtimeGatewaySession>, TransportError> {
        use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
        let url =
            format!("{}?model={}", self.base_url, utf8_percent_encode(model, NON_ALPHANUMERIC));

        let mut headers = vec![
            ("authorization".to_string(), format!("Bearer {}", self.api_key)),
            ("openai-beta".to_string(), "realtime=v1".to_string()),
        ];
        if let Some(cid) = correlation_id {
            headers.push(("x-correlation-id".to_string(), cid.to_string()));
        }

        let transport = WebSocketTransport::connect(&url, &headers).await?;

        Ok(Box::new(OpenAISession {
            session_id: Uuid::new_v4().to_string(),
            transport: Box::new(transport),
            closed: false,
        }))
    }
}

struct OpenAISession {
    session_id: String,
    transport: Box<dyn RealtimeTransport>,
    closed: bool,
}

#[async_trait]
impl RealtimeGatewaySession for OpenAISession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send_event(&mut self, event: &Value) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError("session closed".to_string()));
        }
        let payload = serde_json::to_string(event)
            .map_err(|e| TransportError(format!("encode error: {e}")))?;
        self.transport.send(&payload).await
    }

    async fn next_event(&mut self) -> Option<Value> {
        let frame = self.transport.next_frame().await?;
        match frame {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(raw) => Some(parse_server_event(raw)),
                Err(err) => Some(serde_json::json!({
                    "type": "error",
                    "error": { "type": "parse_error", "message": err.to_string() }
                })),
            },
            Err(err) => Some(serde_json::json!({
                "type": "error",
                "error": { "type": "transport_error", "message": err.to_string() }
            })),
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.transport.close().await
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}
