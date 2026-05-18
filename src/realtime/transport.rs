//! Transport abstraction for the realtime subsystem.
//!
//! The gateway owns I/O against this interface so unit tests can swap
//! the live WebSocket for a scripted, deterministic transport.

use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc;

/// A duplex text-frame channel — minimal surface so providers can swap
/// WebSocket for SSE, in-process loopback, etc.
#[async_trait]
pub trait RealtimeTransport: Send + Sync {
    async fn send(&mut self, payload: &str) -> Result<(), TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
    /// Returns the next text frame, or None on close.
    async fn next_frame(&mut self) -> Option<Result<String, TransportError>>;
}

#[derive(Debug, Clone)]
pub struct TransportError(pub String);

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TransportError {}

/// Production transport — wraps `tokio_tungstenite` and pumps frames in
/// a background task whose output flows through an mpsc receiver.
pub struct WebSocketTransport {
    outbound: mpsc::Sender<String>,
    inbound: mpsc::Receiver<Result<String, TransportError>>,
    close_signal: tokio::sync::watch::Sender<bool>,
}

impl WebSocketTransport {
    /// Connect to a WebSocket URL with optional headers. Spawns a
    /// pump task that lives until the inbound channel is dropped or
    /// the connection closes.
    pub async fn connect(url: &str, headers: &[(String, String)]) -> Result<Self, TransportError> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http;

        let mut req = url
            .into_client_request()
            .map_err(|e| TransportError(format!("invalid url: {e}")))?;
        for (k, v) in headers {
            let header_name = http::header::HeaderName::from_bytes(k.as_bytes())
                .map_err(|e| TransportError(format!("bad header name: {e}")))?;
            let header_value = http::HeaderValue::from_str(v)
                .map_err(|e| TransportError(format!("bad header value: {e}")))?;
            req.headers_mut().insert(header_name, header_value);
        }

        let (ws, _resp) = tokio_tungstenite::connect_async(req)
            .await
            .map_err(|e| TransportError(format!("ws connect failed: {e}")))?;

        let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(64);
        let (inbound_tx, inbound_rx) = mpsc::channel(64);
        let (close_tx, mut close_rx) = tokio::sync::watch::channel(false);

        use futures::stream::StreamExt;
        use futures::SinkExt;

        let (mut sink, mut stream) = ws.split();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(out) = outbound_rx.recv() => {
                        if let Err(err) = sink.send(tokio_tungstenite::tungstenite::Message::Text(out)).await {
                            let _ = inbound_tx.send(Err(TransportError(format!("ws send: {err}")))).await;
                            break;
                        }
                    }
                    maybe = stream.next() => {
                        match maybe {
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                                if inbound_tx.send(Ok(text.to_string())).await.is_err() { break; }
                            }
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => break,
                            Some(Ok(_)) => continue,
                            Some(Err(err)) => {
                                let _ = inbound_tx.send(Err(TransportError(format!("ws recv: {err}")))).await;
                                break;
                            }
                            None => break,
                        }
                    }
                    _ = close_rx.changed() => {
                        if *close_rx.borrow() { break; }
                    }
                }
            }
            let _ = sink.close().await;
        });

        Ok(Self {
            outbound: outbound_tx,
            inbound: inbound_rx,
            close_signal: close_tx,
        })
    }
}

#[async_trait]
impl RealtimeTransport for WebSocketTransport {
    async fn send(&mut self, payload: &str) -> Result<(), TransportError> {
        self.outbound
            .send(payload.to_string())
            .await
            .map_err(|e| TransportError(format!("ws outbound: {e}")))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let _ = self.close_signal.send(true);
        Ok(())
    }

    async fn next_frame(&mut self) -> Option<Result<String, TransportError>> {
        self.inbound.recv().await
    }
}
