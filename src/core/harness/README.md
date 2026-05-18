# CTOX Inference Workspace

This directory is the in-process inference workspace that CTOX builds against.
It is no longer treated as a generic `tools/` dump. The intended structure is:

## Ring 1: Inference Kernel

These crates are the actual reusable inference/runtime surface:

- `core`
- `app-server`
- `app-server-client`
- `app-server-protocol`
- `protocol`

If CTOX starts a persistent in-process session, this ring is the critical path.

## Ring 2: Runtime Support

These crates are still required for CTOX to run, but they support the kernel
rather than being the kernel itself:

- `config`
- `state`
- `secrets`
- `cloud-requirements`
- `backend-client`
- `rmcp-client`
- `connectors`
- `file-search`
- `hooks`
- `shell-command`
- `shell-escalation`
- `apply-patch`
- `feedback`
- `otel`
- `execpolicy`
- `keyring-store`
- `ctox-api`
- `ctox-client`
- `ctox-backend-openapi-models`
- `ctox-experimental-api-macros`
- `package-manager`
- `artifacts`
- `async-utils`
- `utils/*`

This ring is where most future slimming work should happen. The goal is not to
delete it blindly, but to separate what is inference-specific from what is
general runtime/tooling support.

Bundled system-skill installation now lives directly in
`core/src/skills/system.rs` and embeds the canonical repo-root `skills/system`
tree. `src/inference` must not keep a second checked-in copy of those system
skills.

## Ring 3: Platform Support

These crates must remain because CTOX supports multiple host platforms:

- `arg0`
- `linux-sandbox`
- `windows-sandbox-rs`
- `network-proxy`

These are not legacy fallback code. They are platform/runtime integration code.

## What Was Removed

The previous `chatgpt` and `login` crates were part of a broader fork surface,
but they were not part of the CTOX inference kernel. Their runtime behavior has
been removed from the active CTOX inference path.

## Refactor Rule

Future cleanup inside `src/inference` should follow this order:

1. identify whether code belongs to kernel, runtime support, or platform support
2. reduce coupling between those rings
3. only then move or delete crates

That keeps CTOX cross-platform while still converging toward one coherent
inference architecture.
