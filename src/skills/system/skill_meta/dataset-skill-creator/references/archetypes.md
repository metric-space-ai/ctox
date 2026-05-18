# Dataset Skill Archetypes

Choose the generated skill shape from the operating problem, not from the file format.

## `operating-model`

Use when the dataset should teach CTOX how a team, desk, or system usually works.

Typical output:

- recurring families or cases
- handling playbooks
- historical examples
- decision-support retrieval

Good fits:

- helpdesk ticket history
- incident history
- change records
- monitoring events with operator actions

## `lookup-reference`

Use when the dataset is mostly authoritative reference material.

Typical output:

- schema references
- catalogs
- terminology
- lookup helpers

Good fits:

- asset inventories
- CMDB exports
- service catalogs
- entitlement lists

## `workflow`

Use when the dataset shows repeated process steps that should become explicit procedure.

Typical output:

- ordered stages
- branching conditions
- success criteria
- scripts for repeatable execution

Good fits:

- deployment records
- repair logs
- operations run histories

## `policy-gate`

Use when the dataset primarily encodes approval, compliance, or eligibility boundaries.

Typical output:

- allow/deny rules
- escalation rules
- approval boundaries
- evidence requirements

Good fits:

- approval records
- entitlement and security exceptions
- policy decision exports

## Selection Rule

If the generated skill should answer:

- “How do operators usually handle this?” -> `operating-model`
- “What is true in this system?” -> `lookup-reference`
- “What steps should I follow?” -> `workflow`
- “May I do this, and what evidence is required?” -> `policy-gate`
