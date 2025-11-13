# Chat Sessions with Tools

Combine conversational context with tool calling for grounded, actionable responses.

Pattern:
1. Build messages
2. Send via broker
3. If tool call emitted, execute tool and append result
4. Continue until final answer
