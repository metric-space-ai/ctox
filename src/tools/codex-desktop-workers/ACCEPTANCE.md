# Codex Desktop worker acceptance guide

Use this checklist to accept the implemented worker workflow. The
current implementation persists an execution contract without an initialization model turn and has
focused protocol/lifecycle tests; passing those checks does **not** by itself
prove a complete operator PR/merge/archive cycle. Complete the open review,
retrospective, merge and archive steps below before declaring that full cycle
accepted.

## 1. Prepare a disposable worker

1. Start with one open issue in the target repository. The parent task owns the
   issue analysis, solution sketch, acceptance criteria and bounded handover.
2. Check `/Volumes/tmp`, host capacity and the shared heavy-job gate. Put the
   dedicated Git worktree under `/Volumes/tmp/worktrees/...` and route build or
   cache output under `/Volumes/tmp/dev-artifacts/...`.
3. Write the assignment to a durable prompt file. Include the parent task ID and
   title, purpose, owned files, exclusions, validation commands, PR requirement,
   and explicit limits against unrelated edits, recursion and merging.
4. Check model evidence and current availability before selecting a model:
   `python3 ~/.codex/skills/proxy-model-workers/scripts/worker.py availability`.
5. Create and persist the task with the installed helper's `create` command and
   its current required options: repository, worktree, model, reasoning, title,
   issue, parent thread, parent title and prompt file. The helper validates the
   worktree and records the task before persisting its execution contract.
6. Dispatch only after the returned registry record has
   `preparation_status: "ready"` and `preparation_mode: "contract_without_inference"`.
   This means the execution contract was persisted and read back without a model
   turn. There must be no initialization user message or reply; the first actual
   user request is the implementation assignment. A path alone is not ready.
7. Record or verify the returned `thread_id`, `project_id`, requested `model`,
   `provider`, `reasoning`, branch, worktree, issue URL, prompt path and
   `rollout_path`.
8. Verify canonical project inheritance and provider-appropriate listing before
   dispatch. The helper must inherit the parent's canonical project through
   `thread/read`, or resolve exactly one project by the parent's exact root
   through `project/list`; it must persist that `project_id`, pass it to
   `thread/start`, and observe the same assignment on the created thread. The
   canonical project ID remains authoritative in the app-server and registry
   even when the legacy UI project label is null. Ordinary Desktop
   `list_threads` filters to OpenAI (`modelProviders: null`), so absence of a
   proxy worker from ordinary project listing is a documented app limitation,
   not a failed worker. Raw `thread/list` with `modelProviders: []` lists all
   providers. For a proxy worker, the parent pins it with
   `move_thread_to_sidebar_section(sectionId='pinned')`, verifies it in
   `pinnedThreads`, and records that membership evidence. A successful
   `send_message_to_thread` or `read_thread` call alone does not establish
   visibility. If canonical assignment is wrong, stop and return the
   integration boundary to the parent.
9. Send the full prompt file as the first implementation turn through Desktop
   `send_message_to_thread` on that existing task. Keep its saved model/provider
   and reasoning settings. Do not use the general model picker as a provider
   switcher; for proxy models, confirm actual proxy routing and for OpenAI
   confirm no proxy request is used.

10. Follow progress with bounded `wait_threads`. Do not leave duplicate polling
    loops or unowned processes running.

## 2. Register the pushed PR

1. Commit only the owned files, push the recorded branch, and open the PR or
   update the existing draft/ready PR.
2. Run the helper's `bind-pr` command with the task ID and PR URL after the
   worktree HEAD equals the pushed PR head.
3. Apply the exact `required_title` returned by `bind-pr` with Desktop
   `set_thread_title`. It has the form `#[PRNUMBER]: Bounded task title`.
4. Keep the registry's task ID, parent, issue, repository, worktree, branch,
   pushed head and PR association. Run `bind-pr` again after every additional
   pushed commit so the binding and head remain current.

## 3. Apply parent corrections

Wait until the reporting worker turn has ended, then send the consolidated
corrections to the same Desktop task with `send_message_to_thread`. Verify a new
active turn and changed source or fresh validation evidence. A successful send,
an old test result or a correction embedded in a tool output is insufficient. Keep
the same worktree, provider/model settings, branch and exactly one PR. The
parent reviews the actual diff, pushed head and validation evidence after every
rework; worker prose alone is not completion evidence.

## 4. Retrospective, merge, archive and registry closure

Complete this sequence only after the implementation and rework are accepted:

1. While the PR is still **OPEN**, read the shared model-experience notebook,
   review the actual PR diff and evidence, and assess the exact PR head.
2. Save a concise JSON assessment with nonempty `outcome`, `failure_cause`,
   `task_type`, `strengths`, `weaknesses`, `use_for`, `avoid_for` and
   `evidence_note` fields.
3. Record it with
   `worker.py record-review --thread THREAD_ID --head ASSESSED_SHA --assessment /absolute/review.json`.
   If the PR head changed first, update the PR binding and review the new head.
4. Have the parent merge only under the user's and repository's merge
   authority. Verify that GitHub reports `MERGED`, the Desktop task is idle, and
   the retrospective still matches the final PR head.
5. Run `worker.py ready-to-archive --thread THREAD_ID`. It must pass with the
   PR merged, retrospective present for the merged head, the recorded branch
   checked out, HEAD equal to the PR head, and the worktree clean.
6. Archive the idle task with `set_thread_archived`, then record completion with
   `worker.py archived --thread THREAD_ID`. The registry record, not prose or a
   closed PR alone, is the durable lifecycle evidence. After archive, remove the
   temporary proxy-worker pin with
   `move_thread_to_sidebar_section(sectionId='threads')`.
7. Remove a merged worktree only from a surviving checkout after it is clean,
   still on the recorded branch, and at the merged PR head. Never force-remove
   it or affect another task's worktree, processes or data.

## 5. Recover failed preparation without duplication

If `create` fails after the task is created, inspect the returned registry JSON
under `~/.codex/proxy-workers/jobs/`. Keep its `thread_id`, `preparation_error`,
`recovery` and `rollout_path`; the corresponding `prepare.stderr` and process
record are under `/Volumes/tmp/dev-artifacts/...`. Do not rerun `create` for the
same unarchived worktree and do not send the implementation assignment. Recover
the same task through the app-server, or have the parent explicitly retire the
failed preparation before authorizing a replacement. This is intentional:
`create` refuses to silently replace the owner of the worktree.

## 6. Handle quota or capacity deferral

Quota, rate-limit and capacity failures are temporary availability observations,
not model-quality evidence. Record an observed provider reset with
`worker.py defer --model MODEL --until ISO8601_WITH_ZONE --note 'concise sanitized reason'`;
omit `--until` only when the reset is unknown and retry after one day. Wait for
the expiry with a bounded retry, or explicitly select another available model.
Do not silently remap an alias, change the global provider, or record the event
as a quality failure. An unchanged later review preserves earlier quality
findings.

## 7. Verify repeated context compaction

Start with a small, explicit assignment as the first user request. Confirm there
is no initialization exchange. Send a correction that changes an acceptance
criterion and restricts publication, compact, and verify the worker retains both
the assignment and correction. Send another correction, compact again, and verify
the current requirements, completed work, remaining action and parent recipient.
Require at least two successful real compactions; do not infer success merely
from a compaction-start notification. Distinguish an app-server protocol test from
the Desktop dispatch/resume path and record which path was actually exercised.

For proxy workers, verify the effective worktree configuration is 256000 tokens
with automatic compaction at 230400. Runtime may reserve part of that context;
record the observed effective allowance separately. Confirm OpenAI task settings
were not changed. Keep diagnostic tasks bounded and archive them after the test.

## Evidence to retain privately

Retain the issue and PR URLs, task and parent IDs, canonical project assignment,
provider-appropriate Desktop listing evidence (`pinnedThreads` for proxy
workers), pin-removal evidence, bounded prompt file, requested
and observed provider/model/reasoning, branch, worktree, preparation status and
rollout path (or failure record), pushed commit, PR head, diff and validation
results, review requests, retrospective JSON and assessed head, merge evidence,
archive-readiness output, and archived registry state. Keep durable evidence in
the private worker registry and prompt/experience files outside the repository,
not only in the wiped `/Volumes/tmp` volume. The accessible public PR retains only
reviewed source and sanitized technical summaries. Never upload this private
coordination bundle, raw history or operator metadata to the public PR.
