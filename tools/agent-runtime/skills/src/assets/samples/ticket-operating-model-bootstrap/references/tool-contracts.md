# Tool Contracts

## build_ticket_operating_model.py

Builds the durable operating model from a historical ticket dataset.

Inputs:

- `--input-xlsx`
- `--output-dir`
- `--top-families`
- `--min-family-size`
- optional embedding provider and model

Outputs:

- `operating_families.json`
- `family_playbooks.json`
- `state_transition_norms.json`
- `note_style_refs.json`
- `retrieval_index.jsonl`
- optional `retrieval_vectors.npy`
- `operating_model.md`

Each `family_playbooks.json` entry must expose:

- family signals
- usual handling norms
- `decision_support`
- historical examples
- likely sources

Optional refinement flags:

- `--openai-model`
- `--openai-base-url`
- `--openai-api-key-env`
- `--openai-refine-limit`

## query_ticket_operating_model.py

Retrieves useful historical references for a new live ticket.

Inputs:

- `--model-dir`
- `--query`
- optional filters:
  - `--family`
  - `--request-type`
  - `--category`
  - `--top-k`

Behavior:

- loads the family cards and example cards
- applies explicit filters first
- if vectors exist, uses embedding similarity
- otherwise uses text overlap scoring
- returns family-ranked decision support for a live ticket turn:
  - why the family matches
  - what operators usually do next
  - what closure looks like
  - which historical examples are most relevant
