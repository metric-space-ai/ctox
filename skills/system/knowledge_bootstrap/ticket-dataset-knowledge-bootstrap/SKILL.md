---
name: ticket-dataset-knowledge-bootstrap
description: Build a real ticket knowledge base from a large ticket export or record list by deriving a source profile, promoted taxonomies, representative examples, and reusable projections instead of stopping at counts or flat summaries.
metadata:
  short-description: Build a reusable ticket knowledge base from a ticket dataset
cluster: knowledge_bootstrap
---

# Ticket Dataset Knowledge Bootstrap

Use this skill when CTOX has a large ticket export, workbook, CSV, JSON list, or API record dump and needs to turn it into a reusable knowledge base.

This work is only durable when the resulting source profile, taxonomies, glossary, and projections are imported or reflected into SQLite-backed ticket/discovery knowledge. Generated files alone do not count as completed knowledge.

This is not normal ticket handling.
This is a dataset-to-knowledge bootstrap skill.

The target is:

- a source profile
- promoted taxonomy dimensions
- taxonomy buckets with representative examples
- a glossary
- downstream projection hints for ticket onboarding and ticket knowledge

Read these first:

- [../../contracts/tabular-knowledge-taxonomy.md](../../../contracts/tabular-knowledge-taxonomy.md)
- [../tabular-knowledge-bootstrap/SKILL.md](../tabular-knowledge-bootstrap/SKILL.md)
- [references/pipeline.md](references/pipeline.md)

## Output Contract

The run is only acceptable if it produces:

- `source_profile.json`
- `taxonomies.json`
- `glossary.json`
- `semantic_clusters.json`
- `knowledgebase.md`

The output must contain:

- at least one promoted taxonomy dimension
- buckets under that dimension
- canonical and edge examples for those buckets
- projection guidance for later ticket onboarding

## Runtime Approach

Use embeddings for semantic pattern discovery.
Use a small LLM for taxonomy and cluster naming.

The intended exemplar path is:

- local embedding model on the host for row semantics
- `gpt-5.4-nano` for cluster naming, bucket descriptions, glossary cleanup, and projection notes

If the host or batch runner cannot sustain long mixed embedding-plus-LLM jobs, run the same pipeline in staged mode:

- keep embeddings on-host
- use `--semantic-naming-mode deterministic` and/or `--glossary-mode deterministic` for the first durable pass
- then refine selected buckets later with small separate `gpt-5.4-nano` micro-jobs

## Canonical Run Shape

1. inspect the workbook or dataset shape
2. build the source profile
3. promote strong categorical taxonomies from structured columns
4. build semantic issue-pattern clusters from embeddings
5. use the small LLM to name and describe those semantic buckets
6. extract glossary candidates and clean them
7. write the final knowledge artifacts

## Script

Run the bundled script:

```bash
python3 skills/system/knowledge_bootstrap/ticket-dataset-knowledge-bootstrap/scripts/build_ticket_dataset_knowledgebase.py \
  --input-xlsx <path> \
  --output-dir <dir> \
  --embedding-provider sentence-transformers \
  --embedding-model Qwen/Qwen3-Embedding-0.6B \
  --openai-model gpt-5.4-nano \
  --openai-api-key-env OPENAI_API_KEY
```

If the dataset is too large for one semantic pass, limit semantic clustering through:

```bash
  --max-semantic-rows 4000
```

Host-constrained staged exemplar:

```bash
python3 skills/system/knowledge_bootstrap/ticket-dataset-knowledge-bootstrap/scripts/build_ticket_dataset_knowledgebase.py \
  --input-xlsx <path> \
  --output-dir <dir> \
  --embedding-provider sentence-transformers \
  --embedding-model Qwen/Qwen3-Embedding-0.6B \
  --openai-model gpt-5.4-nano \
  --max-semantic-rows 120 \
  --semantic-naming-mode deterministic \
  --glossary-mode deterministic
```

## Guardrails

- Do not claim “knowledge base” if the run only produced counts.
- Do not stop at deterministic value groupings.
- Do not promote every column to a taxonomy.
- Do not let semantic clusters exist without human-readable names and examples.
- Do not write vague prose instead of durable artifacts.
