use super::{engine::PrivateEvidence, *};
use crate::llm::{
    gateway::StreamProgress,
    gateways::{
        ollama::{frame_progress_counts, ollama_evidence, ollama_stream_metrics},
        openai::openai_response_evidence,
    },
    models::{LlmGatewayResponse, LlmToolCall, ResponseEvidence},
};
use serde_json::Value;
use std::collections::BTreeMap;
fn invalid(text: &str) -> PrivateEvidence {
    PrivateEvidence::new(text.as_bytes().to_vec())
}
fn text(value: &Value) -> Result<Option<String>, PrivateEvidence> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        _ => Err(invalid("invalid text field")),
    }
}
fn tools(value: &Value, ollama: bool) -> Result<Vec<LlmToolCall>, PrivateEvidence> {
    if value.is_null() {
        return Ok(vec![]);
    }
    value
        .as_array()
        .ok_or_else(|| invalid("invalid tool calls"))?
        .iter()
        .map(|call| {
            let name = call["function"]["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("missing tool name"))?
                .into();
            let arguments = if ollama {
                call["function"]["arguments"].clone()
            } else {
                serde_json::from_str(
                    call["function"]["arguments"]
                        .as_str()
                        .ok_or_else(|| invalid("invalid tool arguments"))?,
                )
                .map_err(PrivateEvidence::from_error)?
            };
            let arguments = arguments
                .as_object()
                .ok_or_else(|| invalid("tool arguments must be an object"))?
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Ok(LlmToolCall {
                id: text(&call["id"])?,
                name,
                arguments,
            })
        })
        .collect()
}
pub(super) fn decode(
    provider: Provider,
    bytes: &[u8],
    structured: bool,
) -> Result<LlmGatewayResponse<Value>, PrivateEvidence> {
    let body: Value = serde_json::from_slice(bytes).map_err(PrivateEvidence::from_error)?;
    if !body["error"].is_null() {
        return Err(PrivateEvidence::new(bytes.to_vec()));
    }
    let message = if provider == Provider::Ollama {
        &body["message"]
    } else {
        &body["choices"][0]["message"]
    };
    if !message.is_object() {
        return Err(invalid("missing completion message"));
    }
    let content = text(&message["content"])?;
    let thinking = text(
        &message[if provider == Provider::Ollama {
            "thinking"
        } else {
            "reasoning_content"
        }],
    )?;
    let tool_calls = tools(&message["tool_calls"], provider == Provider::Ollama)?;
    if content.is_none() && thinking.is_none() && tool_calls.is_empty() {
        return Err(invalid("empty completion"));
    }
    let object = if structured {
        Some(
            serde_json::from_str(
                content.as_deref().ok_or_else(|| invalid("missing structured content"))?,
            )
            .map_err(PrivateEvidence::from_error)?,
        )
    } else {
        None
    };
    let evidence = if provider == Provider::Ollama {
        ollama_evidence(&body)
    } else {
        openai_response_evidence(&body)
    };
    Ok(LlmGatewayResponse {
        content,
        object,
        thinking,
        tool_calls,
        evidence,
    })
}
pub(super) fn response_progress<T>(
    response: &LlmGatewayResponse<T>,
    progress: &mut SemanticProgress,
) {
    progress.content_bytes = response.content.as_ref().map_or(0, |v| v.len() as u64);
    progress.reasoning_bytes = response.thinking.as_ref().map_or(0, |v| v.len() as u64);
    progress.completed_tool_calls = response.tool_calls.len() as u64;
}
pub(super) fn delivered(event: &RecoveryStreamEvent, progress: &mut SemanticProgress) {
    match event {
        RecoveryStreamEvent::Content(text) => {
            progress.content_bytes = progress.content_bytes.saturating_add(text.len() as u64)
        }
        RecoveryStreamEvent::Reasoning(text) => {
            progress.reasoning_bytes = progress.reasoning_bytes.saturating_add(text.len() as u64)
        }
        RecoveryStreamEvent::ToolFragment(_) => {
            progress.tool_fragments = progress.tool_fragments.saturating_add(1)
        }
        _ => {}
    }
}
#[derive(Default)]
struct Tool {
    id: String,
    name: String,
    arguments: String,
}
pub(super) struct Frames {
    provider: Provider,
    frame_index: usize,
    pub terminal: bool,
    finish: Option<String>,
    content: String,
    reasoning: String,
    tools: BTreeMap<u64, Tool>,
    ollama_tools: Vec<LlmToolCall>,
    evidence: ResponseEvidence,
}
impl Frames {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            frame_index: 0,
            terminal: false,
            finish: None,
            content: String::new(),
            reasoning: String::new(),
            tools: BTreeMap::new(),
            ollama_tools: vec![],
            evidence: ResponseEvidence::default(),
        }
    }
    pub fn parse(
        &mut self,
        line: &[u8],
        progress: &mut SemanticProgress,
        identity: Option<&Identity>,
    ) -> (Vec<RecoveryStreamEvent>, Result<(), PrivateEvidence>) {
        let mut output = Vec::new();
        let result = self.parse_frame(line, progress, identity, &mut output);
        if result.is_err() {
            // A rejected finish cannot deliver semantic output or completed tools.
            // Retain only telemetry extracted from structurally validated fields.
            output.retain(|event| {
                matches!(
                    event,
                    RecoveryStreamEvent::Progress { .. } | RecoveryStreamEvent::Metrics { .. }
                )
            });
        }
        (output, result)
    }
    fn parse_frame(
        &mut self,
        line: &[u8],
        progress: &mut SemanticProgress,
        identity: Option<&Identity>,
        output: &mut Vec<RecoveryStreamEvent>,
    ) -> Result<(), PrivateEvidence> {
        let line = std::str::from_utf8(line).map_err(PrivateEvidence::from_error)?.trim();
        if line.is_empty() || line.starts_with(':') {
            return Ok(());
        }
        if self.terminal {
            return Err(invalid("frame after terminal marker"));
        }
        let line = if self.provider != Provider::Ollama {
            if line.starts_with("event:") || line.starts_with("id:") {
                return Ok(());
            }
            line.strip_prefix("data:").ok_or_else(|| invalid("invalid SSE field"))?.trim()
        } else {
            line
        };
        if line == "[DONE]" && self.provider != Provider::Ollama {
            if !matches!(self.finish.as_deref(), Some("stop" | "tool_calls")) {
                return Err(invalid("terminal marker without safe finish reason"));
            }
            self.terminal = true;
            return Ok(());
        }
        let frame: Value = serde_json::from_str(line).map_err(PrivateEvidence::from_error)?;
        if !frame.is_object() {
            return Err(invalid("frame must be an object"));
        }
        if self.provider == Provider::Omlx && frame["model"] == "keepalive" {
            return Ok(());
        }
        if !frame["error"].is_null() {
            return Err(PrivateEvidence::new(line.as_bytes().to_vec()));
        }
        let reported = if self.provider == Provider::Ollama {
            ollama_evidence(&frame)
        } else {
            openai_response_evidence(&frame)
        };
        if reported.usage.is_some() {
            self.evidence.usage = reported.usage;
        }
        if reported.provider_model.is_some() {
            self.evidence.provider_model = reported.provider_model;
        }
        self.evidence.metadata.extend(reported.metadata);
        let delta = if self.provider == Provider::Ollama {
            &frame["message"]
        } else {
            let choices = frame["choices"].as_array().ok_or_else(|| invalid("missing choices"))?;
            if choices.is_empty() && !frame["usage"].is_null() {
                return Ok(());
            }
            if choices.len() != 1 {
                return Err(invalid("expected one choice"));
            }
            if let Some(reason) = text(&choices[0]["finish_reason"])? {
                self.finish = Some(reason.clone());
                self.evidence.finish_reason = Some(reason);
            }
            &choices[0]["delta"]
        };
        if !delta.is_object() && !(self.provider == Provider::Ollama && delta.is_null()) {
            return Err(invalid("missing stream delta"));
        }
        let accumulated_tool_call_count = self.ollama_tools.len();
        for (field, reasoning) in [
            ("content", false),
            (
                if self.provider == Provider::Ollama {
                    "thinking"
                } else {
                    "reasoning_content"
                },
                true,
            ),
        ] {
            if let Some(text) = text(&delta[field])?.filter(|s| !s.is_empty()) {
                if reasoning {
                    progress.reasoning_bytes =
                        progress.reasoning_bytes.saturating_add(text.len() as u64);
                    self.reasoning.push_str(&text);
                    output.push(RecoveryStreamEvent::Reasoning(text));
                } else {
                    progress.content_bytes =
                        progress.content_bytes.saturating_add(text.len() as u64);
                    self.content.push_str(&text);
                    output.push(RecoveryStreamEvent::Content(text));
                }
            }
        }
        if !delta["tool_calls"].is_null() {
            let calls = delta["tool_calls"]
                .as_array()
                .ok_or_else(|| invalid("invalid tool fragments"))?;
            for call in calls {
                progress.tool_fragments = progress.tool_fragments.saturating_add(1);
                output.push(RecoveryStreamEvent::ToolFragment(call.clone()));
                if self.provider == Provider::Ollama {
                    self.ollama_tools.extend(tools(&Value::Array(vec![call.clone()]), true)?);
                } else {
                    let index =
                        call["index"].as_u64().ok_or_else(|| invalid("missing tool index"))?;
                    let tool = self.tools.entry(index).or_default();
                    if let Some(id) = text(&call["id"])? {
                        tool.id.push_str(&id);
                    }
                    if let Some(name) = text(&call["function"]["name"])? {
                        tool.name.push_str(&name);
                    }
                    if let Some(args) = text(&call["function"]["arguments"])? {
                        tool.arguments.push_str(&args);
                    }
                }
            }
        }
        if self.provider == Provider::Ollama {
            if !(frame["done"].is_null() || frame["done"].is_boolean()) {
                return Err(invalid("invalid done field"));
            }
            let finish = if frame["done"] == true {
                text(&frame["done_reason"])?
            } else {
                None
            };
            self.frame_index += 1;
            let (content_chars, thinking_chars, tool_call_count) = frame_progress_counts(&frame);
            if let Some(identity) = identity {
                output.insert(
                    0,
                    RecoveryStreamEvent::Progress {
                        identity: identity.clone(),
                        progress: StreamProgress {
                            provider: "ollama".into(),
                            frame_index: self.frame_index,
                            done: frame["done"].as_bool().unwrap_or(false),
                            content_chars,
                            thinking_chars,
                            tool_call_count,
                            accumulated_tool_call_count,
                        },
                    },
                );
            }
            if frame["done"] == true {
                self.finish = finish;
                self.evidence.finish_reason = self.finish.clone();
                if let Some(identity) = identity {
                    // Metrics describe the received terminal frame, even when its
                    // finish reason does not prove successful completion.
                    output.insert(
                        1,
                        RecoveryStreamEvent::Metrics {
                            identity: identity.clone(),
                            metrics: ollama_stream_metrics(&frame),
                        },
                    );
                }
                if self.finish.as_deref() != Some("stop") {
                    return Err(invalid("incomplete stream finish"));
                }
                self.terminal = true;
            }
        }
        if self.terminal || self.finish.as_deref() == Some("tool_calls") {
            progress.completed_tool_calls = self.build_tools()?.len() as u64;
        }
        Ok(())
    }
    fn build_tools(&self) -> Result<Vec<LlmToolCall>, PrivateEvidence> {
        if self.provider == Provider::Ollama {
            return Ok(self.ollama_tools.clone());
        }
        let calls: Vec<_> = self.tools.values().map(|t| serde_json::json!({"id":t.id,"function":{"name":t.name,"arguments":t.arguments}})).collect();
        tools(&Value::Array(calls), false)
    }
    pub fn response(&self) -> Result<LlmGatewayResponse, PrivateEvidence> {
        if !self.terminal {
            return Err(invalid("stream is incomplete"));
        }
        Ok(LlmGatewayResponse {
            content: Some(self.content.clone()),
            thinking: (!self.reasoning.is_empty()).then(|| self.reasoning.clone()),
            tool_calls: self.build_tools()?,
            evidence: self.evidence.clone(),
            object: None,
        })
    }
}
