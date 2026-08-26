# Can a Claude Code channel return Claude's response?

_Researched 2026-08-26 against Anthropic's official Claude Code documentation._

## Conclusion

Yes. Claude Code channels can be two-way, but the channel notification protocol is only the inbound half. `notifications/claude/channel` pushes an event into Claude Code; it does not automatically stream the assistant's output back to the MCP server.

For the outbound half, the channel MCP server must:

1. Declare the standard MCP `tools` capability.
2. Expose a tool such as `reply(chat_id, text)`.
3. Include channel instructions telling Claude to call that tool and pass the routing ID from the inbound event metadata.
4. Deliver the tool arguments to the external client, for example over SSE or the chat platform's API.

Anthropic's reference includes a complete two-way webhook example where inbound messages use HTTP POST and replies stream over SSE. The official fakechat channel likewise returns Claude's reply to its browser UI.

## Important limitations

- A successful `mcp.notification()` only means the event was written to the transport. Claude Code does not acknowledge that it received or processed the notification.
- Replies are tool calls chosen by Claude, not a general assistant-output stream. The server instructions must tell Claude to use the reply tool.
- When Claude replies through a channel, the terminal shows the tool call and confirmation; the actual reply is delivered by the channel to the external client.
- Channels remain a research preview, so the contract may change.

## Effect on OnlyDiffs

OnlyDiffs uses its `message_id` metadata as the reply routing key and exposes an MCP `reply(message_id, text)` tool. Claude is instructed to call it once, after completing its work, with the complete final response. The authenticated Electron bridge waits for that tool call and then returns the whole reply to the UI at once; it does not stream partial token output or scrape Claude Code's terminal.

## Primary sources

- Anthropic, [Push events into a running session with channels](https://code.claude.com/docs/en/channels): states that channels can be two-way and that fakechat returns Claude's answer to the browser.
- Anthropic, [Channels reference: Notification format](https://code.claude.com/docs/en/channels-reference#notification-format): documents the lack of notification acknowledgement.
- Anthropic, [Channels reference: Expose a reply tool](https://code.claude.com/docs/en/channels-reference#expose-a-reply-tool): defines the tool capability, handlers, instructions, and complete HTTP/SSE two-way example.
