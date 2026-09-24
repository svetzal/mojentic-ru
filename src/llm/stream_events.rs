//! Single-turn streaming with terminal completion evidence.
//!
//! [`crate::llm::LlmBroker::generate_stream_events`] yields [`StreamEvent`]s for
//! one model turn. Every stream ends with exactly one terminal event:
//! [`StreamEvent::Completed`] when the provider reported a clean stop, or
//! [`StreamEvent::Error`] otherwise. Nothing follows the terminal event.
//!
//! Content yielded before an error is evidence of what the provider sent, not
//! a result. Truncated or unfinished output is always an error.

use crate::error::MojenticError;
use crate::llm::models::ResponseEvidence;
use futures::stream::{Stream, StreamExt};
use serde_json::Value;
use std::pin::Pin;
use thiserror::Error;

/// A boxed stream of [`StreamEvent`]s for one model turn.
pub type StreamEventStream<'a> = Pin<Box<dyn Stream<Item = StreamEvent> + Send + 'a>>;

/// One event in a single-turn event stream.
#[derive(Debug)]
#[non_exhaustive]
pub enum StreamEvent {
    /// Visible assistant content, in the order the provider sent it.
    Content(String),
    /// Terminal success: the provider finished with `stop` and closed the stream.
    ///
    /// The evidence holds the finish reason, usage and provider model when
    /// reported, plus other provider metadata such as Ollama durations.
    Completed(ResponseEvidence),
    /// Terminal failure. Content yielded before it is not a complete result.
    Error(StreamEventError),
}

impl StreamEvent {
    /// Whether this event ends the stream.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, StreamEvent::Content(_))
    }
}

/// Why a single-turn event stream ended without a completed response.
///
/// The variants follow the cross-port reason vocabulary:
/// `incomplete_completion`, `incomplete_stream`, `provider_error`,
/// `unexpected_tool_calls`, `invalid_stream_event`,
/// `stream_events_unsupported` and `request_failed`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StreamEventError {
    /// The provider finished for a reason other than `stop`, for example
    /// `length`. Carries the finish reason, usage, provider model and metadata.
    ///
    /// Ollama servers too old to send `done_reason` always end here.
    #[error("incomplete completion (finish reason: {})", .0.finish_reason.as_deref().unwrap_or("unknown"))]
    IncompleteCompletion(Box<ResponseEvidence>),

    /// The stream ended without the provider's terminal marker. Carries the
    /// evidence that arrived before the end (such as model and usage), or
    /// `None` when none did.
    #[error("stream ended without a terminal marker")]
    IncompleteStream(Option<Box<ResponseEvidence>>),

    /// The provider reported an error, either as an error frame in the stream
    /// or as a non-success HTTP status.
    #[error("provider error{}: {error}", .status.map(|code| format!(" (HTTP {code})")).unwrap_or_default())]
    ProviderError {
        /// HTTP status, when the provider rejected the request with one.
        status: Option<u16>,
        /// The provider's error value: the frame's `error` field, or the
        /// response body (its `error` field when it has one).
        error: Value,
    },

    /// The provider streamed a native tool call. The events API supplies no
    /// tools and executes none.
    #[error("unexpected tool calls in stream")]
    UnexpectedToolCalls,

    /// A frame could not be read as a provider stream event.
    #[error("invalid stream event: {0}")]
    InvalidStreamEvent(String),

    /// The gateway does not implement the events API. No request was sent.
    #[error("stream events are not supported by this gateway")]
    StreamEventsUnsupported,

    /// Building the request, connecting, or reading the response body failed.
    #[error("stream request failed: {0}")]
    RequestFailed(#[source] MojenticError),
}

impl StreamEventError {
    /// A provider error for an error frame inside the stream.
    pub(crate) fn provider_frame(error: Value) -> Self {
        StreamEventError::ProviderError {
            status: None,
            error,
        }
    }

    /// A provider error for a non-success HTTP response.
    fn provider_status(status: u16, body: &str) -> Self {
        let error = match serde_json::from_str::<Value>(body) {
            Ok(Value::Object(mut fields)) if fields.contains_key("error") => {
                fields.remove("error").unwrap_or(Value::Null)
            }
            Ok(value) => value,
            Err(_) => Value::String(body.to_string()),
        };
        StreamEventError::ProviderError {
            status: Some(status),
            error,
        }
    }
}

/// Turns one provider stream line into zero or more events.
///
/// A parser owns the provider's completion rules. It returns a terminal event
/// when the line settles the outcome; the driver stops reading after it.
pub(crate) trait FrameParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent>;

    /// Evidence seen so far, for a stream that ends without a terminal marker.
    fn partial_evidence(&self) -> Option<ResponseEvidence>;
}

/// Send one streaming request and translate its lines into events.
///
/// The returned stream owns the HTTP response. Dropping the stream drops the
/// response body, which closes the connection and cancels the request.
pub(crate) fn drive_event_stream<'a, P>(
    request: reqwest::RequestBuilder,
    mut parser: P,
) -> StreamEventStream<'a>
where
    P: FrameParser + Send + 'a,
{
    Box::pin(async_stream::stream! {
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                yield StreamEvent::Error(StreamEventError::RequestFailed(error.into()));
                return;
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            yield StreamEvent::Error(StreamEventError::provider_status(status.as_u16(), &body));
            return;
        }

        let mut bytes = response.bytes_stream();
        let mut lines = LineBuffer::default();
        while let Some(chunk) = bytes.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield StreamEvent::Error(StreamEventError::RequestFailed(error.into()));
                    return;
                }
            };
            for line in lines.push(&chunk) {
                let events = match std::str::from_utf8(&line) {
                    Ok(text) => parser.parse_line(text.trim_end_matches('\r')),
                    Err(_) => vec![invalid_event("frame is not valid UTF-8")],
                };
                for event in events {
                    let terminal = event.is_terminal();
                    yield event;
                    if terminal {
                        return;
                    }
                }
            }
        }

        yield StreamEvent::Error(StreamEventError::IncompleteStream(
            parser.partial_evidence().map(Box::new),
        ));
    })
}

/// An [`StreamEvent::Error`] for a frame that is not a valid provider event.
pub(crate) fn invalid_event(reason: impl Into<String>) -> StreamEvent {
    StreamEvent::Error(StreamEventError::InvalidStreamEvent(reason.into()))
}

/// Splits a byte stream into complete newline-terminated lines.
///
/// Bytes after the last newline wait for the next chunk, so a frame split
/// across network reads (including inside a UTF-8 sequence) stays intact.
/// A trailing partial line at end of stream is never returned: a stream cut
/// mid-frame is incomplete, not malformed.
#[derive(Default)]
struct LineBuffer {
    pending: Vec<u8>,
}

impl LineBuffer {
    fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.pending.extend_from_slice(chunk);
        let Some(last_newline) = self.pending.iter().rposition(|byte| *byte == b'\n') else {
            return Vec::new();
        };
        let rest = self.pending.split_off(last_newline + 1);
        let complete = std::mem::replace(&mut self.pending, rest);
        complete[..complete.len() - 1]
            .split(|byte| *byte == b'\n')
            .map(<[u8]>::to_vec)
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A local HTTP server that streams forever, for cancellation tests.

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    /// Serve one chunked response that sends `first_frame` and then never ends.
    ///
    /// Returns the base URL and a receiver that resolves when the client
    /// closes the connection.
    pub(crate) async fn endless_stream_server(
        first_frame: &'static str,
    ) -> (String, oneshot::Receiver<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let (closed_tx, closed_rx) = oneshot::channel();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept client");
            read_request(&mut socket).await;
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n";
            let chunk = format!("{:x}\r\n{}\r\n", first_frame.len(), first_frame);
            socket.write_all(head.as_bytes()).await.expect("write head");
            socket.write_all(chunk.as_bytes()).await.expect("write first frame");

            // The response never ends. A read of zero bytes means the client hung up.
            let mut buffer = [0u8; 64];
            while let Ok(read) = socket.read(&mut buffer).await {
                if read == 0 {
                    break;
                }
            }
            let _ = closed_tx.send(());
        });

        (format!("http://{address}"), closed_rx)
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) {
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = socket.read(&mut buffer).await.expect("read request");
            request.extend_from_slice(&buffer[..read]);
            let text = String::from_utf8_lossy(&request);
            if let Some(head_end) = text.find("\r\n\r\n") {
                let length = text[..head_end]
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if request.len() >= head_end + 4 + length {
                    return;
                }
            }
            if read == 0 {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_buffer_returns_only_complete_lines() {
        let mut lines = LineBuffer::default();

        assert!(lines.push(b"data: {\"a\"").is_empty());
        assert_eq!(lines.push(b":1}\n\ndata: [DO"), vec![b"data: {\"a\":1}".to_vec(), vec![]]);
        assert_eq!(lines.push(b"NE]\n"), vec![b"data: [DONE]".to_vec()]);
    }

    #[test]
    fn line_buffer_keeps_utf8_sequences_split_across_chunks() {
        let mut lines = LineBuffer::default();
        let text = "héllo\n".as_bytes();

        assert!(lines.push(&text[..2]).is_empty());
        let complete = lines.push(&text[2..]);

        assert_eq!(complete, vec!["héllo".as_bytes().to_vec()]);
    }

    #[test]
    fn only_content_is_non_terminal() {
        assert!(!StreamEvent::Content("x".into()).is_terminal());
        assert!(StreamEvent::Completed(ResponseEvidence::default()).is_terminal());
        assert!(StreamEvent::Error(StreamEventError::IncompleteStream(None)).is_terminal());
    }

    #[test]
    fn http_provider_error_keeps_status_and_error_field() {
        let error = StreamEventError::provider_status(429, r#"{"error":{"message":"slow"}}"#);

        assert!(matches!(
            &error,
            StreamEventError::ProviderError { status: Some(429), error } if error["message"] == "slow"
        ));
        assert!(error.to_string().starts_with("provider error (HTTP 429)"));
    }

    #[test]
    fn http_provider_error_keeps_a_non_json_body_as_text() {
        let error = StreamEventError::provider_status(502, "Bad Gateway");

        assert!(matches!(
            error,
            StreamEventError::ProviderError { status: Some(502), error } if error == "Bad Gateway"
        ));
    }

    #[test]
    fn incomplete_completion_names_its_finish_reason() {
        let error = StreamEventError::IncompleteCompletion(Box::new(ResponseEvidence {
            finish_reason: Some("length".into()),
            ..Default::default()
        }));

        assert_eq!(error.to_string(), "incomplete completion (finish reason: length)");
    }
}
