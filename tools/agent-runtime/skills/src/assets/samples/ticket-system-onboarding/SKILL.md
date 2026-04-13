---
name: ticket-system-onboarding
description: Use when CTOX is first attached to a ticket system and must build onboarding work, source understanding, and operator-visible review tickets through generic ticket primitives rather than kernel-owned onboarding logic.
metadata:
  short-description: Onboard a ticket system through skill-driven self-work
---

# Ticket System Onboarding

Use this skill when CTOX is attached to an existing ticket system and needs to understand how that system is actually used.

The kernel provides storage, references, self-work CRUD, publishing, and audit. This skill owns the onboarding behavior.

Read the phase guide in [references/onboarding-phases.md](references/onboarding-phases.md).
Read the deterministic stage plan in [references/onboarding-plan.md](references/onboarding-plan.md).

Default rule:

- follow the stage plan in order
- do not redesign the onboarding flow mid-run
- only change the plan when an operator explicitly asks for an adjustment or a real blocker makes the next stage impossible

When the system understanding problem is primarily driven by exports, record lists, CMDB tables, service catalogs, or other row-shaped data, first use [`tabular-knowledge-bootstrap`](../tabular-knowledge-bootstrap/SKILL.md) so the source becomes generic discovery knowledge before you project it into the ticket knowledge plane.

When there is enough durable ticket history or an export of desk history, use [`dataset-skill-creator`](../dataset-skill-creator/SKILL.md) to build a desk-specific operating skill from that evidence. The goal is not only to observe the desk, but to give CTOX a reusable skill that can guide later live-ticket handling in the style of this desk.

## Core Rules

No fixed onboarding choreography belongs in the ticket kernel.

Do not expect prebuilt onboarding tickets from `ctox ticket sync`.

Create only the self-work items that are justified by the observed source knowledge and current gaps.

The remote ticket system is only a communication and review surface. SQLite in CTOX remains the source of truth.

When onboarding reveals missing access rights, credentials, or monitoring blind spots, do not improvise around them. Create explicit CTOX work or requests through the ticket and secret primitives.

Visible operator work is mandatory. If you create internal CTOX work in the remote ticket system, you must work it like a teammate:

1. create the task
2. assign it to the CTOX identity when the adapter supports assignment
3. leave an initial internal note in plain human language
4. leave progress or block notes as the work evolves
5. finish or block the task with a final internal note and a state transition

Do not leave remote work items hanging without ownership, notes, or an end state.

When you need remote ticket work, you must go through the `ctox ticket self-work-*` and `ctox ticket access-request-put` primitives.

Do not use raw HTTP calls, `curl`, ad hoc browser actions, or direct remote API writes to create or update ticket work during onboarding. If the generic ticket primitives are insufficient, stop and state the missing primitive instead of bypassing SQLite truth.

## Required Inputs

Refresh and inspect the current source knowledge first:

```sh
ctox ticket sync --system "<system>"
ctox ticket knowledge-list --system "<system>" --limit 20
ctox ticket knowledge-show --system "<system>" --domain "source_profile" --key "observed"
```

Inspect any already-open CTOX self-work:

```sh
ctox ticket self-work-list --system "<system>" --limit 20
```

Inspect the secure local secret inventory only through explicit local commands:

```sh
ctox secret list
ctox secret show --scope "<scope>" --name "<name>"
```

## Primitive Used For New Work

Create a self-work item in SQLite:

```sh
ctox ticket self-work-put --system "<system>" --kind "<kind>" --title "<title>" --body "<text>" --metadata-json '<json>'
```

Publish a self-work item into the remote ticket system when the operator-facing surface is useful:

```sh
ctox ticket self-work-put --system "<system>" --kind "<kind>" --title "<title>" --body "<text>" --metadata-json '<json>' --publish
```

Or publish an already-created item later:

```sh
ctox ticket self-work-publish --work-id "<work_id>"
```

Assign published self-work to the remote CTOX identity:

```sh
ctox ticket self-work-assign --work-id "<work_id>" --assignee "self" --assigned-by "ctox"
```

Append an internal operator-facing progress note:

```sh
ctox ticket self-work-note --work-id "<work_id>" --body "<plain human note>" --authored-by "ctox" --visibility internal
```

Block or finish the work visibly:

```sh
ctox ticket self-work-transition --work-id "<work_id>" --state blocked --transitioned-by "ctox" --note "<plain human block note>" --visibility internal
ctox ticket self-work-transition --work-id "<work_id>" --state closed --transitioned-by "ctox" --note "<plain human completion note>" --visibility internal
```

Create an access request when onboarding cannot continue without rights or secrets:

```sh
ctox ticket access-request-put --system "<system>" --title "<title>" --body "<text>" --required-scopes "<csv>" --secret-refs "<csv>" --channels "mail,jami" --publish
```

Store a newly delivered secret only in the encrypted local secret store:

```sh
ctox secret put --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>'
```

Ingest monitoring evidence into the knowledge plane when a monitoring system is available:

```sh
ctox ticket monitoring-ingest --system "<system>" --snapshot-json '<json>'
```

Build and activate a desk-specific operating skill when the source has enough history:

```sh
python3 skills/system/ticket-system-onboarding/scripts/bootstrap_ticket_source_skill.py \
  --system "<system>" \
  --skill-name "<system>-desk-operator" \
  --dataset-label "<human dataset label>" \
  --goal "<handle tickets in the historically observed desk style>" \
  --analysis-dir "runtime/output/<system>_desk_skill"
```

This tool will:

1. export canonical ticket history from the SQLite mirror
2. build the operating-model skill from that exported history
3. run the generated-skill evaluation against the same exported history
4. activate the generated skill for the source with `ctox ticket source-skill-set`

## Recommended Onboarding Work Types

These are templates, not mandatory fixed outputs:

- `ticket-system-onboarding`
- `label-landscape-review`
- `glossary-candidate-review`
- `service-catalog-seeding`
- `infrastructure-map-review`
- `team-model-review`
- `access-request`
- `monitoring-landscape-review`
- `adoption-gap-review`

Create only the items that are supported by observed evidence.

## Operating Pattern

The default execution path is the deterministic runner:

```sh
python3 skills/system/ticket-system-onboarding/scripts/run_onboarding_plan.py \
  --ctox-bin "<path-to-ctox>" \
  --system "<system>" \
  --env-file "<runtime.env>" \
  --publish
```

This runner must work the stages in order and only stop at explicit blockers.

Manual building blocks remain available when the operator asks for an adjustment:

1. Sync and inspect the observed source knowledge.
2. Create or advance exactly one visible onboarding guide for the source.
3. Create only justified onboarding work items through `self-work-put`.
4. Publish them only when the remote review surface is useful.
5. When published, immediately assign the work to CTOX if supported and leave an initial internal note with the concrete next step or blocker.
6. Work the item through notes and transitions instead of spawning more tickets than necessary.
7. Keep the remote ticket concise, human, and operator-facing; keep the real model in SQLite.
8. When rights or secrets are missing, create an explicit access request and use mail or Jami if the decision cannot be closed inside the ticket surface.
9. Ingest monitoring observations into the knowledge plane instead of leaving them as free-form ticket prose.
10. Once the source has enough history, build a desk-specific operating skill and bind it to the source with `ctox ticket source-skill-set`.
11. Re-run the onboarding guide step after new evidence appears. The guide should loosen as active source skills, confirmed runbooks, and real assigned work accumulate.

## Skill Activation Check

After building a desk-specific skill, verify that live ticket routing prefers it:

```sh
ctox ticket source-skills --system "<system>"
```

You are done with activation only when:

- the generated skill exists under `runtime/generated-skills/`
- the source binding is visible through `ctox ticket source-skills`
- new routed ticket work for that source receives the generated skill as the preferred skill instead of only a generic onboarding skill

The handoff rule is:

- before an active source skill exists, live work should prefer `ticket-system-onboarding`
- once an active source skill exists, normal work should prefer that source skill
- the onboarding skill should then remain only for the guide itself, explicit onboarding work, and exception correction

## Evaluation Loop

Every onboarding round that creates or updates a desk-specific skill must be evaluated qualitatively.

Minimum loop:

1. pick 3 to 5 real open or historically representative tickets from the source
2. query the generated skill against those tickets
3. inspect whether the suggested family, next steps, escalation threshold, and note style are actually useful
4. if they are weak, rebuild or refine the skill before leaving it active

Do not treat “skill file exists” as success. Activation without qualitative usefulness is not a finished onboarding step.

## Remote Writing Style

Everything written into the remote ticket system must read like a coworker update.

Good remote note shape:

- what you have actually observed
- why that matters for operations
- what the non-obvious lever is
- what is blocked or still unclear
- which exact decision you now need

Never write remote notes that mention:

- SQLite
- database tables
- metadata blobs
- case IDs, bundle versions, or control schemas unless an operator truly needs them
- CLI commands for normal understanding
- parser-like field dumps

Never publish onboarding tickets that lack:

- concrete counts
- example tickets
- an operational implication
- a hidden or non-obvious lever
- a clear decision or correction request

Do not copy structured metadata into the ticket body. Remote ticket text must be authored fresh in plain language.

## Important Boundaries

- Do not assume every ticket system needs the same first tickets.
- Do not encode onboarding behavior in the ticket kernel.
- Do not treat the remote ticket text as canonical source truth after publication.
- Do not create self-work spam; prefer fewer, durable review items over many one-off tickets.
- Do not store raw secret values in ticket work or ticket knowledge. Only the encrypted local secret store may hold real secret material.
- Do not leave machine-authored meta commentary in remote tickets.
- Do not bypass the generic ticket primitives with direct remote API calls.
- Do not create more than one visible onboarding guide per source system. Advance the same guide through notes and state changes.
