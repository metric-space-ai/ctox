# Secret-backed native HTTP MCP

Prefer Codex's native Streamable HTTP transport with `http_headers_helper`
over a custom stdio transport. This package only supplies authentication
headers from a trusted local secret loader. Codex handles real initialization,
tool/resource messages, sessions, SSE, protocol errors and server redactions.
It does not expose a shell tool, raw Business OS HTTP, or caller-supplied role
context. No dependencies, native compilation or installs are needed by the
helper itself.

## Local helper interface

```text
/absolute/path/to/bun /absolute/installed/ctox-business-os-mcp/scripts/http-headers-helper.mjs --secret-loader /absolute/private/loader.mjs
```

This is a command **for Codex to launch with captured stdout**, not a terminal
diagnostic. Never run it with a real loader where stdout enters the task log,
terminal, shell history recording, a file, or any user-visible tool output.
Its successful stdout deliberately contains one JSON object with an
`Authorization` bearer header; that pipe is a credential channel. No token
belongs in argv, config environment, source, files or diagnostic logs.

The operator-controlled absolute `.mjs` module must export:

```js
export async function loadBearerToken() {
  // Retrieve the existing encrypted secret over the authorized local/SSH API.
  // Return the bare token string. Do not log or persist it.
}
```

The helper uses its own runtime (`process.execPath`) for the loader child.
Use Bun when the loader imports Bun-dependent ctox-dev TypeScript SSH code;
use Node 22+ for a standard Node-compatible loader. Runtime compatibility and
the production loader are operator-owned verification, not proven by the
fixture tests. The module and all its imports must be trusted local code,
not writable by tenant apps. This is process isolation, not a sandbox.

The child imports the module once, calls `loadBearerToken()` once, validates a
nonempty bearer token (at most 16 KiB, no spaces/control characters), and
returns the header through a captured pipe. Loader stdout/stderr never enters
the caller's logs: any stray stdout that invalidates the header JSON, any
stderr, failed import, invalid result or exception fails closed. The public
CLI emits only `ctox-mcp: header helper failed` on failure, with no raw error,
module output, arguments, stack or token.

The loader has 20 seconds, stdout is capped at 32 KiB and stderr at 8 KiB.
Stderr is counted but not retained. Timeout, cancellation or excess output
kills the owned child; Unix uses a dedicated process group so its ordinary
subprocesses are included. Windows terminates the immediate child only: loaders
must clean up their own subprocesses there. A malicious loader can escape a
process group; arbitrary imported code is outside this helper's security
boundary. The loader must not create detached jobs or write credentials to
disk. JavaScript strings are not guaranteed zeroizable and this does not
protect against privileged memory inspection or OS swap.

For an authorized ctox-dev SSH loader, `ctox secret get` returns JSON
`{ok, scope, name, value}`, not bare token text. Parse bounded captured output,
require `ok === true`, exact requested scope/name and a nonempty string
`value`, then return only `value`. Do not print the envelope or parser input.
An encrypted entry such as `business_os/codex_client_<tokenId>` stays native;
the local loader contains retrieval code, not the token. Token provisioning,
expiry, revocation and rollback are separate authorized operator actions.
Never mint or rotate credentials from the helper, including after a denial.

## Native Codex configuration

The official [MCP guide](https://learn.chatgpt.com/docs/extend/mcp) documents
`http_headers_helper` for local-environment HTTP connections (not stdio or
remote-executor connections). Its command is a **string, not an array**, as
listed in the [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).
Confirm support in the installed client before changing existing settings.

```toml
[mcp_servers.thesen-business-os]
url = "https://mcp.ctox.dev/mcp/thesen.ctox.dev"
http_headers_helper = "/absolute/path/to/bun /absolute/installed/ctox-business-os-mcp/scripts/http-headers-helper.mjs --secret-loader /absolute/private/loader.mjs"
startup_timeout_sec = 60
tool_timeout_sec = 90
```

Use installed durable paths, not a disposable worktree. The example paths have
no spaces; when paths require quoting, verify the installed client's command
parser without a production credential. Preserve existing enabled/disabled
tool lists and unrelated server entries. Do not add literal `http_headers`,
bearer environment variables or OAuth credentials as an incidental fallback;
explicit bearer/OAuth sources can take precedence over helper Authorization.
The helper cannot itself bind credentials to a URL: the operator must pair
this trusted loader with the exact authorized HTTPS MCP endpoint. Do not use
the local inbound token as a managed client token.

Codex caches helper headers for a connection. Its documented native behavior
may refresh the helper after a same-origin POST receives 401/403 and retry if
headers changed. This reloads the existing secret; it is **not** authorization
to rotate, broaden or mint tokens. A policy denial remains authoritative.
The generic helper does not implement or override network redirects, auth
retries, session lifecycle or business operations.

## Reload and live proof

Configuration or `mcp list`/redacted `mcp get` output alone proves no live
connection. Ask the user to restart/reload when the running app requires it.
If app automation refuses access for safety, respect that boundary; do not
bypass it through another UI tool, shell control or injected keystrokes.

After reload, use the configured native MCP channel for actual `initialize`,
`notifications/initialized`, `tools/list` and relevant advertised resources.
Verify an admitted read such as `business_os.list_modules`; use
`business_os.status` only with its required permissions. Record non-secret
instance/actor, advertised tool names/count, protocol/server identity and
allowed/denied results. Do not dump headers, the loader result or capabilities
containing credentials. Native tool counts are not the token-admitted gateway
catalog and are not a required client count.

Distinguish a missing client entry/catalog (configuration/reload),
`runtime_unavailable` (connector reachability), and authentication or
`business_os_policy` denial (identity/rights). Explicitly authorized SSH setup,
health diagnosis and connector repair are allowed. They must never become an
alternate route to perform a denied business action. Do not fabricate
`_context`, widen grants, expose generic CLI tools or bridge raw records over
HTTP. Preserve server policy errors and redactions.

The package tests use offline loaders only. Production SSH loading, token
bootstrap, machine-local config, user restart and live proof remain with the
operator; do not duplicate a working tenant configuration to test this package.

```text
greppy bash-smart -- node --test --test-concurrency=1 skills/ctox-business-os-mcp/test/http-headers-helper.test.mjs
```
