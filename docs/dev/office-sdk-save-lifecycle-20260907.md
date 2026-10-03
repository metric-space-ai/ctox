# Office SDK save lifecycle repair

The production adapter cleared the Spreadsheet SDK's modified flag immediately
after serializing, before the asynchronous native commit acknowledgement.
Serializer exceptions from the native toolbar/keyboard also escaped after the
runtime set `pendingSave.serialized`, leaving subsequent saves coalesced onto
an unresolved promise. Missing serializers or malformed payloads fell back to
the excluded upstream coauthoring-server path instead of failing the attempt.

Both editors now capture the pending save identity and route serialization,
decoding, SDK errors and RPC failures through one failure handler. It rejects
only the current attempt, emits its original error, retains dirty state and
releases the attempt for retry. A stale commit continuation cannot acknowledge
a replacement attempt. Only a successful native acknowledgement for the current
serialized revision clears SDK history; typing during a commit remains dirty.
Autosave, readiness and read-only gates prevent serialization.

Missing native serializers and invalid DOCY/XLSY payloads fail explicitly. This
intentionally does not support SDK builds that require the excluded coauthoring
server. Real payload compatibility continues to use the existing binary
signature checks and the unchanged XLSY offset decoder.

## Evidence and limits

- Native embedded Pi review completed successfully with one model request,
  no applied files, and bounded proposal-only access. Durable proposal:
  `ctox-dev/output/welsch-office-pi-proposal-1788747950661.json`.
- The save suite executes source-extracted production adapter, save lifecycle,
  acknowledgement cleanup and actual XLSY decoder code. The SDK serializer,
  event delivery and native commit are controlled doubles, not live services.
- Before repair: 35 tests, 5 passed, 22 failed, 8 timed out/cancelled. The first
  repaired run had one test-fixture error (a v10 header counted as ten rather
  than eleven bytes); corrected without changing production decoding.
- After repair: combined Office suite 121 passed, zero failed/cancelled/skipped.
  This includes 35 save tests, of which 32 are added here. Many older Office
  checks inspect existing evidence/source rather than executing user stories.
- `check:office` now includes the save suite, preserving all existing gates.
- This does not establish the cause of missing native command intake. The
  upstream `onSaveDocument` event still has no attempt ID, so arbitrary late
  uncorrelated SDK messages remain outside the captured-continuation guarantee.
- Main publication, rebuilt signed shell activation, actual edit/save/reopen,
  narrow-viewport editor layout, native CLI writeback and full production
  acceptance must be verified separately. No readiness claim follows here.
