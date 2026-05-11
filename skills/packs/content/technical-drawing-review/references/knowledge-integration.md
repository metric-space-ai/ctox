# CTOX Knowledge Integration

The technical drawing review skill must use CTOX source-skill knowledge when it is available.

## Runtime Identity

- Source system: `technical-drawing-review`
- Main skill: `technical-drawing-review-main`
- Skillbook: `technical-drawing-review-skillbook`
- Runbook: `technical-drawing-review-runbook`
- Retrieval unit: `knowledge_runbook_items`

## Bootstrap

Build the seed bundle:

```bash
skills/packs/content/technical-drawing-review/scripts/build_learning_seed_bundle.py \
  --output-dir output/technical-drawing-review/knowledge-seed
```

Import into CTOX runtime knowledge:

```bash
ctox ticket source-skill-import-bundle \
  --system technical-drawing-review \
  --bundle-dir output/technical-drawing-review/knowledge-seed \
  --skip-embeddings
```

Use `--skip-embeddings` only for local smoke tests. For retrieval quality in a live system, import without it so `knowledge_embeddings` is populated.

## Before Each Review

Query the runbook layer with the package/review context:

```bash
ctox ticket source-skill-query \
  --system technical-drawing-review \
  --query "<drawing type, process, package shape, suspected issue family>" \
  --top-k 3
```

Load the matching item guidance into the vision review prompt. If no match exists, continue with the base checklist and record the gap if it recurs.

## After Human Feedback

When communication arrives after a review, convert it into an explicit learning handoff:

```bash
skills/packs/content/technical-drawing-review/scripts/ingest_review_feedback.py \
  --feedback-file feedback.txt \
  --findings findings.json \
  --manifest output/technical-drawing-review/work/manifest.json \
  --review-artifact output/technical-drawing-review/review.html \
  --output-dir output/technical-drawing-review/feedback
```

The script writes `feedback_learning.json`. If the feedback is reusable, it also writes `runbook_candidate.json`.

Classify feedback with these rules:

- One-off drawing fact or customer decision: store as ticket/context evidence.
- False positive caused by missing visible context: refine prompts or mark as `needs_context`.
- Reusable review rule with evidence: promote to a new or corrected runbook item.

For case-bound communication, publish the reusable correction as a CTOX learning candidate:

```bash
skills/packs/content/technical-drawing-review/scripts/ingest_review_feedback.py \
  --feedback-file feedback.txt \
  --findings findings.json \
  --manifest output/technical-drawing-review/work/manifest.json \
  --review-artifact output/technical-drawing-review/review.html \
  --output-dir output/technical-drawing-review/feedback \
  --publish \
  --case-id <case-id> \
  --ctox-bin target/debug/ctox \
  --workspace-root /Users/michaelwelsch/Documents/ctox
```

`ctox ticket learn-candidate-create` is intentionally used only with a real case because CTOX requires a case and dry-run. When no case exists, use the same script with `--publish` and no `--case-id`; it creates CTOX self-work. Add `--remote-publish-self-work` only when the configured ticket adapter should publish the work item externally:

```bash
ctox ticket self-work-list \
  --system technical-drawing-review \
  --state open
```

After owner approval, promote the reusable rule into the source-skill bundle and re-import:

```bash
skills/packs/content/technical-drawing-review/scripts/ingest_review_feedback.py \
  --feedback-file feedback.txt \
  --findings findings.json \
  --manifest output/technical-drawing-review/work/manifest.json \
  --review-artifact output/technical-drawing-review/review.html \
  --output-dir output/technical-drawing-review/feedback \
  --promote-to-bundle output/technical-drawing-review/knowledge-seed

ctox ticket source-skill-import-bundle \
  --system technical-drawing-review \
  --bundle-dir output/technical-drawing-review/knowledge-seed
```

Promotion requirements:

- stable label
- bounded trigger conditions
- explicit tool actions
- explicit verification
- explicit writeback policy
- source evidence from reviewed drawing, human correction, standard, or accepted review

Do not treat workspace notes, generated HTML, or raw findings JSON as durable learning by themselves. Durable reusable learning must land in Skillbook/Runbook/Runbook-Item records.
