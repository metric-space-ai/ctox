---
name: jour-fix
description: Prepare the registered project supervisor's Jour fixe and record owner-confirmed follow-up work.
---

# JourFix

Work only within the native meeting/project and registered Supervisor in the
task. Titles or caller UUIDs cannot authorize another project or speaker. Use
one explicit execution plan and complete its model-owned steps.

Read the previous confirmed goal, configuration, evidenced merged PRs, KPIs,
comments and final transcript. Name missing or stale evidence.
Never invent numeric metrics or completed work.
Use business_os.project_kpi with action read and request {project_id}. For each unbound prompt, select a matching registered recipe with action resolve and BindKpiRequest {operation_id,project_id,kpi_id,prompt_revision,expected_revision,recipe,window_days}. Read returns the recipe catalogue. Match its stated meaning to the prompt; do not substitute a different measure. Native readers calculate the number, persist the recipe and refresh hourly and before preparation. Unsupported sources stay missing_source; never supply a value, SQL, URL or another project.

Prepare slides covering progress against goals, PR
results, KPIs, owner decisions and next actions with acceptance criteria.
Keep source references. Preparation authorizes no deployment, external message,
grant change or unrelated delegated work.

During the currently leased native Supervisor turn, these restricted tools are
available:

- business_os.jour_fixe_read: {action:"read_comments",request:{project_id,meeting_id}}
  or action:"read_transcript"; action:"read_meeting" also returns current deck,
  proposal/revisions and bounded project configuration. Use the actual meeting ID.
- business_os.jour_fixe_update: {action:"prepare_deck",request:<PublishDeckRequest>}.
  Supply operation_id, meeting_id, expected_revision, next deck_revision and
  slides [{id,position,title,body_markdown,meeting_id}]. Positions start at zero;
  IDs are unique. Omit audio. The saved draft remains preparing, not ready.
- After the meeting enters review, the update tool accepts action:"propose_todos"
  with ProposeTodosRequest: operation_id, meeting_id, expected_revision, next
  proposal_revision, items [{id,title,acceptance,priority,owner,evidence_ids,
  due_at_ms?}]. Each owner is explicit; evidence IDs come from this meeting.
  A proposal is not a confirmed goal.

Read current revision before changing intent. Retry an uncertain mutation with
the same operation_id and identical request. Lease expiry or changed authority
requires native recovery, never SQL or forged receipts.

Retain narration text in the workspace. Only the approved native speech gateway
may synthesize it. Missing model/credentials are named readiness failures, not
permission for paid fallback. Publication needs authorized file references,
actual narration/audio hashes and synthesis receipts. No narrate/publication or
goal-confirmation tool is implemented in this slice: report those steps pending,
do not simulate them or declare a draft ready.

Comments bind to exact slide/deck revision. Partials are transient; only
authenticated ordered final turns belong in the durable transcript. Preserve
both speakers and actual source runs and latency. Derive proposals from those
inputs. Only the owner's explicit confirmation of the current proposal installs
the next durable Supervisor goal. Later work retains normal permissions,
completion review and recovery. Never mutate SQLite/RxDB/queue/goals via shell.
