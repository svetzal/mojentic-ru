#[path = "support/mod.rs"]
pub mod boundary;
use mojentic::llm::{
    gateway::LlmGateway,
    gateways::{OmlxConfig, OmlxGateway},
};

#[tokio::test]
async fn section1_configured_authenticated_boundary() {
    let mut server = boundary::Fixture::new_async().await;
    let artifact = contract();
    let case = artifact["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["name"] == "omlx-selection")
        .expect("selection case");
    let request = server
        .mock("GET", "/v1/models")
        .match_header("authorization", "Bearer metadata-key")
        .with_body(case["body"].as_str().expect("contract body"))
        .create_async()
        .await;
    let gateway = OmlxGateway::with_config(OmlxConfig {
        host: server.url(),
        api_key: Some("metadata-key".into()),
        ..Default::default()
    })
    .with_transport(server.transport());
    let window = gateway
        .get_model_context_window("selected")
        .await
        .expect("metadata request")
        .expect("advertised window");
    assert_eq!(window.max_context_tokens.get(), 262144);
    assert_eq!(window.field, "max_model_len");
    assert_eq!(window.model_num_ctx, None);
    request.assert_async().await;
}

use mojentic::{
    error::{MojenticError, Result},
    llm::{
        gateway::{CompletionConfig, ModelContextWindow},
        gateways::{OllamaConfig, OllamaGateway, OpenAIGateway},
        recovery::{RecoveryGateway, RecoveryPolicy},
        LlmGatewayResponse, LlmMessage,
    },
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};

fn contract() -> Value {
    serde_json::from_str(include_str!("contracts/model_metadata_section1_v1.json"))
        .expect("v1 artifact")
}
fn gateway(provider: &str, host: &str, wrapped: bool) -> Arc<dyn LlmGateway> {
    macro_rules! wrap {
        ($gateway:expr) => {{
            let inner = $gateway;
            if wrapped {
                Arc::new(RecoveryGateway::new(inner, RecoveryPolicy::default()).expect("policy"))
                    as Arc<dyn LlmGateway>
            } else {
                Arc::new(inner) as Arc<dyn LlmGateway>
            }
        }};
    }
    match provider {
        "ollama" => wrap!({
            let transport = boundary::transport_for(host);
            OllamaGateway::with_config(OllamaConfig {
                host: host.into(),
                timeout: Some(Duration::from_secs(2)),
                headers: HashMap::from([("authorization".into(), "Bearer metadata-key".into())]),
            })
            .with_transport(transport)
        }),
        "omlx" => wrap!({
            let transport = boundary::transport_for(host);
            OmlxGateway::with_config(OmlxConfig {
                host: host.into(),
                api_key: Some("metadata-key".into()),
                timeout: Some(Duration::from_secs(2)),
            })
            .with_transport(transport)
        }),
        "openai" => wrap!(OpenAIGateway::with_api_key_and_base_url("metadata-key", host)
            .with_transport(boundary::transport_for(host))),
        _ => panic!("unknown contract provider"),
    }
}

#[tokio::test]
async fn section1_http_contract_exact_outcomes_direct_and_wrapped() {
    let artifact = contract();
    assert_eq!(artifact["version"], "model-metadata-section1-v1");
    for case in artifact["cases"].as_array().expect("cases") {
        for wrapped in [false, true] {
            let provider = case["provider"].as_str().expect("provider");
            let model = case["model"].as_str().expect("model");
            let mut server = boundary::Fixture::new_async().await;
            let (method, path) = if provider == "ollama" {
                ("POST", "/api/show")
            } else {
                ("GET", "/v1/models")
            };
            let mut mock =
                server.mock(method, path).match_header("authorization", "Bearer metadata-key");
            if provider == "ollama" {
                mock = mock.match_body(boundary::Matcher::Json(json!({"model":model})));
            }
            let request = mock
                .with_status(case["status"].as_u64().expect("status") as usize)
                .with_body(case["body"].as_str().expect("body"))
                .expect(1)
                .create_async()
                .await;
            let result =
                gateway(provider, &server.url(), wrapped).get_model_context_window(model).await;
            let context = format!("{} wrapped={wrapped}", case["name"]);
            match case["error"].as_str() {
                Some("http") => assert!(
                    matches!(result, Err(MojenticError::HttpError(_))),
                    "{context}: {result:?}"
                ),
                Some("gateway") => assert!(
                    matches!(result, Err(MojenticError::GatewayError(_))),
                    "{context}: {result:?}"
                ),
                None => {
                    let expected: Option<ModelContextWindow> =
                        serde_json::from_value(case["expected"].clone()).expect("window");
                    assert_eq!(result.expect(&context), expected, "{context}");
                }
                _ => panic!("unknown error expectation"),
            }
            request.assert_async().await;
        }
    }
}

struct RequestObserver {
    url: String,
    fixture: boundary::Fixture,
}
impl RequestObserver {
    async fn start() -> Self {
        let fixture = boundary::Fixture::new();
        fixture.respond(|_| {
            http::Response::builder()
                .status(503)
                .body(reqwest::Body::from(""))
                .expect("failure")
                .into()
        });
        Self {
            url: fixture.url(),
            fixture,
        }
    }
    async fn assert_request_free(self) {
        let requests = self.fixture.requests();
        let requests: Vec<_> = requests
            .iter()
            .map(|r| {
                format!(
                    "{} {}{} HTTP/1.1",
                    r.method(),
                    r.url().path(),
                    r.url().query().map_or(String::new(), |q| format!("?{q}"))
                )
            })
            .collect();
        assert!(requests.is_empty(), "unexpected outbound HTTP requests: {requests:?}");
    }
}

#[tokio::test]
async fn section1_registry_exact_lookup_is_request_free() {
    let artifact = contract();
    assert_eq!(artifact["version"], "model-metadata-section1-v1");
    for case in artifact["registry"].as_array().expect("registry") {
        for wrapped in [false, true] {
            let observer = RequestObserver::start().await;
            let gateway = gateway("openai", &observer.url, wrapped);
            let result = gateway
                .get_model_context_window(case["model"].as_str().expect("model"))
                .await
                .expect("local lookup");
            let expected = case["tokens"].as_u64().map(|tokens| ModelContextWindow {
                max_context_tokens: std::num::NonZeroUsize::new(tokens as usize).expect("positive"),
                field: "registry".into(),
                model_num_ctx: None,
            });
            assert_eq!(result, expected, "{case} wrapped={wrapped}");
            observer.assert_request_free().await;
        }
    }
}

#[tokio::test]
#[should_panic(
    expected = "unexpected outbound HTTP requests: [\"POST /ignored-failure?probe=1 HTTP/1.1\"]"
)]
async fn section1_request_observer_rejects_ignored_post_failure() {
    let observer = RequestObserver::start().await;
    // The recorder sees every attempted method, even if its error is discarded.
    let request = reqwest::Client::new()
        .post(format!("{}/ignored-failure?probe=1", observer.url))
        .build()
        .expect("request");
    let _ = observer
        .fixture
        .transport()
        .execute(request)
        .await
        .expect("response")
        .error_for_status();
    observer.assert_request_free().await;
}

#[test]
fn section1_effective_precedence_and_clamping() {
    for case in contract()["effective"].as_array().expect("effective cases") {
        let window = case["maximum"].as_u64().map(|maximum| ModelContextWindow {
            max_context_tokens: std::num::NonZeroUsize::new(maximum as usize).expect("positive"),
            field: "model_info.example.context_length".into(),
            model_num_ctx: case["model_num_ctx"].as_u64().map(|n| n as usize),
        });
        let config = CompletionConfig {
            num_ctx: case["num_ctx"].as_u64().map(|n| n as usize),
            ..Default::default()
        };
        assert_eq!(
            OllamaGateway::effective_context_window(window.as_ref(), &config),
            case["expected"].as_u64().map(|n| n as usize),
            "{case}"
        );
    }
}

#[tokio::test]
async fn section1_transport_failure_direct_and_wrapped() {
    for provider in ["ollama", "omlx"] {
        for wrapped in [false, true] {
            let server = boundary::Fixture::new();
            server.fail(std::io::ErrorKind::ConnectionRefused);
            let result = gateway(provider, &server.url(), wrapped)
                .get_model_context_window("selected")
                .await;
            assert!(matches!(result, Err(MojenticError::HttpError(_))), "{provider}: {result:?}");
        }
    }
}

#[tokio::test]
async fn section1_lookup_preserves_subsequent_completion_payload() {
    let metadata = contract();
    let fidelity: Value = serde_json::from_str(include_str!("contracts/request_fidelity_v1.json"))
        .expect("fidelity contract");
    for provider in ["ollama", "omlx", "openai"] {
        for wrapped in [false, true] {
            for config_case in metadata["completion_configs"].as_array().expect("configs") {
                let mut server = boundary::Fixture::new_async().await;
                let gateway = gateway(provider, &server.url(), wrapped);
                let path = match provider {
                    "ollama" => "/api/chat",
                    "omlx" => "/v1/chat/completions",
                    _ => "/chat/completions",
                };
                let body = metadata[if wrapped {
                    "wrapped_completion_requests"
                } else {
                    "completion_requests"
                }][config_case["name"].as_str().expect("config name")][provider]
                    .clone();
                let response = if provider == "ollama" {
                    "ollama-complete"
                } else {
                    "openai-complete"
                };
                let completion = server
                    .mock("POST", path)
                    .match_body(boundary::Matcher::Json(body))
                    .with_body(fidelity["responses"][response].as_str().expect("contract response"))
                    .expect(2)
                    .create_async()
                    .await;
                let messages = [LlmMessage::user("question")];
                let config = CompletionConfig {
                    num_ctx: config_case["config"]["num_ctx"].as_u64().map(|n| n as usize),
                    max_tokens: config_case["config"]["max_tokens"].as_u64().map(|n| n as usize),
                    temperature: config_case["config"]["temperature"].as_f64().map(|n| n as f32),
                    ..Default::default()
                };
                assert_eq!(
                    gateway
                        .complete("selected", &messages, None, &config)
                        .await
                        .expect("before")
                        .content
                        .as_deref(),
                    Some("answer")
                );
                let metadata_request = if provider != "openai" {
                    let case = metadata["cases"]
                        .as_array()
                        .expect("cases")
                        .iter()
                        .find(|case| case["provider"] == provider)
                        .expect("provider case");
                    Some(
                        server
                            .mock(
                                if provider == "ollama" { "POST" } else { "GET" },
                                if provider == "ollama" {
                                    "/api/show"
                                } else {
                                    "/v1/models"
                                },
                            )
                            .with_body(case["body"].as_str().expect("body"))
                            .create_async()
                            .await,
                    )
                } else {
                    None
                };
                gateway.get_model_context_window("selected").await.expect("lookup");
                assert_eq!(
                    gateway
                        .complete("selected", &messages, None, &config)
                        .await
                        .expect("after")
                        .content
                        .as_deref(),
                    Some("answer")
                );
                completion.assert_async().await;
                if let Some(request) = metadata_request {
                    request.assert_async().await;
                }
            }
        }
    }
}

struct LegacyGateway;
#[async_trait::async_trait]
impl LlmGateway for LegacyGateway {
    fn complete_stream<'a>(
        &'a self,
        _: &'a str,
        _: &'a [LlmMessage],
        _: Option<&'a [Box<dyn mojentic::llm::LlmTool>]>,
        _: &'a CompletionConfig,
    ) -> std::pin::Pin<
        Box<dyn futures::Stream<Item = Result<mojentic::llm::gateway::StreamChunk>> + Send + 'a>,
    > {
        Box::pin(futures::stream::empty())
    }

    async fn complete(
        &self,
        _: &str,
        _: &[LlmMessage],
        _: Option<&[Box<dyn mojentic::llm::LlmTool>]>,
        _: &CompletionConfig,
    ) -> Result<LlmGatewayResponse> {
        Err(MojenticError::GatewayError("unused".into()))
    }
    async fn complete_json(
        &self,
        _: &str,
        _: &[LlmMessage],
        _: Value,
        _: &CompletionConfig,
    ) -> Result<Value> {
        Ok(Value::Null)
    }
    async fn get_available_models(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }
    async fn calculate_embeddings(&self, _: &str, _: Option<&str>) -> Result<Vec<f32>> {
        Ok(vec![])
    }
}
#[tokio::test]
async fn section1_existing_third_party_gateway_is_source_compatible() {
    assert_eq!(
        LegacyGateway
            .get_model_context_window("unknown")
            .await
            .expect("default metadata"),
        None
    );
}

#[test]
fn section1_contract_reuses_coordinating_captures() {
    let artifact = contract();
    for (name, capture) in [
        ("show_context_length", include_str!("fixtures/ollama/show_context_length.json")),
        (
            "show_num_ctx_parameter",
            include_str!("fixtures/ollama/show_num_ctx_parameter.json"),
        ),
        (
            "unknown-model-capture",
            include_str!("fixtures/ollama/error_model_not_found.json"),
        ),
        ("omlx-capture", include_str!("fixtures/omlx/models.json")),
    ] {
        let case = artifact["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|case| case["name"] == name)
            .expect("capture case");
        assert_eq!(case["body"].as_str(), Some(capture), "{name}");
    }
}
