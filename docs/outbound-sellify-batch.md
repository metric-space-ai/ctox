# Outbound Sellify lookup batches

The existing policy-gated `outbound.sellify.lookup` action accepts an optional
`batch` array. Browser callers use the native WebRTC request/command path; this
does not add an HTTP data endpoint, an agent task or a record mutation.
The old single-query and campaign-grouping response shapes remain unchanged.

```json
{
  "batch": [
    {
      "key": "lead-a/company",
      "entity": "company",
      "selectors": [{ "field": "contact_id", "value": "17612" }],
      "fields": ["contact_id", "name", "payload.sql.note"],
      "limit": 25
    },
    {
      "key": "lead-a/people",
      "entity": "person",
      "selectors": [{ "field": "contact_id", "value": "17612" }],
      "fields": ["person_id", "display_name", "email"],
      "limit": 25
    }
  ]
}
```

The response schema is `ctox.outbound.sellify_lookup_batch.v1`, with `results`
in request order. Each result has the original unique `key`, its `entity`,
projected `records` and a `complete` boolean. Match criteria retain the existing
company/person selector whitelist and indexed exact lookup. Each used CRM
collection has one required read-only transaction for the entire batch;
transactions across different collections are not a single atomic snapshot.

There are 1–50 requests, at most 16 exact and 16 fuzzy selectors per request,
1–32 explicit output field paths and a result limit of 1–50. Across the whole
batch there are at most 200 selectors, including at most eight fuzzy probes;
one-character fuzzy probes are rejected rather than reported as executed-empty.
Internal record-ID
queries, nested batches and campaign grouping are rejected in batch mode.
The complete response must fit 1 MiB; exceeding that budget is an error, not a
partial successful receipt. A missing/unreadable collection is also an error.

The native reader requests one extra distinct match to detect truncation,
including overlapping selectors. `complete: false` means the consumer must
narrow or split the probe; it must never mean “new company”, “no contacts” or
“no restriction”. Only a readable, completed probe can establish an empty
match result. Record IDs and existing deletion indicators survive projection.

Nested output fields are an optimization, not a new authorization. An absent
output field is absent data; `complete` describes match enumeration only. It
does not attest a remark interpretation, recipient eligibility or research
provenance. Sellify values are still internal CRM input, not independent
external research evidence.

The Outbound client has not yet been changed to use this contract. In
particular, this backend package alone does not remove the old remark-agent
tasks, fix duplicate campaign membership or prove customer latency. Those
remain separate integration and installed-acceptance obligations.
