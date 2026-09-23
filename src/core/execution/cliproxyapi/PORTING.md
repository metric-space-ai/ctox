# CLIProxyAPI Rust port — control artifact

## State

- Upstream: `a88197f845c979132c8978ea223c6af05cc81536`
- Track A, standalone Rust-port release: **COMPLETE**
- Accepted pin: **617/617 production + 442/442 tests strict**, complete
- Upstream candidate `a88197f`: **111/111 reviews**, **10/10 gates**,
  promotion **YES**, post-promotion full gate **YES**
- Track B, CTOX provider integration: **tracked independently** in
  `src/core/execution/cliproxyapi_integration/provider-integration.json`; its open gates do not
  reduce or inherit Track A's percentage
- Historical accepted-pin capability ledger: **1,000 / 1,000 points**; this is
  explicitly not a project-completion percentage
- Current gate: none; Accepted Pin released, no open upstream candidate
- Scaffolds: not counted
- Owner of shared registry/module graph: CTOX integration lane

This file is the durable steering artifact. It records scope, evidence,
forensics, decisions and strategy changes. `port-map.json` is the mechanical
file ledger; this document is the semantic ledger. A point is awarded only
when its acceptance tests pass. Documentation, signatures, generated files and
ignored tests receive zero points.

The reusable method, forensic lessons and dashboard contract are documented in
[`RUST_PORTING_PLAYBOOK.md`](RUST_PORTING_PLAYBOOK.md).

## Track boundary adopted 2026-08-05

The project now has two release lanes with separate owners and evidence:

1. **Track A — portable CLIProxyAPI Rust port.** This directory owns the
   upstream pin, path mirror, protocol/provider implementation, candidate
   review, differentials and port-release gates. Its 100% value means only that
   the accepted upstream pin passed the frozen Rust-port release predicate.
2. **Track B — CTOX provider integration.** The outer host, encrypted secret
   store, Business OS account management and Pi provider/model selection are
   tracked in `src/core/execution/cliproxyapi_integration/README.md` and its JSON
   ledger. Track B has no synthetic percentage; every provider access mode has
   named capability gates including a real-account E2E gate.

The historical 1,000-point ledger below predates this boundary and includes
some CTOX integration work. It remains immutable forensic evidence and is not
used as the current completion metric for either lane. New upstream work must
not add Business OS, secret-store or Pi integration points to Track A; those
changes update Track B only.

## Frozen historical 1,000-point semantic ledger

| Capability | Points | Status |
|---|---:|---|
| Translator SDK: formats, contracts, registry, hooks, pipeline | 50 | complete; 6 parity tests |
| Raw-JSON/SSE compatibility substrate | 45 | complete; 8 focused tests |
| Protocol-neutral request/response/event contracts | 55 | complete; 1 contract test |
| OpenAI Responses family | 75 | complete for Responses→Claude vertical pair |
| Anthropic Messages family | 75 | complete for Claude→Responses vertical pair |
| OpenAI Chat Completions family | 60 | complete; Claude, Gemini, Antigravity and Codex vertical pairs |
| Gemini family | 60 | complete; native request, non-stream and stream response |
| Interactions family | 45 | complete; request, non-stream and stream in both directions |
| Codex subscription auth and executor | 80 | complete; OAuth, pooled non-stream/stream executor, server handler and exact local token count |
| Claude subscription auth and executor | 65 | complete; refresh, request, fingerprint, replay and persisted outcomes |
| Other subscription auth/executors | 65 | complete for bounded Antigravity auth, non-stream and native stream execution |
| Multi-account scheduling, cooldown and retry | 65 | complete; persisted outcomes plus upstream-parity routing strategies |
| HTTP/SSE server surface | 55 | complete; explicit Claude/Codex/Antigravity Responses dispatch with incremental SSE |
| CTOX typed config, SQLite and secret-store adaptation | 45 | complete; typed factories plus revisioned provider-independent host topology |
| Pi/Claude/Codex harness integration | 40 | complete; explicit Codex subscription and default CTOX-inherit both pass real Pi turns |
| Management/control API | 30 | complete; authenticated catalog/status plus transactional redacted runtime mutation |
| Safe replacement for Go/cgo plugin host | 25 | complete; bounded registered child calls over Unix sockets/Windows named pipes |
| Observability, redaction and policy | 25 | complete; production Error-only logging and root-scoped telemetry |
| Upstream sync, differential conformance and release gates | 40 | complete; 14 Claude + 12 Antigravity + 8 replay + 16 Interactions + 11 Codex-token + 10 Management-model + 1 plugin-schema parity fixtures + 1 named delta |
| **Total** | **1,000** | |

Project governance and the build skeleton are required work but deliberately
carry no semantic port points.

Worker 14e reconstructed the table from the immutable worker-point history.
The prior rendering omitted the 60-point OpenAI Chat Completions row and
mistyped the accepted HTTP/SSE allocation as 65 instead of 55. Those two
presentation defects offset to a hidden 50-point scope hole and made the shown
status rows sum to 865 even though the worker ledger correctly sums to 855.
No points changed: the remaining provider-family budget is exactly 60 Chat
Completions plus 40 native Gemini points.

## Gates

1. A ported unit compiles on stable Rust and has no unconditional `todo!()`.
2. Relevant upstream cases are translated into Rust tests or captured as an
   explicit, reasoned exclusion.
3. No-op JSON paths preserve bytes; mutation paths preserve missing/null/empty
   distinctions and raw tool arguments where the protocol requires them.
4. Streaming units test event order, termination, fragmented input and usage.
5. Subscription credentials never enter logs or ambient runtime environment.
6. `port-map.json` and every upstream `ref` use the same pinned commit.

Operator checks:

```sh
src/core/execution/cliproxyapi/scripts/check_tracking.sh runtime/cliproxyapi-upstream
cargo test -p ctox-cliproxyapi
cargo clippy -p ctox-cliproxyapi --all-targets -- -D warnings
```

The BoringSSL-backed fingerprint transport has a separate macOS execution
gate. A completed run is required; relocating a binary or changing only the
target directory is not itself evidence because the loader stall is
intermittent.

Worker completion protocol:

1. Update the relevant work item and append evidence in `project-state.json`.
2. Update this checkpoint log and semantic points only after its gate passes.
3. Run `scripts/update_project_artifacts.sh runtime/cliproxyapi-upstream`.
4. Keep `runtime/cliproxyapi-porting-dashboard.html` open in the CTOX browser;
   it reloads itself every 15 seconds after each generated update.

## Checkpoint log

### Checkpoint 1 — 5%

Completed at 50/1,000 points.

Evidence:

- The mirrored tree contains all 1,018 upstream Go files as `.rs` paths: 604
  production files and 414 tests. Scaffolds are excluded from the module graph.
- `cargo test -p ctox-cliproxyapi`: 6 passed, 0 failed.
- Covered upstream behavior: no-transform model normalization, native-before-
  plugin precedence, independent stream/non-stream capability reporting,
  suppression of raw fallback after an empty native stream transform,
  middleware order, and request-local stream state return.

Forensic findings:

1. **Cargo target discovery crosses the mirror boundary.** Mirrored
   `examples/*/main.rs` were treated as runnable examples and failed the first
   build. `autoexamples = false` now makes activation explicit. Future `cmd`
   binaries and benchmarks follow the same opt-in rule.
2. **Go package structure is not Rust file structure.** Exact paths are useful
   for upstream diffing, but compiling every scaffold would manufacture a huge
   invalid module graph. Rust package `mod.rs` files are CTOX-owned integration
   points; leaf files remain traceable to Go paths.
3. **Response direction is intentionally asymmetric.** Upstream registers the
   provider-to-client response transform opposite the request transform. The
   Rust registry preserves that behavior and tests it explicitly; ambiguous
   `from/to` call sites must use named types at the HTTP boundary.
4. **`*any` cannot become global erased state.** Rust uses a request-local
   `TranslationState`, moved through the pipeline and recovered after
   middleware completion. This prevents cross-request leakage.
5. **JSON byte identity matters earlier than expected.** Even the registry
   fallback promises an unchanged body when the model already matches.
   Provider bodies therefore cannot be broadly deserialize/serialize ports.
6. **Upstream churn is live.** The pin advanced from the earlier audit commit
   to `41fc5e1`, which added bounded auth refresh timeouts. Sync review must be
   a normal gate, not end-of-project cleanup.

Strategy adaptation after checkpoint 1:

- Checkpoint 2 ports the raw JSON/SSE compatibility substrate before any large
  provider converter. This supplies the invariants that 800–1,200-line gjson/
  sjson translators depend on.
- Large Go converter files will be decomposed behind the same mirrored file
  facade into pure Rust primitives, then differential-tested against upstream.
- Provider work will proceed as complete vertical protocol pairs (request,
  non-stream response, stream response, usage, tools), never as a directory of
  mechanically translated but unexecutable bodies.
- Cargo examples, bins, tests and benchmarks are explicitly enabled only when
  their acceptance gate is active.

### Checkpoint 2 — cumulative 15%

Completed at 150/1,000 points.

Evidence:

- `cargo test -p ctox-cliproxyapi`: 13 passed, 0 failed (6 registry/
  pipeline parity tests plus 7 JSON, SSE and protocol-contract tests).
- The incremental SSE decoder accepts arbitrary chunk boundaries, CRLF and
  multi-line `data` fields. The encoder retains upstream's exact event shape.
- Token-count JSON and raw-array joining have byte-exact upstream fixtures.
- Raw tool arguments retain lexical representation such as `1.00` instead of
  passing through `serde_json::Value`.
- Missing, explicit null and concrete values are separate Rust states.
- The protocol contract covers text, images, files, tools, tool results,
  reasoning, usage and ordered streaming lifecycle events.

Forensic findings:

1. **A canonical IR helps, but cannot be universal.** It removes repeated
   interpretation for ordinary messages and usage, while Claude signatures,
   cache controls, Gemini thought signatures and unknown extensions need raw or
   provider-specific carriers. Forcing those into generic fields would be a
   lossy rewrite disguised as progress.
2. **Streaming is a transport grammar before it is a model grammar.** Upstream
   translators repeatedly parse `data:` chunks themselves. Central incremental
   SSE framing is safer in Rust, then provider adapters consume complete events.
   WebSocket frames will use the same normalized event layer, not the SSE parser.
3. **`serde_json::Value` is acceptable only at mutation boundaries.** It changes
   lexical numbers, whitespace and potentially object ordering. Raw arguments
   and unknown extensions remain byte-backed; no-op paths return the original
   allocation's bytes.
4. **The Rust standard-library surface still needs an MSRV gate.** Slice
   `split_once` compiled as unstable on the repository toolchain and was
   replaced by an explicit delimiter search. Future generated code may not use
   nightly-only convenience APIs.
5. **File status and semantic progress are different ledgers.** Some shared
   facilities are `adapted_to_ctox`, not literal ports. `port-map.json` reports
   that distinction; the 1,000-point ledger reports accepted capabilities.
6. **Fifteen percent is foundation, not production readiness.** No HTTP listener,
   subscription refresh, account scheduler or complete provider pair exists
   yet. The current crate is intentionally not wired into the CTOX daemon.

Strategy challenge and adaptation after checkpoint 2:

- Do **not** start a broad translator wave. The next gate is a differential
  conformance runner that executes identical fixtures through pinned Go
  upstream and Rust and compares JSON semantics plus exact SSE order.
- The first provider vertical slice is OpenAI Responses client ↔ Claude Messages
  subscription: request, non-stream response, streaming response, tools,
  reasoning, usage, cancellation and token refresh as one deployable path.
- The IR is used only for demonstrably lossless fields. Provider-specific raw
  carriers and direct conversion helpers remain allowed and must have fixtures.
- No second provider pair begins until the first pair passes upstream fixtures,
  a local loopback server smoke test and credential-redaction tests.
- Upstream sync becomes a small recurring review: change pin, regenerate map,
  classify changed ported/adapted files, run differential gates, then merge.
  There is no automatic production fetch.

## Checkpoint 3 proposal (completed and superseded)

1. Differential streaming fixtures with mixed reasoning/text/tool blocks.
2. Claude signature provenance and compatibility classifier.
3. Fragmented/malformed SSE plus cancellation tests.
4. Codex/Claude subscription refresh behind CTOX secrets with bounded timeout.
5. Credential-redaction tests before a loopback HTTP smoke.

### Checkpoint 3 — cumulative 25%

Completed at 250/1,000 points. The three partial ledger allocations are
OpenAI Responses 35/75, Anthropic Messages 35/75 and differential conformance
30/40.

Evidence:

- Four fixtures execute the same request/non-stream converters inside the
  pinned Go package and the Rust binary; canonical JSON results are identical.
- Five Rust converter tests cover sanitized tool IDs, tool adjacency, media,
  cache control, root-union schema normalization, reasoning, tool arguments,
  namespace restoration, cache-token accounting and nine ordered SSE events.
- Go probe files are copied into the ignored upstream checkout only for the
  bounded test and removed by a trap; production never compiles or runs Go.
- Request and response source files remain `partial`, not `ported`, because
  signature compatibility and complex interleaving cases are still absent.

Forensic findings:

1. **The differential gate found a semantic bug immediately.** Rust initially
   omitted upstream's approximate `reasoning_tokens` usage detail. The
   four-fixture comparison failed until that field was restored.
2. **Nondeterminism needs an explicit comparison policy.** Upstream generates a
   request metadata user ID and wall-clock `created_at`. The runner removes only
   those named fields; all remaining response structure is compared.
3. **A vertical slice is more informative than file completion.** Two large Go
   converters and their explicit registration facade are correctly marked
   `partial`, while a deployable semantic subset is testable end to end.
4. **SSE is structurally tested but not yet differentially tested.** The Rust
   path proves event names, order, sequence numbers and usage, but complex
   reasoning/text/tool interleavings must enter the Go/Rust corpus before HTTP.
5. **Provider signatures remain the highest correctness risk.** Claude
   encrypted reasoning signatures are deliberately not forwarded until the
   upstream compatibility classifier is ported and differentially gated.

Strategy adaptation after checkpoint 3:

- Expand the differential runner to streaming event arrays, including multiple
  text, reasoning and tool blocks with contiguous output indices.
- Port signature provenance/compatibility before crediting more request-family
  points; do not pass arbitrary encrypted content through to Claude.
- Add malformed/fragmented SSE and cancellation cases at the converter facade.
- Subscription refresh and loopback HTTP remain blocked until these gates and
  credential-redaction tests pass.

### Checkpoint 4 — cumulative 32%

Completed at 320/1,000 points. The incremental allocations are differential
conformance +10 (40/40), OpenAI Responses +25 (60/75), Anthropic Messages +25
(60/75) and the transport-facing SSE facade +10 (10/65).

Evidence:

- Eight fixtures run inside both the pinned Go package and the Rust binary.
  They now include canonical streaming event arrays, a mixed
  reasoning→text→tool lifecycle, Claude reasoning replay and rejection of GPT
  encrypted content on a Claude target.
- Thirty-three Rust tests pass: 11 library/signature tests, 8 raw JSON/SSE and
  protocol tests, 7 Responses↔Claude tests and 7 registry/pipeline tests.
- Claude E/R, double-layer R and CAIS envelopes are structurally inspected;
  GPT/Codex Fernet-shaped payloads, mismatched provider prefixes and unknown
  prefixes cannot be replayed as Claude thinking blocks.
- `ClaudeResponsesStreamDecoder` accepts transport fragments and CRLF, ignores
  comments and malformed JSON without advancing converter state, flushes an
  unterminated final event, and observes `TranslationContext` cancellation.
- `cargo fmt --check`, the full crate suite, Clippy with `-D warnings`, the
  eight-fixture differential gate and tracking validation are green.

Forensic findings:

1. **Streaming parity found two real semantic defects.** The first Rust stream
   emitted an empty `output_tokens_details` object that Go omits without
   reasoning. The mixed fixture then showed reasoning finalization at
   `message_stop` instead of its `content_block_stop`, and text finalization
   after rather than before a following tool block. Both are now event-identical.
2. **Signature compatibility is a protocol boundary, not a field mapping.** A
   string prefix is insufficient: Gemini and Claude may share an `E`-shaped
   base64 prefix. The Rust subset therefore validates the decoded protobuf tree
   and preserves only provider-native content. The file remains `partial`
   because Gemini decisions and every Claude validation option are not ported.
3. **Transport state and model state must be separate.** The incremental SSE
   decoder owns line framing; the Claude converter only receives complete data
   payloads. Malformed transport input can no longer partially mutate model
   aggregation state.
4. **Cancellation needs gates at both levels.** The generic registry suppresses
   a native transform when already cancelled, while the provider facade checks
   between decoded events. Actual socket/task cancellation still belongs to the
   future async server and earns no credit here.
5. **The file mirror remains useful but not executable architecture.** Three
   signature files moved from scaffold into the Rust module graph, while the
   many provider files remain inert. Semantic tests, not `.rs` file count,
   continue to determine the 32% figure.

Strategy challenge and adaptation after checkpoint 4:

- Do **not** begin the HTTP server just because one happy-path stream is green.
  Finish the remaining Responses↔Claude semantics first: multiple assistant
  text/reasoning segments, citations/refusals, provider error events and
  explicit registry activation.
- Port subscription auth in two isolated lanes (Claude and Codex), each through
  typed CTOX config/secrets with bounded refresh, redaction and expiry tests.
  Auth must not be coupled to the format converter or ambient environment.
- Add the loopback HTTP smoke only after provider errors and auth redaction have
  executable gates. WebSocket work remains separate; SSE success is not counted
  as WebSocket coverage.
- Before the next semantic allocation, advance or explicitly revalidate the
  upstream pin and classify changes touching the now-active converter,
  signature and registry files.

### Checkpoint 5 — cumulative 34%

Completed at 340/1,000 points. The checkpoint was deliberately reduced from a
planned 35% to 34%; OpenAI Responses and Anthropic Messages each receive 70/75,
not full credit.

Evidence:

- Eleven Go/Rust fixtures are identical. New cases cover legacy reasoning
  effort→budget mapping, Text→Function→new Text with distinct message IDs,
  multiple reasoning items, contiguous output indices and citation annotations.
- Thirty-five Rust tests pass. The explicit `register_pair` integration test
  activates client→provider requests and provider→client stream/non-stream
  responses without relying on Go package initialization.
- A terminal Claude `error` produces `response.failed` and suppresses a later
  duplicate `message_stop`. This is a tested CTOX correction, not claimed as
  upstream parity: the pinned Go Responses converter currently returns no event
  for that input and the native-empty rule would otherwise hide the failure.
- Formatting, Clippy `-D warnings`, tracking validation and the 11-fixture
  differential runner are green.

Forensic findings:

1. **One accumulator cannot represent Responses output.** After a function
   call, upstream begins a new assistant message with a new ID and output index;
   multiple reasoning items are also independent. Rust now stores finished
   items by output index and keeps only the currently open item mutable.
2. **Consecutive text blocks are not necessarily separate messages.** Claude
   server-tool and citation blocks may sit between text chunks while upstream
   intentionally keeps one message open. The differential citation fixture
   pins that aggregation rule.
3. **Go `init()` hid a direction invariant.** Request transforms are keyed
   client→provider, responses provider→client. `register_pair` encodes both
   directions atomically and makes activation visible to CTOX.
4. **Strict upstream parity can preserve an upstream failure.** The Go
   Responses converter silently drops Claude error events. Rust carries a
   narrow, documented `response.failed` delta because suppressing an upstream
   error is unacceptable at the durable harness boundary.
5. **The original 35% target was too optimistic.** Adaptive Claude 4.6 effort
   needs model-capability data, while `server_tool_use` and
   `web_search_tool_result` still lack a complete output lifecycle. Ten points
   remain unawarded rather than being hidden inside green happy-path tests.

Strategy adaptation after checkpoint 5:

- Close the remaining provider-pair gap through an injected typed model
  capability contract; do not import the full global Go registry into the
  converter.
- Decide and fixture the Responses representation of Claude server tools before
  auth. Citation-only preservation is not equivalent to a server-tool item.
- Keep the `response.failed` deviation in a named CTOX-delta test and re-check it
  whenever upstream adds native error handling; remove the delta when upstream
  becomes equivalent.
- Only after those ten points: start Claude subscription refresh through CTOX
  secrets, with bounded timeout, expiry and redaction tests. HTTP remains later.

### Checkpoint 6 — cumulative 35%

Completed at 350/1,000 points. OpenAI Responses and Anthropic Messages each
reach their 75-point vertical-pair allocation.

Evidence:

- A typed Rust model-capability subset ports the active Claude thinking levels
  for Sonnet/Opus 4.6–5 and normalizes model suffixes without mutable global
  converter state.
- Adaptive `xhigh` on Claude Sonnet 4.6 maps to `thinking.type=adaptive` and
  `output_config.effort=max`; unknown models retain the legacy budget mapping.
  Both paths run inside the Go/Rust differential corpus.
- A server-tool fixture pins upstream's actual contract: `server_tool_use` and
  `web_search_tool_result` are hidden, consecutive text remains one message,
  citations survive, and the next client function call receives the next
  contiguous output index.
- The runner now compares 13 parity fixtures and separately asserts one named
  CTOX delta: Go stops after `response.in_progress` for a Claude error while
  Rust emits the required terminal `response.failed`.
- Thirty-seven Rust tests, formatting, Clippy `-D warnings`, tracking and the
  extended differential runner are green.

Forensic findings:

1. **Model capability belongs outside the converter.** The first legacy budget
   implementation was correct only for unknown/older models. A small typed
   registry seam now supplies adaptive levels; the eventual complete model
   catalog can replace its partial static table without changing conversion.
2. **Server-tool “support” means deliberate invisibility here.** Upstream does
   not expose Claude's native web-search call as a Responses output item. The
   observable contract is text/citation preservation and zero index
   consumption, now differentially pinned.
3. **Expected divergences need executable governance.** Removing the error
   fixture from comparison would let either side drift. The runner instead
   asserts the precise event-name difference and will fail when upstream gains
   native error handling, forcing a deliberate delta review.
4. **The first provider pair is complete, not the whole proxy.** Auth,
   executors, scheduling, server surfaces, other formats and CTOX wiring remain
   in the 650 unearned points.

Strategy adaptation after checkpoint 6:

- Begin Claude subscription auth as its own capability package. Tokens enter
  through typed secret handles; converter and registry APIs never receive a
  refresh token.
- Port pure expiry/refresh decision logic and redacted error types before any
  HTTP client. Then add a bounded transport trait with deterministic tests.
- Reuse the same auth boundary for Codex only after Claude's refresh lifecycle
  passes expiry, cancellation, timeout and redaction gates.

### Checkpoint 7 — review in progress at 39%

Credited so far: 40/65 Claude subscription-auth points.

Evidence:

- `SecretString` zeroizes its allocation on drop, redacts both `Debug` and
  `Display`, and deliberately has no generic serialization implementation.
- Typed secret handles carry only scope, name and access/refresh kind; an empty
  secret or invalid handle is rejected before transport or persistence.
- `ClaudeTokenData` implements deterministic expiration and the upstream
  four-hour Claude refresh lead from `sdk/auth/claude.go`.
- Four focused tests and library Clippy with `-D warnings` pass. A shared Cargo
  target lock was avoided by moving this port's verification to its isolated
  ignored runtime target directory.
- The refresh coordinator ports the upstream 30-second bound, three-attempt
  retry shape, 1s/2s retry delays, `Retry-After`/`Retry-After-Ms` clamping and
  immediate 429 replay blocking.
- Concurrent requests for the same credential share one transport call. Rust
  keys singleflight and cooldown state by a redacted SHA-256 fingerprint rather
  than retaining the raw refresh token as Go currently does.
- The Anthropic refresh JSON request and token-rotation response parser are
  covered by six additional tests; response bodies are zeroized and never
  included in typed errors.
- The concrete async transport matches the pinned Go `uTLS v1.8.2`
  `HelloChrome_Auto` resolution with a Chrome-133 BoringSSL/TLS and HTTP/2
  profile. Browser navigation headers are removed so only upstream's
  `Content-Type`/`Accept` contract is sent.
- Proxy discovery is never ambient: no proxy means direct transport; a proxy
  arrives only as typed configuration and build failures never echo its URL or
  embedded credentials. Native wreq retries are disabled so the coordinator is
  the sole retry owner.
- A real loopback POST proves request path, headers, body, response parsing and
  token rotation. Twelve focused Auth tests and library Clippy are green.
- The portable store boundary now loads the access/refresh pair from one
  consistent snapshot and requires atomic pair rotation. The CTOX adapter maps
  this to encrypted SQLite records without environment fallback.
- Three Root tests use the real encrypted store. A trigger deliberately rejects
  the refresh-token update after the access-token update was attempted; both
  original credentials remain, proving transaction rollback rather than only
  a successful happy path.
- The mirrored `claude_executor_auth.rs` now owns load→refresh→persist and an
  explicit 401 trigger. Refresh runs in an owned Tokio task, so caller
  cancellation does not interrupt credential rotation (the Rust equivalent of
  upstream's cancellation-detached refresh context).
- Four executor tests cover rotation, 401-only triggering, cancellation and
  redaction. The feature-minimal suite passes 52/52 tests; the standalone
  default suite passes 54/54 including the two fingerprint-transport tests.

Forensic transport finding:

- Enabling wreq's `prefix-symbols` on macOS made Rust expect prefixed BoringSSL
  symbols while its static archives exported ordinary symbols; that feature is
  documented for Linux/Android collision avoidance and is therefore not used
  on this macOS gate.
- Fresh child, build-script and 5 GB root test binaries have intermittently
  stalled before `main` in macOS `dyld`, including after relocation to
  `/Volumes/tmp`; this is not specific to BoringSSL. All affected files carried
  `com.apple.provenance`, and removing that attribute only from ignored Cargo
  targets made the same binaries execute normally.
- The Rust Auth core and CTOX encrypted-store adapter do not require the native
  fingerprint stack. `anthropic-fingerprint-transport` is now an explicit
  child-crate feature, enabled for the standalone proxy but disabled in the
  CTOX host dependency. This keeps provider-specific BoringSSL out of the main
  daemon while retaining an independently gated transport implementation.
- A standalone transport process remains a candidate if macOS cannot execute
  the BoringSSL profile reliably. Until that release gate is repeatable, the
  successful loopback run counts as implementation evidence but not as proof
  that every newly linked macOS artifact is operational.

Remaining checkpoint gate:

1. The remaining 25 Claude-auth points cover actual request credential
   injection plus scheduler/account-state integration; they are not implied by
   the completed refresh lifecycle.

Checkpoint 7 closed at 390/1,000 after the 13 parity fixtures, the named CTOX
error delta and tracking gate passed again.

Forensic findings:

1. **Credential pairs are one consistency unit.** A per-secret Rust trait looked
   natural but allowed mixed access/refresh generations. The accepted boundary
   loads a snapshot and rotates both encrypted records in one SQLite
   transaction; an induced failure proves rollback.
2. **Cancellation ownership cannot be copied mechanically.** Upstream detaches
   refresh from request cancellation. Rust must give the operation an owned
   Tokio task; merely awaiting a future under singleflight lets a dropped leader
   strand followers or abandon persistence.
3. **Native fingerprinting is an implementation island.** Pulling BoringSSL
   into the CTOX daemon only to share Auth types inflated and destabilized every
   root link. The portable core is feature-independent; the standalone proxy
   owns the Chrome-profile transport feature.
4. **macOS loader stalls were initially misattributed to TLS.** Plain Cargo
   build scripts exhibited the same pre-main stall. The common factor was
   `com.apple.provenance` on regenerated target artifacts; removing it only from
   ignored targets made the same binaries execute.
5. **File completion remains a poor progress metric.** Only 20/1,018 mirrored
   files are non-scaffolds while 39% of the semantic ledger is accepted. The
   inverse will also occur for large low-value surfaces, so points stay tied to
   executable capabilities.

Strategy adaptation after checkpoint 7:

- Finish the Claude vertical execution path before reusing Auth for Codex:
  request credential injection, provider headers, one unauthorized replay and
  persisted account outcome.
- Extract only the provider-neutral conductor behavior demonstrated by that
  path. Do not port the large Go scheduler wholesale before a real provider
  exposes the required state machine.
- Keep a two-matrix gate: `--no-default-features` proves the embeddable Auth
  core; default features prove the standalone fingerprint transport.
- Do not broaden the daemon dependency to native TLS. If direct in-process
  fingerprint transport is later required, promote it only with a repeatable
  root release gate or isolate it behind the proxy process boundary.

### Checkpoint 8 — closed at 45%

Credited: 60 checkpoint points: request credential boundary, real Messages
transport, provider-neutral unauthorized replay core, persisted cooldown state
and deterministic account selection plus request-outcome feedback.

Evidence:

- `ClaudeCredentialMode` replaces upstream's implicit distinction between an
  `api_key` attribute and an access token in untyped metadata.
- The exact upstream origin rule is preserved: only API-key mode targeting
  HTTPS `api.anthropic.com` uses `x-api-key`; OAuth, custom authorities,
  non-HTTPS and even explicit `:443` use Bearer authorization.
- The mutation always names the conflicting header to remove, so stale
  `Authorization` and `x-api-key` values cannot coexist.
- Header values remain `SecretString`; `Debug` is redacted and invalid/CRLF
  targets are rejected before the transport receives credentials.
- Four focused tests and no-default-feature Clippy with `-D warnings` pass.
- A feature-isolated Chrome-133/BoringSSL transport now sends an actual bounded
  `POST /v1/messages?beta=true`. Loopback evidence covers Bearer auth,
  Anthropic version/beta headers, request bytes, response status and body.
- The Claude subscription executor composes encrypted-store load, request,
  401 refresh, atomic persistence and exactly one rebuilt request using the new
  access token. A second 401 is returned after two attempts; timeouts do not
  trigger credential churn.
- The replay budget itself lives in the mirrored provider-neutral conductor
  file and has no Claude token knowledge. Two policy tests and three Claude
  execution tests guard the split.
- The complete matrices now pass 62/62 tests without native TLS and 65/65 with
  the fingerprint transport. Both feature-minimal and default Clippy gates are
  green with `-D warnings`.
- The mirrored cooldown record stores provider/account/model identity, retry
  and quota recovery timestamps, backoff and a redacted error projection. It
  deliberately excludes credentials and upstream auth-file paths.
- CTOX adapts upstream's per-auth `.cds` files to one typed SQLite payload. A
  real host roundtrip proves stable snapshot ordering and deletion; invalid
  snapshots are rejected before a write can replace accepted state.
- The first selector slice preserves upstream's high-priority-first and sorted
  round-robin order. Future account/model cooldowns block, expired timestamps
  unblock, and an unavailable state without a retry timestamp does not become
  an accidental permanent ban. Ten new focused tests pass; feature-minimal
  Clippy remains green with `-D warnings`.
- A second isolated Root test attempt was stopped in an unrelated, pre-main
  model-crate build script while other repository builds occupied the shared
  machine. The adapter's real SQLite roundtrip completed green; the portable
  validation-before-write behavior is independently covered in the core.
- Claude responses now retain a typed retry delay parsed from `Retry-After` or
  `Retry-After-Ms`; response bodies remain zeroized and are never copied into
  cooldown records. The actual loopback transport proves the header boundary.
- The provider-neutral conductor applies upstream's account windows: explicit
  429 delays, 1s exponential quota fallback capped at 30m, reuse of a still-open
  quota window, 30m terminal auth/payment cooldown, 12h not-found and 1m
  transient cooldown. Invalid request shapes do not penalize a credential.
- The Claude executor records only the final post-replay response for the
  selected account/model. Its integration test proves that a 7s provider hint
  becomes an absolute SQLite-ready deadline and that no response body enters
  persisted state. The portable matrix now passes 78/78 tests; the two focused
  default transport tests pass as well.
- `AccountRouter` reloads the durable cooldown snapshot before every pick and
  fails closed on a store read error. `ClaudeSubscriptionAccountPool` accepts
  only executors whose bound account ID matches their candidate; it never owns
  tokens or secret handles.
- A composed core scenario selects account A, persists its explicit 429 window,
  removes it from the current attempt budget, selects account B and returns its
  200 response. Request-shape failures stop rather than churn credentials. Two
  router tests, one pool test and feature-minimal Clippy are green.
- The same A:429→B:200 path now runs through two real loopback HTTP
  connections and the Chrome/BoringSSL client. Captured wire headers prove
  `Bearer access-a` on the first POST and `Bearer access-b` on the second; the
  final outcome names B while the durable snapshot contains only A's absolute
  seven-second quota deadline.
- Request fingerprints now match the pinned Claude Code baseline: a stable
  one-hour session UUID keyed by a SHA-256 credential fingerprint, a fresh
  client-request UUID for the first-party Anthropic origin, Claude Code 2.1.63,
  Stainless package 0.74.0/runtime v24.3.0, typed OS/architecture fields,
  `X-Stainless-Timeout: 600`, keep-alive and the non-stream compression offer.
  IDs and credentials are redacted from `Debug`; profile values reject control
  characters before reaching HTTP.
- The final gate passes 83/83 tests without native TLS and 88/88 with the
  fingerprint transport. Both all-target Clippy matrices pass with
  `-D warnings`; 13 Go/Rust parity fixtures, the named CTOX stream-error delta
  and tracking against `41fc5e1` are green.

Forensic findings:

1. **Outcome metadata must cross the transport boundary.** Upstream's Claude
   executor turns non-2xx responses into a status error and drops response
   headers, although the conductor can consume `RetryAfter`. Rust retains only
   the typed delay and zeroizes the body, allowing an exact provider window
   without persisting error payloads. This is an intentional CTOX correctness
   adaptation, not accidental byte parity.
2. **A SQLite transaction is not a multiwriter protocol.** Replacing the typed
   payload is atomic, and `CooldownConductor` prevents lost updates inside one
   process. Two independent proxy processes could still race load/modify/save.
   CTOX therefore remains the single runtime-state writer until the payload
   API gains revisions/CAS.
3. **Retry ownership changes at the first streamed byte.** The current transport
   buffers its response, so A:429→B:200 is safe. A true SSE path may switch
   accounts only during upstream bootstrap; once an event reaches the client,
   retrying would duplicate or reorder output.
4. **Session stability currently matches upstream non-home mode, not home KV.**
   The one-hour credential-keyed cache survives concurrent requests but not a
   process restart. Cross-restart identity should use typed CTOX runtime state,
   never a new environment toggle, if product requirements demand it.
5. **The accepted selector is deliberately smaller than upstream.** It covers
   priority, deterministic round robin, per-model cooldowns and expiry. Model
   suffix canonicalization, weighted scheduling, mixed providers, pinned
   sessions and websocket preference remain unearned.
6. **Request-scoped classification needs expansion before provider breadth.**
   400/422 stop account churn, but 404 model-support nuance and provider-specific
   errors still require the upstream classifier. A broad scheduler port would
   hide this gap behind more state rather than resolve it.

Strategy adaptation after checkpoint 8:

- Build a typed CTOX runtime factory next: provider accounts, secret handles,
  cooldown store, selector, transport and bounded timeouts become one validated
  construction path with no ambient proxy or token configuration.
- Put the accepted OpenAI Responses→Claude vertical pair behind the first real
  HTTP route after that factory. Prove non-stream conversion end to end, then
  add an explicit SSE bootstrap boundary before streaming output.
- Delay Codex subscription auth until the Claude path is reachable through the
  server surface. Reusing Auth abstractions before that would test libraries,
  not a usable proxy.
- Keep CTOX as the single cooldown writer. Add revisioned/CAS payload semantics
  only when a second process is intentionally introduced.

### Checkpoint 9 — closed at 55%

Completed at 550/1,000 points. The final 45-point increment credits 35/80
Codex subscription-auth points and another 10/45 CTOX typed-config/store
points. It does not claim Codex request execution or Responses streaming.

Evidence for worker 9a:

- Strict `serde(deny_unknown_fields)` runtime configuration carries only typed
  secret references, account policy, upstream target, optional device profile
  and bounded request timeout; access and refresh handles must be distinct.
- `CtoxCliproxyRuntimeFactory` composes the encrypted CTOX secret store,
  per-account Claude auth/executor, account-specific target/profile, persisted
  SQLite cooldown conductor and deterministic router without token or proxy
  environment fallback.
- Four portable configuration tests and the root integration test
  `typed_runtime_factory_composes_ctox_secrets_and_cooldown_store` pass. The
  root gate used the repository's existing
  `CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS=1` build-only switch after proving that
  macOS provenance blocked a newly generated, unrelated GGML CMake feature
  probe before `main`.
- `sdk/api/handlers/openai/openai_responses_handlers.rs` and
  `internal/api/server.rs` are active partial ports. The bounded HTTP/1.1
  listener accepts `POST /v1/responses`, rejects oversized/unsupported framing,
  translates the request, executes the configured Claude account pool and
  returns translated OpenAI Responses JSON.
- The real TCP loopback proves the complete downstream request → Claude pool →
  translated response path. It also proves that the access token is absent
  from the translated payload. Provider error bodies are not reflected into
  client errors.
- Like upstream, the non-stream format-conversion route requests Claude SSE
  internally and aggregates it. This avoids introducing a second conversion
  authority for Anthropic's native non-stream JSON shape.
- Current gates: 92/92 no-default tests, 97/97 default tests, both
  `cargo clippy --all-targets -- -D warnings` matrices, 13 differential parity
  fixtures plus one named CTOX error delta, and tracking all pass.
- The streaming route now has an explicit bootstrap result type. Pool and
  provider failures before bootstrap return ordinary HTTP status and JSON.
  Once the server writes `200 text/event-stream`, conversion errors terminate
  only through ordered Responses SSE; arbitrary provider error messages are
  replaced with a stable redacted message while the safe provider error code
  is retained.
- A real downstream TCP stream verifies header commit, event ordering and the
  absence of credentials. Updated gates: 95/95 no-default tests and 100/100
  default tests, both Clippy matrices and differential conformance pass.
- This worker does **not** claim incremental upstream transport. The current
  Claude transport buffers the provider body before bootstrap. The type
  boundary intentionally makes replacement with a cancelable chunk source the
  next isolated task without changing downstream commit semantics.
- Worker 9d replaces that limitation with a bounded channel fed directly by
  wreq's response byte stream. Dropping the receiver cancels the pump through
  channel closure and releases the owned HTTP response.
- `message_start` is now the explicit retry/commit threshold. Fragmented SSE
  is buffered only up to a 1 MiB bootstrap cap. A provider error or transport
  failure before `message_start` becomes a persisted synthetic 502 and the
  pool may select another account; a later failure cannot switch accounts and
  instead emits redacted `response.failed` while persisting cooldown for
  subsequent requests.
- The native loopback withholds the chunked response tail until the client has
  already received the bootstrapped `message_start`, proving the transport is
  incremental rather than merely split after buffering.
- Gates after 9d: 99/99 no-default tests, 105/105 default tests, both Clippy
  matrices, the root typed-runtime integration test, differential conformance
  and tracking all pass.
- Codex PKCE generation preserves upstream's 96 random bytes and S256
  challenge. The authorization URL carries the same OpenID/offline scopes,
  simplified-flow switches, redirect URI and public client ID; the verifier is
  never embedded in the URL or rendered by `Debug`.
- JWT claim inspection recovers ChatGPT account ID and email only after a
  successful token response. It deliberately does not pretend to verify a JWT
  signature and never includes a malformed token in its errors.
- Refresh uses the exact form-encoded OpenAI contract, an explicit 30-second
  transport timeout and SHA-256-keyed cross-instance singleflight. Other
  failures follow upstream retry timing; a structured
  `refresh_token_reused` response stops after one call without reflecting the
  provider body.
- The host-integrated auth lifecycle runs load → refresh → atomic persist in a
  detached task. This preserves upstream's cancellation-independent refresh
  intent while keeping ID, access and refresh tokens in a typed, zeroizing
  snapshot rather than untyped metadata.
- `CodexHttpTransport` disables ambient proxy discovery and accepts only a
  typed proxy value. A real loopback proves method, path, content type, accept
  header and form body; proxy errors and response errors remain redacted.
- `CliproxyRuntimeConfig` accepts Codex account IDs, priorities and three
  distinct secret references. IDs are unique across Claude and Codex. The
  `CtoxCodexSecretStore` loads all three values consistently and writes them in
  one encrypted SQLite transaction.
- Final gates: 86 library + 24 integration tests without native transports
  (110/110), 94 + 24 with default transports (118/118), both all-target Clippy
  matrices with `-D warnings`, the native Codex OAuth loopback and the root
  encrypted-store test are green. Differential and tracking remain required
  unchanged gates for the artifact update.

Forensic findings:

1. **Codex is a three-token protocol, not Claude with another endpoint.** The
   ID token provides account routing metadata, the access token authenticates
   requests and the refresh token rotates credentials. Collapsing the types
   would make partial writes and handle swaps much easier to express.
2. **Upstream's empty refresh fields encode “retain old value” at a later
   layer.** Rust makes that postcondition explicit before persistence: an
   omitted ID or refresh token keeps the previous secret, then the complete
   snapshot is written atomically. This preserves effective behavior without
   transiently constructing incomplete credentials.
3. **Singleflight must not index a secret map with raw tokens.** A SHA-256
   fingerprint is sufficient for process-local deduplication and keeps token
   material out of map keys, diagnostics and crash inspection surfaces.
4. **A coordinator future alone cannot reproduce `context.WithoutCancel`.** In
   Rust, dropping the leader future can cancel its transport. The executor-auth
   boundary therefore owns a detached Tokio task through persistence; that is
   the semantic cancellation boundary.
5. **Early genericization would weaken provider separation.** Claude and
   Codex currently use structurally similar secret wrappers, but distinct
   handle kinds prevent a Codex ID token from entering a Claude access slot.
   A provider-neutral abstraction should wait for a third implementation and
   preserve typed provider/kind identities.
6. **Auth tests do not constitute a usable provider.** The remaining 45 Codex
   points require a real request envelope, account-bound execution, one 401
   replay, response handling and streaming commit behavior through the server.

Strategy adaptation after checkpoint 9:

- Port Codex request construction and non-stream Responses execution as the
  next vertical slice, using the accepted auth lifecycle and common cooldown
  conductor. Do not add another standalone auth provider first.
- Reuse the OpenAI Responses wire contract directly for Codex; format
  translation is unnecessary on that path. Concentrate differential evidence
  on Codex-specific instruction injection, headers, account ID and request
  normalization.
- Establish the Codex stream's first accepted Responses event as its own
  commit boundary before enabling cross-account retry. Claude's
  `message_start` rule cannot simply be copied by name.
- Keep both feature matrices. Portable auth/config must remain buildable
  without native HTTP, while loopback tests own the actual wire contract.

### Checkpoint 10 — in progress; target 65%

Completed at 605/1,000 points so far.

Evidence for worker 10a:

- `codex_executor_request.rs` now normalizes a Responses request using the
  pinned upstream rules in this slice: base model and forced upstream SSE,
  removal of unsupported continuation/safety/stream-option fields, non-null
  instructions, Responses-Lite parallel-tool policy and plan-aware image-tool
  injection.
- Input items enforce Codex's 64-character ID ceiling. Long ordinary IDs are
  shortened deterministically with a SHA-256 suffix; overlong encrypted
  reasoning items are dropped rather than forwarding an invalid signature/ID
  pairing.
- The feature-isolated Chrome-133/BoringSSL transport sends the accepted Codex
  TUI user agent, Originator, Bearer token, ChatGPT account ID, optional
  `Session_id`, JSON body and SSE accept header. Its real TCP loopback verifies
  the wire contract and disables ambient proxy discovery.
- Non-stream execution consumes upstream SSE, reconstructs empty completion
  output from ordered `response.output_item.done` events and treats
  `response.incomplete` as a valid terminal response. Terminal provider
  messages are not reflected; only status and a safe error code survive.
- A 401 causes one detached OAuth refresh and a rebuilt request. The test
  proves that both `access-old`→`access-new` and
  `acct-old`→`acct-new` change on replay, and that the rotated refresh token is
  persisted. A second replay is impossible by construction.
- Gates pass 95 library + 24 integration tests without native transports
  (119/119), 104 + 24 with default transports (128/128), plus both all-target
  Clippy matrices with `-D warnings`.

Evidence for worker 10b:

- `CodexSubscriptionAccountPool` binds the existing provider-neutral
  `AccountRouter` and `CooldownConductor` to explicit Codex account candidates,
  executors and upstream targets. Duplicate IDs, non-Codex candidates and
  incomplete executor/target maps are rejected during construction.
- Every account-scoped status is persisted before failover or return. Store or
  join failures are fail-closed. Invalid-request statuses remain request-scoped
  and do not churn accounts; transport/incomplete-response failures become
  bounded transient account outcomes.
- Numeric and HTTP-date `Retry-After` values are converted into a relative
  delay without exposing the provider response body. The deterministic test
  proves A:429 + 7 seconds at clock 10,000 → persisted deadline 17,000, then
  B:200 and a redacted successful pooled outcome.
- The portable focused executor and pool tests pass. Full feature matrices stay
  reserved for the completed Codex server-route unit so the next gate proves
  the whole vertical path rather than repeating an intermediate gate.

Evidence for worker 10c:

- The server accepts one explicit `X-CTOX-Provider` selection and dispatches
  only through a configured allow-list. The default provider is typed router
  configuration; unknown, empty or duplicate selections fail before provider
  execution. The request `model` is never inspected to choose credentials or
  provider.
- The real TCP test deliberately combines `X-CTOX-Provider: codex` with a
  Claude-looking model string. Only the Codex transport runs, the model reaches
  Codex unchanged, and the direct Responses payload returns without a format
  translation or credential leak.
- Codex runtime configuration now owns a validated upstream base URL and plan
  type alongside the three distinct secret references. The CTOX host factory
  composes those records with the encrypted three-token store, injected refresh
  and Responses transports, persisted router/conductor and bounded timeout.
- Both targeted root factory tests pass. Full crate gates pass 97 library + 24
  integration tests without native transports (121/121) and 106 + 24 with
  default transports (130/130). Both all-target Clippy matrices with
  `-D warnings`, 13 differential parity fixtures + one named CTOX delta,
  formatting, diff whitespace and tracking are green.

Forensic findings after the non-stream Codex route:

1. **Provider choice belongs at the server policy boundary.** Putting it in the
   JSON body would risk forwarding a CTOX-only control field upstream; deriving
   it from `model` would couple credentials to naming conventions. A consumed
   HTTP control header keeps the dimensions separate and allow-listed.
2. **Direct Responses compatibility removes translation, not validation.** The
   Codex path still verifies a terminal object with an ID and completed or
   incomplete status before returning it. A pass-through route is not a license
   to reflect arbitrary provider bytes.
3. **The host factory is part of provider completeness.** A crate-local handler
   without typed construction from CTOX secrets would be testable but not
   operational. Provider work is not credited as a server route until both
   construction and real downstream dispatch are proven.
4. **Streaming remains the sharp boundary.** The buffered executor is correct
   for non-stream clients but cannot establish cancellation or post-first-event
   failure semantics. It must not be reused behind a streaming response by
   splitting an already buffered body.

Evidence for worker 10d:

- `CodexResponsesStreamingTransport` owns a bounded channel over wreq's byte
  stream. Dropping the receiver stops the pump and releases the response; the
  body is never first materialized as one buffer.
- Bootstrap accepts only a complete, parseable `response.*` SSE event within a
  1 MiB cap. Error/failed events, malformed closure and transport failure before
  that point become a synthetic 502, are persisted, and permit the pool to pick
  another account.
- The pool test proves A fails before its first event, B reaches
  `response.created`, and a later B transport error cannot trigger failover. It
  records B's transient cooldown for future requests instead.
- The server decodes and re-frames complete Codex SSE events rather than
  forwarding arbitrary HTTP chunks. Terminal provider messages are recursively
  replaced before emission; incomplete closure produces one stable
  `response.failed` event.
- The native chunked-loopback withholds `response.completed` until after the
  client consumed bootstrapped `response.created`, proving actual incremental
  transport. Full gates pass 100 library + 24 integration tests without native
  transports (124/124), 110 + 24 with defaults (134/134), both Clippy matrices,
  and both targeted CTOX runtime-factory tests.

Forensic findings after the streaming slice:

1. **HTTP chunks are not protocol events.** A provider can split a secret-bearing
   terminal JSON string across arbitrary chunks. Redaction after raw forwarding
   is therefore impossible; complete SSE event framing must precede emission.
2. **Codex and Claude share a semantic commit rule, not an event name.** Claude
   commits at `message_start`; Codex commits at the first valid `response.*`
   event. The pool abstraction can share policy while each protocol retains its
   own bootstrap parser.
3. **Post-commit failure changes future routing only.** Once downstream has a
   200 SSE response, switching accounts would splice unrelated response IDs.
   The correct action is one redacted terminal event plus persisted cooldown.

Next gate:

1. Start the Antigravity subscription vertical with its typed credential and
   refresh boundary, then connect one Responses request through a real provider
   request/response path before extracting shared three-provider primitives.
2. Define Antigravity's protocol-specific stream commit event before enabling
   account failover. Do not infer it from either Claude `message_start` or the
   first Codex `response.*` event.

Evidence for worker 10e (upstream sync, zero semantic points):

- The upstream pin advanced nine commits from `41fc5e1` to `ffdb9c9`. All 29
  changed paths were classified against the generated port map before the pin
  moved. Only `internal/config/config_types.go` and the Claude Responses
  non-stream converter intersected active/adapted Rust paths.
- Codex input sanitization now prefixes non-empty message item IDs with
  `msg_` before applying the 64-character limit, matching upstream's helper
  even though the original helper path remains a scaffold in Rust.
- Claude non-stream aggregation now emits reasoning, message and function-call
  items in original content-block order, including multiple distinct message
  items. A new Go/Rust differential fixture locks this behavior.
- That fixture exposed an additional parity bug: a three-character reasoning
  summary produced `reasoning_tokens: 0` in Rust. The field is now omitted
  unless the integer estimate is positive, matching upstream.
- Four newly added Go files have matching `.rs` scaffolds and receive no
  progress points. The `max-context-length` change belongs to the still-
  scaffolded configured model catalog and Codex models endpoint; the active
  CTOX account runtime intentionally does not expose a dead configuration
  field.
- Full gates pass 101 library + 24 integration tests without native transports
  (125/125), 111 + 24 with defaults (135/135), both all-target Clippy matrices,
  and 14 differential parity fixtures plus one named CTOX delta.

Forensic findings after upstream sync:

1. **A pin update is a semantic review, not a hash replacement.** The changed-
   file intersection reduced 29 upstream paths to two active/adapted surfaces,
   while differential testing still found a third observable edge case inside
   the rewritten converter.
2. **Scaffold drift must be visible but must not create dormant product API.**
   `max-context-length` becomes active only with the model catalog route. Adding
   it to CTOX subscription-account config now would claim behavior that no
   endpoint can consume.
3. **The next coherent provider slice is Antigravity.** Codex now has typed
   secrets, refresh, request execution, persisted account routing and both
   non-stream and incremental Responses routes. The remaining five budget
   points are not a safe excuse to duplicate or overfit that path.

Evidence for worker 11a:

- `internal/auth/antigravity/auth.rs` is now an active partial port. It builds
  Google's exact offline-consent authorization URL, including the five
  Antigravity scopes and the upstream localhost callback default.
- Access and refresh tokens are separate zeroizing secret types behind typed
  CTOX handles. Project ID is validated non-secret routing metadata and is
  preserved across refresh; a swapped handle pair fails before store access.
- Refresh form bodies include the installed-application OAuth client contract,
  remain zeroized, and redact both refresh token and public client credential
  from diagnostics. Provider error bodies are never reflected.
- The upstream 3,000-second refresh skew is explicit. Concurrent refreshes are
  deduplicated by a SHA-256 token fingerprint; the deterministic gate proves
  two callers receive one rotated result from one transport call.
- This worker earns only 10 points: the transport is still injected, and no
  CTOX persistence or provider request path is claimed. Full gates pass 106
  library + 24 integration tests without native transports (130/130), 116 +
  24 with defaults (140/140), and both all-target Clippy matrices.

Forensic findings after Antigravity auth core:

1. **Antigravity is not simply “Google OAuth”.** The 50-minute skew is much
   larger than Claude/Codex and project ID is required routing state that must
   survive every access-token rotation.
2. **The OAuth client secret is a public installed-app credential, not runtime
   account state.** It mirrors upstream protocol bytes, but is neither accepted
   from ambient config nor printed. User refresh/access tokens remain in CTOX
   secrets.
3. **Three providers still do not justify merging their stored snapshots.**
   Claude has two tokens, Codex three plus JWT routing, and Antigravity two plus
   project ID/expiry. Common zeroization/handle mechanics can be extracted only
   after the Antigravity store and executor reveal the actual shared boundary.

Evidence for worker 11b:

- The feature-isolated `AntigravityHttpTransport` performs the upstream Google
  OAuth refresh with a ten-second connect bound and the coordinator's explicit
  30-second request timeout. Redirects/retries and ambient proxy discovery are
  disabled; a proxy can enter only through typed host configuration.
- A real TCP loopback proves `POST /token HTTP/1.1`, the
  `oauth2.googleapis.com` Host header, `Go-http-client/2.0`, form content type,
  refresh grant fields and rotated access/refresh tokens while retaining the
  account's project ID.
- Invalid proxy diagnostics cannot echo embedded credentials. This earns five
  transport points; persistence and executable model requests remain unclaimed.
  The default matrix now passes 118 library + 24 integration tests (142/142),
  and both all-target Clippy matrices remain green.

Evidence for worker 11c:

- `CtoxAntigravitySecretStore` persists access token, refresh token and one
  encrypted state record containing project ID plus expiry in a single
  `write_secret_records` transaction. Loading reads the same three records in
  one SQLite snapshot; no environment or unencrypted payload fallback exists.
- A forced SQLite trigger failure on state rotation proves that already-issued
  access/refresh updates roll back together with routing state. A successful
  roundtrip proves millisecond expiry and project ID survive exactly.
- `AntigravitySubscriptionAuth` owns detached load → coordinated refresh →
  atomic persist semantics. Only a 401 can trigger status-based refresh; a 429
  writes nothing. The project ID remains bound to the rotated token snapshot.
- Full gates pass 107 library + 24 integration tests without native transports
  (131/131), 119 + 24 with defaults (143/143), both Clippy matrices, two root
  encrypted-store tests, and 14 differential fixtures plus one CTOX delta.

Forensic findings after Antigravity persistence:

1. **Routing metadata is transactionally credential-adjacent.** Project ID is
   not secret, but persisting it separately could pair a fresh token with stale
   routing after a crash. One encrypted state record makes the snapshot atomic.
2. **The common three-provider primitive is the transaction boundary, not the
   snapshot schema.** All providers need consistent multi-record rotation;
   their token/state shapes remain intentionally distinct.

Evidence for worker 11d:

- The active partial OpenAI Responses→Antigravity converter emits the upstream
  `{project, request, model}` envelope with contents, system instruction,
  function declarations, generation controls and five default safety entries.
- Claude-native E signatures are validated and wrapped exactly once into the R
  form required by Antigravity. Existing R forms remain unchanged;
  cross-provider signatures and empty thought text drop the thought block
  without dropping the following visible message.
- Registry activation is request-only. It deliberately reports no response
  capability until provider responses and execution are ported, preventing a
  raw Gemini payload from being mislabeled as OpenAI Responses.
- A dedicated pinned-Go differential runner passes five fixtures. Full gates
  pass 111 library + 25 integration tests without native transports (136/136),
  123 + 25 with defaults (148/148), both Clippy matrices, the existing 14
  Claude parity fixtures and one CTOX delta.

Evidence for worker 11e:

- The Antigravity facade now unwraps the provider's `response` envelope and
  delegates the complete non-stream semantic surface through its mirrored Rust
  response file. The explicit Registry pair advertises non-stream response
  support while streaming remains absent rather than silently buffering.
- Candidate parts retain semantic order across reasoning, visible messages,
  detached thought signatures and function calls. Gemini signatures receive
  the upstream direction/target carrier; request fields and token usage map
  back to the OpenAI Responses object.
- The pinned-Go corpus expanded from five request fixtures to nine combined
  request/response fixtures. It covers the provider envelope, request echoes,
  fixed timestamps, usage details, reasoning order, generated function-call
  identifiers and detached signatures. Dynamic-ID normalization is scoped only
  to Antigravity function calls so it cannot weaken the Claude corpus.
- Full gates pass 113 library + 25 integration tests without native transports
  (138/138), 125 + 25 with defaults (150/150), both all-target Clippy matrices,
  14 Claude parity fixtures plus one CTOX delta, and nine Antigravity parity
  fixtures.

Forensic findings after the Antigravity non-stream return path:

1. **The apparent wrapper port hides a large shared dependency.** Upstream's
   Antigravity file is only an envelope adapter, but correctness depends on the
   Gemini Responses converter's ordering and signature-carrier state. Porting
   just the wrapper would have produced valid JSON with false capability.
2. **Nondeterministic test policy must remain operation-local.** Function-call
   IDs are generated by both implementations. A first normalization attempt
   accidentally touched Claude fixtures and the existing differential gate
   rejected it; the final policy is explicitly restricted to the Antigravity
   response operation.
3. **Non-stream activation does not imply stream support.** The Registry now
   reports precisely one accepted return mode. The next worker must prove a
   real `generateContent` wire request before account routing or SSE credit is
   awarded.

Evidence for worker 11f:

- Active partial ports now define a validated Antigravity upstream target,
  zeroizing/redacted request and response envelopes, a bounded transport trait
  and a feature-isolated native HTTP implementation. Credential-bearing URLs,
  query strings and fragments are rejected before I/O.
- Request preparation fills the upstream model, `userAgent`, request type,
  project ID, unique request ID and deterministic first-user-text session ID.
  It removes safety settings at the executor boundary, drops
  `maxOutputTokens` for the accepted non-Claude path and enables `VALIDATED`
  function calling for Claude models.
- The native client forces HTTP/1.1, disables ambient proxies, redirects and
  retries, and applies bounded connect/request timeouts. Provider bodies and
  bearer credentials remain redacted from Debug and error variants.
- A real TCP test executes the entire local chain: OpenAI Responses request →
  Antigravity translator → project/session request preparation →
  `POST /v1internal:generateContent` → provider JSON → OpenAI Responses output.
  It verifies Authorization, Antigravity UA, endpoint, project, model, removal
  of safety settings, visible output and usage.
- Full gates pass 116 library + 25 integration tests without native transports
  (141/141), 129 + 25 with defaults (154/154), both all-target Clippy matrices,
  14 Claude parity fixtures plus one CTOX delta, and nine Antigravity parity
  fixtures.

Forensic findings after the first Generate execution:

1. **The stable session is protocol behavior, not observability metadata.** It
   derives from the first user text and therefore must be built before the
   request leaves the process; randomizing it per retry would change provider
   caching and conversation affinity.
2. **HTTP/1.1 is an explicit provider fingerprint.** A generic modern client
   may negotiate HTTP/2. The native transport therefore fixes HTTP/1.1 rather
   than relying on loopback's lack of ALPN as accidental evidence.
3. **A successful wire path still leaves material request semantics open.**
   Nested schema cleaning, model completion caps and native reasoning replay
   remain uncredited. The executor files stay `partial`; the next worker adds
   one 401 rebuild and persisted account outcomes before server exposure.

Evidence for worker 11g:

- `AntigravitySubscriptionExecutor` now loads one complete credential snapshot
  and owns a two-attempt maximum. The first 401 starts the detached
  load→refresh→atomic-persist lifecycle, then rebuilds body, request ID and
  bearer authorization from the rotated snapshot. A second 401 is returned;
  transport failures never trigger refresh.
- The focused test captures both outgoing attempts and proves
  `access-old`→`access-new`, one persisted refresh, identical project binding,
  distinct request IDs and `attempts == 2`. The provider's 401 body is absent
  from all error state.
- `AntigravitySubscriptionAccountPool` binds the provider-neutral persisted
  router and cooldown conductor to account-specific executors and targets.
  Candidate/provider mismatches, duplicates and incomplete maps fail at
  construction; outcome persistence is fail-closed before failover or return.
- The deterministic pool gate proves A returns 429 with `Retry-After: 7` at
  clock 10 000, producing a redacted cooldown until 17 000; B then returns 200.
  The selected and attempted account IDs remain evidence, while credentials
  never enter pool state.
- Full gates pass 116 library + 25 integration tests without native transports
  (141/141), 131 + 25 with defaults (156/156), both all-target Clippy matrices,
  14 Claude parity fixtures plus one CTOX delta, and nine Antigravity parity
  fixtures.

Forensic findings after 401 and account routing:

1. **A replay must rebuild provider metadata, not just replace a header.** The
   credential snapshot owns project routing state, and each provider attempt
   owns a unique request ID. Reusing the first serialized body after refresh
   would couple a new token to potentially stale routing metadata.
2. **Pool errors need an account-scoped classifier.** HTTP quota/auth and
   transport failures affect future account selection; invalid request shapes
   do not. This boundary is now explicit and shares the accepted conductor
   without importing provider response bodies.
3. **Antigravity is not server-ready yet.** Non-stream execution is complete
   enough for a pool, but nested schema sanitization and incremental SSE still
   determine whether arbitrary tool-heavy requests and streaming clients are
   safe to expose.

Evidence for worker 11h:

- The executable Antigravity request path now sanitizes only the provider's
  declared schema locations: function declarations under both supported tool
  spellings, their parameter/response schema variants, and response schemas in
  both generation-config spellings. `parametersJsonSchema` is normalized to
  the provider's `parameters` key.
- The bounded cleaner ports the critical upstream transformations for refs,
  const/enum, nullable required fields, tool unions and unsupported schema
  keywords. Claude tool schemas receive the required fallback fields, while
  response schemas preserve their unions/types and never receive tool-only
  placeholders.
- Two regression gates prove that historical
  `contents[].parts[].functionCall.args` payloads are never traversed or
  rewritten and that Claude `_`/`reason` tool placeholders cannot leak into a
  response schema.
- Full gates pass 118 library + 25 integration tests without native transports
  (143/143), 133 + 25 with defaults (158/158), both all-target Clippy matrices,
  and nine Antigravity differential fixtures.

Forensic findings after the schema boundary:

1. **Whole-document schema cleaning is data corruption.** Historical function
   arguments can contain arbitrary user keys that resemble JSON-Schema
   keywords. The explicit path list is therefore a load-bearing protocol
   boundary, not merely an implementation detail.
2. **Tool and response schemas require different policies.** Tool schemas may
   need union flattening and provider placeholders; response schemas must retain
   union/type intent. A shared recursive policy would silently change output
   constraints.
3. **This is the executable critical subset, not complete `gemini_schema.go`
   parity.** The mirrored executor remains `partial` until the remaining
   upstream hint/merge/idempotency and reasoning-replay behavior plus streaming
   are implemented and differentially gated.

Evidence for worker 11i:

- The Antigravity Responses registration now activates a request-scoped stream
  state rather than falling back to raw Gemini chunks. The facade unwraps each
  optional `response` envelope and emits `response.created` and
  `response.in_progress` exactly once.
- Reasoning summaries, visible text and function calls retain output-index and
  sequence-number order across provider chunks. `finishReason` closes active
  items and emits one `response.completed` with ordered aggregate output,
  request echoes and Gemini usage details.
- Three new Go/Rust differential cases cover split text, reasoning→text and a
  complete function-call lifecycle. The Antigravity corpus is now 12 parity
  fixtures; the stream transform is active in the explicit Registry.
- Full gates pass 119 library + 25 integration tests without native transports
  (144/144), 134 + 25 with defaults (159/159), both all-target Clippy matrices,
  14 Claude parity fixtures plus one CTOX delta, and all 12 Antigravity parity
  fixtures.

Forensic findings after the first stream state machine:

1. **The provider facade is thin, but the delegated state is not.** Upstream
   Antigravity only unwraps its envelope; all irreversible Responses ordering
   lives in the shared Gemini converter. Port status therefore remains
   `partial` until the rarer detached-signature transitions are also captured.
2. **Stream request-name restoration differs from non-stream.** Upstream passes
   the wrapped original request into the stream utility but unwraps it for the
   non-stream path. Differential comparison caught this observable asymmetry;
   Rust preserves it instead of applying a tempting common normalization.
3. **A registered converter is not a transport commit boundary.** The next
   worker must prove incremental body reads, bootstrap only after the first
   valid Responses event, pre-commit account failover, and post-commit
   redacted terminal failure before Antigravity can be exposed by the server.

Evidence for worker 11j:

- The native Antigravity transport now sends HTTP/1.1
  `POST /v1internal:streamGenerateContent` with the provider bearer/user-agent
  headers and `Accept: text/event-stream`. Its bounded mpsc channel owns the
  upstream byte stream; dropping the consumer drops the receiver and ends the
  producer task.
- `AntigravityResponsesStream` incrementally decodes arbitrarily fragmented
  provider SSE, feeds each data payload through the request-scoped Responses
  state and buffers only translated events. EOF performs the upstream `[DONE]`
  tail transition exactly once.
- `AntigravitySubscriptionExecutor::execute_stream` retains the two-attempt 401
  refresh/rebuild budget and does not return success until at least one valid
  Responses event has been translated. HTTP errors still remain pre-commit
  request errors.
- A real chunked TCP loopback blocks its terminal chunk after the first data
  event. The executor-side stream bootstraps and yields `response.created`
  before that tail is released, then completes with the terminal usage event.
- Full gates pass 119 library + 25 integration tests without native transports
  (144/144), 135 + 25 with defaults (160/160), both all-target Clippy matrices,
  and all 12 Antigravity parity fixtures.

Forensic findings after native stream bootstrap:

1. **Bootstrap belongs after translation, not after HTTP 200.** A successful
   status can still contain malformed/empty provider SSE. The first valid
   `response.*` event is the earliest safe downstream commit point.
2. **Provider chunk boundaries cannot be treated as SSE boundaries.** The
   transport exposes bytes; a persistent decoder owns framing and the
   translator owns semantic state. Conflating those layers would make real
   network fragmentation nondeterministic.
3. **The remaining pool work is post-commit accounting.** Single-account native
   streaming is executable, but pre-commit A→B failover and post-commit
   cooldown/terminal redaction still need one pooled gate before server
   exposure. This work remains in the scheduling and server ledgers, not the
   now-complete bounded Antigravity executor capability.

Evidence for worker 11k:

- `AntigravitySubscriptionAccountPool::execute_stream_configured` performs the
  same persisted candidate selection as non-stream execution, but an executor
  is only accepted after its translated stream has bootstrapped.
- The deterministic two-account gate makes A return a protocol error before any
  Responses event. That 502-equivalent outcome is persisted before B is
  selected; B's `response.created` is then the irreversible commit.
- `AntigravityTrackedResponsesStream` attributes a later transport failure to
  B exactly once and persists a redacted 502 cooldown. Neither provider bytes
  nor credentials enter the shared cooldown records.
- The mirrored `antigravity_executor_stream.rs` is now an active `partial`
  facade over the Rust ownership split instead of remaining a misleading
  scaffold in the file ledger.
- Full gates pass 119 library + 25 integration tests without native transports
  (144/144), 136 + 25 with defaults (161/161), both all-target Clippy matrices,
  and all 12 Antigravity parity fixtures.

Forensic findings after pooled streaming:

1. **Selection and commitment are separate states.** Merely selecting or
   receiving HTTP 200 from A cannot consume the failover budget; only a valid
   translated Responses event commits the account to the downstream client.
2. **Post-commit failure must not re-enter routing.** Once B has emitted an
   event, selecting C would splice two provider conversations into one stream.
   The only safe actions are terminalization at the server boundary and
   account-scoped cooldown evidence.
3. **Antigravity is now executor-complete but not integration-complete.** Typed
   CTOX runtime config/factory and server dispatch remain deliberately absent,
   and the upstream reasoning-replay cache still has uncovered semantics.

Evidence for worker 11l:

- The mirrored Antigravity replay-cache file is active and `partial`. It ports
  the in-process consistency core behind an owned Rust cache rather than using
  mutable package globals.
- Model/session continuity keys are SHA-256 indexed, so session IDs do not
  appear in map keys or Debug output. Entries enforce the upstream one-hour
  TTL, 4,096-item and 16-MiB chain bounds.
- Only normalized `thought_signature` and `function_call_part` records survive;
  unknown fields are removed, validator bypass markers and short signatures
  are rejected, and malformed records cannot create an empty accepted chain.
- Reads fence misses with a per-key tombstone. Conditional replacement accepts
  the exact revision or a same-branch descendant prefix, while stale first
  writers, stale clears and sibling branches cannot overwrite newer state.
- Full gates pass 123 library + 25 integration tests without native transports
  (148/148), 140 + 25 with defaults (165/165), both all-target Clippy matrices,
  and all 12 Antigravity parity fixtures.

Forensic findings after the Replay CAS core:

1. **Replay correctness starts at publication, not insertion.** Two overlapping
   turns may both be individually valid; only the turn whose snapshot still
   belongs to the current branch may publish or clear the chain.
2. **An absent read must create evidence.** Without a tombstone, a stale first
   writer can resurrect a chain after a newer turn deliberately cleared it.
   The reservation is therefore semantic state, not an optimization.
3. **This is not yet complete replay parity.** HOME/CTOX durable adaptation,
   session-lane derivation, function-call identity reconstruction and terminal
   SSE accumulation remain in the 2,007-line upstream executor file. The next
   worker ports payload reconstruction against this CAS contract instead of
   embedding persistence logic in the request transformer.

Evidence for worker 11m:

- Replay of thought signatures is pinned to the original signed-part
  fingerprint and occurrence. A changed surrounding context is accepted when
  that part remains identical; an edited target part, stale positional hint or
  non-model content fails closed.
- A client-carried copy of the same signature is removed only after the native
  model part is uniquely identified. Existing native signatures are never
  overwritten.
- Function calls match by trimmed name and canonical JSON arguments. Provider
  IDs are restored only from the exact native ID or its exact
  `cpa_gemini_` SHA-256 provenance ID, and the matching function-response ID
  and name move as one pair.
- A reused opaque provider ID with changed call semantics poisons the entire
  replay item instead of falling through to another coincidentally matching
  call. Multiple semantic candidates remain untouched.
- Four dedicated Go/Rust replay fixtures are equal. Full gates pass 128
  library + 25 integration tests without native transports (153/153), 145 +
  25 with defaults (170/170), both all-target Clippy matrices, 14 Claude
  parity fixtures plus one CTOX delta, and all 12 Antigravity fixtures.

Forensic findings after Replay payload reconstruction:

1. **Part identity outranks conversational position.** Context can legitimately
   grow between turns, while a cryptographically opaque thought signature is
   valid only for the exact provider-authored part. Fingerprint-first matching
   preserves that distinction.
2. **An opaque ID collision is semantic poison.** Treating a reused native ID
   as an ordinary miss would allow fallback matching to attach stale signature
   evidence to a different call. The whole item must be rejected.
3. **Identity restoration is an atomic relation.** Restoring only the
   `functionCall.id` leaves the corresponding `functionResponse` detached.
   Native ID, response ID and response name therefore change together or not
   at all.
4. **Rust eager option helpers can violate fail-closed code.** `then_some(x[i])`
   evaluates the index even when the condition is false. A zero-candidate
   negative test caught the panic; lazy `then(|| x[i])` is required at guarded
   index boundaries.
5. **The mirrored 2,007-line replay executor remains partial.** Omitted-call
   insertion, schema-default equivalence, terminal stream accumulation/CAS
   publication and the durable CTOX adapter are still deliberately unclaimed.

Evidence for worker 11n:

- `prepare_antigravity_reasoning_replay` performs one cache read, applies that
  chain to the exact outgoing payload and constructs an accumulator bound to
  the returned CAS snapshot. The accumulator re-derives the complete chain from
  this reconstructed request before it observes the new response tail.
- Split provider chunks are folded into one text/thought segment and pinned by
  the same kind-plus-content SHA-256 fingerprint as upstream. Function calls
  retain canonical semantic occurrences, provider IDs and directly attached
  or detached thought signatures.
- A non-terminal stream returns `NotTerminal` without any cache mutation.
  `finishReason` is the sole publication boundary; empty or oversized terminal
  chains invalidate only the exact snapshot, and a stale sibling receives
  `RejectedStale` instead of overwriting its winner.
- Four additional Go/Rust fixtures match for split visible text, thought→text,
  detached function signatures and non-terminal abort. Full gates pass 133
  library + 25 integration tests without native transports (158/158), 150 +
  25 with defaults (175/175), both Clippy matrices, 14 Claude + one CTOX delta,
  12 Antigravity and all eight replay fixtures.

Forensic findings after terminal replay publication:

1. **The accumulator starts after replay application.** Seeding only from the
   cache or only from the new response loses native request history. Reading,
   reconstructing the outgoing request and then scanning that exact request is
   one ordered transaction.
2. **Network EOF is not semantic completion.** A cleanly closed HTTP body
   without provider `finishReason` can still be a truncated turn. It must leave
   the existing replay lane untouched rather than publishing a plausible but
   incomplete tool/signature chain.
3. **Deletion is also a CAS write.** Overflow or a terminal empty chain may
   clear only the snapshot read by this turn; otherwise a slow failing turn can
   erase a newer successful descendant.
4. **Context normalization must preserve unrelated extension data.** The
   upstream fingerprint removes only the three signature fields, not the whole
   `extra_content` object. Differential preparation exposed that this is a
   protocol identity rule, not a convenient redaction shortcut.
5. **The replay core is now transactionally complete but not yet live in the
   server.** Typed Antigravity factory wiring must provide the owned cache and
   explicit session lane to both non-stream and stream executors; server
   exposure is the next worker and remains unclaimed here.

Evidence for worker 11o:

- The live Antigravity executor derives the replay lane from an explicit
  downstream `session_id` or `prompt_cache_key` before falling back to the
  generated provider envelope. Provider selection remains an orthogonal,
  allow-listed request dimension; the model string selects neither provider
  nor credentials.
- Replay preparation runs only after the final provider request envelope is
  constructed. Non-stream and stream paths observe raw Antigravity response
  payloads before Responses translation; transport EOF without a provider
  `finishReason` cannot publish a replay chain.
- Typed Antigravity account configuration resolves access, refresh and state
  secret handles, upstream target, proxy and model candidate. The CTOX factory
  gives every account executor the same owned replay cache, so account failover
  cannot split a downstream conversation into isolated lanes.
- The provider router exposes Antigravity through `X-CTOX-Provider` for both
  non-stream and streaming `/v1/responses`. Real TCP tests verify the translated
  response and the ordered `response.created` → `response.output_text.delta` →
  `response.completed` lifecycle.
- Full gates pass 135 library + 25 integration tests without native transports
  (160/160), 153 + 25 with defaults (178/178), both all-target Clippy matrices,
  the root CTOX factory test, 14 Claude parity fixtures plus one CTOX delta, 12
  Antigravity fixtures and all eight replay fixtures.

Forensic findings after live runtime/server integration:

1. **The downstream lane must outrank the generated upstream session.** A fresh
   provider session ID per request is useful wire metadata but cannot preserve
   multi-turn reasoning continuity. Explicit client session identity is the
   stable replay boundary.
2. **Replay belongs between final wire construction and translation.** Applying
   it earlier can be overwritten by envelope metadata; observing translated
   Responses events loses provider signatures and function-call provenance.
3. **A clean transport close is still not a semantic commit.** Streaming replay
   publishes only after `finishReason`; a transport error or truncated clean EOF
   leaves the previous lane intact.
4. **The cache is provider-runtime state, not account state.** A per-account
   cache would make normal cooldown failover forget the immediately preceding
   turn. One cache is therefore shared by all executors in the pool.
5. **Restart continuity remains deliberately unclaimed.** The in-process cache
   now matches the active upstream consistency core, but durable CTOX replay
   persistence and restart recovery need a separate adaptation and migration
   gate rather than being hidden inside the request converter.

Checkpoint 11 is complete at 70.5%. Before the next semantic allocation, the
remaining 295-point ledger and current upstream pin are re-audited so the next
vertical targets the highest integration risk rather than maximizing mirrored
file count.

### Checkpoint 11 forensic review — 70.5%

The pinned checkout remains clean at `ffdb9c9`; tracking regenerates without a
diff. The mechanical ledger contains 10 ported, 33 partial, 15 CTOX-adapted and
969 scaffold files. That distribution confirms that file count is still a poor
progress driver: the active runtime verticals carry substantially more product
value than the dormant mirror.

The remaining 295 semantic points are concentrated in Interactions (45), the
last Codex executor gap (5), scheduling/retry (10), typed CTOX adaptation (15),
Pi/Claude/Codex harness integration (40), management (30), safe plugin-host
replacement (25), observability/policy (25), and incomplete provider-family
semantics. The highest immediate integration risk is the 40-point harness lane:

- the Business OS Coding Agents app already keeps provider and model separate
  in its payload contract;
- the Pi sidecar already speaks OpenAI Responses through a loopback CTOX
  gateway and keeps upstream credentials server-side;
- however, its default is hard-coded to Kimi K3 rather than inheriting the
  active CTOX main model, and the browser offers a static two-model list;
- the freshly ported subscription pools are not yet part of the daemon-owned
  gateway lifecycle, so the UI cannot truthfully discover their capabilities.

Strategy adaptation after the 70% review:

1. Checkpoint 12 integrates the proxy into the existing Pi/Claude/Codex and
   Business OS control surfaces before adding another format family.
2. The first unit replaces hard-coded Pi defaults and browser presets with a
   typed, server-authoritative model/provider capability contract. Omission
   continues to mean “inherit CTOX main model/provider.”
3. Provider choice remains explicit and orthogonal to model. Harnesses send a
   provider route only when the user selects an override; model IDs never act
   as credential selectors.
4. The daemon owns proxy pool construction, secrets and lifecycle. The Pi
   process receives only a loopback Responses target and public routing data.
5. A real bounded Pi turn and Business OS command projection must gate the
   vertical before harness points are credited.

Evidence for worker 12a:

- `coding_default_model` no longer hard-codes Kimi K3. With no explicit model
  in `ctox.coding.turn`, Pi receives `GatewayConfig.active_model`, the same
  resolved main model used by CTOX, through the loopback Responses endpoint.
- The new `ctox.coding.models.v1` capability document has an explicit
  `inherit_ctox` default and publishes only daemon-known public routes. It does
  not infer subscriptions from model names or expose secret handles.
- `ctox.coding.models` is an exact control command guarded by module-scoped
  `apps.view`. The real command roundtrip returns the active model in a terminal
  projection, while the dispatcher inventory guard includes the new arm.
- The Coding Agents app loads that terminal projection through the command bus,
  validates provider/model tuples and keeps a model-less CTOX inherit option.
  Invalid, denied or stale discovery cannot replace that fallback; a selection
  race cannot apply capabilities for the previously active module.
- Gates: three focused root tests, 17 Coding Agents browser tests,
  `cargo check -p ctox`, full root `cargo fmt --check`, and the unchanged pinned
  proxy tracking gate are green.

Forensic findings after the first harness unit:

1. **“Same provider/model as CTOX” must be executable behavior.** The prior
   comment promised inheritance while the implementation selected Kimi K3.
   Resolving through `GatewayConfig` removes that split source of truth.
2. **Discovery is a policy-gated control read, not browser configuration.** A
   static selector silently becomes a credential router as soon as subscription
   providers exist. The browser may render capabilities but cannot author them.
3. **Omission is a meaningful routing state.** `model: null` means inherit the
   active CTOX provider/model contract. It is not equivalent to sending a
   guessed public model ID.
4. **Capability truth is intentionally narrow for now.** Only the live main
   gateway route is advertised. Claude, Codex and Antigravity subscriptions
   enter the list only after daemon-owned pool startup and health are wired;
   claiming them earlier would recreate the static-preset defect.

Evidence for worker 12b:

- `instance_codex_runtime_config` converts the encrypted Business OS ChatGPT
  subscription login into one validated Codex account containing only typed
  secret references and public routing metadata. Missing or incomplete login
  state disables the route instead of constructing a partially authenticated
  pool.
- The canonical Codex handles resolve through `CtoxCodexSecretStore` to the
  existing `ctox-auth/chatgpt_subscription_auth_json` record. A refresh writes
  ID, access and refresh tokens back as one encrypted JSON snapshot and keeps
  the existing routing `account_id`.
- The portable store does not mutate process environment or a globally
  discovered Codex home. Harness projection remains an explicit daemon
  lifecycle concern, avoiding hidden filesystem writes during a proxy request
  and making isolated tests safe.
- The focused root gate passes all 12 `cliproxyapi_host` integration tests,
  including missing-config, initial load, rotation, plan transition and
  post-rotation reload. Targeted formatting and diff checks are green.

Forensic findings after the shared subscription snapshot:

1. **The Business OS login is already the correct instance authority.** Adding
   three parallel proxy records would create two independently rotating token
   sets. Canonical handles therefore adapt the existing encrypted record
   instead of duplicating credentials.
2. **A three-token refresh is one semantic write.** Updating only access and
   refresh tokens can leave plan/workspace claims from an old ID token. The
   complete `AuthDotJson` is replaced in one encrypted SQLite upsert.
3. **Harness projection must not be a store side effect.** Writing whatever
   Codex home happens to be globally discoverable is not instance-scoped and
   would make tests or multi-instance operation mutate unrelated state.
4. **The listener is still deliberately unclaimed.** The root binary does not
   yet link the native Codex HTTP transport or supervise a loopback listener.
   That construction/health boundary is the next worker before subscription
   capabilities may be advertised to Business OS.

Evidence for worker 12c:

- The CTOX root dependency enables only `codex-http-transport`; it does not
  import all provider fingerprints. This links the native refresh, buffered
  Responses and cancel-on-drop streaming transports required by the instance
  Codex route.
- `build_instance_codex_responses_router` resolves optional proxy configuration
  through typed secret references, constructs inert native transports, builds
  the persisted account pool with system clocks and wraps it in the explicit
  Codex provider router. Construction performs no external request.
- The first isolated feature build exposed a hidden coupling: Codex streaming
  used `futures-util` and `wreq/stream`, but those were enabled only by the
  Claude fingerprint feature. The Codex feature now declares both directly.
- Gates pass 13/13 CTOX host tests, 139 proxy library tests plus 25 integration
  tests with only the Codex transport feature, both native Codex loopbacks, and
  isolated all-target Clippy with warnings denied.

Forensic findings after native router construction:

1. **Default-feature success did not prove provider isolation.** The previous
   full matrix masked Codex's undeclared streaming dependencies through the
   Claude feature. Every native provider needs a no-default single-feature
   build gate.
2. **Runtime construction and listener ownership are separate boundaries.** A
   transport may safely allocate clients at daemon start, but it must not make
   an upstream request until a supervised downstream request exists.
3. **Proxy configuration remains secret-store owned.** Even noncredential
   routing URLs are resolved from typed handles for native-client construction;
   ambient HTTP proxy discovery remains disabled.
4. **A built router is not a healthy service.** Binding, readiness, shutdown,
   restart behavior and capability publication still require the daemon-owned
   loopback lifecycle and remain uncredited.

Evidence for worker 12d:

- Service boot starts one process-wide `ctox-codex-subscription-proxy`
  supervisor. With no usable instance login it remains in
  `WaitingForSubscription` and does not bind a port; invalid config reports a
  redacted fault and retries without terminating the CTOX control plane.
- A valid snapshot binds only `127.0.0.1:12435`. The primary CTOX inherit
  contract on `:12434` remains reserved; a Codex-only default router therefore
  cannot accidentally replace the main provider/model route.
- Accepted streams run as independent Tokio tasks. A one-second typed snapshot
  check tears down and rebuilds the listener/router after login, logout, plan or
  credential-shape changes while token-only refreshes continue through the
  shared encrypted secret store.
- A real TCP test proves loopback binding and bounded HTTP dispatch with a 405
  response before any upstream call. All 14 CTOX host integration tests pass;
  the service boot wiring compiles in the root unit-test binary.

Forensic findings after daemon listener integration:

1. **The documented main gateway port was not an implemented listener.** It is
   still the Pi default contract, so occupying it with a Codex-only router
   would make “inherit CTOX” silently mean “use ChatGPT subscription.”
2. **Absence of auth is a waiting state, not a crash loop.** Business OS login
   may happen after service boot. The supervisor must observe that transition
   without requiring a daemon restart or binding an unusable endpoint.
3. **Readiness follows bind plus router construction.** A stored login alone is
   not a publishable capability; native-client construction or port ownership
   can still fail.
4. **Connection lifetime cannot own listener progress.** Streaming turns may be
   long-lived, so accepted connections are spawned independently while the
   supervisor continues health/reconfiguration checks.
5. **Capability publication is the next unclaimed boundary.** Business OS may
   expose the Codex preset only when the in-process status for the same CTOX
   root is `Ready`; faulted or merely configured routes remain hidden.

Evidence for worker 12e:

- `coding_model_capabilities` always keeps the model-less CTOX-inherit preset
  first. It appends a Codex subscription preset only when the supervisor status
  for the exact requested root is `Ready`; stopped, waiting and faulted states
  publish no selectable alias.
- The subscription preset is a complete pi-ai OpenAI Responses model override:
  current public model ID, provider/API metadata and the distinct
  daemon-owned loopback base URL. Selecting it changes the provider path
  without changing the chosen model string.
- The browser validator preserves the complete server-authored model object and
  sends it only for an explicit nondefault pick. Invalid capability documents
  and selection races still fail back to an omitted model override.
- Gates pass the focused Pi root test, a real `ctox.coding.models` command
  roundtrip before and after readiness, and all 17 Coding Agents browser tests.

Forensic findings after capability publication:

1. **Configured is not selectable.** Credentials may exist while native client
   construction or bind ownership is faulted. Publishing from secret presence
   would send Pi turns to a dead endpoint.
2. **Readiness must be root-scoped.** Static process status without the owning
   root could leak a route from an isolated test or a different CTOX instance
   into the current Business OS capability document.
3. **Provider and model remain orthogonal.** Both presets may name the same
   active model; their difference is the server-owned loopback provider path,
   not a model-name heuristic.
4. **The browser transports an opaque typed model object.** It validates only
   the minimum public shape and does not reconstruct base URLs, credentials or
   provider aliases locally.
5. **End-to-end execution remains unclaimed.** The next gate must execute an
   actual Pi-sidecar turn through the ready `:12435` listener with a controlled
   upstream, proving request/stream/termination behavior beyond discovery.

Evidence for worker 12f:

- The published main and subscription model base URLs now end in `/v1`.
  Pi-AI's OpenAI client appends `/responses`, while the bounded Rust server
  deliberately accepts only `/v1/responses`; the previous host-only URLs
  produced an unreachable `/responses` request.
- Real Pi turns no longer depend on ambient provider keys. The sidecar supplies
  the public sentinel `ctox-loopback` only when the provider is exactly
  `ctox-gateway` and the parsed base URL is credential-free HTTP loopback.
  Arbitrary remote URLs and other providers remain unauthorized, while all
  real Subscription credentials stay in the daemon-owned encrypted store.
- A multi-threaded root integration test runs the built Node sidecar and the
  real Pi-AI OpenAI Responses stream through an ephemeral Rust listener,
  validated Codex account pool and native `wreq` transport into a controlled
  upstream. It asserts `/backend-api/codex/responses`, rewritten model, Bearer
  token, ChatGPT account ID, terminal SSE text and response-side redaction.
- Gates pass the real E2E test, three instance-listener/config tests, two
  capability tests, 153 proxy unit tests plus 25 integrations, all 17 Coding
  Agents browser tests, the sidecar runtime smoke and targeted formatting.
  The explicit TypeScript compiler run remains red with 23 pre-existing
  Result-narrowing/return-type errors; build and runtime are green, but the
  sidecar is not claimed as type-clean.

Forensic findings after the real Pi subscription turn:

1. **An API-compatible base URL is part of the protocol contract.** Publishing
   a healthy listener was insufficient because the SDK owns the final relative
   path. Capability tests must exercise the consuming client, not only compare
   the advertised host and port.
2. **A local credential sentinel is safer than forwarding daemon auth.** Pi-AI
   requires an API-key-shaped value before it opens a Responses stream. A
   public, provider- and loopback-bound sentinel satisfies that client contract
   without duplicating or exposing the Subscription bearer.
3. **The validated-config seam is useful test architecture.** Supplying a
   controlled upstream after config validation proves the native wire without
   adding a production environment toggle or weakening the official target.
4. **Runtime-green is not type-clean.** Esbuild and the real socket turn pass,
   while `tsc` exposes old structural Result errors. Those errors become an
   explicit backlog gate rather than being hidden by the bundler.
5. **The harness lane is 35/40, not complete.** The explicit Codex subscription
   route is real; the default CTOX-inherit route on `:12434` still needs the
   same consuming-client listener/turn proof before the lane closes.

Evidence for worker 12g (zero semantic points):

- `npm run typecheck` now executes a real strict project configuration. The
  earlier ad-hoc non-strict invocation disabled discriminated-union narrowing
  and manufactured 23 errors; strict mode reduced the actual gap to the
  untyped legacy `virtualfs` package and five implicit callback parameters.
- A narrow local declaration types only the filesystem methods CTOX uses.
  This removes the implicit callback types without adding an abandoned
  third-party `@types` package or weakening the rest of the project to `any`.
- Two Node security tests prove that the public Pi credential sentinel accepts
  only provider `ctox-gateway` on credential-free HTTP IPv4/hostname/IPv6
  loopback. HTTPS, remote hosts, URL credentials, malformed URLs and foreign
  providers are rejected.
- Strict typecheck, bundle, both security tests, runtime smoke, the rebuilt
  embedded Rust E2E test and all 17 browser tests pass. No semantic points are
  awarded: these are acceptance-gate repairs, while `:12434` remains absent.

Forensic correction after the strict compiler gate:

1. **Compiler flags can create false debt.** TypeScript discriminated Result
   unions narrow under `strictNullChecks`; invoking `tsc` without a project
   made valid error branches look broken. The repository now owns the flags.
2. **Legacy typing should be capability-shaped.** The declaration mirrors only
   the synchronous in-memory filesystem calls in use, making upstream API drift
   visible without claiming a complete type model for `virtualfs` 2.2.0.

Current-source correction (2026-09-20, base `9dab76e73d2949bf011914e38a3becdb3db24267`):

The worker 12h account below is historical and is **not evidence that the
current service owns a listener on `:12434`**. `GatewayConfig.listen_port` and
the main-gateway status cell do not establish socket ownership. Do not start a
replacement listener or point Pi at another process on that port to satisfy
this historical claim.

The current embedded Pi inherit path is owned by each coding turn:

- `coding_agents/pi_sidecar.rs::gateway_model` returns an `inherit_ctox`
  descriptor with an intentionally unusable public endpoint. Before invoking
  Pi, `prepare_coding_turn_model` resolves that descriptor through
  `prepare_inherited_coding_model`.
- `resolve_inherited_coding_route` resolves the main provider, active model,
  endpoint and credential selector from the supplied root. It explicitly
  rejects port `12434`; unsupported providers fail closed.
- `prepare_inherited_coding_model_route` loads the same-root credential and
  creates an `AnthropicCodingBridge` using Responses or Chat Completions.
  Pi receives its ephemeral loopback URL and per-turn capability header, not
  the provider credential. The bridge validates the selected model and owns
  upstream credential injection. Its `Drop` stops and joins its worker.
- Existing source regressions include
  `inherited_responses_bridge_authenticates_scopes_streams_and_stops`,
  `inherited_ctox_proxy_route_keeps_credentials_and_configuration_native`,
  `inherited_minimax_route_drives_real_pi_tools_through_native_bridge`, and
  `gateway_model_defers_private_route_resolution_to_the_turn_owner`.

This correction is source inspection only. These tests were not rerun for this
correction; an installed binary, same-root route readiness, real Mail module
turn and bounded shutdown under stalled I/O still require exact-source runtime
evidence. Neither the historical counts below nor the mere presence of these
tests establish that evidence.

Historical worker 12h report (superseded for current inherit wiring; gate
results have not been re-established on the revision above):

- The CTOX service now supervises a dedicated loopback-only main Responses
  listener on `127.0.0.1:12434`. It resolves `GatewayConfig` per request, so an
  active main-model switch takes effect without restarting Pi or the listener.
- The server rejects client-side provider selection and overwrites every
  request model with CTOX's active model. Local runtimes use the existing
  line-delimited `responses_create` IPC contract; remote API and subscription
  credentials are loaded only inside the daemon from typed runtime/secret
  state.
- OpenAI-compatible Responses and the Codex subscription proxy preserve their
  SSE body. Provider-native adapters use the existing request/response plans
  and convert their buffered normalized response back to Responses JSON or SSE.
- A real Node/Pi-AI turn crosses an ephemeral instance of the main Rust
  listener into a controlled OpenAI Responses upstream. It proves `/v1`
  joining, server-authoritative model replacement, daemon-only Bearer injection,
  terminal assistant text and response-side secret absence.
- Gates pass all 9 main-gateway tests, 153 proxy unit tests plus 25 integration
  tests, strict Pi typecheck, both sentinel security tests and the sidecar smoke.
  Two stale gateway fixtures were corrected from `openai/gpt-oss-120b`, which
  the current engine explicitly classifies as API-only, to the supported local
  model `Qwen/Qwen3.6-27B`.

Forensic findings after the main inherit turn:

1. **A config field is not a listening service.** The old `listen_port: 12434`
   fed discovery and telemetry but had no socket owner. Consuming-client E2E is
   now required before any advertised harness endpoint is considered real.
2. **The daemon must own both routing dimensions.** Pi can describe a model for
   SDK compatibility, but neither that string nor an `X-CTOX-Provider` header
   may choose the credential pool on the inherit boundary.
3. **Local and remote execution cannot be one blind HTTP pass-through.** Local
   inference is private IPC with same-user transport semantics; remote models
   need credential injection and sometimes format adaptation. The shared
   boundary is Responses-shaped, not transport-shaped.
4. **Existing adapters were unconsumed capability.** The provider-native route
   plans already expressed request and response rewrites, but no gateway called
   them. The main listener is now their first daemon-owned consuming path.
5. **Incremental streaming remains protocol-specific work.** Canonical
   OpenAI/Subscription SSE stays intact, while provider-native adapters buffer
   one JSON response before synthesizing outer SSE. A generic incremental
   adapter stream interface is required before claiming equivalent latency and
   commit semantics for those providers.

Evidence for worker 12i:

- `internal/api/middleware/request_logging.go` and its upstream test facade are
  now active partial Rust modules instead of five-line scaffolds. The port
  preserves plain-GET skipping while retaining Responses WebSocket upgrades,
  and excludes both management URL prefixes from request logging.
- Error-only capture accepts known nonmultipart bodies through exactly 1 MiB;
  larger and unknown lengths remain uncaptured unless full logging is enabled.
- Authorization, API-key, token and secret headers plus sensitive query values
  are masked with the upstream key-shape convention. Query strings with no
  sensitive parameter are returned byte-for-byte unchanged.
- Identity and stacked Zstd capture decoding fail back to the original bytes on
  malformed or unsupported input. Limited decoding stops expansion at the
  configured bound and appends the explicit upstream truncation marker.
- Seven new parity/security tests pass inside the full 160-test proxy unit set;
  all 25 proxy integration tests remain green.

Forensic findings after the redaction core:

1. **Persistence must follow redaction, not precede it.** Bringing the file
   logger into the module graph before masking and decompression limits would
   create a real credential and decompression-bomb sink.
2. **“Logging disabled” still has error semantics.** Upstream deliberately
   captures small known JSON bodies for error-only logs, but spools large or
   unknown bodies. A boolean logging flag therefore cannot simply bypass the
   middleware.
3. **WebSocket GET is not an ordinary read.** Skipping all GET requests would
   erase the only request-side evidence for a Responses WebSocket session.
4. **This is 5/25 points, not a completed logger.** No request or response is
   persisted yet; response-writer finalization, bounded deferred spooling,
   retention and CTOX telemetry consumption remain the next observability work.

Evidence for worker 12j:

- `response_writer.go`, its upstream test facade and `request_logger.go` are
  active partial Rust modules. The response state captures status, cloned
  headers, body, request identity and the same Content-Type-first streaming
  decision as upstream.
- Error-only mode forces a log only for HTTP status 400 or higher when regular
  request logging is disabled. Successful bodies are neither retained nor
  handed to the logger in that mode.
- The file sink applies the already-gated query/header redaction before writing,
  uses collision-resistant create-new names and bounds retained forced-error
  files. An integration test traverses response finalization through a real
  temporary log file.
- Gates pass 166 proxy unit tests, all 25 proxy integration tests, all-target
  Clippy with warnings denied, root `cargo check`, format/diff checks and pinned
  tracking.

Forensic findings after the response/file vertical slice:

1. **Error-only is a separate execution mode, not a filename convention.** The
   response writer must decide whether to buffer from status before the sink is
   called; letting a disabled logger decide later would already have discarded
   the body.
2. **Content-Type wins over a client stream hint.** A JSON error answering a
   request containing `"stream": true` must remain non-streaming, otherwise
   finalization would enter the wrong lifecycle.
3. **Retention belongs after a durable close.** Cleanup runs only after the new
   error log has been flushed, so a retention failure cannot silently replace
   the evidence currently being written.
4. **This is 10/25, not HTTP middleware completion.** The Rust state contract
   and file sink are operational together, but no production listener consumes
   them yet. Large/unknown request bodies still need bounded file-backed
   sources, and streaming needs a non-blocking writer before HTTP wiring earns
   further points.

Strategy adaptation after worker 12j:

- Port file-backed body sources and the streaming writer as one bounded unit,
  including queue saturation, close ordering and cleanup tests. Only then wire
  request/response logging into the bounded HTTP server, so production traffic
  never uses an unbounded synchronous streaming sink.

Evidence for worker 12k:

- `FileBodySource` stores ordered sections in a per-request temporary
  directory, recreates a manually removed part directory, merges while skipping
  missing parts, and cleans files idempotently on explicit cleanup or drop.
- `FileStreamingLogWriter` uses a 100-slot synchronous queue exclusively via
  `try_send`. A saturated queue drops and counts a log chunk instead of applying
  backpressure to the response path; an OS worker thread spools accepted chunks
  directly to a temporary file.
- Close drops the sender, joins the worker, writes the redacted final log only
  after the spool is drained, and removes the temporary body on success or
  failure. `ResponseWriterWrapper` now selects this path for enabled SSE
  responses and retains its buffered fallback when stream setup fails.
- Gates pass 169 proxy unit tests, all 25 integrations, all-target Clippy with
  warnings denied, format/diff checks and pinned tracking. The mirror ledger is
  now 10 ported, 40 partial, 15 adapted and 962 scaffolds.

Forensic findings after bounded streaming:

1. **Backpressure policy is observability semantics.** Blocking preserves every
   log byte but can stall a model stream; the upstream contract deliberately
   prefers client latency. Rust makes the loss explicit with a counter instead
   of silently waiting.
2. **Drain precedes assembly.** Closing the final log before the worker exits
   races the last chunks. Sender drop, worker join, final assembly is now a
   tested order.
3. **Temporary files need failure cleanup, not only happy-path cleanup.** Worker
   and final-file errors converge on the same removal path, while RAII covers a
   caller that drops a writer without closing it.
4. **File-backed capability is not yet large-body integration.** The source can
   preserve large sections without a memory merge, but the request logger's
   public record still owns `Vec<u8>` bodies. Context/source wiring remains part
   of the production middleware slice.
5. **This is 15/25 points.** HTTP listener integration, drop/error telemetry and
   typed runtime policy still remain before observability can be called
   production-active.

Strategy adaptation after worker 12k:

- The next unit wires a logger policy into the bounded server request lifecycle
  and exports only counters/status to CTOX telemetry. Full body logs stay
  disabled by default; error-only logging must be explicitly typed and must not
  introduce an environment toggle.

Evidence for worker 12l:

- `server_middleware.go` is now an active partial Rust facade with explicit
  full and error-only policies plus atomically snapshotted counters. No process
  environment variable controls runtime logging behavior.
- The bounded HTTP server retains all parsed request headers, applies the
  method/path policy, decodes captured request bodies with the existing Zstd
  expansion limit and wraps each accepted POST before route dispatch.
- Response bytes are written to the client before they enter the capture path.
  Final file assembly and streaming-thread join run on a blocking worker; a
  logging failure updates its counter but never changes the already determined
  model response.
- The historical report claimed error-only policies on main CTOX inherit
  `:12434` and Codex subscription `:12435`. The `:12434` production-wiring
  claim is superseded by the current-source correction above; do not infer
  logging coverage for the per-turn coding bridge from it. The reported TCP
  test exercised a provider 429 becoming a redacted file and
  exactly one forced-error metric without leaking query or Authorization
  credentials.
- Gates pass 170 proxy unit tests, all 25 integrations, all-target Clippy with
  warnings denied, root `cargo check`, format/diff checks and pinned tracking.
  The mirror ledger is 10 ported, 41 partial, 15 adapted and 961 scaffolds.

Forensic findings after production listener wiring:

1. **Logging must be response-transparent.** The network write remains first;
   logger finalization is isolated afterward. Failure to persist evidence is a
   metric, never a replacement 5xx sent to the harness.
2. **Policy belongs to listener ownership.** Constructing it once per supervised
   listener gives all connection tasks the same retention and counter state,
   without ambient configuration or per-request logger reconstruction.
3. **A raw request parser must retain headers before middleware exists.** The
   former minimal parser extracted only Content-Length and provider selection;
   redaction and request identity require the complete multivalue header map.
4. **Error-only is the safe production default.** Successful model traffic
   creates no body log, while failed Responses requests retain bounded evidence
   with sensitive URL/header values masked.
5. **This is 20/25 points.** The counters are typed and shared but still local
   to each supervisor policy. Projection into `RuntimeTelemetry`, policy control
   through the existing typed runtime store, and log discovery remain open.

Strategy adaptation after worker 12l:

- Finish Observability with a process/root-scoped metrics registry projected
  into existing runtime telemetry. Do not add a second metrics server or expose
  log bodies through HTTP; Business OS should receive only redacted counters and
  health state through its existing authoritative projection path.

Evidence for worker 12m:

- Each supervised listener registers its metrics by `(CTOX root, scope)` using
  a weak reference. `main-responses` and `codex-subscription` remain distinct;
  replacing or dropping a policy cannot leave an immortal stale counter owner.
- `request_logging_metrics_for_root` produces deterministic per-scope snapshots
  and a saturating total for finalized, forced-error and streaming logs,
  dropped stream chunks and logger failures.
- The existing serializable `RuntimeTelemetry` contract now includes that root
  snapshot. It contains no log path, URL, header, request/response body,
  credential or prompt, and no second HTTP or metrics service was introduced.
- The real TCP error-log test proves the same request updates both its listener
  scope and the root total. Gates pass 170 proxy unit tests, all 25 integrations,
  all-target Clippy with warnings denied, root `cargo check`, formatting,
  diff-check and pinned tracking.

Forensic findings after telemetry projection:

1. **Registry ownership must be weak.** A global strong metrics map would retain
   obsolete supervisors forever and make a restarted listener's counters
   ambiguous. Listener policy remains the lifetime owner.
2. **Root isolation is part of correctness.** Tests and multiple CTOX instances
   can share a process; aggregating only by listener name would cross tenant and
   test boundaries.
3. **Telemetry is not a log retrieval channel.** Operational counters explain
   health and loss without turning Business OS into an HTTP bridge for request
   bodies or secrets.
4. **Existing cache semantics are acceptable.** The five-second runtime
   telemetry cache makes counters eventually consistent, avoiding expensive
   runtime re-resolution on every model request.
5. **Observability is now 25/25.** Further log-management UI/API work belongs to
   the separate Management/control ledger, not to hidden expansion of this lane.

Strategy adaptation after worker 12m:

- Return to the frozen ledger rather than polishing logging further. The next
  checkpoint selection should compare Management/control, safe plugin-host
  replacement and remaining format/provider gaps by deployability and upstream
  churn, then choose the smallest vertical path toward 80%.

### Worker 12n — 77% remaining-ledger forensics (zero points)

- The final five Codex executor points are not another HTTP route. Upstream's
  missing `CountTokens` path selects cl100k/o200k-family tokenizers and counts a
  normalized semantic segment set. The workspace has no equivalent Tiktoken
  basis; a character or whitespace estimate would violate upstream parity.
- Management/control spans configuration, OAuth sessions, auth-file CRUD,
  usage, logs, quota, plugins and model definitions across more than fifty Go
  files. Safe plugin replacement is similarly a complete out-of-process RPC,
  lifecycle, stream and platform boundary. Neither is a credible five-line
  checkpoint filler.
- Interactions already has explicit Responses request, non-stream response and
  stream converters with focused upstream tests. Request plus non-stream can be
  differential-gated independently before activating its stateful SSE path.

Strategy decision after worker 12n:

1. Port Responses ↔ Interactions request and non-stream response as the next
   bounded vertical, preserving raw tool arguments, files/images, reasoning
   carriers, usage and missing/null distinctions.
2. Add it to the existing pinned Go/Rust differential runner before awarding
   points; do not activate streaming registration in the same worker.
3. Port Interactions SSE only after the non-stream corpus is stable. Keep Codex
   token counting open until an exact tokenizer asset/dependency strategy is
   accepted, and treat Management/plugin-host as later control-plane projects.

### Worker 12o — Responses ↔ Interactions request/non-stream (10 points)

- Both request directions are active through the explicit Rust registry. They
  cover system instructions, previous interaction IDs, text/image/file/media
  content, function calls/results with stable call IDs and JSON arguments,
  tools, tool choice, reasoning controls, response format, modalities and
  service tier while dropping fields with no Responses equivalent.
- Both non-stream response directions are active. Model output, thought
  summaries/signatures, function calls and detailed token usage are converted;
  nested Interactions envelopes and alternate total-usage locations retain the
  upstream precedence rules.
- Streaming functions are intentionally absent from the registered
  capabilities. A registry test proves non-stream is available in both
  directions while stream remains false, preventing raw passthrough from being
  mistaken for protocol support.
- Eight fixtures execute inside the pinned Go package and the Rust binary and
  compare equal after canonical JSON sorting. Nine focused Rust converter tests
  and the capability gate pass.

Forensic findings after worker 12o:

1. **The stateless seam is genuinely separable.** No stream state is needed for
   request or completed-response conversion, so the registry can expose useful
   capability without constructing an incomplete erased state machine.
2. **JSON strings are protocol data, not always text.** Function arguments and
   results cross this pair as both objects and JSON-encoded strings. The port
   parses only at the same mutation boundary as upstream and serializes back to
   compact JSON when Responses requires a string.
3. **Usage has several envelope locations.** Interactions reports usage at six
   possible paths and uses both short and `total_*` field names. Treating only
   root `usage` as canonical would silently lose cache and thought accounting.
4. **A file mirror still needs Rust-owned module glue.** The five mirrored Go
   files are activated through parent `mod.rs` files and explicit registration;
   no Go package initializer or global registration side effect is emulated.
5. **Stream is the remaining 35-point Interactions risk.** Its two directions
   maintain IDs, output indices, deduplication sets, accumulated arguments,
   reasoning signatures and terminal usage. It will be credited only after an
   event-array differential corpus covers fragmentation and duplicate events.

Strategy adaptation after worker 12o:

- Port the Interactions→Responses stream direction first because it is the
  consuming path for an OpenAI Responses client. Differential fixtures must
  prove text/reasoning/function ordering, idempotent duplicate start/stop,
  completed output reconstruction, usage and `[DONE]`. The reverse stream
  direction follows as a separate gate; neither direction may activate through
  a raw-response fallback.

### Worker 12p — Interactions → Responses stream and 80% gate (20 points)

- The first stateful stream direction is active in the registry. Its
  request-local Rust state tracks output IDs/types, sequence numbers, text,
  reasoning summaries/signatures and function arguments without global erased
  state.
- `step.start`, `step.delta`, `step.stop`, interaction creation/completion,
  `finish`, `done` and raw `[DONE]` map to ordered Responses events. Completed
  output is rebuilt in index order with detailed input/cache/output/reasoning
  usage.
- Repeated function-call start and stop are idempotent. Initial object arguments
  become one arguments delta; later deltas accumulate into the done item and
  completed response. Cancellation stops conversion before state mutation.
- Four new stream event arrays join the eight stateless fixtures and compare
  equal against the pinned Go package. The full gate passes 182 unit tests, 26
  integration tests, all-target Clippy with warnings denied, formatting and
  pinned tracking.

Forensic review at 80%:

1. **Directional activation reduced the blast radius.** The OpenAI Responses
   consuming path is now stream-capable while the reverse Responses→Interactions
   stream remains visibly absent. No raw SSE can silently masquerade as that
   missing translation.
2. **Stream state is protocol state, not transport state.** The converter owns
   semantic accumulation and idempotence per complete upstream event. Byte
   fragmentation remains the HTTP/SSE decoder's responsibility and should not
   be duplicated in every format converter.
3. **Index order is more authoritative than arrival order at completion.** Live
   deltas preserve arrival order, while the terminal `response.output` walks
   known indices. This matches upstream and prevents duplicate or sparse starts
   from reshaping the completed response.
4. **`serde_json::Value` is acceptable here because every event is rewritten.**
   Unlike no-op request paths, this adapter constructs a different protocol
   object for each event. Raw function-argument text is retained separately and
   never round-tripped through an object after it enters delta state.
5. **The remaining 200 points are qualitatively different.** Reverse
   Interactions streaming is the last bounded format slice. Exact Codex token
   counting needs tokenizer assets; Management and safe plugin replacement are
   control-plane/lifecycle projects. A broad mechanical wave would now hide
   more risk than it retires.

Strategy after the 80% challenge:

- Complete the reverse Responses→Interactions stream next for the remaining
  15 Interactions points, with direct-event and Responses SSE-envelope fixtures
  covering text/function deduplication, status lifecycle, usage and terminal
  done.
- Then perform an 81.5% selection gate between tokenizer-exact Codex completion
  and the smallest deployable Management vertical. Do not start the plugin host
  until its process isolation, framing, cancellation and crash-recovery contract
  is written as an explicit CTOX replacement design.

### Worker 13a — Responses → Interactions reverse stream (15 points)

- The second stateful direction is active and the Interactions family is now
  45/45. Responses creation/status, output-item lifecycle, text, reasoning,
  function arguments, step switching, completion usage and terminal done map
  to the upstream Interactions SSE contract.
- Text deduplication is keyed by item/output/content identity, with a separate
  unkeyed-delta guard. Function-argument deduplication recognizes item ID, call
  ID and output index, so `output_item.done` cannot repeat deltas already sent.
- Completion-only responses synthesize missing model-output steps, while a
  normal delta path is not repeated from either item-done or completed output.
  Function/reasoning/message transitions close the previous step before opening
  the next.
- Four reverse-stream event arrays expand the pinned Go/Rust corpus to 16
  fixtures. Fourteen focused converter tests, the bidirectional stream registry
  gate and all-target Clippy pass.

Forensic findings after completing Interactions:

1. **Dedupe identity needs overlapping keys.** Providers inconsistently include
   item IDs, call IDs, output indices and content indices. A single preferred
   key would miss valid fallback events or repeat already delivered content.
2. **Completed responses are both terminal data and recovery data.** They fill
   missing message deltas when an upstream emits only a terminal object, but
   must not replay text that already crossed the streaming boundary.
3. **Status lifecycle is not symmetric with Responses.** Interactions emits an
   explicit status-update after creation and a distinct done event after its
   completed object. The port retains all three boundaries and idempotently
   suppresses repeats.
4. **The mirrored response file can now be marked ported.** Both stream state
   machines, both non-stream converters and shared usage/content helpers are
   active. Its upstream test mirror remains partial because Rust consolidates
   cases rather than copying every Go test function one-for-one.

Strategy adaptation after worker 13a:

- Run the promised 81.5% selection audit before another implementation wave.
  Compare exact Codex token-count feasibility with a deployable Management
  vertical using current dependencies and active server boundaries. Preserve
  the plugin host as a separate isolation design rather than folding unsafe
  dynamic loading into Management.

### Worker 13b — tokenizer-exact Codex CountTokens (5 points)

- The final Codex executor gap is closed with the same cl100k/o200k family
  selection as upstream: GPT-5, GPT-4.1 and GPT-4o use o200k; GPT-4, GPT-3.x,
  empty and unknown models use cl100k.
- Instructions, message text, function names/arguments/results, tool metadata
  and response-format schemas are trimmed, joined and counted in upstream
  order. JSON-valued fields stay as borrowed `RawValue` subtrees so object-key
  order, numeric spelling and escapes reach BPE unchanged.
- `CodexSubscriptionResponsesExecutor::count_tokens` is synchronous and local:
  it neither loads credentials nor calls a transport, reports zero attempts and
  returns the internal Responses-shaped usage object for later boundary
  translation.
- `tiktoken-rs` embeds the tokenizer assets, so runtime operation is offline and
  does not introduce an asset downloader or ambient configuration. Eleven
  fixtures execute the same unexported Go tokenizer/count helpers and Rust
  implementation; every token count is identical.
- The gate passes 188 proxy unit tests, 26 proxy integration tests, all-target
  Clippy with warnings denied and the 11-fixture Go/Rust differential corpus.

Forensic findings after CountTokens:

1. **Raw JSON is token input.** Deserializing schemas into `Value` before
   counting can reorder object keys and change the BPE result even though the
   JSON is semantically equivalent. Borrowed raw subtrees are required here,
   not merely an optimization.
2. **Tokenizer assets are a build dependency, not runtime state.** The selected
   Rust crate ships the exact offline encodings under MIT and supports the
   repository toolchain. This avoids network availability and mutable cache
   paths in production.
3. **Local counting has no account lifecycle.** Upstream's method accepts an
   auth argument through a common interface but does not use it. The Rust
   executor makes this explicit with a zero-attempt outcome and no async call.
4. **Invalid JSON is intentionally fail-closed.** Upstream reaches the helper
   only after request translation; the Rust public boundary reports a typed 400
   error instead of treating malformed JSON as an empty request. Valid-request
   behavior is differentially identical.
5. **Codex is now 80/80.** The next work is not more provider polish; it is the
   smallest deployable Management/control vertical, followed by a second
   strategy review before the safe plugin-host process contract.

Strategy adaptation after worker 13b:

- Begin Management read-only and typed: inventory upstream routes and select a
  useful end-to-end slice that can expose configuration/model/runtime facts
  without creating an HTTP bridge for Business OS records or secrets.
- Keep mutation, OAuth session lifecycle and log-body retrieval out of the first
  Management worker. Each requires its own permission, redaction and persistence
  gate rather than a broad handler-file translation.
- Re-evaluate after the first 5–10 Management points whether the remaining
  control surface should continue vertically or whether the isolated plugin
  process protocol now retires more risk per point.

### Worker 13c — authenticated Management model catalog (5 points)

- `internal/api/handlers/management/handler.rs` is active with the upstream
  five-failure IP counter and 30-minute ban. Local and remote requests both
  require a key; remote clients are rejected unless the typed policy allows
  them.
- CTOX adapts upstream's environment/plaintext fallback into a constructor-time
  secret boundary: callers resolve the key from the CTOX secret store and the
  portable authenticator retains only a SHA-256 digest. Comparison is
  constant-time, Debug is redacted and invalid configuration fails closed.
- A bounded HTTP/1 connection serves
  `GET /v0/management/model-definitions/claude` with both Bearer and
  `X-Management-Key` authentication, no-store response policy and build/plugin
  capability headers. Unsupported channels return an explicit 400 instead of
  an empty success.
- The pinned 15-entry Claude catalog is semantically identical to upstream in
  a Go/Rust differential fixture. A real loopback proves the no-body GET path,
  authentication, headers and JSON response.
- The gate passes 194 proxy unit tests, 26 proxy integration tests, all-target
  Clippy, root `cargo check` using the documented optional-runtime build switch,
  management/Codex/Interactions differential gates and pinned tracking.

Forensic findings after the first Management slice:

1. **Management starts with authentication, not data.** Even static model
   metadata must not create an unauthenticated side channel that later grows
   into config, log or credential access.
2. **The upstream environment override does not fit CTOX.** Runtime secrets
   belong to the typed secret store. Digest-only retention gives compatible
   client behavior without adding a production environment toggle.
3. **GET exposed an HTTP-parser coupling.** The Responses-only parser required
   `Content-Length` for every method. Management made the correct boundary
   visible: GET/HEAD/DELETE may have a zero body without that header, while POST
   still returns 411.
4. **Stable `created` fields are not nondeterminism.** The existing differential
   normalizer removed every field with that name. Model metadata forced the
   runner to scope time normalization to converter operations so catalog drift
   can no longer be hidden.
5. **A standalone loopback is not daemon wiring.** Only 5/30 points are earned.
   The next Management points require typed CTOX lifecycle/secret wiring and
   more source-pinned channels; config mutation and raw log access remain
   separate security gates.

Strategy adaptation after worker 13c:

- Wire the read-only Management handler through an explicit CTOX supervisor and
  secret reference before broadening into mutations. Listener readiness must
  be instance-scoped and remote access must default off.
- Expand static channels from the pinned upstream catalog through a generated
  or source-snapshotted asset with differential drift detection; do not hand-
  maintain a second 100-model catalog indefinitely.
- Keep `/config`, auth-file CRUD, OAuth sessions and request-log bodies closed
  until each has a redaction, permission, persistence and rollback contract.

### Worker 13d — instance-scoped Management supervisor (5 points)

- The read-only Management route is now a real CTOX service boundary on
  `127.0.0.1:12436`, separate from the main model gateway on `:12434` and the
  Codex subscription listener on `:12435`. Its status records
  stopped/waiting/starting/ready/faulted and the service boot path owns the
  process-wide supervisor.
- Startup is fail-closed. The listener is not bound until
  `cliproxyapi-management/management-api-key` exists in the encrypted CTOX
  secret store and contains at least 32 bytes. There is no environment,
  browser or command-line fallback and remote access remains disabled.
- The host reads the plaintext into `Zeroizing<String>`, constructs the
  portable digest-only authenticator, retains only a SHA-256 fingerprint for
  change detection and drops the plaintext snapshot before binding. Deletion,
  invalidation or rotation tears down the listener and rebuilds from a fresh
  store snapshot.
- Two root tests cover missing/weak/valid store state, redacted Debug and a real
  authenticated loopback request through the store-built handler. The full
  gate passes root compile/link, 194 proxy unit tests, 26 integration tests,
  all-target Clippy with warnings denied, the pinned Management differential
  fixture and whitespace validation.

Forensic findings after Management lifecycle wiring:

1. **A secret reference is also an availability state.** Treating a missing
   key as a startup error would either take down CTOX or invite an insecure
   default. `WaitingForSecret` makes the disabled boundary observable without
   binding a port.
2. **Rotation cannot mutate the portable verifier in place.** The authenticator
   deliberately owns only a digest and its attempt/ban state. Rebuilding the
   listener from a new typed snapshot gives rotation an atomic lifecycle edge
   and never adds a plaintext setter.
3. **The host must not retain plaintext for drift detection.** A non-secret
   digest is sufficient to compare snapshots. Explicitly dropping the
   zeroizing value before bind shortens the exposure window to construction.
4. **Management deserves a separate port.** Combining it with the public
   Responses boundary would widen route/auth middleware and make later control
   mutations easier to expose accidentally. The three loopback ports now have
   distinct model, subscription and management ownership.
5. **The remaining catalog should not be hand-copied.** The next bounded worker
   will source-pin and generate additional static model channels with drift
   tests. Config/auth CRUD, OAuth and raw log bodies remain closed.

Strategy adaptation after worker 13d:

- Port the remaining static model-definition channels from one pinned upstream
  source snapshot or generator, with exact Go/Rust differential fixtures and
  an explicit provenance check. Avoid manually maintaining a second catalog.
- Keep Management read-only through that catalog wave. Only after the channel
  inventory is complete should one typed runtime-status endpoint be evaluated;
  mutation routes still require a separate CTOX permission and rollback design.
- The 17-minute cold root test build is a validation-cost finding, not a reason
  to skip service integration tests. Reuse the warm target and continue with
  narrow root filters plus full portable-crate gates per worker.

### Worker 13e — source-pinned static model catalog (5 points)

- `internal/registry/models/models.json` now mirrors the pinned upstream asset
  in the same source-tree position. Its upstream bytes are bound by SHA-256 and
  a colocated provenance note documents the pin/update gate.
- All Management catalog channels are active: Claude, Gemini, Vertex,
  AI Studio, Codex Pro, Kimi, Antigravity and xAI, including `x-ai` and `grok`
  aliases. Codex injects its two image built-ins and xAI its four image/video
  built-ins after replacing any asset entries with matching IDs.
- The Rust registry does not return raw asset JSON. It decodes through a typed
  `ModelInfo`/thinking/config shape and serializes again, preserving Go's
  observable `omitempty` rules and the non-optional `created: 0` default.
  Unknown asset fields fail closed instead of silently disappearing.
- Ten fixtures run the pinned Go registry and Rust Management payload across
  every channel and alias. The complete output is semantically identical. The
  gate passes 195 proxy unit tests, 26 integration tests, all-target Clippy,
  root check and whitespace validation.

Forensic findings after the complete static catalog:

1. **An embedded source asset is not necessarily the served API.** Go first
   decodes `models.json` into `ModelInfo` and encodes it again. Raw inclusion
   leaked explicit `false` values that Go omits and failed to synthesize
   `created: 0` where the source omits it.
2. **Differential coverage found this immediately.** The first 10-channel run
   identified the exact Kimi/xAI `zero_allowed` and Antigravity `created`
   differences. Scoping normalization correctly left these stable fields
   visible instead of hiding the bug.
3. **Built-ins are code, not catalog data.** Codex and xAI deliberately append
   models after asset loading and replace duplicate IDs case-insensitively.
   Copying only `models.json` would therefore still be incomplete.
4. **Typed decoding is an upstream-drift alarm.** `deny_unknown_fields` makes a
   newly added ModelInfo field fail the catalog gate until its Rust semantics
   are explicitly ported. The asset hash separately detects content drift.
5. **Management is now 15/30 and remains read-only.** Static metadata and
   lifecycle are deployable; config/auth mutations, OAuth and raw logs still
   have no permission/rollback contract and remain closed.

Strategy adaptation after worker 13e:

- Evaluate one typed read-only runtime-status route next. It should expose only
  the three local gateway phases/addresses and active provider/model facts that
  are already non-secret, with no Business OS record bridge.
- Do not port broad upstream `/config`, auth-file CRUD or log retrieval merely
  to raise file coverage. Each mutation family remains a separate CTOX policy,
  persistence and rollback project.
- In parallel with that selection audit, write the safe plugin replacement's
  process/framing/cancellation/crash contract before any dynamic plugin code;
  upstream Go shared-object loading will not be translated in-process.

### Worker 14a — isolated plugin process schema (5 points)

- `sdk/pluginabi/types.rs` is active with the pinned ABI/schema versions, all
  method names and the upstream result/error envelope. The first required
  `pluginapi` types are active: metadata, configuration fields and executor
  model scope.
- Lifecycle, registration and capability RPC schemas preserve the observable
  Go JSON contract, including base64 `[]byte`, capitalized metadata fields,
  `omitempty` arrays, missing-field zero values and ignored future fields.
- CTOX adds a separate version-1 process envelope: a four-byte big-endian
  length, an 8 MiB limit, bounded request IDs/methods, optional deadline,
  explicit cancellation and ordered stream chunk/end signatures. Unknown
  outer fields and trailing bytes fail closed; codec errors never echo input.
- The process contract is documented next to the port. It intentionally does
  not yet spawn a process or load a plugin. Seven focused tests, 184
  no-default/202 default unit tests, 26 integration tests in each matrix, both
  Clippy gates and one pinned Go/Rust schema fixture pass.

Forensic findings after the first plugin slice:

1. **The upstream JSON schema is permissive.** Go fills omitted fields with
   zero values and ignores unknown additions. A direct Serde translation with
   required fields or `deny_unknown_fields` broke compatibility even though a
   complete fixture passed. The inner schema now follows Go; the new outer
   process frame owns strict validation.
2. **Go `[]byte` is a base64 JSON string.** Rust `Vec<u8>` would normally
   serialize as an integer array. The lifecycle config needs an explicit
   base64 adapter to preserve wire compatibility.
3. **ABI and process protocol versions are different axes.** Upstream schema
   version 2 describes plugin payload capabilities. CTOX process version 1
   describes correlation, limits and cancellation; neither substitutes for
   the other.
4. **A codec is not isolation.** No safety claim or additional points are made
   for process lifecycle yet. Platform IPC, inherited-handle closure, inflight
   limits, deadline enforcement, crash recovery and restart backoff remain
   mandatory before any plugin execution.
5. **In-process shared-object loading stays retired.** The upstream files and
   signatures remain traceable, but their Rust implementation will use a
   supervised child boundary rather than reproducing Go/cgo failure domains.

Strategy adaptation after worker 14a:

- Implement a bounded async frame reader/writer over an abstract duplex stream,
  then a request table that enforces one terminal response, stream sequence,
  inflight limits, deadline and idempotent cancellation without spawning yet.
- Add Unix-socket tests first and define the Windows named-pipe adapter behind
  the same transport trait. TCP loopback is not a managed-plugin fallback.
- Only after lifecycle and crash/backoff tests may a supervisor launch a child;
  it must inherit no provider credentials or ambient runtime configuration.

### Worker 14b — bounded async transport and inflight state (5 points)

- The process frame now crosses any Tokio `AsyncRead`/`AsyncWrite` boundary.
  The reader checks the four-byte length before allocating payload memory,
  distinguishes clean EOF from truncation and rejects oversized frames. The
  writer emits the full frame and flushes it.
- An inflight table has a hard ceiling of 256 requests. It rejects duplicate
  IDs and already-expired deadlines, expires active requests deterministically
  and treats repeated cancellation as an idempotent terminal action.
- Unary responses terminate exactly once. Streams require zero-based,
  contiguous sequence numbers and a matching terminal `next_sequence`; replay
  after completion is rejected. Directly constructed messages are revalidated
  rather than trusting that every caller used the codec.
- Payload-bearing messages, lifecycle config, envelopes and emitted events no
  longer derive payload-printing `Debug`; diagnostics expose only correlation,
  sizes and status booleans. Tests use a secret sentinel across both codec
  directions.
- Fifteen focused plugin tests, 193 no-default/211 default unit tests, 26
  integration tests in each matrix, both warning-denied Clippy gates and the
  pinned schema differential fixture pass.

Forensic findings after the async session worker:

1. **The allocation gate belongs before `Vec::resize`.** Validating only after
   reading JSON still lets a malicious length prefix choose memory use. The
   reader consumes exactly four bytes, rejects over 8 MiB, then allocates.
2. **Terminality is state, not framing.** A valid frame can still replay a
   unary response, skip stream sequence numbers or emit two ends. The inflight
   table owns those invariants independently of transport.
3. **Cancel must be idempotent.** Cancellation races naturally with terminal
   responses and peer shutdown. A second cancel is a successful no-op, while a
   second response/end remains a protocol violation.
4. **Derived Debug is unsafe for generic RPC.** `RawValue`, config bytes and
   error envelopes may contain credentials or request bodies. Manual Debug
   implementations expose lengths and booleans, never payload content.
5. **The abstract duplex test is necessary but insufficient.** It proves
   fragmentation, EOF and limits without a process. Filesystem socket
   ownership, peer lifecycle and crash behavior remain untested, so no loader
   or child supervisor is enabled.

Strategy adaptation after worker 14b:

- Build the Unix transport as an instance-owned socket directory with strict
  permissions, stale-path cleanup limited to that exact root and a peer
  handshake before requests. Mirror the interface for Windows named pipes.
- Add request deadlines to the future client task so expiry sends one cancel
  and releases its inflight slot even if the child stops reading.
- Only after a real child fixture proves shutdown, crash detection and bounded
  restart backoff should process launch earn the remaining plugin points.

### Worker 14c — real Unix socket and protocol handshake (5 points)

- Unix now has a real instance-scoped transport at
  `<runtime-root>/.cpa/<instance>/s`. Namespace and instance directories are
  `0700`; the socket is `0600`. IDs are allow-listed before path construction.
- A conservative 100-byte path gate runs before bind. The initial descriptive
  directory name failed on macOS even inside its normal temporary directory;
  the short internal namespace now fits both macOS and Linux limits and reports
  a typed error before an opaque bind failure.
- The host sends a random-nonce request and requires a response with the same
  correlation ID, supported plugin schema and expected plugin-ID claim within
  five seconds. A real Tokio Unix peer proves the full frame/handshake path;
  mismatched identity claims fail closed.
- Cleanup is path-race aware. Existing regular files and symlinks are never
  removed. The endpoint records the bound socket's device/inode and removes it
  on drop only if the path still names that exact socket, preserving a
  replacement created after unlink.
- Twenty focused plugin tests, 197 no-default/215 default unit tests, 26
  integration tests in each matrix, both warning-denied Clippy gates and the
  pinned schema differential fixture pass.

Forensic findings after the Unix transport worker:

1. **Unix socket paths are not ordinary paths.** macOS rejected the first
   namespace below `temp_dir()` even though the filesystem path was valid. A
   compact private namespace plus a conservative cross-Unix limit is required.
2. **File type alone is insufficient cleanup ownership.** Another same-user
   process can unlink and replace a socket while the listener remains open.
   Comparing device and inode prevents drop from deleting the replacement.
3. **Stale cleanup must be surgical.** Only a socket at the exact validated
   instance path may be removed. A regular file, symlink or non-directory
   namespace is a hard failure and remains untouched.
4. **A handshake claim is not process authentication.** Permissions restrict
   access to the current user and nonce correlation rejects stale frames, but
   the plugin ID is still self-asserted. The future supervisor must bind the
   accepted peer to the child it launched before advertising readiness.
5. **Platform completion is asymmetric.** Unix is now proven on the operator
   platform; Windows named-pipe lifecycle is still missing and receives no
   implied coverage from the portable codec tests.

Strategy adaptation after worker 14c:

- Implement a test-only child fixture and supervisor that passes only the
  socket locator and a one-shot handshake token, binds the peer to the launched
  child, kills it on host drop and records exit without payloads.
- Gate crash-before-handshake, crash-with-inflight, graceful shutdown and
  bounded exponential restart delay before exposing plugin registration.
- Design the Windows named-pipe endpoint against the same handshake/session
  contract before declaring the safe plugin replacement complete.

### Worker 14d — supervised real child and crash backoff (5 points)

- A Unix supervisor now launches an absolute executable from the typed runtime
  root with `env_clear`, null stdout/stderr, piped stdin and kill-on-drop. It
  accepts no caller-provided child arguments; provider configuration belongs in
  the later RPC lifecycle payload, not command-line or environment state.
- Each launch creates a 256-bit one-shot token, writes its base64 form only to
  child stdin, closes the pipe, and zeroizes both binary and encoded copies.
  The socket proof binds token, random nonce, plugin ID and schema version and
  is compared constant-time before readiness.
- The real child fixture rejects any inherited environment, completes the same
  frame/handshake implementation and handles `plugin.shutdown`. Graceful host
  shutdown waits for its correlated response and exit; active request IDs are
  returned as an explicit aborted set rather than disappearing.
- A crashing fixture exits with code 23 while one stream is inflight. The host
  reports the redacted exit, drains that request, restarts after 20 then 40 ms,
  caps the delay and refuses a third restart beyond the configured budget.
- The full gate passes 197 no-default and 215 default unit tests, 28 integration
  tests in each matrix, both warning-denied Clippy runs, the pinned Go/Rust
  schema fixture and root `cargo check` (with pre-existing root warnings).

Forensic findings after the child-supervisor worker:

1. **Pipe shutdown is not necessarily EOF while the handle lives.** The first
   real run wrote the token and called async `shutdown`, but retained
   `ChildStdin`; the child correctly blocked in bounded `read_to_string` and the
   host reached its five-second handshake timeout. Explicitly dropping the
   handle is the required ownership edge.
2. **A nonce alone does not bind a child.** A socket peer can self-assert a
   plugin ID. The one-shot token is delivered over the spawned child's private
   stdin and never sent by the host on the socket; its proof now also binds ID
   and schema to prevent replay/substitution.
3. **Arbitrary process arguments are another secret channel.** The initial
   generic config allowed extra args for the fixture. That API was removed;
   even redacted Debug would not prevent credentials appearing in process
   listings.
4. **Crash completion must drain request state.** Waiting only for the process
   status would strand inflight calls. Exit reports now carry a stable aborted
   ID set while Debug exposes only its count.
5. **Restart success does not imply stability.** Consecutive failures remain
   counted until the owner explicitly marks the child stable. This prevents a
   crash loop from resetting its budget merely by completing a handshake.

Strategy adaptation after worker 14d:

- Reserve the final five plugin points for Windows named-pipe parity plus one
  real registered capability request through the supervised child. Lifecycle
  scaffolding alone is no longer sufficient.
- Audit the remaining 145-point ledger before choosing the next wave; prioritize
  deployable missing semantics over broad Management mutations or mechanical
  activation of plugin adapter files.
- Keep the test-child mode inside the differential tool only. CTOX production
  wiring must select an explicitly installed executable through typed policy
  before the supervisor becomes reachable.

### Worker 14e — 85.5% semantic-ledger reconciliation (zero points)

- The immutable `completed_workers[].points` sequence sums to exactly 855 and
  remains the authoritative earned total.
- The capability table previously summed to only 950 maximum points because
  its OpenAI Chat Completions family was absent. The historical checkpoints
  also prove that the accepted server-surface allocation reached 55 points,
  not the displayed 65: 10 transport-facing SSE points at checkpoint 4 plus
  45 HTTP/SSE runtime points at checkpoint 9.
- Restoring the 60-point Chat Completions row and correcting HTTP/SSE from 65
  to 55 makes the frozen maximum exactly 1,000 and its earned statuses exactly
  855, without retroactively moving or awarding a point.
- The remaining 145 points are now explicit: OpenAI Chat Completions 60,
  native Gemini 40, typed CTOX adaptation 15, Management 15, scheduling 10 and
  safe plugin replacement 5.

Forensic decision after the ledger repair:

1. Resume with native OpenAI Responses↔Gemini translation because it retires
   the highest-risk shared semantics already reused indirectly by Antigravity.
2. Keep Chat Completions as its own 60-point family; do not mislabel Gemini
   transport or Responses mediation as Chat Completions coverage.
3. Award the next points only after a bounded pinned-Go/Rust request corpus is
   green and only activate the proven translation direction.

### Worker 15a — native Responses → Gemini request direction (5 points)

- The mirrored native Gemini request facade and registration are active and
  partial. Registration advertises only OpenAI Responses→Gemini requests;
  non-stream and stream provider responses remain explicitly unavailable.
- The converter emits native `generateContent` JSON rather than Antigravity's
  `{project, request, model}` envelope. The accepted subset covers string and
  structured messages, system/developer instructions, role splitting,
  data-URL images, inline audio, function calls/results, sanitized function
  names, simple tool schemas, generation controls, structured-output fields,
  reasoning effort, default safety and trailing unsigned model-prefill removal.
- Consecutive model contents coalesce before signature sanitation. In a
  parallel function-call turn exactly the first synthetic call receives the
  Gemini bypass sentinel; sibling calls remain unsigned, and reversed
  consecutive tool results are restored to their pending-call order.
- Six pinned-Go/Rust fixtures are semantically identical. They cover the
  simple request, media and controls, system/developer roles, role splitting
  with trailing prefill, simple tools plus JSON Schema output, parallel calls
  plus reordered outputs, and reasoning followed by a function call.
- Gates pass 201 unit plus 28 integration tests without default transports,
  219 unit plus 28 integrations with defaults, both all-target Clippy matrices
  with warnings denied, all existing differential suites, root `cargo check`
  (with 410 pre-existing warnings) and the pinned upstream package test.

Forensic findings after the first native Gemini direction:

1. **Antigravity is not a reusable wire envelope.** Its request converter was
   a useful semantic reference, but native Gemini requires top-level
   `contents`, safety and generation configuration without project/model
   wrapping.
2. **Parallel-call signing is turn-scoped.** Creating one model content per
   call initially makes every call appear first. Coalescing must happen before
   sanitation so only the first synthetic function call receives the bypass.
3. **Unsigned assistant history is a provider prefill, not ordinary history.**
   Upstream removes a trailing model turn unless it carries reasoning,
   function-call or signature semantics. Preserving it changes generation
   behavior even though the JSON remains valid.
4. **The first differential failure was meaningful.** Rust initially retained
   a synthetic bypass on the reasoning part preceding a function call; pinned
   Go drops it from the thought and keeps it only on the call.
5. **Carrier-aware signed reasoning remains open.** Complex tool-schema
   cleaning, directional/detached signature carriers and native response
   conversion receive no implied coverage; the request file stays `partial`.

Strategy adaptation after worker 15a:

- Complete the carrier/signature request state next, using the upstream
  carrier tests as the corpus. Do not start the response converter while a
  native Gemini multi-turn request can still mis-bind a signed thought.
- Extract schema cleaning only when its full required/union/metadata fixture
  set is ported; a generic whole-document cleaner remains forbidden.

### Worker 15b — validated leading Gemini signature carriers (5 points)

- The Rust signature boundary now recognizes only native Gemini 3.x
  field-2→field-1 protobuf envelopes after bounded standard-base64 decoding.
  Trusted `gemini#`/`google#` prefixes are stripped; Claude/GPT prefixes,
  arbitrary base64, malformed envelopes and the synthetic bypass are not
  mistaken for provider provenance.
- The mirrored carrier codec accepts only versioned `next`, `previous` or
  `standalone` directions and `text`, `function` or `any` targets. It bounds
  envelopes before decoding, rejects nesting, removes all client-authored
  `_cpa_*` metadata before interpretation and validates semantic adjacency.
- The request path now binds validated leading raw or marked carriers to the
  immediately following assistant text or function call even when the model is
  an alias without `gemini` in its name. Carrier envelopes and internal fields
  never reach the provider wire; malformed empty carriers disappear without
  crossing a user-message boundary.
- Four new pinned-Go/Rust fixtures extend the native request corpus to 10:
  raw leading function signature, marked leading text, marked leading function
  and malformed/spoofed carrier. Three focused Rust tests cover the envelope
  validator, codec/nesting and normalization/drop behavior.
- Gates pass 204 unit plus 28 integrations without default transports, 222
  unit plus 28 integrations with defaults, both warning-denied Clippy matrices
  and all 10 native Gemini request parity fixtures.

Forensic findings after the leading-carrier slice:

1. **Model naming is not signature provenance.** A valid carrier deliberately
   restores native Gemini layout for an alias model; looking for `gemini` in a
   model string would lose signed history.
2. **Base64 validity is far too weak.** The accepted opaque value still needs
   the observed protobuf field-2→field-1 envelope and Tink/UUID payload shape;
   otherwise cross-provider or client data could be replayed as Gemini state.
3. **Internal metadata is derived state.** `_cpa_reasoning_*` fields are always
   stripped first and recreated only from a successfully decoded carrier, so
   duplicate/spoofed client fields cannot steer binding.
4. **This is leading-carrier coverage, not full pairing parity.** Previous and
   standalone carriers, detached-after reordering, alternating post-call
   signatures and mismatch boundaries remain open. The carrier and request
   files therefore stay `partial`, and response capabilities stay disabled.

Strategy adaptation after worker 15b:

- Port previous/standalone and post-call pairing as a separate differential
  state-machine worker before declaring the native request direction complete.
- Then close path-local Gemini schema cleaning; only after both gates should
  the non-stream response facade enter the module graph.

### Worker 15c — previous, post-call and standalone Gemini carriers (5 points)

- Carrier binding now handles all three upstream directions. A validated
  `previous` carrier can sign the immediately preceding assistant text or
  function call; an unmarked detached signature uses the same conservative
  previous-first rule before trying the following semantic item.
- A post-call function signature binds only when a matching
  `function_call_output` follows before a user-message boundary. This prevents
  a detached carrier from retargeting a stale function call in later turns.
- `standalone` carriers remain independent signed Gemini model parts and never
  donate their signature to the next function. Provider-native coalescing then
  preserves the signature while the final sanitizer still removes unsigned
  parallel-call signatures.
- Four pinned-Go/Rust fixtures extend the native Gemini request corpus to 14:
  marked previous function, unmarked post-call function with matching output,
  marked previous text and standalone function carrier.
- Gates pass 204 unit plus 28 integrations without default transports, 222
  unit plus 28 integrations with defaults, both warning-denied Clippy matrices,
  formatting, all 14 Gemini request parity fixtures and the CTOX root check.

Forensic findings after the full carrier-direction slice:

1. **Post-call binding needs output evidence.** Adjacency alone can attach a
   detached signature to a stale call; the matching output is the transaction
   witness that makes previous-function binding safe.
2. **Conversation turns are hard boundaries.** A user message stops the output
   search, so signatures cannot cross from one semantic turn into another.
3. **Standalone is a wire-layout instruction.** It must survive as its own
   signed model part rather than being treated as an unclaimed `next` carrier.
4. **Ordering is load-bearing.** Carrier binding occurs before item conversion
   and model-content coalescing; signature sanitation occurs only afterwards.
5. **The request facade remains partial.** Alternating/consecutive carrier
   stress cases and path-local schema cleaning remain open, and both native
   Gemini response capabilities stay disabled.

Strategy adaptation after worker 15c:

- Finish path-local Gemini tool/response-schema cleaning with a pinned
  required/union/metadata corpus before promoting the request facade.
- Add adversarial alternating-carrier fixtures alongside that schema slice;
  do not widen registry capabilities until both the schema and ordering corpus
  are green. Then begin non-stream response conversion as a separate worker.

### Worker 15d — path-local Gemini schema cleaning (5 points)

- `internal/util/gemini_schema.rs` is now in the Rust module graph and exposes
  the Gemini tool-schema cleaner. The request converter invokes it only for
  each function declaration's `parametersJsonSchema`; conversation arguments
  and structured response schemas never enter this destructive traversal.
- The cleaner ports the provider compatibility transformations for refs,
  const/enums, enum and constraint hints, allOf merge, anyOf/oneOf selection,
  nullable type arrays, unsupported metadata/extensions and required-name
  repair. Property names that happen to equal schema keywords remain data.
- Six schema fixtures cover required/title/metadata, enum/constraints,
  union/nullable, allOf/ref, placeholders and the path-local safety boundary.
  Two additional alternating next/previous function-carrier histories close
  the adversarial ordering corpus. All 22 pinned-Go/Rust request fixtures are
  structurally identical.
- Gates pass 206 unit plus 28 integrations without default transports, 224
  unit plus 28 integrations with defaults, both warning-denied Clippy matrices,
  formatting, 22 request parity fixtures and the CTOX root check.

Forensic findings after the schema slice:

1. **A schema cleaner is not a JSON cleaner.** Applying it to a request would
   delete legitimate function-call argument keys such as `title`, `format`,
   `default` and `const`; the Rust API accepts a single schema value and is
   called only at the tool declaration path.
2. **Property maps are a protected namespace.** Unsupported keyword names are
   removed from schema objects but preserved when they are user property names.
3. **Upstream quirks are compatibility behavior.** Its path matcher retains
   root-level `_`/`reason` placeholders while removing the same generated forms
   when nested. The differential corpus caught this; Rust now matches rather
   than silently changing wire semantics.
4. **Tool and response schemas intentionally differ.** Gemini tool schemas are
   flattened and metadata-cleaned, while OpenAI `responseJsonSchema` is passed
   through unchanged by this converter.
5. **Request capability is now bounded by a 22-case corpus.** Native response
   conversion remains entirely disabled and is the next independent risk area.

Strategy adaptation after worker 15d:

- Start with native Gemini non-stream response conversion and a dedicated
  Go/Rust corpus for text, thought signatures, function calls, finish states
  and usage. Keep stream response registration empty.
- Only after non-stream parity should a stateful SSE worker port incremental
  event ordering and carrier direction across chunks.

### Worker 15e — native Gemini non-stream response (10 points)

- The native Gemini response facade is active for complete JSON responses and
  registered independently from streaming. It accepts direct Gemini payloads
  and Vertex-style `response` wrappers, normalizes response IDs/timestamps and
  maps provider parts into ordered Responses output items.
- The implementation reuses the already ported shared Gemini part algorithm in
  the Antigravity adapter. This avoids a second copy of the reasoning/message/
  function/carrier state machine while keeping a native mirrored facade and a
  direction-specific Registry capability.
- Request echo fields, model fallback, usage and cached/reasoning token details,
  signed reasoning, visible text, sanitized tool-name restoration, function
  arguments and leading/trailing detached carriers are covered. Dynamic
  function IDs and timestamps are normalized only in the differential probe.
- Seven new pinned-Go/Rust non-stream fixtures pass alongside all 22 request
  fixtures. Gates pass 206 unit plus 28 integrations without default
  transports, 224 unit plus 28 integrations with defaults, both warning-denied
  Clippy matrices, formatting and the CTOX root check.

Forensic findings after the non-stream response slice:

1. **The provider core was already shared.** Antigravity unwraps a Gemini
   response envelope but its output-part semantics are native Gemini's. A thin
   facade is safer for upstream sync than maintaining two 400-line copies.
2. **Capability direction is easy to invert.** `register_pair` stores the
   response under provider→client, while pipeline translation receives the
   original client→provider pair and reverses it internally. Both capability
   inspection and a real Registry transform are now tested.
3. **Original requests own response echo and name recovery.** The translated
   Gemini request cannot reconstruct every OpenAI option or the pre-sanitized
   tool name, so valid original JSON takes precedence.
4. **Dynamic identities are not ignored semantically.** Tests normalize only
   timestamp/call-ID values; prefixes, item relationships, order and carrier
   direction remain exact comparisons.
5. **Non-stream parity says nothing about chunk state.** Stream registration is
   still absent; replay across chunks, terminal once-only behavior and ordered
   SSE events are the remaining 10 Gemini points.

Strategy adaptation after worker 15e:

- Reuse the shared incremental Gemini state only after a native facade proves
  direct JSON, `data:` framing, Vertex wrappers, `[DONE]`, fragmentation-like
  chunk sequences and duplicate terminal suppression against pinned Go.
- Activate the stream Registry slot only in the same worker that passes that
  corpus; no fallback to raw provider chunks is acceptable.

### Worker 15f — native Gemini stream response (10 points)

- The native Gemini response facade now exposes an incremental converter and
  a request-scoped state type. The Registry activates request, non-stream and
  stream directions together, and its test executes a real two-chunk stream
  rather than inspecting capability flags alone.
- Direct JSON chunks, `data:` framing, Vertex wrappers, `[DONE]`, duplicate
  terminal suppression, signed reasoning, signed functions and visible text
  with trailing signatures are covered by seven pinned-Go/Rust fixtures.
- The first differential run found two missing cases in the shared Gemini
  stream core: signed function calls require a detached `next:function`
  carrier before the call, while signatures attached to visible or empty text
  require a `previous:text` carrier after that semantic item. The shared state
  now tracks pending and emitted signatures and flushes pending state at the
  terminal boundary.
- All seven native Gemini stream fixtures, seven non-stream fixtures, 22
  request fixtures and all 12 existing Antigravity differential fixtures pass.
  Gates pass 206 unit plus 28 integrations without default transports, 224
  unit plus 28 integrations with defaults, both warning-denied Clippy matrices,
  formatting and the CTOX root check.

Forensic findings after the native stream slice:

1. **Shared implementation needs shared regression evidence.** Reusing the
   Antigravity Gemini stream core reduces drift, but every native correction
   can change Antigravity output; its full differential corpus is therefore a
   required gate for changes in this state machine.
2. **Thought signatures are output items, not decorations.** A signature on a
   visible text or function part can become a separate detached carrier. That
   also shifts `output_index` and global `sequence_number`, so exact event
   order is part of compatibility.
3. **Terminal behavior is stateful.** `[DONE]` produces completion only after
   the stream has started, and repeated provider terminal markers must not emit
   duplicate `response.completed` events.
4. **The converter boundary consumes decoded SSE data payloads.** Framing is
   handled before translation. The current multi-chunk corpus proves semantic
   state across complete provider payloads; it does not claim support for a
   single JSON object split at arbitrary byte offsets inside this function.
5. **Gemini is now 60/60.** Request, non-stream response and stream response
   are independently differential-gated; no remaining Gemini points are being
   inferred from scaffold files.

Strategy adaptation after worker 15f:

- Freeze the completed Gemini family behind the request, non-stream, stream and
  Antigravity regression corpora. Do not grow it merely to increase mirrored
  file count.
- Audit the remaining 105 points before choosing the next worker. The next
  checkpoint must cross 90% with a real Management, typed CTOX, scheduling or
  plugin capability; Chat Completions remains a separate 60-point provider
  family and must not be conflated with Responses compatibility.

### Worker 16a — typed read-only Management runtime status (5 points)

- `GET /v0/management/runtime-status` is active only when the CTOX host injects
  a typed status source. The portable handler owns authentication and response
  shape; it has no path, SQLite, runtime-state or Business OS dependency.
- The response reports the main Responses gateway, Codex subscription gateway
  and Management gateway as typed phase/address pairs plus the persisted active
  provider and model when present. It deliberately excludes upstream URLs,
  internal error strings, secret state, filesystem paths and Business OS data.
- Main Responses supervision now records root-scoped stopped/starting/ready/
  faulted state at its actual thread, bind and accept lifecycle edges. Codex
  and Management reuse their existing supervised phase records rather than
  inferring readiness from configuration or credential presence.
- Authentication runs before the injected source is called. The status read
  uses only an existing runtime-state record and never resolves or initializes
  missing state, so the new GET remains side-effect-free.
- Gates pass 207 unit plus 28 integrations without default transports, 225
  unit plus 28 integrations with defaults, both warning-denied Clippy matrices,
  a focused CTOX root test, the full root check, the 10-case Management catalog
  differential gate, tracking and whitespace validation.

Forensic findings after the first 90% worker:

1. **Configured is not ready.** Main-gateway status required real lifecycle
   evidence at thread creation, bind, accept failure and retry. Reusing its
   configured address alone would have produced a false health signal.
2. **Authentication must precede observation.** An unauthorized request does
   not invoke the source at all; this prevents future, more expensive status
   collectors from becoming an unauthenticated timing or resource side channel.
3. **Read-only includes hidden writes.** `load_or_resolve_runtime_state` can
   initialize defaults, so the endpoint uses the existing-state loader and
   omits provider/model when no record exists.
4. **Error detail is not status data.** Supervisor error text can contain local
   paths or transport detail. Only the typed `faulted` phase crosses the API;
   errors remain in the owning local diagnostics channel.
5. **This is a CTOX adaptation, not invented upstream parity.** The pinned Go
   Management API has no equivalent aggregate route. Acceptance is therefore
   based on the injected boundary and real host lifecycle tests, while the
   existing 10-case upstream Management catalog differential remains mandatory.

Strategy adaptation after worker 16a:

- The 90% gate is crossed with an operational control-plane capability. Keep
  the final 10 Management points reserved for explicitly permissioned,
  transactional mutation or OAuth lifecycle work; do not expose raw config,
  auth files or logs as a shortcut.
- Reconcile the remaining 100 points as Chat Completions 60, typed CTOX
  adaptation 15, Management 10, scheduling 10 and plugin completion 5. The next
  worker should close one of the smaller lifecycle families before beginning
  the large Chat Completions matrix.

### Worker 16b — upstream-parity account scheduler (10 points)

- The runtime router now uses a stateful scheduler with typed round-robin,
  fill-first and smooth weighted-round-robin strategies. The configured
  strategy is parsed once through `CliproxyRuntimeConfig` and propagated into
  the Claude, Codex and Antigravity account pools.
- Account metadata now carries priority, weight, WebSocket eligibility and an
  optional supported-model set. Selection filters disabled, cooling, tried,
  pinned and model-incompatible accounts before applying the highest available
  priority, matching upstream's failover boundary.
- Single-provider Codex/xAI selection prefers WebSocket-capable accounts before
  priority when the downstream request is a WebSocket and no account is
  pinned. Mixed selection reuses that path when only one provider remains;
  otherwise its cursor operates over ready accounts rather than equally over
  provider names.
- Model state keys remove only the final validated thinking suffix, so
  `model(high)` and `model(low)` share a cursor with `model`, while unrelated
  prefixes remain independent. Smooth weighted state resets when the eligible
  account/weight set changes, preventing stale accumulated weights.
- Eleven fixtures run the same sequential choices inside the pinned Go
  scheduler and the Rust scheduler: priority, all three strategies, zero
  weights, WebSocket preference, pinning, mixed-provider ordering and weights,
  thinking suffixes and tried-account exclusion. All outputs are identical.
- Gates pass 211 unit tests without default transports, 229 with defaults,
  28 integrations in both matrices, both warning-denied Clippy matrices, the
  full root test compile, 18 focused CTOX host tests, the root check, formatting
  and the 11-case scheduler differential gate.

Forensic findings after the scheduler slice:

1. **Mixed routing is account-weighted, not provider-weighted.** Flattening the
   ready accounts makes a provider with two usable subscriptions receive two
   cursor positions; equal provider rotation would diverge under load.
2. **Transport preference precedes priority.** For Codex/xAI WebSocket
   requests, an enabled WebSocket account wins even when an HTTP account has a
   higher priority. An explicit pin is the only override.
3. **One mixed provider is not a mixed algorithm.** Upstream delegates back to
   single-provider selection, preserving WebSocket behavior. The initial Rust
   implementation missed this branch; the differential corpus now locks it.
4. **Weights are mutable scheduler state.** Smooth weighted accumulators must
   reset when candidates or weights change, or removed accounts distort later
   decisions.
5. **The upstream registry is part of scheduler eligibility.** Differential
   probes must register each account's requested model before rebuilding the Go
   scheduler; an auth record alone is intentionally unavailable for a concrete
   model.

Strategy adaptation after worker 16b:

- Scheduling is frozen at 65/65 behind the eleven-fixture differential gate.
  The remaining 90 points are Chat Completions 60, typed CTOX adaptation 15,
  Management 10 and plugin completion 5.
- Continue with another bounded lifecycle capability before the 60-point Chat
  matrix. The smallest executable next slice is the final plugin capability
  call; it must prove registration plus a real bounded request/response across
  the supervised child, not merely add more process-state tests.

### Worker 16c — cross-platform registered plugin capability (5 points)

- The common process supervisor now performs a typed `plugin.register`
  lifecycle exchange and retains the returned upstream-compatible capability
  declaration. An executor identifier call is dispatched only after a valid
  schema-v2 registration explicitly advertises the executor capability.
- Registration and capability calls use the existing 8 MiB frame codec,
  request IDs, unary Inflight state and a two-second exchange bound. Timeouts,
  malformed responses, mismatched IDs, plugin rejection and invalid metadata
  fail closed without rendering config payloads or plugin response bodies.
- Four real-process integration cases cover empty child environment and clean
  shutdown, crash/inflight abort/restart backoff, successful registration plus
  `executor.identifier`, and rejection of a non-advertised capability before
  child dispatch.
- Windows now uses Tokio named pipes under a deterministic root/instance hash,
  rejects remote clients and applies the identical nonce/plugin/schema/one-shot
  token proof used by Unix sockets. The supervisor and fixture child compile as
  one shared lifecycle implementation on both platforms.
- The mirrored Unix, Windows and unsupported in-process loaders are explicitly
  marked `replaced_by_ctox`; unsupported platforms fail closed rather than
  reverting to in-process loading.
- Gates pass 211 unit plus 30 integrations without default transports, 229
  unit plus 30 integrations with defaults, native warning-denied Clippy in both
  matrices, Windows GNU no-default all-target cross-check and Clippy, one
  pinned-Go/Rust RPC-schema fixture, formatting, tracking and the CTOX root
  check. Runtime execution is proven on Unix; the Windows named-pipe path is
  cross-compiled because this checkpoint ran on macOS.

Forensic findings after plugin completion:

1. **Handshake-ready is not plugin-ready.** Process identity and protocol proof
   only establish a peer. Capability dispatch must wait for a separately
   validated registration snapshot.
2. **Capability claims are authorization data.** A valid process cannot invoke
   an executor method unless its registration advertises that interface; the
   host rejects the call before any bytes reach the child.
3. **The transport is platform-specific, the lifecycle is not.** Keeping one
   supervisor for Unix sockets and Windows named pipes prevents deadline,
   restart and registration semantics from drifting between platforms.
4. **Cross-compilation is not runtime evidence.** Windows API/type correctness
   and the complete child test binary are checked, while actual named-pipe I/O
   remains a release-matrix responsibility on a Windows runner.
5. **Replacement status must be explicit.** Leaving the mirrored Go loader
   files as scaffolds made the safe process host look additive. They now record
   that CTOX intentionally replaces in-process loading.

Strategy adaptation after worker 16c:

- The plugin family is frozen at 25/25 behind its RPC differential, native
  process suite and Windows cross-target Clippy gate. Remaining scope is 85
  points: Chat Completions 60, typed CTOX adaptation 15 and Management 10.
- Before entering the 60-point Chat matrix, audit the typed CTOX gap for a
  bounded provider/model configuration seam that materially completes the
  Business OS/harness independence requirement without exposing browser data
  over HTTP.

### Worker 16d — persisted provider-independent CTOX runtime (15 points)

- CTOX now persists a versioned, non-secret `ctox.cliproxyapi.runtime-config.v1`
  topology in the existing runtime SQLite database. Writes use an immediate
  transaction and optimistic revision check; JSON stores only typed secret
  scope/name handles, never credential values.
- The supervised subscription listener consumes the effective persisted
  Claude/Codex/Antigravity configuration and builds all three native transport
  pools. The automatic Business OS ChatGPT snapshot remains a separate Codex
  source and merges without duplicating or misresolving generic secret records.
- Ready provider/model tuples are projected through the policy-gated
  `ctox.coding.models` command. Every explicit preset carries an allow-listed
  `X-CTOX-Provider` header, so model identity and credential/provider routing
  remain independent all the way from Business OS through Pi to the Rust
  Responses router.
- Pi supplies its public API-key-shaped sentinel only to a validated HTTP
  loopback `ctox-gateway` URL. HTTPS, remote hosts, URL credentials, malformed
  URLs and foreign providers fail closed. The compiled sidecar bundle exports
  and tests the same predicate used by the live agent loop.
- Gates pass 211/229 portable/default unit tests plus 30 integrations in both
  matrices, both warning-denied Clippy matrices, 21 focused CTOX host tests,
  the real Pi→proxy→native Codex loopback, 17 Business OS tests, 2 Pi sentinel
  tests and the full CTOX root check.

Forensic findings after the typed CTOX slice:

1. **Configuration and credentials need distinct persistence contracts.** A
   serializable topology may contain secret handles, but accepting values in
   the same document would make revision history and diagnostics a leak path.
2. **Automatic Codex credentials are not ordinary secret records.** Their
   three handles resolve from one atomically rotated ChatGPT snapshot. Applying
   the generic per-record availability check after merging initially rejected
   a valid mixed Claude+Codex setup; an explicit regression test now fixes the
   boundary.
3. **Provider independence must survive the UI boundary.** A model-only preset
   silently recreates model-name routing. Server-issued, allow-listed headers
   preserve the provider dimension without exposing credentials to JavaScript.
4. **Build success was insufficient for Pi auth policy.** Esbuild accepted an
   internally referenced predicate that the executable test could not import.
   The bundle export and runtime test are now part of the gate.
5. **A concurrent repository reset is operational evidence.** The untracked
   port tree survived while tracked host wiring disappeared. The wiring was
   reconstructed and revalidated before points were awarded; future handoff
   should commit an intentional scope promptly, without sweeping unrelated
   user changes into it.

Strategy adaptation after worker 16d:

- The typed CTOX family is frozen at 15/15 behind revisioned persistence,
  mixed-provider merge, listener-readiness projection and end-to-end Pi gates.
- Remaining scope is 70 points: Chat Completions 60 and Management 10. Before
  the broad format family, finish the bounded Management mutation seam that
  can transactionally update this topology under existing authentication and
  policy, without adding a Business OS HTTP data path or accepting raw secrets.

### Worker 16e — authenticated transactional runtime mutation (10 points)

- The loopback-only Management listener now exposes authenticated
  `GET/PUT /v0/management/runtime-config`. PUT accepts only the versioned
  topology document, `application/json`, and at most 256 KiB; unknown fields,
  raw credential fields, invalid providers/configuration and foreign secret
  scopes fail closed.
- A mutation carries `expected_revision`. The host validates every referenced
  provider secret before an immediate SQLite transaction and returns `409`
  when another writer won. Failed validation, missing credentials or a stale
  revision leave the previous row untouched.
- Responses contain only revision, default provider, per-provider account
  counts and sorted model names. Secret values, secret handle names, upstream
  URLs, proxy URLs and internal store errors never cross the HTTP boundary.
- GET uses the read-only SQLite path and does not initialize a missing config
  table. The running provider supervisor already snapshots this same row and
  rebuilds its native pools after a successful revision change.
- Gates pass 212/230 unit tests plus 30 integrations in both feature matrices,
  both warning-denied Clippy matrices, 23 focused CTOX host tests including a
  real TCP PUT and stale-revision replay, the 10-case Management differential,
  formatting, tracking, Pi/UI regressions and the full CTOX root check.

Forensic findings after Management completion:

1. **Authentication alone is not a secret capability boundary.** Allowing an
   arbitrary secret scope would let a management-key holder turn the proxy
   into a confused deputy. Mutations may reference only the dedicated
   `provider-subscriptions` scope.
2. **Optimistic revision belongs inside the write transaction.** A preliminary
   read improves error classification but cannot prevent a race; the immediate
   SQLite transaction remains the authority and surfaces a typed conflict.
3. **A config response should not echo its request.** Secret handles are not
   secret values, but names reveal credential inventory. The response is a
   deliberately smaller operational summary.
4. **Read-only endpoints must avoid schema creation.** The config snapshot uses
   a read-only connection and checks `sqlite_master`; an absent row/table is
   observation, not an implicit migration.
5. **Mutation activates an existing lifecycle instead of inventing another.**
   The subscription supervisor observes the revisioned row and rebuilds the
   router; no ad-hoc background process or Browser-to-CTOX HTTP data bridge was
   introduced.

Strategy adaptation after worker 16e:

- Management is frozen at 30/30 behind auth-first routing, bounded schema,
  secret-scope confinement, CAS persistence and real loopback mutation tests.
- The only remaining ledger family is OpenAI Chat Completions at 60 points.
  Start it as request/non-stream/stream vertical slices with pinned-Go
  differential evidence; do not infer compatibility from the completed
  Responses family or mechanically activate mirrored files.

### Worker 17a — OpenAI Chat → Claude request direction (10 points)

- The first Chat Completions direction converts model/max tokens/top-p/stop,
  legacy and adaptive reasoning effort, top-level Claude system blocks,
  user/assistant content, URL/base64 images, base64 documents, tool calls,
  grouped tool results, function schemas, cache controls and tool choice.
- The OpenAI Chat format is registered separately from OpenAI Responses. The
  registry advertises only Chat→Claude requests; Chat non-stream and stream
  responses remain explicitly unavailable.
- Eight fixtures execute the same payloads in pinned Go and Rust and match
  semantically after removing only upstream's process-random pseudonymous
  metadata ID. Tool identifiers, content order, controls, schemas and cache
  boundaries are not normalized.
- Invalid JSON remains byte-identical. System-only input receives upstream's
  minimal empty user turn, and consecutive tool results coalesce into one
  Claude user message instead of introducing invalid alternating roles.
- Gates pass 214/232 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting and the eight-case
  pinned-Go differential. Tracking regeneration is part of the artifact update.

Forensic findings after the first Chat slice:

1. **OpenAI Chat and Responses are different registry formats.** Reusing the
   completed Responses registration would make the request appear supported
   while sending the wrong input contract through the pipeline.
2. **System content changes envelope structure.** Chat system messages become
   Claude's top-level block array, not ordinary messages; a system-only request
   still needs a minimal conversational turn for downstream validation.
3. **Tool-result adjacency is protocol behavior.** Consecutive OpenAI `tool`
   messages must merge into one Claude user content array to preserve valid
   role alternation.
4. **Upstream metadata is intentionally nondeterministic.** Differential
   normalization removes only its process-local pseudonymous `user_id`; using
   it as parity evidence would confuse randomness with request semantics.
5. **Request parity does not imply response parity.** The response facade
   remains a scaffold and registry assertions prove both response capabilities
   are off.

Strategy adaptation after worker 17a:

- The 95% gate is crossed with 10/60 Chat points. Freeze the accepted request
  corpus and port Claude→Chat non-stream next, including text, thinking,
  tool calls, finish reasons and usage. Streaming remains off until a separate
  state/order/terminal differential gate.

### Worker 17b — Claude → OpenAI Chat non-stream response (10 points)

- The aggregate converter consumes Claude `data:` events and emits one OpenAI
  Chat completion with the upstream message ID, model and wall-clock creation
  time. Text and thinking deltas retain order within their respective fields.
- Tool calls accumulate partial JSON by Claude content-block index, default an
  empty argument stream to `{}`, and are compacted in ascending index order.
  Any completed tool call forces the OpenAI `tool_calls` finish reason.
- Claude input, output, cache-read and cache-creation usage is merged exactly
  like upstream. Prompt totals include both cache classes and expose the two
  cache counters in `prompt_tokens_details` only after usage was observed.
- Four pinned-Go/Rust fixtures cover text/cache usage, reasoning with length
  termination, sparse out-of-order tools and empty defaults. Differential
  normalization removes only the independently sampled wall-clock timestamp.
- The registry now advertises request and non-stream response support for the
  Chat/Claude pair. Streaming remains explicitly unavailable.
- Gates pass 215/233 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting, eight request fixtures and
  four non-stream fixtures.

Forensic findings after the non-stream Chat slice:

1. **A dynamic field can still reveal a semantic bug.** The first Rust version
   emitted `created: 0`; the differential harness normalized the timestamp and
   therefore could not detect it. A direct Rust assertion now requires a
   positive Unix timestamp after `message_start`.
2. **Claude cache tokens are part of prompt usage.** Adding only
   `input_tokens` under-reports OpenAI prompt and total usage; both cache-read
   and cache-creation tokens participate in the totals.
3. **Sparse tool indices are protocol indices, not output-array positions.**
   A sorted map preserves upstream traversal while compacting the OpenAI
   `tool_calls` array without gaps.
4. **Non-stream parity cannot activate streaming.** Aggregating the full SSE
   body has no request-local event state, terminal-once rule or fragment
   handling, so the stream registration stays off.

Strategy adaptation after worker 17b:

- Chat Completions stands at 20/60 and overall coverage at 96%. Freeze the
  aggregate converter behind its direct and differential gates. Next port the
  Claude→Chat stream as a stateful event converter with creation/response
  identity, text, reasoning, completed tools, finish/usage and terminal-once
  evidence; only that gate may activate the stream capability.

### Worker 17c — Claude → OpenAI Chat stream response (10 points)

- A request-local typed state now preserves response identity, creation time,
  accumulated usage and partial tool arguments across Claude events. The
  registry activates the stream direction only through this state adapter.
- `message_start` emits the assistant-role chunk using the client-selected
  model. Text and thinking become `content` and `reasoning_content` deltas;
  partial tool JSON stays buffered until `content_block_stop`, then emits one
  complete tool delta with the original sparse Claude index.
- `message_delta` maps all upstream finish reasons and merges input/output plus
  both cache token classes into OpenAI usage. Ping, message-stop, malformed,
  non-data and unknown events emit nothing; Claude error events retain the
  upstream OpenAI error envelope.
- Four pinned-Go/Rust sequence fixtures cover text/reasoning/cache usage,
  multi-chunk and empty tool arguments, sparse tool indices, empty text,
  length/tool finish reasons, ignored frames and provider errors. Only the
  independently sampled wall-clock creation time is normalized.
- Gates pass 216/234 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting and all 8 request + 4
  non-stream + 4 stream Claude Chat fixtures.

Forensic findings after the Claude Chat vertical pair:

1. **The stream model is the requested model, not provider metadata.** Go
   deliberately ignores `message.model` for chunks and carries the translator
   argument; Rust now preserves the same client-visible identity.
2. **Tool JSON is atomic at this compatibility boundary.** Claude argument
   fragments produce no OpenAI chunk until block stop; emitting each fragment
   would be a plausible optimization but not upstream behavior.
3. **The translator emits JSON chunks, not the HTTP `[DONE]` sentinel.** The
   OpenAI handler owns SSE framing and the terminal sentinel, matching the Go
   layer boundary. `message_stop` therefore emits no converter payload.
4. **Capability direction and invocation direction differ in the registry.**
   Capability lookup is provider→client; the pipeline call supplies the
   client/provider pair and internally resolves the reverse response key. A
   real registry execution test now locks this non-obvious contract.

Strategy adaptation after worker 17c:

- Claude/OpenAI Chat is complete as a 30-point vertical pair and total coverage
  is 97%. Audit the remaining Chat provider paths before assigning the last 30
  points; prefer thin adapters over the already verified Gemini, Antigravity
  and Codex Responses cores only where pinned-Go fixtures prove that the Chat
  envelopes retain provider-specific semantics.

### Worker 17d — OpenAI Chat ↔ Gemini vertical pair (10 points)

- The native Chat request converter preserves Gemini's own envelope instead of
  chaining through Responses: model, generationConfig passthrough, reasoning
  effort, sampling/token/candidate controls, structured output, modalities,
  image configuration and default safety settings are mapped directly.
- System/developer messages, single-system fallback, text, image, video, file
  and audio parts, assistant reasoning, function calls and paired tool results
  retain Gemini role/part ordering. Function declarations use the existing
  schema cleaner; built-in Google Search, Code Execution and URL Context tools
  remain separate native tool nodes.
- Non-stream responses preserve multiple candidates, candidate indices,
  reasoning/content concatenation, completed functions, inline images, native
  finish reasons and usage details. The request-derived name map follows the
  upstream top-level-tool quirk rather than guessing from nested declarations.
- Streaming keeps per-candidate function indices, tool-seen and finish state.
  Finish appears only when the current chunk also carries usage; usage-only,
  direct/data-prefixed and `[DONE]` inputs match upstream behavior.
- Eight pinned-Go/Rust fixtures cover all three directions. Dynamic function
  IDs and creation timestamps alone are normalized. Direct Rust tests exercise
  request history/media, multi-candidate non-stream, retained stream state and
  real registry activation.
- Gates pass 220/238 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting and all eight Gemini Chat
  differential fixtures.

Forensic findings after the Gemini Chat pair:

1. **Responses composition was observably wrong.** It would add native
   function IDs, change inline-data/signature spelling, omit built-in tools and
   collapse Gemini's multi-candidate Chat semantics.
2. **Tool-result strings preserve their raw JSON representation.** A Chat tool
   result `"done"` becomes the string containing `\"done\"`; treating it as
   already-decoded text was a differential mismatch.
3. **Upstream's name-map input is narrower than the request converter.** The
   response helper reads top-level tool names while Chat declarations normally
   nest under `function`. Rust intentionally preserves that quirk rather than
   restoring names more aggressively.
4. **Stream finish is candidate-local and usage-gated.** A finish reason seen
   earlier is cached per candidate but is emitted only on a chunk with usage;
   a global boolean would corrupt `n > 1` streams.

Strategy adaptation after worker 17d:

- Overall coverage is 98%. Reuse Gemini's proven response primitives for the
  Antigravity envelope only where the wrapper semantics are identical; keep
  Antigravity's disambiguated tool-name map, project/request nesting and final
  finish policy provider-local. Codex remains last because its Responses-event
  tool lifecycle and image deduplication are structurally different.

### Worker 17e — OpenAI Chat ↔ Antigravity vertical pair (10 points)

- The request direction keeps Antigravity's `project/request/model` envelope,
  native safety settings, generation controls, structured output and media
  spelling while reusing only Gemini primitives proven byte-shape compatible.
- Tool declarations are deduplicated in input order. Distinct names that
  sanitize to the same identifier receive the upstream deterministic SHA-256
  suffix; tool choice, history calls, result IDs and response names use the
  same reversible map.
- Non-stream responses unwrap the Antigravity response member, preserve Gemini
  content/reasoning/tool/image/usage semantics and restore colliding names.
  Missing response members retain upstream's empty-output behavior.
- Streaming is request-local, consumes only Antigravity candidate zero and
  emits the upstream empty completion shell for an empty response. Tool state,
  finish and usage reuse the previously verified Gemini stream primitive only
  after wrapper and candidate semantics have been applied.
- Eight pinned-Go/Rust fixtures cover all three directions. Four direct Rust
  tests additionally lock collision reversal, first-candidate behavior, the
  empty shell and real registry activation.
- Gates pass 224/242 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting and all eight Antigravity
  Chat differential fixtures.

Forensic findings after the Antigravity Chat pair:

1. **Antigravity is not merely Gemini with another URL.** The project envelope,
   response wrapper, candidate-zero restriction and empty stream shell are
   observable Chat-contract differences.
2. **Sanitization must be collision-aware and request-global.** Mapping each
   declaration independently aliases `read file` and `read/file`; the stable
   six-byte SHA-256 suffix keeps both callable and response names reversible.
3. **Tool-result strings preserve the raw JSON token.** A Chat string result is
   encoded as a string containing its original quoted JSON representation;
   parsing it early changes the provider payload.
4. **Shared primitives require a provider-local facade.** Gemini's part and
   usage conversion is reusable only after Antigravity wrapper, name and
   candidate policy has been enforced and differential-tested.

Strategy adaptation after worker 17e:

- Overall coverage is 99%. The final 10-point slice is Codex Chat and must be
  implemented as its own Responses-event lifecycle: request input/instructions
  plus non-stream and stateful stream output, function argument events, image
  deduplication, finish and usage. After it passes pinned-Go parity, run the
  complete tracking/upstream/release audit before declaring the port complete.

### Worker 17f — OpenAI Chat ↔ Codex vertical pair (10 points)

- Chat requests become native Responses input with developer/user/assistant
  messages, text/image/file/audio parts, structured output, verbosity, default
  or explicit reasoning effort, encrypted-reasoning opt-in and `store:false`.
- Function, custom and built-in tools retain their distinct Responses shapes.
  Long names use the upstream 64-byte unique shortening map; history calls,
  missing/ambiguous IDs, typed rich tool outputs and reverse response names
  follow the same request-local metadata.
- Non-stream responses combine reasoning summaries, visible text, function and
  custom calls, generated images, usage details and complete/incomplete finish
  reasons into the OpenAI Chat envelope.
- The stream converter tracks response identity, tool indices and argument
  lifecycle, suppresses done-event duplicates, handles skipped added events,
  deduplicates identical partial/final images by item and emits terminal usage
  and finish exactly once per provider event.
- Nine pinned-Go/Rust fixtures cover all three directions; four direct Rust
  tests lock request composition, aggregate output, state/deduplication and
  actual registry activation.
- Gates pass 228/246 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting and all nine Codex Chat
  differential fixtures.

Forensic findings after the Codex Chat pair:

1. **Codex Chat is a Responses-event facade, not a Responses request alias.**
   Tool call announcements, argument deltas/done events and generated images
   require their own state before Chat chunks can be emitted correctly.
2. **A lookup miss with an explicit output index is not “current tool”.** The
   upstream state machine falls back to the current call only when no key was
   supplied; doing so after an unknown index swallowed a second completed tool.
3. **Function-shaped calls can target custom tools.** Request-local declaration
   metadata decides whether history becomes `function_call` or
   `custom_tool_call`; the response always returns Chat's function envelope.
4. **Image deduplication is item-local.** Repeated bytes for one image item are
   suppressed while changed progressive bytes and the same bytes under another
   item remain observable.

Strategy adaptation after worker 17f:

- The frozen 1,000-point semantic ledger is complete, but this is not by itself
  a repository-wide full-port claim. The generated mirror still contains 927
  scaffolds: 516 production and 411 upstream-test files. Checkpoint 18 therefore
  keeps the project open and classifies production scaffolds by executable
  reachability and replacement status before new bodies are activated.

### Checkpoint 18 — repository-wide mirror closure audit

The semantic gateway, subscription-provider and harness scope has reached
1,000/1,000 points. All 18 differential suites pass, both crate feature
matrices and Clippy matrices pass, tracking is pinned, and the CTOX root build
passes with pre-existing warnings.

This audit deliberately rejects “1,000 points = every upstream file ported”.
After the Codex files are regenerated into `port-map.json`, 927 mirrored files
remain scaffolds (516 production, 411 tests), concentrated in translator,
runtime, SDK/cliproxy, examples, pluginhost, API, auth and watcher packages.
They are excluded from the Rust module graph and cannot be counted as ported.

Checkpoint 18 remains in progress. Its next control step is a second ledger for
mirror closure that distinguishes: required active parity, intentionally
CTOX-replaced functionality, platform-specific release evidence, examples and
translated upstream tests. Only explicit classification plus passing gates may
reduce this backlog; the goal is not marked complete at the semantic milestone.

### Worker 18a — shared translator primitives and exact MIME semantics

- Four shared translator helpers now mirror cache-control propagation, Claude
  system filtering, OpenAI file-data normalization and Interactions usage-path
  precedence. Gemini's default safety attachment and Claude Code attribution
  detection are active in their mirrored modules.
- File-data normalization initially used the host `mime_guess` database. The
  pinned-Go differential gate exposed `.wasm` as an incompatible extra. The
  final Rust implementation therefore ports the complete closed 732-entry
  upstream MIME table, including upstream aliases and omissions, rather than
  inheriting platform or crate MIME policy.
- Nineteen pinned-Go/Rust fixtures cover cache precedence/no-op behavior,
  system filtering, raw and data-URL files, MIME aliases and omissions, usage
  fallback order, nested safety insertion and Unicode-leading attribution.
  Eight direct Rust tests lock the individual primitives.
- Gates pass 236/254 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting and all 19 new differential
  fixtures.
- Mirror closure advances from 89/605 to 96/605 classified production files;
  509 production scaffolds and 411 upstream-test scaffolds remain open.

Forensic findings after worker 18a:

1. **A mature MIME crate is not an upstream-compatible MIME policy.** It had
   674 extra extensions, missed 30 upstream entries and preferred a different
   MIME value for 47 shared extensions; differential testing a few popular
   formats alone would have hidden this contract drift.
2. **No-op byte identity must be designed explicitly.** Cache-control and
   safety helpers parse only after their upstream preconditions are met and
   return the original bytes for invalid, absent or already-populated paths.
3. **Shared helpers are high-fan-out closure work.** Porting them before the
   remaining translator matrix reduces repeated policy code while still
   requiring each provider pair to prove its own envelope and stream state.

Strategy adaptation after worker 18a:

- Continue translator closure in small vertical pairs, but first reuse these
  exact shared primitives. Select the next pair by upstream dependency fan-out
  and prove request, non-stream and stream directions independently against the
  pinned Go tree; do not treat the 100% semantic ledger as repository closure.

### Worker 18b — OpenAI Responses ↔ Codex compatibility pair

- Responses requests now enforce Codex's required stream/store/parallel/include
  fields, remove unsupported token, sampling, truncation, compaction and user
  fields, retain only the priority service tier and rewrite system roles to
  developer roles.
- String input expands to the native Responses message/content form. Preview
  web-search aliases are normalized in declarations, top-level tool choice and
  allowed-tool lists while unrelated tool types remain untouched.
- Streaming preserves Codex events byte-for-byte except for upstream's exact
  `data:` spacing/trim compatibility. Non-stream output accepts only completed
  or incomplete terminal events and returns the raw `response` subtree without
  reserializing it.
- Eleven pinned-Go/Rust fixtures and five direct Rust tests cover all three
  directions, normalized-request byte identity, tool aliases, completed,
  incomplete, ignored and null terminal events.
- Gates pass 241/259 unit tests plus 31 integrations in both feature matrices,
  both warning-denied Clippy matrices, formatting and all 11 differential
  fixtures. Mirror closure advances to 99/605; 506 production and 411 test
  scaffolds remain.

Forensic findings after worker 18b:

1. **Format identity still has compatibility semantics.** Responses→Codex is
   not a no-op: subscription upstream rejects otherwise valid OpenAI fields and
   legacy built-in-tool names.
2. **The stream and non-stream paths have intentionally different framing.**
   SSE retains whole provider events, whereas non-stream returns only a raw
   terminal response subtree and suppresses intermediate events.
3. **A typed parser can preserve raw payloads selectively.** Borrowed
   `RawValue` validates the terminal envelope while retaining exact response
   bytes; parsing the entire event to `Value` would add avoidable format drift.

Strategy adaptation after worker 18b:

- The thin Codex pair validates the closure method. Continue with another
  bounded pair only after auditing whether it composes from existing native
  cores without losing provider-specific tools, candidates, signatures or
  state; otherwise port the direct upstream converter.

### Worker 18c — OpenAI Chat same-format passthrough

- The same-format request adapter changes only the selected model and preserves
  the original bytes when the model is already correct or input is invalid.
- Streaming strips only an exact `data:` prefix, applies upstream whitespace
  trimming and suppresses `[DONE]`; non-stream responses remain byte-identical.
- Rust keeps the mirrored `openai/openai` files but exposes them as the
  `openai::passthrough` module to satisfy the warning-denied module-inception
  gate without changing the source correspondence.
- Seven pinned-Go/Rust fixtures and four direct tests pass. Full gates pass
  245/263 unit tests plus 31 integrations in both feature matrices and both
  Clippy matrices. Mirror closure reaches 102/605; 503 production and 411 test
  scaffolds remain.

Forensic findings after worker 18c:

1. **Same-format does not mean raw SSE passthrough.** Prefix normalization and
   `[DONE]` suppression are part of the translator contract.
2. **File mirroring and Rust module naming are separate concerns.** Exact
   mirrored paths can coexist with an idiomatic, lint-clean public module name.

Strategy adaptation after worker 18c:

- Thin compatibility pairs are efficient closure units when their complete
  observable contract fits a small differential corpus. Larger Responses↔Chat
  converters remain direct ports and must not be mislabeled as passthrough.

### Worker 18d — Gemini same-format normalization and signature replay

- Gemini requests now normalize missing/invalid roles by alternating user and
  model, translate `functionDeclarations`/`parameters` to the v1beta schema,
  rename response schemas, attach default safety policy and backfill empty
  function-response names from the immediately preceding model calls.
- The previously scaffolded Gemini signature sanitizer is active: only the
  first unsigned function call receives the bypass sentinel, compatible native
  signatures remain canonical, incompatible sibling signatures are dropped
  and function-response signatures are always removed.
- Stream/non-stream passthrough and token-count responses reproduce upstream
  framing, DONE suppression and modality details. Mirrored `gemini/gemini`
  paths are exposed as `gemini::passthrough` to avoid Rust module inception.
- Twelve pinned-Go/Rust fixtures and five direct Rust tests cover request,
  signature, stream, non-stream and token-count directions. Full gates pass
  250/268 unit tests plus 31 integrations in both feature matrices and both
  warning-denied Clippy matrices.
- Mirror closure advances to 106/605; 499 production and 411 test scaffolds
  remain.

Forensic findings after worker 18d:

1. **Provider-native requests still need replay hygiene.** Gemini accepts its
   own envelope, but client history can contain invalid roles, legacy schema
   keys, empty response names and cross-provider signature carriers.
2. **Parallel calls have asymmetric signature policy.** Synthesizing bypass on
   every sibling changes native Gemini history; only the first call is repaired.
3. **Backfill is adjacency- and index-sensitive.** Named responses consume a
   call index, and only the immediately following non-model turn can inherit
   pending call names.

Strategy adaptation after worker 18d:

- Continue extracting high-fan-out correctness helpers before large provider
  matrices, but keep each helper tied to a complete active vertical pair and a
  pinned-Go corpus. The next audit should prefer a pair that reuses the now
  verified Gemini replay and schema boundary without bypassing provider-local
  response semantics.

### Worker 18e — mirrored upstream-test closure for recent pairs

- Five mirrored upstream test files are no longer inert scaffolds: Gemini
  signature sanitizing, Gemini same-format requests, Codex Responses request
  and terminal response, and OpenAI same-format model rewriting now execute as
  file-local Rust test modules.
- The large Go suites are marked `adapted_to_ctox` where their behavioral matrix
  is split between direct Rust tests and the pinned-Go differential runners;
  byte-exact single-case ports retain `ported` status. No test scaffold is
  counted merely because equivalent inline coverage happened to exist.
- Twelve new file-based Rust cases raise the full matrices to 262/280 unit
  tests plus 31 integrations in both modes; both warning-denied Clippy matrices
  remain green. Upstream-test scaffolds fall from 411 to 406 while production
  closure remains 106/605.

Forensic finding after worker 18e:

1. **Behavioral coverage and mirror closure are separate evidence.** Inline and
   differential tests proved behavior, but the agreed file-based port also
   requires each mirrored upstream test file to carry an explicit disposition
   and executable Rust body.

Strategy adaptation after worker 18e:

- Every future production slice closes its directly associated upstream test
  mirrors in the same worker, using `ported` only for direct translations and
  `adapted_to_ctox` when the suite is deliberately split across Rust and
  pinned-Go differential evidence.

### Worker 18f — Antigravity ↔ Gemini and reversible tool-name utilities

- The Antigravity/Gemini request path now wraps the selected project and model,
  moves the system instruction to the upstream field spelling, normalizes roles,
  attaches default safety policy and groups CLI-style function responses in the
  same order as their preceding calls.
- Tool declarations use a request-global, sorted and collision-aware name map.
  Invalid names are sanitized, SHA-256 suffixes disambiguate collisions,
  duplicate declarations are removed, schemas move to
  `parametersJsonSchema`, and allowed-function plus history names are rewritten
  consistently. The response direction reverses those aliases.
- Stream and non-stream responses unwrap the Antigravity envelope, restore
  `cpaUsageMetadata` and tool names, and preserve the pinned upstream's explicit
  alternate-framing behavior. Token-count output is registered separately.
- Five production files and three directly associated upstream-test mirrors are
  newly classified. Two shared utility mirrors are intentionally `partial`:
  the high-fan-out tool-name algorithms are active, while unrelated upstream
  walk/rename/JSON-repair helpers remain open rather than being falsely counted
  as complete.
- Eleven pinned-Go/Rust fixtures, nine pair tests and five utility tests pass.
  Full gates pass 273/291 unit tests plus 31 integrations in both feature
  matrices, both warning-denied Clippy matrices and formatting.
- Mirror closure advances from 106/605 to 111/605; 494 production files and 403
  upstream-test scaffolds remain open.

Forensic findings after worker 18f:

1. **Tool aliases are a request-wide reversible protocol.** Per-declaration
   sanitizing silently aliases different upstream names; deterministic sorting
   and a stable digest suffix are needed so requests and responses agree.
2. **Function-response grouping precedes role repair.** Upstream temporarily
   builds a CLI `function` group and then subjects that group to Gemini's
   user/model normalization. Reordering those passes changes the final role.
3. **Odd framing behavior is still compatibility behavior.** A non-empty
   alternate stream mode currently follows an upstream path that can emit an
   empty result; the differential gate records that quirk instead of improving
   it unilaterally.
4. **Partial utility ports must remain visibly partial.** Landing the functions
   needed by an active vertical slice does not close every body in the mirrored
   Go file.

Strategy adaptation after worker 18f:

- Reuse the now differential-proven collision map in later provider pairs, but
  finish the remaining bodies in both partial utility mirrors before claiming
  support-core closure. Continue pairing each production slice with its own
  test mirrors and preserve provider-specific framing quirks until an explicit
  upstream divergence decision is recorded.

### Worker 18g — Antigravity/Claude Gemini signature carriers

- The first Antigravity/Claude slice ports the Claude-facing carrier envelope
  used to round-trip native Gemini signatures. Encode/decode now validate the
  versioned prefix, direction, semantic target kind, raw-base64 payload,
  nesting prohibition, size bound and provider-native Gemini signature.
- The history filter keeps legacy raw carriers only in assistant messages,
  binds `next` and `previous` markers to adjacent semantic text or tool-use
  blocks, permits an unsigned non-empty thinking block only after a matching
  forward carrier, and removes malformed, mismatched or user-authored replay
  state.
- `signature_validation.rs` remains explicitly `partial`: carrier and Gemini
  history-filter bodies are active, while the Claude E/R strict-mode, cache and
  signature-tree wrappers remain open until the request slice ports their
  owning policy.
- The direct upstream test mirror contributes five executable Rust cases. Ten
  pinned-Go/Rust fixtures independently cover encoding, marked and legacy
  decoding, invalid envelopes, role boundaries and directional adjacency.
- Full gates pass 278 no-default and 296 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. The
  differential runner uses `--no-default-features` because this pure format
  boundary does not need to rebuild the BoringSSL transport feature island.
- Mirror closure advances from 111/605 to 112/605; 493 production files and 402
  upstream-test scaffolds remain open.

Forensic findings after worker 18g:

1. **A thinking block can be transport, content or both.** Empty carrier blocks
   attach a native signature to neighboring semantic content, while a matching
   forward carrier may authorize the following non-empty thought without a
   second signature.
2. **Direction and target kind are security-relevant replay metadata.** A valid
   provider signature is still discarded when its declared text/function
   adjacency does not match the Claude history returned by the client.
3. **Legacy compatibility has a role boundary.** Unmarked Gemini signatures are
   accepted in assistant history but never in user-authored thinking blocks.
4. **Format-only gates must not pull transport feature islands.** Running the
   differential binary without default features avoids unnecessary native TLS
   builds and reduces both disk churn and false infrastructure failures.

Strategy adaptation after worker 18g:

- Port the Antigravity/Claude request next, using this carrier filter as an
  invariant. Split cache-backed Claude E/R handling from pure message/media/tool
  conversion in the differential corpus, and keep response plus web-search
  capabilities unregistered until their independent state-machine gates pass.

### Worker 18h — Antigravity/Claude normal request path

- The normal Claude-to-Antigravity request path now converts system prompts,
  alternating roles, text, images, tool calls, tool results, thinking blocks,
  generation controls, tool choice and default safety settings into the pinned
  Gemini request envelope.
- Tool declarations, calls, results and forced tool choice share one
  deterministic request-wide sanitized-name map, including collision handling.
  Tool-result images stay inside the owning `functionResponse.parts` instead of
  becoming unrelated sibling message parts.
- Antigravity schema cleaning is now a distinct utility policy. It retains
  metadata stripped by the plain Gemini cleaner and reproduces VALIDATED-mode
  placeholders for empty or optional-only object schemas.
- Carrier binding from worker 18g is integrated into the request conversion.
  Thinking and semantic parts are reordered like upstream, with signatures
  kept next to their semantic targets and trailing transport parts kept last.
- The production mirror remains honestly `partial`: native Web Search and the
  cache-backed Claude E/R strict-mode policy are still excluded, so this
  request capability is not globally registered yet.
- Six direct request tests plus the carrier and schema regressions pass.
  Thirteen pinned-Go/Rust fixtures cover the pure conversion path with the Go
  signature cache explicitly disabled, separating converter parity from the
  still-open cache policy.
- Full gates pass 285 no-default and 303 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. Mirror
  closure advances to 113/605; 492 production files and 401 upstream-test
  scaffolds remain open.

Forensic findings after worker 18h:

1. **Tool results own their media.** Images returned by a tool belong inside
   the same `functionResponse.parts`; emitting them as message siblings changes
   the provider's call/result association.
2. **Tool-name normalization is one request-wide protocol.** Declarations,
   calls, results and forced selection must consult the same collision map;
   local sanitization cannot reverse names reliably.
3. **Thinking order and signature adjacency are coupled.** Reordering content
   without moving its carrier changes replay meaning even when both JSON
   fragments remain individually valid.
4. **Antigravity and Gemini schema cleaning are not aliases.** Antigravity's
   VALIDATED-mode placeholders and metadata retention require a separate entry
   point over a shared recursive cleaner.
5. **Cache-disabled differential parity proves only the pure converter.** The
   request file stays partial until cache lookup/write, strict-mode behavior and
   native Web Search have independent executable evidence.

Strategy adaptation after worker 18h:

- Close cache-backed signature policy and native Web Search as separate slices
  before changing the request mirror to `ported` or registering its capability.
  Keep response and streaming work independent so a converter-only success
  cannot accidentally claim an end-to-end Antigravity/Claude vertical path.

### Worker 18i — Claude signature cache and bypass policy

- The upstream text-bound signature cache is now active as a separate Rust
  component with SHA-256/64-bit text keys, model-family buckets, a sliding
  three-hour TTL, exact deletion, group/all clearing, the 50-byte minimum and
  Gemini's cache-miss sentinel.
- Cache-enabled Claude request conversion first resolves `(model group,
  thinking text)` cache hits, then accepts only a sufficiently long client
  signature carrying the matching `group#signature` prefix. An unprefixed or
  cross-group client signature is not treated as a cache replacement.
- The executor-facing Claude policy now distinguishes prefix-only legacy
  cleanup from basic bypass validation and strict protobuf-tree validation.
  Basic mode requires decodable E/R layers and the decoded `0x12` marker;
  strict mode additionally requires the known field-2/container/channel tree.
- The local cache mirror remains `partial`: pinned upstream Home-KV reads,
  writes, exact deletes and error propagation need a typed CTOX durable-store
  adapter. The in-process port uses lazy expiry instead of an otherwise idle
  cleanup task and introduces no environment-controlled runtime behavior.
- The request and signature-validation mirrors also remain `partial`: durable
  cache errors, signature-tree inspection wrappers and native Web Search are
  still open, and the request capability therefore remains unregistered.
- The request differential corpus grows from 13 to 23 pinned-Go/Rust cases:
  four cache-mode cases and six prefix/basic/strict policy cases join the
  existing pure conversion fixtures. Four adapted cache tests and three policy
  regressions directly cover storage, grouping, deletion, mode switching and
  location-aware validation failures.
- Full gates pass 293 no-default and 311 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. Mirror
  closure advances to 114/605; 491 production files and 400 upstream-test
  scaffolds remain open.

Forensic findings after worker 18i:

1. **The two Antigravity caches have different identities.** Claude signature
   replay is keyed by model family plus thinking text and slides for three
   hours; conversation reasoning replay is keyed by model plus session and has
   ordering/CAS semantics. Combining them would corrupt both lifecycles.
2. **Cache mode changes client-signature authority.** A valid raw Claude
   signature accepted in bypass mode is intentionally insufficient in cache
   mode unless it carries the matching model-group prefix or the text lookup
   already produced a cached signature.
3. **Prefix cleanup is not validation.** The first executor pass preserves any
   legacy E/R-shaped block; only the later selected basic/strict policy decides
   whether the payload is replayable.
4. **Strictness is a protobuf-depth choice, not a length choice.** Both modes
   share the 32-MiB bound and base64/marker checks; strict mode alone inspects
   the provider-specific tree.
5. **Durable cache errors cannot degrade to misses.** Upstream's required
   Home-KV API distinguishes storage failure from absence. The Rust mirror stays
   partial until CTOX exposes the same typed failure boundary.

Strategy adaptation after worker 18i:

- Do not bind upstream Home-KV directly into the converter or add an ambient
  toggle. Port native Web Search next as a pure, separately differential-gated
  request branch; then bind cache persistence and executor preprocessing at the
  typed runtime boundary before request registration. Response and streaming
  remain independent follow-up slices.

### Worker 18j — Capability-gated native Web Search request

- The request half of `web_search.go` now builds the independent Antigravity
  `requestType: web_search` envelope with the fixed search-only system
  instruction, one candidate, native `googleSearch`, image-search result count
  and optional trimmed domain allow-list.
- Activation is fail-closed and typed. The converter receives an explicit
  `AntigravityClaudeRequestCapabilities` value; a model string or typed tool
  alone cannot enable native search. The default converter supplies no search
  capability until the runtime model catalog is wired.
- Native search additionally requires an exclusively typed Claude Web Search
  tool list and an absent/auto/any or explicitly matching `web_search` tool
  choice. Mixed custom tools, `none`, a different forced tool or an unsupported
  route stay on the normal request path.
- Query extraction walks backward to the last eligible user/role-less message,
  trims string content and joins non-empty array text parts with newlines.
  Positive `max_uses` maps to image-search `maxResultCount`; absent or invalid
  values retain upstream's default of five.
- `web_search.rs` remains explicitly `partial`: grounding metadata, citations,
  aggregate Claude responses and streaming translation occupy the rest of the
  pinned Go file and require response-state gates. Runtime discovery must still
  project the provider's `SupportsWebSearch` flag into the typed capability.
- Five new pinned-Go/Rust cases extend the shared corpus to 28 and cover the
  capable envelope, default/forced choice, mixed tools, disabled choice and
  unsupported route. Three direct Rust tests cover query/domain/max extraction
  and the fail-closed activation matrix.
- Full gates pass 296 no-default and 314 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. Mirror
  closure advances to 115/605; 490 production files and 400 upstream-test
  scaffolds remain open.

Forensic findings after worker 18j:

1. **Web Search support is discovered state, not a model-name heuristic.** The
   same apparent Gemini family may expose or omit native search at runtime; the
   route must consume the catalog capability for the exact normalized model.
2. **Native search is an independent request class.** It replaces the normal
   chat/system/tool envelope instead of adding `googleSearch` beside function
   declarations.
3. **Mixed tools must stay normal.** Converting a mixed request to the dedicated
   search envelope would silently discard every custom function declaration.
4. **Tool choice is an authorization constraint.** `none` or a different forced
   tool must override the presence of a typed Web Search declaration.
5. **`max_uses` has an upstream-specific target.** It controls native image
   search result count, not a generic tool-call budget, and therefore must not
   leak into ordinary generation settings.

Strategy adaptation after worker 18j:

- Port grounding metadata and aggregate Web Search response translation next,
  followed by its streaming state machine. Only then wire the discovered model
  capability and durable signature-cache errors through the Antigravity
  executor and consider request registration; do not infer capability from a
  hard-coded list in the converter.

### Worker 18k — Web Search grounding aggregate response

- The native-grounding non-stream branch now emits the Claude aggregate order
  required by server tools: `server_tool_use`, `web_search_tool_result`, then
  uncited/cited text blocks. A deterministic injected tool-use ID keeps the
  format core pure while leaving collision-free ID generation to its caller.
- Activation requires evidence on both sides of the translation boundary: the
  original Claude request must contain a typed Web Search tool and the actual
  translated Antigravity request must contain native `googleSearch`. Grounding
  metadata alone cannot synthesize a server tool result.
- Search results trim and de-duplicate URLs while preserving optional titles
  and Claude's explicit `page_age: null`. Citation supports retain their raw
  chunk indexing independently of the deduplicated result list.
- Grounding segment positions are interpreted as UTF-8 byte offsets like Go.
  Overlapping supports are clipped to the previous end; gaps and tails become
  plain text blocks. A support without a valid chunk still advances the consumed
  range, matching pinned upstream even though it emits no cited block.
- Usage sums candidate and thought tokens, falls back to non-negative
  `total-prompt`, preserves cache-read tokens and reports one
  `server_tool_use.web_search_requests`. Nested and direct grounding shapes are
  accepted; the non-stream usage envelope remains nested like upstream.
- `antigravity_claude_response.rs` and its upstream test mirror move from
  scaffold to `partial`. Normal text/thinking/tool aggregate conversion and the
  streaming Params state machine remain open; `web_search.rs` still lacks the
  SSE event emission half.
- Seven new pinned-Go/Rust cases extend the shared corpus to 35, covering
  duplicate URLs, token fallback, direct grounding, overlaps, missing native
  gating, UTF-8 byte offsets and chunkless supports. Two direct response tests
  cover aggregate content/usage and dual-boundary gating.
- Full gates pass 298 no-default and 316 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. Mirror
  closure advances to 116/605; 489 production files and 399 upstream-test
  scaffolds remain open.

Forensic findings after worker 18k:

1. **Grounding metadata is insufficient authorization.** Only the conjunction
   of the original typed tool and the translated native tool proves that a
   Claude server-tool response may be synthesized.
2. **Result deduplication and citation addressing use different views.** Search
   result cards de-duplicate URLs, while supports continue to address the raw
   provider chunk array by index.
3. **Citation indices are byte offsets.** Treating them as Rust character
   indices breaks non-ASCII responses; byte slicing with lossy JSON-safe repair
   reproduces Go's boundary behavior.
4. **Overlaps are monotonic, not duplicated.** Each support begins no earlier
   than the previous emitted end, preventing repeated cited text.
5. **Invalid support references still consume text.** This surprising upstream
   rule can hide the affected segment, so it is pinned explicitly rather than
   silently “fixed” in the port.
6. **Tool ID generation is orchestration state.** The aggregate builder only
   needs a consistent ID shared by its first two blocks; uniqueness belongs at
   the request/response lifecycle boundary.

Strategy adaptation after worker 18k:

- Port the Web Search SSE block emitter and buffering state next, using this
  aggregate core as the single source for result/citation semantics. Gate
  server-tool ordering, 50-rune deltas, output-token timing and terminal-once
  behavior before integrating the wider normal Antigravity→Claude response
  state machine.

### Worker 18l — Web Search streaming response state

- The Web Search streaming branch now owns explicit per-request Rust state for
  first response, content index/type, pre-grounding text, finish/usage metadata,
  server-tool counts and final/terminal emission. No mutable translator global
  is introduced.
- The first provider chunk emits Claude `message_start` and reads CPA prompt
  usage, model and response ID. In native Web Search mode its CPA candidate
  count deliberately cannot appear as early output usage, matching upstream's
  response-timing contract.
- Visible text arriving before grounding is buffered; thought parts and
  function calls are excluded. On first grounding, the stream emits and closes
  `server_tool_use`, then `web_search_tool_result`, then cited/plain text blocks,
  so no text can race ahead of the server-tool evidence.
- Citation deltas precede their text deltas. Text is split in chunks of 50
  Unicode scalar values rather than bytes; aggregate citation ranges continue
  to use provider-defined UTF-8 byte offsets.
- A completed response without grounding flushes the buffered text as a normal
  Claude text block. Usage subtracts cached prompt tokens, includes thought
  tokens, applies the pinned non-negative total-token fallback and reports the
  cache and Web Search request count.
- CTOX hardens repeated `[DONE]` handling to emit `message_stop` at most once.
  The pinned Go code can repeat that terminal marker if called repeatedly; this
  is an intentional lifecycle adaptation covered by a direct Rust regression.
- `web_search.rs` moves from `partial` to `adapted_to_ctox`: all pinned helper
  semantics are active, while stream state/SSE framing live beside the wider
  response machine and capability discovery/tool-ID generation remain typed
  lifecycle inputs. The normal Antigravity→Claude response path is still open,
  so `antigravity_claude_response.rs` and its test mirror remain `partial`.
- Five new pinned-Go/Rust stream cases extend the shared corpus from 35 to 40:
  grounded ordering/usage, pre-grounding buffering, no-grounding fallback,
  Unicode rune chunking and cached/total-token fallback. Two direct stream tests
  cover content order, early usage suppression, terminal usage and terminal-once.
- Full gates pass 300 no-default and 318 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. The
  already-classified Web Search file improves from partial to adapted without
  inflating closure: 116/605 remain classified; 489 production files and 399
  upstream-test scaffolds remain open.

Forensic findings after worker 18l:

1. **Grounding is a stream commit boundary.** Text must remain buffered until
   native grounding either arrives or the provider finishes; emitting it early
   makes the required server-tool-first order unrecoverable.
2. **CPA usage and final usage have different clocks.** `message_start` may
   expose input tokens, but Web Search output tokens are only authoritative in
   the final usage metadata after search blocks have been emitted.
3. **The protocol uses two notions of position.** Citation spans are UTF-8 byte
   offsets, while SSE payload sizing is 50 Unicode runes. One indexing helper
   cannot safely implement both.
4. **Cached prompt accounting changes fallback arithmetic.** Candidate fallback
   subtracts the cache-adjusted prompt and thoughts; final output fallback uses
   total minus that same adjusted prompt, matching the two upstream stages.
5. **A Web-Search-only adapter must not absorb normal streaming.** When the
   dual request gate is false, the full Go converter continues through its
   ordinary Claude branch. Rust keeps that as the next independently testable
   response slice rather than mixing incomplete normal semantics into this API.
6. **Terminal-once is a safe lifecycle adaptation.** Repeated transport DONE
   markers must not duplicate a downstream terminal event even though the raw
   upstream helper does not guard that misuse.

Strategy adaptation after worker 18l:

- Preserve the proven Web Search core and move next to the normal
  Antigravity→Claude aggregate/stream state machine. Only after that response
  mirror is closed should runtime capability discovery and durable signature
  cache errors be wired together and the translator pair registered.

### Worker 18m — Normal Antigravity→Claude aggregate response

- The ordinary non-stream response branch now builds the complete Claude
  Messages aggregate outside Web Search: provider identity, ordered content,
  stop reason and optional usage/cache accounting.
- Adjacent visible parts are coalesced, while thinking/text/tool transitions
  flush at the same boundaries as pinned Go. Detached signatures become empty
  directional thinking carriers; signatures adjacent to thinking, visible text
  or functions retain their next/previous/standalone target semantics.
- Claude-target tool signatures stay on `tool_use`; Gemini-target signatures
  remain provider replay carriers. Cache mode prefixes Claude signatures with
  their model group, while cache-disabled Claude R-form values decode to the
  E-form expected by Anthropic clients.
- Tool names are restored through the request-wide collision map. Object args
  remain structured input; non-object args fail closed to `{}`. A newly ported
  `claude_tool_id.rs` implements protocol-safe fallback IDs and deterministic
  `cpa_gemini_` IDs bound to native call ID, name and canonical JSON args.
- A function call dominates the stop reason with `tool_use`; otherwise
  `MAX_TOKENS` maps to `max_tokens` and all other provider finishes to
  `end_turn`. Candidate plus thought usage falls back to non-negative
  total-minus-prompt, and a wholly absent usage object is omitted.
- Eight normal-response fixtures extend the shared pinned-Go/Rust corpus from
  40 to 48. They cover text aggregation, next/previous carriers, Gemini stable
  IDs, name restoration, Claude tool signatures, cache prefixes, token clamp
  and absent usage. Two direct aggregate tests and two adapted tool-ID tests add
  local provenance/order regressions.
- Full gates pass 304 no-default and 322 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. The tool
  ID production/test pair advances Mirror Closure to 117/605 and reduces the
  upstream-test scaffold backlog to 398; 488 production files remain open.

Forensic findings after worker 18m:

1. **A tool ID is replay provenance, not presentation.** Gemini IDs must bind
   the native call identity, name and canonical args; a sequential Claude ID is
   safe only when that provider provenance is unavailable.
2. **Signatures have provider-specific containers.** Claude keeps a function
   signature on `tool_use`; Gemini represents the same replay state through a
   directional thinking carrier unless it can attach to accumulated thinking.
3. **Flush boundaries carry meaning.** Coalescing across a carrier or function
   would change what a signature authorizes even if the resulting visible text
   were identical.
4. **Canonical JSON belongs only in the ID hash.** The emitted tool input
   remains the parsed object, but stable provenance must ignore object-key order
   so equivalent provider payloads receive the same ID.
5. **Usage absence differs from zero usage.** An existing empty metadata object
   keeps the zero-valued usage envelope; only a missing object removes it.
6. **Response formatting does not validate provider cryptography.** The
   formatter preserves opaque signatures; the separate replay validator decides
   whether a later request may trust them.

Strategy adaptation after worker 18m:

- Port the remaining ordinary SSE state machine next, reusing the now-proven
  aggregate carrier/tool-ID rules but testing transitions chunk by chunk. Once
  that closes `antigravity_claude_response.rs`, wire the typed model capability
  and durable cache error boundary before registry activation.

### Worker 18n — Normal Antigravity→Claude SSE state machine

- The unified streaming entry now selects Web Search only from the proven dual
  request gate and otherwise runs an explicit normal per-request state machine.
  Normal and Web Search states cannot accidentally share indices, buffers or
  terminal flags.
- `message_start` consumes CPA identity and early usage. Unlike Web Search, the
  ordinary path preserves CPA candidate tokens at start; final usage is still
  derived from cache-adjusted provider metadata.
- Text, thinking and function blocks transition incrementally across provider
  chunks. Thinking text accumulates for cache binding, signature deltas close
  replay authority at the correct block, and later signed thinking begins a new
  block rather than mutating an already signed one.
- Signature-only parts, signatures without a thought flag and visible text
  signatures use next/previous/standalone carriers according to observed
  semantic content. Function signatures stay on Claude tool blocks or become
  Gemini carriers exactly as in the aggregate path.
- Function args remain JSON text inside `input_json_delta`. Stable Gemini tool
  IDs reuse the native provenance hash; other streaming tools receive
  process-unique protocol-safe fallback IDs rather than per-response IDs that
  could collide across turns.
- Finish and usage may arrive in a chunk without content. The state closes the
  current block exactly once, lets tool use dominate stop reason, applies cached
  token arithmetic and emits an idempotent terminal sequence. DONE with no
  content synthesizes the required empty text block.
- Eight new stream fixtures extend the shared pinned-Go/Rust corpus from 48 to
  56, covering multi-chunk text, signed thinking, detached carriers, Gemini and
  Claude tools, consecutive tools, empty DONE and cached total-token fallback.
  Two direct stream regressions cover ordered deltas and terminal-once.
- `antigravity_claude_response.rs` moves from `partial` to
  `adapted_to_ctox`; lifecycle ID injection, state separation and terminal-once
  are explicit CTOX adaptations. Its 35-case upstream test mirror remains
  `partial` until every remaining adversarial carrier case is individually
  classified. Mirror Closure therefore stays honestly at 117/605 with 488
  production and 398 test scaffolds open.
- Full gates pass 306 no-default and 324 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18n:

1. **The first event is a protocol commit.** CPA identity and early usage must
   be fixed before later chunks, so account failover cannot remain legal after
   `message_start` reaches the client.
2. **Signed thinking is immutable stream state.** More thinking after a
   signature must open a new block; appending it to the signed block changes the
   signed payload without a matching signature.
3. **Finish and content are independent channels.** A finish-only chunk must
   close existing content and publish final usage without inventing a new text
   delta.
4. **Tool stop reason outranks provider finish.** Once any function call was
   emitted, Claude expects `tool_use` even when Gemini reports STOP or
   MAX_TOKENS later.
5. **Fallback IDs need cross-turn uniqueness.** Aggregate sequential IDs mirror
   pinned Go, but streaming IDs are lifecycle identifiers and must not restart
   at `tool_1` on every response.
6. **Web Search and normal CPA timing differ intentionally.** Early output
   tokens are valid for normal generation but suppressed for native search
   until grounded tool usage is known.

Strategy adaptation after worker 18n:

- The request and response format cores are now behaviorally complete. Next,
  close the 35-case response-test disposition and the `init.go` registry mirror,
  then wire runtime model capability discovery plus durable signature-cache
  errors. Registration must be the consequence of those typed boundaries, not
  a manual enablement flag.

### Worker 18o — Complete Claude response-test disposition

- All 35 upstream response-test families are now mapped to executable Rust
  evidence rather than inferred from the production converter. The adapted
  mirror records the disposition across Web Search, normal aggregate/stream,
  carrier and direct state/cache suites.
- Eight adversarial pinned-Go/Rust cases extend the shared corpus from 56 to 64:
  leading visible-text carriers, distinct thought/text signatures in both
  modes, consecutive detached carriers, trailing function carriers, leading
  carriers before thinking, signature-only parts without a thought flag and a
  thought-bound signed tool.
- A direct cache regression proves that multiple thinking chunks accumulate
  into one exact text key, that a later signature-only chunk binds and stores
  the signature, and that the test uses the same serialized cache guard as the
  rest of the suite.
- `antigravity_claude_response_test.rs` moves from `partial` to
  `adapted_to_ctox`. Because the file was already classified, this improves its
  evidence level without inflating closure: the upstream-test scaffold backlog
  stays at 398; production Mirror Closure remains 117/605 with 488 files open.
- Full gates pass 307 no-default and 325 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18o:

1. **Output parity cannot prove cache mutation.** Signature storage and exact
   accumulated text require a direct stateful assertion in addition to matching
   SSE bytes.
2. **Carrier bugs live in sequences.** Individual valid carriers are
   insufficient evidence; consecutive, leading, trailing and cross-kind
   placement must be tested as ordered histories.
3. **Adapted tests need an explicit disposition.** Condensing 35 Go tests into
   fewer Rust bodies is valid only when every upstream behavior family is named
   and backed by an executable corpus.
4. **Thought and visible signatures are independent.** Preserving one must not
   overwrite or retarget the other when both occur in the same response.

Strategy adaptation after worker 18o:

- Port `init.go` as explicit registry construction next. Keep capabilities
  disabled unless the typed Antigravity runtime can provide model Web Search
  discovery and durable signature-cache failures; then prove registration by
  executing request, aggregate, stream and token-count through the registry.

### Worker 18p — Explicit Claude ↔ Antigravity registry construction

- The mirrored Go package initializer is now an explicit Rust registration
  function. It activates the Claude→Antigravity request direction and the
  reverse stream, aggregate and token-count surfaces as one auditable pair.
- The default registration is deliberately capability fail-closed. It never
  guesses native Google Search support from a model string, so a typed Claude
  Web Search tool remains on the normal lossless tool path unless runtime
  discovery authorizes the exact selected model.
- A second constructor accepts an `Arc`-owned typed capability resolver. This
  gives the future dynamic Antigravity model registry a direct integration
  point without adding process-environment toggles, mutable translator globals
  or a provider/model routing shortcut.
- Registry streaming owns a per-request wrapper containing both the normal/Web
  Search converter state and one lifecycle-stable `srvtoolu_` ID. Aggregate
  conversion receives a fresh collision-resistant ID per call. Pure converter
  functions retain injected IDs so differential tests remain deterministic.
- Three direct registry tests prove all four capabilities, exact token-count
  shape, fail-closed default behavior and exact-model capability enablement.
  Full gates pass 310 no-default and 328 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.
- `init.rs` moves from `scaffold` to `adapted_to_ctox`, advancing Mirror Closure
  to 118/605. 487 production files and 398 upstream-test scaffolds remain open.

Forensic findings after worker 18p:

1. **Registration and capability discovery are different lifecycles.** A
   format pair can be safely active while a provider capability stays
   fail-closed; conflating them would require either a global registry or a
   model-name guess.
2. **Capability resolution belongs at request time.** Provider model catalogs
   can change while a process lives, so the registration captures a resolver,
   not a one-time boolean snapshot.
3. **Protocol IDs belong beside stream state.** The same Web Search tool-use ID
   must survive every chunk of one response but must not leak across requests.
4. **A synchronous byte transformer cannot report durable cache failure.** The
   current registry signature returns `Vec<u8>`, so durable signature-cache
   error propagation must land at a fallible runtime/executor boundary instead
   of being hidden in this initializer.

Strategy adaptation after worker 18p:

- Keep the now-complete format registry pair. Next port the dynamic model
  capability source far enough to resolve `supports_web_search` by exact
  Antigravity provider/model identity, then place the durable signature-cache
  adapter at the fallible executor boundary. Do not widen the infallible
  translator closure merely to simulate storage errors.

### Worker 18q — Antigravity model-capability catalog

- The provider discovery payload from `fetchAvailableModels` is now parsed by
  the mirrored Rust SDK file. Only `webSearchModelIds` affects the capability
  snapshot; the response's `models` object remains intentionally non-authoritative,
  matching upstream registration policy.
- IDs are trimmed, lowercased and deduplicated. Request-time reasoning suffixes
  such as `(high)` normalize to the exact base model for lookup without
  permitting prefixes, aliases or cross-provider fallback.
- Snapshot replacement intersects discovery hints with the authenticated
  runtime's already-known model IDs. A fetched-only model cannot create a
  route, while a static-only model remains available without an invented
  capability.
- Replacement is atomic under an explicit catalog lock. Malformed discovery
  clears stale capability state before returning a redacted typed error, so a
  previously authorized search route cannot survive failed revalidation.
- Three direct tests cover normalization/deduplication, known-model
  intersection, malformed-clear semantics and an end-to-end Claude registry
  request driven by the live catalog. Full gates pass 313 no-default and 331
  default unit tests plus 31 integrations in each matrix, both warning-denied
  Clippy matrices and formatting.
- `sdk/cliproxy/antigravity_models.rs` moves from `scaffold` to
  `adapted_to_ctox`, advancing Mirror Closure to 119/605. 486 production files
  and 398 upstream-test scaffolds remain open.

Forensic findings after worker 18q:

1. **Discovery is annotation, not model installation.** Accepting the remote
   `models` object would let a capability endpoint silently create executable
   routes that the trusted runtime catalog never exposed.
2. **A stale capability is authority.** Parse failure must not leave the last
   successful Web Search grant active indefinitely.
3. **Reasoning suffixes are not aliases.** Removing one terminal `(level)` is
   required by upstream routing; broader prefix/suffix guessing would cross the
   exact provider/model boundary.
4. **The catalog is provider-neutral storage only in shape.** Its values are
   Antigravity evidence and must be injected only into an Antigravity resolver,
   never reused as a general Gemini capability list.

Strategy adaptation after worker 18q:

- Add the bounded authenticated discovery transport next: POST `{}` to the
  configured account base URL or daily→production fallback, exact headers,
  timeout/status/body limits and no token-bearing diagnostics. Only after a
  successful response may it atomically replace this catalog.

### Worker 18r — Authenticated Antigravity capability discovery

- Capability discovery now uses the existing typed Antigravity request and
  transport boundary. A dedicated request constructor targets
  `/v1internal:fetchAvailableModels`, owns a zeroizing `{}` body and keeps its
  bearer token redacted in `Debug`.
- Target selection mirrors upstream exactly: one validated account override,
  otherwise daily Cloud Code followed by production. Credential-bearing URLs,
  query strings, fragments and non-HTTP schemes are rejected by the shared
  target type before dispatch.
- The async refresh applies the configured request timeout, accepts only 2xx
  responses below the one-MiB semantic limit, skips transport/status/parse/
  empty-hint failures and advances to the next target. Exhaustion clears stale
  capability authority and returns a redacted typed error.
- A default-feature TCP loopback proves the real native HTTP request is `POST
  {}` with exact path, JSON content type, Antigravity user agent and bearer
  authorization. No process proxy environment is consulted by the injected
  transport.
- Two transport-state tests cover fallback order, configured target behavior,
  fetched-only filtering and stale-state clearing. The relevant fourth family
  from `service_excluded_models_test.go` is now executable in its mirrored Rust
  test file; the other three Service/exclusion families keep that file honestly
  `partial`.
- Full gates pass 316 no-default and 335 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. Production
  Mirror Closure remains 119/605 because the touched production mirrors were
  already classified; the upstream-test scaffold backlog falls to 397.

Forensic findings after worker 18r:

1. **Fallback applies to discovery, not generation.** Daily→production retry is
   safe before capability publication; it must not silently reroute an already
   selected generation request.
2. **A successful HTTP status is not capability evidence.** Empty, malformed or
   oversized bodies cannot replace the catalog and must permit the next
   discovery target.
3. **The bearer token belongs to the transport request only.** Errors expose no
   URL body, token or upstream response payload.
4. **Post-read size rejection is not an allocation bound.** The shared native
   generate transport currently materializes the response before the SDK checks
   one MiB. A specialized incremental discovery-body reader is a later hardening
   task; current evidence claims semantic rejection, not allocation prevention.

Strategy adaptation after worker 18r:

- Bind discovery lifecycle to authenticated Antigravity account/runtime
  assembly only after selecting how multi-account capability disagreement is
  handled. Fail closed unless the eventual request can be routed to an account
  known to support the selected model. In parallel, begin the fallible durable
  signature-cache boundary; neither concern belongs inside the pure converter.

### Worker 18s — Explicit durable signature-cache boundary

- The complete upstream signature KV semantics now have an explicit Rust
  interface: get, set-with-TTL, exact delete and sliding expire. CTOX supplies a
  store object directly; no global Home client, environment switch or implicit
  process discovery is introduced.
- Request-time `get_cached_signature_required` treats an injected durable store
  as authoritative. Store miss returns the provider-specific miss value and
  store failure propagates a redacted typed error; neither path can consult a
  populated process-local cache.
- Completed response writes remain best-effort. A durable failure returns false
  without contaminating local state, while an explicit `None` store preserves
  the proven in-memory implementation for portable and test-only use.
- Durable hits refresh the three-hour TTL before returning. Invalid UTF-8 is
  rejected as an invalid store value rather than manufacturing a lossy replay
  signature. Exact deletes propagate failure; whole-cache clear intentionally
  stays local because upstream does not perform unsafe remote prefix deletion.
- `internal/home/kv_helpers.rs` ports the shared full 64-hex SHA-256 key-part
  primitive. Signature keys are `cpa:signature:<model-group>:<hash>` and never
  expose thinking text.
- Four durable-cache tests cover hashed keys, TTL refresh, miss/error isolation,
  best-effort write/delete, expire failure and Gemini empty-thinking sentinel;
  the hash helper has its own pinned behavior test. Full gates pass 321
  no-default and 340 default unit tests plus 31 integrations in each matrix,
  both warning-denied Clippy matrices and formatting.
- `kv_helpers.rs` moves from scaffold to ported and advances Mirror Closure to
  120/605. `signature_cache.rs` moves from partial to `adapted_to_ctox`; 485
  production files and 397 upstream-test scaffolds remain open.

Forensic findings after worker 18s:

1. **Durable mode must be authoritative.** Falling back to local state after a
   remote miss or outage can replay a signature that another process deleted or
   never observed.
2. **Read success includes TTL refresh.** Returning a value after failed expire
   would hide loss of the upstream sliding-expiration contract.
3. **Best-effort applies only after completion.** Response publication may
   continue when a cache write fails; request-time replay reads must fail the
   fallible request boundary.
4. **Prefix deletion is not a portable clear operation.** The safe exact-key
   API cannot emulate an unbounded remote scan, and upstream deliberately keeps
   `ClearSignatureCache` local in Home mode too.

Strategy adaptation after worker 18s:

- Thread the injected `SignatureKvStore` through a fallible Claude→Antigravity
  request converter next, so every thinking lookup uses the durable result
  directly rather than preflight/hydration. Then bind that converter and the
  capability catalog at the account-aware runtime boundary.

### Worker 18t — Fallible durable request-conversion boundary

- The Claude→Antigravity request transformer now exposes an explicit fallible
  runtime entry point that borrows an optional `SignatureKvStore`. Every
  thinking-signature lookup is performed at the point where the corresponding
  message part is converted; there is no cache preflight, hydration pass or
  process-local fallback in durable mode.
- Durable cache failures propagate as the redacted
  `AntigravityClaudeRequestTranslationError` before provider dispatch. The
  existing infallible converter remains the portable no-store facade used by
  registry-only and differential tests; it cannot accidentally claim durable
  runtime semantics.
- Invalid JSON still returns the original bytes unchanged. The native Web
  Search branch does not consult the signature store because it does not replay
  Claude thinking blocks.
- Two direct tests prove that a durable value is consumed without local-cache
  hydration and that an injected store failure crosses the conversion boundary
  before dispatch. Full gates pass 323 no-default and 342 default unit tests
  plus 31 integrations in each matrix, warning-denied Clippy and formatting.
- `antigravity_claude_request.rs` moves from `partial` to
  `adapted_to_ctox`. Because that production mirror was already classified,
  Mirror Closure remains 120/605; 485 production files and 397 upstream-test
  scaffolds remain open.

Forensic findings after worker 18t:

1. **Fallibility must travel with the converted data.** A separate preflight
   can race with deletion or expiry and cannot prove which signature was used
   to build the actual provider envelope.
2. **The store lifetime is request-scoped.** Borrowing the interface through
   synchronous conversion keeps ownership and account selection at the runtime
   boundary instead of introducing another global cache owner.
3. **Portable registration remains deliberately infallible.** The generic
   translator registry cannot represent provider-store failures today; only a
   runtime adapter may select the fallible entry point.
4. **Response publication is the matching half of the contract.** The durable
   store must next be bound to response-time signature publication and to the
   selected account/runtime, not merely made available as a global option.

Strategy adaptation after worker 18t:

- Add the durable response-publication variant before activating the runtime
  path, then bind capability discovery, request replay and response publication
  to the selected Antigravity account. Multi-account capability disagreement
  must fail closed or select an account known to support the exact model; a
  process-wide union is not sufficient routing evidence.

### Worker 18u — Durable streaming-response publication

- The normal Antigravity→Claude streaming converter now has an explicit runtime
  variant that borrows the same optional `SignatureKvStore` as request replay.
  The portable registry facade delegates with no store and retains the proven
  process-local behavior.
- When a terminal thinking signature arrives, the exact accumulated thinking
  text and provider signature are written directly through
  `cache_signature_best_effort`. An injected durable store remains
  authoritative: write failure does not hydrate or fall back to local state,
  while downstream signature emission continues as upstream requires.
- Web Search does not publish thinking signatures. Aggregate response
  conversion also remains store-free because the pinned Go implementation
  publishes this replay cache only from its streaming state machine; the port
  does not manufacture an additional aggregate-side lifecycle.
- Two direct tests prove the exact durable key/value/three-hour TTL and prove
  that failed durable publication still emits the signature delta without
  contaminating the local cache. Full gates pass 325 no-default and 344 default
  unit tests plus 31 integrations in each matrix, warning-denied Clippy and
  formatting.
- The response mirror was already `adapted_to_ctox`, so Mirror Closure remains
  120/605; 485 production files and 397 upstream-test scaffolds remain open.

Forensic findings after worker 18u:

1. **Best-effort means output survives, not that storage may change.** A failed
   durable write must not silently switch to process-local storage, because a
   later request on another process would observe different replay authority.
2. **Publication occurs only at the signature boundary.** Accumulated thinking
   without a provider signature is not valid replay evidence and is never
   persisted.
3. **Request and response use asymmetric error policy by design.** Required
   replay reads are fallible before dispatch; completed-response writes are
   best-effort after provider work has already succeeded.
4. **The registry still cannot own runtime resources.** Both runtime variants
   now exist, but account selection must provide their store and capability
   snapshot for the selected execution lane.

Strategy adaptation after worker 18u:

- Inspect the Antigravity account pool and introduce an account-scoped runtime
  binding that translates only after selection. Capability evidence, required
  replay reads, provider dispatch and best-effort response publication must use
  the same selected lane; do not activate a global union of account
  capabilities.

### Worker 18v — Account-scoped Claude non-stream runtime

- The Antigravity account pool now exposes a buffered Claude Messages execution
  path. It evaluates the original request for native Web Search eligibility,
  filters candidates by `(auth_id, exact_model)` capability evidence before
  routing, and translates only after a concrete account has been selected.
- Each failover iteration performs its own account-specific translation. A
  native Web Search request therefore cannot downgrade to an account whose
  discovery snapshot lacks the exact model, and an empty eligible set returns a
  typed capability-unavailable error without touching credentials or transport.
- Required durable signature reads use the injected `SignatureKvStore` inside
  that selected-lane translation. A store failure becomes a redacted pool
  translation error before authentication or provider I/O.
- Buffered provider execution was factored into one raw internal result holding
  the exact project-wrapped request and raw response. The established OpenAI
  Responses path and the new Claude aggregate path then apply their respective
  response converters before the account success is persisted; invalid format
  conversion still records no false success.
- New pool errors have redacted HTTP mappings. Three real account-pool tests
  prove exact-model selection of account B while A remains untouched, forbid
  failover from capable A to incapable B after a 503, and prove durable-read
  failure before either transport. Full gates pass 325 no-default and 347
  default unit tests plus 31 integrations in each matrix, both warning-denied
  Clippy matrices and formatting.
- The executor and handler mirrors remain honestly `partial`; this slice closes
  a vertical runtime behavior but no new production mirror file. Mirror Closure
  remains 120/605, with 485 production files and 397 upstream-test scaffolds
  open.

Forensic findings after worker 18v:

1. **Capability filtering belongs before router selection.** Selecting first
   and checking later perturbs cooldown/round-robin state with an account that
   could never execute the requested native operation.
2. **Failover must retranslate.** A provider body derived for account A's
   capability snapshot is not valid routing input for account B.
3. **Conversion precedes success persistence.** HTTP 200 alone is insufficient;
   the selected downstream format must validate before the account outcome is
   committed as successful.
4. **Streaming is not implied by buffered parity.** The current runtime stream
   owns an OpenAI-Responses state machine. Claude streaming needs a format-aware
   raw event stream carrying the same account binding and durable publication
   store; reusing the Responses stream would corrupt event semantics.

Strategy adaptation after worker 18v:

- Generalize the native Antigravity stream transport into a bounded raw-event
  lane whose downstream adapter is selected explicitly. Then add the Claude
  stream pool using the same pre-selection capability filter, per-account
  request translation, post-commit failure accounting and injected durable
  response publication. Do not duplicate transport/auth/replay loops per
  format.

### Worker 18w — Format-aware Antigravity stream lane

- The existing Antigravity runtime stream now owns an explicit downstream
  adapter enum instead of a hard-wired Responses state. Responses retains its
  established state machine; Claude owns its independent stream state, stable
  Web Search tool-use ID and optional `Arc<dyn SignatureKvStore>`.
- Raw SSE decoding, bounded upstream channel behavior, response observation for
  reasoning replay, EOF/DONE handling and replay commit remain one shared lane.
  Only complete provider event data crosses into the chosen downstream
  converter.
- Stream opening was factored from downstream bootstrap. Auth load, one 401
  refresh/replay, project/request wrapping, provider-status validation and
  reasoning-replay preparation are identical for Responses and Claude. The
  selected adapter must emit at least one valid event before pool success is
  persisted, preserving pre-commit failover semantics.
- The Claude stream pool uses the same pre-router `(auth_id, exact_model)` Web
  Search filter and per-selected-account request conversion as buffered Claude.
  Its durable store is owned by the surviving stream, so response-time
  signature publication cannot outlive or detach from the request runtime.
- Post-commit transport failure continues through the existing tracked-stream
  account outcome path. Two real stream tests prove account B is selected before
  bootstrap for an exact-model Web Search request and prove selected-lane
  thinking/signature events publish the exact value with a three-hour TTL to
  the durable store.
- Full gates pass 325 no-default and 349 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. The two
  touched production mirrors remain `partial`; Mirror Closure stays 120/605,
  with 485 production files and 397 upstream-test scaffolds open.

Forensic findings after worker 18w:

1. **Raw means decoded provider events, not arbitrary network chunks.** SSE
   fragmentation is normalized once before a format adapter sees payload data;
   otherwise adapter behavior would depend on TCP chunk boundaries.
2. **Bootstrap is the streaming commit gate.** Account success is recorded only
   after the chosen adapter emits a downstream event; failures before that point
   remain eligible for cross-account retry.
3. **The durable store must be stream-owned.** Borrowing a request-local store
   into an escaping stream would be invalid; `Arc` makes the lifecycle explicit
   without introducing a global owner.
4. **Format state is never shared.** Responses and Claude use the same transport
   lane but separate state machines, terminal rules and event envelopes.

Strategy adaptation after worker 18w:

- Add the Claude Messages HTTP route and typed runtime assembly around the two
  account-scoped pool methods. The host must provide per-account exact-model
  capability resolvers and one provider-scoped durable signature store; no
  subscription secret or browser Business OS data may enter the HTTP handler.

### Worker 18x — Claude Messages HTTP surface for Antigravity

- `sdk/api/handlers/claude/code_handlers.rs` enters the Rust module graph with
  a typed Antigravity handler for `/v1/messages`. It validates a non-empty model
  and boolean stream flag, while provider selection remains the independent
  `X-CTOX-Provider` input and accepts only the configured Antigravity lane.
- Buffered calls use the account-scoped Claude pool and return the native Claude
  message envelope. Streaming calls retain the selected tracked stream and
  forward Anthropic `message_*`/`content_block_*` SSE without passing through an
  OpenAI Responses wrapper.
- Handler construction requires explicit capability and optional durable-store
  objects. It contains no credentials and its Debug view reports only attachment
  state. Pool, transport, capability and store errors are normalized into a
  Claude `type:error` envelope with fixed redacted messages.
- A dedicated supervised-listener primitive now serves exactly
  `POST /v1/messages` with the same header/body limits and explicit provider
  header parsing as the Responses listener. It emits Content-Length for
  buffered JSON and commits SSE headers only for a stream already bootstrapped
  by the pool.
- Stream transport failures yield at most one redacted Claude `event: error`;
  the selected tracked stream records the account failure before raw transport
  details are discarded.
- Two handler unit tests cover request shape and redacted Claude errors. Two
  real TCP loopbacks prove buffered provider request/response translation and
  ordered Anthropic streaming with no Responses events or subscription secret.
  Full gates pass 329 no-default and 353 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.
- `code_handlers.rs` moves from scaffold to `partial`, advancing Mirror Closure
  to 121/605. Its wider upstream model-list/error/interceptor surface remains
  open; 484 production files and 397 upstream-test scaffolds remain.

Forensic findings after worker 18x:

1. **Claude needs its own HTTP response type.** Reusing the Responses route enum
   would couple terminal detection and error envelopes to `response.*` events
   and silently corrupt Anthropic streaming semantics.
2. **Provider is not inferred from model.** A Claude-looking model can execute
   on Antigravity only because the route handler itself is explicitly bound to
   that provider; the model string never selects credentials.
3. **The route is executable but not yet host-active.** The crate exposes and
   loopback-tests the listener primitive, while the CTOX host still supervises
   only its existing configured surfaces. Claiming production availability
   awaits typed host assembly, listener lifecycle and request-logging policy.
4. **Upstream handler closure remains partial.** Models endpoints, upstream
   error extraction, interceptor callbacks and the rest of the 503-line Go
   handler were not implied by this Antigravity Messages vertical slice.

Strategy adaptation after worker 18x:

- Wire a separately configured loopback Messages listener into the CTOX
  CLIProxyAPI host, using account-indexed capability catalogs and a
  provider-scoped durable signature-store adapter. Apply the existing
  retention-bounded request-logging policy before declaring the route ready;
  then return to the upstream handler's remaining model/error test families.

### Worker 18y — Typed CTOX host assembly for Claude Messages

- The supervised subscription gateway now assembles one typed provider-route
  bundle for Responses and Claude Messages. Both routes share the existing
  `127.0.0.1:12435` lifecycle and bounded HTTP parser, while dispatch, response
  envelopes and streaming state remain protocol-specific.
- `POST /v1/messages` is therefore host-active whenever an Antigravity account
  is configured. Its client base URL is `http://127.0.0.1:12435`; Anthropic
  clients append the native `/v1/messages` path.
- Responses and Messages share the same account-scoped Antigravity pool. The
  Claude handler receives account-indexed capability catalogs whose resolver
  requires an exact `(auth_id, model)` match; there is no global union and no
  model-name heuristic.
- The host starts an authenticated, bounded capability refresh with the route
  lifecycle and repeats it every ten minutes. Configured account models form
  the trusted intersection; an empty model list admits only the effective CTOX
  chat model. Malformed, rejected or unreachable discovery clears that
  account's authority and therefore fails Web Search closed.
- `CtoxSignatureKvStore` persists opaque provider replay signatures in the
  existing typed runtime SQLite database. Keys are bounded to 256 bytes,
  values to 1 MiB, writes use upsert, reads honor TTL, refresh is exact-key and
  deletes never broaden scope. No HTTP or Business OS data path exposes this
  table.
- The combined listener applies the existing retention-bounded request logging
  policy to both formats. Captured Messages responses retain their Claude
  envelope rather than being rewritten as Responses data.
- A real combined-listener test proves logged `/v1/messages` dispatch; the root
  store test proves persistence, refresh, expiry and exact deletion. Full
  portable gates pass 330 no-default and 354 default unit tests plus 31
  integrations in each matrix, both warning-denied Clippy matrices and
  formatting. Root `cargo check` and the targeted store test also pass.
- Mirror Closure remains 121/605: the host assembly is CTOX-origin integration
  and the portable server mirror was already partial. 484 production files and
  397 upstream-test scaffolds remain open.

Forensic findings after worker 18y:

1. **One listener does not mean one protocol state machine.** Port reuse removes
   another lifecycle and configuration surface; it does not permit Claude SSE,
   terminality or errors to pass through a Responses wrapper.
2. **Capability authority is account-local and revocable.** A successful model
   discovery for account A can never authorize account B, and any failed
   refresh removes stale authority instead of silently extending it.
3. **Discovery freshness follows the route lifecycle.** The task is aborted on
   configuration rebuild. Before its initial network result, Web Search is
   deliberately unavailable rather than guessed from the model name.
4. **Durability has explicit storage limits.** The provider signature is opaque
   runtime state, not a credential or browser record; bounded keys, values and
   TTL cleanup prevent the adapter from becoming an unbounded generic KV API.
5. **Host activation depends on configured provider accounts.** The public
   Messages base URL describes the supervised surface, but the route is only
   installed when a typed Antigravity pool can be assembled.

Strategy adaptation after worker 18y:

- Return to the mirrored upstream Claude handler in two evidence slices: port
  the error-extraction and pending-stream test family first, then evaluate the
  model-list/rewrite family against CTOX's provider-independent model catalog.
  Keep host lifecycle changes separate from handler semantic closure so test
  disposition cannot overstate the remaining 503-line Go surface.

### Worker 18z — Claude handler error and pre-commit stream semantics

- The upstream Claude error classifier is now a format-local Rust primitive.
  It maps HTTP status to Anthropic error types, recognizes nested OpenAI- and
  Claude-style JSON, prefers a nested message and falls back to a nested code
  without string-splicing untrusted JSON into an envelope.
- `ClaudeMessagesHttpResponse::error` uses that common classifier. In
  particular, a redacted local HTTP 429 now correctly carries
  `rate_limit_error` instead of the previous generic `api_error` label.
- Three pinned upstream tests cover OpenAI-style extraction, Claude-style
  extraction and the native Claude HTTP envelope. An additional adversarial
  test covers status fallback and the nested-code branch.
- The Go pending-error channel race is adapted at the actual Rust ownership
  boundary: the Antigravity tracked stream must bootstrap before the handler
  returns `Stream`. A rejected streaming upstream therefore returns buffered
  Claude JSON and never commits SSE headers. The test uses the real account
  pool and confirms that the rejected provider body cannot leak a credential.
- `code_handlers_error_test.rs` moves from scaffold to `adapted_to_ctox`,
  reducing the upstream-test backlog to 396. Mirror Closure remains 121/605
  with 484 production files open.
- Full gates pass 335 no-default and 359 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18z:

1. **Error format and error disclosure are separate decisions.** The Claude
   envelope and status-derived type can match upstream while the subscription
   transport still discards raw provider error text at the credential boundary.
2. **Commit state replaces the Go channel peek.** Rust's tracked stream owns
   bootstrap ordering, so introducing a second non-blocking error receiver only
   to mimic `select default` would recreate a race that the ownership model has
   already removed.
3. **Status classification must be shared by local and upstream errors.** A 429
   with a redacted local message is still a rate-limit error; message redaction
   is not a reason to weaken the machine-readable type.
4. **JSON extraction stays structural.** Invalid JSON remains trimmed text and
   valid JSON can only replace type/message through explicit string fields;
   arbitrary objects are never serialized back as trusted error details.

Strategy adaptation after worker 18z:

- Evaluate the model-list/rewrite family as a policy slice, not a mechanical
  helper port. DD model IDs and list cloaking affect the provider-independent
  CTOX catalog boundary: first locate the pinned encoder/decoder and model
  response rules, then either port them with round-trip fixtures or mark them
  replaced by an explicit typed CTOX model-alias policy.

### Worker 18aa — Claude model catalog and reversible DD aliases

- `internal/client/claude/models/models.rs` is fully ported and active. It
  clones the injected catalog, optionally aliases non-Claude IDs, stable-sorts
  by display name then ID and emits Anthropic `data`, `has_more`, `first_id`
  and `last_id` fields without mutating or reordering the caller's input.
- The pinned `claude-fable-5-dd-` compatibility alias is reversible and
  Unicode-safe: Rust reverses characters rather than UTF-8 bytes. Prefix
  matching stays case-sensitive and an optional final `(thinking)` suffix is
  decoded only after isolating the aliased base ID.
- The Claude handler exposes a native JSON model-response primitive with an
  explicitly injected typed catalog and `disable_cloaking` policy. This replaces
  the Go global registry dependency at the handler boundary; it does not infer
  providers or credentials from any model ID.
- `/v1/messages` now resolves a DD alias before model parsing, exact capability
  lookup and account selection. Invalid JSON, missing models and already-native
  IDs retain byte identity; only a recognized alias causes JSON rewriting.
- Four pinned model-module families plus a Unicode adversarial case cover
  catalog ordering, cloning, empty responses, enabled/disabled aliasing and
  round trips. Four handler families cover configured display names, disabled
  cloaking, request rewrites and byte-identical no-op/error paths.
- `models.rs` moves scaffold→ported; `models_test.rs` moves scaffold→ported and
  `code_handlers_model_test.rs` moves scaffold→adapted_to_ctox. Mirror Closure
  reaches 122/605, with 483 production and 394 upstream-test scaffolds open.
- Full gates pass 343 no-default and 367 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18aa:

1. **DD aliases are compatibility, not security.** Character reversal is
   intentionally reversible and must never be described as protecting a model,
   provider or credential.
2. **Alias resolution precedes routing but does not perform routing.** The
   decoded model is still evaluated against the independently selected provider
   and account-local exact capability catalog.
3. **No-op byte identity is observable.** Re-serializing every request would
   perturb whitespace and key order for no semantic reason; the port returns the
   original bytes unless a recognized alias actually changes the model.
4. **Catalog ownership stays outside the portable handler.** The Go singleton is
   replaced by typed input so CTOX can assemble a provider-independent view
   without creating hidden global mutation in the Rust crate.
5. **The builder is active before the GET route.** Native model-response
   semantics exist and are tested, but the combined listener still exposes only
   the two POST generation routes. HTTP model-list activation remains separate
   host work.

Strategy adaptation after worker 18aa:

- Add a bounded `GET /v1/models` route to the combined listener only after the
  CTOX host can supply a typed, secret-free catalog snapshot and explicit alias
  policy. Reuse the model response primitive; do not introduce a portable
  global registry. Then resume API-control-plane mirror closure around the next
  smallest upstream handler family.

### Worker 18ab — Host-active provider-independent model listing

- The combined supervised listener now serves bounded `GET /v1/models` beside
  `POST /v1/responses` and `POST /v1/messages`. Non-GET model requests receive
  a native Claude 405 envelope; the existing 32-KiB header and 2-MiB body
  limits, response capture and retention-bounded logging remain shared.
- Model listing no longer depends on whether an Antigravity Messages handler is
  installed. The host always runs the combined dispatcher: Responses remains
  active for configured providers, Messages returns Claude 404 when absent and
  the secret-free model snapshot remains available.
- The host derives that snapshot from the same validated runtime configuration
  used to assemble provider pools. Disabled accounts are excluded, empty
  account model lists use the effective CTOX chat model, and duplicate raw
  model IDs are grouped into a sorted `providers` array with a `default` flag.
- Each snapshot record contains only model ID, display name, object kind,
  `owned_by: ctox`, provider names and the default marker. Secret handles,
  scopes and values are neither read into nor serialized by the builder.
- Snapshot construction and JSON serialization occur once per supervised
  configuration lifecycle. Individual GET requests only clone the bounded
  response, so the route cannot turn into a per-request secret/config query.
- A real TCP test proves the Claude model envelope, DD alias and multi-provider
  metadata. A root test proves validated-config derivation and absence of both
  credential values and handle names. Full portable gates pass 344 no-default
  and 368 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and formatting; root `cargo check` and the
  targeted snapshot test pass.
- Mirror Closure remains 122/605 with 483 production and 394 upstream-test
  scaffolds open because this slice changes the already-partial portable server
  and CTOX-origin host assembly rather than classifying another Go file.

Forensic findings after worker 18ab:

1. **Catalog consistency is lifecycle consistency.** The GET response and
   provider pools originate from one effective configuration snapshot and are
   rebuilt together; an independently refreshed global registry would permit a
   model to be listed without an executable route.
2. **Provider metadata does not select a provider.** `providers` explains where
   a raw model is configured, while execution still requires the independent
   provider header/default-provider policy.
3. **No per-request store access is safer and cheaper.** The response is an
   immutable serialized snapshot, so model listing cannot accidentally acquire
   credential-store authority or observe half-applied configuration.
4. **The DD prefix has a reserved-name collision.** Upstream decoding treats any
   `claude-fable-5-dd-…` ID as an alias; a genuinely native model using that
   prefix would be ambiguous. The port preserves compatibility and records the
   prefix as reserved rather than inventing a divergent escape syntax.
5. **Loopback exposure still needs bounded disclosure.** The list is not secret,
   but only operational model/provider names are emitted; account IDs, upstream
   addresses, proxy settings and credential handles are absent.

Strategy adaptation after worker 18ab:

- Resume mirror closure in the API-control-plane wave by inventorying the next
  smallest handler/test cluster whose route semantics can reuse the combined
  listener. Prefer a complete vertical family over adding isolated helpers to a
  large scaffold, and update this artifact after that worker.

### Worker 18ac — Upstream response header safety filter

- `sdk/api/handlers/header_filter.rs` is fully ported and active. A typed
  multi-value header map removes RFC hop-by-hop fields, `Set-Cookie`, CPA-owned
  framing/CORS/trace fields and every header dynamically named by any
  comma-separated `Connection` value.
- Matching is ASCII-case-insensitive while surviving headers retain their
  original spelling and complete value vectors. Multiple `Connection` lines
  are unioned before filtering, so map iteration order cannot re-admit a scoped
  header.
- The pinned third-party AI-gateway metadata prefixes are preserved as a
  compatibility/privacy rule. They affect response metadata only and have no
  role in provider selection, authentication or request transformation.
- The write helper copies complete upstream value lists but never overwrites a
  handler-owned header, including when the two maps use different casing.
- Two pinned test families prove dynamic connection scoping and the all-blocked
  `None` result. Two adversarial tests cover gateway/CPA case folding and
  non-overwrite behavior.
- `header_filter.rs` and `header_filter_test.rs` both move scaffold→ported.
  Mirror Closure reaches 123/605, with 482 production and 393 upstream-test
  scaffolds open.
- Full gates pass 348 no-default and 372 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ac:

1. **`Connection` is a dynamic denylist.** Removing only the standard
   hop-by-hop names is insufficient; every token named by the upstream
   `Connection` header becomes hop-local for that response.
2. **Header names are case-insensitive, maps are not.** The port compares names
   without case while preserving original keys, avoiding both bypasses and
   gratuitous output normalization.
3. **Framing remains listener-owned.** Upstream `Content-Length`,
   `Content-Encoding` and `Transfer-Encoding` cannot survive because the Rust
   listener computes its own buffered/SSE framing.
4. **Cookies never cross the subscription boundary.** Provider session cookies
   are security-sensitive runtime state and cannot become client response data.
5. **Gateway-prefix maintenance is an upstream-drift hotspot.** New metadata
   families can be added upstream independently of RFC rules, so sync review
   must classify changes to this explicit list rather than treating them as
   cosmetic comments.

Strategy adaptation after worker 18ac:

- Take the next complete small transport cluster: inspect `buffered_conn.go`,
  `mux_listener.go` and `protocol_multiplexer.go` together with their test and
  determine whether CTOX's single loopback listener replaces or still needs
  their protocol-prefix replay semantics. Port only the minimal coherent set;
  record a replacement when the upstream multi-protocol ownership model does
  not apply.

### Worker 18ad — CTOX replacement for the HTTP/Redis protocol mux

- `buffered_conn.rs`, `mux_listener.rs` and `protocol_multiplexer.rs` move from
  scaffold to `replaced_by_ctox`. Upstream peeks a shared TLS/TCP socket and
  dispatches HTTP or Redis RESP through synthetic listeners; CTOX deliberately
  owns a loopback HTTP-only subscription gateway and keeps durable queue/state
  inside the daemon.
- Because no component consumes a protocol prefix before the HTTP parser, Rust
  needs neither a replaying buffered connection nor a channel-backed listener.
  Introducing either would add ownership and backpressure paths without an
  executable CTOX consumer.
- The upstream liveness invariant is preserved at the host boundary: the
  supervisor accepts first and immediately spawns a task before any header read,
  so an idle peer cannot block later accepts.
- The combined connection primitive now also applies the upstream ten-second
  initial-header deadline. Timeout closes the connection quietly, bounding the
  task/socket lifetime rather than merely moving an idle leak into a spawned
  task. Tests inject a shorter deadline without changing production policy.
- `protocol_multiplexer_test.rs` moves scaffold→adapted_to_ctox. Its real TCP
  replacement test establishes an idle first connection, proves a subsequent
  bounded `GET /v1/models` completes, and proves the idle task terminates on its
  header deadline.
- Mirror Closure reaches 126/605 with 479 production and 392 upstream-test
  scaffolds open. Full gates pass 349 no-default and 373 default unit tests plus
  31 integrations in each matrix, both warning-denied Clippy matrices and
  formatting.

Forensic findings after worker 18ad:

1. **Replacement must preserve the reason, not the mechanism.** The load-bearing
   upstream behavior is accept-loop liveness and bounded idle resources, not
   `net.Listener` emulation or byte replay by itself.
2. **Spawn without deadline is incomplete.** It removes head-of-line blocking
   but permits unbounded tasks and file descriptors; the sniff/header deadline
   is part of the same fix.
3. **Redis-on-the-provider-port violates CTOX ownership.** Queueing and durable
   state belong to the daemon's typed stores and must not be reintroduced as an
   unaffiliated wire protocol beside subscription HTTP.
4. **TLS ALPN routing is intentionally absent.** The managed endpoint is local
   loopback HTTP; external TLS termination or remote exposure would require a
   separate architecture decision rather than silently reviving this mux.
5. **Silent timeout is deliberate.** A peer that has not supplied an HTTP head
   receives no protocol-specific error envelope; the connection simply closes,
   matching the upstream sniff-timeout boundary and avoiding noisy logs.

Strategy adaptation after worker 18ad:

- Continue with a portable semantic pair rather than another host replacement:
  compare `request_body.go` and the two Responses stream-error helpers/tests
  against existing Rust parser and terminal-error behavior. Select whichever
  can close its production and test files without pulling in the large generic
  handler base.

### Worker 18ae — OpenAI Responses top-level stream error chunks

- `sdk/api/handlers/openai_responses_stream_error.rs` is fully ported and
  active. It emits the streaming union member with top-level `type: error`,
  `code`, `message` and non-negative `sequence_number`, never the buffered HTTP
  shape with a nested `error` object.
- HTTP statuses map to the pinned Responses codes for authentication, quota,
  rate limit, model lookup, request timeout, invalid request and internal
  failures. Missing status defaults to 500; missing text uses a bounded status
  phrase and negative sequence values clamp to zero.
- Structurally valid top-level stream-error JSON can supply message, scalar code
  and sequence; nested HTTP error JSON can supply message and code. Nested
  values take the same final precedence as upstream. Invalid JSON remains the
  trimmed message instead of being interpolated into another JSON document.
- A format-local helper frames the chunk as one `data:` SSE event. The generic
  handler test is adapted to this primitive and proves the payload has no nested
  HTTP error object after stream commit.
- Existing provider-native `response.failed` terminal events remain valid and
  retain their tested redaction/tracking semantics. This worker does not replace
  them merely to force one visual event form; it closes the generic forwarding
  error contract that upstream tests separately.
- `openai_responses_stream_error.rs` and its direct test move scaffold→ported;
  `openai/openai_responses_handlers_stream_error_test.rs` moves
  scaffold→adapted_to_ctox. Mirror Closure reaches 127/605 with 478 production
  and 390 upstream-test scaffolds open.
- Full gates pass 353 no-default and 377 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ae:

1. **Streaming and buffered errors are different schemas.** A JSON object valid
   for an HTTP 4xx/5xx body can still be rejected by a Responses SSE client if
   `type` is not top-level.
2. **Embedded sequence zero is ambiguous by design.** Upstream accepts an
   embedded sequence only when the caller supplies zero; the port preserves
   this contract rather than inventing an optional parameter.
3. **Code values are scalar-normalized.** String, numeric and boolean upstream
   codes become strings; absent/null codes fall back to the status-derived
   machine code.
4. **Parsing does not imply disclosure.** Runtime subscription adapters should
   call the builder with redacted text. The pure compatibility helper can parse
   supplied JSON, but it has no access to credentials or transport internals.
5. **Terminal form is provider-path-specific.** Generic forwarding needs the
   top-level error chunk; an already valid and redacted `response.failed` event
   need not be rewritten and may carry richer response lifecycle state.

Strategy adaptation after worker 18ae:

- Port `request_body.go` next as an independent bounded decoding primitive,
  but add a decompressed-size limit absent from the pinned Go helper before
  wiring it into the HTTP parser. Preserve upstream's valid-raw-JSON fallback
  for incorrectly labeled content encoding and test reverse-order stacked
  encodings plus decompression-bomb rejection.

### Worker 18af — Bounded Zstd request-body decoding

- `sdk/api/handlers/request_body.rs` is fully ported and active. Absent,
  whitespace-only and case-insensitive `identity` encodings preserve the exact
  input bytes; `zstd` is decoded and comma-stacked encodings are applied in
  reverse order as required by HTTP representation layering.
- The pinned compatibility fallback is preserved: if decoding fails but the raw
  body is already valid JSON, the original bytes are returned. Incorrectly
  labeled clients therefore remain compatible without accepting arbitrary
  undecodable binary data.
- Rust adds a deliberate safety bound absent from the Go helper. Each decoded
  Zstd layer reads at most 2 MiB plus one sentinel byte; high-ratio payloads fail
  with a typed `DecodedBodyTooLarge` error before allocating an unbounded body.
- The common HTTP parser now decodes before route/model inspection and maps
  invalid encoding to a fixed 400 message. After successful decode or valid-JSON
  fallback it removes `Content-Encoding`, preventing request logging or later
  handlers from trying to decode the normalized body again.
- Five direct tests cover identity byte preservation, single/stacked Zstd,
  mislabeled valid JSON, unsupported/corrupt data and the decompression limit.
  A real TCP Messages request proves compressed bytes become the expected
  Antigravity request content before provider dispatch.
- `request_body.rs` moves scaffold→ported. Mirror Closure reaches 128/605 with
  477 production and 390 upstream-test scaffolds open.
- Full gates pass 359 no-default and 383 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18af:

1. **Encoding order is reversed on decode.** A body encoded A then B must be
   decoded B then A; left-to-right iteration only appears correct for a single
   layer or two identical codecs.
2. **Compatibility fallback is JSON-gated.** A false encoding label may be
   ignored only when the bytes already form valid JSON; arbitrary binary or
   malformed text remains a request error.
3. **Compressed-size limits do not stop decompression bombs.** The listener's
   2-MiB wire-body limit is insufficient without an independent decoded limit.
4. **Normalization consumes the encoding header.** Retaining `Content-Encoding:
   zstd` beside decoded bytes would create double-decoding bugs in logging,
   middleware or a later transport adapter.
5. **Errors are redacted at the parser boundary.** Decoder internals and codec
   library messages do not enter the HTTP response; clients receive one fixed
   invalid-encoding message.

Strategy adaptation after worker 18af:

- Inventory the remaining sub-5-KiB API files with their dependency/test pairs.
  Prefer `server_keepalive.go` only if its lifecycle can be expressed without a
  detached ambient task; otherwise take a self-contained Management read model
  such as usage/quota and keep mutation/auth authority in typed CTOX stores.

### Worker 18ag — Owned portable keep-alive watchdog

- `internal/api/server_keepalive.rs` is fully ported and active as an owned
  Tokio primitive. `KeepAliveWatchdog::spawn` rejects a zero timeout, uses a
  capacity-one heartbeat channel, resets a pinned timer and invokes the timeout
  callback exactly once.
- `signal` is non-blocking and coalesces redundant heartbeats just like the
  upstream buffered channel. `stop` awaits task termination; `Drop` requests
  shutdown so the portable type cannot intentionally leave an unowned ambient
  watchdog behind.
- Authorization preserves Bearer parsing, case-insensitive scheme matching,
  `X-Local-Password` fallback and constant-time byte comparison. Empty expected
  password means no authorization gate; supplied passwords never enter Debug,
  Display or error values.
- Tests use Tokio's paused clock rather than wall-clock sleeps. They prove
  Bearer/local-password acceptance and rejection, timeout callback, heartbeat
  reset, stop suppression and invalid-timeout rejection without flaky timing.
- The primitive is not yet exposed as `/keep-alive` by the CTOX host. Route
  activation requires typed runtime timeout and a secret-store handle; adding
  an ambient env password or an unconfigured shutdown callback is prohibited.
- `server_keepalive.rs` moves scaffold→ported. Mirror Closure reaches 129/605
  with 476 production and 390 upstream-test scaffolds open.
- Full gates pass 363 no-default and 387 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ag:

1. **Task ownership is part of the port.** A direct `tokio::spawn` translation
   without a handle would recreate an ambient goroutine and make shutdown,
   tests and host rebuilds nondeterministic.
2. **Heartbeat backpressure should collapse.** Only freshness matters; queuing
   every heartbeat would grow work without changing the deadline state.
3. **Stop and timeout are mutually terminal.** The callback is consumed once,
   while explicit stop/drop suppresses it and terminates the task.
4. **Constant-time comparison does not authorize a route by itself.** Host
   activation still needs typed secret resolution and policy; the primitive
   accepts values but never decides where they originate.
5. **Paused time is stronger test evidence.** Deterministic advancement proves
   reset ordering without depending on scheduler latency or generous sleeps.

Strategy adaptation after worker 18ag:

- Do not activate keep-alive until its configuration schema is ported. Continue
  with Management `usage.go`: separate its pure count/record JSON semantics from
  the upstream global Redis queue, then bind any destructive pop operation only
  to a typed durable CTOX queue adapter with explicit authority.

### Worker 18ah — Typed Management usage-queue boundary

- `internal/api/handlers/management/usage.rs` is fully ported and active in the
  Rust module graph. The exact `count` contract is preserved: absent or blank
  means one, while zero, negative, malformed and overflowing values fail with
  the pinned `count must be a positive integer` message.
- The upstream process-global Redis queue is replaced by
  `ManagementUsageQueue`, an injected destructive oldest-first interface that
  returns owned record bytes and a typed store failure. This keeps queue
  authority visible at host assembly and avoids ambient mutable state.
- `/v0/management/usage-queue` is authenticated before count parsing or store
  access, permits only GET, and exists only when a queue adapter is explicitly
  attached. CTOX currently has no authorized durable usage-record store, so the
  root host deliberately leaves the route inactive instead of installing an
  in-memory or Redis compatibility singleton.
- JSON record semantics remain byte-oriented. Valid JSON records are embedded
  exactly as queued, including surrounding whitespace; invalid records become
  JSON strings with lossy invalid-UTF-8 replacement matching Go string/JSON
  behavior.
- The two pinned upstream cases are executable HTTP-handler tests: requested
  records pop in FIFO order, and an invalid count never mutates the queue. A
  third adversarial test covers valid scalar JSON and invalid-record quoting.
- `usage.rs` and `usage_test.rs` move scaffold→ported. Mirror Closure reaches
  130/605 with 475 production and 389 upstream-test scaffolds open.
- Full gates pass 366 no-default and 390 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ah:

1. **A destructive GET is still a command boundary.** Authentication must run
   before count parsing, and count validation must complete before `pop_oldest`;
   otherwise unauthorized or malformed reads can consume durable evidence.
2. **Raw JSON records are not ordinary DTOs.** Parsing and reserializing valid
   queue entries would change byte representation and can lose upstream audit
   fidelity; validation is used only to choose raw embedding versus quoting.
3. **Injection is necessary but not sufficient activation.** A trait removes
   the global singleton, but the root must still select a durable store and
   explicit retention/consumer policy before exposing the route.
4. **Owned bytes make the pop atomicity visible.** The store transfers records
   out of the queue in one call; the HTTP layer cannot borrow mutable queue
   storage across serialization or network writing.
5. **Compatibility does not justify Redis revival.** CTOX already owns durable
   daemon state; opening a second Redis-like ownership plane solely for this
   endpoint would contradict the protocol-mux replacement from worker 18ad.

Strategy adaptation after worker 18ah:

- Keep unauthorised persistence surfaces inactive even when their pure handler
  is complete. Inventory the next small Management/API pair for read-only or
  injected semantics that can close without inventing a global store; prefer a
  complete production+test cluster over expanding `server_management.rs`
  horizontally with unbacked routes.

### Worker 18ai — Secret-free API-key usage projection

- `internal/api/handlers/management/api_key_usage.rs` is
  `adapted_to_ctox` and active in the Management module graph. It keeps the
  upstream provider grouping, case/whitespace normalization, `compat_name`
  override, success/failure totals and positional recent-request bucket merge.
- The upstream response key `base_url|api_key` is intentionally rejected: it
  would serialize live credentials into an HTTP response and persisted browser
  traces. CTOX groups by a host-supplied public account ID instead; the input
  record has no API-key or base-URL field.
- `ManagementApiKeyUsageSource` is an injected read-only snapshot boundary.
  `/v0/management/api-key-usage` remains auth-first, GET-only and 404 unless a
  source is explicitly attached. The root host has no suitable per-account
  secret-free telemetry projection yet, so it remains inactive.
- Output uses deterministic `BTreeMap` ordering and saturating counter merges.
  Empty public account IDs are excluded and blank providers become `unknown`.
- The two upstream test intents are adapted to public account IDs: separate
  provider grouping and OpenAI-compatible `compat_name` grouping. A third test
  proves duplicate account records merge totals/buckets. Serialized evidence
  asserts absence of `api_key`, `base_url` and URLs.
- `api_key_usage.rs` and `api_key_usage_test.rs` move
  scaffold→adapted_to_ctox. Mirror Closure reaches 131/605 with 474 production
  and 388 upstream-test scaffolds open.
- Full gates pass 369 no-default and 393 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ai:

1. **Management authentication does not make secrets response-safe.** A caller
   authorized to inspect usage still should not receive reusable provider
   credentials, especially as JSON object keys that logs and browser tools tend
   to retain.
2. **Grouping identity must be a separate type of datum.** Provider, public
   account identity and credential are different dimensions; concatenating
   origin and key conflates all three and makes later redaction unreliable.
3. **A source DTO can enforce absence better than output scrubbing.** Because
   the injected record has no credential or URL field, serialization cannot
   accidentally expose them through a missed redaction branch.
4. **Compatibility names are presentation/routing metadata, not secrets.** The
   port preserves their normalized grouping precedence without using them to
   resolve accounts or credentials.
5. **Stable ordering improves evidence reproducibility.** Go map iteration is
   intentionally unordered; deterministic Rust maps make snapshots and tests
   comparable without changing the JSON object contract.

Strategy adaptation after worker 18ai:

- Do not port `quota.go` in isolation: its reset command requires an opaque
  auth-index resolver and a transition-locked durable cooldown mutation. First
  audit `config_auth_index.go` plus the existing `CooldownConductor`; take that
  combined cluster only if index generation can remain secret-free and reset
  can reuse the same writer lock as request outcomes.

### Worker 18aj — Opaque auth-index and atomic quota control

- `internal/api/handlers/management/quota.rs` is `adapted_to_ctox` and active.
  Both quota-exceeded fallback toggles retain their GET and PUT/PATCH JSON
  contracts through `ManagementQuotaSwitchSource`; no process environment or
  untyped config mutation is introduced.
- `POST /v0/management/reset-quota` preserves the pinned body/status contract,
  accepts only `auth_index`, rejects raw account IDs and file names, caps the
  body at 16 KiB and authenticates before parsing or mutation.
- `config_auth_index.rs` moves scaffold→partial. Its accepted primitive hashes
  the upstream `id:<public-account-id>` fallback with SHA-256 and exposes the
  first eight bytes as 16 lowercase hex characters. Upstream path- and
  API-key-derived seeds are intentionally not ported; the remaining config DTO
  enrichment stays open; the partial classification receives Mirror-Closure
  credit but does not claim a complete semantic port.
- `CooldownManagementQuotaReset` resolves indexes to public account IDs and
  delegates to `CooldownConductor::reset_account`. Reset and ordinary request
  outcome transitions therefore share the exact same mutex-protected
  load/modify/save sequence; account- and model-scoped records are removed in
  one persisted snapshot and model names are returned sorted/deduplicated.
- The two upstream reset tests are adapted: a valid opaque index clears only
  the target account and reports affected models; `auth_id`, `id`, file name and
  raw account ID never resolve. A third route test covers both fallback switch
  families, and a direct pinned-hash test covers public-ID index generation.
- `quota.rs` and `quota_test.rs` move scaffold→adapted_to_ctox. Together with
  the newly classified partial auth-index file, Mirror Closure reaches 133/605
  with 472 production and 387 upstream-test scaffolds open.
- Full gates pass 373 no-default and 397 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18aj:

1. **An opaque identifier must not be credential-derived by necessity.** The
   same upstream wire shape is available from a public stable account ID, so
   hashing API keys or local paths is avoidable and would broaden secret input.
2. **Reset must enter the normal writer serialization lane.** A separate
   load/filter/save helper could race a request result and resurrect or discard
   cooldown state; using the conductor lock makes reset a real state transition.
3. **Account existence and cooldown existence are distinct.** A known account
   can validly reset with no current records; the index catalog supplies
   existence while the conductor supplies affected model state.
4. **Raw IDs are not aliases for management indexes.** Accepting both would
   defeat the purpose of a stable public control-plane handle and could expose
   internal routing identity through probing differences.
5. **Partial classification remains necessary.** The accepted hash primitive
   does not close upstream's large config-enrichment file; claiming it fully
   ported would hide all provider-specific DTO work still absent.

Strategy adaptation after worker 18aj:

- Keep the quota routes host-inactive until the supervisor can pass the exact
  conductor instances used by live provider pools and a revisioned switch
  store. Next inventory should leave Management mutation breadth and select a
  self-contained API/utility pair, unless host assembly can expose those owned
  instances without duplicating conductors or SQLite writers.

### Worker 18ak — Claude thinking-model name heuristic

- `internal/util/claude_model.rs` and its test are fully ported and active in
  the Rust utility module. Unicode lowercasing followed by literal `claude` and
  `thinking` substring checks preserves the complete upstream function.
- All fourteen pinned cases pass, including case-insensitive Claude thinking
  names and rejection of non-Claude thinking models. An adversarial test makes
  the deliberately broad substring contract explicit: `unthinking` still
  matches when `claude` is also present.
- The helper is exported for later generic-handler header compatibility, but it
  is not wired into provider routing or actual thinking capability decisions.
  Existing request translators continue to use the typed model registry for
  those higher-authority choices.
- Both files move scaffold→ported. Mirror Closure reaches 134/605 with 471
  production and 386 upstream-test scaffolds open.
- Full gates pass 375 no-default and 399 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ak:

1. **Tiny helpers can encode deliberately broad compatibility.** Replacing the
   two substring checks with suffix parsing or a curated model list would be a
   behavior change, even if it looked more precise.
2. **Header heuristics are not capability authority.** A spoofed or unusual
   model name may trigger a harmless compatibility header, but must not select
   provider, credential, route or reasoning semantics.
3. **Case conversion is Unicode-aware upstream.** Rust `to_lowercase` is the
   appropriate semantic port; ASCII-only normalization would be an unstated
   narrowing even though current model names are usually ASCII.
4. **Edge behavior deserves executable documentation.** The `unthinking`
   example prevents a later cleanup from silently turning the compatibility
   predicate into token or suffix matching.

Strategy adaptation after worker 18ak:

- Continue with small production+test pairs, but connect them only at their
  proper authority level. Audit `disable_image_generation_mode.go` next: if it
  is pure configuration normalization, port it independently and keep actual
  tool injection governed by the existing typed model/runtime path.

### Worker 18al — Four-state image-generation configuration

- `internal/config/disable_image_generation_mode.rs` is fully ported and active
  as a four-variant Rust enum: `Off`, `All`, `Chat` and `Passthrough`. Display,
  JSON/YAML serialization and deserialization preserve bool values for Off/All
  and strings for Chat/Passthrough.
- String parsing keeps all pinned aliases: empty/false/0/off/no, true/1/on/yes,
  chat and passthrough after trim and Unicode lowercase. Unknown values fail
  with a typed fixed-shape error.
- A dedicated JSON parser preserves the Go edge contract that empty input and
  `null` mean Off, while raw numeric JSON remains invalid even though quoted
  `"0"` and `"1"` are accepted aliases. Generic Serde remains available for
  typed config documents.
- Three test groups cover the four pinned YAML cases with round-trip, four
  pinned JSON cases with round-trip, all aliases, null/empty input and invalid
  values. `serde_yaml` is added only as a dev dependency for wire-format tests.
- The mode is intentionally not collapsed into the existing Codex executor
  boolean. `Chat` and `Passthrough` require endpoint-aware tool injection and
  stripping semantics that a bool cannot represent.
- Production and test files move scaffold→ported. Mirror Closure reaches
  135/605 with 470 production and 385 upstream-test scaffolds open.
- Full gates pass 378 no-default and 402 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18al:

1. **A four-state option must remain four-state through runtime assembly.** A
   premature bool conversion loses the behavioral difference between chat-only
   suppression and non-images passthrough.
2. **Text aliases and JSON tokens are different inputs.** Quoted `"0"` is a
   supported string alias; raw JSON `0` is not a bool or string and must fail.
3. **Empty JSON needs an explicit compatibility entry point.** A generic Serde
   deserializer rejects an empty document before visiting the type, whereas the
   upstream custom method maps it to Off.
4. **Serialization is part of the config contract.** Off/All must remain YAML
   and JSON booleans, not strings, so existing configuration files and mutation
   clients retain their shape.
5. **Parsing does not authorize tool behavior.** Endpoint category, model
   capability and client payload still belong to the executor/request layer;
   this enum only carries operator intent without loss.

Strategy adaptation after worker 18al:

- Before wiring this mode, port or audit the upstream payload helper that
  distinguishes image endpoints from chat/non-images endpoints. Until then,
  keep the current typed Codex boolean path unchanged and avoid claiming
  `Chat`/`Passthrough` runtime support.

### Worker 18am — Endpoint-aware image-generation payload slice

- `internal/runtime/executor/helps/payload_helpers.rs` moves scaffold→partial
  and enters a new explicit `helps` module. Only the coherent image-generation
  slice is accepted; upstream's defaults/overrides/filters, header gates,
  source-protocol rules, query-path DSL and model globbing remain open.
- Exact and suffix-tolerant Images endpoint recognition covers generations and
  edits. `All` strips on every path, `Chat` strips only outside Images routes,
  and `Off`/`Passthrough` never strip.
- The mutation removes exact `image_generation` entries from root-relative
  `tools`, and removes string/object `tool_choice` forms by case-insensitive
  type or `{type:"tool",name:"image_generation"}`. Other tools and nested root
  objects remain intact.
- Empty, invalid-JSON, absent-root and no-change paths preserve exact bytes.
  Changed JSON is structurally reserialized; byte-level differential evidence
  for upstream SJSON edit locality remains part of the open full helper port.
- Four tests cover flat/nested tool removal, both tool-choice forms, Chat
  endpoint distinction and exact Off/Passthrough no-ops. The upstream test file
  moves scaffold→partial because its later payload-rule DSL cases are not yet
  ported.
- Partial classification raises Mirror Closure to 136/605 with 469 production
  and 384 upstream-test scaffolds open; it is not a full semantic-port claim.
- Full gates pass 382 no-default and 406 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18am:

1. **A large helper file can contain a valid smaller authority slice.** Endpoint
   gating and tool removal are cohesive; pulling in the entire rule DSL merely
   to activate four-state config would multiply unrelated parser risk.
2. **Tool declaration and tool choice use different comparison contracts.**
   Upstream removes declarations only for exact type text but treats explicit
   choice type/name case-insensitively; the port preserves that asymmetry.
3. **Passthrough is stronger than enabled.** Both avoid stripping here, but
   Passthrough also forbids injection elsewhere; executor wiring must retain the
   variant rather than deriving only a strip boolean.
4. **No-op byte identity is achievable independently.** Parsing is skipped or
   original bytes are returned whenever no mutation is required, protecting the
   dominant passthrough path even before an edit-local JSON engine is ported.
5. **Partial tests must remain partial.** The same upstream test file contains
   header, source-protocol and conditional payload-rule cases; four green image
   tests cannot honestly disposition those families.

Strategy adaptation after worker 18am:

- Do not wire the enum into the live Codex executor until request-path metadata
  and the separate injection policy carry all four modes end to end. Continue
  the payload helper later as its own rule-engine cluster with differential
  fixtures; choose another self-contained pair for the next Mirror worker.

### Worker 18an — Credential-weight normalization and typed config bounds

- `internal/credentialweight/weight.rs` and its test are fully ported and
  active. Empty strings retain the default weight `1`; signed integers at or
  below zero normalize to exclusion weight `0`; positive values are capped at
  the upstream maximum `1_000_000`.
- String and `serde_json::Value` parsing reject fractional values, booleans,
  containers and signed overflow. Unsigned JSON integers above the maximum
  return the same maximum-bound error without a narrowing cast.
- `internal/config/weight.rs` and its test are `adapted_to_ctox`. Upstream's
  raw YAML-node walk is replaced by validation on CTOX's three typed runtime
  account families: Claude, Codex and Antigravity. Serde rejects fraction and
  integer overflow before semantic validation; every active family applies the
  shared maximum afterward.
- Omitted account weights still materialize as `1`, while an explicit `0`
  survives serialization and candidate construction. Negative values remain
  valid exclusion inputs and the weighted scheduler already excludes every
  nonpositive candidate.
- All four mirrored files move scaffold→ported/adapted_to_ctox. Mirror Closure
  reaches 138/605 with 467 production and 382 upstream-test scaffolds open.
- Full gates pass 388 no-default and 412 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18an:

1. **Syntax rejection and semantic bounds belong at different layers.** Typed
   deserialization eliminates fractions and integer overflow; the shared
   normalization primitive still owns exclusion and maximum semantics.
2. **Nonpositive weight is an exclusion signal, not invalid input.** Rejecting
   `-1` would break upstream configuration that deliberately removes an
   account from weighted selection.
3. **The maximum is a scheduler safety invariant.** It bounds per-pick addition
   and aggregate weight growth before Rust's existing saturating arithmetic is
   needed, while preserving the pinned upstream limit.
4. **Explicit zero must remain distinguishable from omission.** CTOX stores a
   materialized typed config, so a serialize/reload cycle must not turn a
   disabled weighted lane back into default weight `1`.
5. **A generic provider matrix would be invented surface area.** CTOX currently
   exposes exactly three subscription account families; adapting validation to
   those types is complete for the active runtime without fabricating the
   upstream YAML DTO hierarchy.

Strategy adaptation after worker 18an:

- Keep selecting complete production+test clusters whose authority already
  exists in CTOX. Audit the next candidate for hidden global config/store
  dependencies before implementation; return to the payload-rule DSL only as
  a bounded parser/evaluator cluster with differential fixtures.

### Worker 18ao — Claude built-in tool registry augmentation

- `internal/runtime/executor/helps/claude_builtin_tools.rs` and its test are
  fully ported and active in the executor-help module. A missing registry seeds
  exactly `web_search`, `code_execution`, `text_editor` and `computer`.
- Valid root `tools` arrays add only entries with both a nonempty `type` and a
  nonempty `name`. Untyped custom tools remain excluded; newly discovered
  typed names and repeated existing names are set to `true`.
- A caller-supplied registry is deliberately not default-seeded. Its existing
  false entries survive unless the same name is discovered as a typed tool,
  matching Go map mutation semantics through Rust ownership.
- Empty, invalid or non-array payloads return the registry unchanged. Rust uses
  strict `serde_json` parsing at this already JSON-validated request boundary;
  it does not reproduce GJSON's permissive scanning of malformed documents.
- Both mirrored files move scaffold→ported. Mirror Closure reaches 139/605
  with 466 production and 381 upstream-test scaffolds open.
- The pinned Go tests pass. Full Rust gates pass 392 no-default and 416 default
  unit tests plus 31 integrations in each matrix, both warning-denied Clippy
  matrices and formatting.

Forensic findings after worker 18ao:

1. **Nil and empty registry are different inputs upstream.** Only absence
   installs the four defaults; an explicitly supplied empty map stays empty
   until typed tools are observed.
2. **Type presence is the built-in classifier.** Names such as `Read` are not
   sufficient on their own, preventing ordinary client tools from being
   mistaken for Claude server tools.
3. **Registry values are state, not mere set membership.** A preexisting false
   value must remain false unless an observed typed tool promotes that exact
   name, so `HashMap<String, bool>` is more faithful than an unconditional set.
4. **Malformed JSON tolerance is not runtime authority.** CTOX accepts this
   helper only after request JSON validation; strict parse failure therefore
   leaves state unchanged instead of inventing a second permissive parser.
5. **Discovery does not grant capability by itself.** The helper records names
   for downstream interpretation; provider/model capability gates still decide
   whether a built-in tool can actually be dispatched.

Strategy adaptation after worker 18ao:

- Continue with a self-contained helper or policy pair. Avoid the tempting
  `config_apikey.go` leaf until the shared `Auth` classification/type cluster is
  ported, because recreating that leaf against a second DTO would split auth
  authority.

### Worker 18ap — Structured plugin identity log fields

- `internal/pluginhost/logging.rs` and its test are fully ported and compiled
  as a reusable Pluginhost API. Normal and metadata-derived field builders keep
  `plugin_id` unconditionally and add trimmed nonempty name, version and path.
- Hot-reload fields preserve distinct active and retired version/path keys, so
  a replacement event cannot collapse both identities into a single ambiguous
  plugin record.
- Rust uses `HashMap<String, String>` in place of Logrus' dynamically typed
  `Fields`: every accepted upstream value is textual, so the narrower value type
  prevents unrelated structured payloads from entering this identity helper.
- A focused test adds trim/blank coverage beyond the three pinned cases. The
  helper is public inside the already active Pluginhost module rather than
  hidden behind a dead-code exception; supervisor/loader logging can consume
  one canonical field contract when that lifecycle is expanded.
- Both mirrored files move scaffold→ported. Mirror Closure reaches 140/605
  with 465 production and 380 upstream-test scaffolds open.
- The pinned Go tests pass. Full Rust gates pass 396 no-default and 420 default
  unit tests plus 31 integrations in each matrix, both warning-denied Clippy
  matrices and formatting.

Forensic findings after worker 18ap:

1. **Plugin identity needs two hot-reload sides.** Logging only the new version
   erases which executable was retired and makes crash/restart evidence harder
   to reconstruct.
2. **The plugin ID is structurally mandatory even when blank.** Upstream always
   emits its key; validation of whether an ID is usable belongs to the process
   supervisor, not to a log-field formatter.
3. **Optional blanks must be omitted after trimming.** Emitting empty paths or
   versions would make downstream evidence consumers distinguish meaningless
   empty strings from true identity fields.
4. **A textual map is a useful Rust narrowing.** The upstream helper never
   stores numbers, booleans or objects, so a dynamic `Value` map would add
   representational states with no compatibility benefit.
5. **Compiled-but-private helpers can become false closure.** Warning-denied
   Clippy exposed that risk; making the canonical field builders reachable is
   preferable to suppressing dead-code evidence.

Strategy adaptation after worker 18ap:

- The smallest-file heuristic remains useful only after dependency audit.
  Continue with another pair whose owning type is already active; batch closely
  related leaves when a tiny function otherwise depends on a scaffolded core.

### Worker 18aq — Codex credential-file replacement boundary

- `internal/auth/codex/filename.rs` moves scaffold→replaced_by_ctox. The Go
  filename derivation is intentionally not implemented: CTOX does not persist
  subscription OAuth material in vendor JSON files or derive filesystem paths
  from email, plan and account identity.
- The existing `CodexSecretHandle`/`CodexCredentialHandles` boundary replaces
  that persistence identity. Account-scoped opaque handle names distinguish
  accounts that share an email and plan, while typed ID/access/refresh kinds
  distinguish the three records inside one atomic credential snapshot.
- `filename_test.rs` moves scaffold→adapted_to_ctox and is active in the Codex
  auth test graph. It proves two account hashes remain distinct without email,
  `.json` or path material, and proves all three typed records use distinct
  names and kinds.
- The production replacement points to the existing host-owned encrypted
  secret-store path and full-snapshot rotation; no file helper, ambient home
  directory or vendor CLI auth flow is reintroduced.
- Mirror Closure reaches 141/605 with 464 production and 379 upstream-test
  scaffolds open.
- Full gates pass 398 no-default and 422 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18aq:

1. **Filename compatibility would violate the owning architecture.** Porting a
   pure string helper looks harmless, but it creates the canonical primitive
   later code would use to reopen forbidden credential-file persistence.
2. **The upstream uniqueness intent survives without its storage mechanism.**
   Account identity remains part of opaque record names, so same-email accounts
   do not collide even though no path is generated.
3. **Credentials need three typed identities, not one flattened document.** A
   swapped ID/access/refresh handle is rejected at the type boundary, and the
   host store rotates their values as one transaction.
4. **Email and plan are unnecessary persistence identifiers.** Removing them
   from storage handles reduces personal-data exposure in logs, diagnostics and
   filesystem metadata without weakening account routing.
5. **Replacement status is substantive closure.** The file is not skipped; its
   behavior is explicitly owned by a stronger CTOX primitive and exercised by
   adapted tests.

Strategy adaptation after worker 18aq:

- Audit other provider filename/home-store leaves as replacement candidates,
  but award closure only when an active typed secret-store primitive and an
  adapted behavioral test demonstrate the displaced intent.

### Worker 18ar — Antigravity credential-file replacement boundary

- `internal/auth/antigravity/filename.rs` moves
  scaffold→replaced_by_ctox. The email-derived `antigravity[-email].json`
  filename is intentionally absent because CTOX never persists subscription
  tokens or routing state in vendor credential files.
- Active `AntigravityCredentialHandles` provide account-scoped opaque names for
  access, refresh and state records. `CtoxAntigravitySecretStore` encrypts all
  three records; the state record binds expiry and project ID to the same
  transactional snapshot as both tokens.
- Existing Root tests are the replacement evidence: a real encrypted-store
  roundtrip recovers tokens, expiry and project while keeping Debug redacted;
  an injected SQLite trigger failure on the state record rolls back rotated
  access token, refresh token, expiry and project together.
- No additional mirrored upstream test exists for this file. The stronger
  replacement is therefore evidenced by the host-store tests rather than an
  invented filename unit test.
- Mirror Closure reaches 142/605 with 463 production and 379 upstream-test
  scaffolds open. The crate remains at 398 no-default and 422 default unit
  tests plus 31 integrations per matrix; both focused Root replacement tests
  pass.

Forensic findings after worker 18ar:

1. **Antigravity state is part of credential consistency.** Project ID and
   expiry are not bearer tokens, but storing them outside the token transaction
   can route a fresh token with stale project authority after a crash.
2. **Email-based filenames add PII without adding account authority.** Opaque
   account-scoped handles distinguish records while avoiding addresses in
   filesystem metadata and diagnostics.
3. **A rollback test is stronger than a naming test.** The critical replacement
   property is all-or-nothing rotation, not reproduction of a display string.
4. **Replacement can reuse existing evidence.** Adding a second test that only
   restates the already active store contract would inflate counts without
   improving the completion proof.

Strategy adaptation after worker 18ar:

- The provider filename family is now fully dispositioned. Return to complete
  body+test pairs; prefer policy or parsing helpers over standalone leaves that
  require the scaffolded generic Auth aggregate.

### Worker 18as — Plugin update-version comparison

- `internal/pluginstore/version.rs` and its test are fully ported and active in
  a new explicit Pluginstore module. Public `update_available` retains trim and
  single leading `v`/`V` normalization without importing a broader SemVer policy.
- Dotted numeric releases compare segment-by-segment as signed 64-bit
  nonnegative integers; missing segments are zero, so `0.1`, `0.1.0` and
  `0.1.0.0` are equal. Numeric ordering avoids the `9` versus `10`
  lexicographic bug and never advertises an installed newer release.
- Any distinct pair containing a prerelease, negative, empty, overflow or other
  nonnumeric segment falls back to “update available”, exactly as upstream.
  Exact normalized string equality still returns false before parsing.
- The ten pinned cases pass in both Go and Rust. Two additional Rust test groups
  cover leading zeros/zero tails, whitespace, negative/empty segments, `i64`
  overflow and the upstream single-`v` edge.
- Both files move scaffold→ported. Mirror Closure reaches 143/605 with 462
  production and 378 upstream-test scaffolds open.
- Full gates pass 401 no-default and 425 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18as:

1. **This is intentionally not semantic-version precedence.** Prereleases do
   not sort below releases; any unequal nonnumeric form is simply offered as an
   update.
2. **Equality has two levels.** Normalized textual equality short-circuits even
   malformed versions, while numerically equivalent dotted releases compare
   equal despite leading or missing zero segments.
3. **Overflow is noncomparability, not saturation.** Clamping a huge segment
   would create an ordering upstream does not define.
4. **A single prefix character is special.** Bare `v` is not normalized to
   empty and therefore follows the nonnumeric inequality fallback.

Strategy adaptation after worker 18as:

- Keep Pluginstore network/install files scaffolded until checksum, manifest,
  registry and process-isolation boundaries can be ported as one verified
  vertical slice. Continue with another pure pair outside that I/O cluster.

### Worker 18at — Example API-key safe mode and warning page

- `internal/safemode/example_api_keys.rs` and its test are fully ported and
  active in a new explicit SafeMode module. Detection recognizes only the three
  pinned template values after trim, deduplicates them and preserves first-seen
  input order.
- Similar strings such as `your-api-key`, `your-api-key-4`, `change-me` and
  embedded matches do not trigger the guard. `has_example_api_keys` derives
  solely from the same detector, avoiding a second predicate.
- The complete pinned warning-page document is ported, including its optional
  key list and trimmed Management link. Empty inputs omit both optional blocks;
  no local configuration path is rendered.
- Key text and Management href use Go-compatible HTML escaping for ampersand,
  apostrophe, angle brackets and quote. An adversarial test proves neither key
  nor URL can inject HTML attributes or elements.
- The three pinned Go tests and five Rust tests pass. Both mirrored files move
  scaffold→ported. Mirror Closure reaches 144/605 with 461 production and 377
  upstream-test scaffolds open.
- Full gates pass 406 no-default and 430 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18at:

1. **Safe mode uses an exact denylist, not a weak-key heuristic.** Broadening it
   to `change-me` or substring matches would disable legitimate configurations
   beyond upstream behavior.
2. **First-seen order is part of the diagnostic contract.** Sorting detected
   keys would change the operator-facing report even though set membership is
   unchanged.
3. **The warning page is a security boundary.** Values usually come from a
   pinned list, but the renderer itself accepts strings and must escape both
   element text and an attribute value.
4. **Management navigation is optional authority.** A blank path must not emit
   a dead or attacker-controlled button placeholder.

Strategy adaptation after worker 18at:

- Safe-mode parsing is now available but should be wired only when the typed
  top-level downstream API-key config enters the Rust host. Next select another
  complete pure pair; do not add an ambient config reader merely to activate it.

### Worker 18au — Browser-facing recursive JSON HTML sanitizer

- `internal/htmlsanitize/htmlsanitize.rs` and its test are fully ported and
  active through an explicit internal module. The shared string primitive uses
  Go-compatible escaping for ampersand, apostrophe, angle brackets and quote.
- JSON sanitization parses exactly one document, recursively escapes string
  values only and emits compact JSON. Object keys, numbers, booleans and null
  remain semantic data rather than HTML presentation content.
- Empty input, malformed JSON, invalid UTF-8 and trailing documents remain an
  exact byte-for-byte no-op. Arbitrary-precision Serde numbers preserve the
  upstream `UseNumber` boundary instead of rounding large integer lexemes.
- Likely-JSON detection recognizes valid `application/json` MIME values,
  structured `+json` suffixes and leading object/array shapes. Malformed content
  types use a conservative lowercase fallback; detection never replaces strict
  JSON validation.
- SafeMode now consumes the shared sanitizer rather than maintaining a second
  HTML-escaping implementation. Five pinned Rust tests, the upstream Go package
  tests and the five SafeMode regressions pass.
- Both mirrored files move scaffold→ported. Mirror Closure reaches 145/605 with
  460 production and 376 upstream-test scaffolds open.
- Full gates pass 411 no-default and 435 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18au:

1. **Sanitizing JSON changes application data intentionally.** This helper is a
   browser-facing management-response defense, not a transparent transformation
   for provider protocol payloads.
2. **Values and keys have different trust roles.** Escaping object keys would
   silently rename fields and break protocol dispatch; only recursively reached
   string values are transformed.
3. **Number handling is a compatibility boundary.** A default floating-point
   parse would corrupt sufficiently large provider counters or identifiers even
   though the worker appears concerned only with strings.
4. **Shape detection is not validity.** Content type and leading delimiters may
   select the sanitizer, but malformed or multi-document bodies must still pass
   through unchanged.
5. **One canonical escape primitive prevents drift.** SafeMode and JSON response
   hardening now share the exact Go mapping, including its numeric quote entity.

Strategy adaptation after worker 18au:

- Activate HTML sanitization only at explicitly browser-facing management
  response boundaries. Never place it in the provider request/response adapter
  pipeline, where byte and semantic transparency take precedence.

### Worker 18av — Upstream Home control-plane replacement

- `internal/config/home.rs` moves scaffold→replaced_by_ctox and its test moves
  scaffold→adapted_to_ctox. Both are active in the Rust Config module graph.
- Upstream's `HomeConfig` models a second Redis-backed control plane injected by
  `-home-jwt`. CTOX already owns durable queueing, runtime state, policy and
  lifecycle, so the gateway does not recreate that competing authority.
- The typed `CliproxyRuntimeConfig` replacement is stricter than upstream's
  loader behavior: its closed Serde schema rejects a top-level `home` object in
  both YAML and JSON instead of accepting and then silently discarding it.
- The adapted tests include representative discovery and TLS fields, including
  a secret-looking client-key value. Diagnostics identify only the unknown
  top-level field and do not echo nested secret material.
- The pinned upstream Go test still passes and documents the intentional delta:
  Go ignores Home config from ordinary bytes, while CTOX fails closed because
  Home state must enter through no gateway configuration path at all.
- Mirror Closure reaches 146/605 with 459 production and 375 upstream-test
  scaffolds open.
- Full gates pass 413 no-default and 437 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18av:

1. **Silent ignore is unsafe at the ownership boundary.** An operator who
   supplies Home configuration should receive a deterministic schema error,
   not believe that a second control plane is active.
2. **Porting the DTO would create architectural gravity.** Even unused Redis and
   TLS fields become a compatibility surface that later workers may mistakenly
   wire into the proxy lifecycle.
3. **Closed schemas protect secret provenance.** Rejecting the parent field
   before interpreting children prevents Home JWT, client key and certificate
   material from becoming generic gateway configuration.
4. **Adaptation preserves the upstream security intent, not its mechanism.**
   Ordinary config bytes still cannot activate Home; CTOX strengthens the
   evidence from post-parse zero values to parse-time rejection.

Strategy adaptation after worker 18av:

- Treat other displaced upstream control-plane and global-store surfaces as
  replacement candidates only when an active CTOX owner and fail-closed test
  exist. Do not port dormant DTOs merely to improve structural resemblance.

### Worker 18aw — Request-scoped WebSocket replay signal

- `sdk/cliproxy/executor/websocket.rs` and its test are fully ported and active
  through a new explicit SDK Executor module. The constructor returns a typed,
  zero-payload Rust error rather than allocating a boxed interface value.
- Display preserves the exact upstream JSON error envelope, including status
  426, `server_error` and `upstream_http_replay_required`. Inherent accessors
  expose the same status and request-scoped classification without prematurely
  porting the much larger generic Executor type surface.
- `is_upstream_websocket_replay_required` walks the standard Rust `source()`
  chain, matching Go `errors.As` for direct and conventionally wrapped errors.
  An unrelated I/O error is pinned false.
- The signal remains distinct from provider failure: it requests a full HTTP
  replay because incremental WebSocket state cannot continue, and therefore
  must not cool down or fail over the selected credential as if auth failed.
- Both Rust tests and the pinned upstream Go test pass. Both mirrored files move
  scaffold→ported. Mirror Closure reaches 147/605 with 458 production and 374
  upstream-test scaffolds open.
- Full gates pass 415 no-default and 439 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18aw:

1. **Transport replay is not credential failure.** Losing reusable WebSocket
   state says nothing about token validity or provider quota, so account health
   must remain untouched.
2. **426 is an internal control signal with a stable wire representation.** The
   JSON body lets HTTP-facing adapters preserve upstream behavior while typed
   Rust callers can branch without parsing the display text.
3. **Wrapped recognition is part of the contract.** Checking only the outermost
   dynamic error would lose the signal as soon as a runtime adds context.
4. **The generic Executor interfaces need not be guessed early.** Inherent typed
   methods close this leaf faithfully while `types.go` remains scaffolded until
   its request, response, metadata and lifecycle shapes can land together.

Strategy adaptation after worker 18aw:

- Wire the replay signal only at an incremental WebSocket→full-HTTP fallback
  boundary that explicitly suppresses credential penalties. Do not treat 426 as
  generic provider unavailability or automatically retry it across accounts.

### Worker 18ax — Typed SDK auth-weight boundary

- `sdk/cliproxy/auth/weight.rs` and its test move scaffold→adapted_to_ctox and
  are active in the SDK Auth module graph. `validate_auth_weight` accepts the
  already typed candidate weight and delegates to the single ported
  CredentialWeight normalization/bounds core.
- Upstream's two mutable sources, string `Attributes[weight]` and dynamically
  typed `Metadata[weight]`, are intentionally absent. The closed
  `CliproxyRuntimeConfig` account schema rejects both objects as unknown fields,
  so neither can override routing weight after typed configuration validation.
- A configured weight of seven survives validation into `AccountCandidate`.
  Nonpositive input remains valid exclusion semantics and values above
  1,000,000 fail with the shared typed error.
- `ApplyAuthWeightMetadata` has no Rust analogue because CTOX does not copy
  arbitrary request/source metadata into immutable credential authority. This
  is an ownership replacement, not an omitted convenience function.
- Both adapted Rust tests and the pinned upstream Go validation test pass.
  Mirror Closure reaches 148/605 with 457 production and 373 upstream-test
  scaffolds open.
- Full gates pass 417 no-default and 441 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ax:

1. **Weight needs one authority.** Supporting attributes, metadata and typed
   config simultaneously would recreate precedence ambiguity at the scheduler
   boundary.
2. **Nonpositive weight is eligibility policy, not malformed input.** The shared
   normalizer maps it to zero so Weighted Round Robin excludes the candidate
   without rejecting the whole topology.
3. **Validation must precede candidate construction.** The host factory already
   validates all Claude, Codex and Antigravity accounts before exposing their
   typed candidates; the SDK helper remains available for other typed producers.
4. **Schema rejection is stronger than ignored metadata.** Operators receive an
   explicit unknown-field error instead of believing a runtime override won.

Strategy adaptation after worker 18ax:

- When the remaining generic SDK Auth aggregate lands, keep routing weight a
  typed immutable field. Do not restore `attributes.weight`, `metadata.weight`
  or a post-selection metadata copier for upstream structural familiarity.

### Worker 18ay — Borrowed full-syntax GJSON lookup

- `internal/util/gjson.rs` and its test are fully ported and active through the
  existing Util module. The helper returns `gjson::Value<'a>` tied to the input
  lifetime, so Rust enforces upstream's manual “do not retain or mutate” rule.
- The dependency is pinned exactly to Tidwall's MIT-licensed Rust `gjson 0.8.1`,
  whose documented path syntax mirrors the Go package. The port therefore keeps
  escaped keys, array indexes, filters, projections, wildcards and modifiers
  rather than substituting a dotted-key heuristic.
- Empty input returns a missing value exactly as upstream. Invalid UTF-8 also
  returns missing through a checked conversion; JSON is UTF-8 by definition,
  and the Rust boundary does not reproduce Go's unsafe byte-to-string cast.
- A pointer-range assertion proves a simple nested result borrows directly from
  the source buffer. Additional tests pin escaped-dot lookup and a filtered
  multi-result projection, where GJSON may legitimately own synthesized output.
- Three Rust tests and both pinned upstream Go tests pass. Both mirrored files
  move scaffold→ported. Mirror Closure reaches 149/605 with 456 production and
  372 upstream-test scaffolds open.
- Full gates pass 420 no-default and 444 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ay:

1. **Sixteen wrapper lines contain a whole query language.** Reimplementing only
   literal dot segments would have passed the two upstream unit cases while
   breaking real callers that use filters, projections or escaped keys.
2. **Borrowing is observable performance behavior.** The direct result must
   point into the original request bytes; materializing `serde_json::Value`
   would silently turn the no-copy hot path into an allocating tree parse.
3. **The lifetime is stronger than a comment.** Rust prevents a borrowed result
   from outliving its request buffer, eliminating the unsafe retention class the
   Go helper can only document.
4. **Invalid UTF-8 deserves no unsafe compatibility.** A malformed byte sequence
   is not JSON, so fail-missing preserves safe lookup behavior without unchecked
   aliasing or replacement-character matches.

Strategy adaptation after worker 18ay:

- Reuse the borrowed helper in the remaining Gemini/Kimi/payload readers that
  already depend on Go GJSON. Keep the exact Tidwall crate pin in the upstream
  drift audit and do not introduce parallel ad-hoc path walkers.

### Worker 18az — Raw-preserving Vertex tool-call ID stripper

- `internal/runtime/executor/helps/vertex_payload_helpers.rs` and its test are
  fully ported and active in the Runtime Executor Helps module. The public Rust
  signature returns `Cow<[u8]>`, explicitly representing upstream's shared
  backing buffer on no-op and owned bytes only after a real mutation.
- The helper activates only for a trimmed, case-insensitive
  `openai-response` source. A borrowed full-syntax GJSON scan proves that
  `functionCall.id` or `functionResponse.id` exists before any allocation.
- Target IDs are removed as bounded raw byte-range edits inside the immediate
  call objects. Nested `args.id` and `response.id`, all other member order,
  whitespace, top-level bytes and large integer lexemes remain untouched.
- The first implementation passed semantic tests but reserialized the complete
  body on change. Forensic review rejected that broader mutation and replaced it
  with a fail-closed JSON member scanner plus overlap-checked reverse edits.
- Malformed payloads, unsupported source formats and valid payloads without the
  target IDs are borrowed byte-identical no-ops. Four Rust tests and both pinned
  upstream Go tests pass.
- Both mirrored files move scaffold→ported. Mirror Closure reaches 150/605 with
  455 production and 371 upstream-test scaffolds open.
- Full gates pass 424 no-default and 448 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18az:

1. **Semantic JSON equality was insufficient here.** Tool arguments can contain
   precision-sensitive numbers and opaque provider material; a two-field policy
   must not normalize the entire request body.
2. **The no-op path is the performance contract.** Vertex requests without
   translated Responses IDs—including multi-megabyte text bodies—perform only a
   borrowed path scan and return the original slice.
3. **Scope is immediate and asymmetric.** Vertex rejects transport call IDs,
   while an application's own `args.id` or tool result `response.id` remains
   domain data and must survive.
4. **Raw edits need structural proof.** Each edited object must be a borrowed
   subrange of the input, all ranges must be bounded and non-overlapping, and
   any scanner ambiguity returns the original payload rather than partial data.

Strategy adaptation after worker 18az:

- Prefer borrowed detection plus narrowly bounded raw edits for remaining
  provider payload policies that promise byte-preserving no-ops. Reuse this
  pattern only with explicit range, overlap, nested-value and malformed-input
  tests; do not turn it into an unchecked generic SJSON clone.

### Worker 18ba — Typed execution-resource lifecycle

- `sdk/cliproxy/executor/lifecycle.rs` and its test move
  scaffold→adapted_to_ctox and are active beside the WebSocket replay signal in
  the SDK Executor module.
- `ExecutionLifecycle` receives a clonable `BoundResourceCloser`. Its shared
  `OnceLock` caches the first close result, so concurrent or repeated handles
  execute the underlying one-shot cleanup exactly once and observe the same
  success or failure.
- `bind_execution_resource` accepts the lifecycle directly instead of partially
  inventing the still-scaffolded generic `Options` aggregate. Missing lifecycle
  or closer remains a no-op; a failed bind closes immediately.
- When bind and cleanup both fail, `BindAndCloseError` retains both typed error
  objects. A plain bind failure is returned by identity after successful cleanup,
  matching Go's `errors.Join`/`errors.Is` intent without string flattening.
- Four Rust tests cover close-once across cloned handles, bind failure cleanup,
  dual-error retention and nil-equivalent no-ops. Both pinned upstream Go tests
  pass.
- Mirror Closure reaches 151/605 with 454 production and 370 upstream-test
  scaffolds open.
- Full gates pass 428 no-default and 452 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18ba:

1. **Binding transfers cleanup authority, not resource ownership ambiguity.**
   Once a closer is constructed, either the lifecycle holds it or a failed bind
   invokes it immediately.
2. **Close-once includes the result.** Re-running a fallible closer is unsafe;
   later calls must replay the first failure just as they replay success.
3. **A bind failure must not mask a cleanup failure.** Both causes matter for
   diagnosing leaked sockets or streams during a lifecycle race.
4. **Porting `Options` one field at a time would harden the wrong shape.** Direct
   injection closes this resource contract while request/response/metadata and
   interceptor fields remain one coherent future type worker.

Strategy adaptation after worker 18ba:

- Use `BoundResourceCloser` at future stream/socket ownership transfers and
  require every rejected bind to prove immediate cleanup. Keep the generic
  Executor `types.go` scaffold open until all metadata, format, interceptor and
  lifecycle fields can be modeled together.

### Worker 18bb — Typed WebSocket transport context

- `sdk/cliproxy/executor/context.rs` moves scaffold→adapted_to_ctox and is active
  in the SDK Executor module. There is no mirrored upstream test file for this
  leaf; three inline Rust tests provide direct behavioral evidence.
- Private Go `context.Context` keys become an immutable, copyable
  `ExecutionTransportContext` with two private booleans. `Option`-based helpers
  preserve nil semantics: missing context reports false and setters create a
  default context before marking their own flag.
- `downstream_websocket` records request origin, while
  `required_upstream_websocket` records that incremental state would be lost by
  HTTP fallback. Setters never conflate the two intentions.
- Chaining a required-upstream child from a downstream-WebSocket parent keeps
  both flags on the child without mutating the parent, matching Go context
  derivation without an untyped value bag.
- The complete pinned upstream Executor package remains green. Mirror Closure
  reaches 152/605 with 453 production and 370 upstream-test scaffolds open.
- Full gates pass 431 no-default and 455 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18bb:

1. **Origin and fallback safety are different facts.** A downstream WebSocket
   request may still allow HTTP upstream execution, while an incremental request
   can require its current upstream WebSocket regardless of downstream origin.
2. **Private typed fields prevent context spoofing.** Arbitrary extensions cannot
   inject a same-named string key or non-boolean value into transport policy.
3. **Copy-on-derive matches request scoping.** A child may strengthen transport
   requirements without retroactively changing its parent request state.
4. **This is a future `Options` input, not ambient global state.** The context is
   explicitly carried and remains independent from provider, credential and
   model selection.

Strategy adaptation after worker 18bb:

- Embed this typed context when the full Executor request/options cluster lands
  and connect `required_upstream_websocket` to the 426 replay signal. Preserve
  both flags independently through translation and account selection.

### Worker 18bc — Coherent typed Executor request/response contract

- `sdk/cliproxy/executor/types.rs` and its mirrored test move
  scaffold→adapted_to_ctox and are active in the SDK Executor module beside the
  lifecycle and WebSocket contracts.
- All pinned metadata keys remain explicit constants, but the Go
  `map[string]any` is split into `ExecutionMetadata` for known execution policy
  and a JSON-only extension map. Auth-selection callbacks are typed `Arc`
  closures outside serializable data; generation keeps the upstream
  missing/true-enabled and explicit-false-disabled rule.
- Request, post-auth interceptor request/response, termination response,
  execution options, aggregate response and stream result land as one cluster.
  Bodies remain bytes, Debug reports lengths rather than content, lifecycle is
  an injected trait object and Tokio `mpsc::Receiver` gives the stream consumer
  a single owned receive lane.
- `Options` now carries the typed `ExecutionTransportContext` explicitly. The
  downstream-origin and required-upstream flags remain independent, and the
  existing WebSocket replay error implements the common `StatusError` and
  `RequestScopedError` traits.
- `RequestTerminatedError` retains the exact upstream error text, typed status
  and cloned response-header/body accessors. `response_format_or_source`
  preserves the explicit-response override and empty-response fallback.
- Six Rust tests cover both pinned format cases plus response-copy ownership,
  typed metadata/callback policy, transport-context propagation and stream-lane
  ownership. The complete pinned upstream Executor package passes.
- Mirror Closure reaches 153/605 with 452 production and 369 upstream-test
  scaffolds open.
- Full gates pass 437 no-default and 461 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18bc:

1. **Executable callbacks are not metadata.** Keeping closures outside the
   JSON-like extension map prevents accidental serialization, persistence and
   log projection while preserving the selected-auth notifications.
2. **The aggregate must land coherently.** Interceptor, lifecycle, transport,
   response format and stream ownership constrain one another; separate field
   workers would have created incompatible intermediate APIs.
3. **Go channel direction becomes ownership.** A single Tokio receiver is
   stronger than a receive-only channel annotation because it also prevents
   cloning competing consumers.
4. **Bodies need an evidence-safe Debug boundary.** Request, interceptor,
   termination and stream types expose byte lengths only; raw provider or
   plugin bodies do not enter routine diagnostics.
5. **Header behavior is not closed by the container alias alone.** The current
   ordered multi-value map preserves values and deterministic iteration, but
   any future mutation helper must still prove Go `http.Header`'s
   case-insensitive replacement/removal semantics.

Strategy adaptation after worker 18bc:

- Make new SDK auth/session/executor leaves consume `ExecutionMetadata` instead
  of rebuilding stringly maps. Add header mutation semantics only with a
  focused parity gate, and connect required-upstream transport to the 426
  replay branch at the actual WebSocket execution/fallback boundary rather
  than inside the passive type layer.

### Worker 18bd — Shared provider-neutral auth error and status types

- `sdk/cliproxy/auth/errors.rs`, `status.rs` and the compatibility test move
  scaffold→adapted_to_ctox and become active SDK Auth exports.
- `AuthError` preserves the four upstream wire fields, `omitempty` for empty
  code/zero status, exact code-prefixed Display behavior and typed retryability.
  It implements the Executor `StatusError` and `RequestScopedError` traits, so
  auth selection can classify failures without downcasting provider types.
- Routine Debug exposes the code, flags, status and message length but not the
  message body. Display and serialized downstream responses retain the explicit
  human-readable message as required by the public error contract.
- `AuthStatus` models all six pinned values and an `Other(String)` variant.
  This deliberately preserves Go's open named-string domain instead of turning
  an upstream-added state into a deserialization failure.
- The upstream unkeyed-literal ABI test becomes a Rust named-field plus exact
  JSON-shape test: source-order coupling has no Rust equivalent, while the wire
  schema and public fields remain executable compatibility surfaces.
- Four Rust tests cover complete/omitted error shapes, shared traits, redacted
  Debug and known/future status roundtrips. The pinned upstream compatibility
  test passes.
- Mirror Closure reaches 155/605 with 450 production and 368 upstream-test
  scaffolds open.
- Full gates pass 441 no-default and 465 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18bd:

1. **A Go named string is an open set.** A closed Rust enum would reject future
   states that the upstream binary can carry; an explicit `Other` variant keeps
   forward compatibility visible and lossless.
2. **Source ABI and wire compatibility differ.** Go's unkeyed struct literal
   pins field order for callers. Rust callers use named fields, so exact Serde
   output and trait behavior are the relevant compatibility evidence.
3. **Classification should be trait-based.** WebSocket replay, plugin
   termination and auth failures now expose the same status surface without
   string parsing or provider-specific manager branches.
4. **Debug and public messages have different trust roles.** The public error
   may intentionally carry a safe message, while routine structural Debug
   still avoids copying it into diagnostic logs.

Strategy adaptation after worker 18bd:

- Reuse the shared error traits in later conductor/manager ports and preserve
  unknown lifecycle states through `AuthStatus::Other`. Before porting generic
  Auth metadata maps, audit custom-header extraction and classification
  together so executable headers and OAuth/API-key identity cannot become
  unvalidated JSON authority.

### Worker 18be — Active auth identity, classification and custom-header core

- `sdk/cliproxy/auth/classification.rs` and `custom_headers.rs` move
  scaffold→adapted_to_ctox; `types.rs` moves scaffold→partial because the
  pinned 716-line aggregate still owns refresh, storage, quota and model-state
  behavior that has not yet landed.
- The active `Auth` identity core preserves provider-neutral IDs, indexes,
  labels, status, counters, recent-request buckets and JSON-constrained
  attributes/metadata. Routine Debug reports only keys and lengths; credential,
  proxy and metadata values remain outside diagnostics.
- Index derivation follows the upstream SHA-256/first-eight-byte contract and
  its precedence across API-key identity, compatibility base URL, canonical
  JSON-file path, explicit seed and ID fallback. Credential material may affect
  the opaque index but never appears in its output or Debug representation.
- `AuthKind` and `AuthSourceKind` retain upstream aliases and precedence,
  including legacy OAuth metadata detection. Unknown classifications remain
  explicit rather than being guessed from the provider name.
- Custom headers are extracted from the JSON metadata object and projected into
  the existing `header:` attribute namespace. The Rust boundary additionally
  rejects invalid HTTP-token names and CR/LF or control-bearing values before
  they become executable request authority.
- The twenty ten-minute recent-request buckets preserve chronological order,
  zero-fill gaps and retain aggregate success/failure counters.
- Eleven focused Rust tests and the corresponding pinned Go Auth tests pass.
  Full gates pass 452 no-default and 476 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.

Forensic findings after worker 18be:

1. **The Auth aggregate cannot be faked by a narrow replacement type.** Its
   identity and classification surface is already shared, while refresh,
   storage, quota and model state form separate coherent ownership clusters;
   `types.rs` therefore remains honestly partial.
2. **Hash input and diagnostic output have different trust boundaries.** Raw
   credentials are legitimate input to the compatibility index, but neither
   the digest nor structural Debug may reveal those inputs.
3. **Header metadata is executable authority.** Go accepts the strings and
   relies on later HTTP machinery; Rust validates token syntax and control
   characters at projection time so persisted metadata cannot inject headers.
4. **Local-time formatting can alter process isolation indirectly.** Enabling
   Chrono's `clock` feature linked macOS CoreFoundation, which injected
   `__CF_USER_TEXT_ENCODING` into an otherwise `env_clear()` plugin child and
   tripped the strict empty-environment guard. A small `localtime_r` boundary
   preserves local bucket labels without weakening process isolation.
5. **Untyped Go metadata is intentionally narrower here.** JSON values cover
   persisted extension data; callbacks and other executable objects remain
   outside the serializable map.

Strategy adaptation after worker 18be:

- Complete the remaining `types.go` surface as coherent refresh/storage and
  quota/model-state workers, not as isolated fields. Preserve typed secret-store
  authority, keep callbacks non-serializable and rerun the plugin empty-env
  guard whenever a dependency-feature change can alter the child process image.

### Worker 18bf — Injected token-storage ownership boundary

- `internal/auth/models.rs` and `internal/auth/empty/token.rs` move
  scaffold→adapted_to_ctox and become active internal Auth modules.
- `TokenStorage` preserves the upstream mutable save operation, but accepts a
  typed `Path` and can only act through an implementation explicitly injected
  by the runtime/secret owner. An `Auth` record alone gains no ambient file or
  secret-store authority.
- `SharedTokenStorage` models Go interface shallow-copy behavior with one shared
  `Arc` implementation. Its mutex makes the mutable receiver and concurrent
  refresh-write serialization explicit.
- The active partial `Auth` aggregate now carries the optional shared storage
  handle. Clone retains implementation identity, while structural Debug exposes
  only whether a storage handle exists.
- `EmptyStorage` preserves the exact `{"type":"empty"}` wire form and marks
  itself on save, but performs no path access or file creation.
- Two focused Rust tests cover no-I/O EmptyStorage behavior and Auth clone
  identity. Both pinned Go packages compile; full gates pass 454 no-default and
  478 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and formatting.

Forensic findings after worker 18bf:

1. **An interface field carries authority, not just data.** The port therefore
   shares an injected implementation object rather than interpreting Auth
   metadata or a filename as permission to persist credentials.
2. **Go shallow clone semantics matter for storage.** Deep-cloning an unknown
   implementation could fork locks or persistence state; one `Arc` preserves
   the original identity across Auth clones.
3. **The mutable receiver needs an explicit concurrency rule.** A mutex around
   the implementation prevents simultaneous refresh saves from racing through
   a storage backend that upstream treats as mutable.
4. **No-op storage still has observable behavior.** It updates its public type
   marker but must not touch even a syntactically valid supplied path.

Strategy adaptation after worker 18bf:

- Build expiration parsing and refresh-lead selection on this injected boundary
  before activating conductor refresh. Then add quota/model state and timestamp
  serialization as one clone-safe aggregate; never derive persistence authority
  from `file_name`, metadata or the presence of an expiry timestamp.

### Worker 18bg — Auth expiration and refresh-lead decision core

- The active partial `sdk/cliproxy/auth/types.rs` now ports expiration
  extraction and refresh-lead selection without enabling Chrono's process-image
  altering `clock` feature.
- Expiration preserves the six-key upstream precedence, ignores an invalid
  earlier value in favor of the next valid key, and recursively supports the
  legacy `token`/`Token` object shapes.
- RFC3339 with offsets/fractions, both timezone-free UTC layouts, integer
  strings and JSON numeric seconds/milliseconds are supported. The threshold is
  the pinned strict `> 1_000_000_000_000`; numeric fractions truncate toward
  zero like Go.
- A nonpositive numeric value is a successfully parsed Go zero time, represented
  as `0001-01-01T00:00:00Z`, rather than an absent expiration.
- `RefreshLeadRuntime` is a typed, non-serializable injected runtime surface.
  A positive runtime lead wins; missing/zero runtime output falls back to the
  normalized provider registry, whose factory output must also be positive.
- Auth clones share the same runtime identity through `Arc`; structural Debug
  exposes only `has_runtime`.
- Three Rust tests cover timestamp precedence/formats/nesting/units/zero,
  runtime-vs-registry precedence and clone/Debug behavior. A temporary public
  Go differential probe produced matching values and was removed afterwards;
  the complete pinned Go Auth package passes.
- Full gates pass 457 no-default and 481 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. Mirror
  Closure remains 160/605 because this worker deepens the already-partial
  `types.rs`; 445 production and 365 test scaffolds remain open.

Forensic findings after worker 18bg:

1. **Go zero time is not parse failure.** For float64/json.Number values at or
   below zero, `ExpirationTime` returns `(time.Time{}, true)`; mapping that to
   Rust `None` would suppress refresh work and change scheduling semantics.
2. **The unit heuristic is deliberately asymmetric.** Exactly `1e12` is still
   seconds; only larger values are milliseconds. A conventional digit-count
   heuristic would silently diverge.
3. **Runtime policy precedes provider defaults only when usable.** A zero or
   absent runtime value does not disable refresh; it deliberately falls through
   to the registered provider factory.
4. **Persisted JSON narrows Go's untyped numeric domain.** Go accepts float64,
   int64 and json.Number but not a native `int` in this parser. Rust metadata is
   JSON-constrained, so every representable number follows the JSON-number
   branch; the differential probe records this intentional boundary.
5. **Clock access is unnecessary for parsing.** Chrono's clock feature remains
   disabled, and the plugin empty-environment integration gate stays green.

Strategy adaptation after worker 18bg:

- Add `QuotaState`, `ModelState`, aggregate timestamps and deep-clone behavior
  together, including exact zero-time wire representation. Only then activate
  conductor refresh/scheduling consumers so they share one authoritative Auth
  state rather than translating between the existing cooldown DTO and a second
  incomplete model.

### Worker 18bh — Complete provider-neutral Auth aggregate and wire contract

- `sdk/cliproxy/auth/types.rs` moves partial→adapted_to_ctox: every pinned
  upstream type, field and helper now has an active Rust owner.
- `QuotaState` and `ModelState` preserve exceeded/reason/recovery/backoff,
  lifecycle/message/unavailable/retry/error/quota/update state and their exact
  `omitempty` behavior.
- Auth now owns quota, all five UTC lifecycle timestamps and the per-model state
  map. Chrono gains only its Serde feature; `clock` remains disabled.
- The default and populated JSON documents match a public Go differential probe,
  including the empty named-string zero status, mandatory quota/timestamps,
  year-one zero times and omission of index, filename, storage, runtime,
  counters and empty optional fields.
- Storage and runtime authority remain non-serializable and Arc-shared across
  clones. Model state, nested errors, maps and JSON values clone independently;
  routine Debug exposes map keys, lengths and authority-presence booleans rather
  than metadata, proxy, reason or status-message contents.
- `RequestInfo`, `PostAuthContext` and `PostAuthHook` replace Go context-value
  lookup with a typed copy-on-derive request context. Hooks remain executable,
  non-Serde objects and receive mutable Auth explicitly.
- Three additional Rust tests cover exact default/populated wire roundtrip,
  deep data clone/shared injected owners and typed post-auth context/hook
  behavior. The temporary public Go wire/clone probe was removed after use; the
  complete pinned Go Auth package passes.
- Full gates pass 460 no-default and 484 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting. The plugin
  empty-environment gate remains green.
- Mirror Closure stays 160/605 because `types.rs` was already classified, but
  the status mix improves from 64 partial/68 adapted to 63 partial/69 adapted;
  445 production and 365 test scaffolds remain open.

Forensic findings after worker 18bh:

1. **The zero value of a Go named status is empty, not `StatusUnknown`.** Exact
   default JSON therefore uses `"status":""`; the explicit constant still
   roundtrips as `"unknown"` when assigned.
2. **Wire state and executable authority must be separated field by field.**
   Quota and timestamps persist, while storage/runtime handles, local indexes,
   filenames and counters stay outside Serde exactly as their Go `json:"-"`
   tags require.
3. **Go's shallow metadata clone retains dangerous nested aliases.** Rust
   intentionally deep-clones owned JSON values, preventing a cloned Auth from
   mutating its source. Only injected owners retain the upstream shared identity.
4. **Context values become a typed capability.** Copy-on-derive preserves the
   parent context and makes request query/header access explicit without a
   globally spoofable key.
5. **Build-cache pressure is now recurrent evidence.** A second scoped
   `cargo clean -p ctox-cliproxyapi` removed 6.1 GiB of regenerable package
   output after disk exhaustion; no source, runtime state or foreign artifacts
   were removed.

Strategy adaptation after worker 18bh:

- Port conductor refresh against this single Auth aggregate next: due-time
  evaluation, expiration lead, refresh result merge and storage persistence must
  land together. Keep the existing CTOX typed secret-store authority as the only
  production backend, and use scoped package-cache cleanup before dual-matrix
  gates while host disk pressure remains.

### Worker 18bi — Refresh policy and deterministic due-time index

- `sdk/cliproxy/auth/conductor_refresh.rs` and `auto_refresh_loop.rs` move
  scaffold→partial; `auto_refresh_loop_test.rs` moves
  scaffold→adapted_to_ctox and becomes active.
- Refresh policy now gates unauthorized terminal refresh failures and future
  `next_refresh_after`, delegates to an injected runtime evaluator, resolves
  last-refresh metadata/attribute aliases, applies preferred intervals before
  provider leads and handles missing/expired credentials exactly at a supplied
  UTC instant.
- API-key auths are unscheduled, while a disabled OAuth auth retains its expiry
  refresh schedule like upstream. Runtime-evaluator presence schedules the next
  check independently of its current boolean decision.
- The complete positive Go duration language is active: ns/us/µs/μs/ms/s/m/h,
  compound and fractional values, leading plus and bare numeric seconds. Invalid,
  zero and negative values are ignored at the policy boundary.
- Long fractional parsing preserves Go's observable float64 rounding, and huge
  positive bare seconds saturate at the current Go `time.Duration` maximum.
- Access-/refresh-token alias lookup and unauthorized-auth detection are shared
  policy helpers; values never enter Debug output.
- `RefreshSchedule` replaces the mutex-protected updateable Go heap with a
  mutex-protected ordered due/id index. Upsert, remove, peek and pop-due remain
  logarithmic; equal deadlines dispatch deterministically by public auth ID.
- Seven Rust tests cover the six pinned scheduling families plus Duration,
  unauthorized/alias policy and queue mutation. The pinned Go schedule tests and
  a temporary package-local Duration differential probe pass; the probe was
  removed, and the complete pinned Go Auth package passes.
- Full gates pass 467 no-default and 491 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices and formatting.
- Mirror Closure reaches 162/605 with 443 production and 364 upstream-test
  scaffolds open.

Forensic findings after worker 18bi:

1. **Go Duration fractions are not pure decimal truncation.** The standard
   parser multiplies through float64 for fractional units; the long-tail probe
   `0.000000001999…s` observably yields 2 ns rather than 1 ns.
2. **Bare numeric overflow follows the active Go conversion.** Under the pinned
   Go 1.26 toolchain, `1e100` seconds becomes `MaxInt64` nanoseconds and remains
   positive. Rust caps at the same signed duration maximum.
3. **Scheduling and evaluation are separate runtime capabilities.** Merely
   having an evaluator keeps an auth in the periodic schedule; its boolean
   result decides whether the current check actually refreshes.
4. **Disabled does not mean unscheduled for OAuth.** Upstream still schedules a
   disabled OAuth credential at its expiry lead, whereas static API keys leave
   the schedule entirely.
5. **The upstream policy mixes explicit and ambient time.** Its provider-lead
   branch calls `time.Until(expiry)` despite receiving `now`. Rust consistently
   uses the supplied instant so tests, scheduling and execution share one clock;
   this is an explicit CTOX determinism adaptation.
6. **Equal Go heap deadlines have unspecified order.** The Rust due/id index
   adds stable auth-ID ordering without changing due membership.

Strategy adaptation after worker 18bi:

- Next port the synchronous refresh transaction: one per-auth lock, stale-token
  coalescing, cloned executor input, success/failure normalization, model-state
  recovery and authoritative store save. The worker/loop lifecycle stays partial
  until the active Manager/store contract can own cancellation and bounded
  concurrency without a parallel state authority.

### Worker 18bj — Durable per-auth refresh transaction

- `sdk/cliproxy/auth/store.rs` moves scaffold→adapted_to_ctox and exposes the
  injected `AuthStore` list/save/delete boundary with typed read, write, delete
  and invalid-record failures. The crate does not infer credential authority
  from `Auth.file_name`; the CTOX host remains the only production owner.
- `conductor_refresh.rs` remains partial but now owns an active synchronous
  transaction: stable auth IDs select per-auth locks, the authoritative store is
  reloaded while locked, and the complete result transition is saved before the
  lock is released.
- Concurrent callers presenting the same failed access token coalesce after the
  first rotation. A changed current token returns the authoritative record and
  neither invokes the refresher nor performs another save.
- Refreshers receive a mutable owned clone. A successful `None` result preserves
  in-place mutation like Go's nil-success path, while cancellation performs no
  state transition and no persistence.
- Successful refresh preserves the prior runtime when the executor supplies
  none, normalizes refresh/error/availability timestamps and status, recovers
  unauthorized model states and quota, recomputes aggregate availability and
  applies the 30-second ineffective-refresh backoff.
- Stable identity is enforced before persistence: a refresher cannot change the
  auth ID and thereby replace another account record.
- Unlike upstream's refresh-failure path, CTOX durably saves failure evidence.
  Unauthorized failures become terminal/unavailable without another scheduled
  refresh; other failures receive the five-minute retry backoff. If both refresh
  and store fail, the typed transaction error retains both causes.
- `conductor_unauthorized_refresh_test.rs` moves scaffold→adapted_to_ctox. Eight
  Rust tests cover stale-token concurrency, missing refresh credentials,
  success normalization/model recovery, ineffective refresh, unauthorized and
  ordinary failure persistence, joined store evidence, stable identity and
  cancellation.
- The complete pinned Go Auth package passes. Full gates pass 475 no-default and
  499 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and formatting.
- Mirror Closure reaches 163/605 with 442 production and 363 upstream-test
  scaffolds open.

Forensic findings after worker 18bj:

1. **The lock must span reload through save.** Locking only the network refresh
   still permits a waiting request to read stale manager state and rotate the
   same subscription twice.
2. **The store, not the caller's Auth clone, is authoritative.** Request retries
   carry the access token that failed; after acquiring the lock they must compare
   it with a freshly loaded record to detect another caller's completed rotation.
3. **Go nil-success is a mutation contract.** Some executors mutate the supplied
   clone and return nil without error. Rust represents that explicitly as
   `Ok(None)` and promotes the mutated clone.
4. **Cancellation is not an ordinary provider failure.** It leaves refresh
   timestamps, backoff and durable error evidence unchanged.
5. **Upstream loses refresh-failure evidence at restart.** Its error branch
   updates only the manager map; the CTOX durable-state-first adaptation saves
   the transition and exposes a joined refresh/store error if persistence fails.
6. **Model recovery has a second owner boundary.** The transaction returns the
   resumed model IDs after clearing their 401 state; mutating the global model
   registry remains a host/Manager lifecycle responsibility.
7. **Store return strings are not authority.** They may describe a backend
   location, but credential identity remains the stable Auth ID and typed CTOX
   secret handles.

Strategy adaptation after worker 18bj:

- Activate `conductor_lifecycle.rs` next around the injected store: load,
  register and update must preserve runtime-owned fields, validate identity and
  feed scheduler mutations without creating a second in-memory authority. Once
  lifecycle persistence is active, connect the existing partial auto-refresh
  loop to this transaction with bounded cancellation and explicit model-registry
  resumption.

### Worker 18bk — Store-first auth lifecycle and typed persistence policy

- `sdk/cliproxy/auth/conductor_lifecycle.rs` moves scaffold→partial and becomes
  active; `persist_policy.rs` moves scaffold→adapted_to_ctox. Update, remove and
  persist-policy test mirrors all move scaffold→adapted_to_ctox.
- `AuthLifecycle` implements authoritative Load, Register, Update, runtime-only
  Remove and explicit owning-store Delete around the injected `AuthStore`, an
  in-process runtime cache and the existing `RefreshSchedule`.
- A single lifecycle mutation lock spans store verification/write, cache
  publication and schedule mutation. Durable Register/Update saves before
  publishing, so store failure cannot manufacture a newer in-memory authority.
- Load replaces the complete cache from the store, skips empty IDs, assigns
  stable public indexes, rejects duplicate stable IDs and rebuilds refresh
  scheduling at an explicitly supplied time.
- Update reloads durable source state before merging. It preserves the cached
  index, injected storage/runtime owners, aggregate counters and recent-request
  ring. Active→active updates inherit missing model state; every transition with
  a disabled side deliberately drops stale model cooldown state.
- Missing durable records fail closed instead of being resurrected by a late
  update. Updates may not silently change persistence ownership between durable
  and runtime-only classes.
- Runtime Remove mirrors upstream and never deletes the owning credential.
  Durable Delete is a separate explicit API and removes store, cache and refresh
  schedule only after the store deletion succeeds.
- `PersistenceIntent::{Persist, SourceAlreadyPersisted}` replaces private Go
  context keys. Config, memory/runtime-only, plugin-virtual and metadata-empty
  records never enter the Auth store. The already-persisted intent verifies that
  a durable record actually exists before cache publication.
- Rust deliberately treats empty metadata like Go nil metadata: an empty owned
  map contains no credential authority and is runtime-only. This removes a
  meaningless allocation-state distinction that Serde cannot preserve.
- Fourteen Rust tests cover load/indexing, generated IDs, save-before-publish,
  source projection, active/disabled merge rules, counters/recent state,
  missing-source and ownership failures, rollback visibility, runtime removal,
  scheduling, explicit deletion and persistence classification.
- The complete pinned Go Auth package passes. Full gates pass 489 no-default and
  513 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and formatting.
- Mirror Closure reaches 165/605 with 440 production and 360 upstream-test
  scaffolds open.

Forensic findings after worker 18bk:

1. **Upstream publishes before persistence and often ignores the error.** That
   can leave the running proxy newer than its restart state. CTOX reverses the
   order: durable save succeeds before cache/scheduler publication.
2. **A typed skip flag alone is insufficient.** `SourceAlreadyPersisted` could
   otherwise become a persistence bypass. Durable-class projections must prove
   the record exists in the injected store.
3. **The cache is runtime enrichment, not durable authority.** Store records win
   for update existence and model-state merge; only non-serializable owners,
   counters, recent buckets and assigned index are retained from the cache.
4. **Persistence class is an ownership boundary.** Letting an ordinary Update
   add `runtime_only` would strand an older durable credential. The transition is
   rejected until a future explicit migration API owns deletion and rollback.
5. **Remove and Delete cannot share a method.** Upstream Remove is intentionally
   runtime-only because file/token-store deletion belongs to the caller. Rust
   makes the destructive durable operation separately named and fallible.
6. **Go nil-map behavior is not a sound Rust authority signal.** The owned Auth
   aggregate cannot distinguish nil from allocated-empty metadata. Empty means
   no durable credential payload and is consistently classified runtime-only.
7. **Manager side effects remain open.** Executor replacement/session close,
   hooks, API-key alias rebuild, cooldown snapshots, home-session maps and model
   registry callbacks keep `conductor_lifecycle.rs` honestly partial.

Strategy adaptation after worker 18bk:

- Connect `RefreshCoordinator` to `AuthLifecycle` next through one transactional
  refresh entry point that republishes a successfully saved record into the
  cache and schedule, and resumes returned model IDs through an injected host
  callback. Then activate the bounded auto-refresh worker around `RefreshSchedule`;
  do not add executor/session/hook side effects until their typed owner contracts
  exist.

### Worker 18bl — Transactional refresh republication

- `conductor_lifecycle.rs` remains partial but now owns the single supported
  bridge from `RefreshCoordinator` into lifecycle cache and refresh schedule.
  `conductor_scheduler_refresh_test.rs` moves scaffold→partial and activates the
  refresh/scheduler subset of the larger upstream test file.
- Lifecycle refresh holds the lifecycle mutation lock while the per-auth refresh
  transaction reloads, refreshes and saves. Only after durable success does it
  republish the Auth record and recompute the next due time.
- Non-serializable runtime/storage owners, counters, recent-request buckets and
  stable index are reattached from the lifecycle cache after the store-backed
  refresh result returns.
- Successful refresh republishes normalized state, schedules the next provider
  lead and emits recovered model IDs through an injected `ModelResumeSink`.
  The sink runs after the lifecycle lock is released, preventing host registry
  callbacks from reentering the transaction.
- A durably saved ordinary refresh failure is reloaded and republishes the
  five-minute retry time. A durably saved unauthorized failure republishes its
  terminal status and is removed from the auto-refresh schedule.
- If failure evidence cannot be reloaded after the refresh transaction reports a
  saved error, `AuthLifecycleRefreshError::Republish` retains both the original
  refresh failure and the lifecycle/store failure.
- Three Rust tests cover success/cache/schedule/model-resume, unauthorized
  terminal unscheduling and ordinary five-minute failure scheduling.
- The complete pinned Go Auth package passes. Full gates pass 492 no-default and
  516 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and formatting.
- Mirror Closure remains 165/605 because the production lifecycle was already
  partial; the upstream-test backlog falls to 359, with 440 production
  scaffolds open.

Forensic findings after worker 18bl:

1. **Durable success is not complete until runtime state is republished.** A
   refreshed token in the store with a stale scheduler/cache can still trigger
   duplicate work or route a request with the old credential.
2. **Store serialization intentionally drops process owners.** Refresh results
   must regain runtime/storage handles and counters from the cache without
   allowing the cache to overwrite durable credential fields.
3. **Failure is also a state transition.** Since worker 18bj persists refresh
   failure evidence, lifecycle must reload and schedule/unschedule that saved
   state even though the public operation returns an error.
4. **Callbacks cannot run under mutation locks.** Global model-registry resume
   may trigger observers or future lifecycle calls; it is emitted only after
   cache and schedule are consistent and the lock is released.
5. **The upstream scheduler test file contains unrelated selection repair.** Its
   refresh subset is active, but model-registration and cooldown-driven scheduler
   rebuild cases remain open; the test mirror therefore stays honestly partial.

Strategy adaptation after worker 18bl:

- Activate the bounded auto-refresh worker next using `RefreshSchedule` as its
  only clock index and `AuthLifecycle::refresh` as its only mutation entry. Use
  explicit cancellation, bounded concurrent refreshes and injected provider
  refresher lookup; retain deterministic supplied-clock helpers for unit tests.

### Worker 18bm — Bounded cancellable auto-refresh worker

- `auto_refresh_loop.rs` moves partial→adapted_to_ctox. Its existing due-time
  policy/index now owns a live Tokio dispatcher, bounded job lane, configurable
  worker set, provider-refresher resolver, clock and owned shutdown handle.
- `RefreshSchedule` emits coalescing wake notifications on effective Upsert and
  Remove, so lifecycle mutations reset the worker without a second dirty map or
  duplicate heap authority.
- Upstream defaults remain five seconds, sixteen workers and at least a
  sixty-four-job buffer. Explicit configuration normalizes zero values and never
  permits a job buffer smaller than the worker count.
- Every popped job rechecks current cached Auth state, next due time and
  `should_refresh` immediately before provider lookup. Missing providers retry at
  the configured interval; cancelled/infrastructure attempts use the upstream
  sixty-second pending backoff.
- The worker uses `AuthLifecycle::refresh_with_cancellation` as its only mutation
  entry. `RefreshCancellation` is a clonable typed signal: the default refresher
  checks it before work, while provider I/O implementations can observe it
  during a request. Stop signals cancellation before joining the dispatcher.
- A bounded MPSC lane and fixed worker set replace goroutine-per-item behavior.
  Blocking provider refresh work runs through Tokio's blocking pool rather than
  occupying the async reactor.
- The first concurrency test exposed a global Lifecycle-lock bottleneck: two
  configured workers achieved only one active refresh. `AuthLifecycle` now uses
  an exclusive full-Load gate plus per-auth mutation locks, preserving same-auth
  serialization while allowing independent accounts to refresh concurrently.
- A per-auth pending set coalesces wakeups during in-flight refresh. The entire
  popped due set is marked before bounded sends; on shutdown, queued or not-yet-
  sent IDs are reinserted with pending backoff so no due credential disappears.
- Eight new Rust tests cover defaults/buffer bounds, late Schedule wakeup,
  missing-provider retry, exact parallelism bound, same-auth coalescing, future
  timer shutdown, popped-job recovery, cancellation propagation and pending
  backoff. The two concurrency-sensitive tests each pass eight repeated runs.
- The complete pinned Go Auth package passes. Full gates pass 500 no-default and
  524 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and formatting.
- Mirror Closure remains 165/605 because the production file was already
  classified partial; 440 production and 359 upstream-test scaffolds remain.
- A scoped `cargo clean -p ctox-cliproxyapi` removed 7.0 GiB of regenerable build
  output after free space fell to 371 MiB; source and runtime state were untouched.

Forensic findings after worker 18bm:

1. **Configured worker count does not prove concurrency.** A global lifecycle
   lock silently reduced independent refreshes to one. Full reload needs global
   exclusivity; steady-state mutation needs per-auth serialization.
2. **Pop-all plus bounded-send can lose work on shutdown.** IDs later in the
   popped vector never reached the channel. Marking the complete set pending
   before the first send makes recovery complete and deterministic.
3. **Schedule wakeups must come from the index owner.** A parallel dirty map
   duplicates membership authority. Notify permits the same index mutation to
   wake/reset the timer without another state structure.
4. **Pending is execution state, not credential state.** The Rust worker keeps a
   private per-auth pending set and retry deadline instead of temporarily writing
   `NextRefreshAfter` into the durable Auth record before network execution.
5. **Cancellation needs a provider-visible capability.** Stopping only the async
   dispatcher cannot interrupt blocking provider I/O. `RefreshCancellation`
   crosses the resolver/lifecycle/coordinator boundary; concrete transports must
   bind it to their request cancellation in their integration worker.
6. **Drop and awaited Stop have different evidence.** Drop requests cancellation;
   callers requiring proof that workers exited must consume the handle through
   `stop().await`.
7. **The wall/monotonic split matches timer reality.** UTC decides credential
   policy and persisted deadlines; Tokio's monotonic duration owns the actual
   sleep. Every wake recomputes UTC before popping.

Strategy adaptation after worker 18bm:

- Audit the remaining `conductor_lifecycle.rs`, `conductor_selection.rs` and
  scheduler-refresh test deltas as one Manager-assembly problem. Next activate
  typed executor registration/replacement, session-close and scheduler-entry
  rebuild owners around `AuthLifecycle`; do not reintroduce a second Auth map or
  global provider registry.

### Worker 18bn — Typed executor capability registry

- `conductor.rs` remains honestly partial but now owns the active
  `ProviderExecutorRegistration` and concurrent `ProviderExecutorRegistry`.
  The registry is the single injected provider lookup for auto refresh; it does
  not duplicate Auth records, model capabilities or CTOX secret authority.
- Rust cannot safely reproduce Go's runtime assertion from `ProviderExecutor`
  to optional `ExecutionSessionCloser`. The registration therefore composes a
  required `AuthRefresher` with an explicit optional session closer. Full
  request execution remains on the already active provider transports until the
  manager-dispatch slice wires their async contract.
- Provider keys are trimmed and lowercased on both registration and lookup.
  This intentionally fixes a pinned-upstream asymmetry where registration kept
  case but unregister/lookup normalized case.
- Replacement swaps under the registry write lock, then closes the displaced
  provider with `__all_execution_sessions__` after releasing the lock.
  Re-registering the same capability Arcs is idempotent, including through a
  freshly constructed registration wrapper.
- Unregister preserves upstream ownership semantics and does not implicitly
  close an externally owned executor. Auth removal can invoke the separate
  typed `close_all_sessions` capability when the Manager assembly lands.
- `conductor_executor_replace_test.rs` moves scaffold→adapted_to_ctox with seven
  direct tests for replacement close, normalized lookup and resolver use,
  idempotence, unregister ownership, explicit close and invalid provider keys.
- The complete pinned Go Auth package passes. Full gates pass 507 no-default
  and 531 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and package-scoped formatting.
- Mirror Closure remains 165/605 because `conductor.rs` was already partial;
  440 production and 358 upstream-test scaffolds remain.

Forensic findings after worker 18bn:

1. **Optional Go interfaces need capability composition, not downcasting.** A
   Rust trait object cannot generally be queried for another unrelated trait.
   An explicit optional closer makes ownership visible and testable.
2. **Executor identity is capability identity in this slice.** Comparing the
   refresher and optional closer Arcs avoids closing a live executor when a
   configuration reload merely rebuilds its registration wrapper.
3. **Never call provider code while holding the registry lock.** Session close
   may release transports, trigger telemetry or re-enter manager lookup. The
   displaced entry is captured under lock and closed afterward.
4. **Normalization must be symmetric.** The pinned Go implementation trims on
   registration but lowercases on removal/lookup. Rust normalizes at the
   boundary once, preventing unreachable mixed-case registrations.
5. **This is not yet the full ProviderExecutor port.** Execute, stream,
   token-count and HTTP-request dispatch need a coherent async trait over the
   active Rust transports; labeling the refresh/session capability slice as the
   complete executor would hide real work.

Strategy adaptation after worker 18bn:

- Build the scheduler-entry view next from `AuthLifecycle` snapshots plus an
  injected model-capability source. Keep it a derived index with explicit
  refresh/remove operations; do not copy credentials, create a second durable
  Auth authority or consult an ambient global model registry. Then wrap
  lifecycle, executor registry and scheduler view in the smallest Manager
  assembly that can close provider sessions on Auth removal.

### Worker 18bo — Derived scheduler capability view

- `conductor_selection.rs` moves scaffold→partial with an active
  `AuthSchedulerView`. `AuthLifecycle` remains the sole Auth owner; the view
  stores only secret-free `AccountCandidate` projections keyed by stable Auth
  ID.
- `SchedulerCapabilitySource` replaces upstream's ambient global model registry
  and scheduling values hidden in arbitrary Auth metadata. Its typed result
  carries priority, validated weight, websocket support and supported models;
  it receives only public Auth ID and normalized provider.
- An account with no capability snapshot or no supported models is absent from
  the view. After model discovery, `refresh_entry` canonicalizes suffix models,
  deduplicates deterministically and makes the account schedulable without
  copying credential data.
- Disabled, missing and runtime-removed Auth records are pruned on entry
  refresh. Full refresh derives a complete temporary map and publishes it under
  one write lock; invalid weight leaves the previous view intact.
- `AuthLifecycle::snapshot_cached` provides a cloned point-in-time input for
  derived views. It does not change durable ownership or expose any new
  serialization/logging path.
- Three additional tests in the already partial upstream scheduler-refresh
  mirror cover register-before-model-discovery, canonical model refresh and
  actual scheduler pick; atomic rebuild rejection; and disabled/removed pruning.
- The complete pinned Go Auth package passes. Full gates pass 510 no-default
  and 534 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and package-scoped formatting. Default Clippy
  was rerun with incremental compilation disabled after a missing Cargo cache
  file caused an infrastructure-only first failure.
- Mirror Closure rises to 166/605; 439 production and 358 upstream-test
  scaffolds remain.

Forensic findings after worker 18bo:

1. **An empty model set is not unrestricted in the Manager scheduler.** The
   existing low-level candidate type uses an empty vector as a permissive
   direct-provider default. The derived Manager view therefore omits accounts
   until discovery supplies at least one model instead of overloading that
   representation.
2. **Capabilities should not receive Auth metadata.** Passing the whole Auth to
   model discovery would recreate a secret-bearing global registry boundary.
   Public ID and provider are sufficient lookup keys.
3. **Full rebuild must be publish-atomic.** Validating while mutating the live
   map can leave half of the accounts on a new capability revision. Build then
   swap preserves the last complete view on error.
4. **Canonicalization belongs at index construction.** Every request pick can
   then compare stable base-model keys without repeatedly normalizing a mutable
   registry snapshot.
5. **Cross-owner mutation still needs Manager serialization.** Lifecycle and
   scheduler view are internally safe, but a direct lifecycle remove racing an
   entry refresh can temporarily republish stale routing metadata. The next
   assembly worker must serialize compound lifecycle/view/session operations.

Strategy adaptation after worker 18bo:

- Add the smallest `AuthManager` composition with one assembly mutation lock
  around Load/Register/Update/Remove/Delete and scheduler publication. Removal
  must capture the provider, remove lifecycle and view state, release assembly
  locks, then close all provider sessions through the typed executor registry.
  Provider callbacks must never run while a manager or registry lock is held.

### Worker 18bp — Compound Auth Manager assembly

- `conductor.rs` remains partial but now exposes the active `AuthManager`
  composition over `AuthLifecycle`, `ProviderExecutorRegistry` and
  `AuthSchedulerView`. Each owner stays independently testable; one assembly
  mutex orders only compound publication operations.
- Load, Register and Update publish lifecycle state before rebuilding the
  derived scheduler entry. If post-persistence capability validation fails,
  the affected routing entry is removed and a typed `AuthManagerError` is
  returned. Durable/Auth state remains available for operator repair but cannot
  route through stale metadata.
- An invalid full publication after authoritative Load clears the complete old
  routing view, because those candidates belong to a previous Auth snapshot.
  A capability-only `refresh_scheduler_all` retains the view's atomic
  build-then-swap behavior.
- Runtime Remove and explicit Delete capture the provider, remove Auth and
  scheduler state under the assembly lock, then release it before invoking
  `close_all_sessions`. Runtime Remove still leaves the owning CTOX store
  untouched; Delete retains the explicit owning-store boundary.
- Executor registration is forwarded to the single typed registry; candidate
  snapshots contain only the derived secret-free view. The manager does not add
  another Auth map, model registry or secret store.
- Three new assembly tests in the adapted Remove mirror cover complete runtime
  prune plus session cleanup, re-entrant session callback without deadlock,
  fail-closed invalid Update publication and old-view clearing after invalid
  Load publication.
- The complete pinned Go Auth package passes. Full gates pass 513 no-default
  and 537 default unit tests plus 31 integrations in each matrix, both
  warning-denied Clippy matrices and package-scoped formatting.
- Mirror Closure remains 166/605 because the production/test mirrors were
  already classified; 439 production and 358 test scaffolds remain.

Forensic findings after worker 18bp:

1. **Persistence and routability have different rollback semantics.** Once an
   Auth update is durably accepted, silently rolling it back because a derived
   capability is invalid would create a second persistence transaction. The
   safe response is to retain Auth evidence and remove routing authority.
2. **Authoritative Load invalidates every old candidate.** The scheduler view's
   normal atomic-rebuild rule preserves its previous map on error, but that map
   is unsafe after the Auth generation changed. Manager Load explicitly clears
   it.
3. **Session cleanup is an external callback.** A test closer re-enters
   `refresh_scheduler_all`; completion proves Manager and registry locks are no
   longer held when provider cleanup runs.
4. **Thread-safe components do not imply atomic composition.** Lifecycle and
   view locks prevent memory races independently, but only the manager lock
   prevents a refresh/remove interleaving from republishing stale candidates.
5. **Auto-refresh publication is the remaining composition seam.** The worker
   still calls `AuthLifecycle::refresh_with_cancellation` directly. Its cache
   and refresh schedule are consistent, but model-capability routing view must
   be refreshed through a manager-owned completion sink before this assembly is
   the sole mutation path.

Strategy adaptation after worker 18bp:

- Route auto-refresh completion through a manager-owned `ModelResumeSink`/
  scheduler-publication adapter without holding Lifecycle locks. Then audit the
  full async `ProviderExecutor` request/stream/token-count contract against the
  three active provider transports; do not label the current refresh/session
  registration as complete execution dispatch.

### Worker 18bq — Auto-refresh routing publication bridge

- `ModelResumeSink` gains a default `auth_published` completion callback.
  `AuthLifecycle::refresh_with_cancellation` invokes it after releasing all
  lifecycle locks on both successful refresh and durably published provider
  refresh failure; it fires even when no model state was resumed.
- `ManagerRefreshPublicationSink` bridges that callback into
  `AuthManager::refresh_scheduler_entry` and then forwards model-resume events
  to the original sink. It is constructed from a weak Manager reference, so the
  worker cannot keep a retired Manager alive.
- Scheduler publication failures increment a non-secret observable counter and
  remove the affected routing entry fail-closed. No provider error text,
  credential metadata or Auth snapshot enters this evidence.
- `AuthSchedulerView` now rejects unavailable Auths and explicit Disabled
  status in addition to the boolean disabled flag. A durably failed 401 refresh
  can therefore no longer leave its previous Candidate routable.
- A new test drives a manager-owned account through a durable unauthorized
  refresh failure and proves: lifecycle error returns, publication callback
  executes after lock release, Candidate disappears, downstream notification
  fires once and no publication failure is recorded.
- Full gates pass 514 no-default and 538 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices, package formatting and
  the complete pinned Go Auth package.
- Mirror Closure remains 166/605; 439 production and 358 test scaffolds remain.

Forensic findings after worker 18bq:

1. **Model resumption is not a refresh completion signal.** A successful token
   rotation may resume zero models, and a durable failure resumes none. Routing
   publication therefore needs its own callback.
2. **Failure return can follow successful state publication.** The refresh API
   returns the provider failure while cache/schedule/error state has already
   changed durably. Downstream views must be notified before that error leaves
   the lifecycle boundary.
3. **Unavailable belongs in Candidate construction.** Cooldown filtering alone
   cannot protect a terminal Auth whose derived view predates its refresh
   failure.
4. **Completion bridges must not own the Manager.** A weak reference prevents a
   long-lived worker/sink cycle during runtime reconfiguration.

Strategy adaptation after worker 18bq:

- Audit and port the full async provider execution trait next, including unary,
  stream, refresh, token count and credential-injected HTTP behavior. Bind
  concrete Claude, Codex and Antigravity transports incrementally behind the
  same typed registry, with an explicit optional session closer and no trait
  downcasts.

### Worker 18br — Plugin executor DTO and async signature cluster

- `sdk/pluginapi/types.rs` remains honestly partial but now ports the complete
  executor-facing DTO cluster: host HTTP request/response/stream, executor HTTP
  request/response, model request/response/stream, typed headers/query/metadata,
  `HostHttpClient` and the object-safe async `ProviderExecutor` contract.
- The async trait exposes identifier, unary execute, streaming execute, token
  count and credential-injected HTTP request through boxed Send futures. Stream
  responses own a single Tokio receiver, preserving Go's receive-only channel
  ownership without cloneable consumers.
- Host HTTP clients are injected as typed trait objects and skipped in Serde.
  Debug output reports only lengths, keys and client presence; storage JSON,
  payload bodies and auth metadata values are not rendered.
- The first wire test found two mechanical Go→Rust traps and drove corrections:
  acronym fields require explicit `AuthID`/`URL`/`StorageJSON` names, and Go
  `[]byte` JSON is Base64 rather than Serde's numeric byte array. One shared
  strict Base64 codec now covers every serializable executor byte field.
- `sdk/pluginapi/types_test.rs` moves scaffold→partial with an exact DTO
  roundtrip test and an object-safety/async execution test covering unary,
  stream signature, token-count error and HTTP response paths.
- Full gates pass 516 no-default and 540 default unit tests plus 31 integrations
  per matrix, both warning-denied Clippy matrices, package formatting and the
  pinned Go PluginAPI plus Auth packages.
- Mirror Closure remains 166/605; the upstream-test backlog falls to 357 while
  439 production scaffolds remain.

Forensic findings after worker 18br:

1. **Serde rename rules do not understand Go acronyms.** `auth_id` becomes
   `AuthId`, which is wire-incompatible with `AuthID`; every acronym field must
   be anchored explicitly.
2. **Byte arrays are a protocol decision.** Rust's natural JSON array compiles
   and roundtrips locally but breaks Go RPC peers. Base64 fidelity must be
   tested at the DTO boundary before transport integration.
3. **Injected clients are process authority, not payload.** They stay outside
   serialization and are represented as optional typed owners after decode.
4. **The plugin and Manager executor contracts are adjacent, not identical.**
   Plugin execution carries flattened serializable Auth projections and host
   HTTP policy; Manager execution owns live `Auth` plus refresh/session
   capabilities. The registry adapter must bridge them explicitly rather than
   conflating the traits.

Strategy adaptation after worker 18br:

- Add an optional async execution capability to
  `ProviderExecutorRegistration`, include it in replacement identity, and
  expose typed registry dispatch methods. Then implement the built-in adapter
  for one existing provider end-to-end before binding the other two; retain the
  separate `AuthRefresher` and optional session closer capabilities.

### Worker 18bs — Manager registry async execution dispatch

- `ProviderExecutorRegistration` now composes three independent capabilities:
  required credential refresh, optional async model execution and optional
  execution-session close. No runtime trait downcast or monolithic capability
  requirement is reintroduced.
- `with_execution` validates the normalized async executor identifier against
  the registration provider before publication. A Claude executor cannot be
  inserted under the Codex key even when both are otherwise valid trait
  objects.
- `ProviderExecutorRegistry` exposes typed async dispatch for unary execution,
  streaming execution, token count and credential-injected HTTP request. It
  distinguishes unknown provider from a registered refresh-only provider and
  preserves the provider error as typed data while redacting routine
  Debug/Display output.
- Capability identity now includes execution Arc identity. Replacing only the
  execution implementation is a real replacement and closes all sessions on
  the displaced registration after releasing the registry lock; rebuilding a
  wrapper around the same capability Arcs remains idempotent.
- Three new tests extend the adapted executor-replacement mirror: all four
  dispatch paths, mismatch/missing/unknown rejection, and execution-only
  replacement cleanup with the same refresh/session owner.
- Full gates pass 519 no-default and 543 default unit tests plus 31 integrations
  in each matrix, both warning-denied Clippy matrices, package formatting and
  the pinned Go PluginAPI/Auth packages.
- Mirror Closure remains 166/605; 439 production and 357 test scaffolds remain.

Forensic findings after worker 18bs:

1. **Provider identity must be checked at capability binding.** Waiting until
   request dispatch would make misregistration a runtime routing bug rather
   than a rejected configuration transition.
2. **Refresh-only registration is legitimate but not executable.** Auto-refresh
   can become ready before a provider transport is installed. The registry
   reports `ExecutionUnavailable` instead of conflating that state with an
   unknown provider.
3. **Execution replacement invalidates retained sessions.** Even with identical
   refresh and closer owners, a new async transport may not understand the old
   websocket/session resources; replacement identity must include it.
4. **Provider errors remain typed but not printable by default.** The registry
   keeps the error Arc for policy handling while preventing arbitrary upstream
   messages from entering routine logs.

Strategy adaptation after worker 18bs:

- Implement a built-in adapter for the already active Claude subscription
  executor first. Map the typed plugin request into the existing Claude request
  envelope and response/stream types without serializing secret handles; gate
  unary and streaming execution before adding token-count/HTTP support or the
  Codex and Antigravity adapters.

### Workers 18bt–18ca — First parallel mirror-closure wave

- The active Claude subscription pool now implements the plugin
  `ProviderExecutor` boundary for auth-exact unary and streaming execution.
  Selection validates the requested Auth ID against the configured pool and
  never silently fails over to another subscription. Stream bootstrap remains
  bounded and cancel-on-drop; post-bootstrap transport failures are terminal
  typed events. Native Claude token counting is active locally, while the
  credential-injected generic HTTP bridge remains explicitly unsupported
  instead of returning a fabricated response.
- Claude OAuth error/PKCE leaves, Codex OAuth error/template leaves, provider
  header helpers, SDK access errors/types/registry/manager, misc OAuth/header/
  Claude-instruction helpers, the provider-neutral Thinking core and the SDK
  proxy utility are now compiled APIs. Adapted boundaries replace Go globals,
  ambient proxy inheritance and cleartext token-file ownership with injected
  registries, typed configuration and CTOX secret owners.
- `sdk::access::Manager` retains provider order, nil slots, stable snapshots,
  mutable request handoff and invalid-credential priority. Provider futures
  run after the manager lock is released. The upstream `missing` flag is not
  retained because both its true and false terminal branches return the same
  no-credentials error and therefore have no observable Rust result.
- `sdk::proxyutil` provides real async direct, HTTP(S) CONNECT and SOCKS5(H)
  dialers with WebPKI TLS, remote DNS, buffered tunnel bytes, cancel-on-drop,
  credential-safe errors and strict target/header bounds. CTOX deliberately
  maps upstream `inherit` to an environment-independent direct route; runtime
  proxy authority remains typed configuration rather than process variables.
- Thinking activates types, errors, text, suffix and summary behavior for
  OpenAI Chat/Responses, Codex, Claude, Gemini, Antigravity and Interactions.
  `validate.rs` remains an honest scaffold until its conversion dependency is
  ported; provider-specific `apply.rs` implementations remain outside this
  wave.
- Direct evidence is green: Claude adapter 4 tests, Claude token helpers 7,
  Claude OAuth/PKCE 8, Util 12, SDK Access registry 13 plus manager 5, Misc 12,
  Thinking 16 in both feature matrices, Codex Auth 21 and Proxyutil 13. The
  corresponding pinned Go package tests are green. The common crate-wide gate
  is intentionally deferred until the immediately following parallel leaf
  wave stops editing the shared module graph.
- Mirror Closure is 189/605 classified production files; 416 production and
  352 upstream-test scaffolds remain. The standalone dashboard was regenerated
  from these source headers and no longer presents semantic-scope 100% as
  repository completion.

Forensic findings after workers 18bt–18ca:

1. **Exact Auth dispatch and pool failover are different contracts.** The
   manager registry already chose an Auth. Re-running account selection inside
   a provider adapter could execute with a different subscription than the
   persisted decision and is therefore rejected.
2. **Token counting can close before generic HTTP authority.** Native Claude
   request validation and local counting have no credential or host-policy
   ambiguity. The generic injected HTTP method does, so unsupported is safer
   than an incomplete bridge.
3. **Parallel leaves need an atomic module-graph owner.** Workers own disjoint
   implementation files, while Root owns `mod.rs`, manifests, common gates and
   tracking. Transient compilation gaps during edits are not completion
   evidence and are excluded from worker claims.
4. **Go environment inheritance is incompatible with CTOX runtime authority.**
   An empty proxy setting remains distinguishable as `inherit`, but execution
   is direct until typed configuration supplies a proxy.
5. **Mechanical status must follow observable completeness.** Exact constants
   and pure DTOs can be `ported`; async Rust I/O, injected authorities and
   security-hardened renderers are `adapted_to_ctox`; dependency-blocked files
   remain scaffolds even when adjacent helpers are active.

Strategy adaptation after workers 18bt–18ca:

- Keep all three worker slots saturated with bounded dependency-coherent leaf
  clusters, then stop at short integration waves for a single Root-owned fmt,
  test and warning-denied Clippy matrix. The next active wave ports Thinking
  conversion/validation, Claude Auth data/template leaves and small Util proxy/
  image/tool-result helpers. After that gate, prefer completing already-partial
  provider adapters before opening new top-level runtime authorities.

### Workers 18cb–18ci — Conversion, callback and support leaves

- Thinking conversion and validation now carry the full level↔budget matrix,
  open model capability representation, suffix clamping, auto midrange,
  same-family strictness and cross-family conversion. Strip plus the OpenAI and
  xAI appliers share one JSON path-mutation owner. The canonical `ModelInfo`
  gained an explicit `user_defined` bit so upstream's nil/user-defined OpenAI
  compatibility does not become a second registry.
- Claude Auth now has redacted aggregate/token-storage/template leaves and a
  bounded concurrent Tokio OAuth callback server. It binds only loopback,
  checks hashed state constant-time at the callback boundary, retains
  first-result-wins and drains at shutdown. A real slowloris counterprobe proves
  that one partial request cannot block a valid callback.
- Util closes the canonical async proxy bridge, white-image generation,
  Claude tool-result image extraction, Claude schema normalization,
  attribution and all three Gemini schema modes. Go byte-order guarantees are
  claimed only where the port preserves raw JSON; the `serde_json::Value`
  Gemini API is deliberately `adapted`, not `ported`.
- Misc closes credential metadata merge and template copy without introducing
  stdout/logrus or plaintext token-file authority. Copy preserves upstream's
  open-before-mkdir mutation order, Unix 0700/0600 creation and final fsync.
  Build metadata is explicitly injected instead of linker-mutated global
  variables; provider constants and config defaults are exact literal ports.
- Direct worker evidence is green: Thinking Convert/Validate 30, Thinking
  Strip/OpenAI/xAI 44 in both matrices, Claude Auth leaves 17, Claude OAuth
  server 8 including slowloris, Util proxy/image/tool-result 34, Claude
  schema/attribution 15, Gemini schema 17, Misc credentials/template copy 6,
  and Buildinfo/constants/defaults 3. Corresponding pinned Go packages pass.
- The settled common gate passes 722 no-default and 751 default unit tests plus
  every integration group, both warning-denied Clippy matrices and package
  formatting. The combined pinned Go reference run covers Executor/Helps,
  Claude/Codex Auth, Thinking, Util, Misc, Config, Buildinfo, Constants,
  SDK Access/Proxyutil/PluginAPI and is green.
- Package-scoped `cargo clean -p ctox-cliproxyapi` removed 14.1 GiB of solely
  regenerable artifacts after the Data volume reached 117 MiB free. All
  following targets and Go temp files live under `/Volumes/tmp`.
- Verified Mirror Closure is 207/605, with 398 production and 347 test
  scaffolds open. Claude `anthropic_auth`, Thinking Codex/Gemini and Util Core
  are now included in the settled gate rather than merely classified headers.

Forensic findings after workers 18cb–18ci:

1. **Concurrency is part of OAuth-server fidelity.** A sequential bounded
   listener looked safe but was behaviorally narrower than Go's HTTP server;
   the slowloris counterprobe forced bounded concurrent handlers.
2. **Registry nil semantics need a canonical field, not a provider exception.**
   OpenAI treats missing or explicitly user-defined models differently from a
   registered unsupported model. `ModelInfo.user_defined` makes that decision
   testable at the authority that owns it.
3. **Decoded equivalence is not byte identity.** Rust and Go PNG encoders may
   emit different compressed streams for identical RGBA pixels; Value-based
   schema mutation may reorder objects. Both remain adapted until a wire
   contract demands raw-byte preservation.
4. **Filesystem mutation order is observable.** Opening the source before
   creating the destination parent ensures a missing template leaves no empty
   directory behind, exactly as upstream.
5. **Common gates must run on a settled module graph.** Parallel compilation is
   useful evidence for each leaf, but only a Root-owned rerun after worker
   completion can certify the integrated wave.

Strategy adaptation after workers 18cb–18ci:

- Continue with dependency-coherent Util and provider clusters, keeping build
  output off the constrained Data volume. Complete the remaining Thinking
  provider appliers and top-level registry before opening another runtime
  authority. Never count a changed status header as verified completion before
  its worker evidence and common integration gate land.

### Worker 18cj — Upstream automation, XAI auth and module-closure correction

- Upstream maintenance is now a staged, machine-checked operation. A candidate
  commit produces a non-mutating file delta, affected module assignments and a
  review manifest. Promotion refuses incomplete per-file dispositions, missing
  Rust/Go evidence, red feature/Clippy/tracking gates or a mismatched checkout;
  only a successful promotion changes the accepted pin and source anchors.
- XAI authentication is active end-to-end across internal and SDK boundaries:
  device discovery/polling including `slow_down`, refresh coalescing, proxy
  handling, typed provider errors and the final Auth record. Transport, clock,
  presenter, handle factory and secret store are instance-injected. Access and
  refresh tokens never enter public metadata or a plaintext token file.
- The Common translator module is now genuinely closed. Porting its three
  pinned test mirrors exposed that `SetRawArrayItems` had been labelled ported
  while accepting only top-level keys and reserializing JSON. It now preserves
  surrounding bytes, raw item payloads and dotted paths such as
  `request.contents`; Go nil-versus-empty allocation is explicit as `Option`.
- Evidence is green for XAI Auth (11 Rust tests and both pinned Go packages)
  and Common translation (11 Rust tests and pinned Go package). The prior
  settled default-feature common run passed 918 unit tests plus all 31
  integration tests. The current XAI Executor and Antigravity Auth/cache wave
  remains outside settled-gate credit until its shared Root gate completes.
- Module tracking now splits provider Auth, SDK Auth and Runtime Executor
  clusters instead of treating each large parent directory as one unit. The
  current map contains 128 dependency-coherent modules; a module closes only
  when production and test mirrors have no scaffold, partial or missing file.

Forensic findings after worker 18cj:

1. **A `ported` header is not behavioral proof.** Activating the previously
   scaffolded Common tests found a production-path fidelity defect that file
   counting alone had hidden.
2. **Raw JSON mutation is a wire contract.** Decoded equality is insufficient
   when tool arguments, field order or no-op byte identity are observable by a
   downstream harness.
3. **Provider-sized modules are the useful update unit.** A 140-file Executor
   directory cannot express that XAI is closed while Codex or Gemini remains
   open; upstream delta ownership and dashboard closure must use the same
   provider-aware grouping.
4. **Candidate detection and baseline promotion are different authorities.**
   Automated scans may be frequent, but the accepted upstream commit changes
   only after reproducible port and test evidence.

Strategy adaptation after worker 18cj:

- Continue provider-vertical waves: Auth plus SDK wrapper, then HTTP/SSE/
  WebSocket Executor and format translators. Close their pinned test mirrors
  in the same work unit. Keep the module graph compilable between parallel
  edits and assign Root sole ownership of shared registration, common gates,
  tracking and dashboard regeneration.

### Workers 18ck–18cn — Provider translators, SDK auth and upstream-delta v2

- XAI is closed across subscription Auth and its HTTP/SSE/WebSocket executor.
  Antigravity now closes all 17 production and 14 test mirrors, including
  Interactions, Chat, Responses and request-local identity/time state. Codex ↔
  Gemini closes request/response media, tools, service tier, thinking,
  streaming and registry behavior with both feature matrices and pinned Go.
- SDK Claude and Codex login are active without process-owned listeners,
  environment authority or plaintext token files. Callback sessions,
  presentation, state, clocks, secret stores and handle factories are injected.
  Codex browser and device flows share the active `codex_device` contract.
  Until the internal Codex authorization-code exchange lands, that single
  authority remains an explicit injected service rather than hidden SDK HTTP.
- Vertex Auth is no longer a scaffold. PKCS#1 and RSA PKCS#8 service-account
  keys normalize to LF `RSA PRIVATE KEY` PEM, tolerate upstream's reconstructable
  ANSI/noise forms and return the untouched secret bytes on normalization
  failure through a redacted, zeroizing failure object. Persistence ignores
  the legacy path authority and writes only through an injected secret store.
- Codex JWT parsing now covers the complete upstream claim shape and padded or
  unpadded base64url. PKCE is complete. The shared Claude signature inspector
  gained full E/R tree and CAIS envelope inspection plus indexed payload
  validation. A Gemini differential immediately caught that provider detection
  must request `Strict:true`: default Claude validation is intentionally
  shallow upstream, but using it as a classifier steals E-prefixed Gemini
  envelopes. Provider classification and Antigravity replay now use strict
  validation explicitly.
- Upstream delta schema v2 scans the full relevant repository rather than only
  `*.go`. Dependency manifests, build/release files, runtime assets,
  documentation and licenses receive typed actions and impacted modules. A
  synthetic candidate proved a README rename is classified as non-Go review
  and a Go rename remains mapped to its Rust provider module. Candidate scans
  still never mutate the accepted pin.
- Current verified tracking is 229/605 strictly closed production mirrors and
  115/418 strictly closed test mirrors; partial files remain open. The current
  full graph passes both warning-denied Clippy matrices. The previous settled
  full unit/integration gate remains 977 no-default / 1011 default; a new full
  gate is intentionally deferred until the active Gemini and top-level
  translator workers settle.

Forensic findings after workers 18ck–18cn:

1. **Validation defaults are not classification policy.** A permissive default
   may preserve forward compatibility while provider detection must use the
   strongest structural discriminator explicitly.
2. **Failure return values can contain secrets.** Go's `(original, error)`
   service-account contract must become a redacted, zeroizing Rust error owner,
   not an error that silently discards the caller's recoverable bytes.
3. **Upstream drift is not limited to source extensions.** A `go.mod`, release
   workflow, embedded asset or license change can alter the Rust port even when
   no Go file changes; update automation must classify the repository boundary.
4. **Parallel workers require a compilable shared graph.** A partial move in a
   newly activated SDK file blocked unrelated translator Clippy runs. Workers
   now restore graph-green state before continuing and Root alone regenerates
   tracking after final evidence.

Strategy adaptation after workers 18ck–18cn:

- Finish the active Gemini and top-level translator modules, then run one
  settled full test/Clippy/Go gate and regenerate the dashboard. The next Auth
  unit closes internal Codex code exchange so the SDK's injected temporary gap
  can gain a concrete adapter. Continue assigning provider-sized modules, while
  Root audits shared signature/registry changes and owns all status credit.

### Upstream maintenance hardening after worker 18cn

- Candidate preparation is now one idempotent command keyed by the immutable
  candidate commit. It emits a full-repository delta, fail-closed review ledger
  and module-grouped impact summary into ignored runtime state without changing
  the accepted pin or Rust sources.
- Candidate preparation is resume-safe. It first builds the delta, empty review
  and impact summary beside the destination, then validates every existing
  artifact before publishing a missing one atomically. An existing review with
  the same base commit, candidate commit and ordered change identity is retained
  byte-for-byte, including dispositions and recorded gate evidence. A different
  candidate, altered delta or inconsistent summary/review aborts without
  replacing any artifact. Operators must choose a new output directory or
  explicitly reconcile the mismatch; preparation never guesses which review is
  authoritative.
  The focused regression gate is:

  ```sh
  bash src/core/execution/cliproxyapi/scripts/tests/prepare_upstream_candidate_test.sh
  ```

  It uses the configured local upstream checkout only as immutable Git input
  and writes every candidate workspace beneath a temporary directory.
- The review validator compares module, change kind, source kind and required
  action exactly with the generated delta. Its ten required gates must be
  present by name and true; omitting a gate or relabeling a Go change as
  documentation is rejected.
- Pin promotion snapshots every affected Rust anchor, pin, project state,
  generated map and dashboard before mutation. Anchor and tracking gates run
  after regeneration; any failure restores the previous accepted baseline.
- Smoke evidence covers a clean candidate, a real historical nonempty delta,
  a complete review, rejection of a missing dashboard gate and rejection of a
  retyped source change. Shell syntax checks cover all workflow commands.

Forensic finding: automation is safe only when detection and acceptance have
different authority. Candidate discovery may be scheduled and repeated, but a
pin advance remains conditioned on immutable impact identity, explicit file
dispositions and reproducible Go/Rust/build/tracking evidence. Fully automatic
merging would erase the forensic decision point required for intentional CTOX
adaptations and is therefore not part of the maintenance design.

### Worker 18co — Native Interactions and Kimi thinking

- The final two provider-thinking scaffolds are active. Interactions removes
  all snake/camel aliases across both generation-config carriers, normalizes
  level requests against model capability, converts budgets and restores only
  explicit summary intent. An explicit `none` with no fallback cannot
  accidentally reactivate default-on thinking through a summary flag.
- Kimi emits only its native `thinking.type`/`thinking.effort` object, removes
  the legacy `reasoning_effort`, preserves unrelated fields when enabling and
  clears the entire previous thinking object when disabling. Registered models
  without thinking support remain byte-identical; user-defined models retain
  the upstream compatibility path.
- Six focused Rust cases pass in both feature matrices, and the complete pinned
  Go `internal/thinking/...` package family is green. The shared no-default
  strict Clippy graph is green after activation.

Forensic finding: thinking amount and summary visibility are independent wire
dimensions. Treating `none` as merely another level can silently restore a
summary flag and turn reasoning back on; provider appliers must define the
disabled-state mutation before reapplying visibility metadata.

### Worker 18cp — Codex client model catalog

- Codex client catalog validation now enforces the complete upstream serving
  contract: nonempty/unique slugs, required strings, integral bounded windows
  and priority, default-level membership, unique reasoning efforts and the
  mandatory `gpt-5.5` template.
- The package-global byte store and updater goroutine are replaced by an
  injected `CodexClientModelsStore` plus an async bounded source interface.
  Snapshots are copied, revisions change only with validated content, invalid
  refreshes retain the last valid snapshot and source fallback is explicit.
- The default-feature transport performs real HTTP GET through a no-environment
  wreq client, follows no redirects, has bounded connect/request timeouts and
  stops streaming at 8 MiB. CTOX remains the owner of the three-hour schedule
  and cancellation rather than hiding a second process-global scheduler.
- The `cliproxy-validate-codex-models` binary ports the upstream validation CLI.
  It validated the pinned 291,862-byte upstream catalog. Three no-default and
  four default Rust tests, a real loopback HTTP fetch, the pinned Go registry
  package, both strict Clippy matrices and formatting are green.

Forensic finding: replacing a global updater with dependency injection must not
remove the concrete upstream capability. The source contract therefore has a
real bounded HTTP implementation and canonical fallback URLs; only recurrence
ownership moves into CTOX.

### Upstream watch operationalization

- A fresh audit fetch found candidate `a88197f845c979132c8978ea223c6af05cc81536`.
  Relative to the accepted pin it changes 74 repository files: 47 Go
  production files, 22 Go tests, three documentation/license files, one runtime
  asset and one other file across twenty impacted modules. The large
  topological commit count comes from merged branch history; nine first-parent
  commits and the file delta are the useful review dimensions.
- The candidate is prepared under
  `runtime/cliproxyapi-upstream-reviews/<commit>/` with an immutable delta,
  fail-closed review ledger and module impact summary. The accepted pin remains
  `ffdb9c9fbc78a6235d59c9ccbdc4243ba35ecdcd` while the current port wave
  settles.
- `.github/workflows/cliproxyapi-upstream-watch.yml` now runs the same read-only
  preparation daily or manually from a fresh clone and uploads the
  commit-addressed review artifact for 30 days. It has only `contents: read`;
  no workflow path can promote the pin or modify port sources.

Forensic finding: commit distance is a poor churn metric after upstream merges
an old feature branch. Promotion planning uses changed files, affected modules
and first-parent integration commits; raw topological ancestry remains
recorded but cannot inflate or minimize the actual porting scope.

### Worker 18cq — Side-effect-free native browser launcher

- `internal/browser/browser.go` is active as an `adapted_to_ctox` Rust mirror.
  macOS, Windows and the complete ordered Linux launcher list are preserved;
  URLs remain one process argument and never cross a shell boundary.
- Command discovery validates executable regular files. Platform metadata
  retains upstream's public `darwin` value and Linux command ordering through
  a typed serializable structure.
- Upstream's `IsAvailable` opens `about:blank` as a probe. CTOX instead performs
  the same command-capability check without launching a browser, so metadata
  and readiness requests cannot create UI side effects.
- Six injected-platform tests cover Linux priority, Windows arguments, URL
  argument integrity, metadata, unsupported platforms and executable lookup.
  The no-default shared strict Clippy gate reached the live Codex executor wave
  after this module and is therefore held until that worker's newly wired
  identity path settles; the browser-local tests themselves are green.

Forensic finding: platform parity does not require reproducing an availability
probe's externally visible side effect. The durable contract is launcher
selection and error behavior; capability inspection must remain observational.

### Worker 18cr — Bounded injected HTTP fetch

- `internal/httpfetch/httpfetch.go` and its upstream test mirror are fully
  active. The Rust boundary keeps an injected async `HttpDoer`, validates the
  URL before transport, forwards nonempty headers, accepts only 2xx statuses
  and distinguishes construction, transport, body-read, status and size
  failures.
- Success bodies enforce positive limits as chunks arrive. Non-success bodies
  retain upstream's independently truncated 4 KiB diagnostic detail without
  converting truncation into a size-limit error.
- The default transport matrix exposes a concrete streaming Wreq adapter; the
  no-default matrix retains the complete transport-neutral contract for host
  injection and deterministic tests.
- Five Rust tests pass in both feature matrices and the three pinned Go tests
  pass. The shared strict no-default Clippy graph reaches only the live Codex
  executor worker's typed-parameter refactor, so final shared credit remains
  deferred until that atomic patch settles.

Forensic finding: a post-hoc `Vec` length check is not a bounded fetch. The
body abstraction must expose chunks so the port can stop accepting bytes at
the same boundary where upstream's `LimitReader(max+1)` would reject them.

### Worker 18cs — Config access and provider reconciliation

- Both `internal/access` production mirrors are active against the already
  completed SDK access boundary. Inline configuration keys preserve carrier
  priority across Authorization, Google, Anthropic and both query parameters;
  missing, invalid and not-handled outcomes remain distinct.
- Keys are normalized, deduplicated, zeroized on drop and compared in constant
  time. CTOX deliberately replaces upstream's raw-secret `Principal` with a
  stable truncated SHA-256 identifier so authentication evidence and logs do
  not become a credential disclosure path.
- Reconciliation preserves registry order, compares provider instance identity,
  sorts added/updated/removed lists and excludes the implicit inline provider
  from external change notifications. Five direct tests pass in both feature
  matrices and both shared strict Clippy matrices are green at this settled
  point.

Forensic finding: API-key authentication parity and API-key observability are
separate contracts. Carrier recognition must remain compatible; emitting the
matched secret as an identity is neither required for routing nor acceptable
at CTOX's durable evidence boundary.

### Worker 18ct — Full OpenAI→Gemini translator

- The rejected minimal leaf was replaced by all five pinned mirrors and 1,465
  Rust lines. Request conversion covers generation configuration, system
  instructions, media, tools, tool choice and FIFO tool-response binding;
  response conversion covers fragmented tool arguments, reasoning/content
  ordering, finish/usage timing, non-stream overwrite behavior and token count.
- Tool IDs are deterministic request-local SHA-256 identities rather than a
  global clock/random authority. Model names remain registry-bound and no
  Atomics, environment state or process-global mutable translator state exist.
- Fifteen tests pass in both matrices, both shared strict Clippy gates pass,
  the pinned Go package is green and six direct Go/Rust fixtures are parity
  exact. Root activated the module and its explicit registry direction.

Forensic finding: the isolated differential found a missing
`promptTokensDetails` field even though ordinary token-count tests passed.
Shape parity must compare the complete provider envelope, not only totals.

### Worker 18cu — Full OpenAI→Claude translator

- All five pinned OpenAI/Claude mirrors are fully active. Fifty-four tests
  cover request roles, signed/unsigned thinking, attribution, schemas, tool
  results and images plus streaming/non-stream block order, late IDs, usage,
  finish reasons, name restoration and token count in both feature matrices.
- The pinned Go package and five direct Go/Rust differential fixtures pass.
  Both shared strict Clippy matrices are green.
- Root integration caught a directional registration defect: this leaf owns
  Claude→OpenAI request plus OpenAI→Claude response, while the existing
  `claude/openai/chat-completions` leaf owns the opposite pair. Explicit
  two-direction registry assertions now prevent either registration from
  overwriting the other.

Forensic finding: directory names describe the upstream package location, not
necessarily the request transform direction. Registration must be derived
from function contracts and proved in both directions after all `init()`
replacements run together.

### Worker 18cv — Registry direction and benchmark mirrors

- The two top-level translator benchmark mirrors are active as bounded manual
  benchmarks plus ordinary request/response route smokes. Their activation
  exposed a real registry defect: Rust stored a `register_pair` response under
  `(provider, client)` and then reversed the lookup again during response
  dispatch, while pinned Go stores request and response under the same
  `(client, provider)` key and reverses exactly once at call time.
- `Registry::register_pair`, every explicit registration assertion and the
  affected facade/integration calls now follow that single-reversal contract.
  The settled no-default matrix passes 317 translator tests plus ten registry
  integrations; the default matrix passes 336 translator tests plus the same
  ten integrations. Both strict library Clippy matrices, formatting and the
  complete pinned Go translator package tree are green.

Forensic finding: a unit test that proves only that both direction keys exist
cannot detect a double reversal when symmetric pairs overwrite one another.
Route smokes must inspect the translated provider envelope, and response calls
must name their provider/client roles rather than relying on ambiguous
`from`/`to` intuition.

### Worker 18cw — Complete Codex executor cluster

- All 33 pinned `internal/runtime/executor/codex*.go` mirrors are closed: 16
  production files and 17 tests. The final three production gaps now cover the
  full request, execution and facade paths instead of leaving their previously
  partial bodies behind.
- Direct image execution accepts bounded JSON and multipart requests, native
  stream completion and provider-native result shapes. Reasoning replay is
  request/session scoped and committed only from terminal evidence. The live
  WebSocket path uses bounded channels, reconnects only before commit and
  releases executor-owned sessions deterministically.
- Identity/cloaking uses deterministic UUID-v5-style request-local identities
  with a typed opt-out; no process-global random or ambient environment source
  participates. Root re-ran 52 no-default and 54 default Codex tests, the
  pinned Go Codex executor selection, both strict library Clippy matrices and
  formatting successfully.

Forensic finding: transport parity is not proved by constructing the right
request body. Image multipart limits, stream commit boundaries, reconnect
authority and terminal reconstruction are executor semantics and must be
tested through the owning request-scoped state machine.

### Worker 18cx — OpenAI Chat Completions ↔ Interactions

- Four production mirrors and three test mirrors are fully active through the
  shared module graph and `register_all`; both request directions and both
  stream/non-stream response directions are registered explicitly.
- The converters preserve messages, system steps, media/files, tools and tool
  results, generation controls, response formats, reasoning, usage and
  terminal metadata. Synthetic identities and timestamps are deterministic
  and request-local rather than global clock/atomic state.
- Nineteen tests pass in each shared feature matrix. Nine direct Go/Rust
  fixtures and the complete pinned Go package pass, as do both strict library
  Clippy matrices and formatting.

Forensic finding: the first isolated implementation silently dropped
`response_format`, emitted an empty `generation_config` and joined text parts
with incorrect newlines. File-complete ports still require bidirectional
fixture comparison; compiling all converter bodies did not reveal any of those
wire-contract defects.

### Worker 18cy — Instance-owned Redis queue substrate

- `internal/redisqueue/queue.go`, `usage_toggle.go` and the queue test mirror
  are active. Usage delivery still broadcasts before buffering, drops a slow
  subscriber without blocking publishers, buffers only when no usage
  subscriber exists, discards unobserved error records, sends the initial
  support-refresh capability and closes both subscriber families on disable.
- Retention keeps upstream's 60-second default and 3,600-second cap. A
  `VecDeque` replaces the Go head/compaction implementation without changing
  FIFO or prune behavior.
- CTOX replaces the package-global queues and atomics with an instance-owned
  `UsageQueue` and `UsageStatisticsSwitch`; one gateway lifecycle therefore
  cannot clear or disable another. Five Rust tests pass in both matrices, the
  complete pinned Go Redisqueue package passes, and both strict library Clippy
  matrices plus formatting are green. The usage-record serializer/plugin
  remains an explicit separate scaffold.

Forensic finding: copying upstream's atomics literally would preserve
single-process behavior but violate multi-instance isolation. Queue ordering,
backpressure and close semantics are the portable contract; ownership belongs
to the typed CTOX runtime that creates the gateway.

### Worker 18cz — Canonical token accounting

- `sdk/cliproxy/usage/accounting.go` and its test mirror are active. The Rust
  contract represents mutually exclusive input/output buckets, schema version,
  quality and unclassified remainder, and validates all parent/child totals.
- Provider classification preserves upstream's four semantics: subset
  (OpenAI/Codex/xAI/Kimi and OpenAI-compatible), independent cache/reasoning
  (Claude), separate reasoning (Gemini/Vertex/Antigravity/Interactions), and
  fail-closed unclassified for unknown plugins. Legacy cached-only usage is
  promoted only where upstream permits it.
- Checked arithmetic rejects negative and overflowing buckets instead of
  relying on release-mode integer behavior. Eleven Rust tests pass in both
  matrices, the full pinned Go Usage package passes, and both strict library
  Clippy matrices are green. The asynchronous Usage manager remains the next
  file in this SDK module.

Forensic finding: totals alone cannot prove usage parity. A numerically correct
sum can still double-count cache or reasoning tokens; the breakdown invariants
and provider-specific overlap model are part of the durable billing contract.

### Worker 18da — OpenAI-compatible executor and Responses signatures

- All five pinned mirrors are active: two production files and three tests.
  The executor covers request/model/payload overrides, Compact, prompt-cache
  identity, Images JSON and multipart rewriting, bounded SSE framing, token
  counting and text-only tool-result normalization. HTTP, registry, config and
  request state are injected rather than ambient.
- The Responses sanitizer preserves byte-identical no-ops, validates encrypted
  GPT reasoning carriers and removes orphan reasoning IDs only when `store` is
  disabled, matching the upstream history-replay policy.
- Twenty-two shared tests pass in each feature matrix; the pinned Go executor
  selection and direct SHA-1 namespace UUID fixture pass. Both raw strict
  library Clippy matrices and formatting are green after Root activation.
  Usage reporting, Home refresh and shared payload/thinking helpers remain
  separately tracked dependencies rather than being claimed through local
  substitutes.

Forensic finding: an executor leaf can be behavior-complete at its injection
boundary while its owning service still has open dependencies. Tracking must
credit the five closed mirrors without treating injected Usage/Home/Thinking
interfaces as evidence that those separate upstream files are already ported.

### Worker 18db — Claude executor, identity and signature cluster

- Fifteen previously open production mirrors and five test mirrors are now
  active across the Claude executor, its account-owned identity/cache helpers
  and the Claude message signature sanitizer. The pre-existing Claude auth
  mirror was revalidated but receives no duplicate closure credit.
- Cloaking, device/session identity, CCH xxHash64 signing, cache TTL/limit,
  OAuth tool-name round trips, request shaping, fragment-safe SSE, compressed
  response decoding, retry/persistence, usage parsing, token counting and the
  authenticated generic HTTP bridge share explicit request/account authority.
  No Gin context, ambient configuration or process-global secret-derived
  identity remains in the cluster.
- Root independently passed 48 no-default and 52 default Claude tests, the
  cache/session/signature helper suites in both matrices, the selected pinned
  Go executor and helps packages, and six direct Go/Rust fixtures. Both strict
  Clippy matrices and package formatting are green. The separate
  `helps/claude_input_tokens.go` production/test pair remains honestly partial.

Forensic finding: a module gate must enumerate both newly closed and merely
revalidated mirrors. Counting every file exercised by a test filter would have
incorrectly credited the already-adapted auth mirror and the still-partial
input-token helper.

### Worker 18dc — Instance-owned usage manager and queue serializer

- `sdk/cliproxy/usage/manager.go` and its test mirror now provide a typed
  `UsageContext`, complete Record/Failure contract, lazy FIFO dispatcher,
  named plugin replacement, deterministic drain-on-stop and panic isolation.
  CTOX deliberately omits the upstream default global manager; the embedding
  gateway owns the instance and its plugin authority.
- `internal/redisqueue/plugin.go` and its test mirror are wired explicitly to
  an instance-owned `UsageQueue` and `UsageStatisticsSwitch`. The serializer
  resolves framework-neutral request metadata, failure status, service tier,
  generated/not-generated semantics and provider-aware canonical token
  accounting while retaining response-header snapshots.
- Six manager and six plugin tests pass in both feature matrices. Both full
  pinned Go reference packages, both strict library Clippy matrices and
  package formatting are green.

Forensic finding: porting the Go `init()` registration or default manager as a
Rust global would make queue lifecycle cross-instance and unauditable. The
portable behavior is ordered dispatch and payload shape; ownership must stay
with the typed CTOX gateway runtime.

### Worker 18dd — Kimi executor and thinking replay leaf

- Both Kimi production mirrors and both dedicated test mirrors are active.
  Model normalization, Kimi auth/device shaping, Claude envelope delegation,
  tool-link repair and credential-/metadata-scoped thinking replay cover
  unary JSON and fragmented SSE paths.
- Root passed 49 Kimi-filtered tests in each feature matrix and the selected
  pinned Go Kimi executor tests. Both raw strict Clippy matrices and package
  formatting are green after shared module activation.
- The Kimi-specific Claude delegate, host-owned cache/clock/device/refresh
  assembly, full payload policy, usage/API logging, Home refresh and exact
  Claude input-token stream accounting remain separate open mirrors and do
  not receive credit through dependency injection at this leaf boundary.

Forensic finding: sharing Claude wire format does not authorize sharing the
Claude account pool. Kimi delegation must preserve Kimi credential ownership;
reusing `ClaudeProviderExecutor` directly would silently route through the
wrong subscription authority.

### Worker 18de — Model configuration hashes and capabilities

- Both production mirrors and the test mirror are active. Five model hash
  families preserve order, trimming, casing, modalities and Go `omitempty`
  behavior; capability lookup strips provider suffixes and applies explicit,
  normalized thinking overrides without inventing data for unknown models.
- Seven tests pass in both feature matrices and the complete pinned Go
  `internal/modelconfig` package is green. The shared raw Clippy and formatting
  gates pass after activation.

Forensic finding: capability absence is information. A partial registry must
leave unknown models unknown rather than treating missing upstream catalog
data as permission to synthesize a default thinking profile.

### Worker 18df — Instance-owned execution lifecycle registry

- Both production mirrors and all three test mirrors are active. Scope begin,
  pending dispatch, install, exactly-once end/close, bounded drain, in-flight
  freezing and release sequence replay retain the upstream lifecycle contract.
- Go channels and cancellation contexts become an instance-owned `Condvar`
  registry with explicit `WaitBudget` and typed acknowledgements. No default
  global registry or environment authority exists.
- Fourteen tests pass in both matrices and fourteen pinned Go reference tests
  pass. Root also re-ran both raw strict Clippy matrices and formatting.

Forensic finding: release sequence allocation and sink lookup require separate
borrow phases in Rust. The first integration compile caught an overlapping
mutable/immutable borrow; narrowing the sequence borrow also makes the
publication snapshot boundary explicit.

### Worker 18dg — Protocol-neutral session root identity

- `sdk/cliproxy/session/identity.go` and its test mirror are active. Explicit
  IDs reject controls and oversized UTF-8, Claude JSON/legacy metadata and all
  supported session headers/body shapes retain their precedence, and request
  payloads are snapshotted before selection.
- Canonical roots cover OpenAI Chat, Claude, Responses/Codex, Gemini and
  Interactions. They hash the bounded instruction prefix and complete first
  user input, including media and recursively normalized cache-control-free
  JSON, under an irreversible caller scope.
- Twelve Rust tests pass in each matrix, the complete pinned Go package passes
  with 8 top-level/35 subtests, and fixed OpenAI/Gemini/caller hash vectors
  prove byte-level canonical parity. Root strict Clippy and formatting pass.

Forensic finding: conversation identity must remain stable as history grows
without collapsing callers or distinct long first-user inputs. Hashing the
whole request is unstable; truncating the user input creates collisions. Only
instructions are prefix-bounded.

### Worker 18dh — Instance-owned home plugin synchronization

- The complete Homeplugins production and test mirrors are active. Sync,
  resolved sync, installed-version projection, artifact selection, delete,
  unload, cancellation, partial-error reporting and auth clearing retain the
  pinned behavior behind instance-owned store, boundary and operation-context
  interfaces.
- Eighteen tests pass in each Rust feature matrix, the complete pinned Go
  package passes, both strict Clippy matrices pass without lint exceptions and
  formatting is clean.

Forensic finding: an independently ported facade can preserve local behavior
while still duplicating its neighboring SDK types. Homeplugins remains
integration-incomplete until an executable adapter binds it to the SDK
Pluginstore authority; the mirror itself receives file credit, not end-to-end
integration credit.

### Worker 18di — Bounded management asset updater

- The production and test mirrors are active with instance-owned throttling,
  scheduling and configuration, injected HTTP/auth/URL policy, typed paths,
  digest and fallback parity, bounded downloads, symlink rejection and atomic
  fsync-plus-rename replacement.
- Eleven tests pass in both feature matrices and the full pinned Go package is
  green. Both strict `--lib --tests` Clippy matrices and formatting pass.

Forensic finding: atomic replacement is only safe after the path and transport
boundaries agree on the same target. Digest validation alone does not prevent
symlink redirection or authorize an ambient download URL.

### Worker 18dj — Plugin ABI test closure

- The remaining PluginABI test mirror now validates raw-result envelope
  roundtrips and every pinned ABI-v2 method name, including scheduler and model
  routing. Three Rust tests pass in both matrices and the complete pinned Go
  package passes.

Forensic finding: protocol constants are executable compatibility surface, not
documentation. A typed ABI still needs exact wire-name tests so a harmless
looking Rust rename cannot silently split host and plugin processes.

### Worker 18dk — Instance-owned model registry and updater

- Three production and four test mirrors close the remaining Registry module:
  the complete owned model catalog, deep defensive snapshots, duplicate and
  provider reconciliation, quota/suspension/cache expiry, handler wire maps,
  builtins, static and dynamic lookup plus bounded fallback refresh and
  changed-provider notification are active.
- Registry, clock, hook, catalog store and updater are instance-owned. The
  no-default matrix passes 24 tests, the default matrix 25 including HTTP, the
  uncached pinned Go package passes, and both strict Clippy matrices plus
  formatting are green.

Forensic finding: removing a Go global requires a live ownership replacement,
not a frozen snapshot. `ModelCatalogStore`, `ModelRegistry::from_store` and
`ModelsUpdater::run` retain dynamic update semantics while the legacy free
management helper remains embedded and cannot silently become a second catalog
authority.

### Worker 18dl — SDK Pluginstore and Home type convergence

- The SDK Pluginstore production and test mirrors are active as the complete
  public typed/helper/client facade. `Client` owns an injected
  `PluginStoreIo`; URL, manifest, artifact, GitHub, platform, version, auth and
  sync validation require no global HTTP client, environment lookup or ambient
  plugin directory.
- Homeplugins now consumes the SDK manifest, sync, auth, install and platform
  types directly. A scoped `SdkPluginStoreAdapter` carries owned auth plus
  expiry through Sync→SDK→Store-I/O and clears the response item afterward.
- `Secret` zeroizes on clear/drop and has a strictly redacted manual Debug;
  nested-auth leak tests search for the actual token. Nine SDK and nineteen
  Home tests pass in both matrices, the pinned Go packages pass, and both shared
  strict Clippy matrices plus formatting are green.

Forensic finding: mirrored modules are not integrated merely because their
independent tests pass. Type convergence removed a duplicate DTO authority and
the executable adapter proves credential lifetime and install options across
the boundary. Concrete HTTP/ZIP/filesystem effects remain exclusively owned by
the separately gated internal Pluginstore implementation.

### Worker 18dm — Safe internal Pluginstore I/O

- Eight production and five test mirrors close the internal Pluginstore behind
  `SafePluginStoreIo`. Registry/auth/direct/GitHub/manifest/sync/install paths
  stream bounded bodies, reapply scoped auth after every redirect, hide
  authenticated failure bodies, zeroize transport headers and verify SHA-256
  before any filesystem mutation.
- ZIP extraction rejects traversal and unsafe entries, the plugin root rejects
  symlinks, and verified installs use fsync plus atomic rename on Unix or
  `MoveFileExW` on Windows. Transport, URL policy and clock are injected; there
  is no environment credential path, cgo or hidden async `block_on`.
- Twelve tests pass in both matrices, the complete pinned Go package passes,
  strict Clippy including tests and formatting are green, and a Windows GNU
  cross-check plus real TCP/filesystem end-to-end test cover platform effects.

Forensic finding: checking a digest after buffering an unrestricted response is
not a size gate. The accepted path streams the socket body directly through the
bound and checksum, then validates archive structure before entering the atomic
write boundary. Redirects are new authority decisions and must select auth
again rather than forwarding the previous request header.

### Worker 18dn — Provider signature validation and compatibility closure

- Four production and five test mirrors close the remaining `internal/signature`
  surface. Claude, Gemini and GPT validators distinguish missing, null, empty
  and duplicate canonical fields without first collapsing the raw JSON shape;
  provider compatibility and foreign-envelope rejection are active in the
  normal module graph.
- Forty-eight Rust tests pass in both feature matrices, the pinned Go package
  passes, and the Responses, Antigravity replay/accumulator, Claude carrier and
  Claude request differential corpora remain green. Both strict Clippy
  matrices pass.

Forensic finding: model naming is insufficient provenance. A valid native
signature must also belong to the expected provider envelope, and duplicate
canonical fields cannot be safely validated through a generic object map that
silently keeps only one occurrence.

### Worker 18do — Executor payload regressions and built-in translator

- The SDK built-in facade now constructs an independently owned, fully
  registered `Registry` and `Pipeline`; upstream's mutable package global is
  not reproduced. Registry request translation carries summary intent across
  native and plugin translators while preserving normalizer ownership and
  source-shaped fallback bodies.
- Four upstream test mirrors cover owned byte chunks, summary semantics,
  Claude cache breakpoints and Kimi/Codex payload preservation. The multipart
  corpus exposed a real defect: textual `images` fields were overwritten by
  uploaded parts. The parser now appends uploads after existing images.
- Fourteen targeted Rust tests pass in both matrices, the pinned Go SDK and
  executor corpora pass, and both strict Clippy matrices plus Cargo formatting
  are green.

Forensic finding: an optimization test can be protocol evidence. Preserving a
large JSON integer and canonical bytes is correctness, while Go allocation
benchmarks are not Rust unit-test credit. The inactive Aistudio colon-spacing
helper remains with its production mirror instead of being invented as an
orphan function.

### Worker 18dp — Instance-owned Antigravity version lifecycle

- The Antigravity Hub version cache, manifest URL and refresh lifecycle are
  instance-owned. A typed transport carries the exact URL, electron-builder
  user agent, no-cache policy, ten-second timeout and 4,096-byte response bound;
  a supplied clock controls the six-hour cache lifetime.
- Refresh failure preserves a fresh value and restores the pinned fallback
  after expiry. Hub, legacy, loadCodeAssist and onboard user-agent forms match
  the upstream corpus. The supervisor owns shutdown of the periodic updater;
  no `sync.Once`-equivalent global is introduced, and Debug redacts the URL.
- Four Rust tests pass in both matrices, the pinned Go misc corpus passes, and
  both strict Clippy matrices plus formatting are green.

Forensic finding: moving an upstream global cache verbatim would transfer
lifecycle authority into a utility module. Explicit ownership makes multiple
gateway instances independent and lets CTOX supervise cancellation without
changing the wire-visible manifest or user-agent contracts.

### Worker 18dq — Instance-owned Thinking orchestration

- `internal/thinking/apply.go` and both upstream test mirrors are active. The
  Rust boundary uses an instance-owned `ThinkingEngine`, an injected
  `ModelInfoResolver`, and isolated native/plugin provider registration with
  the upstream priority and owner tie-break instead of Go package globals.
- Apply, request extraction, validation, summary handling, configured API-key
  behavior and the Kimi maximum-budget clamp are covered. Rust's type system
  excludes a nil applier, and errors leave the caller-owned input unchanged.
- Eighty Thinking tests pass in both Rust feature matrices, the pinned Go
  Thinking packages pass, and both strict Clippy matrices plus formatting are
  green. This closes one production and two upstream-test mirrors, bringing
  strict closure to 321/605 production and 189/418 tests.

Forensic finding: upstream-followability does not require preserving mutable
package authority. The owned engine keeps registration deterministic and
multi-instance safe while retaining the observable provider selection and
wire behavior that future upstream deltas must revalidate.

### Worker 18dr — Transactional object and Postgres stores

- Object, Postgres auth and Postgres cooldown mirrors are active behind
  injected transactional CTOX backends. Spool paths are absolute and
  symlink/traversal guarded; writes use private permissions, random temporary
  files, fsync and atomic replacement. Cooldown deletion uses CAS tombstones
  so a stale transaction cannot erase a newer state.
- Nine Store tests pass in both Rust matrices, the complete pinned Go Store
  package passes, and both strict Clippy matrices plus formatting are green.
  This closes three production and one upstream-test mirror.
- `gitstore.rs` and `gitstore_test.rs` remain partial without credit. Local
  remotes, managed-path commits and conflict/deletion guards work, but safe
  HTTPS/SSH credentials and full corruption, packfile, GC and concurrent-empty
  recovery parity require an isolated LocalTransport Git helper with its own
  TLS graph and typed secret IPC.

Forensic finding: disabling `git2` default TLS features fixes the concrete
OpenSSL/BoringSSL link collision but is not feature parity. The partial status
prevents a successful local-only Git subset from hiding the missing remote and
recovery behavior.

### Worker 18ds — Bounded authority-injected WebSocket relay

- All four `internal/wsrelay` production mirrors are active. CTOX injects
  relay authority, transport, clock and cancellation and hands in an already
  authenticated typed WebSocket stream; the relay owns no listener or ambient
  configuration.
- Bounded queues turn saturation into an explicit terminal error instead of
  silently dropping upstream frames. Shutdown waits for pending sessions, and
  replacement, cancellation, heartbeat and read-timeout causes remain
  distinguishable.
- Ten supplemental Rust tests pass in both matrices, including real duplex
  loopback, frame fragmentation, cancellation and backpressure. The pinned Go
  package and AIStudio TTFT loopback consumer pass, as do both strict Clippy
  matrices and formatting. Upstream contains no dedicated wsrelay test files,
  so no test-mirror credit is invented.

Forensic finding: the default-feature matrix exposed a cleanup race between
the cancellation watcher and pending drain. Fixing it before host wiring keeps
the relay lifecycle deterministic; AIStudio executor and HTTP host activation
remain separate, visible scopes.

### Worker 18dt — Injected SDK provider registry and RoundTripper policy

- The SDK model registry, provider helpers and runtime provider transport are
  active. Upstream globals become an injected `Arc` registry and typed API-key
  client builder; direct/proxy clients are cached per instance without ambient
  proxy environment inheritance.
- Proxy failures cross the boundary as a credential-safe error kind rather
  than raw transport diagnostics. Eight targeted tests pass in both Rust
  matrices, the pinned Go RoundTripper reference passes, and both strict Clippy
  matrices plus formatting are green.
- Service activation remains explicitly assigned to the open Builder/Service
  cluster; these capabilities do not self-register or acquire lifecycle
  authority.

Forensic finding: provider discovery and transport construction can retain the
upstream API while removing singleton authority. That makes parallel gateway
instances and future upstream revalidation deterministic.

### Worker 18du — Byte-preserving Codex input item IDs

- Codex message IDs receive the upstream `msg_` normalization, overlong IDs
  use the same SHA-256 suffix and deterministic collision attempts, and only
  overlong reasoning items with non-empty encrypted content are removed.
- Unicode character count, rather than UTF-8 byte length, enforces the 64
  character limit. The implementation surgically replaces only the top-level
  input array, preserving the outer payload and unchanged item bytes, including
  large numeric spellings and whitespace.
- The helper is active in both Codex HTTP and WebSocket request preparation.
  Seven direct tests plus the existing HTTP/WS regression suites pass in both
  matrices, the complete pinned Go helps package passes, and shared strict
  Clippy plus formatting are green.

Forensic finding: the former HTTP-only duplicate sanitizer produced different
IDs for repeated identical overlong values. Routing both transports through
the mirrored helper restores the upstream mapping cache and prevents HTTP/WS
format drift.

### Worker 18dv — Instance-owned SDK management and ordered options

- The SDK management and options mirrors are active as typed, instance-owned
  capabilities. OAuth state, clock, persistence, endpoint execution and route
  registration are injected; Gin and process-global callback authority do not
  enter the port.
- Provider aliases, plugin callback names, state TTLs, callback file naming,
  JSON and YAML-comment preservation follow upstream. Options execute in
  caller order, nil/zero options are no-ops, and registration stops on error.
- Ten supplemental tests pass in both matrices, `go test ./sdk/api` builds the
  pinned package with no upstream test files, and strict Clippy plus formatting
  are green. No upstream-test credit is invented.

Forensic finding: an API facade can be complete without owning an HTTP server.
The concrete auth HTTP exchange remains injected host authority, preserving
CTOX lifecycle and secret boundaries while retaining the SDK operations.

### Worker 18dw — Derived session, Thinking and payload helpers

- Derived session IDs retain provider-namespaced UUID-v5, execution-session
  preference and Antigravity's negative decimal identifier. Typed execution
  metadata replaces dynamic maps.
- Thinking summary priority, normalizer removal and original-only summaries are
  preserved through injected `ThinkingEngine` and translator registry
  capabilities. String, boolean and raw payload mutations preserve bytes and
  allocations on no-op paths; raw arrays are joined without re-encoding and
  numeric projection follows Go float32 formatting.
- The three source and three test mirrors pass 4+15+3 tests in both matrices;
  the OpenAI-compatible consumers pass 18/18, the pinned Go helps package and
  both strict Clippy matrices plus formatting are green.

Forensic finding: Kimi still uses its specialized Thinking route because the
current translator registry exposes its wire format as OpenAI. The shared
helper is complete; logical Kimi format registration remains visible in its
own integration scope rather than being faked here.

### Worker 18dx — Bounded Session-ID cache and injected Home refresh

- The Session-ID cache retains SHA-256 API-key scoping, UUID-v4 generation,
  one-hour sliding TTL, KV expiry and setNX/read-after-set behavior. It is
  instance-owned, shareable through a durable store, opportunistically purged
  and bounded to 4,096 entries instead of spawning an immortal cleanup global.
- Home refresh retains enabled/handled semantics, heartbeat, auth-index
  fallback, returned overrides, direct/envelope payloads and status mapping.
  Payloads fail closed above 1 MiB and runtime auth authority is retained.
- Claude subscription execution now consumes the injected cache/store. Session
  10/10, Home 9/9 and Claude consumer 10/10 pass in both matrices alongside
  pinned Go, strict Clippy and formatting gates.

Forensic finding: Home remains an exported injected authority because CTOX has
no global Home/Redis configuration. Making it a silent default would create
new ambient runtime ownership rather than port upstream behavior.

### Worker 18dy — Grounding redirects and OpenAI tool results

- Antigravity grounding resolution accepts only the exact Vertex Search HTTPS
  host/path without credentials or alternate ports, uses an injected
  non-following HEAD transport, accepts only 3xx responses and validates a
  credential-free HTTPS target. Duplicate URLs share one lookup and failures
  preserve the original bytes.
- OpenAI-compatible tool results preserve text, replace image parts with the
  upstream marker and retain unknown JSON. Model names take precedence over
  aliases, Thinking suffixes are stripped, and a mixed alias pool remains
  image-capable. The executor now delegates to this single mirrored helper.
- Five grounding, four direct tool-result and two existing consumer tests pass
  in both matrices. The pinned Go helps package, both strict Clippy matrices
  and formatting are green.

Forensic finding: a transport-injected redirect resolver prevents arbitrary
redirect/proxy authority and SSRF-like authority confusion while keeping the
best-effort upstream output contract.

### Worker 18dz — Runnable translator example

- The upstream translator example is now an explicit Cargo example despite
  `autoexamples = false`. It creates an independently owned built-in registry,
  translates the OpenAI chat request to Gemini, then translates the Gemini
  tool-call response back to OpenAI chat.
- A supplemental example test validates request role, response object/model,
  tool-call finish reason and function name in both Rust feature matrices. The
  pinned Go example builds with no test files; example-scoped strict Clippy and
  formatting are green. No upstream-test credit is invented.

Forensic finding: registering the example in Cargo is part of the port. A
translated source file that is never compiled would not prove the public SDK
surface remains usable as upstream evolves.

### Worker 18eb — Bounded retry, proxy and token helpers

- JSON retry extraction preserves nested delete/no-op behavior, Google
  `RetryInfo`/`ErrorInfo` priority, message fallbacks and Go-compatible signed,
  compound duration syntax. Proxy selection follows Auth, config, injected
  transport, then explicit direct fallback without ambient environment proxy.
- Token counting covers the model/tokenizer matrix and OpenAI message, content,
  tool, function and response-format segments. The OpenAI-compatible executor
  now uses the canonical helper and minimal Usage builder rather than a local
  duplicate.
- Retry bodies are bounded to 1 MiB, proxy URLs to 8 KiB and token payload/text
  to 16 MiB. The three production and one test mirrors pass their 3+3+4 tests
  and 18 consumers in both matrices, pinned Go, strict Clippy and formatting.

Forensic finding: JSON and proxy consumers whose parent files remain scaffolds
cannot be claimed as integrated. Their helpers are complete and active; only
the already ported token consumer is wired in this checkpoint.

### Worker 18ec — Fail-visible SDK Builder and Watcher types

- Provider results/loaders, plugin parsers and every Watcher wrapper
  nil/no-op/error path are active. Builder overrides preserve validation,
  contextual credential weight errors, plugin/access ordering, hooks,
  manager/cooldown/RoundTripper choices and server-option order.
- Because `service.go` and `watcher.go` remain separate open mirrors, Build
  returns a typed `ServiceAssembly` with explicit `ServiceBindingRequirement`s
  for missing host authorities. It does not silently start listeners, watchers
  or process globals.
- Seven Builder and four Types tests pass in both matrices; exactly one is the
  upstream weight test. The pinned Go test/package build, both strict Clippy
  matrices and formatting are green.

Forensic finding: a fail-visible assembly is more upstream-followable than a
fake running Service. The later Service/Watcher port has an explicit,
machine-testable materialization contract instead of hidden partial behavior.

### Worker 18ed — Canonical bounded multi-provider Usage accounting

- Usage parsing covers OpenAI Chat/Responses/Codex/Image, independent Claude
  cache accounting, Gemini/Antigravity/Interactions aliases, reasoning,
  service tier, partial/zero/total-only values and raw/SSE forms. Negative and
  overflow values fail closed.
- `UsageReporter` is exactly-once under concurrency and carries latency, TTFT,
  aliases, executor/provider, reasoning, service tier, generate and response
  headers through typed Manager authority. The SSE filter is instance-owned,
  bounded to 4,096 traces, has a ten-minute TTL and refuses chunks over 1 MiB.
- Thirty-five Usage tests, two semantic benchmark-mirror tests and eleven
  Claude consumer tests pass in both matrices. All 37 pinned Go top-level
  tests, strict Clippy and formatting are green, so both upstream test mirrors
  receive credit rather than treating the benchmark as compile-only.

Forensic finding: Go's global `sync.Map` plus per-entry timers is not required
for wire parity. Bounded instance state preserves correlation and terminal
events without immortal timers or cross-gateway trace leakage.

### Worker 18ee — Runnable injected HTTP credential example

- The HTTP request example is an explicit Cargo example using the typed
  provider registry. Its executor injects the selected auth API key, then hands
  GET and POST requests to an injected `HostHttpClient`; no ambient default
  client or live network is required for validation.
- Two supplemental tests in both matrices prove credential/header/body
  forwarding and fail-visible missing transport without secret-bearing Debug.
  The pinned Go example builds with no tests, and example strict Clippy plus
  formatting are green. No upstream-test credit is invented.

Forensic finding: replacing `http.DefaultClient` with a supplied host client
keeps the example useful in CTOX while making proxy, TLS and cancellation
authority explicit and testable.

### Workers 18ef–18ei — SDK Service assembly and Home plugin transaction

- Service, lifecycle, Watcher and Pprof mirrors now materialize the previously
  fail-visible `ServiceAssembly`. Host directory/listener/clock/shutdown
  capabilities remain injected; provider loading precedes listener start and
  shutdown is serialized and idempotent. Pprof ownership transfer and stale
  serve completion cannot stop a replacement listener.
- Config and model state are instance-owned snapshots with validate-before-
  publish, monotone commit sequence, injected model registry/catalog and
  credential-checked config-index fallback. Exclusions, prefixes, Codex
  defaults, OpenAI-compatible modalities and OAuth alias/fork behavior retain
  their upstream ordering without a Go global registry.
- Auth, executor and plugin orchestration is active behind typed managers:
  updates coalesce, stale runtime state is not resurrected, captured cooldown
  persistence cannot be swapped through a global, Codex/XAI bindings are
  preserved unless explicitly rebound, and plugin ownership/scheduler/model
  phases stay ordered per gateway instance.
- `home_plugins.rs` ports the transaction state machine without recreating the
  displaced Redis Home control plane. A deterministic sync key includes the
  credential revision and upstream-encoded raw plugin config; fetch failure,
  unsupported-protocol fallback, disabled state, staged deletes, status retry,
  delete-at-most-once and mark-after-commit are explicit.
- The settled strict closure is 359/605 production and 218/418 upstream-test
  mirrors. `home_plugins_test.rs` and the 2,984-line
  `service_executionregistry_test.rs` remain partial because replacement,
  overlay-commit and subscriber-lifetime cases require the separate
  `service_home.rs` slice now in progress. Their direct disposition tests count
  as verified classification, not strict closure.

Forensic findings after workers 18ef–18ei:

1. **Builder requirements are a useful migration boundary only temporarily.**
   Once Service owns the injected host capabilities, leaving the requirement
   list unmaterialized would conceal a lifecycle gap rather than protect it.
2. **Model catalog and executor ownership must remain instance-scoped.** A Go
   singleton would make two embedded gateways influence each other's model
   visibility, plugin binding and cooldown persistence.
3. **Home acknowledgement is a commit protocol.** Advancing a status cursor or
   marking a sync key before a successful control-plane write loses retry
   evidence; repeating a delete after a failed status write violates at-most-
   once mutation. The retained finalization work separates both boundaries.
4. **A consolidated Go integration test cannot receive file credit from one
   passing Rust assertion.** The registry isolation assertion is real, but the
   mirror remains partial until Home generation, replacement and shutdown
   ownership are ported and exercised.

Strategy adaptation after workers 18ef–18ei:

- Finish `internal/home` and `service_home.rs` as one ownership vertical slice,
  then promote both partial Home test mirrors together. Continue selecting
  module clusters whose lifecycle authority and failure evidence can be closed
  in the same wave; never use a large upstream test file as a shortcut to
  credit unrelated behavior.

### Worker 18ej — Injected internal Home protocol and release runtime

- Six remaining production mirrors and all five Home test mirrors are closed.
  Auth/model/refresh, in-flight, release, plugin-task and plugin-status frames
  use strict typed serialization. Dispatch distinguishes pre-send failure from
  ambiguous post-send failure and fences credentials by membership/lifetime.
- KV operations retain NX/XX and EX/PX validation, TTL ceiling, CAS, queue/key
  naming and required-versus-best-effort behavior. Keys are hashed/redacted in
  diagnostics. Release publication is cumulative per credential/model and
  retains the latest snapshot across retry, waiter and sender replacement.
- `HomeTransport`, `HomeRuntime`, `CertificateStore` and enrollment boundaries
  replace process globals, Redis pools/discovery and implicit home-directory
  authority. The client directly implements the release and plugin-status
  sinks used by the active coordinators.
- Twenty-two Rust tests pass in both matrices and the pinned Go
  `internal/home` package passes. Both scoped strict Clippy matrices and
  formatting of the module are green. Strict closure reaches 365/605
  production and 223/418 test mirrors.

Forensic finding: wire compatibility does not require transport ownership.
Keeping RESP enrollment and Home DTO semantics while injecting durable queue,
certificate and release boundaries lets `service_home.rs` port generation and
acknowledgement semantics without reintroducing a second CTOX control plane.

The full Rust matrix currently exposes two unrelated Gemini Responses
thought-signature differential failures. They are recorded as open shared-gate
findings rather than attributed to or hidden by the Home wave.

### Workers 18ek–18el — Complete executor Helps and repair shared differential

- The last four executor Helps production mirrors and four tests are closed.
  Claude input-token state patches only missing/zero `message_start` usage while
  preserving SSE/CRLF bytes; payload rules retain upstream prefilter,
  defaults, overrides and reverse-filter order; API/WebSocket logging is
  request-local, masked and bounded; protected Anthropic/ChatGPT HTTP routing
  delegates TLS/socket ownership to an injected host factory.
- OpenAI-compatible execution consumes the shared token, payload and error-
  summary helpers. Targeted Claude, payload, logging, uTLS and consumer suites
  pass, as does the complete pinned Go Helps package. Both library feature
  matrices compile and pass strict Clippy for the settled scope.
- The previously reported Gemini full-matrix failure was reproduced
  independently. `compatible_gemini_signature` correctly accepts the special
  validator bypass, but that meant the cleanup pass accidentally reinserted it
  on the second parallel synthetic function call. Later siblings now exclude
  the synthetic bypass before validating real provider signatures.
- The Gemini Responses suite is 16/16 green, including the complete pinned-Go
  22/22 request differential. This repair changes no file-credit count; strict
  mirror closure is 369/605 production and 227/418 tests.

Forensic finding: a syntactically valid provider signature is not necessarily
provider-originated authority. Synthetic carrier/bypass values require an
explicit provenance rule at every normalization stage, otherwise a generic
compatibility validator can turn a temporary sentinel into durable protocol
state.

### Worker 18em — Fenced SDK Service-Home transaction lifecycle

- `service_home.rs` closes the production mirror and
  `home_plugins_test.rs` closes its strict test mirror. The runtime now owns an
  injected overlay `stage → commit → apply → finalize → publish` transaction
  with generation and ownership fencing, FIFO ordering, monotone sequences,
  at-most-once commit/publish, retry-safe plugin deletion and explicit
  replacement/shutdown cancellation.
- Fifteen Home-plugin and ten Registry tests pass in both Rust feature
  matrices, the pinned Go mirrors pass, and both strict Clippy matrices plus
  formatting are green. The 2,984-line `service_executionregistry_test.rs`
  remains partial because its concrete Home subscriber, log-forwarder and
  selector/watcher surfaces are not being claimed through mocks. Strict
  closure reaches 370/605 production and 228/418 tests.

Forensic finding: lifecycle ownership is the stable porting boundary, not Go's
file size. Keeping the large integration mirror partial while closing the
transactional production path avoids manufacturing parity from unrelated test
cases.

### Worker 18en — Internal Config clone and provider compatibility slice

- Clone, Codex-live and Vertex compatibility production mirrors plus five
  upstream tests are closed. Codex websocket header defaults are converted
  into the real executor type; XAI alpha/API-key behavior and deep-clone
  isolation are exercised through consumers rather than inert helpers.
- `config_normalization.rs` and `parse.rs` are active partial ports needed by
  this slice, but receive no strict production credit. The pinned Go Config
  package, 25 no-default and 26 all-feature Rust Config tests, both strict
  Clippy all-target matrices and formatting pass. Strict closure reaches
  373/605 production and 233/418 tests.

Forensic finding: compatibility parsing should be promoted only together with
the consumers that prove its precedence and defaults. A compile-complete
normalizer without the remaining YAML/load/validation behavior would be a
misleading full-port claim.

### Worker 18eo — Presence-aware Config credential concurrency

- The concurrency config preserves YAML field presence, so a missing value is
  distinct from explicit zero or null. Go duration nanoseconds/strings,
  legacy defaults, limiter bounds and signed-64-bit overflow behavior are
  covered by the production mirror and both upstream test mirrors.
- Four focused Rust tests pass in both feature matrices, the complete pinned
  Go Config package passes, and both strict Clippy all-target matrices plus
  formatting are green. Strict closure reaches 374/605 production and 235/418
  tests.

Forensic finding: defaulting at deserialization time loses information needed
for upstream-compatible migration. Presence must survive until the config
normalization boundary decides whether a legacy default applies.

### Worker 18ep — Injected SDK execution pipeline context

- The final SDK Pipeline production mirror is active. It carries typed request,
  options, selected Auth and translator state; optional before/after/stream
  hooks retain upstream order and nil behavior.
- CTOX adapts the mutable Go `http.Client` field to an immutable injected
  `HttpTransport` route. Actual socket, TLS and cancellation ownership remains
  with the executor host. Debug output exposes only whether Auth was selected,
  never its ID, attributes or proxy credentials.
- Two supplemental tests pass in both matrices, the pinned Go package builds
  without upstream tests, and both strict Clippy matrices plus formatting are
  green. No test credit is invented; strict production closure reaches
  375/605.

Forensic finding: the pipeline should carry transport choice, not transport
authority. This keeps middleware composition upstream-compatible without
allowing request-local code to create an unsupervised network client.

### Worker 18eq — Complete instance-owned internal Logging runtime

- The remaining ten production and five test mirrors close the module at
  13/13 production and 6/6 upstream tests. Request-log storage, clock and sinks
  are injected; the output controller, rotating file sink, bounded backups and
  directory cleaner have explicit instance ownership.
- Home app/request forwarding uses typed sinks without a global Redis/Home
  fallback. Rebind/deactivate is owner-safe, streaming is bounded, request
  information uses its injected start time, decompression covers the upstream
  formats, and temporary body files are cleaned deterministically.
- All 27 focused Rust tests pass in both feature matrices, the pinned Go
  Logging package passes, and both full-target strict Clippy matrices plus
  compilation and scoped formatting are green. Strict closure reaches 385/605
  production and 240/418 tests.

Forensic finding: a logger is lifecycle infrastructure, not a collection of
format helpers. Rotation ownership, asynchronous backpressure and Home sink
replacement must close together or shutdown and reconfiguration races remain
hidden behind otherwise-correct log bytes.

### Worker 18er — macOS empty-environment Plugin Supervisor gate repair

- The full integration run exposed all four Plugin Supervisor tests exiting
  before readiness. `Command::env_clear` was working; CoreFoundation creates
  `__CF_USER_TEXT_ENCODING` inside a newly exec'd macOS process even when the
  supplied environment is empty. The child fixture now permits exactly this
  UID/locale metadata key on macOS and rejects every other key on every
  platform.
- Spawn, capability registration/gating, crash/restart backoff and graceful
  shutdown pass 4/4 in both feature matrices. The complete common test runs
  pass 1,844/1,847 no-default and 1,879/1,882 all-features, with the same three
  intentional ignores; all integration targets and both strict Clippy
  all-target matrices are green. This repair receives no mirror credit.

Forensic finding: an empty exec environment and an empty post-runtime
environment are not identical on macOS. Security tests must distinguish
OS-created non-secret metadata from inherited parent authority without
loosening the allowlist to arbitrary locale, loader or credential variables.

### Worker 18es — Scaffold-free typed internal Config boundary

- The final Config block closes all remaining file statuses. Load/parse use a
  `TypedConfigSource` and injected `data_root`; plugin credentials carry
  `RuntimeSecretRef` instead of `*-env`, and plaintext management secrets are
  rejected. Canonical closed-schema YAML preserves leading document comments
  without retaining unknown legacy authority.
- Six production files changed status, but `config_normalization.rs` and
  `parse.rs` were already verified partials; promotion to `adapted_to_ctox`
  therefore is not counted twice. The block adds four newly classified
  production files and six tests, with strict credit for `sdk_config.go` and
  five exact tests. Config is scaffold-free.
- The pinned Go package, 42/42 Rust tests in both matrices, both strict Clippy
  all-target matrices and formatting pass. No process environment or HOME
  access remains. Strict closure reaches 386/605 production and 245/418 tests.

Forensic finding: preserving every unknown YAML node is not automatically
forward compatibility when those nodes can encode secret or process authority.
The public SDK boundary is closed and typed; unsupported authority must be
reviewed before it enters CTOX runtime state.

### Worker 18et — Complete supervised internal Watcher runtime

- All 17 production and nine test mirrors are active. Config/credential
  discovery, hashing, ordered add/modify/delete diffs, bounded dispatch and
  event coalescing run under one supervised start/stop lifecycle.
- Paths, filesystem, clock, config decoder, plugin-auth parser and persistence
  sink are injected. JSON is the built-in decoder; other formats remain a host
  capability. Native fsnotify is represented by an injected event source with
  a supervised polling fallback and at most 100 ms stop latency.
- The pinned Go watcher, diff and synthesizer packages pass; 16 focused Rust
  tests, compilation and strict Clippy pass in both matrices, as does scoped
  formatting. Strict closure reaches 403/605 production and 254/418 tests.

Forensic finding: filesystem observation and config decoding are separate
authorities. Keeping both injected lets the port preserve ordering and reload
semantics without creating an ambient HOME watcher or a second global YAML
configuration owner.

### Worker 18eu — Complete injected Gemini executor cluster

- AI Studio, Gemini/Interactions and Vertex production executors plus both test
  mirrors are active. The cluster covers endpoint and revision selection,
  header precedence, SSE framing and `[DONE]`, thinking/payload rules, output
  limits, image aspect defaults, token counting, relay colon normalization,
  regional Vertex routes and Imagen conversions.
- Registry, `HostHttpClient`, `TranslationContext` and service-account token
  minting through `VertexAccessTokenProvider` are injected. Observability/TTFT
  and Home refresh remain host responsibilities instead of new executor
  globals.
- Sixteen Gemini, four AI Studio and five Vertex Rust tests pass in both
  matrices; all 22 selected pinned-Go mirror tests, both strict Clippy matrices
  and scoped formatting pass. Strict closure reaches 406/605 production and
  256/418 tests.

Forensic finding: provider execution parity does not require the executor to
own telemetry or credential refresh. Keeping those as host capabilities lets
all three Gemini deployment modes share protocol behavior without sharing
mutable process authority.

### Worker 18ev — Runnable injected custom-provider example

- The custom-provider mirror is an executable Cargo example. It composes a
  `ProviderExecutor`, refresher, provider registry, OpenAI↔custom translator,
  instance-owned model registry and injected host HTTP client without a live
  httpbin dependency or global SDK registries.
- Credential injection occurs only while preparing host requests; Debug and
  errors do not render the API key. Unary execution, bounded synthetic stream
  and fail-visible token counting retain the upstream demonstration surface.
- Two supplemental tests pass in both matrices, `cargo run` produces the real
  echo request/response, the pinned Go example builds without tests, and both
  Example strict Clippy matrices plus formatting pass. No upstream-test credit
  is invented; strict production closure reaches 407/605.

Forensic finding: an integration example is stronger when its extension
points actually execute but external network availability is not part of its
correctness. The host-client boundary demonstrates provider wiring while
remaining deterministic and secret-contained.

### Worker 18ew — Complete host-injected internal TUI runtime

- All 12 production mirrors and the upstream test mirror are active. A
  synchronously owned terminal runloop guarantees `leave` on quit and render
  failure. Backend, clock, size, event source and management transport are
  injected; dashboard, config, auth, keys, logs and OAuth tabs carry real state.
- OAuth state is generation-fenced against stale messages with bounded error
  budget and cancellation. The log hook is bounded. Crossterm, HTTP and browser
  opening remain host capabilities; the default browser launcher fails closed
  rather than creating a second terminal/runtime owner.
- The pinned Go TUI test, four focused Rust tests in both matrices, compilation,
  both strict Clippy matrices and formatting pass. Strict closure reaches
  419/605 production and 257/418 tests.

Forensic finding: TUI parity is state-transition and ownership parity, not a
specific rendering dependency. Keeping the terminal backend injected preserves
the Go lifecycle while allowing CTOX to remain the sole runtime authority.

### Worker 18ex — Complete safe internal Util translator and SSH boundary

- The strict Translator port covers escaped/dynamic JSON paths, exact missing-
  key errors, tool-name maps, byte-preserving declaration deduplication through
  `RawValue`, restore semantics and real Claude response consumers.
- The SSH helper is a classified CTOX adaptation around injected `IpProbe` and
  `SshProcessRunner`: credential-free HTTPS discovery, bounded timeout, typed
  addresses/ports, absolute executable and key paths, validated usernames and
  argv-only command plans. No HOME, process environment, shell or implicit
  `Command` authority remains.
- Five supplemental tests and 65 Util tests pass in both matrices, the pinned
  Go package passes, and both strict Clippy matrices plus formatting are green.
  The strict Translator adds one production closure; SSH remains adapted
  classification, bringing strict closure to 420/605.

Forensic finding: safely reproducing command intent is not equivalent to
spawning it. Separating SSH discovery and argv planning from the host-owned
process runner preserves behavior without granting the utility layer ambient
process authority.

### Worker 18ey — Complete typed internal Command orchestration

- All ten production mirrors are active. Provider login, prompt I/O, foreground
  and background service lifecycle, cancellation and Vertex credential import
  run through injected handlers, factories, filesystems and sinks. Secret
  values resolve outside the module through typed `SecretRef` capabilities.
- No `os.Exit`, HOME, process environment, shell, global flag or ambient store
  authority is present. Signal ownership, concrete service construction and
  provider token persistence remain explicit host adapters.
- Four supplemental Rust tests pass in both matrices. The pinned Go package has
  no tests and builds green; compilation, both strict Clippy all-target
  matrices and scoped formatting pass. No upstream-test credit is invented;
  strict production closure reaches 430/605.

Forensic finding: command packages should compile user intent into capabilities
and lifecycle operations. Keeping exit, signals and persistence in the host
prevents library calls from terminating or reconfiguring the CTOX daemon.

### Worker 18ez — Complete Antigravity executor and replay cluster

- Seven production and eight test mirrors close the cluster; the already closed
  auth mirror was revalidated without duplicate credit. Auth, transport,
  registry and cancellation remain injected. The executor covers 401
  singleflight refresh, durable credits balance/cooldown/lease, structured 429
  decisions, token counting, schema sanitation, Interactions, signature repair
  and lane-specific clearing, plus terminal-only reasoning replay commit.
- The signature mirror exposed a false assumption in an already-ported Gemini
  translator: parallel responses must be correlated and reordered by call ID
  and name. That defect is repaired with zero additional file credit.
- Thirty-five focused no-default and 46 all-feature Rust tests pass. All tests
  from the eight pinned-Go mirror files, both strict Clippy matrices and scoped
  formatting pass. Strict closure reaches 437/605 production and 265/418 tests.

Forensic finding: a passing broad fixture count can still encode the wrong
ordering assumption. The first 137/140 run was used to correct three false
mirror assertions before credit; test adaptation must follow pinned behavior,
not force implementation output into the expected shape.

### Worker 18fa — Typed fail-closed top-level command cores

- Fetch-Antigravity, Fetch-Codex and Server are classified CTOX adaptations;
  both upstream command tests are strict ports. Their testable cores inject
  auth store, secret resolution/write, HTTP, clock, cancellation, file/output,
  typed config source, filesystem and service-host authority.
- There is no auth-directory scan, HOME/environment config, implicit proxy,
  live-network test, shell or process exit in a core. Custom headers reject
  CR/LF, non-2xx bodies are not reflected, and cancellation fences store,
  network, write and service-start boundaries.
- Standalone mains are thin argv/stdout/filesystem adapters but deliberately
  fail closed without CTOX-bound credential/config/HTTP/service capability;
  the three production mirrors therefore receive classification but no strict
  production credit. Ten focused Rust tests, the three pinned-Go command
  packages, full 1,902/1,905 no-default and 1,937/1,940 all-feature runs, both
  strict Clippy matrices and formatting pass. The two tests raise strict test
  closure to 267/418.

Forensic finding: a runnable binary that silently reconstructs HOME credential
discovery would move the port away from CTOX even if its CLI matched upstream.
The command cores are the integration seam; operational standalone authority
must be bound explicitly by the host before those production files can receive
strict closure credit.

### Worker 18fb — GitStore recovery hardening without false closure

- The local-filesystem Git remote now fences dirty recovery byte-for-byte,
  resets the index before staging an explicit managed-path whitelist, restores
  HEAD after a failed push and retains retryable worktree bytes. Delete fencing
  and `AuthStore` mapping are covered as well.
- Nine Rust tests pass in both matrices and the pinned Go `internal/store`
  package is green. The two mirrors deliberately remain `partial` with zero
  production and test credit.
- Authenticated HTTPS/SSH, lease-conflict retry, packfile/object recovery that
  proves non-conflicting dirty paths and post-push GC still require a typed
  process/LocalTransport capability.

Forensic finding: more reliable behavior is not the same as complete parity.
Recovery code receives no file credit while a class of upstream remotes and
conflict semantics remains unavailable.

### Worker 18fc — Authority-injected SDK Config facade

- `sdk/config/config.rs` is live as an `adapted_to_ctox` public facade. It
  reexports the completed internal configuration types and delegates parsing
  through an injected `TypedConfigSource` plus explicit data root.
- Writes require a `TypedConfigSink`; the optional filesystem adapter is
  caller-constructed. Management and OpenAI credentials are secret references,
  not plaintext fields. No cwd, HOME, environment or process authority is
  inferred by the SDK.
- Four supplemental Rust tests pass in both matrices, the pinned Go package has
  no tests, and both strict Clippy matrices pass. The file adds one classified
  production mirror but no strict standalone-authority credit.

Forensic finding: a public compatibility facade may preserve type and helper
roles while deliberately changing authority-bearing function signatures. Such
files are CTOX adaptations and must not be labeled literal ports.

### Worker 18fd — Complete OpenAI Responses translator

- Four production and two test mirrors move from `partial` to
  `adapted_to_ctox`, adding four production and two test strict credits without
  new classified credit.
- The request, aggregate response and persistent SSE state cover tool-call
  draining, byte-content-preserving tool outputs, JSON-pointer image checks,
  translated-vs-original request roles, namespace propagation, parallel tool
  correlation, stable output indices and request-local sequence numbers.
- Both Rust matrices pass 38/38 cases, the pinned package passes 33 Go tests,
  six Go/Rust differential fixtures pass and both all-target strict Clippy
  matrices are green. The real `register_all` consumer is exercised.
- Five older OpenAI-to-Claude markers named `full` were normalized to canonical
  `ported` without credit; their already-recorded semantic evidence is unchanged.

Forensic finding: an unrecognized status spelling can silently disappear from
both open and closed counters. `build_port_map.sh` now rejects every unknown
`Port-Status` value, so metadata drift fails tracking instead of inflating
completion.

### Worker 18fe — Pluginhost process/RPC/config/platform subwaves

- Seven production and five test mirrors are closed as a coherent
  process-isolated pluginhost kernel: ABI, callback contexts, client guard,
  unary/stream RPC clients, normalized config and executable platform discovery.
- Recursive host-handle sanitization, bounded decoded streams, terminal errors,
  detach-then-drain shutdown and callback deadline/cancellation ownership have
  no package globals. Platform discovery uses an injected filesystem/platform
  triple, absolute pre-resolved directories and explicit cleanup authority.
- The pinned Go package, 38/38 Rust tests in both matrices, both strict Clippy
  matrices and scoped formatting pass. Strict closure is now 448/605 production
  and 274/418 tests; the rest of `internal/pluginhost` remains an active module
  wave rather than being split into unrelated file claims.

Forensic finding: keeping mirrored files does not require reproducing Go's
in-process shared-library authority. The upstream inner contracts can remain
reviewable while the owning Rust module consistently enforces process isolation.

### Worker 18ff — Complete public SDK PluginAPI contract

- `sdk/pluginapi/types.rs` and its upstream test mirror move from `partial` to
  `ported`, adding one strict production and one strict test credit without
  changing classified counts.
- All 106 declared Go contract types are represented. Twenty-one object-safe,
  `Send + Sync` async traits cover models, auth/login/refresh, frontend auth,
  scheduling/routing, provider execution, host HTTP/model/stream/auth access,
  translators, normalizers, interceptors, lifecycle, thinking, usage, CLI and
  Management API handlers.
- Injected host HTTP clients are excluded from Serde, redacted from Debug and
  legacy wire fields are ignored on decode; DTO construction grants no network
  or process authority.
- Nine Rust contract tests pass in both matrices, all eight upstream cases are
  represented, the pinned Go package is green, and both strict Clippy plus the
  whole-crate formatting gate pass. Strict closure reaches 449/605 production
  and 275/418 tests.

Forensic finding: compile-time trait coverage is part of wire compatibility.
Porting DTO shapes without proving that every capability remains dynamically
dispatchable would leave the public plugin SDK structurally incomplete.

### Worker 18fg — Machine-attested upstream promotion gates

- Upstream review ledgers use schema v3. `record_upstream_gate.sh` executes the
  exact argv for one of the ten promotion gates, captures its output in the
  candidate review directory and records completion time plus SHA-256.
- `check_upstream_review.sh` requires evidence for every gate, constrains each
  log to its expected commit-local filename and rehashes it before promotion.
  Editing a boolean is no longer sufficient authorization.
- A full zero-delta self-test recorded and verified all ten gates. The live
  `a88197f` candidate ledger was regenerated fail-closed with 74 pending file
  reviews and 0/10 attested gates.

Forensic finding: automation is sustainable only when its success claims are
machine-derived. A review checklist remains useful for semantic dispositions,
but executable gates need replayable argv and tamper-evident output evidence.

### Worker 18fh — Complete transport-neutral SDK API Handler module

- Both Handler waves close 24 production and 16 upstream-test mirrors. No
  `scaffold` or `partial` marker remains below `sdk/api/handlers`.
- The active graph covers typed request context and errors, routing,
  interceptors, model execution, Responses, WebSocket, Images, Videos and Codex
  surfaces. Seventy-four Rust cases pass in both feature matrices, the focused
  pinned-Go packages pass, and both strict Clippy matrices plus formatting are
  green.
- Strict closure reaches 473/605 production and 291/418 tests.

Forensic finding: mirror closure is not synonymous with HTTP production
readiness. Gin/listener ownership is adapted into transport-neutral typed
cores; real pool-backed execution and listener-level behavior remain a
separate readiness dimension in the dashboard.

### Worker 18fi — Codex terminal failure forces fresh dispatch

- The Codex terminal-stream regression mirror is active. A committed terminal
  WebSocket failure invalidates the prior dispatch so the next request reloads
  credentials and establishes a fresh connection.
- The exact Rust case passes in both matrices and its pinned-Go reference case
  passes. No HOME/global dispatcher is introduced; selection stays behind the
  typed subscription boundary. Strict test closure reaches 292/418.

Forensic finding: reconnecting a transport is insufficient when its credential
selection may be stale. Terminal failure must invalidate both connection and
dispatch ownership before another request is attempted.

### Worker 18fj — Pluginhost bridge, stream and management continuation

- Thirteen more production and six test mirrors close after the initial
  process/RPC kernel, bringing the pluginhost wave to 20 production and 11
  tests. Scheduler/router, command-line, HTTP, HTTP-stream, model-stream and
  Management contracts are active.
- Stream handles are plugin-owner-bound; model authority receives the caller
  plugin identity to prevent recursive dispatch. HTTP and process authority are
  injected, and Management registration cannot become a Browser Business OS
  data bridge.
- The pinned-Go package, 52 Rust cases in both matrices, both strict Clippy
  matrices and formatting pass. Strict closure reaches 486/605 production and
  298/418 tests.

Forensic finding: a capability bridge is safe only if every opaque handle is
scoped to its creating plugin. Globally readable stream IDs would turn process
isolation into a cosmetic boundary.

### Worker 18fk — Authority-injected SDK Auth leaves

- Nine production and two upstream-test mirrors close: API-key config,
  credential policy, session cache, error events, Antigravity credits, Home
  session aliasing and the response-model JSON/SSE rewrite state machine.
- Stores, clocks and event sinks are injected. There are no package globals,
  HOME/environment reads, plaintext secret stores or autonomous schedulers.
  Seven supplemental Rust cases strengthen the port without being counted as
  upstream-test mirrors.
- Focused tests pass in both matrices, both strict all-target Clippy gates and
  formatting are green. Strict closure reaches 495/605 production and 300/418
  tests.

Forensic finding: pure leaves can close independently, but Manager, Home and
force-mapping integration tests remain zero-credit scaffolds until the shared
ownership path is executable. Supplemental tests cannot substitute for those
upstream contracts.

### Workers 18fl–18fm — Resume-safe candidates and durable promotion receipts

- Candidate preparation builds delta, empty ledger and impact summary beside
  the destination. It preserves an identity-matching operator review
  byte-for-byte and publishes only missing artifacts atomically without
  overwriting. Candidate, delta, summary or review mismatch aborts before any
  destination mutation.
- A successful promotion now creates one non-overwritable,
  commit-addressed receipt under `docs/cliproxyapi-upstream-history`. It embeds
  the full immutable delta, every semantic disposition and all ten gate
  attestations while retaining the SHA-256 identities of the original ignored
  documents. The receipt is part of rollback scope.
- Separate regression tests cover resume with operator progress, four mismatch
  classes, valid receipt creation, candidate-identity tampering, gate tampering
  and overwrite refusal. Both shell suites, syntax checks and diff-check pass.

Forensic finding: a 30-day CI artifact is useful evidence but not durable
project history. Promotion must leave a tracked receipt, while repeatable
candidate discovery must never reset human or agent review progress.

### Worker 18fn — Pluginhost auth callback and provider bridge

- Two production and two test mirrors close. Caller plugin identity scopes
  auth list/get/runtime/save operations; saves accept only a valid basename,
  `.json` suffix and object payload before invoking host-owned persistence.
- The RPC auth provider normalizes returned identity and receives a typed,
  redacted host config summary. Failure tests avoid `Debug` formatting of
  envelopes, so payloads and secrets cannot enter assertion output.
- Focused and broad Rust tests, the pinned-Go pluginhost package, both strict
  all-target Clippy matrices and scoped formatting pass. Strict closure reaches
  497/605 production and 302/418 tests.

Forensic finding: process isolation does not by itself authorize a plugin to
enumerate or mutate another plugin's credential material. Callback routing
must bind every auth operation to the calling plugin before host dispatch.

### Worker 18fo — Repository Claude-Code and builtin-tool sentinels

- The Claude-Code compatibility sentinel validates the four upstream wire
  shapes for tool progress, session state, tool-use summaries and
  `can_use_tool` control requests without inventing a production DTO owner.
- The repository translation sentinel proves that OpenAI→Codex preserves a
  built-in `web_search` declaration, context size and tool choice, while
  Responses→Chat drops the unsupported built-in tool.
- Six focused Rust cases pass in both feature matrices, the exact pinned-Go
  filters pass, and both strict all-target Clippy matrices plus formatting are
  green. Two upstream test mirrors close; strict test closure reaches 304/418.

Forensic finding: repository-level sentinels are valuable only when they run
through the same instance-owned registries as production conversion. Copying
their expected JSON into isolated helpers would test fixtures rather than the
port.

### Worker 18fp — Complete bounded plugin example SDK graph

- All 31 production and nine test mirrors under `examples/plugin` close. The
  Rust port compiles them as one test-only Example SDK graph instead of creating
  22 separately authoritative production binaries.
- Example state is instance-owned. Host callbacks, search and stream operations
  are injected; no example reads ambient environment, spawns a shell, binds a
  listener or carries live credentials. The Claude web-search router retains
  bounded routing, fallback, penalty and stream behavior.
- Thirty-five Rust tests pass in both matrices. All 22 nested pinned-Go modules
  pass; the Claude router uses a temporary modfile because its pinned `go.sum`
  omits an indirect `x/sys` checksum. Both strict Clippy matrices and scoped
  formatting pass. Strict closure reaches 528/605 production and 313/418 tests.

Forensic finding: executable documentation must not silently become another
runtime owner. A common bounded graph proves every example contract while
keeping process, network and secret authority in explicitly injected hosts.

### Worker 18fq — Auth force-mapping, usage and publication tests

- Seven more upstream test mirrors close around force-mapped JSON/SSE/finish
  rewriting, Codex WebSocket forwarding, Antigravity simulation,
  request-termination fallback, usage context, selected-auth publication and
  recent-request state preservation.
- The necessary OAuth/model, conductor execution and stream bridges are active
  but remain honestly `partial`; no production credit is awarded until Manager
  and Home ownership paths are complete.
- All 139 Auth tests pass in both matrices, the complete pinned-Go Auth package
  passes, and both strict all-target Clippy matrices plus formatting are green.
  Strict test closure reaches 320/418.

Forensic finding: integration tests may legitimately close before their broad
production facade, provided they execute the real typed sub-contract and do
not relabel the still-incomplete owner. This preserves evidence without hiding
the remaining Manager/Home work.

### Worker 18fr — Pluginhost adapters, callbacks and transactional snapshot

- Eight more production and three test mirrors close. Standard host callbacks
  cover HTTP, HTTP stream, executor stream and logging with owner-bound stream
  handles. Five adapters construct SDK capabilities from one immutable
  registration snapshot.
- Every callback identity is short-lived, caller-plugin-bound and deadline
  scoped. Async and inline executor streams retain their plugin lease until a
  terminal read/close. Host apply/reconfigure is serialized and transactional;
  invalid registrations are never published, and targeted unload/shutdown
  operate on immutable priority snapshots.
- Manual `Debug` implementations expose only normalized identifiers and safe
  counts. Sixty-seven Pluginhost tests pass in both matrices, the pinned-Go
  package passes, and both strict Clippy matrices plus global formatting are
  green. Strict closure reaches 536/605 production and 323/418 tests.

Forensic finding: snapshot immutability and process isolation solve different
problems. A process-safe plugin host still needs atomic publication, otherwise
dispatch can observe a half-reconfigured capability graph.

### Workers 18fs–18fu — Lifecycle, management and complete Pluginhost closure

- One Codex WebSocket test mirror closes around generation-bound execution
  lifecycle ownership. Reusing the same lifecycle and connection generation
  binds exactly once; every terminal session path ends the lifecycle before
  closing the transport, while bind failure cleans up fail-closed.
- Three Management API production and three test mirrors close through an
  injected transactional configuration store. Validation and persistence are
  atomic, deletion uses stable identifiers, and only secret references cross
  the boundary; the route cannot become a Browser Business OS data bridge.
- The last two production and four test mirrors in `internal/pluginhost`
  close. The module is now exactly 36/36 production plus 21/21 tests, with no
  scaffold or partial marker remaining. Both full Rust matrices, both strict
  Clippy matrices, formatting and the pinned Go package pass.
- Strict closure reaches 541/605 production and 331/418 tests. Mechanical
  classification reaches 581/605 production and 361/418 tests; four newly
  classified root integration tests remain outside strict credit until their
  final all-features Clippy gate completes.

Forensic finding: marker closure and strict credit must remain separate even
late in the port. A worker may finish its local reference tests while a shared
graph gate is temporarily unstable; the dashboard should expose that lag
instead of predicting the credit.

### Worker 18fv — Root integration sentinels complete

- Four remaining root test mirrors close, bringing `test/` to 6/6 with three
  literal ports and three CTOX adaptations. They execute the real registry
  stream lifecycle, registered request translators, `ThinkingEngine` model
  capabilities and the instance-owned Gemini usage reporting queue.
- Focused Rust cases pass in both feature matrices, the four exact pinned-Go
  tests pass, and both strict all-target Clippy matrices plus formatting are
  green. Strict test closure reaches 335/418.

Forensic finding: a repository integration sentinel earns credit only when it
crosses the production ownership graph. In particular, a zero-usage event is
still an auditable successful record; filtering it in a test helper would miss
the real queue contract.

### Worker 18fw — Management auth and transport header edge

- One Management handler test mirror closes. Authentication bans after five
  failures using an injected clock and constant-time digest comparison; the
  server header reports actual `LocalTransport` capability instead of a
  hard-coded value.
- Handler and management-server production mirrors remain honestly partial.
  Two focused cases and four supplemental cases pass in both matrices, along
  with both strict Clippy matrices and formatting. Strict test closure reaches
  336/418.

Forensic finding: closing an edge-case test does not close its broad server
owner. Keeping the production facade partial preserves the distinction between
validated policy behavior and complete route parity.

### Worker 18fx — Management handler owner closure

- The Management handler production mirror moves from partial to a complete
  CTOX adaptation. It composes injected authentication, transactional config
  service and reload authority; save then reload is generation-monotonic, while
  invalid configuration triggers neither durable mutation nor reload.
- Debug output redacts key and secret references, and the owner has no ambient
  environment/filesystem authority or Business OS HTTP data path. Four handler
  cases pass in both matrices, along with both strict Clippy matrices and
  formatting. Strict production closure reaches 542/605.

Forensic finding: reload is part of the configuration transaction's observable
ordering even when persistence and runtime application use separate injected
authorities. Publishing generations out of order would make a valid durable
write appear rolled back in the live runtime.

### Worker 18fy — Runtime target and service registry closure

- The WebSocket session-target test mirror closes with three real
  store/generation/lifecycle cases. Physical close is terminal-once, stale
  generations cannot remove replacements, and distinct store instances do not
  share session ownership.
- The large service execution-registry mirror closes as a CTOX adaptation with
  ten cases over the actual Registry and Home lifecycle coordinator: fencing,
  FIFO configuration, drain, failover, publisher teardown, barriers, retry and
  cancellation. Both Rust matrices, the exact pinned-Go tests, both strict
  Clippy matrices and formatting pass. Strict test closure reaches 338/418.

Forensic finding: a large upstream integration file should map to its actual
Rust owners, not to one equally large compatibility test. The credit remains
auditable because every extracted behavior crosses the production Registry or
coordinator boundary.

### Worker 18fz — Public SDK Pluginhost facade

- The final `sdk/pluginhost/host.go` production mirror closes as a
  provider-neutral facade over the already isolated internal host. Construction
  requires an injected `PluginLoader`; the facade cannot acquire filesystem,
  process, network or secret authority on its own.
- Dispatch reads immutable registration snapshots, configuration translation
  is typed, and Debug exposes only safe counts. Three focused Rust cases pass
  in both matrices, the pinned Go package compile-gate passes, and both strict
  Clippy matrices plus formatting are green. Strict production closure reaches
  543/605.

Forensic finding: an SDK facade must not recreate the authority its internal
host deliberately removed. Requiring the loader at construction keeps plugin
process ownership with CTOX while still exposing provider-neutral capability
dispatch to embedders.

### Workers 18ga–18gc — Store, credentials and model capabilities

- `internal/store/gitstore` closes its production and test mirrors. Local
  remotes remain in-process; HTTPS/SSH uses typed delegated authority rather
  than ambient `git`, environment, argv or credential helpers. Lease-aware
  retry, baseline-backed recovery and bounded object GC are active.
- Two Management credential production mirrors and three test mirrors close
  through stable IDs, auth indices and an injected store. HTTP responses expose
  neither secret values, filesystem paths, auth JSON nor Business OS data.
- API-key model capabilities close one production and two test mirrors with an
  immutable snapshot bound to the exact credential/config index. Suffix and
  force mapping, duplicate keys and keyless OpenAI compatibility preserve
  upstream semantics without placing credentials in request metadata.
- All three waves pass both Rust feature matrices, their pinned-Go references,
  both strict Clippy matrices and formatting. Strict closure reaches 547/605
  production and 344/418 tests; mechanical classification reaches 585/605 and
  367/418 respectively.

Forensic finding: provider-independent routing still needs credential-specific
capability identity. A model alias shared by two subscriptions cannot safely
reuse one mutable capability record, because thinking support and force-mapping
policy may differ even when the upstream model name is identical.

### Worker 18gd — Candidate documentation review begins

- The three README changes in candidate `a88197f` are reviewed and recorded in
  the immutable candidate ledger. Each adds only the translated four-line Alex
  entry to the inspired-project list; none changes API, runtime, dependency,
  license text or Rust behavior.
- The runtime configuration example and embedded Claude Code instruction asset
  remain pending because their diffs describe real cloaking, header, signing,
  timezone and cache-control semantics. The candidate therefore remains at
  3/74 file reviews and 0/10 gates and is not eligible for promotion.

Forensic finding: non-Go review is not a blanket documentation waiver. A README
project-list addition may be dispositioned without code, while a YAML example
or embedded text asset can encode runtime behavior and must stay coupled to the
corresponding production delta.

### Workers 18ge–18gf — API tools and Conductor core

- Management API tools close production and test mirrors behind an injected
  executor that owns network, proxy and secret resolution. The HTTP-facing
  layer validates HTTPS, method and size bounds, passes `$TOKEN$` unresolved to
  that authority and redacts headers and bodies from Debug.
- Eight Auth Conductor production mirrors close together: Manager, cooldown,
  execution, lifecycle, model rewriting, refresh, selection and stream. The
  scheduler-refresh test mirror also closes against the real instance-owned
  graph. Home-specific owners remain partial and outside this credit.
- API Tools pass four focused Rust cases per matrix and their exact pinned-Go
  cases; Conductor passes 51 per matrix and its exact pinned-Go scheduler cases.
  Both waves pass both strict Clippy matrices and formatting. Strict closure
  reaches 556/605 production and 346/418 tests.

Forensic finding: the Conductor is one ownership graph even though the upstream
source is split by concern. Closing its files together prevents refresh,
selection and stream paths from observing different mutable routing snapshots;
Home remains a separate lifecycle owner and is therefore gated separately.

### Worker 18gg — Candidate Gemini schema name-map parity

- The candidate production and test deltas for `internal/util/gemini_schema`
  are ported and reviewed. Traversal now alternates schema-node and author-name
  map context for `properties`, `patternProperties`, `dependentSchemas`,
  `$defs` and `definitions`.
- A property itself named `properties` therefore remains an ordinary schema
  node whose unsupported nested keywords are cleaned, while author properties
  named `propertyNames` or `patternProperties` remain intact. Three focused
  Rust cases pass in both matrices and the exact three candidate-Go regressions
  pass. Candidate review reaches 5/74; global gates remain 0/10 and the pin is
  unchanged.

Forensic finding: path suffix matching loses semantic context when schema
keywords are also legal property names. Carrying the node kind through recursive
traversal is both simpler in Rust and equivalent to upstream's trailing-keyword
parity rule.

### Workers 18gh–18gj — Logs, Home and Conductor regressions

- Management Logs close production and test mirrors behind an injected store.
  Cursor, limit, request ID and error-log names are validated; traversal and
  caller-provided paths are rejected, attachments are redacted in Debug.
- The complete Auth Home owner closes five production and twelve test mirrors.
  Manager, Registry, sessions, transport, clock and publisher are instance
  owned; the SDK consumes deliberate Home facade reexports rather than private
  submodule paths.
- Eight remaining Conductor regressions close Availability, Credits ranking,
  Force-Mapping, OAuth alias suspension, overrides and weight validation. A
  real parity defect found during the wave is fixed: negative scheduler weight
  is now published as zero, never `-1`.
- The Home wave passes 26 focused tests, both complete Rust matrices and the
  whole pinned Go Auth package. Logs and Conductor regressions pass their exact
  focused/pinned suites; every wave passes both strict Clippy matrices and
  formatting. Strict closure reaches 562/605 production and 367/418 tests.

Forensic finding: lifecycle closure must include telemetry publication, not
just request dispatch. If Home's clock or publisher is ambient/global, a
replacement generation can publish stale in-flight state after its registry
has already been retired.

### Worker 18gk — Candidate Codex terminal ID hydration

- Candidate deltas for Codex terminal accumulation and its stream-output test
  are ported and reviewed. When `response.completed.output` is non-empty but an
  item ID is absent, null or blank, the accumulator hydrates only that ID from
  the same indexed `response.output_item.done` event.
- Existing terminal IDs and all terminal item fields remain authoritative; the
  output-item event supplies no broader replacement. The focused Rust case
  passes in both feature matrices and the exact candidate-Go executor case
  passes. Candidate review reaches 7/74, with 0/10 global gates and no pin
  promotion.

Forensic finding: terminal reconstruction and terminal hydration are different
operations. Replacing a non-empty output array would discard the final event's
newer names or arguments; filling only a missing identifier preserves both
event order and terminal authority.

### Worker 18gl — Auth scheduler, selector and OAuth alias closure

- Scheduler, selector and OAuth model-alias production mirrors move from
  partial to `adapted_to_ctox`; eight scheduler, cooldown, compatibility-pool
  and request-auth test mirrors move from scaffold to adapted.
- The lane adds 22 focused regressions and exposes an owned
  `prepare_executor_request` path without moving request authority out of the
  instance-owned Home/Conductor graph.
- Both complete Rust matrices pass (2259/3 ignored without defaults and 2294/3
  ignored with all features), as do the pinned Go Auth package, both strict
  Clippy all-targets matrices and formatting. Strict closure reaches 565/605
  production and 375/418 tests; mechanical classification is 592/605 and
  397/418.

Forensic finding: selector parity is only meaningful together with cooldown
and credential preparation. Treating these as separate mechanical files would
allow a selected subscription to diverge from the identity actually attached
to the upstream request.

### Worker 18gm — Candidate Claude header timezone baseline

- Candidate deltas for Claude header-default types, normalization and their
  regression are ported into the consolidated Rust configuration module.
  `ClaudeHeaderDefaults` now carries a serde-defaulted timezone and sanitizes
  it with the other measured baseline fields.
- The Rust regression parses and trims `Pacific/Honolulu` while preserving an
  explicit false stabilized-profile choice. It passes with no default features
  and all features; the exact candidate Go regression, formatting and diff
  checks pass as well.
- Candidate review reaches 10/74 files. The behavior comments concerning
  cloaking and automatic signing remain coupled to their still-pending executor
  deltas; no global gate is claimed and the accepted pin is unchanged.

Forensic finding: upstream Go separates configuration types from normalization,
while the Rust port deliberately consolidates them. Candidate accounting must
therefore remain file-based in the ledger but may share one Rust implementation
and one parity gate when that implementation proves all three changed files.

### Workers 18gn–18gp — API and Management ownership closure

- Nine API production and five test mirrors close server routing, middleware,
  reload and RESP2 queue behavior. Redis, reload hooks and logging metrics are
  injected authorities; HTTP exposes explicit provider/control-plane routes
  while Business OS and RxDB records remain excluded from the data plane.
- OAuth Sessions close one production and one test mirror with an
  instance-owned clock, TTL/tombstones, completion guards and redacted plugin
  metadata. Management Auth Files close three production and seven tests with
  typed secret-free projections, injected store/runtime/provider authorities
  and atomic plugin-expansion rollback.
- API passes 102 focused tests in both matrices and 55 exact pinned-Go tests.
  Auth Files pass 19 focused tests plus the complete 2273/3 ignored no-default
  and 2308/3 ignored all-features matrices. OAuth passes 6 focused tests per
  matrix and 12 pinned-Go tests. Both strict Clippy matrices and formatting are
  green on the integrated tree.
- Strict closure reaches 578/605 production and 388/418 tests; marker-based
  classification reaches 599/605 and 408/418. Only five production scaffolds,
  one partial production mirror and ten test scaffolds remain mechanically
  open.

Forensic finding: API parity does not justify broad process authority. Queue,
reload, OAuth and credential-file behavior remain separately injected even
when upstream places them behind one server package; this preserves bounded
CTOX ownership and keeps future upstream deltas reviewable by module.

### Worker 18gq — Candidate Claude Code instruction asset

- The candidate embedded instruction identifies Claude Code as Anthropic's
  official CLI and removes the explicit one-hour ephemeral-cache TTL. The
  mirrored text resource and exact-payload regression are updated together.
- Rust source text keeps a conventional final newline, while the exported
  embedded wire value trims only trailing ASCII whitespace. A candidate-blob
  comparison and the focused Rust test in both feature matrices prove the
  effective payload; scoped formatting and diff checks pass.
- Candidate review reaches 11/74 files with 0/10 global gates. The accepted
  upstream pin remains unchanged.

Forensic finding: repository text-file conventions must not leak into embedded
protocol bytes. Normalizing the final source newline at the include boundary
keeps readable source files and still matches the candidate's JSON payload.

### Worker 18gr — Candidate Claude adaptive-effort invariant

- The candidate thinking-matrix delta requires every Claude output containing
  `output_config.effort` to also contain `thinking.type=adaptive`. The Rust
  mirror now proves the pair for low, medium, high and max effort suffixes.
- The focused Rust regression passes in both feature matrices, as does the
  candidate Go `TestThinkingE2EClaudeAdaptive_Body`; scoped formatting and diff
  checks are green. Candidate review reaches 12/74 with no global gate or pin
  promotion claimed.

Forensic finding: effort is not an independent Claude wire control. Testing it
as a pair prevents an otherwise valid-looking conversion from producing a
shape that native Claude Code never sends.

### Workers 18gs–18gu — Mechanical mirror closure

- OAuth Callback closes one production and two tests with injected session and
  callback authorities. Plugins/Store close two production and four tests with
  staged rollback, revisioned configuration and injected store/runtime
  ownership. Management Config/Vertex close three production and four tests
  with typed provider mutation, secret-free auth-index views and injected
  service-account persistence.
- The integrated tree passes 2300/3 ignored no-default tests and 2335/3 ignored
  all-features tests, both strict Clippy all-targets matrices, exact pinned-Go
  Management selections, formatting and diff checks.
- Marker-based coverage is now 605/605 production and 418/418 tests: no
  scaffold or partial marker remains. Strict checkpoint credit is 584/605 and
  398/418, so the Base port remains `in_progress` while the remaining 21
  production and 20 test mirrors are mapped to reconstructable or newly run
  full gate evidence.

Forensic finding: zero scaffolds is necessary but not sufficient for declaring
the port finished. Mechanical completeness and strict evidence completeness are
separate ledgers; the dashboard must show both until every adapted file has a
reproducible gate receipt.

### Worker 18gv — Strict-credit membership audit

- The 21-production/20-test strict gap is audited against the full port map,
  completed-worker ledger and narrative checkpoints. Twelve production and
  five test paths are reconstructable: safe host adapters, Config authority,
  three fail-closed command adapters, the large service-registry integration
  mirror and two translator benchmarks.
- Historical cluster-only checkpoints do not preserve membership for the
  remaining nine production and fifteen test credits. Their paths are not
  guessed. `strict-credit-audit.json` records both the exact reconstructed
  paths and the unresolved arithmetic membership.
- Reconstructed clusters require focused accepted-pin/Rust gates. Unresolved
  membership may close only through a reproducible umbrella receipt covering
  the root Go module, all nested Go modules, both Rust matrices, both strict
  Clippy matrices, formatting, tracking and dashboard generation.

Forensic finding: cumulative counters are not a durable file ledger. A future
porting checkpoint must record the exact credited path list so arithmetic drift
cannot survive after all mechanical markers close.

### Workers 18gw–18gy — Reconstructed strict closure

- Config's five internal production mirrors, its plugin-config test and the SDK
  facade close through one `TypedConfigSink` and caller-constructed
  `FileConfigDocument`. Internal YAML and SDK writes now delegate to the same
  injected persistence core; no environment, HOME, cwd or autonomous filesystem
  discovery remains.
- `ServiceRuntimeGraph` closes the large service integration mirror with owned
  Home lifecycle, shared Home/Watcher apply arbitration, Config/Auth projection,
  selector identity preservation and a generation-fenced reusable log
  forwarder. Two translator benchmark mirrors close by separating deterministic
  semantic tests from explicitly ignored manual stress loops.
- Browser, HTTPFetch and SSH close behind injected authorities. The three
  command cores are public Send+Sync library ABIs consumed by explicit binders
  in the outer CTOX host; thin binaries remain fail-closed without that host.
- The integrated crate passes 2314/3 ignored no-default tests and 2349/3
  ignored all-features tests, both strict Clippy all-targets matrices, pinned-Go
  Config/SDK/Service/Translator/Browser/HTTP/Util/Command packages, formatting,
  tracking and `cargo check --bin ctox`.
- Strict closure reaches 596/605 production and 403/418 tests. Only the
  historical nine-production/fifteen-test membership gap remains; every
  path-reconstructed gap is now closed.

Forensic finding: fail-closed adapters become strict only when a real owner can
consume their capability ABI. Moving command cores into the library graph and
binding them from CTOX closes authority, while retaining thin fail-closed bins
prevents accidental ambient execution.

### Worker 18gz — Accepted-pin strict umbrella closure

- A new `strict-umbrella-receipt.v1` derives all 23 Go modules from the accepted
  Git tree: the root module plus 22 nested plugin-example modules. Missing
  nested `go.sum` material is hydrated only in an archived sandbox; every module
  then passes offline with `-mod=readonly`, while the accepted checkout remains
  byte-clean at `ffdb9c9` before and after the run.
- The same receipt records 2314 passing no-default Rust library tests and 2349
  passing all-feature tests, with three intentional stress ignores in each
  matrix; both Clippy all-targets matrices pass with warnings denied. Formatting,
  the isolated outer `cargo check --bin ctox`, tracking and standalone dashboard
  generation are green.
- Every subgate has a SHA-256-bound log. The receipt also binds a Rust source
  manifest and the hydrated sandbox Go-module manifest. The independently
  revalidated receipt closes the historical arithmetic-only nine-production and
  fifteen-test gap without inventing per-file membership. Accepted-pin strict
  closure is now 605/605 production and 418/418 tests.
- Candidate promotion remains separate and blocked. A forensic comparison found
  that the current 74-entry candidate review omits 37 newly added paths from the
  actual 111-path diff; its inventory must be repaired before further promotion
  claims or global candidate gates.

Forensic finding: an umbrella gate must isolate both test roots and Cargo target
state. A long in-repository TMP path broke bound Git-filesystem tests, a shared
Cargo target suffered an incremental-file race, and forcing Go `-parallel=1`
can deadlock tests that deliberately synchronize parallel children. The durable
runner now uses a canonical system temp root, an exclusive Cargo target and
serial modules without overriding test-internal parallelism.

### Worker 18ha — Candidate inventory reconciliation

- The candidate generator dropped every added path because `select(length >
  0)` inside the `old_upstream` object value suppressed the entire jq record
  for an empty old path. The fixed representation uses explicit `null` and a
  fail-closed conservation invariant requires raw Git records, normalized rows
  and emitted JSON changes to have identical counts.
- Candidate `a88197f` therefore contains 111 paths rather than 74: 60 production
  Go files, 46 Go tests and five non-Go changes. A synthetic Git fixture locks
  added, modified, deleted and renamed records plus nested manifest, build,
  documentation, runtime-asset and other classifications.
- The legacy 74-entry delta, review and impact summary are retained beneath
  `legacy-inventory-74/`. A generic reconciler replays completed work only by
  unique upstream key. Nine completions replay directly; three Config
  completions that legacy positional updates had attached to API paths are
  explicitly remapped with reasons and hashes. No array-index replay is
  permitted.
- The corrected ledger is 12/111 complete with 99 pending. All ten global gates
  and their evidence are reset fail-closed. Candidate preparation is again
  idempotent, and no accepted pin or promotion state changes.
- The accepted-pin receipt checker now binds the receipt commit to the audit pin
  and confines receipts to the expected commit-local runtime subtree. Its source
  manifest remains deliberately historical: pre-promotion candidate work may
  change the current Rust tree without rewriting prior accepted-pin evidence.

Forensic finding: inventory conservation must be proven before semantic review
starts. A review ledger may preserve operator work across regeneration only by
stable path identity; any evidence recovery from a corrupt positional write
requires an explicit, inspectable remap and a full promotion-gate reset.

### Worker 18hb — Candidate `propertyNames` regressions

- The candidate's Antigravity test delta is ported across both reported schema
  nestings, every declaration/generation schema location, both declaration
  container spellings and both sanitizer modes. Gemini and Claude outbound
  bodies lose the unsupported keyword, while history arguments and a real
  property named `propertyNames` remain data.
- The Claude Messages ingress regression ports the exact two-tool shape and
  proves declarations remain usable after cleaning, including a property named
  `properties` whose nested schema keyword is removed.
- Six focused Rust cases pass with no default features and all features;
  formatting, accepted-pin anchors and tracking pass. The isolated candidate-Go
  run was aborted during dependency hydration and is therefore not claimed as
  executed evidence.
- Candidate review advances from 12/111 to 14/111. All ten promotion gates stay
  pending and the accepted pin remains unchanged.

### Workers 18hc–18hd — Candidate helper, identity and translator wave

- Two complete Claude helper pairs add a strict first-party Anthropic-origin
  predicate and the upstream HMAC-SHA256/Base32 MCP-alias protocol. Their
  focused Helps suite passes 34/34 in both Rust feature matrices.
- Diagnostics ports credential/session identity, monotone generations and
  commits, one-hour TTL, periodic cleanup and bounded LRU eviction. A forensic
  correction moved TTL regeneration out of the cleanup interval; five focused
  tests pass in both matrices.
- Claude identity ports canonical one-device pools, five-to-one migration,
  repair, defensive metadata ownership and stable selection. Rust's exclusive
  map ownership plus the aggregate-owner lock replaces the Go global map
  mutex; eight focused tests pass in both matrices.
- All three production and three test deltas under `internal/translator/claude`
  are closed: developer/system lifting, source ordering, cache precedence,
  fallback markers, redacted-thinking replay and shared stream/non-stream
  carriers. The module suite passes 49/49 in both matrices.
- The token fingerprint helper is completed behind a typed request-local
  observer; its two tests pass in both matrices. `home_result` remains pending
  because its required Usage record delta was not yet ported; no contract was
  invented.
- Candidate-only files use `Candidate-Port-Status` and do not change accepted
  closure counters. Review advances to 29/111 with 82 pending, zero global gates
  and no promotion.

Forensic finding: compile a parallel implementation wave once in a shared,
exclusive target. Three fresh targets saturated `/Volumes/tmp`; one
non-incremental target built in 82 seconds and then executed every focused
filter cheaply. Candidate files must use `candidate-ref`, never replace the
accepted-pin `ref` namespace.

### Worker 18he — Candidate API/cache evidence and deletion staging correction

- The Candidate Alpha Search route is reviewed against the exact upstream
  401-refresh diff. The typed Rust client rebuilds the request from refreshed
  auth, retries once, terminates on a second 401 and rejects API-key routes
  without an explicit base URL. Seven focused API tests pass.
- The new bounded LRU production/test pair is reviewed byte-semantically:
  capacity clamping, single creation under contention, recency and post-lock
  eviction callbacks are covered by three focused tests.
- These four paths advance the canonical Candidate ledger from 29/111 to
  33/111; 78 remain pending and all ten promotion gates remain false.
- A premature physical deletion of `claude_system_prompt.rs` broke the
  Accepted-Pin mechanical closure. The file is restored with an explicit
  Candidate `deleted` anchor. Deletions must remain staged until the
  transactional pin-promotion step; Candidate intent must never mutate the
  accepted mirror early.
- Tracking deliberately stopped dashboard generation first on the Accepted
  closure mismatch and then on stale project-state counters. This proves the
  dashboard must be derived only after both the accepted mirror and Candidate
  ledger agree; neither guard may be bypassed.

### Worker 18hf — Isolated Claude Candidate review

- The Candidate Go oracle passes `go test ./internal/runtime/executor/...` for
  both `executor` and `executor/helps` at `a88197f`.
- Twelve independently reviewable paths are credited: request-auth race/tests,
  the beta-order matrix, active diagnostics production/tests, active Fast-error
  production/tests, MCP-remap/thinking-signature/wire-casing tests, and active
  final-body signing plus known vectors.
- The current no-default Rust binary passes 61 Claude-executor tests and four
  signing tests. Credit is path-specific; this does not close the large
  executor, cloaking, count-token, device-profile, Home-refresh or uTLS
  production cutovers.
- Candidate review advances from 33/111 to 45/111 with 66 pending. Promotion
  remains false and all ten promotion gates remain pending.

### Worker 18hg — Claude provider request-context cutover

- The real provider adapter now carries incoming headers, original payload,
  prepared auth metadata and auth attributes through the account pool into the
  selected subscription executor. Detection, session identity, credential
  metadata and device profile are therefore request-scoped instead of helper-
  only behavior.
- Candidate cloaking is fail-closed: a copied `claude-cli` User-Agent is not a
  native-client proof. Only strong detection bypasses cloaking, even under an
  `always` operator policy; `never` remains the explicit unconfirmed bypass.
- Two end-to-end provider-path tests and one direct policy regression pass.
  Installed Messages header order and the Antigravity one-refresh replay are
  also green. These implementation results do not receive file credit until
  each large Candidate path has a complete forensic disposition.

### Worker 18hh — Active Alpha Search TCP routing boundary

- The TCP server owns an object-safe auxiliary-route boundary and dispatches
  the Alpha Search compatibility alias through the injected handler. Request
  sanitizing, response status/header/body fidelity and method gating are tested
  over real loopback connections.
- Seven Alpha Search tests and both Rust feature-matrix checks pass. The
  accepted compatibility API remains fail-closed when no auxiliary authority
  is installed.
- The CTOX supervisor does not yet construct the production Alpha Search
  selector/transport, so no new Candidate path is credited here. Live and
  Sideband remain open until HTTP upgrade, stream ownership and a host-bound
  session/auth authority exist; a buffered 101 response is not acceptable.

### Worker 18hi — Candidate Home and SDK execution wave

- Fourteen Candidate paths are closed across Home client/request transport,
  Redis queue production/tests, redacted Claude SDK auth, Home conductor,
  execution, concurrency, result, selection and unauthorized-refresh behavior,
  plus the usage manager.
- Forensic review found and fixed two real mismatches: a refreshed auth result
  without serialized runtime data now retains the original runtime authority;
  result-only 401 usage is explicitly non-generating and carries alias,
  reasoning-effort and service-tier metadata.
- The current Rust tree passes 24 Home-core, 11 Redis queue, two Claude SDK,
  38 Home-conductor and 17 usage tests. The corresponding six Candidate Go
  packages pass at `a88197f`.
- Review advances from 45/111 to 59/111, or 53.2% of the Candidate file-review
  inventory. This percentage is not the final completion predicate; all ten
  gates, promotion and the post-promotion full gate remain open.

### Worker 18hj — Candidate ordered HTTP wire cutover

- The Candidate ordered-connection production/test pair is closed. Rust keeps
  the parser/writer as a direct semantic port for fixed, chunked, keep-alive
  and partial-write behavior, with five passing no-default tests.
- The active Claude Messages transport does not insert that raw connection
  wrapper beneath wreq. It expresses the same captured header sequence through
  `OrigHeaderMap` on the production `RequestBuilder`; a default-feature
  integration test proves the Candidate order is installed.
- Review advances from 59/111 to 61/111, displayed as 55.0% of the Candidate
  file-review inventory. Promotion remains false and every gate remains open.

### Worker 18hk — Candidate Claude OAuth core wave

- Six Candidate paths close the OAuth identity bundle, native exchange/refresh
  flow, its expanded test matrix, stacked response decoding production/tests
  and typed token storage. Profile and roles lookups remain advisory, profile
  identity wins when present, and missing refresh fields preserve prior values.
- Candidate `go test ./internal/auth/claude` passes. The current default-feature
  Rust Claude-auth filter passes 73 tests, including every Candidate-specific
  exchange, refresh, compression, identity and storage regression.
- The uTLS production/test pair stays pending: shared bounded session-cache
  shape and a real stalled-handshake timeout are proven, but the Rust suite has
  not yet demonstrated a completed second TLS handshake with actual resumption.
- Review advances from 61/111 to 67/111, displayed as 60.4% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hl — Candidate Claude OAuth real TLS resumption

- The OAuth uTLS production/test pair is closed only after adding the missing
  behavioral proof. A production-shaped wreq/BoringSSL client connects twice
  to a rustls loopback server through one bounded session cache; the server
  observes a full TLS 1.3 handshake followed by `HandshakeKind::Resumed` on a
  separate TCP connection.
- The focused ten-test suite also proves captured request-header order,
  Node/OpenSSL TLS options, proxy cache sharing/bounds, actual exchange/refresh
  requests and bounded stalled-handshake termination.
- Review advances from 67/111 to 69/111, displayed as 62.2% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hm — Candidate config impact and OAuth metadata

- The complete Candidate `config.example.yaml` delta is dispositioned through
  `CANDIDATE_CONFIG_IMPACT.md`. Each example change maps to one active typed
  CTOX authority; no plaintext, ambient or duplicate configuration path is
  introduced.
- The management OAuth save path carries nonempty account, organization and
  device identity metadata without exposing token material. Candidate Go's
  focused management filter and five focused Rust config/metadata tests pass.
- Review advances from 69/111 to 71/111, displayed as 64.0% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hn — Candidate Claude helper forensics

- Twelve Candidate paths close the builtin-tool registry, strong native-client
  detector, typed credential identity including its concurrent ownership test,
  measured device profiles, active cloaking utility adaptation and the
  instance-owned user-ID cache, together with their test mirrors.
- Current focused Rust suites pass 5 builtin, 4 detection, 6 credential, 13
  device, 3 cloak and 11 cache tests. Candidate focused Go tests pass, and the
  two shared-credential tests additionally pass under the Go race detector.
- The same review found two real gaps and deliberately gives them no credit:
  Count Tokens still lacks a typed fingerprint/header-order transport
  capability, and Usage Records do not yet carry the refreshed access-token
  fingerprint.
- Review advances from 71/111 to 83/111, displayed as 74.8% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18ho — Candidate obsolete static Claude prompt removal

- Candidate deletes the static Claude Code 2.1.63 prompt constants. Rust still
  carried an unused private copy behind `dead_code`; both that file and its
  module declaration are now removed.
- The active Candidate cloaking pipeline remains the sole owner of dynamically
  assembled billing, identity and system material. No production consumer was
  removed. No-default library check and diff check pass.
- Review advances from 83/111 to 84/111, displayed as 75.7% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hp — Candidate Codex Live and Sideband refresh core

- Live bootstrap and Sideband now close their Candidate deltas: selected auth
  is pinned to the session, a 401 is reported, refreshed and retried exactly
  once, a second 401 remains terminal, and failed/drop paths release media and
  session claims.
- The current Rust Live module passes 32 tests and the Candidate Go Live package
  passes. This file review does not claim that the CTOX host already accepts an
  HTTP upgrade; production listener activation remains a separate fail-closed
  promotion requirement.
- Review advances from 84/111 to 87/111, displayed as 78.4% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hq — Candidate generic non-Home Conductor

- `GenericAuthRuntime` closes the six Candidate selection, refresh, execution,
  stream, cancellation and fast-error paths. It owns serialized preparation,
  unary/count/stream dispatch, exactly one OAuth-401 replay before commit and
  typed cooldown/outcome updates.
- Seven new E2E cases, the 74-test Conductor filter, Service construction,
  no-default check and Clippy pass. The compatibility Raw Registry APIs remain;
  production route callers are being cut over separately before promotion.
- Review advances from 87/111 to 93/111, displayed as 83.8% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hr — Candidate usage access-token fingerprint

- Usage Records now carry the request reporter's current access-token SHA-256
  rather than an empty placeholder. Initial load and buffered/streaming refresh
  update the same reporter; invalid or raw input clears the value fail-closed.
- Thirty-six focused Usage tests and two 401-refresh integrations pass, as does
  the Candidate Go oracle. Records and Debug output contain no token material.
- The three Antigravity Candidate paths remain pending until Count Tokens also
  receives an actively constructed fingerprint sink.
- Review advances from 93/111 to 94/111, displayed as 84.7% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hs — Candidate Antigravity fingerprint propagation

- Buffered, streaming and token-count execution publish the actually loaded
  Antigravity access-token SHA-256 and update it immediately after the one
  allowed 401 refresh. The injected sink is typed; absence is a safe no-op.
- Buffered and streaming refresh integrations pass. A new Count Tokens test
  proves that only the 64-character digest reaches the sink and the token never
  does. Candidate focused Go executor tests also pass.
- Review advances from 94/111 to 97/111, displayed as 87.4% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18ht — Candidate Claude active main executor forensics

- Seven Candidate main paths close the transport envelope, active cloaking,
  unary/stream execution, native request transport, large alias-rewrite smoke,
  provider stream adapter and behavior matrix. The Host builds the same pool
  and Node/OpenSSL transport that the tests exercise.
- Claude executor tests pass 88/88, a real Host pool loopback passes 1/1 and the
  Candidate-adapted differential probe passes 6/6; Candidate Go's executor
  package also passes.
- `claude_executor_auth.go` remains deliberately pending: its profile/device
  preparer is implemented but not yet registered in the active Host path.
- Review advances from 97/111 to 104/111, displayed as 93.7% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hu — Candidate injected Home refresh authority

- Home refresh closes as an injected authority rather than an upstream-style
  mutable package global. It sends auth index plus access-token SHA-256,
  preserves cancellation/deadline identity, redacts transient errors, maps
  terminal statuses and rejects disabled replacement credentials.
- The focused Rust suite passes 11/11 and the Candidate focused Go suite passes.
- Review advances from 104/111 to 106/111, displayed as 95.5% of the Candidate
  file-review inventory. Promotion remains false and all gates remain open.

### Worker 18hv — Candidate Claude Auth, CountTokens and native TLS closure

- The active Host factory now attaches `ClaudeRequestAuthPreparer` before
  CountTokens, unary and streaming requests. Two real pool executions fetch
  the OAuth profile exactly once, derive account/device identity and keep the
  access token exclusively inside `ClaudeSubscriptionAuth`.
- `/v1/messages/count_tokens` is an active auxiliary route using one selected
  account lane, the pool-owned wreq/BoringSSL transport, all 21 Candidate
  header positions, Candidate beta semantics and exactly one 401
  refresh/rebuild without account switching.
- Six isolated CountTokens tests, the real Host factory test, Claude 88/88,
  six differential fixtures and both Candidate Go packages pass. The shared
  Rust transport additionally proves real TLS 1.3 resumption and bounded
  session/cache ownership.
- Review advances from 106/111 to 111/111, displayed as 100.0% of the Candidate
  file-review inventory. This is not project completion: all ten attested
  promotion gates, explicit promotion and the post-promotion full gate remain.

## Known translation rules

- Go `context.Context` becomes `TranslationContext` at the synchronous format
  boundary and Tokio cancellation at I/O boundaries.
- `[]byte` remains byte-oriented. `serde_json::Value` is not permitted on a
  no-op path that promises byte identity.
- `*any` stream state becomes a per-request `TranslationState`; global mutable
  translation state is prohibited.
- Go package `init()` becomes an explicit, testable registration function.
- Go plugin/cgo files may be mirrored but must be marked `replaced_by_ctox` when
  the safe out-of-process boundary lands.
