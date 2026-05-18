# Realtime Voice

`mojentic::realtime::RealtimeVoiceBroker` opens duplex voice + tool
sessions against realtime-capable providers (currently OpenAI's
Realtime API over WebSocket). It mirrors the Python, Elixir, and
TypeScript ports — a long-lived broker, a per-session driver that owns
a WebSocket, and a vendor-neutral event enum you consume.

## 30-second example (text mode)

```rust,ignore
use mojentic::realtime::{
    RealtimeVoiceBroker, RealtimeVoiceConfig, RealtimeEvent,
    RealtimeModality, OpenAIRealtimeGateway, OpenAIRealtimeGatewayOptions,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway = Arc::new(OpenAIRealtimeGateway::new(
        OpenAIRealtimeGatewayOptions::default(),
    )?);

    let config = RealtimeVoiceConfig {
        modalities: Some(vec![RealtimeModality::Text]),
        instructions: Some("You are a concise assistant.".to_string()),
        ..Default::default()
    };

    let broker = RealtimeVoiceBroker::new("gpt-realtime", gateway, config, None, None);
    let mut session = broker.connect().await?;
    session.send_text("What's the capital of Canada?")?;

    while let Some(event) = session.next_event().await {
        match event {
            RealtimeEvent::AssistantTextDelta { delta, .. } => print!("{delta}"),
            RealtimeEvent::AssistantTurnCompleted { .. } => {
                println!();
                break;
            }
            RealtimeEvent::Error { error, .. } => {
                eprintln!("[error] {error}");
                break;
            }
            _ => {}
        }
    }

    session.close()?;
    Ok(())
}
```

## Events

`RealtimeEvent` is an enum with 23 variants spanning session lifecycle,
user speech, assistant output, tool calls, and control. Pattern match
on it in `match` arms.

## Tools

Pass tools (any `Box<dyn LlmTool>`) via
`RealtimeVoiceConfig::tools`. The session dispatches them through
`ParallelToolRunner` by default — multiple `function_call` items in
one assistant turn execute concurrently via `FuturesUnordered` +
`Semaphore` (capped at 4) and the results are batched back as
`function_call_output` items before the next `response.create`.

## Audio I/O

The library is hardware-free. Use `session.send_audio_frame(Vec<i16>)`
to push samples and consume `AssistantAudioDelta` events on the way
back. For live device I/O integrate a platform library (e.g.
`cpal`) at the boundary; the session API stays the same.

## Interruption

`session.interrupt()` cancels the in-flight turn and aborts running
tools by signalling their `CancellationToken`. Server-driven barge-in
(`input_audio_buffer.speech_started`) does the same automatically.

`RealtimeVoiceConfig::on_interrupt` controls what happens to tool
outputs already in flight when the cancel lands: `Drop` (default),
`SubmitCompletedOnly`, or `Submit`.
