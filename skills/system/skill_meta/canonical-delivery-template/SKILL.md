---
name: canonical-delivery-template
description: Canonical template and escalation policy for CTOX delivery skills. Use when creating or refining install, provisioning, configuration, migration, and secret-handling skills so they stay compatible with the shared SQLite evidence kernel and the operator-facing completion contract.
cluster: skill_meta
---

# Canonical Delivery Template

Any generated delivery guidance for CTOX mission work must assume that only SQLite-backed runtime state counts as durable knowledge. Workspace artifacts alone do not count as durable knowledge.

Use this skill when you are defining or changing the CTOX delivery-skill family.

Current delivery family:

- `service-deployment`
- `secret-management`
- `acceptance-verification`
- later `environment-provisioning`
- later `configuration-rollout`
- later `delivery-refinement`

This is not a host execution skill. It is the canonical template for delivery work.

## Family Invariants

1. Delivery work uses the same shared SQLite kernel as ops work:
   - `discovery_run`
   - `discovery_capture`
   - `discovery_entity`
   - `discovery_relation`
   - `discovery_evidence`
2. Delivery work must separate:
   - target state
   - preflight evidence
   - credential classification
   - executed change
   - verification result
3. Helper scripts are inspectable resources, not hidden authority.
4. Operator-facing replies must distinguish:
   - `proposed`
   - `prepared`
   - `executed`
   - `blocked`
   - `needs_repair`
5. High-impact work must never end with a vague promise. It must either:
   - complete
   - block with exact missing inputs
   - fail verification with an explicit repair path
   - or leave a durable next-work record
6. Secret material must not be silently forgotten. If CTOX generates or discovers credentials, it must persist a concrete local secret reference or say exactly why it could not.
7. Local installations and external integrations are different work shapes. The family must classify that difference early.

## Section Policy

Locked by default:

- frontmatter `name`
- family invariants
- shared SQLite commitment
- no-hidden-authority rule
- operator completion contract

Editable:

- helper scripts
- workflow detail
- examples
- fallback notes
- completion gates

Candidate only:

- boundary changes between delivery skills
- kernel changes
- full rewrites

## Delivery Escalation Rule

For delivery-family refinement, always choose the smallest effective change:

1. use the canonical skill as-is
2. patch helper scripts and tests
3. patch editable sections in `SKILL.md`
4. propose structural skill changes
5. only under the highest gate, rewrite the skill

## Resources

- [references/delivery-invariants.md](references/delivery-invariants.md)
- [references/template-skeleton.md](references/template-skeleton.md)
