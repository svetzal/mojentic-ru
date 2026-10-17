use mojentic::llm::{
    gateway::LlmGateway,
    gateways::{OmlxConfig, OmlxGateway},
};

#[tokio::test]
async fn section1_configured_authenticated_boundary() {
    let mut server = mockito::Server::new_async().await;
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
    });
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
        "ollama" => wrap!(OllamaGateway::with_config(OllamaConfig {
            host: host.into(),
            timeout: Some(Duration::from_secs(2)),
            headers: HashMap::from([("authorization".into(), "Bearer metadata-key".into())]),
        })),
        "omlx" => wrap!(OmlxGateway::with_config(OmlxConfig {
            host: host.into(),
            api_key: Some("metadata-key".into()),
            timeout: Some(Duration::from_secs(2)),
        })),
        "openai" => wrap!(OpenAIGateway::with_api_key_and_base_url("metadata-key", host)),
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
            let mut server = mockito::Server::new_async().await;
            let (method, path) = if provider == "ollama" {
                ("POST", "/api/show")
            } else {
                ("GET", "/v1/models")
            };
            let mut mock =
                server.mock(method, path).match_header("authorization", "Bearer metadata-key");
            if provider == "ollama" {
                mock = mock.match_body(mockito::Matcher::Json(json!({"model":model})));
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

#[tokio::test]
async fn section1_registry_exact_lookup_is_request_free() {
    for wrapped in [false, true] {
        let mut server = mockito::Server::new_async().await;
        let no_request = server.mock("GET", mockito::Matcher::Any).expect(0).create_async().await;
        let gateway = gateway("openai", &server.url(), wrapped);
        for case in contract()["registry"].as_array().expect("registry") {
            let result = gateway
                .get_model_context_window(case["model"].as_str().expect("model"))
                .await
                .expect("local lookup");
            let expected = case["tokens"].as_u64().map(|tokens| ModelContextWindow {
                max_context_tokens: std::num::NonZeroUsize::new(tokens as usize).expect("positive"),
                field: "registry".into(),
                model_num_ctx: None,
            });
            assert_eq!(result, expected, "{case}");
        }
        no_request.assert_async().await;
        // An unreachable host also proves lookup needs no successful HTTP exchange.
        assert!(self::gateway("openai", "http://127.0.0.1:0", wrapped)
            .get_model_context_window("gpt-4o")
            .await
            .expect("offline registry")
            .is_some());
    }
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
            let result = gateway(provider, "http://127.0.0.1:0", wrapped)
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
                let mut server = mockito::Server::new_async().await;
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
                    .match_body(mockito::Matcher::Json(body))
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
