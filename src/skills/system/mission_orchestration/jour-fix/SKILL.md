---
name: jour-fix
description: Prepare the registered project Supervisor's Jour fixe and retain Owner-confirmed follow-up work.
---

# JourFix

Stay within the task's native meeting/project and registered Supervisor. Names
or caller UUIDs confer no authority. Use one explicit execution plan and finish
its model-owned steps. Preparation permits no deployment, external message,
grant change or unrelated delegated work. Never mutate stores, queues or goals
via shell.

Read configuration, previous confirmed goal, evidenced PRs, KPIs, comments and
final transcript. Name missing/stale evidence. Never invent numeric metrics or completed work.
Use business_os.project_kpi action read, request {project_id}, for prompts and
recipe catalogue. Resolve a matching recipe with BindKpiRequest
{operation_id,project_id,kpi_id,prompt_revision,expected_revision,recipe,window_days}.
Match its meaning; unconnected sources stay missing_source. Supply no values,
SQL, URLs or foreign source. Native definitions refresh hourly and before prep.

During the current leased Supervisor turn:

- business_os.jour_fixe_read: {action:"read_meeting",request:{project_id,meeting_id}}
  returns configuration, previous goal, deck/proposal revisions and native
  narration_inputs. Actions read_comments/read_transcript retain that request.
- Generate a SlideDocument via learnordie.slide-agent.v1 with factual sourceRefs.
  Read business_os.presentation_read action read_guide, request {}, then follow
  its schemas to validate/save/publish using business_os.presentation_update.
  Missing tools/evidence: incomplete preparation; never substitute title/text
  slides or a prepare_deck fallback.
- Use learnordie.excalidraw.v1, 1600x900, handwriting font family 1; themes
  learnordie-north, learnordie-technical or learnordie-dark-room. Use
  data-backed three.js scenes, at least one for acceptance; never invent values.
  Stable slideId binds comments, todos and narration.
- Cover goals, KPIs, Essential KPI/exit value, workers, merged PRs and todos.
  Compare KPIs with the penultimate Jour fixe: cite both retained occurrences,
  matching definition/unit/window. Missing/stale/changed baselines or zero
  denominators mean unavailable percentages, never invented zeros. Flag missing
  exit evidence. Save under this meeting/project via native tools and existing
  RxDB/WebRTC files, never HTTP. Saved decks do not prove rendered/ready audio.
- For each saved slide without audio, read its current narration_input and call
  update action narrate, NarrateRequest:
  {operation_id,meeting_id,slide_id,deck_revision,expected_revision,narration_text_sha256}.
  Copy the native input/hash; do not invent a hash. No text, model, voice or audio
  override. Read the new revision before the next slide. Only the configured
  native gateway produces WAV; custody binds actual bytes, duration and hashes.
  Every slide needs retained audio before ready. Name missing configuration or
  uncertain synthesis as readiness failure; no fallback or invented AudioRef.
  Uncertain operations are not automatically resynthesized.
- In review, update action propose_todos, ProposeTodosRequest:
  {operation_id,meeting_id,expected_revision,proposal_revision,items}. Each item
  has {id,title,acceptance,priority,owner,evidence_ids,due_at_ms?}; owner is explicit,
  evidence IDs belong to this meeting. Derive proposals from comments and final
  transcript; retain both speakers, source runs and measured latency. Partials
  are transient; only authenticated ordered finals persist.

Read revisions before new intent. Recover uncertain mutations using the same
operation_id and identical request. Changed authority/expired lease needs native
recovery, never forged receipts. Only the owner's explicit confirmation of the
current proposal installs the next durable Supervisor goal. A proposal is not a
confirmed goal; subsequent work retains normal permissions, review and recovery.

Confirmed goal: business_os.workjet_worker_dispatch {action:"observe"} first. Use worker_outcomes reports for PRs; startup proves no completion.
