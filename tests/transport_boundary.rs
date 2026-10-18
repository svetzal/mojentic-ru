//! Regression acceptance at the public request/response boundary.
use async_trait::async_trait;
use futures::StreamExt;
use mojentic::llm::{
    gateway::{CompletionConfig, StreamChunk},
    gateways::OllamaGateway,
    transport::HttpTransport,
    LlmGateway, LlmMessage,
};
use std::sync::{Arc, Mutex};

struct Interrupted {
    requests: Mutex<Vec<reqwest::Request>>,
}
#[async_trait]
impl HttpTransport for Interrupted {
    async fn execute(&self, request: reqwest::Request) -> reqwest::Result<reqwest::Response> {
        self.requests.lock().expect("capture").push(request);
        let contract: serde_json::Value =
            serde_json::from_str(include_str!("contracts/stream_timing_v1.json"))
                .expect("contract");
        let body = contract["transport_interruption"]["providers"]["ollama"][0]["body"]
            .as_str()
            .expect("contract body")
            .to_owned();
        let chunks = futures::stream::iter(vec![
            Ok(body),
            Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "contract interruption")),
        ]);
        Ok(http::Response::builder()
            .status(200)
            .body(reqwest::Body::wrap_stream(chunks))
            .expect("response")
            .into())
    }
}
#[tokio::test]
async fn interrupted_public_stream_preserves_request_and_terminal_error() {
    let transport = Arc::new(Interrupted {
        requests: Mutex::new(vec![]),
    });
    let gateway =
        OllamaGateway::with_host("http://contract.invalid").with_transport(transport.clone());
    let messages = [LlmMessage::user("question")];
    let config = CompletionConfig::default();
    let events: Vec<_> =
        gateway.complete_stream("gpt-4o", &messages, None, &config).collect().await;
    let requests = transport.requests.lock().expect("requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(requests[0].url().path(), "/api/chat");
    let body: serde_json::Value =
        serde_json::from_slice(requests[0].body().expect("body").as_bytes().expect("bytes"))
            .expect("json");
    assert_eq!(body["model"], "gpt-4o");
    assert_eq!(body["messages"][0]["content"], "question");
    assert_eq!(body["stream"], true);
    assert!(matches!(events.last(), Some(Err(mojentic::error::MojenticError::HttpError(_)))));
    assert!(matches!(events.get(events.len() - 2), Some(Ok(StreamChunk::Metrics(_)))));
}
