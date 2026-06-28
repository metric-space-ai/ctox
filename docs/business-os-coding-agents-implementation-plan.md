# Business OS Coding Agents Implementation Status

Date: 2026-06-13

## Scope

Business OS Coding Agents is the CTOX control surface for external coding-agent
providers. The product contract is intentionally narrow:

- install or repair provider CLIs for OpenAI Codex, Google Antigravity, and
  Anthropic Claude Code
- delegate authorization to provider-owned login flows
- require explicit workspace grants before a provider can run against a path
- create, continue, list, inspect, and logically stop coding-agent sessions
- project provider sessions, events, and outcomes into Business OS
- show all supported providers through one Business OS app

This is not desktop UI remoting. CTOX does not attempt to drive every detail of
the desktop apps. It runs robust task sessions through provider CLIs and uses
the desktop apps/provider auth state where the provider exposes it.

## Current Verdict

The requested provider order is implemented and verified: Codex first,
Antigravity second, Claude third.

The local production v1 path is ready for controlled use on this Mac:
Core/CLI commands, provider adapters, Business OS command dispatch, WebRTC-only
browser-to-Rust execution, RxDB projections, schema contracts, provider auth
checks, workspace grants, real provider task execution, and the unified Coding
Agents UI have all passed end-to-end checks.

For wider release, the remaining work is release hardening rather than missing
core functionality: installer trust policy, CI/manual release gates for real
provider E2E, warning cleanup, async cancellation semantics, and Browser-app
input/runtime hardening if credential handoff is expected to use the Business
OS Browser app.

## Implemented Surfaces

Core and CLI:

- `src/core/coding_agents/mod.rs`
- `src/core/main.rs`
- `src/core/business_os/store.rs`
- `src/core/business_os/importer.rs`

Business OS app and command bridge:

- `src/apps/business-os/modules/coding-agents/index.html`
- `src/apps/business-os/modules/coding-agents/index.js`
- `src/apps/business-os/modules/coding-agents/schema.js`
- `src/apps/business-os/modules/coding-agents/module.json`
- `src/apps/business-os/modules/registry.json`
- `src/apps/business-os/shared/command-bus.js`

Schema, generated contracts, and RxDB cache:

- `src/core/business_os/business_os_schema_contract.json`
- `src/core/business_os/business_os_schema_hashes.json`
- `src/apps/business-os/rxdb/src/schema.mjs`
- `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`
- `src/apps/business-os/shared/db.js`
- `src/apps/business-os/shared/sync.js`
- `src/apps/business-os/modules/matching/ui/businessOsDataSource.js`

Tests and smoke harnesses:

- `src/apps/business-os/modules/coding-agents/tests/coding-agents.test.mjs`
- `src/apps/business-os/rxdb/tests/command-bus-projection-smoke.mjs`
- `src/core/rxdb/tools/browser_rust_smoke.js`
- `src/core/rxdb/tools/browser_rust_smoke_matrix.js`

Supporting production fixes made during verification:

- `src/apps/business-os/modules/buchhaltung/schema.js`
- `src/apps/business-os/modules/buchhaltung/collections.schema.json`
- `src/apps/business-os/modules/outbound/index.js`
- `src/core/service/service.rs`

## Command Contract

CLI:

```sh
ctox coding-agent providers
ctox coding-agent status --provider codex
ctox coding-agent install --provider codex
ctox coding-agent install --provider codex --apply
ctox coding-agent auth start --provider antigravity
ctox coding-agent auth status --provider claude
ctox coding-agent workspace grant --provider codex --path /abs/project
ctox coding-agent workspace revoke --provider claude --path /abs/project
ctox coding-agent session create --provider codex --workspace /abs/project --prompt "..."
ctox coding-agent session prompt --provider antigravity --session <ctox-session-id> --prompt "..."
ctox coding-agent session list --provider claude --workspace /abs/project
ctox coding-agent session get --provider codex --session <ctox-session-id>
ctox coding-agent session stop --provider antigravity --session <ctox-session-id>
```

Business OS command types:

- `ctox.coding_agent.status`
- `ctox.coding_agent.install`
- `ctox.coding_agent.auth.start`
- `ctox.coding_agent.auth.status`
- `ctox.coding_agent.workspace.grant`
- `ctox.coding_agent.workspace.list`
- `ctox.coding_agent.workspace.revoke`
- `ctox.coding_agent.session.create`
- `ctox.coding_agent.session.prompt`
- `ctox.coding_agent.session.list`
- `ctox.coding_agent.session.get`
- `ctox.coding_agent.session.stop`

The legacy `ctox.coding_agent.execute` shape remains supported as a
compatibility shim in Core. The Business OS app emits typed commands.

## Provider Adapters

| Provider | CLI | App bundle | Auth probe | Execution mode |
| --- | --- | --- | --- | --- |
| Codex | `/opt/homebrew/bin/codex`, `codex-cli 0.139.0` | `/Applications/Codex.app` | `codex login status` | `codex-cli` |
| Antigravity | `/Users/you/.local/bin/agy`, `1.0.8` | `/Applications/Antigravity.app` | `agy models` | `antigravity-cli` |
| Claude | `/opt/homebrew/bin/claude`, `2.1.163 (Claude Code)` | `/Applications/Claude.app` | `claude auth status` | `claude-code-cli` |

Codex task create:

```sh
codex -a never exec --json --sandbox workspace-write --cd <workspace> --skip-git-repo-check <prompt>
```

Codex task resume:

```sh
codex -a never -s workspace-write -C <workspace> exec resume --json --skip-git-repo-check <thread-id> <prompt>
```

Antigravity task create:

```sh
agy --log-file <temp-log> --print-timeout 600s --print <prompt>
```

Antigravity task resume:

```sh
agy --log-file <temp-log> --print-timeout 600s --conversation <conversation-id> --print <prompt>
```

Claude task create:

```sh
claude -p --output-format json --permission-mode acceptEdits <prompt>
```

Claude task resume:

```sh
claude -p --output-format json --permission-mode acceptEdits --resume <session-id> <prompt>
```

All adapters redact provider output before persistence and parse provider
session ids, assistant result text, terminal status, and available usage/cost
metadata.

## Installation And Auth

`install` without `--apply` is a safe discovery/plan command. It does not start
a remote installer.

`install --apply` is explicit user intent. It runs the provider-owned installer
and then rechecks provider discovery. Business OS exposes this as
`Install / Repair CLI`.

Installer plans currently wired:

| Provider | Docs | Apply command |
| --- | --- | --- |
| Codex | `https://developers.openai.com/codex/cli` | `curl -fsSL https://chatgpt.com/codex/install.sh \| CODEX_NON_INTERACTIVE=1 sh` |
| Antigravity | `https://antigravity.google/docs/cli-install` | `curl -fsSL https://antigravity.google/cli/install.sh \| bash` |
| Claude | `https://code.claude.com/docs/en/quickstart` | `curl -fsSL https://claude.ai/install.sh \| bash` |

Auth remains provider-owned:

- Codex: `codex login` or `codex login --device-auth`
- Antigravity: `agy` or the provider-owned prompt/login flow
- Claude: `claude auth login`

Business OS never accepts or stores provider passwords.

## Data Model

Host tables:

- `coding_agent_workspace_grants`
- `coding_agent_sessions`
- `coding_agent_events`

Business OS projections:

- `coding_agent_workspace_grants`
- `coding_agent_sessions`
- `coding_agent_events`
- terminal `business_commands` outcomes for `ctox.coding_agent.*`

Projection writes go through the native Business OS projection path, updating
both generic `business_records` and the provider-specific RxDB collection where
applicable. Command completion and failure outcomes are also written to the
native `business_commands` collection, so browser waiters can observe terminal
states reliably.

Schema contracts were regenerated from module fixtures. The app-local RxDB
bundle was rebuilt from source, and import cache versions were bumped. The dist
bundle was not patched directly.

## UI Behavior

- The Coding Agents module dispatches typed `ctox.coding_agent.*` commands
  through `business_commands`.
- Provider status, auth status, workspace grants, sessions, and events render in
  one app.
- Session create and prompt wait up to ten minutes, matching provider execution
  timeout.
- Workspace grants, sessions, and events render from `coding_agent_*`
  projections, with command-result fallback for empty local states.
- Provider-owned auth and explicit CLI install/repair are available from the
  settings modal.
- Command-bus errors surface structured provider stderr/error text.
- Legacy email/password auth inputs were removed.

## End-To-End Evidence

Real CLI provider sessions passed before the browser UI gate:

| Provider | CTOX session | Provider session | Create marker | Resume marker |
| --- | --- | --- | --- | --- |
| Codex | `ca_codex_8f22e13fd7644c818b15cbe67a0cb21a` | `019ec02b-6b49-7f43-96bd-24d1875380df` | `CTOX_E2E_CODEX_1781340521869_CREATE` | `CTOX_E2E_CODEX_1781340521869_RESUME` |
| Antigravity | `ca_antigravity_c91faafb15db46019098116a01fd884c` | `f3ebbf33-6292-4465-8d2b-8b0ef5b29b8b` | `CTOX_E2E_ANTIGRAVITY_1781340689887_CREATE` | `CTOX_E2E_ANTIGRAVITY_1781340689887_RESUME` |
| Claude | `ca_claude_e9691d513af34c6ea6abaf061d620030` | `a19f0d5e-bf92-41f0-b952-239a9da263a7` | `CTOX_E2E_CLAUDE_1781340726205_CREATE` | `CTOX_E2E_CLAUDE_1781340726205_RESUME` |

Browser-to-Rust Coding Agents UI E2E then passed for all three providers:

| Provider | UI session | Auth | Create marker | Follow-up marker | Event count | Browser errors |
| --- | --- | --- | --- | --- | --- | --- |
| Codex | `ca_codex_da5f81f0232e42cca4c6bd9dec27d0dc` | ready | seen | seen | 4 | 0 |
| Antigravity | `ca_antigravity_909fa744277647ce9d5b6137273b2c61` | ready | seen | seen | 4 | 0 |
| Claude | `ca_claude_53a118e978c346e298aaded70995ef4b` | ready | seen | seen | 4 | 0 |

The UI smoke path performed the representative workflow:

1. Open Business OS through the clean-profile browser harness.
2. Open the Coding Agents module.
3. Verify provider status and auth readiness.
4. Grant the workspace.
5. Create a real provider-backed session.
6. Send a follow-up prompt into the same CTOX session.
7. Wait for visible UI markers and RxDB projections.
8. Stop the session and revoke the workspace grant.

The broad `business-os-ui-regression` smoke also passed after cleanup:

- 21 modules opened and rendered.
- 22 start-menu items were visible.
- Primary modules checked: `ctox`, `documents`, `knowledge`, `research`.
- Secondary modules checked included `outbound`, `buchhaltung`, and
  `coding-agents`.
- Browser error count: 0.
- WebSocket failure count: 0.
- Request failure count: 0.
- Asset failure count: 0.

Additional Browser app checks:

- `browser-lifecycle-ui` passed after removing client-side optimistic runtime
  status writes: start, navigate, reload, back, forward, reset, and stop were
  accepted and projected, with zero browser errors.
- `browser-handoff-ui` passed: `ctox.browser_context.capture` produced queued
  CTOX tasks, and large frame data stayed out of command payloads.
- `browser-input-runtime` is still not green. The real runtime can produce
  frames, but the UI form/input navigation path did not reliably advance the
  tab URL from `probe=1` to `probe=2`. This means the Browser app is not yet a
  production-grade credential-handoff surface for arbitrary provider logins.

## Production Fixes Completed In This Pass

1. Added Coding Agents to the generated Business OS schema contract and hash
   registry.
2. Fixed Accounting schema generation by exporting
   `accounting_number_series`.
3. Rebuilt the app-local RxDB browser bundle from source and bumped cache
   versions.
4. Added a targeted `SMOKE_MODE=coding-agents-ui` browser harness for real
   provider E2E.
5. Added smoke-matrix evidence checks for `coding-agents-ui`.
6. Hardened CommandBus terminal outcome handling so direct control-command
   results are observable from the browser.
7. Fixed Outbound Knowledge setup so expected missing tables are created or
   restored without noisy native failures.
8. Ensured Outbound importer paths create the required Knowledge contract
   before direct table reads/writes.
9. Fixed a Rust compile blocker in the dirty service validation code path
   without reverting unrelated local work.

## Verification

Passed JavaScript and module checks:

```sh
node --check src/apps/business-os/modules/outbound/index.js
node --check src/apps/business-os/modules/coding-agents/index.js
node --check src/apps/business-os/shared/command-bus.js
node --test src/apps/business-os/modules/outbound/outbound.test.mjs
node --test src/apps/business-os/modules/coding-agents/tests/coding-agents.test.mjs
node src/apps/business-os/rxdb/tests/command-bus-projection-smoke.mjs
node src/apps/business-os/scripts/assert-rxdb-only.mjs
node src/core/rxdb/tools/build_business_os_module_schema_files.mjs
node src/core/rxdb/tools/build_business_os_schema_contract.mjs
node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs
node src/apps/business-os/rxdb/tests/bundle-reproducible-smoke.mjs
node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs coding-agents
```

Passed Rust checks:

```sh
cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target
cargo test --no-default-features --bin ctox --target-dir runtime/build/core-rxdb-integration-target native_all_schema_hashes_match_browser_contract_fixture -- --nocapture
cargo test --no-default-features --bin ctox --target-dir runtime/build/core-rxdb-integration-target coding_agents -- --test-threads=1
cargo build --bin ctox --target-dir runtime/build/core-rxdb-integration-target
cargo test --bin ctox --target-dir runtime/build/core-rxdb-integration-target native_all_schema_hashes_match_browser_contract_fixture -- --nocapture
cargo test --bin ctox --target-dir runtime/build/core-rxdb-integration-target coding_agents -- --test-threads=1
```

Passed real provider UI E2E:

```sh
CTOX_BIN=runtime/build/core-rxdb-integration-target/debug/ctox \
SMOKE_MODE=coding-agents-ui SMOKE_PAGE_PATH=/index.html \
SMOKE_CODING_AGENT_PROVIDER=codex BUSINESS_PORT=8913 SIGNALING_PORT=18913 \
node src/core/rxdb/tools/browser_rust_smoke.js

CTOX_BIN=runtime/build/core-rxdb-integration-target/debug/ctox \
SMOKE_MODE=coding-agents-ui SMOKE_PAGE_PATH=/index.html \
SMOKE_CODING_AGENT_PROVIDER=antigravity BUSINESS_PORT=8914 SIGNALING_PORT=18914 \
node src/core/rxdb/tools/browser_rust_smoke.js

CTOX_BIN=runtime/build/core-rxdb-integration-target/debug/ctox \
SMOKE_MODE=coding-agents-ui SMOKE_PAGE_PATH=/index.html \
SMOKE_CODING_AGENT_PROVIDER=claude BUSINESS_PORT=8915 SIGNALING_PORT=18915 \
node src/core/rxdb/tools/browser_rust_smoke.js
```

Passed broad Business OS UI regression:

```sh
CTOX_BIN=runtime/build/core-rxdb-integration-target/debug/ctox \
SMOKE_MODE=business-os-ui-regression SMOKE_PAGE_PATH=/index.html \
BUSINESS_PORT=8917 SIGNALING_PORT=18917 \
node src/core/rxdb/tools/browser_rust_smoke.js
```

## Remaining Production Hardening

These are the remaining items before broad distribution:

1. Installer trust policy: pin expected provider install locations, add
   checksum/signature verification where providers expose it, and provide clear
   rollback/error messaging per OS.
2. Real-provider release gate: keep the current `coding-agents-ui` smoke as an
   opt-in manual/release gate because it depends on local auth state and spends
   provider execution quota.
3. Background execution and cancellation: `session.stop` is currently a logical
   state transition. Long-running provider calls should move to an async worker
   with persisted process handles before claiming hard cancellation.
4. Warning cleanup: Cargo currently passes but emits many existing warnings.
   These are not Coding Agents blockers, but they should be reduced before a
   clean release signal.
5. Shell import retry warning: one broad UI run still showed shell import retry
   warnings before succeeding. This is not blocking the Coding Agents task flow,
   but it should be tracked because it adds noise to smoke results.
6. Browser app input/runtime hardening: lifecycle and handoff pass, but the
   form/input navigation smoke still fails. Do not rely on the Business OS
   Browser app as the primary Coding Agents credential handoff surface until
   `browser-input-runtime` is green.
7. Provider status projections: session, workspace, and event projections exist;
   provider status is still command-polled and can be promoted to persisted
   projection records if the UI needs historical status.
8. Budget normalization: provider usage and cost metadata are captured where
   available, but cross-provider quota/budget policy is not normalized yet.

## Final Readiness Classification

Production ready for controlled local Business OS v1 on this machine.

Not yet ready as an unattended, broad-distribution release until installer trust
policy, opt-in real-provider release gates, warning cleanup, and hard
cancellation semantics are completed.

## References

- OpenAI Codex CLI: `https://developers.openai.com/codex/cli`
- OpenAI Codex auth: `https://developers.openai.com/codex/auth`
- Google Antigravity CLI: `https://antigravity.google/docs/cli-overview`
- Google Antigravity CLI install: `https://antigravity.google/docs/cli-install`
- Anthropic Claude Code CLI reference: `https://docs.anthropic.com/en/docs/claude-code/cli-reference`
- Anthropic Claude Code quickstart: `https://code.claude.com/docs/en/quickstart`
