# Tabular Knowledge Method

Use this reference when the source is large enough that a one-pass read would collapse into noise.

## 1. Structural Pass

The first pass is purely structural.

Determine per table or worksheet:

- approximate row count
- heading set
- likely primary key columns
- likely foreign-key or join columns
- enum-like columns
- high-cardinality free-text columns
- sparse columns
- date or time fields

Good questions:

- What does one row appear to represent?
- Which columns behave like identifiers?
- Which columns are likely routing or ownership controls?
- Which columns look like codes that require interpretation rather than display?

Do not infer domain meaning yet.

## 2. Semantic Column Pass

For each important column, classify it into one of these roles:

- `id`
- `display_name`
- `enum_state`
- `category_or_tag`
- `owner_or_team`
- `reference`
- `time`
- `free_text`
- `metric`
- `ranking_or_score`
- `unknown`

If a column could plausibly be two things, keep it ambiguous.

## 3. Candidate Taxonomy Pass

The goal is not “what columns exist” but “what stable axes of variation exist”.

Candidate taxonomy dimensions usually come from:

- repeated categorical values
- repeated combinations of two or three columns
- repeated workflow paths
- repeated owner/team routing
- repeated free-text clusters
- repeated co-occurrence between labels, services, states, or teams

Good taxonomy questions:

- Which field values partition the rows into operationally distinct families?
- Which row families keep recurring over time?
- Which free-text themes appear often enough to justify a reusable category?
- Which dimensions would help a later agent classify new rows?

## 4. Bucket Refinement Pass

Each candidate taxonomy must be refined into buckets.

For every bucket:

- define the bucket name
- define the inclusion rule
- identify rows that clearly belong
- identify ambiguous rows that should stay out

Reject buckets that are:

- too small and one-off
- semantically mixed
- dependent on noisy text fragments only
- impossible to name clearly

## 5. Example Selection Pass

For every promoted bucket, keep:

- `canonical examples`
  - the clearest rows that define the bucket
- `common examples`
  - rows that represent ordinary members
- `edge examples`
  - rows near the boundary

Do not keep giant example sets.
The point is to support explainability and future classification.

## 6. Promotion Pass

Promote a taxonomy dimension only if:

- it is repeated
- it is coherent
- it has distinct buckets
- each important bucket has examples
- the dimension can be named in stable operational language

Leave the rest as candidates.

## 7. Projection Pass

Only promoted taxonomies may feed downstream projections.

Examples:

- ticket onboarding may consume:
  - queue family
  - repeated issue pattern
  - service family
  - ownership family
- monitoring may consume:
  - service family
  - asset family
  - coverage gap
- access may consume:
  - team ownership
  - approval scope
  - environment family

Downstream systems must not silently invent new categories if the promoted taxonomy already covers the case.

## 8. Large Dataset Rule

Do not load the full dataset into one turn when it is large.

Instead work in slices:

- structural sample
- high-frequency bucket sample
- low-frequency/rare-value sample
- recency slice
- per-taxonomy refinement slice

The point is not to read every row at once.
The point is to infer stable structure and keep evidence traceable.
