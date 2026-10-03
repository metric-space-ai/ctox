# Spreadsheet snapshot report opening contract

Status: proposed contract and regression coverage; implementation and installed
acceptance are pending. This does not claim WELSCH or THESEN is repaired.

## Caller

An exporter passes the exact generated XLSX File to the existing shell action:

```js
ctx.actions.openApp('spreadsheets', {
  openFile: {
    file,
    source_kind: 'research_generated',
    open_purpose: 'snapshot_report',
    report_snapshot: {
      source_module: 'outbound',
      source_collection: 'leads',
      source_record_ids: savedRecordIds,
      captured_at_ms: capturedAt,
      file_sha256: exactFileSha256,
    },
  },
});
```

`file_sha256` is the lower-case 64-character SHA-256 hex digest of the exact
File bytes. Record IDs identify the saved rows used to build that file;
`captured_at_ms` is a positive finite timestamp for the snapshot. The descriptor
requires a nonempty source module, collection and record ID list.

## Receiving module

- Validate the descriptor and compare the digest before any chunk or record
  write. Missing or mismatched descriptors fail closed.
- Preserve `research_generated` in record and version ingestion metadata.
  Persist `open_purpose`, `report_snapshot` and `evidence_eligible: false` inside
  the existing `knowledge_lineage` object. Native commit already preserves this
  object; the descriptor refers to the original source snapshot after edits.
- Display the saved snapshot origin and its unverified status in the editor.
  The declared origin and digest bind the caller's file; they do not verify its
  factual content or authorize use as evidence.
- Keep the existing Research evidence guard. Caller-provided provenance or
  eligibility assertions cannot authorize an evidence opening.
- Resolve supplied source file IDs through shell database handles. An unresolved
  ID cannot become a user import merely because a snapshot descriptor is supplied.
- Reuse a record only when the source hash, ingestion kind and snapshot context
  are compatible. Identical bytes cannot launder an ordinary import into a
  Research report, nor merge reports for different source rows or timestamps.
- Preserve ordinary CSV/TSV/XLSX import behavior, write permissions, save-before-
  navigation safeguards and authoritative native SaveACK. Do not schedule Research
  or remark-check work merely to open this saved-state report.

## Verification

`spreadsheets.test.mjs` exercises the public file opening through the existing
Office bridge with controlled shell collections and source push acknowledgments.
It covers valid reports, exact persisted source bytes, source acknowledgment
before record/version references, opening from fresh app state, context-separated
deduplication, invalid metadata/hash, unresolved source IDs and forged evidence
eligibility. The fixture
contains a placeholder XLSX payload; it does not test parsing, the real editor,
WebRTC durability or native commit.

Implementation must additionally demonstrate the origin label and metadata on
committed versions. Final acceptance remains the real installed CSV/Create/Edit/
SaveACK/Reload/Reopen/XLSX journey after `ctox upgrade --dev` on WELSCH and THESEN,
using the coordinated production writer and existing browser lane.
