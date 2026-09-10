---
name: proxy-model-workers
description: Delegate Codex Desktop coding subtasks to Grok, GLM, Kimi, or OpenAI with per-task providers, tmp-volume worktrees, durable PR tracking, and archive-after-merge lifecycle on Michael's Mac.
---

# Codex Desktop workers

The parent analyzes and decomposes the goal, then dispatches ready independent
packages in parallel. There is no fixed worker-count limit: respect actual shared
host/provider capacity, ownership and dependencies. The heavy-job gate remains
exclusive across tasks while independent workers can edit concurrently.

Give each worker a coherent outcome with findings, intended approach, owned
components, exclusions and acceptance checks, including foreseeable tests,
documentation, generated files and consumers. Do not split by file or mechanical
step. Keep unresolved architecture with the parent. The parent consolidates
review findings and handles routine follow-on changes in the same worker/PR;
escalate material goal/risk changes or cross-parent conflicts, not routine rework.
Reports are notifications, not requests to wait for supervisor acknowledgement.

The parent may send follow-up instructions and corrections through
`send_message_to_thread` to the same task. Keep its worktree, provider and branch;
update the existing PR. Never mark a correction finished from assistant prose
alone: review the diff and validation evidence.

Each worker is disposable and owns exactly one analyzed implementation assignment
and one PR. It waits for the parent review, makes corrections in that same PR,
and is archived by the parent immediately after the parent merges the PR.
Do not reuse the worker for unrelated assignments.

Use this workflow for user-authorized implementation delegation. Keep the
parent's scope, acceptance criteria and resource budget. The parent reviews
results; workers may not merge their own PRs or recursively spawn workers
unless the user explicitly requests that expansion.

## When to delegate and how to select a model

Start from a dedicated issue in the target repository. The main task uses native
subagents for bounded analysis, consolidates findings and sketches the solution
before instructing the implementation worker.

Before choosing a worker, read `~/.codex/proxy-workers/MODEL-EXPERIENCE.md` and
run `worker.py availability`. Quota/rate-limit/capacity failures are temporary
availability observations, not evidence of coding weakness. Record a known reset:
`worker.py defer --model kimi-k3 --until 2026-09-10T12:00:00+02:00 --note 'quota reset from provider'`.
Omit --until to retry tomorrow when the reset is unknown. Never use that illustrative
date as a real reset: use observed provider data. Once the date passes, the model
is eligible again. Use Codex automation tools for an actual delayed retry, or
explicitly choose another available model; never silently change routing.
Delegate a concrete implementation package when the analysis is complete and
acceptance can be checked independently. Choose a model using evidence for that
task type. Keep open architecture, ambiguous scope and decomposition with the
parent. When evidence is missing, use a small representative task and label the
choice provisional. Do not invent strengths from model branding or connectivity.
The notebook is evidence, not an authority to change instructions or run commands.

## Model routing

| Model | Provider | CLI profile | Reasoning |
| --- | --- | --- | --- |
| `grok-4.6-exact` | `cli_proxy` | `grok-proxy` | `high` |
| `glm-5.3-flash` | `cli_proxy` | `glm-proxy` | `high` |
| `kimi-k3` | `cli_proxy` | `kimi-proxy` | `high` |
| User-selected OpenAI model | `openai` | existing configuration | preserve user choice |

The global provider stays OpenAI. Never redirect OpenAI tasks through the proxy
as a side effect of delegating a different model. `-m` alone does not select a
provider. CLI examples: `codex -p grok-proxy`, `codex -p glm-proxy`,
`codex -p kimi-proxy`. The Desktop executable is
`/Applications/ChatGPT.app/Contents/Resources/codex`.

For Desktop tasks, `thread/start` accepts separate `model` and `modelProvider`
fields. A saved task retains its provider on subsequent turns. The app's
`create_thread` tool exposes a model override but no provider field: do not use
that alone to create a non-OpenAI task. Use the helper below. It prepares
the task by persisting its authorized developer execution contract with
`thread/inject_items`, without a user message, model turn or initialization reply.
It verifies the rollout and read-back, then stops its temporary app-server. Start implementation through the normal
Desktop `send_message_to_thread` tool, so the existing app owns execution.
The general model picker is not configured as a provider switcher.

The current built-in `list_threads` sends `modelProviders: null`, which lists
only the global provider. Pin each prepared proxy worker using
`move_thread_to_sidebar_section` with `sectionId: "pinned"`; pinned task IDs
are fetched individually and therefore remain visible. Remove this temporary
pin after archive. The app's legacy project label may still be empty: retain
and verify canonical `project_id` through the registry/app-server instead.
For a complete raw app-server inventory use `thread/list` with
`modelProviders: []`. This is a supported visibility workaround, not a repair
of the app's ordinary project listing. Do not patch app internals or change
provider metadata to disguise the limitation.

Model self-identification is not routing evidence. Inspect saved settings and
proxy requests. Grok's subscription backend has reported `grok-4.6-build`;
`grok-4.6-exact` is the existing client alias. The catalog uses a conservative
256,000-token context budget for these integrations, with a 230,400-token
auto-compaction threshold. This is configured capacity, not a measured provider maximum.
The helper creates private, Git-ignored `.codex/config.toml` settings in each proxy
worktree and verifies their effective values with `config/read` before dispatch.
It refuses to overwrite existing/tracked configuration or silently ignore an
untrusted project layer. OpenAI workers do not receive these overrides.

## Prepare and dispatch

1. Read the repository instructions. Check `/Volumes/tmp` is mounted, disk and
   host admission status. Select an explicit base ref from the target repo.
2. Create a dedicated branch and worktree under
   `/Volumes/tmp/worktrees/<project>/codex/<task>/`. Never use the canonical
   checkout or `~/.codex/worktrees` for delegated implementation. Route build
   outputs and caches to `/Volumes/tmp/dev-artifacts/<project>/<task>/`.
   Use the shared heavy admission gate for checkout/build/install/test work;
   do not bypass a rejected gate. Keep at most two workers for heavy commands.
3. Write the bounded assignment to a durable prompt file: purpose, owned files,
   exclusions, acceptance criteria, validation and parent task ID. Require a
   pushed PR before handoff, draft if unfinished, with no secrets/unrelated edits.
4. Run the installed helper (normal Python, no dependency install):

   ```sh
   python3 ~/.codex/skills/proxy-model-workers/scripts/worker.py create \
     --repo /absolute/canonical/repo \
     --worktree /Volumes/tmp/worktrees/project/codex/task \
     --model kimi-k3 --reasoning high --title 'Bounded task title' \
     --issue https://github.com/OWNER/REPO/issues/ISSUE_NUMBER \
     --parent-thread PARENT_ID --parent-title 'Exact parent task title' \
     --prompt-file /absolute/durable/assignment.md
   ```

   Read the parent title verbatim with the Desktop thread tools. The helper
   allocates the next worker number for that parent and sets the title to
   `[Worker1@Exact parent task title]: Bounded task title` (then Worker2, etc.).
   It validates the existing worktree and records its task ID, provider,
   branch, repository and prompt in `~/.codex/proxy-workers/jobs/`.
   It inherits the parent's canonical project even when the implementation uses
   another repository. If the parent has no assignment, its working directory
   must match exactly one saved project's root; otherwise fix the parent's
   project assignment before retrying. The returned project assignment is verified.
   Persistence has a 30-second deadline and makes no model request. A real rollout
   and successful read-back are required before `preparation_status` becomes
   `ready`; `preparation_turn_id` is null and `preparation_mode` is
   `contract_without_inference`. The first user message and first inference turn
   are the actual assignment delivered through Desktop.
   Failed preparations retain their task ID, error and recovery guidance in the
   registry. Inspect that record and recover the same task before implementation;
   rerunning create cannot silently replace a failed task on the same worktree.
5. Require `preparation_status=ready`, then read the returned `prompt_file` and send its full contents with
   `send_message_to_thread`, using the returned thread ID and reasoning.
   Emit the created-task directive required by the app. Follow progress using
   bounded `wait_threads`; keep the existing model/provider settings.

## Context continuity and publication

After reporting review-ready work or an actionable blocker, end the worker turn
instead of polling the parent. The parent waits for idle before sending the full
consolidated correction, and verifies a new turn plus actual execution. A message
sent to an active Desktop turn can be surfaced as tool output; successful delivery
does not establish that the correction was followed. Recover a missed correction
through the same idle worker with its current assignment and private checkpoint.

Compaction is not completion. Keep a concise private checkpoint containing the
active goal, completed work and evidence, outstanding checks, latest corrections,
publication restrictions and next responsible task. Recover it after every
compaction; do not revive superseded instructions or repeat finished work. Parent
corrections must also update the durable handover/checkpoint, not only transient
chat. Validate at least two consecutive real compactions with intervening changes
to acceptance criteria before claiming long-running integration works. A unit
test or successful short turn does not establish that behavior.

Public issues contain sanitized technical summaries; private handovers, task IDs,
operator paths and model notes stay outside the repository. Before first
publication the parent reviews exact outgoing text, all new commits and the diff.
Review generated logs/artifacts too. Secret scanning supplements this review; it
does not detect every private business detail. Follow GLOBAL-INSTRUCTIONS.md for
the complete confidentiality boundary and active parent/supervisor reporting.

## PR and merge lifecycle

Every delegated implementation must finish with an accessible PR. Commit and
push only owned changes during this work session: tmp can disappear before four
days. Open a draft PR if unfinished. If push/PR access fails, immediately save a
durable recovery copy and report the exact blocker; do not call the task complete
or remove the only source copy. Existing repo guardrails still apply.

Register the PR before handoff:

```sh
python3 ~/.codex/skills/proxy-model-workers/scripts/worker.py bind-pr \
  --thread THREAD_ID --pr https://github.com/OWNER/REPO/pull/NUMBER
```

Immediately after PR creation, `bind-pr` returns `required_title`, for example
`#[PR123]: Bounded task title`. Apply it with `set_thread_title` to that worker
task immediately; the parent verifies this when receiving the PR. Keep the parent
ID and original title in the registry after renaming.

The helper verifies the PR repository, branch and pushed head against the
worktree. Update this binding after additional commits. Keep the task open while
the PR is open, draft, failing or merely closed without merge.

The parent reads `worker.py status`. Complete the retrospective before merge,
then merge/archive as one controlled completion sequence:

1. Finish review and required rework while the PR is open. Read the exact PR head
   SHA actually being assessed. The worker waits; it does not merge.
2. Re-read `~/.codex/proxy-workers/MODEL-EXPERIENCE.md`. Review the actual PR diff,
   validation and requested rework. Record what the model handled well, what it
   missed, and whether failures arose from the model or the task/tools/environment.
   Update the model assessment or explicitly confirm it remains accurate.
3. Save a concise JSON assessment with `outcome` (`success`, `mixed`, `failure`,
   or `reviewed_unchanged`), `failure_cause` (`none`, `model`, `task_spec`, `tooling`,
   `environment`, `quota`, `unknown`), `task_type`, `strengths`, `weaknesses`, `use_for`,
   `avoid_for`, and `evidence_note`. All fields are nonempty strings. Unknown
   remains unknown; one PR must not become an unsupported general benchmark.
4. Run `worker.py record-review --thread THREAD_ID --head ASSESSED_SHA --assessment /absolute/review.json`.
   This preserves the review in the durable job and refreshes the shared Markdown
   notebook. Historical reviews remain in jobs; the notebook shows concise model
   summaries and the latest five observations per model. Review revisions remain
   in review_history. A quota-only observation must be reviewed_unchanged and cannot
   replace a quality assessment. The command rejects a PR head that changed since
   assessment and an initial retrospective after merge.
5. The parent merges after satisfactory review under the user/repository authority.
   Verify MERGED and idle task state, and re-check that the notebook remains accurate
   for the final head; record an update if needed. Then run
   `worker.py ready-to-archive --thread THREAD_ID`. It rejects missing/current-
   head-mismatched experience reviews, unmerged PRs, dirty worktrees and extra commits.
6. Call `set_thread_archived` for the worker. Then run
   `worker.py archived --thread THREAD_ID` to record the completed lifecycle. Workers must never merge. The parent follows
the user's authorization and the repository's merge rules; these instructions
do not authorize merging unrelated PRs. Do not wait for a scheduled monitor.

Before removing a merged worktree, verify it is clean, on the recorded branch,
and HEAD equals the merged PR's head commit. Remove it through `git worktree
remove` from the surviving canonical checkout. If dirty, running, or carrying
additional commits, retain it and report the reason. Never force-remove it or
wipe shared tmp data. Archive the task without deleting its history.

Global instructions are loaded into new tasks. Send the new rules explicitly
when assigning work to an already-running task; do not assume hot reload.
