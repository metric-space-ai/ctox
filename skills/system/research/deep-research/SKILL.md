---
name: deep-research
description: Disabled reset placeholder. Do not use for production research, feasibility studies, market research, competitive analysis, or client-ready Word/PDF reports until rebuilt from scratch.
class: system
state: disabled
cluster: research
---

# Deep Research Reset

This skill has been deliberately reset.

Do not use the previous deep-research workflow, feasibility-study generator,
DOCX helper, validators, or pipeline. They were removed because they produced
client-facing reports that were structurally and editorially unacceptable.

## Current Status

- No report generator is available.
- No feasibility-study DOCX workflow is available.
- No bundled research modules are available.
- No scripts in this skill are valid production tools.

## Required Rebuild Rule

Before this skill can be used again, rebuild it from first principles:

1. Define the generic feasibility-study process as a state machine.
2. Define stage contracts before implementation.
3. Build one stage at a time with tests.
4. Validate against human-quality reference artifacts before exposing any
   client-facing report generation.
5. Do not claim that a Word document is client-ready until rendered output has
   been reviewed against an explicit report archetype.

Until that rebuild exists, route research work through ordinary Codex reasoning
and domain-specific tools, not this skill.
