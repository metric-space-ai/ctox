# Codex Desktop worker delegation on Michael's Mac

Analyzed, bounded implementation subtasks can run as separate Desktop tasks using
Grok 4.6, GLM 5.3 Flash or Kimi K3 through the existing CLIProxyAPI. OpenAI tasks
retain their direct provider. This is an operator-side Codex Desktop workflow,
separate from the embedded CTOX harness and Business OS coding sidecar.

Canonical instructions and helper:
[`src/tools/codex-desktop-workers/SKILL.md`](../src/tools/codex-desktop-workers/SKILL.md).
Install that directory as `~/.codex/skills/proxy-model-workers/` and append its
`GLOBAL-INSTRUCTIONS.md` as a clearly marked section in `~/.codex/AGENTS.md`.
Do not install from an expiring symlink into a tmp worktree: use durable copies.
The shared instructions govern future tasks; explicitly include them when
assigning or correcting a task that was already running.

The helper prepares a saved task with an explicit model/provider and a validated
existing tmp worktree and records the task ID durably before running a no-tool,
READY-only preparation turn. This bounded model request (at most 90 seconds)
materializes the rollout file, which `thread/start` alone creates lazily. Only a
successful completed turn with a real rollout file marks preparation ready.
Failures retain the existing task ID and recovery guidance; do not rerun create
or dispatch implementation until that preparation is recovered. The helper always
stops its temporary app-server.
The parent dispatches the prompt with the Desktop `send_message_to_thread` tool.
This keeps the normal Desktop process responsible for execution and subsequent
corrections. The app's model menu alone is not a provider selector.

The helper also assigns the canonical parent project through `projectId`.
The current built-in task listing filters to the global provider, so proxy
workers are temporarily pinned for visibility and unpinned after archival.
Their legacy UI project label can be empty despite the correct backend
assignment. A raw `thread/list` inventory with `modelProviders: []` includes
them. This limitation is documented rather than hidden behind a model alias.
The supported `desktop.git-worktree-root` setting routes native managed
worktrees to `/Volumes/tmp/worktrees`; existing system-disk worktrees are not moved.

Worker registry: `~/.codex/proxy-workers/jobs/<thread-id>.json` and corresponding
private prompt files. Each worker is disposable and owns exactly one PR. It waits
for the parent review and makes corrections in that same PR. Before merging,
the parent records a retrospective for the exact reviewed PR head and checks
the shared `~/.codex/proxy-workers/MODEL-EXPERIENCE.md`. After merging, the parent
checks `worker.py ready-to-archive`, verifies the task is idle, archives through
the app tool, and records success using `worker.py archived`. This is a
required part of the parent's completion step, not a scheduled monitor. Do not
infer merge from task prose, PR closure or a local branch. Workers may not merge.
Do not force-remove worktrees or touch unrelated processes. Failed, dirty or
running workers remain visible for intervention.

Rate limits and subscription quotas are temporary availability observations,
not evidence of poor model quality. Record a known reset timestamp, or defer
selection for 24 hours when it is unknown. Expired deferrals become eligible
automatically. A later unchanged review preserves earlier quality findings.

The connection probe on Codex 0.153.4 exercised two turns and a function-tool
round trip for each proxy model; an OpenAI control task made zero proxy requests.
The probe used conservative 128k context metadata and did not establish each
provider's maximum context or compatibility with every Desktop tool. Provider
metadata and observed requests establish routing; model self-identification does
not. Grok reasoning was changed to high and verified in its Desktop turn context.
