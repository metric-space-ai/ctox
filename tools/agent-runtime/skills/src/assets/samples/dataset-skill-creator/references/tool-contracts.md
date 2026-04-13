# Tool Contracts

## `create_dataset_skill.py`

Creates a new skill folder from a dataset-derived analysis bundle.

Inputs:

- `--skill-name`
- `--skill-path`
- `--archetype`
- `--dataset-label`
- `--goal`
- optional `--analysis-dir`
- optional `--query-command`
- optional `--display-name`
- optional `--short-description`
- optional `--default-prompt`

Behavior:

- creates the skill folder
- writes `SKILL.md`
- writes `agents/openai.yaml`
- writes reference docs
- copies durable analysis artifacts into `references/generated/` when an analysis directory is provided

Outputs:

- generated skill folder under `<skill-path>/<skill-name>/`

## `bootstrap_dataset_skill.py`

Runs the full path from raw dataset to generated skill.

Inputs:

- `--input`
- `--source-kind`
- `--skill-name`
- `--skill-path`
- `--archetype`
- `--dataset-label`
- `--goal`
- `--analysis-dir`
- optional analysis tuning flags

Behavior:

- selects an analysis strategy from source kind plus archetype
- runs the matching analysis skill script
- then calls `create_dataset_skill.py`

Current automatic strategy coverage:

- `ticket-history` + `operating-model` -> `ticket-operating-model-bootstrap`
- `ticket-history` + `lookup-reference` -> `ticket-dataset-knowledge-bootstrap`

## Generated Skill Contract

The generated skill must contain:

- a truthful trigger description
- a concise operator-facing workflow
- reference links to the promoted artifacts
- script/tool entrypoints when they are necessary
- a success/validation section
