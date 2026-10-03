# Lossless module-source projection recovery

The historical generic projection clamp replaced source `content: string`
with an `_omitted` object above 256 KiB. This violated the source schema and
could recreate damaged projections at native peer startup. The full source
remained in the canonical store and, for installed applications, on disk.

Module source documents now retain their typed content within the existing
generic master-response ceiling (1 MiB of serialized JSON). Oversized source
documents fail explicitly without rewriting their content. This does not
raise a wire limit or introduce a second data path. It does not yet implement
chunked source files above that admitted inline size. Legacy startup repair
skips typed module-source records entirely: an existing oversized source must
neither be truncated nor abort peer bring-up. A real SQLite startup test
preserves a 2 MiB historical source, revision and write timestamp byte-for-byte
(1 passed, 0 failed; 0.10 s).

Large live storage events now emit the existing Resync signal when their
serialized size exceeds their collection's pull-response budget. Peers drain
the durable records through the existing byte-bounded masterChangesSince
contract; the live event cannot send an arbitrarily large document batch.

Verification includes:
- A real SQLite projection-writer test restoring a historical omitted source,
  preserving content/hash through the startup admission rule, and rejecting an
  oversized source without modifying its bytes.
- A native storage/replication test draining 21 large records after a Resync,
  preserving every content byte and the following small live update.
- The `module-source-lossless-browser-to-rust` mode of
  `src/core/rxdb/tools/browser_rust_smoke.js`: a real browser issues
  `ctox.source.load`; the native instance loads 21 Unicode source files;
  their complete content and SHA256 return over WebRTC. Their combined size
  exceeds 8 MiB. A native restart then checks all persisted source hashes.
  It reports command, complete transfer, and restart durations separately.

The E2E baseline failed on the unchanged optimized daemon immediately after
restart: source-00.js was truncated. The candidate completed the same flow with
21 records / 11,130,431 verified bytes, both before and after restart. Command
completion took 731.6 ms, full transfer/hash verification 2,701.3 ms, and native
restart 12,257 ms. Browser warnings, errors, request failures, cache repairs and
startup reloads were zero. These debug-binary fixture measurements are separate
from warm-command p50 and critical-collection boot p95; neither production
performance target is certified by this run.

Candidate binary SHA256:
`3a2f2241149b0fafc547a75c385f162511fb03b58ea2a2ab51cecdcb4742d54a`.
The full native RxDB suite passed 430 tests; the real-store source repair test,
five Office staging tests, cargo check and formatting passed. The complete JS
suite passed 119 tests with the freshly rebuilt wire daemon and no skips.
[Raw E2E measurements](beweise/raw/module-source-lossless-candidate-20260907.json).

The optimized candidate from main cc9a76e6e (SHA256
`e31b47bc968d651a05c341c2a5adbb3ea6629a688f8c0a0e2cf5b87b2e1eabcd`)
and signed shell beta17 completed 30/30 warm commands: p50 264.5 ms,
p95 337.05 ms, min 192 ms, max 406 ms, no reported issues. The debug control
on beta17 had p50 380 ms and did not meet the 300 ms target. The optimized
measurement passes the existing local warm-command target, not production WAN
or boot-p95 acceptance. It predates the startup-only legacy-source safeguard.
[Raw warm-command measurements](beweise/raw/warm-command-release-20260907.json).

Native production deployment has now completed. The fourteen historical source
repairs remain pending. The actual installed binary passed the source/restart
fixture and the local warm-command p50 target; general projection truncation,
a worker-schema conflict and slower complete source transfer remain open.
See [production postflight and architecture acceptance limits](native-sync-postflight-20260907.md).
The seven Office staging tombstones were repaired separately and are documented
in the incident report.
