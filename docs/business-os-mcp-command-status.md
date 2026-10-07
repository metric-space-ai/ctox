# Recovering a timed-out MCP action

`business_os.get_command_status` accepts the ordinary command ID. If an action
times out before its response delivers that ID, the same tool also accepts
`command_id: "mcp-request:<request_id>"`. Obtain the original request ID from the
MCP activity audit; an audit event marked `completed` only means its handler
returned, not that the coding turn succeeded.

The native reader resolves an exact request ID against the persisted command
correlation metadata, restricted to the originating MCP actor and workspace.
It reads at most two command IDs, without returning unrelated prompts or scanning
a bounded latest-record window in the client. Malformed selectors, missing
correlation and multiple matches fail closed. Multiple matches require an
explicit command ID; no command is selected by timestamp.

Once resolved, the ordinary status reader, collection permission policy and
Workjet owner visibility checks apply. The correlation table is a lookup index,
not execution authority: Core continues to own lifecycle state and queue links.
The lookup never dispatches, retries, cancels or changes the original command.
It does not manufacture a success or failure when correlation or a status record
is missing. Missing correlation can also mean the original request has not yet
persisted a command; do not treat it as permission to repeat a coding turn.

The selector uses the existing string argument, so consumers do not need an
additional tool or argument to perform the read after the native release is
deployed. Request IDs must be nonempty, at most 256 bytes, without surrounding
whitespace or control characters. OAuth credentials, grants and provider
accounts are unaffected.
