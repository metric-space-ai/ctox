---
name: jour-fix
description: Prepare the native project Jour fixe and retain Owner-confirmed goals.
---

# JourFix

Use the native meeting/project and registered Supervisor; names/UUIDs grant no
authority. Finish one explicit execution plan. Preparation permits no deployment,
external message, grant change or unrelated delegation. No shell store/queue/goal edits.

Read configuration, previous confirmed goal, evidenced PRs, KPIs, comments and
final transcript. Name missing/stale evidence. Never invent numeric metrics or completed work.
Use business_os.project_kpi action read, request {project_id}, for prompts and
recipe catalogue. Resolve a matching recipe with BindKpiRequest
{operation_id,project_id,kpi_id,prompt_revision,expected_revision,recipe,window_days}.
Match the prompt; disconnected sources stay missing_source. No supplied values,
SQL, URLs or foreign source. Native refreshes hourly and before prep.

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
- Publishing queues native sequential narration of the stored deck. Do not call
  narrate in the agent turn. CTOX uses each native narration_input, retains WAV
  custody and sets ready only after every slide has audio. Saving is not proof
  of ready audio; read the persisted meeting state. Never invent AudioRef or
  automatically resynthesize uncertain operations.
- In review, update action propose_todos, ProposeTodosRequest:
  {operation_id,meeting_id,expected_revision,proposal_revision,items}. Each item
  has {id,title,acceptance,priority,owner,evidence_ids,due_at_ms?}; owner is explicit,
  evidence IDs belong to this meeting. Derive proposals from comments and final
  transcript; retain both speakers, source runs and measured latency. Partials
  are transient; only authenticated ordered finals persist.

Read revisions before intent. Recover uncertain mutations with the same
operation_id and request. Changed authority/expired lease needs native recovery;
never forge receipts. Only the owner's explicit confirmation of the
current proposal installs the next durable Supervisor goal. A proposal is not a
confirmed goal; subsequent work retains normal permissions, review and recovery.

Confirmed goal: business_os.workjet_worker_dispatch {action:"observe"}. Read worker_outcomes for PRs; startup is not completion.
