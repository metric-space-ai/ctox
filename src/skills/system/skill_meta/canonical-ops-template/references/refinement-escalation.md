# Refinement Escalation Ladder

This ladder defines what `refinement` should try, in order.

## Level 0: Use The Canonical Skill As-Is

First try to solve the problem by:

- using the current skill correctly
- using the current helper scripts correctly
- using the current references correctly

No mutation.

## Level 1: Helper-Script Refinement

Allowed:

- patch `scripts/*.py`
- add or remove helper scripts
- patch tests
- improve collectors, parsers, fallbacks, or wrappers

This is the normal refinement zone.

## Level 2: Reference And Editable-Sector Refinement

Allowed:

- patch `references/*.md`
- patch explicitly editable `SKILL.md` sectors

Not allowed here:

- changing skill identity
- changing family boundaries
- changing autonomy level

## Level 3: Candidate Structural Change

Allowed only as proposal:

- handoff changes
- skill-scope changes
- guardrail changes
- kernel changes
- large `SKILL.md` rewrites

This must produce a candidate patch, not silent promotion.

## Level 4: Full Skill Rewrite

This is the outermost escalation.

Only use when:

- levels 0 through 3 are insufficient
- the current skill is structurally wrong
- family consistency can still be preserved

Full rewrite must be treated as high-risk candidate work.
