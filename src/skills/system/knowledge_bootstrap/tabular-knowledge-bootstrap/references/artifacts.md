# Tabular Knowledge Artifacts

This reference defines the durable artifact set expected from a serious tabular knowledge run.

## 1. Source Profile

Minimum content:

- `source_scope`
- `row_unit_description`
- `tables_or_sheets`
- `key_columns`
- `enum_like_columns`
- `free_text_columns`
- `ownership_columns`
- `reference_columns`
- `ambiguities`

The source profile is incomplete if it cannot explain what one row basically means.

## 2. Taxonomy Dimension

Each promoted dimension should have:

- `dimension_name`
- `purpose`
- `inference_basis`
- `source_fields`
- `bucket_count`
- `stability_note`

Examples of good dimension names:

- `queue_family`
- `service_family`
- `ownership_family`
- `issue_pattern`
- `access_scope`
- `monitoring_coverage`

Bad names:

- vague headings copied from raw columns
- temporary spreadsheet tab names
- opaque internal codes with no interpretation

## 3. Taxonomy Bucket

Each bucket should have:

- `bucket_name`
- `dimension_name`
- `definition`
- `inclusion_signals`
- `exclusion_signals`
- `row_count_estimate`
- `confidence`

The definition must be understandable without reopening the original sheet.

## 4. Example Set

Each bucket needs an example set:

- `canonical_examples`
- `common_examples`
- `edge_examples`

Each example should retain:

- a stable row reference or source reference
- a short explanation of why it belongs

If the bucket is fuzzy, include at least one edge example.

## 5. Candidate Vs Promoted State

Use this distinction strictly:

- `candidate`
  - plausible but not yet stable
- `promoted`
  - stable enough for downstream use
- `rejected`
  - looked promising but turned out noisy or incoherent

Do not let downstream knowledge planes consume mere candidates unless the user explicitly wants speculative output.

## 6. Projection View

When a promoted taxonomy is consumed downstream, record:

- `projection_target`
- `source_dimension`
- `buckets_used`
- `projection_rule`
- `known_gaps`

Examples:

- `projection_target=ticket_knowledge`
- `source_dimension=issue_pattern`
- `projection_target=service_catalog`
- `source_dimension=service_family`

## 7. Completion Check

A run is not done unless it can show:

1. source profile
2. at least one promoted taxonomy dimension
3. buckets under that dimension
4. example sets for those buckets
5. any projection that consumed them

If any of those are missing, the run is still exploratory.
