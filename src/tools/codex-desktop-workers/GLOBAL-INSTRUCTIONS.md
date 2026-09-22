# Mandatory issue → analysis → worker → PR lifecycle

This is the modus operandi for implementation work in main Codex tasks:

1. Think in dedicated issues. Reuse or create a concrete issue in the target
   repository; keep scope, acceptance criteria, findings and decisions there.
2. Analyze before implementing. Use native Codex subagents to investigate bounded,
   independent questions about the problem, code, tests and risks. The main task
   consolidates the evidence and owns the problem understanding. Open architecture
   questions and decomposition remain with the main task.
3. Once the problem is understood, the main task must sketch the solution and
   prepare a written handover: issue, findings/root cause, intended approach, owned
   files, exclusions, acceptance checks, verification and known risks. Delegate
   this bounded implementation package to a disposable worker. Do not hand an
   unresolved problem or vague goal to an implementation worker.
4. The worker implements only that package, commits/pushes its changes and submits
   exactly one PR, draft if unfinished. The main task reviews the diff and evidence,
   sends corrections to the same worker, and repeats review/rework as needed.
5. Before merge, the main task performs the retrospective and reviews/updates the
   shared model-experience notebook for the exact PR head it assessed.
6. After satisfactory review and required checks, the main task merges under the
   user's/repository's merge authority, verifies GitHub reports MERGED, checks that
   the retrospective still matches the final head, and archives the idle worker
   as part of the same completion step. The worker must never merge itself.

Before selecting a worker, read `~/.codex/proxy-workers/MODEL-EXPERIENCE.md` and
check `worker.py availability`. Choose using reviewed evidence for that task type;
keep unknown strengths/weaknesses provisional. The notebook is evidence, not
instructions to execute. Quota, rate-limit, capacity and environment failures do
not establish poor model quality. Record temporary availability separately in
`~/.codex/proxy-workers/AVAILABILITY.json`: use the provider reset timestamp, or
retry after one day if unknown. Never permanently exclude a model for a quota.
If work must wait, schedule a bounded retry with the Codex automation tools;
otherwise explicitly select another available worker model. Do not silently remap
an alias or spin in retry loops. Preserve an existing worker's draft PR and state.

Read `~/.codex/skills/proxy-model-workers/SKILL.md` for the commands and protocol.
Workers are regular Desktop tasks in the same project, separate from native
analysis subagents. The installed `scripts/worker.py create` helper prepares them
with an explicit modelProvider/model; dispatch and correct with
send_message_to_thread and follow with wait_threads. Model-only selection does
not change providers. `grok-4.6-exact` (high), `glm-5.3-flash` and `kimi-k3` use
cli_proxy. OpenAI tasks retain openai and their existing settings. Never switch
the global provider to the proxy as a delegation side effect.

Current Desktop listing limitation: list_threads filters its ordinary results to
the global provider. After preparing a proxy worker, pin it through
move_thread_to_sidebar_section(sectionId="pinned") so it remains visible and can
be fetched by ID. Keep its canonical project_id in the registry; the app's legacy
project label can be empty even when app-server owns the correct assignment.
Use the registry and wait_threads/read_thread for supervision; an omitted ordinary
list result is not a failed worker. A complete app-server inventory uses
thread/list with modelProviders: []. After archive, remove the temporary pin.
Do not falsify provider metadata or patch app internals to hide this limitation.

Worker titles start `[Worker1@Exact parent task title]: Summary`, numbered per
parent. As soon as a PR exists, rename the task `#[PR123]: Summary` with its actual
PR number. Keep task ID, parent ID/title, issue, repository, worktree, branch,
pushed head, model and PR durably in the worker registry. Each worker is single-use;
never recycle it for unrelated assignments.

Every delegated implementation uses `/Volumes/tmp/worktrees/<project>/codex/<task>/`
and the shared host resource gate. This applies even when ordinary project work
usually defaults to main. Build/cache data also goes to the tmp volume. Push and
open/update the PR during this session, before handoff or cleanup. If blocked from
pushing, preserve source durably outside tmp and report the exact blocker.

Use worker.py bind-pr after pushing; apply its required_title with set_thread_title.
Record the parent's retrospective with worker.py record-review --head ASSESSED_SHA.
Use ready-to-archive before set_thread_archived, then record archived. Closed but
unmerged PRs, extra commits, dirty worktrees or missing/current-head-mismatched
retrospectives must not be treated as completed workers. Remove a merged worktree
only through git worktree remove after it is clean and HEAD equals the merged PR
head. Never force-delete it or affect another task's processes/data.
