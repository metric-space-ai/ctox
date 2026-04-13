---
name: canonical-ops-template
description: Canonical template and refinement policy for CTOX operational system skills. Use when creating, updating, or refining the shared ops skill family so discovery, reliability, incident, change, security, recovery, automation, and insight remain compatible through one SQLite evidence kernel, open helper resources, and consistent escalation boundaries.
---

# Canonical Ops Template

Use this skill when you are defining or changing a CTOX ops skill in the shared family:

- `discovery_graph`
- `reliability_ops`
- `incident_response`
- `change_lifecycle`
- `security_posture`
- `recovery_assurance`
- `automation_engineering`
- `ops_insight`
- later `refinement`

This is not an execution skill for host work.

It is the canonical template and governance layer for the ops-skill family.

## Purpose

The family must stay:

- operationally useful
- skill-separated
- SQLite-compatible
- inspectable by the agent
- refineable without drifting into eight unrelated mini-systems

Use this template before changing a family skill.

## Family Invariants

These invariants are locked unless an outer governance decision explicitly changes them.

1. One shared SQLite evidence kernel.
   The family persists into the same 5-table kernel:
   - `discovery_run`
   - `discovery_capture`
   - `discovery_entity`
   - `discovery_relation`
   - `discovery_evidence`
2. Separation happens through `skill_key`, not through new parallel table families.
3. Raw evidence stays the source of truth.
4. Helper scripts are open resources, not hidden black-box authority.
5. Skills stay sharply separated by focus.
6. The current CTOX service loop stays the only execution loop.
7. Refinement must choose the smallest effective intervention first.
8. Operator-facing replies must clearly distinguish proposal, preparation, and execution.
9. Owner-facing continuity must consider the recent relevant communication history, not just the newest inbound line.

Read [references/family-invariants.md](references/family-invariants.md) before changing shared family behavior.

## Template Layers

Every ops skill in this family should be structured in the same layers:

1. `SKILL.md`
   - purpose
   - boundaries
   - operating model
   - tool contracts
   - workflow
   - guardrails
   - resources
2. `scripts/`
   - open helper resources
   - collectors
   - capture wrappers
   - store/query wrappers
   - bootstrap fallback normalizers when justified
3. `references/`
   - interpretation rules
   - command palettes
   - helper explanations
   - family-specific notes
   - operator-facing response contract

Read [references/template-skeleton.md](references/template-skeleton.md) for the canonical section layout.

## Locked vs Editable Zones

For this family, the default section policy is:

### Locked by Default

- frontmatter `name`
- frontmatter `description`
- skill purpose
- skill boundaries against other family skills
- shared SQLite kernel commitment
- no-hidden-loop rule
- guardrails that define mutation authority or autonomy level

### Editable by Refinement

- helper script set
- helper script invocation patterns
- interpretation references
- workflow details that do not change skill identity
- completion gates
- examples and fallback guidance

### Candidate Only

These may be proposed, but not silently promoted:

- skill handoff changes
- skill scope changes
- guardrail changes
- kernel changes
- complete rewrite of a family skill

Read [references/refinement-escalation.md](references/refinement-escalation.md) for the escalation ladder.

## System-Specific Joker Layer

System-specific adaptation should not be the first reason to rewrite a family skill.

Handle system specifics in this order:

1. use the canonical family skill as-is
2. use or add host-/site-specific helper scripts
3. add host-/site-specific references or fallback notes
4. patch explicitly editable sections
5. only then consider a candidate rewrite

The system-specific joker layer is therefore:

- helper scripts
- tests
- site-specific reference notes
- narrow fallback logic

Not:

- immediate rewrite of the base skill identity

## Refinement Escalation Rule

`refinement` must apply this order:

1. solve with the canonical template and current skill behavior
2. patch or add helper scripts and tests
3. patch only editable sectors in `SKILL.md`
4. propose candidate-level structural skill changes
5. only under the highest gate, propose a full skill rewrite

If a lower step can solve the problem, higher steps are not allowed.

## Family Boundary Rule

When updating or creating a family skill, always state:

- what this skill does
- what it explicitly does not do
- which sibling skill should be used instead when the scope shifts

This is mandatory. If the boundary is fuzzy, the skill is not ready.

## Operator Feedback Contract

Every family skill must produce a user-facing answer that is understandable to an operator.

The answer must clearly distinguish:

- what is only proposed
- what was prepared or researched
- what was actually executed
- what remains blocked, pending, or unverified

The answer must not begin with internal persistence details such as database writes or entity counts.

The default response order for family skills is:

1. current state or outcome
2. activation or execution state
3. concrete findings or configured scope
4. autonomous actions allowed or taken
5. escalations required or triggered
6. next recommended operator step

If a skill starts a high-impact or multi-step task and does not finish it in the same slice, the answer must also:

- say that the work is still open
- state whether the work is `prepared` or `blocked`
- point to the durable next-work record in queue or plan state instead of implying silent continuation

## Resources

- [references/family-invariants.md](references/family-invariants.md)
- [references/operator-feedback-contract.md](references/operator-feedback-contract.md)
- [references/refinement-escalation.md](references/refinement-escalation.md)
- [references/template-skeleton.md](references/template-skeleton.md)
