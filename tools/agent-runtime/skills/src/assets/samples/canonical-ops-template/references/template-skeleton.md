# Canonical Ops Skill Skeleton

Every family skill should stay close to this layout.

## Required Sections

1. frontmatter
2. title
3. short purpose statement
4. sibling boundary statement
5. operating model
6. preferred helper scripts
7. tool contracts
8. workflow
9. operator feedback contract
10. guardrails
11. resources

## Locked Sections

These sections are treated as identity-bearing unless explicitly reapproved:

- frontmatter
- purpose statement
- sibling boundary statement
- shared-kernel commitment
- guardrails

## Editable Sections

These sections may be refined more freely:

- preferred helper scripts
- tool contracts
- workflow details
- operator-facing response shape
- resource lists
- completion gates

## Skeleton Example

```md
---
name: <skill-name>
description: <skill-trigger-description>
---

# <Skill Title>

Use this skill when ...

Do not use it for ...

This skill uses the shared SQLite kernel via `skill_key=<skill_key>`.

## Operating Model

...

## Preferred Helpers

...

## Tool Contracts

...

## Workflow

...

## Operator Feedback Contract

...

## Guardrails

...

## Resources

...
```
