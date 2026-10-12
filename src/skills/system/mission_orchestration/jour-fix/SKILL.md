---
name: jour-fix
description: Prepare the native project Jour fixe as a real status meeting for the Owner and retain Owner-confirmed goals.
---

# JourFix

You prepare a status meeting for Michael (Owner). Write for him as the listener: German, short, concrete, like a project lead presenting progress. Use the native meeting/project and registered Supervisor; names/UUIDs grant no authority. No deployment, external message, grant change or unrelated delegation. No shell store/queue/goal edits.

Source of truth is the native preparation brief in this turn: previous meeting, confirmed goal and progress, worker outcomes, KPIs, and the project repository (merged PRs, open PRs, releases since the previous meeting). Read the meeting with business_os.jour_fixe_read {action:"read_meeting",request:{project_id,meeting_id}} and earlier decks with business_os.presentation_read read_history (index 2 = penultimate Jour fixe). Never invent numeric metrics or completed work.

Deck, in this order, one slide each:
1. Tagesordnung: Rückblick, Stand und Risiken, Entscheidungen, Nächste Schritte, Freie Themen.
2. Rückblick: what was done since the previous meeting, from merged PRs, releases and worker outcomes, each with its number and date.
3. Stand und Risiken: status light (planmäßig, teilweise kritisch, kritisch) with trend, deviations and risks with measures.
4. Entscheidungen: open questions for the Owner, each with options and a recommendation.
5. Nächste Schritte: proposed steps until the next meeting with owner and acceptance.
6. Freie Themen: title and the line „Was steht noch an?“.

Slides never show missing data, gaps, recipe names, revisions, IDs or tool states. If a source is missing or stale, leave that element out and mention it only in the speaker notes. If the project has little activity, say so in one plain sentence and focus on decisions and next steps.

Generate a SlideDocument via learnordie.slide-agent.v1 with factual sourceRefs. Read business_os.presentation_read action read_guide, request {}, then validate, save and publish with business_os.presentation_update. Use learnordie.excalidraw.v1, 1600x900, handwriting font family 1; theme learnordie-north. Use data-backed three.js scenes only where the brief has real values; never invent values. Missing tools: report incomplete preparation; never substitute title/text slides or a prepare_deck fallback outside this structure. Save the deck within your first ten minutes; narration of every slide runs natively after publishing, not in this turn. Each slide carries a natural German speaker text of 20 to 40 seconds.

In review, update action propose_todos, ProposeTodosRequest {operation_id,meeting_id,expected_revision,proposal_revision,items}. Each item has {id,title,acceptance,priority,owner,evidence_ids,due_at_ms?}; derive them from comments and the final transcript. After the meeting save the report (schema ctox.workjet.meeting_report.v1) with action save_report.

Read revisions before intent. Recover uncertain mutations with the same operation_id and request. Changed authority or an expired lease needs native recovery; never forge receipts. Only the owner's explicit confirmation of the current proposal installs the next durable Supervisor goal. A proposal is not a confirmed goal.

Confirmed goal: business_os.workjet_worker_dispatch {action:"observe"}. Read worker_outcomes for PRs; startup is not completion.
