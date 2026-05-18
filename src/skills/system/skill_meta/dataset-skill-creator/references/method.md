# Dataset Skill Creation Method

This skill turns a dataset or dataset-derived bundle into a new reusable skill.

## 1. Start From The Operating Goal

First answer:

- what should another CTOX instance be able to do after this skill exists
- what kinds of user requests should trigger it
- what the skill should help decide, retrieve, or execute

Do not start from the file format alone.

## 2. Require Durable Evidence

Before generating a skill, ensure there is a durable evidence bundle.

Examples:

- `family_playbooks.json`
- `operating_model.md`
- `source_profile.json`
- `taxonomies.json`
- curated historical examples
- query helper scripts

If the evidence is still missing, first run the appropriate analysis skill.

For raw datasets, prefer a two-stage path:

1. analysis skill
2. skill promotion

That is exactly what `bootstrap_dataset_skill.py` orchestrates.

## 3. Choose The Skill Archetype

Select one archetype from [archetypes.md](archetypes.md).

That determines:

- what belongs in `SKILL.md`
- what belongs in `references/`
- whether `scripts/` are necessary
- how the generated skill should be phrased

## 4. Promote The Right Artifacts

The generated skill should only carry artifacts that another CTOX instance needs repeatedly.

Promote:

- stable references
- deterministic scripts
- templates that shape output

Do not promote:

- extraction chatter
- notebook-style experiment notes
- raw logs unless they are canonical examples

## 5. Write The Generated Skill As A User-Facing Capability

The generated skill must read like:

- what this skill does
- when to use it
- what reference files to consult
- what scripts to run
- what success looks like

Not like:

- how the analysis bundle was parsed
- how SQLite stores the truth
- which preprocessing tricks were used

## 6. Validate The Generated Skill

The generated skill should pass:

- structural validation
- trigger sanity
- reference completeness
- at least one realistic usage check

The right test is:

“Would another CTOX instance be helped by this skill when facing the target task?”
