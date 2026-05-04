# CTOX

CTOX is an agentic daemon for long-running technical work.

It is built for people who already use coding agents and notice where a single
agent session stops being enough: real work spreads across tickets, servers,
communication, approvals, waiting states, failed checks, follow-ups, and context
that must still be correct tomorrow.

CTOX does not try to be "another better coding agent". It owns the layer around
agent execution: durable work state, context assembly, queue and ticket
management, verification, process evidence, communication, and continuation.

## Links

- Project page: <https://metric-space-ai.github.io/ctox/>
- Technical documentation: <https://metric-space-ai.github.io/ctox/docs.html>
- CLI reference: <https://metric-space-ai.github.io/ctox/cli.html>
- Releases and binaries: <https://github.com/metric-space-ai/ctox/releases>

## Quick Introduction

Coding agents are good at bounded sessions. CTOX is for the work around those
sessions.

A CTOX instance runs on a workstation, server, or remote host. You give it work
through the TUI, `ctox chat`, mail, tickets, schedules, or other configured
channels. The daemon records that work in durable state, builds the next worker
context from the runtime database, lets an agent perform a bounded run, records
the result, and decides whether the work is done, blocked, waiting, scheduled,
or needs another continuation.

The important unit is not a chat transcript. The important unit is the runtime
state:

- current work, queue items, plans, schedules, and follow-ups
- tickets, cases, approvals, writebacks, and audit history
- Focus, Anchors, Narrative, knowledge, claims, and recent communication
- verification records and process-mining events
- a core state model with transition checks and process evidence

That is the practical difference: CTOX uses agents, but CTOX itself is the
daemon that keeps technical work organized over time.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh | bash
```

The installer creates a managed layout by default:

- install root: `~/.local/lib/ctox`
- state root: `~/.local/state/ctox`
- cache root: `~/.cache/ctox`
- binary symlink directory: `~/.local/bin`

Most first-time users should not pass installer flags. Install CTOX first, then
open the TUI with `ctox` and configure model source, API keys, local inference,
context window, autonomy, and communication there.

Only use installer flags during the first install when you already know you need
to override hardware detection or seed a specific local model:

```sh
curl -fsSL https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh \
  | bash -s -- --backend=metal
```

First-install overrides:

| Flag | When to use it |
| --- | --- |
| `--backend=<cuda\|metal\|cpu>` | Forces the local inference backend. Leave it unset unless auto-detection is wrong or you intentionally want CPU fallback. `cuda` is for NVIDIA Linux hosts, `metal` is for Apple Silicon macOS, and `cpu` is the slow fallback. |
| `--model=<model>` | Seeds the default local model profile. Do not use this during a first install unless you know the exact model id is supported by the current build; model selection can be changed later in the TUI. |
| `--api-provider=<provider>` | Seeds a remote provider for first boot. Most users should configure this in the TUI; currently useful for scripted Azure Foundry installs. |
| `--azure-foundry-endpoint=<url>` | Seeds the Azure Foundry resource endpoint. CTOX appends `/openai/v1` when needed. |
| `--azure-foundry-deployment-id=<id>` | Seeds the Azure Foundry deployment ID and uses it as the chat model. |

### Example: API model with key

Use this when CTOX should call an API-backed model instead of local inference:

```sh
ctox secret put --scope credentials --name OPENAI_API_KEY --value "sk-..."
ctox
```

In the TUI settings, select:

```text
Chat Source: api
API Provider: openai
Chat Model: gpt-5.4-mini
```

Supported API provider choices are `openai`, `anthropic`, `openrouter`,
`minimax`, and `azure_foundry`. Provider tokens should be stored through the
TUI or `ctox secret`; normal shell exports are not the recommended
configuration path.

### Example: OpenAI model with Codex subscription auth

Use this when CTOX should use the Codex/ChatGPT OAuth credentials already
stored by Codex instead of an `OPENAI_API_KEY`:

```text
API Provider: openai
OpenAI Auth: chatgpt_subscription
Chat Model: gpt-5.4-mini
```

In this mode CTOX ignores `OPENAI_API_KEY` for OpenAI direct sessions and does
not record those turns as OpenAI API spend. Other API providers still require
their provider tokens in the CTOX secret store.

For Azure Foundry, choose `azure_foundry` and enter:

```text
Foundry Endpoint: https://<resource>.openai.azure.com
Deployment ID: <deployment-id>
Foundry Token: <token>
```

CTOX normalizes the endpoint to `/openai/v1` when needed. The Deployment ID is
used as the request model name, which matches the Azure Foundry deployment
model flow.

Then start the daemon:

```sh
ctox start
ctox chat "Check this CTOX installation and confirm that the API model is reachable."
```

You can also enter the API key directly in the TUI. Do not use global shell
exports for normal CTOX configuration.

### Example: supported 27B model on CUDA

Use this on a Linux host with an NVIDIA GPU/CUDA setup and the required local
model artifacts for `Qwen/Qwen3.5-27B`:

```sh
curl -fsSL https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh \
  | bash -s -- --backend=cuda --model=Qwen/Qwen3.5-27B
ctox
```

In the TUI settings, select:

```text
Chat Source: local
Local Runtime: candle
Chat Model: Qwen/Qwen3.5-27B
```

Then verify and switch:

```sh
ctox doctor
ctox runtime switch Qwen/Qwen3.5-27B quality --context 128k
```

If the CUDA backend, model weights, or runtime artifacts are missing,
`ctox doctor` or the TUI should show that before you assign real work.

Advanced installer options:

| Flag | What it changes |
| --- | --- |
| `--install-root=<path>` | Where the managed CTOX installation is stored. Use this for nonstandard filesystem layouts or multiple installs on one host. |
| `--state-root=<path>` | Where runtime state is stored, including the SQLite database. Use this when state must live on a specific volume or service account path. |
| `--cache-root=<path>` | Where downloaded models and cache files are stored. Use this when the default home cache does not have enough disk space. |
| `--bin-dir=<path>` | Where the `ctox` command symlink is placed. Use this if `~/.local/bin` is not on `PATH` or your system uses a different user-local binary directory. |
| `--repo=<url>` | Installs from a fork or custom repository. Normal users should keep the default repository. |
| `--branch=<branch>` | Installs from a non-default branch. This is mainly for development, testing, or controlled rollout of a fork. |

## First Run

```sh
ctox doctor
ctox
ctox start
ctox status
ctox chat "Check this CTOX installation, summarize what is configured, and list the next setup steps before taking on real work."
```

What these commands do:

- `ctox doctor` checks the installation and runtime environment.
- `ctox` opens the TUI for configuration and operation.
- `ctox start` starts the persistent daemon.
- `ctox status` shows the current service state.
- `ctox work-hours set 08:00 18:00` lets the daemon accept and start work only
  inside that local-time window; `ctox work-hours off` disables the guard.
- `ctox chat <instruction>` submits a small first check to the daemon.

Most users should start in the TUI, configure the model backend and credentials
there, then submit work through the TUI or `ctox chat`.

## How CTOX Runs Work

The daemon loop is roughly:

```text
intake
  -> durable queue item, ticket case, schedule, or plan step
  -> leased worker run
  -> context build from runtime state
  -> bounded agent execution
  -> verification, writeback, knowledge, and process events
  -> complete, blocked, waiting, scheduled, requeued, or continued
```

CTOX workers can call CTOX commands themselves. This is intentional: internal
tools such as `ctox ticket`, `ctox queue`, `ctox verification`, and
`ctox process-mining` make the daemon inspect and update its own runtime state
through an auditable command surface instead of relying only on prompt memory.

The command surface is documented in the
[CLI reference](https://metric-space-ai.github.io/ctox/cli.html). Many commands
are daemon tools first and human commands second; normal operation should happen
through the TUI, `ctox chat`, configured channels, tickets, and schedules.

## Model Backends

CTOX can run with API-backed models or with the integrated local inference path,
depending on the configured runtime.

Typical configuration is done in the TUI. Important runtime settings include:

- chat source: `api` or `local`
- API provider: `openai`, `anthropic`, `openrouter`, or `minimax`
- OpenAI auth mode: `api_key` or `chatgpt_subscription`
- provider credentials, stored through the TUI or CTOX secret store
- local runtime
- active chat model
- context window
- autonomy level

See the technical documentation for the current model/runtime details:
<https://metric-space-ai.github.io/ctox/docs.html#configuration>

The harness review, task-spawn, subagent, and liveness-proof model is documented
in [HARNESS.md](HARNESS.md#review-gate-spawner-und-subagent-liveness).

## Desktop App

The CTOX Desktop app is an optional management surface. It is useful when you
want to install CTOX locally, connect to remote CTOX instances, or manage
multiple instances from one place.

The core runtime is still the daemon. The Desktop app manages instances; the
daemon owns the work.

## Update

```sh
ctox update status
ctox update check
ctox upgrade --stable
ctox upgrade --dev
ctox update apply --version <tag>
ctox update rollback
```

`ctox upgrade --dev` follows the current `main` branch as a source upgrade.
`ctox upgrade --stable` follows the latest published release.

## Repository Layout

- `src/` - CTOX daemon, runtime, mission systems, TUI, model control, and tools.
- `src/harness/` - integrated in-process agent harness.
- `src/inference/` - local inference work.
- `skills/` - system skills used by CTOX workers.
- `tools/` - supporting tool packages.
- `site/` - GitHub Pages project site and documentation.
- `.github/workflows/` - CI, release, and Pages workflows.

## Development

```sh
cargo fmt --check
cargo check
cargo test
cargo run -- process-mining spawn-liveness
```

The repository contains platform-specific code paths for macOS, Linux, Windows,
and optional local inference backends. Use the release workflow for production
binaries. Release builds also gate on `ctox process-mining spawn-liveness`, which
checks both runtime task-spawn contracts and harness subagent liveness.

## License

[Apache License 2.0](LICENSE)

See [NOTICE](NOTICE) for attribution of integrated source trees.
