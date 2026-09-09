# Codex Desktop worker acceptance guide

Use this checklist to accept the implemented worker workflow in PR #93. The
current implementation has a bounded READY-only preparation integration and
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
   worktree and records the task before the preparation turn.
6. Dispatch only after the returned registry record has
   `preparation_status: "ready"`. This means the bounded, no-tool first turn
   ended successfully with the exact `READY` reply and a real, nonempty
   persisted rollout file. `thread/start` returning a path alone is not ready.
7. Record or verify the returned `thread_id`, requested `model`, `provider`,
   `reasoning`, branch, worktree, issue URL, prompt path and `rollout_path`.
8. Send the full prompt file as the first implementation turn through Desktop
   `send_message_to_thread` on that existing task. Keep its saved model/provider
   and reasoning settings. Do not use the general model picker as a provider
   switcher; for proxy models, confirm actual proxy routing and for OpenAI
   confirm no proxy request is used.
9. Follow progress with bounded `wait_threads`. Do not leave duplicate polling
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

Send corrections to the same Desktop task with `send_message_to_thread`. Keep
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
   closed PR alone, is the durable lifecycle evidence.
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

## Evidence to retain

Retain the issue and PR URLs, task and parent IDs, bounded prompt file, requested
and observed provider/model/reasoning, branch, worktree, preparation status and
rollout path (or failure record), pushed commit, PR head, diff and validation
results, review requests, retrospective JSON and assessed head, merge evidence,
archive-readiness output, and archived registry state. Keep durable evidence in
the worker registry, prompt/experience files and accessible PR—not only in the
wiped `/Volumes/tmp` volume.
