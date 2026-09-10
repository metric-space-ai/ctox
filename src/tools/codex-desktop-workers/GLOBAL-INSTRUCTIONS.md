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
   components, exclusions, acceptance checks, verification and known risks. Delegate
   a coherent implementation outcome to a disposable worker. Include foreseeable
   tests, documentation, generated outputs and dependent consumer adjustments in
   the package. Do not hand an
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

## Package size and decision ownership

For multi-hour goals, delegate a substantial, independently reviewable outcome,
not one worker per file, command, test failure or workflow step. Bundle tightly
coupled changes that share acceptance and would otherwise require stacked PRs
just to pass the same checks. Split for independent outcomes, meaningful risk or
ownership boundaries, or a package too large to review; do not bundle unrelated
work merely to make a worker larger. A small standalone worker needs a concrete
benefit such as isolation or independent delivery. Otherwise include the change
in the relevant package before dispatch. Do not invent minimum runtimes or
require extra planning reports to justify every assignment.

The main task leads multiple workers: decompose a substantial goal into coherent
packages, identify dependencies, then launch the ready independent packages as a
parallel wave. This is the default after decomposition, not an optional exception.
After analysis, dispatch independent, substantial packages concurrently; do not
wait for one worker's PR to merge before starting an unrelated ready package.
Start with at most two active implementation workers per parent, subject to shared
host/provider capacity and other tasks. Give them separate worktrees and clear
ownership; serialize actual dependencies or overlapping changes. Follow both by
ID and handle whichever result is actionable. The single shared heavy-job gate
still serializes builds and heavy tests across the host; it does not require all
editing or model work to run serially. Never bypass the gate to gain parallelism.

The parent owns routine implementation decisions, review corrections, validation
and directly required follow-on files within the agreed outcome. Update the
handover and keep the same worker/PR; do not request supervisor approval for each
file or correction. Explicit exclusions, other owners' work, privacy boundaries,
new product behavior and materially different risks still require resolution by
the responsible owner. The supervisor handles cross-parent conflicts and material
changes to the overall goal. Reports are notifications, not approval requests.
Do not wait for acknowledgements or relay unchanged status through the hierarchy.
Complete the relevant review pass before sending one prioritized correction list
with locations, expected behavior and acceptance checks. Include available CI and
subagent findings together; do not dispatch one turn per finding. On the next pass,
review the correction delta and remaining risks. Reuse analysis and test evidence;
avoid duplicating the parent's review or full worker history at supervisor level
unless a concrete failure requires it. Existing PR, publication and archive gates
remain in force. Do not recreate or combine active workers solely to adopt this rule.

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

There must be no initialization model turn or READY handshake. Prepare only the
execution contract, then send the complete implementation assignment as the first
user request. Proxy workers use a configured context of 256000 tokens with an
automatic compaction threshold of 230400, verified through their private ignored
worktree configuration so Desktop resume reloads it. Do not override OpenAI tasks.

Compaction is continuation, never completion. Maintain a durable private checkpoint
with the current assignment, completed changes and evidence, unresolved checks,
latest corrections, publication restrictions, exact parent ID and next action.
Recover it after every compaction; newer corrections supersede historical summaries.
Do not revive an old handshake or repeat completed work. Validate consecutive
compactions with intervening corrections before claiming continuity works; a
context-size setting or one successful model reply does not prove this.

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

## Public repository confidentiality boundary

Public issues, comments, commits, pushes, PRs (including drafts), CI logs and
artifacts are publication surfaces. Before publishing, inspect the exact outgoing
text/files, all new commits and the staged diff, including generated data and
attachments. Review for secrets AND nonpublic information: credentials, OAuth
callback URLs/cookies, customer or personal/business data, private chats/handovers,
raw logs, operator-specific usernames/paths/hosts, and internal task/session IDs.
Never copy private coordination records or the model-experience notebook into a
public issue or PR. Use repository-relative paths, synthetic fixtures and minimal
sanitized technical context. Keep parent/worker IDs and private evidence in durable
local records outside the repository. Do not put real secrets into scanner output.

The parent prepares a public-safe issue/acceptance summary separately from the
private worker handover. Workers that receive private context must obtain parent
review of their exact proposed first public diff/new commits and outgoing text
before pushing or posting. Parent review also covers later new sensitive content.
A scanner is supplemental; a clean scan is not proof of confidentiality. Recheck
that generated logs/artifacts do not expose private data before uploading them.
Keep ambiguous sensitive content local and escalate only that content category;
do not block already sanitized unrelated work or demand repetitive approvals.

If accidental disclosure is found, preserve evidence privately, report location
and category without repeating values, and correct the affected owned public
content. An edit does not recall notifications, clones or caches. Exposed
credentials require revocation/rotation by the authorized owner; do not silently
rewrite shared history or claim exposure undone. Never claim zero leaks from a
prompt rule or limited scan alone.

## Mandatory completion and escalation reporting

Every worker handover must name the exact parent task ID. If a supervisor is
assigned, the main task's coordination record must also name its exact task ID.
Do not infer recipients from titles or assume a final chat answer wakes a parent.

On PR-ready, rework-ready, a blocker requiring parent action, or an execution
failure, the worker must send_message_to_thread to its parent before ending its
turn. Include worker ID, issue/PR URL (or why none exists), pushed head,
validation results and gaps, owned processes/lease, and the next required action.
A PR-ready report is a request for review, not permission to merge or archive.

After reporting, the worker ends its turn; it must not poll the parent or keep the
turn alive waiting for review. The parent waits for that worker turn to complete
before sending one consolidated rework assignment, then verifies a new active
turn and actual progress. Delivery success is not execution evidence: messages
sent during an active Desktop turn can appear among tool outputs and be missed.
If corrections were delivered but not acted on, preserve the current assignment
and latest corrections privately, then resend the consolidated assignment to the
same idle worker as its next user request. Do not create a replacement or revive
initialization. Urgent updates during work still need observed acknowledgement
and a durable checkpoint; never treat a successful send alone as compliance.

The parent acknowledges by taking the next review/rework/recovery action and
reports material transitions to its assigned supervisor with send_message_to_thread:
PR ready, blockers/failures needing supervisor action, and finally merged plus archived/cleaned or the exact
remaining cleanup blocker. Include evidence links and worker ID. For a smoke test,
explicitly state whether acceptance passed and what remains untested. Never report
an overall goal complete merely because one worker finished.

Persist the latest reported state and next responsible task in the durable
coordination record. Check message-tool success; on failure preserve a pending
notification with recipient, event and error. Retry only on the next bounded
supervision pass, not in a loop. Do not send repeated unchanged status messages.

Notifications supplement observation: the parent follows its worker by exact ID
with bounded wait_threads and registry/PR evidence. The supervisor's existing
heartbeat reconciles all assigned parents/workers and pending notifications,
including idle workers with open PRs and merged PRs awaiting archive. Missing
Desktop toolmarkers alone do not mean inactivity. Reconcile an unreported state
with the existing owner; never spawn a replacement merely because a message or
progress projection is missing. Reuse the existing monitor rather than creating
a competing polling chain.
