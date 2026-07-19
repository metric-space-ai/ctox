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
  results, and snippets are discovery-only.
- `freshness_status=current` is recorded at retrieval time and the snapshot is
  the bytes actually downloaded from that URL. A failed, stale, redirected,
  login, cookie, JavaScript shell, metadata, abstract-only, or snippet read is
  rejected, never repaired from model memory.
- Every admitted Web Stack read has a
  `ctox.web-read.workspace-evidence.v2` receipt artifact. The manifest must
  repeat its requested URL, final URL, status, `checked_at_epoch`, byte count,
  content kind, and response hash exactly. The guard verifies the artifact
  hash and every repeated field; a worker cannot rewrite a redirect,
  interstitial URL, timestamp, content type, or response identity after the
  read. The evidence `canonical_url` must equal the persisted final URL.
- `content_scope=full_text` for prose or `content_kind=data_file` for data.
  The snapshot is non-empty, its SHA-256 is recorded in both the snapshot and
  evidence row, and the hash verifies before every downstream use.
- Prose evidence carries a server-extracted full-text artifact whose SHA-256
  is verified and whose receipt is bound to the original snapshot SHA-256.
  Every claim includes a verbatim `evidence_quote` of at least six words and
  40 characters; the guard requires that normalized quote to occur in that
  extracted text. A plausible paraphrase or model-written quote is rejected.
- `relevance_score` is numeric and at least 8/10, with a short reason tied to
  the research facet. A shared keyword is not a relevance decision.

Required lineage is immutable and exact:

```text
claim_id -> evidence_id -> snapshot_id -> source_id -> canonical_url
```

For prose evidence, copy the paths and hashes from the Web Stack workspace
receipt into the manifest using this shape (paths must be relative to the
manifest directory):

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
`source_id`, and `canonical_url`. Never update a snapshot, source URL,
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
