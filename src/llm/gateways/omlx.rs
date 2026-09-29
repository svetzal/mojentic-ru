//! Gateway for [oMLX](https://github.com/jundot/omlx), an LLM server for Apple Silicon.
//!
//! oMLX speaks the OpenAI chat completions protocol, but this gateway does not
//! treat it as OpenAI. It sends every setting as configured, with no per-model
//! parameter adaptation and no OpenAI model registry. It maps the reasoning
//! trace oMLX reports (`reasoning_content`) to the response's `thinking`, and
//! it sends embeddings in one request with no client-side chunking.
//!
//! It reuses the OpenAI message adapter and the OpenAI stream parsers.

use crate::error::{MojenticError, Result};
use crate::llm::gateway::{
    CompletionConfig, LlmGateway, ReasoningEffort, ResponseFormat, StreamChunk,
};
use crate::llm::gateways::openai::{add_response_format, openai_response_evidence};
use crate::llm::gateways::openai_legacy_stream::{
    legacy_body_stream, read_legacy_line, LegacyLine, OpenAiLegacyParser,
};
use crate::llm::gateways::openai_messages_adapter::{adapt_messages_to_openai, convert_tool_calls};
use crate::llm::gateways::openai_stream_events::OpenAiEventParser;
use crate::llm::models::{LlmGatewayResponse, LlmMessage, ResponseEvidence};
use crate::llm::stream_events::{
    drive_event_stream, FrameParser, StreamEvent, StreamEventError, StreamEventStream,
};
use crate::llm::tools::LlmTool;
use async_trait::async_trait;
use futures::stream::Stream;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::Value;
use std::pin::Pin;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Response metadata key for the `Warning` header oMLX sends when it could
/// not enforce a requested structured output format.
///
/// When oMLX cannot compile a grammar for `json_object` or `json_schema`, it
/// falls back to instructions in the prompt and says so in a `Warning`
/// header. The gateway records the header value here and logs a warning. It
/// does not retry or fail: callers validate content as they would anyway.
pub const RESPONSE_FORMAT_WARNING: &str = "response_format_warning";

/// Host used when neither the configuration nor `OMLX_HOST` sets one.
const DEFAULT_HOST: &str = "http://localhost:8000";

/// Timeout used when `OMLX_TIMEOUT` is unset or unreadable: ten minutes.
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(600_000);

/// The `model` oMLX reports on keep-alive stream frames.
const KEEPALIVE_MODEL: &str = "keepalive";

/// Characters left as they are in a model id path segment (RFC 3986 unreserved).
const PATH_SEGMENT: &AsciiSet =
    &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// Configuration for connecting to an oMLX server.
///
/// [`Default`] reads the environment, so explicit fields take precedence over
/// the environment, which takes precedence over the built-in defaults:
///
/// | Field | Environment | Default |
/// | ----- | ----------- | ------- |
/// | `host` | `OMLX_HOST` | `http://localhost:8000` |
/// | `api_key` | `OMLX_API_KEY` | none |
/// | `timeout` | `OMLX_TIMEOUT`, in milliseconds | 10 minutes |
///
/// ```no_run
/// use mojentic::llm::gateways::{OmlxConfig, OmlxGateway};
///
/// let gateway = OmlxGateway::with_config(OmlxConfig {
///     host: "http://studio.local:8000".to_string(),
///     ..Default::default()
/// });
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct OmlxConfig {
    /// Server address without the `/v1` suffix. The gateway adds `/v1`.
    pub host: String,
    /// Sent as `Authorization: Bearer <key>`. No header is sent without one.
    pub api_key: Option<String>,
    /// Applies to every request, including model load and whole streams.
    /// `None` means no timeout.
    pub timeout: Option<Duration>,
}

/// Shows whether a key is set, never the key itself.
impl std::fmt::Debug for OmlxConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OmlxConfig")
            .field("host", &self.host)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl Default for OmlxConfig {
    fn default() -> Self {
        Self {
            host: std::env::var("OMLX_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string()),
            api_key: std::env::var("OMLX_API_KEY").ok().filter(|key| !key.is_empty()),
            timeout: Some(timeout_from_env(std::env::var("OMLX_TIMEOUT").ok())),
        }
    }
}

/// Read an `OMLX_TIMEOUT` value in milliseconds, falling back to the default.
fn timeout_from_env(value: Option<String>) -> Duration {
    let Some(value) = value else {
        return DEFAULT_TIMEOUT;
    };
    match value.trim().parse::<u64>() {
        Ok(millis) => Duration::from_millis(millis),
        Err(_) => {
            warn!(value = %value, "OMLX_TIMEOUT is not a number of milliseconds, using the default");
            DEFAULT_TIMEOUT
        }
    }
}

/// Gateway for a local [oMLX](https://github.com/jundot/omlx) server.
///
/// Supports chat, structured output, tool calls, both streaming APIs,
/// embeddings, and loading and unloading models.
///
/// # Thinking
///
/// oMLX reports a model's reasoning trace as `reasoning_content`. The gateway
/// maps it to [`LlmGatewayResponse::thinking`] and, in the legacy streaming
/// API, to [`StreamChunk::Thinking`]. The events API has no thinking event
/// and yields none.
///
/// `reasoning_effort` of low, medium or high is sent unchanged to the model's
/// chat template as `"low"`, `"medium"` or `"high"`. Its effect depends on the
/// model. [`ReasoningEffort::Disabled`] sends `enable_thinking: false` and no
/// `reasoning_effort`, which turns thinking off. Leaving it unset keeps the
/// model's default; Qwen 3 models think by default.
///
/// # Truncation
///
/// When `max_tokens` ends generation during thinking, a non-streaming response
/// carries the partial reasoning in `content` and no thinking, with a finish
/// reason of `length`. A streaming response keeps it as thinking. The gateway
/// maps the fields as oMLX reports them. When the finish reason in
/// [`LlmGatewayResponse::evidence`] is not `stop`, `content` is not an answer.
///
/// # Example
///
/// ```no_run
/// use mojentic::llm::gateways::OmlxGateway;
/// use mojentic::llm::{LlmBroker, LlmMessage};
/// use std::sync::Arc;
///
/// # async fn run() -> mojentic::Result<()> {
/// let broker = LlmBroker::new("Qwen3.8-27B-MLX-8bit", Arc::new(OmlxGateway::new()), None);
/// let reply = broker.generate(&[LlmMessage::user("Hello")], None, None, None).await?;
/// # Ok(())
/// # }
/// ```
pub struct OmlxGateway {
    client: Client,
    config: OmlxConfig,
}

impl OmlxGateway {
    /// Create a gateway configured from the environment. See [`OmlxConfig`].
    pub fn new() -> Self {
        Self::with_config(OmlxConfig::default())
    }

    /// Create a gateway with explicit configuration.
    pub fn with_config(config: OmlxConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Create a gateway for `host`, reading the key and timeout from the environment.
    pub fn with_host(host: impl Into<String>) -> Self {
        Self::with_config(OmlxConfig {
            host: host.into(),
            ..Default::default()
        })
    }

    /// Load a model into memory ahead of its first request.
    ///
    /// Blocks until the model is in memory. A chat request loads its model on
    /// its own, so this is only for warming up. oMLX downloads models only
    /// through its admin dashboard; there is no pull operation.
    ///
    /// # Errors
    ///
    /// [`MojenticError::GatewayError`] with the HTTP status and response body
    /// when oMLX rejects the request, or [`MojenticError::HttpError`] when the
    /// request fails.
    pub async fn load_model(&self, model: &str) -> Result<()> {
        info!(model, "Loading oMLX model");
        self.model_action(model, "load").await
    }

    /// Unload a model from memory.
    ///
    /// # Errors
    ///
    /// [`MojenticError::GatewayError`] with the HTTP status and response body
    /// when oMLX rejects the request. Unloading a model that is not loaded is
    /// a 400 `invalid_request_error`. [`MojenticError::HttpError`] when the
    /// request fails.
    pub async fn unload_model(&self, model: &str) -> Result<()> {
        info!(model, "Unloading oMLX model");
        self.model_action(model, "unload").await
    }

    async fn model_action(&self, model: &str, action: &str) -> Result<()> {
        let model = utf8_percent_encode(model, PATH_SEGMENT);
        let response =
            self.request(Method::POST, &format!("/models/{model}/{action}")).send().await?;
        success(response).await?;
        Ok(())
    }

    /// A request to `/v1{path}` with authorization and timeout applied.
    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}/v1{}", self.config.host.trim_end_matches('/'), path);
        let mut request = self.client.request(method, url);
        if let Some(key) = &self.config.api_key {
            request = request.bearer_auth(key);
        }
        if let Some(timeout) = self.config.timeout {
            request = request.timeout(timeout);
        }
        request
    }

    fn chat_request(&self, body: &Value) -> RequestBuilder {
        self.request(Method::POST, "/chat/completions").json(body)
    }

    /// Send a non-streaming chat request and read the response.
    async fn chat(&self, body: &Value, structured: bool) -> Result<ChatReply> {
        let response = success(self.chat_request(body).send().await?).await?;
        let warning = structured.then(|| format_warning(&response)).flatten();
        let body: Value = response.json().await?;
        let mut evidence = openai_response_evidence(&body);
        if let Some(warning) = warning {
            warn!(warning = %warning, "oMLX did not enforce the requested response format");
            evidence
                .metadata
                .insert(RESPONSE_FORMAT_WARNING.to_string(), Value::String(warning));
        }
        Ok(ChatReply { body, evidence })
    }
}

impl Default for OmlxGateway {
    fn default() -> Self {
        Self::new()
    }
}

/// A chat completion body and the evidence read from it and its headers.
struct ChatReply {
    body: Value,
    evidence: ResponseEvidence,
}

impl ChatReply {
    fn message(&self) -> &Value {
        &self.body["choices"][0]["message"]
    }

    fn text(&self, field: &str) -> Option<String> {
        self.message()[field].as_str().map(String::from)
    }
}

/// The response when its status is a success, otherwise a gateway error
/// carrying the status and the body as oMLX sent it.
async fn success(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(MojenticError::GatewayError(format!("oMLX API error: {status} - {body}")))
}

/// Every `Warning` header value, joined with `, `.
fn format_warning(response: &Response) -> Option<String> {
    let values: Vec<_> = response
        .headers()
        .get_all(reqwest::header::WARNING)
        .iter()
        .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
        .collect();
    (!values.is_empty()).then(|| values.join(", "))
}

/// Whether the configuration asks for structured output.
fn requests_structured_output(config: &CompletionConfig) -> bool {
    matches!(config.response_format, Some(ResponseFormat::JsonObject { .. }))
}

/// Build a chat completion body from the configuration, unadapted.
///
/// `num_ctx` and `num_predict` are not sent: oMLX sets context length per model.
fn chat_body(model: &str, messages: &[LlmMessage], config: &CompletionConfig) -> Result<Value> {
    let mut body = serde_json::json!({
        "model": model,
        "messages": adapt_messages_to_openai(messages)?,
        "temperature": config.temperature,
        "max_tokens": config.max_tokens,
    });
    if let Some(top_p) = config.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(top_k) = config.top_k {
        body["top_k"] = serde_json::json!(top_k);
    }
    match config.reasoning_effort {
        // oMLX turns thinking off with its own flag, not an effort level.
        Some(ReasoningEffort::Disabled) => body["enable_thinking"] = Value::Bool(false),
        Some(effort) => body["reasoning_effort"] = serde_json::to_value(effort)?,
        None => {}
    }
    add_response_format(&mut body, config);
    Ok(body)
}

fn tool_definitions(tools: &[Box<dyn LlmTool>]) -> Result<Value> {
    let descriptors: Vec<_> = tools.iter().map(|tool| tool.descriptor()).collect();
    Ok(serde_json::to_value(descriptors)?)
}

/// Whether a stream frame is one of oMLX's keep-alive frames.
fn is_keepalive(frame: &Value) -> bool {
    frame["model"] == KEEPALIVE_MODEL
}

/// Whether an SSE line is a `data:` line carrying a keep-alive frame.
fn is_keepalive_line(line: &str) -> bool {
    // Most lines are not keep-alives; skip the JSON parse for them.
    if !line.contains(KEEPALIVE_MODEL) {
        return false;
    }
    line.strip_prefix("data:")
        .and_then(|data| serde_json::from_str::<Value>(data.trim_start()).ok())
        .is_some_and(|frame| is_keepalive(&frame))
}

/// The OpenAI event parser, behind a filter that drops keep-alive frames so
/// their `keepalive` model never enters the completion evidence.
#[derive(Default)]
struct OmlxEventParser {
    inner: OpenAiEventParser,
}

impl FrameParser for OmlxEventParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        if is_keepalive_line(line) {
            return Vec::new();
        }
        self.inner.parse_line(line)
    }

    fn partial_evidence(&self) -> Option<ResponseEvidence> {
        self.inner.partial_evidence()
    }
}

/// Legacy stream chunks for one line: keep-alives dropped, reasoning as thinking.
fn legacy_chunks(parser: &mut OpenAiLegacyParser, line: &str) -> Vec<StreamChunk> {
    match read_legacy_line(line) {
        Some(LegacyLine::Done) => parser.done(),
        Some(LegacyLine::Frame(frame)) if is_keepalive(&frame) => Vec::new(),
        Some(LegacyLine::Frame(frame)) => {
            let reasoning = frame["choices"][0]["delta"]["reasoning_content"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(|text| StreamChunk::Thinking(text.to_string()));
            reasoning.into_iter().chain(parser.frame(&frame)).collect()
        }
        None => Vec::new(),
    }
}

#[async_trait]
impl LlmGateway for OmlxGateway {
    async fn complete(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: Option<&[Box<dyn LlmTool>]>,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse> {
        info!("Delegating to oMLX for completion");
        debug!("Model: {}, Message count: {}", model, messages.len());

        let mut body = chat_body(model, messages, config)?;
        if let Some(tools) = tools {
            body["tools"] = tool_definitions(tools)?;
        }

        let reply = self.chat(&body, requests_structured_output(config)).await?;
        let tool_calls =
            reply.message()["tool_calls"].as_array().map(|calls| convert_tool_calls(calls));

        Ok(LlmGatewayResponse {
            content: reply.text("content"),
            object: None,
            tool_calls: tool_calls.unwrap_or_default(),
            thinking: reply.text("reasoning_content"),
            evidence: reply.evidence,
        })
    }

    async fn complete_json(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<Value> {
        self.complete_json_response(model, messages, schema, config)
            .await?
            .object
            .ok_or_else(|| MojenticError::GatewayError("No content in response".to_string()))
    }

    async fn complete_json_response(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: Value,
        config: &CompletionConfig,
    ) -> Result<LlmGatewayResponse<Value>> {
        info!("Requesting structured output from oMLX");

        let config = CompletionConfig {
            response_format: Some(ResponseFormat::JsonObject {
                schema: Some(schema),
            }),
            ..config.clone()
        };
        let body = chat_body(model, messages, &config)?;

        let reply = self.chat(&body, true).await?;
        let content = reply
            .text("content")
            .ok_or_else(|| MojenticError::GatewayError("No content in response".to_string()))?;
        let object: Value = serde_json::from_str(&content)?;

        Ok(LlmGatewayResponse {
            content: Some(content),
            object: Some(object),
            tool_calls: Vec::new(),
            thinking: reply.text("reasoning_content"),
            evidence: reply.evidence,
        })
    }

    async fn get_available_models(&self) -> Result<Vec<String>> {
        debug!("Fetching available oMLX models");

        let response = success(self.request(Method::GET, "/models").send().await?).await?;
        let body: Value = response.json().await?;

        let mut models: Vec<String> = body["data"]
            .as_array()
            .ok_or_else(|| MojenticError::GatewayError("Invalid response format".to_string()))?
            .iter()
            .filter_map(|model| model["id"].as_str().map(String::from))
            .collect();
        models.sort();
        Ok(models)
    }

    /// Embed `text` in one request.
    ///
    /// oMLX has no standard embedding model, so `model` is required. There is
    /// no client-side chunking; the server applies its model's input limit.
    ///
    /// # Errors
    ///
    /// [`MojenticError::InvalidArgument`] without sending a request when
    /// `model` is missing or blank. [`MojenticError::GatewayError`] with the
    /// status and body when oMLX rejects the request, for example a 400 for a
    /// model that is not an embedding model.
    async fn calculate_embeddings(&self, text: &str, model: Option<&str>) -> Result<Vec<f32>> {
        let model = model.filter(|model| !model.trim().is_empty()).ok_or_else(|| {
            MojenticError::InvalidArgument(
                "oMLX embeddings need a model: oMLX has no default embedding model".to_string(),
            )
        })?;
        debug!("Calculating embeddings with model: {}", model);

        let body = serde_json::json!({"model": model, "input": text});
        let response =
            success(self.request(Method::POST, "/embeddings").json(&body).send().await?).await?;
        let body: Value = response.json().await?;

        let embedding = body["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| MojenticError::GatewayError("Invalid embeddings response".to_string()))?
            .iter()
            .filter_map(|value| value.as_f64().map(|number| number as f32))
            .collect();
        Ok(embedding)
    }

    fn complete_stream<'a>(
        &'a self,
        model: &'a str,
        messages: &'a [LlmMessage],
        tools: Option<&'a [Box<dyn LlmTool>]>,
        config: &'a CompletionConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send + 'a>> {
        Box::pin(async_stream::stream! {
            info!("Starting oMLX streaming completion");
            debug!("Model: {}, Message count: {}", model, messages.len());

            let mut body = match chat_body(model, messages, config) {
                Ok(body) => body,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };
            body["stream"] = serde_json::json!(true);
            if let Some(tools) = tools {
                match tool_definitions(tools) {
                    Ok(definitions) => body["tools"] = definitions,
                    Err(e) => {
                        yield Err(e);
                        return;
                    }
                }
            }

            let response = match self.chat_request(&body).send().await {
                Ok(response) => response,
                Err(e) => {
                    yield Err(e.into());
                    return;
                }
            };
            let response = match success(response).await {
                Ok(response) => response,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            let mut parser = OpenAiLegacyParser::default();
            let chunks = legacy_body_stream(response.bytes_stream(), move |line: &str| {
                legacy_chunks(&mut parser, line)
            });
            for await chunk in chunks {
                yield chunk;
            }
        })
    }

    fn complete_stream_events<'a>(
        &'a self,
        model: &'a str,
        messages: &'a [LlmMessage],
        config: &'a CompletionConfig,
    ) -> std::result::Result<StreamEventStream<'a>, StreamEventError> {
        info!("Starting oMLX stream events");
        let mut body =
            chat_body(model, messages, config).map_err(StreamEventError::RequestFailed)?;
        body["stream"] = serde_json::json!(true);
        body["stream_options"] = serde_json::json!({"include_usage": true});

        Ok(drive_event_stream(self.chat_request(&body), OmlxEventParser::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[tokio::test]
    async fn legacy_stream_keeps_a_character_split_across_network_chunks() {
        use crate::llm::stream_events::testing::{
            split_body_server, split_inside_first_multibyte_char,
        };
        let body = concat!(
            "data: {\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"\u{1f914} hmm\"}}]}\n\n",
            "data: {\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"caf\u{e9}\"}}]}\n\n",
            "data: [DONE]\n\n",
        );
        let url = split_body_server(split_inside_first_multibyte_char(body)).await;
        let gateway = OmlxGateway::with_config(OmlxConfig {
            host: url,
            api_key: None,
            timeout: None,
        });
        let messages = vec![LlmMessage::user("Hi")];
        let config = CompletionConfig::default();

        let items: Vec<_> = gateway.complete_stream("m", &messages, None, &config).collect().await;

        assert!(
            matches!(
                items.as_slice(),
                [Ok(StreamChunk::Thinking(thinking)), Ok(StreamChunk::Content(content))]
                    if thinking == "\u{1f914} hmm" && content == "caf\u{e9}"
            ),
            "{items:?}"
        );
    }

    #[test]
    fn debug_output_hides_the_api_key() {
        let config = OmlxConfig {
            host: "http://localhost:8000".into(),
            api_key: Some("secret-key".into()),
            timeout: None,
        };

        let shown = format!("{config:?}");

        assert!(!shown.contains("secret-key"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn timeout_env_is_milliseconds_with_a_ten_minute_default() {
        assert_eq!(timeout_from_env(None), Duration::from_secs(600));
        assert_eq!(timeout_from_env(Some("2500".into())), Duration::from_millis(2500));
        assert_eq!(timeout_from_env(Some(" 1000 ".into())), Duration::from_secs(1));
        assert_eq!(timeout_from_env(Some("ten".into())), Duration::from_secs(600));
        assert_eq!(timeout_from_env(Some("-5".into())), Duration::from_secs(600));
    }

    #[test]
    fn a_keepalive_line_has_exactly_the_keepalive_model() {
        assert!(is_keepalive_line(r#"data: {"model":"keepalive","choices":[]}"#));
        assert!(is_keepalive_line(r#"data:{"model":"keepalive","choices":[]}"#));
        assert!(!is_keepalive_line(r#"data: {"model":"keepalive-7b","choices":[]}"#));
        assert!(!is_keepalive_line(
            r#"data: {"model":"m","choices":[{"delta":{"content":"keepalive"}}]}"#
        ));
        assert!(!is_keepalive_line(": keepalive"));
        assert!(!is_keepalive_line("data: [DONE]"));
    }

    #[test]
    fn the_event_parser_ignores_keepalive_evidence() {
        let mut parser = OmlxEventParser::default();

        let events = parser.parse_line(
            r#"data: {"id":"c","created":0,"model":"keepalive","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
        );

        assert!(events.is_empty());
        assert_eq!(parser.partial_evidence(), None);
    }

    #[test]
    fn reasoning_comes_before_content_in_one_delta() {
        let mut parser = OpenAiLegacyParser::default();

        let chunks = legacy_chunks(
            &mut parser,
            r#"data: {"model":"m","choices":[{"delta":{"reasoning_content":"why","content":"what"}}]}"#,
        );

        assert!(matches!(
            chunks.as_slice(),
            [StreamChunk::Thinking(why), StreamChunk::Content(what)] if why == "why" && what == "what"
        ));
    }

    #[test]
    fn only_json_formats_request_structured_output() {
        let with = |format| CompletionConfig {
            response_format: format,
            ..Default::default()
        };

        assert!(!requests_structured_output(&with(None)));
        assert!(!requests_structured_output(&with(Some(ResponseFormat::Text))));
        assert!(requests_structured_output(&with(Some(ResponseFormat::JsonObject {
            schema: None
        }))));
    }
}
