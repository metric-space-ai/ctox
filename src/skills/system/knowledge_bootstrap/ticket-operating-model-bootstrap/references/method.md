# Ticket Operating Model Method

This skill learns how a desk works, not just what its tickets talk about.

## 1. Build Ticket Families

Derive recurring families from stable operational fields such as:

- request type
- category
- subcategory

The family should be specific enough to reuse, but broad enough to have repeated historical evidence.

## 2. Derive Historical Handling Norms

For each strong family, learn:

- common intake channel
- common lifecycle and closure states
- common reporter or source patterns
- repeated action wording from historical notes
- repeated request wording from incoming descriptions

## 3. Select Good Historical Examples

For each family keep:

- canonical examples
- common examples
- note-style examples
- closure examples

Examples are what later make the skill operational instead of abstract.

## 4. Write Family Playbooks

Each family playbook should answer:

- what signals identify this family
- what operators usually do first
- what usually happens next
- what kind of notes are normal
- what closure looks like
- what similar past cases exist

The family playbook is not complete until it contains a `decision_support` view for live work:

- `operator_summary`
- `triage_focus`
- `handling_steps`
- `close_when`
- `caution_signals`
- `note_guidance`

This is the part CTOX can actually reuse while handling a new live ticket.

## 5. Build Retrieval Artifacts

The skill should build a fast lookup surface for later ticket turns:

- family summaries
- representative examples
- optional embedding vectors

That lets CTOX answer a live question such as:
"what historically similar cases and handling playbooks match this new ticket, and what should I do first?"

## 6. Optional LLM Refinement

When a compact model such as `gpt-5.4-nano` is available, refine the strongest families from evidence into tighter operator-facing decision support.

The refinement step is allowed to improve:

- the operator summary
- the triage focus
- handling step wording
- closure wording
- caution wording
- note guidance

It is not allowed to invent systems, ownership, or steps that are unsupported by the evidence.
