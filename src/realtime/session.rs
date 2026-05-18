//! Realtime session — driver that normalises raw OpenAI events into
//! vendor-neutral [`RealtimeEvent`] values and dispatches tool batches.
//!
//! Compared to the Python and TypeScript ports, the Rust session is
//! consumed via two channels:
//!
//! - `events_rx`: vendor-neutral [`RealtimeEvent`] stream
//! - `audio_rx`: decoded PCM frames (also surfaced as
//!   `AssistantAudioDelta` events for completeness)
//!
//! Callers spawn the session via [`RealtimeSession::run`], which takes
//! the gateway session and returns the channels; the run task pumps the
//! gateway until close. This mirrors the broker → session orchestration
//! in the other ports while staying idiomatic Rust async.

use crate::llm::tools::{LlmTool, ToolCallExecution, ToolRunCtx, ToolRunner};
use crate::realtime::codec::{decode_base64_pcm16, encode_base64_pcm16};
use crate::realtime::config::{
    defaults, InterruptOutputPolicy, RealtimeAudioFormat, RealtimeModality, RealtimeToolChoice,
    RealtimeVoiceConfig, SemanticVadConfig, ServerVadConfig, TurnDetectionMode,
};
use crate::realtime::events::{InterruptReason, RealtimeEvent, SessionCloseReason, TokenUsage};
use crate::realtime::gateway::RealtimeGatewaySession;
use crate::tracer::TracerSystem;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Outbound handle returned by [`RealtimeSession::run`].
pub struct RealtimeSession {
    events_rx: mpsc::UnboundedReceiver<RealtimeEvent>,
    commands_tx: mpsc::UnboundedSender<Command>,
}

#[derive(Debug)]
enum Command {
    SendText(String),
    SendAudio(Vec<i16>),
    CommitAudio,
    Interrupt,
    UpdateInstructions(String),
    Close,
}

#[derive(Debug, Default)]
struct PendingCall {
    name: String,
    args_buffer: String,
    done: bool,
}

#[derive(Debug)]
struct TurnState {
    turn_id: String,
    calls: HashMap<String, PendingCall>,
    cancelled: bool,
    cancel_token: CancellationToken,
}

impl RealtimeSession {
    /// Receive the next vendor-neutral event from the session.
    pub async fn next_event(&mut self) -> Option<RealtimeEvent> {
        self.events_rx.recv().await
    }

    pub fn send_text(&self, text: impl Into<String>) -> Result<(), &'static str> {
        self.commands_tx
            .send(Command::SendText(text.into()))
            .map_err(|_| "session closed")
    }

    pub fn send_audio_frame(&self, samples: Vec<i16>) -> Result<(), &'static str> {
        self.commands_tx.send(Command::SendAudio(samples)).map_err(|_| "session closed")
    }

    pub fn commit_audio(&self) -> Result<(), &'static str> {
        self.commands_tx.send(Command::CommitAudio).map_err(|_| "session closed")
    }

    pub fn interrupt(&self) -> Result<(), &'static str> {
        self.commands_tx.send(Command::Interrupt).map_err(|_| "session closed")
    }

    pub fn update_instructions(&self, instructions: impl Into<String>) -> Result<(), &'static str> {
        self.commands_tx
            .send(Command::UpdateInstructions(instructions.into()))
            .map_err(|_| "session closed")
    }

    pub fn close(&self) -> Result<(), &'static str> {
        self.commands_tx.send(Command::Close).map_err(|_| "session closed")
    }

    /// Spawn a session driver that orchestrates the gateway. Returns
    /// the session handle; the driver task lives until the gateway
    /// session closes or [`RealtimeSession::close`] is called.
    pub async fn run(
        gateway: Box<dyn RealtimeGatewaySession>,
        config: RealtimeVoiceConfig,
        tool_runner: Arc<dyn ToolRunner>,
        tracer: Option<Arc<TracerSystem>>,
        correlation_id: String,
    ) -> Self {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (commands_tx, commands_rx) = mpsc::unbounded_channel();

        let driver = SessionDriver {
            gateway,
            config,
            tool_runner,
            tracer,
            correlation_id,
            events_tx,
            current_turn: None,
            current_response_id: None,
        };

        tokio::spawn(driver.run(commands_rx));

        Self {
            events_rx,
            commands_tx,
        }
    }
}

/// Internal driver — owns the gateway session and pumps both inbound
/// gateway events and outbound commands.
struct SessionDriver {
    gateway: Box<dyn RealtimeGatewaySession>,
    config: RealtimeVoiceConfig,
    tool_runner: Arc<dyn ToolRunner>,
    tracer: Option<Arc<TracerSystem>>,
    correlation_id: String,
    events_tx: mpsc::UnboundedSender<RealtimeEvent>,
    current_turn: Option<TurnState>,
    current_response_id: Option<String>,
}

impl SessionDriver {
    async fn run(mut self, mut commands_rx: mpsc::UnboundedReceiver<Command>) {
        let initial = build_session_update(&self.config);
        let _ = self.gateway.send_event(&initial).await;
        let _ = self.events_tx.send(RealtimeEvent::SessionOpened {
            session_id: self.gateway.session_id().to_string(),
        });

        loop {
            tokio::select! {
                cmd = commands_rx.recv() => {
                    match cmd {
                        Some(Command::SendText(text)) => self.send_text(text).await,
                        Some(Command::SendAudio(samples)) => self.send_audio(samples).await,
                        Some(Command::CommitAudio) => self.commit_audio().await,
                        Some(Command::Interrupt) => self.cancel_turn(InterruptReason::Manual).await,
                        Some(Command::UpdateInstructions(text)) => self.update_instructions(text).await,
                        Some(Command::Close) | None => {
                            let _ = self.gateway.close().await;
                            let _ = self.events_tx.send(RealtimeEvent::SessionClosed { reason: SessionCloseReason::Client });
                            return;
                        }
                    }
                }
                event = self.gateway.next_event() => {
                    match event {
                        Some(raw) => self.handle_server_event(raw).await,
                        None => {
                            let _ = self.events_tx.send(RealtimeEvent::SessionClosed { reason: SessionCloseReason::Server });
                            return;
                        }
                    }
                }
            }
        }
    }

    async fn send_text(&mut self, text: String) {
        let _ = self
            .gateway
            .send_event(&json!({
                "type": "conversation.item.create",
                "item": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": text }]
                }
            }))
            .await;
        let _ = self.gateway.send_event(&json!({ "type": "response.create" })).await;
    }

    async fn send_audio(&mut self, samples: Vec<i16>) {
        let _ = self
            .gateway
            .send_event(&json!({
                "type": "input_audio_buffer.append",
                "audio": encode_base64_pcm16(&samples)
            }))
            .await;
    }

    async fn commit_audio(&mut self) {
        let _ = self.gateway.send_event(&json!({ "type": "input_audio_buffer.commit" })).await;
        let _ = self.gateway.send_event(&json!({ "type": "response.create" })).await;
    }

    async fn update_instructions(&mut self, instructions: String) {
        self.config.instructions = Some(instructions.clone());
        let _ = self
            .gateway
            .send_event(&json!({
                "type": "session.update",
                "session": { "instructions": instructions }
            }))
            .await;
    }

    async fn cancel_turn(&mut self, reason: InterruptReason) {
        let Some(turn) = self.current_turn.as_mut() else {
            return;
        };
        if turn.cancelled {
            return;
        }
        turn.cancelled = true;
        turn.cancel_token.cancel();
        let _ = self.events_tx.send(RealtimeEvent::Interrupted {
            turn_id: turn.turn_id.clone(),
            reason,
        });
        if self.current_response_id.is_some() {
            let _ = self.gateway.send_event(&json!({ "type": "response.cancel" })).await;
        }
    }

    async fn handle_server_event(&mut self, raw: Value) {
        let Some(event_type) = raw.get("type").and_then(|v| v.as_str()) else {
            return;
        };

        match event_type {
            "session.created" => {}
            "session.updated" => {
                let _ = self.events_tx.send(RealtimeEvent::SessionUpdated {
                    config: HashMap::new(),
                });
            }
            "input_audio_buffer.speech_started" => {
                let at_ms = raw.get("audio_start_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                let _ = self.events_tx.send(RealtimeEvent::UserSpeechStarted { at_ms });
                if self.current_turn.is_some() {
                    self.cancel_turn(InterruptReason::BargeIn).await;
                }
            }
            "input_audio_buffer.speech_stopped" => {
                let at_ms = raw.get("audio_end_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                let _ = self.events_tx.send(RealtimeEvent::UserSpeechStopped { at_ms });
            }
            "conversation.item.input_audio_transcription.delta" => {
                self.emit_user_transcript_delta(&raw);
            }
            "conversation.item.input_audio_transcription.completed" => {
                self.emit_user_transcript(&raw);
            }
            "response.created" => self.start_response(&raw),
            "response.output_item.added" => self.add_output_item(&raw),
            "response.function_call_arguments.delta" => self.fn_args_delta(&raw),
            "response.function_call_arguments.done" => self.fn_args_done(&raw),
            "response.text.delta" | "response.output_text.delta" => self.text_delta(&raw),
            "response.text.done" | "response.output_text.done" => self.text_done(&raw),
            "response.audio_transcript.delta" | "response.output_audio_transcript.delta" => {
                self.transcript_delta(&raw)
            }
            "response.audio_transcript.done" | "response.output_audio_transcript.done" => {
                self.transcript_done(&raw)
            }
            "response.audio.delta" | "response.output_audio.delta" => self.audio_delta(&raw),
            "response.done" => self.response_done(&raw).await,
            "rate_limits.updated" => {
                let _ = self.events_tx.send(RealtimeEvent::RateLimited {
                    reset_ms: 0,
                    details: HashMap::new(),
                });
            }
            "error" => self.emit_error(&raw),
            _ => {}
        }
    }

    fn emit_user_transcript_delta(&self, raw: &Value) {
        if let (Some(id), Some(delta)) = (
            raw.get("item_id").and_then(|v| v.as_str()),
            raw.get("delta").and_then(|v| v.as_str()),
        ) {
            let _ = self.events_tx.send(RealtimeEvent::UserTranscriptDelta {
                item_id: id.to_string(),
                delta: delta.to_string(),
            });
        }
    }

    fn emit_user_transcript(&self, raw: &Value) {
        if let (Some(id), Some(text)) = (
            raw.get("item_id").and_then(|v| v.as_str()),
            raw.get("transcript").and_then(|v| v.as_str()),
        ) {
            let _ = self.events_tx.send(RealtimeEvent::UserTranscript {
                item_id: id.to_string(),
                text: text.to_string(),
            });
        }
    }

    fn start_response(&mut self, raw: &Value) {
        let Some(id) = raw.pointer("/response/id").and_then(|v| v.as_str()) else {
            return;
        };
        let turn_id = id.to_string();
        self.current_response_id = Some(turn_id.clone());
        self.current_turn = Some(TurnState {
            turn_id: turn_id.clone(),
            calls: HashMap::new(),
            cancelled: false,
            cancel_token: CancellationToken::new(),
        });
        let _ = self.events_tx.send(RealtimeEvent::AssistantTurnStarted { turn_id });
    }

    fn add_output_item(&mut self, raw: &Value) {
        let Some(turn) = self.current_turn.as_mut() else {
            return;
        };
        let Some(item) = raw.get("item") else { return };
        if item.get("type").and_then(|v| v.as_str()) != Some("function_call") {
            return;
        }
        let (Some(call_id), Some(name)) = (
            item.get("call_id").and_then(|v| v.as_str()),
            item.get("name").and_then(|v| v.as_str()),
        ) else {
            return;
        };
        turn.calls.insert(
            call_id.to_string(),
            PendingCall {
                name: name.to_string(),
                args_buffer: String::new(),
                done: false,
            },
        );
        let _ = self.events_tx.send(RealtimeEvent::ToolCallStarted {
            turn_id: turn.turn_id.clone(),
            call_id: call_id.to_string(),
            name: name.to_string(),
        });
    }

    fn fn_args_delta(&mut self, raw: &Value) {
        let Some(turn) = self.current_turn.as_mut() else {
            return;
        };
        let (Some(call_id), Some(delta)) = (
            raw.get("call_id").and_then(|v| v.as_str()),
            raw.get("delta").and_then(|v| v.as_str()),
        ) else {
            return;
        };
        if let Some(call) = turn.calls.get_mut(call_id) {
            call.args_buffer.push_str(delta);
        }
        let _ = self.events_tx.send(RealtimeEvent::ToolCallArgsDelta {
            call_id: call_id.to_string(),
            delta: delta.to_string(),
        });
    }

    fn fn_args_done(&mut self, raw: &Value) {
        let Some(turn) = self.current_turn.as_mut() else {
            return;
        };
        let Some(call_id) = raw.get("call_id").and_then(|v| v.as_str()) else {
            return;
        };
        if let Some(call) = turn.calls.get_mut(call_id) {
            if let Some(args) = raw.get("arguments").and_then(|v| v.as_str()) {
                call.args_buffer = args.to_string();
            }
            call.done = true;
        }
    }

    fn text_delta(&self, raw: &Value) {
        let Some(turn) = self.current_turn.as_ref() else {
            return;
        };
        if let Some(delta) = raw.get("delta").and_then(|v| v.as_str()) {
            let _ = self.events_tx.send(RealtimeEvent::AssistantTextDelta {
                turn_id: turn.turn_id.clone(),
                delta: delta.to_string(),
            });
        }
    }

    fn text_done(&self, raw: &Value) {
        let Some(turn) = self.current_turn.as_ref() else {
            return;
        };
        if let Some(text) = raw.get("text").and_then(|v| v.as_str()) {
            let _ = self.events_tx.send(RealtimeEvent::AssistantText {
                turn_id: turn.turn_id.clone(),
                text: text.to_string(),
            });
        }
    }

    fn transcript_delta(&self, raw: &Value) {
        let Some(turn) = self.current_turn.as_ref() else {
            return;
        };
        if let Some(delta) = raw.get("delta").and_then(|v| v.as_str()) {
            let _ = self.events_tx.send(RealtimeEvent::AssistantTranscriptDelta {
                turn_id: turn.turn_id.clone(),
                delta: delta.to_string(),
            });
        }
    }

    fn transcript_done(&self, raw: &Value) {
        let Some(turn) = self.current_turn.as_ref() else {
            return;
        };
        if let Some(text) = raw.get("transcript").and_then(|v| v.as_str()) {
            let _ = self.events_tx.send(RealtimeEvent::AssistantTranscript {
                turn_id: turn.turn_id.clone(),
                text: text.to_string(),
            });
        }
    }

    fn audio_delta(&self, raw: &Value) {
        let Some(turn) = self.current_turn.as_ref() else {
            return;
        };
        let Some(delta) = raw.get("delta").and_then(|v| v.as_str()) else {
            return;
        };
        if let Ok(pcm) = decode_base64_pcm16(delta) {
            let _ = self.events_tx.send(RealtimeEvent::AssistantAudioDelta {
                turn_id: turn.turn_id.clone(),
                pcm,
            });
        }
    }

    async fn response_done(&mut self, raw: &Value) {
        let Some(turn) = self.current_turn.take() else {
            return;
        };
        let Some(response_id) = raw.pointer("/response/id").and_then(|v| v.as_str()) else {
            return;
        };
        if turn.turn_id != response_id {
            return;
        }

        let _ = self.events_tx.send(RealtimeEvent::AssistantTurnCompleted {
            turn_id: turn.turn_id.clone(),
            usage: token_usage_from(raw),
        });

        if turn.calls.is_empty() {
            self.current_response_id = None;
            return;
        }

        self.run_tool_batch(turn).await;
        self.current_response_id = None;
    }

    fn emit_error(&self, raw: &Value) {
        let message = raw
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown realtime error");
        if message.to_lowercase().contains("no active response") {
            return;
        }
        let _ = self.events_tx.send(RealtimeEvent::Error {
            error: message.to_string(),
            recoverable: true,
        });
    }

    async fn run_tool_batch(&mut self, turn: TurnState) {
        let mut executions = Vec::new();
        for (call_id, call) in &turn.calls {
            if !call.done && call.args_buffer.is_empty() {
                continue;
            }
            let args: HashMap<String, Value> = if call.args_buffer.is_empty() {
                HashMap::new()
            } else {
                match serde_json::from_str::<HashMap<String, Value>>(&call.args_buffer) {
                    Ok(a) => a,
                    Err(err) => {
                        let _ = self.events_tx.send(RealtimeEvent::ToolCallFailed {
                            call_id: call_id.clone(),
                            name: call.name.clone(),
                            error: err.to_string(),
                        });
                        continue;
                    }
                }
            };
            executions.push(ToolCallExecution {
                id: call_id.clone(),
                name: call.name.clone(),
                args,
            });
        }

        if executions.is_empty() {
            return;
        }

        let tools_vec: Vec<Box<dyn LlmTool>> = self
            .config
            .tools
            .as_ref()
            .map(|ts| ts.iter().map(|t| t.clone_box()).collect())
            .unwrap_or_default();

        for exec in &executions {
            let _ = self.events_tx.send(RealtimeEvent::ToolCallDispatched {
                call_id: exec.id.clone(),
                name: exec.name.clone(),
                args: exec.args.clone(),
            });
        }

        let ctx = ToolRunCtx {
            cancel: turn.cancel_token.clone(),
            correlation_id: Some(self.correlation_id.clone()),
            source: Some("RealtimeVoiceBroker".to_string()),
        };

        let outcomes = self.tool_runner.run_batch(&executions, &tools_vec, &ctx).await;

        for outcome in &outcomes {
            if outcome.ok {
                let _ = self.events_tx.send(RealtimeEvent::ToolCallCompleted {
                    call_id: outcome.id.clone(),
                    name: outcome.name.clone(),
                    result: outcome.result.clone().unwrap_or(Value::Null),
                });
            } else {
                let _ = self.events_tx.send(RealtimeEvent::ToolCallFailed {
                    call_id: outcome.id.clone(),
                    name: outcome.name.clone(),
                    error: outcome.error.clone().unwrap_or_default(),
                });
            }
        }

        if let Some(tracer) = &self.tracer {
            let ok = outcomes.iter().filter(|o| o.ok).count();
            let fail = outcomes.len() - ok;
            tracer.record_tool_batch(
                Uuid::new_v4().to_string(),
                executions.iter().map(|e| e.name.clone()).collect(),
                ok,
                fail,
                outcomes.iter().map(|o| o.duration_ms).sum::<u64>() as f64,
                Some("RealtimeVoiceBroker".to_string()),
                "RealtimeVoiceBroker",
                &self.correlation_id,
            );
        }

        let policy = self.config.on_interrupt.unwrap_or(defaults().on_interrupt);
        let to_submit: Vec<_> = if turn.cancelled {
            match policy {
                InterruptOutputPolicy::Drop => Vec::new(),
                InterruptOutputPolicy::Submit => outcomes.iter().collect(),
                InterruptOutputPolicy::SubmitCompletedOnly => {
                    outcomes.iter().filter(|o| o.ok).collect()
                }
            }
        } else {
            outcomes.iter().collect()
        };

        let mut submitted_ids = Vec::new();
        for outcome in to_submit {
            let output = if outcome.ok {
                serde_json::to_string(outcome.result.as_ref().unwrap_or(&Value::Null))
                    .unwrap_or_default()
            } else {
                json!({ "error": outcome.error.clone().unwrap_or_default() }).to_string()
            };
            let _ = self
                .gateway
                .send_event(&json!({
                    "type": "conversation.item.create",
                    "item": {
                        "type": "function_call_output",
                        "call_id": outcome.id,
                        "output": output
                    }
                }))
                .await;
            submitted_ids.push(outcome.id.clone());
        }

        if !submitted_ids.is_empty() {
            let _ = self.events_tx.send(RealtimeEvent::ToolBatchSubmitted {
                turn_id: turn.turn_id.clone(),
                call_ids: submitted_ids,
            });
            let _ = self.gateway.send_event(&json!({ "type": "response.create" })).await;
        }
    }
}

fn token_usage_from(raw: &Value) -> Option<TokenUsage> {
    let usage = raw.pointer("/response/usage")?;
    Some(TokenUsage {
        prompt_tokens: usage.get("input_tokens").and_then(|v| v.as_u64()).map(|n| n as u32),
        completion_tokens: usage.get("output_tokens").and_then(|v| v.as_u64()).map(|n| n as u32),
        total_tokens: usage.get("total_tokens").and_then(|v| v.as_u64()).map(|n| n as u32),
        extras: None,
    })
}

/// Build the vendor-specific ``session.update`` payload from a
/// vendor-neutral config. Matches the OpenAI Realtime GA shape.
pub fn build_session_update(config: &RealtimeVoiceConfig) -> Value {
    let modalities = config.modalities.clone().unwrap_or_else(|| defaults().modalities);

    let output_modalities: Vec<&str> = if modalities.contains(&RealtimeModality::Audio) {
        vec!["audio"]
    } else {
        vec!["text"]
    };

    let mut audio_input = serde_json::Map::new();
    audio_input.insert(
        "format".to_string(),
        encode_audio_format(config.input_audio_format.unwrap_or(defaults().input_audio_format)),
    );
    audio_input.insert(
        "turn_detection".to_string(),
        encode_turn_detection(
            config.turn_detection.as_ref().unwrap_or(&TurnDetectionMode::ServerVad),
        ),
    );

    if config.disable_input_audio_transcription {
        audio_input.insert("transcription".to_string(), Value::Null);
    } else if let Some(t) = config.input_audio_transcription.as_ref() {
        audio_input.insert("transcription".to_string(), json!({ "model": t.model }));
    }

    let mut audio_output = serde_json::Map::new();
    audio_output.insert(
        "format".to_string(),
        encode_audio_format(config.output_audio_format.unwrap_or(defaults().output_audio_format)),
    );
    if let Some(voice) = &config.voice {
        audio_output.insert("voice".to_string(), Value::String(voice.clone()));
    }

    let mut session = serde_json::Map::new();
    session.insert("type".to_string(), Value::String("realtime".to_string()));
    session.insert(
        "output_modalities".to_string(),
        Value::Array(output_modalities.iter().map(|s| Value::String(s.to_string())).collect()),
    );
    session.insert("audio".to_string(), json!({ "input": audio_input, "output": audio_output }));
    session.insert(
        "tool_choice".to_string(),
        encode_tool_choice(config.tool_choice.as_ref().unwrap_or(&RealtimeToolChoice::Auto)),
    );
    if let Some(instr) = &config.instructions {
        session.insert("instructions".to_string(), Value::String(instr.clone()));
    }
    if let Some(max) = config.max_response_output_tokens {
        session.insert("max_output_tokens".to_string(), Value::from(max));
    }
    if let Some(tools) = &config.tools {
        let tool_list: Vec<Value> = tools
            .iter()
            .map(|t| {
                let d = t.descriptor();
                json!({
                    "type": "function",
                    "name": d.function.name,
                    "description": d.function.description,
                    "parameters": d.function.parameters
                })
            })
            .collect();
        session.insert("tools".to_string(), Value::Array(tool_list));
    }
    if let Some(extras) = &config.provider_extras {
        for (k, v) in extras {
            session.insert(k.clone(), v.clone());
        }
    }

    json!({ "type": "session.update", "session": session })
}

fn encode_audio_format(fmt: RealtimeAudioFormat) -> Value {
    match fmt {
        RealtimeAudioFormat::Pcm16 => json!({ "type": "audio/pcm", "rate": 24000 }),
        RealtimeAudioFormat::G711Ulaw => json!({ "type": "audio/pcmu" }),
        RealtimeAudioFormat::G711Alaw => json!({ "type": "audio/pcma" }),
    }
}

fn encode_turn_detection(mode: &TurnDetectionMode) -> Value {
    match mode {
        TurnDetectionMode::None => Value::Null,
        TurnDetectionMode::ServerVad => json!({ "type": "server_vad" }),
        TurnDetectionMode::SemanticVad => json!({ "type": "semantic_vad" }),
        TurnDetectionMode::ServerVadCustom(vad) => encode_server_vad(vad),
        TurnDetectionMode::SemanticVadCustom(vad) => encode_semantic_vad(vad),
    }
}

fn encode_server_vad(vad: &ServerVadConfig) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), Value::String("server_vad".to_string()));
    if let Some(t) = vad.threshold {
        map.insert("threshold".to_string(), Value::from(t));
    }
    if let Some(p) = vad.prefix_padding_ms {
        map.insert("prefix_padding_ms".to_string(), Value::from(p));
    }
    if let Some(s) = vad.silence_duration_ms {
        map.insert("silence_duration_ms".to_string(), Value::from(s));
    }
    if let Some(c) = vad.create_response {
        map.insert("create_response".to_string(), Value::from(c));
    }
    if let Some(i) = vad.interrupt_response {
        map.insert("interrupt_response".to_string(), Value::from(i));
    }
    if let Some(t) = vad.idle_timeout_ms {
        map.insert("idle_timeout_ms".to_string(), Value::from(t));
    }
    Value::Object(map)
}

fn encode_semantic_vad(vad: &SemanticVadConfig) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), Value::String("semantic_vad".to_string()));
    if let Some(e) = &vad.eagerness {
        map.insert("eagerness".to_string(), Value::String(e.clone()));
    }
    Value::Object(map)
}

fn encode_tool_choice(choice: &RealtimeToolChoice) -> Value {
    match choice {
        RealtimeToolChoice::Auto => Value::String("auto".to_string()),
        RealtimeToolChoice::None => Value::String("none".to_string()),
        RealtimeToolChoice::Required => Value::String("required".to_string()),
        RealtimeToolChoice::Function(name) => json!({ "type": "function", "name": name }),
    }
}

#[allow(unused_imports)]
use crate::realtime::config::RealtimeVoiceConfig as _RealtimeVoiceConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_session_update_text_only() {
        let cfg = RealtimeVoiceConfig {
            modalities: Some(vec![RealtimeModality::Text]),
            instructions: Some("be brief".to_string()),
            ..Default::default()
        };

        let payload = build_session_update(&cfg);

        assert_eq!(payload["type"], "session.update");
        assert_eq!(payload["session"]["type"], "realtime");
        assert_eq!(payload["session"]["output_modalities"], json!(["text"]));
        assert_eq!(payload["session"]["instructions"], "be brief");
    }

    #[test]
    fn build_session_update_includes_voice() {
        let cfg = RealtimeVoiceConfig {
            voice: Some("verse".to_string()),
            ..Default::default()
        };

        let payload = build_session_update(&cfg);

        assert_eq!(payload["session"]["audio"]["output"]["voice"], "verse");
    }
}
