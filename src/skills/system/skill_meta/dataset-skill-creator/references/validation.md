# Validation Checklist

Validate the generated skill against these questions:

## Trigger Quality

- Does the frontmatter description clearly say when to use the generated skill?
- Would another CTOX instance know from the description that this is the right skill?

## Artifact Quality

- Are the promoted references durable and reusable?
- Are weak or noisy artifacts excluded?
- Are helper scripts linked where the skill depends on them?

## Capability Quality

- Does the generated skill help another CTOX instance do real work?
- Does it answer the target operator problem?
- Does it avoid extraction chatter and tooling internals?

## Language Quality

- Does the generated `SKILL.md` avoid internal field names such as ``triage_focus`` or ``handling_steps`` in operator-facing instructions?
- Does it avoid wording that assumes source code, schema, parser, or JSON knowledge?
- Do the main work steps read like natural operator guidance instead of an artifact container?
- Are references and helper scripts supportive rather than the main content of the skill?

## Minimal Structural Checks

- `SKILL.md` exists and has valid frontmatter
- `agents/openai.yaml` exists
- `references/` exists
- generated references are linked from `SKILL.md`

## Recommended Mechanical Check

Run:

```bash
python3 /Users/michaelwelsch/.codex/skills/.system/skill-creator/scripts/quick_validate.py <generated-skill-dir>
```

If the environment lacks validator dependencies, at least confirm:

- `SKILL.md` exists and reads like a usable capability
- `agents/openai.yaml` exists
- the promoted references are present under `references/generated/`
- the helper entrypoint in `SKILL.md` points to a real script
- the language review is clean: no internal field-name leaks, no code-aware wording, no cryptic artifact jargon in the main operator guidance
