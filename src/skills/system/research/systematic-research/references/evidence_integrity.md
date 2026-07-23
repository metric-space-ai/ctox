# Evidence Integrity Contract

Use this contract for every durable systematic-research run. Discovery is a
candidate queue; it is never an evidence register and its fields (`title`,
`snippet`, DOI, abstract, ranking, or metadata) must not support a claim.

## Eligible evidence

An evidence entry is eligible only when all of these are true:

- `evidence_status=eligible`, `http_status` is 200–299, and the URL is the
  current canonical URL of the publisher, institution, original data owner,
  standards body, or lawful repository.
- `url_role` is `original_content` or `original_data`. DOI resolvers,
  publisher/DOI landing pages, metadata APIs, aggregators, mirrors, search
  results, tertiary encyclopedias, and snippets are discovery-only. Canonical
  original URLs must be unique; URL aliases do not count as additional sources.
- `freshness_status=current` is recorded at retrieval time and the snapshot is
  the bytes actually downloaded from that URL. A failed, stale, redirected,
  login, cookie, JavaScript shell, metadata, abstract-only, or snippet read is
  rejected, never repaired from model memory.
- Every admitted Web Stack read has a
  `ctox.web-read.workspace-evidence.v3` receipt artifact. The manifest must
  repeat its requested URL, final URL, status, `checked_at_epoch`, byte count,
  content kind, and response hash exactly. The guard verifies the artifact
  hash and every repeated field, including the machine-computed relevance
  score and eligibility decision; a worker cannot rewrite a redirect,
  interstitial URL, timestamp, content type, or response identity after the
  read. The evidence `canonical_url` must equal the persisted final URL.
- Final evidence must come from a direct typed `ctox_web_read` call. The
  persisted receipt path and SHA-256 must be present in the immutable harness
  rollout for that call. Deep Research output is discovery inventory only.
  Never create, copy, reconstruct, rename, or rewrite `receipt.json`; if a
  direct read did not persist `workspace_evidence`, the source is ineligible.
- `content_scope=full_text` for prose or `content_kind=data_file` for data.
  The snapshot is non-empty, its SHA-256 is recorded in both the snapshot and
  evidence row, and the hash verifies before every downstream use.
- Prose evidence carries a server-extracted full-text artifact whose SHA-256
  is verified and whose receipt is bound to the original snapshot SHA-256.
  Its `byte_count` must equal the extracted-text artifact's own byte size —
  for PDF sources the extracted-text byte count is never the PDF source byte
  count — and the manifest `extracted_text` path/SHA-256 must match the
  server-written receipt's `extracted_text_path`/`extracted_text_sha256`.
  Every claim includes a verbatim `evidence_quote` of at least six words and
  40 characters; the guard requires that normalized quote to occur in that
  extracted text. Data-file claims have the same quote requirement and must
  bind `data_excerpt` to either the original text snapshot or a hash-verified
  ZIP member chain. The guard reads those bytes itself. A plausible
  paraphrase, model-written extract, or unbound transformed table is rejected.
- `relevance_score` is the exact integer `evidence_relevance_score` returned
  by the current typed `ctox_web_read`, is between 8 and 10 inclusive, and has
  a short reason tied to the research facet. Never estimate, rescale, round,
  or overwrite this machine-computed value. A shared keyword is not a
  relevance decision.

Required lineage is immutable and exact:

```text
claim_id -> evidence_id -> snapshot_id -> source_id -> canonical_url
```

For prose evidence, copy the paths and hashes from the Web Stack workspace
receipt into the manifest using this shape. Paths are relative to the task
workspace root, even though the manifest itself lives below `validation/`:

```json
{
  "schema_version": "ctox.research.evidence.v2",
  "evidence": [{
    "snapshot_sha256": "<original-response-sha256>",
    "retrieval_receipt": {
      "request_url": "<persisted-requested-url>",
      "final_url": "<persisted-final-url-and-canonical-url>",
      "http_status": 200,
      "checked_at_epoch": 1768640000,
      "byte_count": 12345,
      "body_sha256": "<original-response-sha256>",
      "content_kind": "html",
      "receipt_artifact": {
        "path": ".ctox/web-read/<call-id>/receipt.json",
        "sha256": "<receipt-artifact-sha256>"
      }
    },
    "extracted_text": {
      "path": "snapshots/source-0001.extracted.txt",
      "sha256": "<extracted-text-sha256>",
      "byte_count": 23456,
      "source_snapshot_sha256": "<original-response-sha256>"
    }
  }],
  "claims": [{
    "evidence_quote": "<verbatim passage present in extracted text>"
  }]
}
```

Claims repeat the linked IDs and URL and carry `lineage_sha256`, computed over
`claim_id`, `claim_text`, `evidence_quote`, `evidence_id`, `snapshot_id`,
`source_id`, `canonical_url`, and `data_excerpt`. Never update a snapshot, source URL,
evidence row, quote, or claim in place; create a new version and invalidate
dependants.

## Original data files

For every dataset used in a calculation, download the original file from its
canonical data URL. Record its SHA-256, byte count, format, parser and parser
version. Run a deterministic check that verifies the file hash, schema/column
names, row count, encoding/delimiter, units, null handling, and any required
checksums. Only `downloaded=true`, `original_data=true`, `generated=false`,
`quarantine_status=accepted`, and `deterministic_check.status=pass` may enter a
table or claim. Missing, corrupt, ambiguous, transformed, or invented rows go
to a quarantine record with a reason; do not impute or fabricate replacements.

Every claim over original data also supplies one of these inspectable bindings:

```json
{
  "evidence_quote": "<at least 40 characters and six words>",
  "data_excerpt": {
    "extraction": "snapshot_text",
    "encoding": "utf-8",
    "source_snapshot_sha256": "<original-data-sha256>"
  }
}
```

For an archive member, use `extraction=zip_member_chain` and include every
nested member from the downloaded outer archive to the final text file:

```json
{
  "data_excerpt": {
    "extraction": "zip_member_chain",
    "encoding": "utf-8",
    "source_snapshot_sha256": "<outer-archive-sha256>",
    "member_chain": [
      {"path": "dataset.zip", "sha256": "<inner-archive-sha256>"},
      {"path": "data/results.csv", "sha256": "<csv-sha256>"}
    ]
  }
}
```

The final member must be ASCII or UTF-8 text. Binary XLSX, Parquet, or other
formats require a separately specified deterministic parser contract before
their values can support claims; an agent-authored text export is not enough.

## Review gate and living outputs

Before promotion, the deterministic evidence guard checks source authority,
canonical URL, current 2xx full content, freshness, hashes, original data,
parsing, units, derivations, and exact claim lineage. CTOX then runs its
independent service-owned completion review. Free subagents and parent-created
review threads are forbidden.

Living Knowledge and reports are append-only versions. A refresh creates a new
Knowledge version from new snapshots, reruns the evidence and completion
review gates, and records
explicit invalidations for claims/tables/reports that depended on superseded
snapshots. A report records its Knowledge version and claim IDs. A superseded
or invalidated version is never silently made current.

Run the deterministic guard before importing data, promoting Knowledge, or
publishing a report:

```sh
python3 src/skills/system/research/systematic-research/scripts/evidence_guard.py \
  /path/to/evidence-manifest.json
```

The guard exits non-zero on any missing or inconsistent field.
