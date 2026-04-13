# Ops Skill Family Invariants

These invariants are the stable contract for the CTOX ops skill family.

## Persistence

- one shared SQLite kernel
- same five tables for all family skills
- separation via `skill_key`
- no parallel per-skill table families without explicit approval

## Evidence

- raw captures are the authority
- normalized entities and relations are downstream interpretation
- helper scripts may assist, but must not become opaque authority

## Runtime

- no second execution loop
- no hidden daemons outside the CTOX substrate
- schedule, queue, plan, follow-up, and service loop remain the canonical execution path

## Skill Family

- each skill has one narrow operational focus
- sibling-skill boundaries must be explicit
- no omnibus skill that absorbs multiple family responsibilities without review
- operator-facing replies must distinguish `proposed`, `prepared`, `executed`, and `blocked`
- internal persistence is not the lead section of a user-facing answer

## Refinement

- smallest effective intervention first
- direct full rewrite is never the default move
- family consistency beats host-local convenience when the two conflict
