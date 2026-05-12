# OpenAI Tool Round-Trip Fixtures

These fixture files MUST be byte-identical across all four mojentic ports (ts/py/ex/ru).
If you change one file, you MUST update the corresponding file in all four ports.

## Scenario

User asks "What's the weather in Paris?"

1. `response-1-tool-call.json` — The LLM responds with a `get_weather(location="Paris")` tool call.
2. `tool-result.json` — The tool returns this JSON result to the broker.
3. `response-2-final.json` — After receiving the tool result, the LLM produces its final text response.
