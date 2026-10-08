# Native Jour fixe narration

A current, leased, registered project Supervisor uses the existing
`business_os.jour_fixe_update` tool with `action: narrate` and a generated
`NarrateRequest`. The request supplies operation, meeting and slide IDs, deck and
meeting revisions, and the SHA256 of the stored slide body. The native meeting read supplies eligible slide inputs and exact hashes, so the model does not have to calculate or guess a cryptographic hash. Owner, model, voice,
text, audio, credentials and provider attestations cannot be supplied by the
caller. The authenticated project/lease boundary remains the same as the existing
JourFix read, draft-deck and todo-proposal tools.

The native handler authorizes and reserves a single operation for that slide and
deck. It drops both Core and Policy writer transactions before invoking the
configured `SpeechGateway::synthesize_verified_async` on the existing MCP runtime,
with a 90-second timeout, WAV output and no fallback or voice override. A slide
body is bounded to 4096 UTF-8 bytes; output is bounded to 8 MiB and five minutes.
The producer's branded result binds actual text and audio hashes, run and model.
The physical WAV frames determine duration.

Before committing, the native handler revalidates the current leased Supervisor,
Owner, project, file policies, deck, slide text and meeting revision. An immutable
native `desktop_files` record and base64 chunks, the slide `AudioRef`, meeting
revision and stable operation receipt commit together in the Policy database.
Only a deck whose every slide has audio enters `ready`. The Core transaction
fences lease changes; this is not a claim of a cross-database atomic commit.
The existing RxDB projection transports file bytes; no HTTP business-data bridge
is introduced. A projection failure replays the committed custody receipt without
another speech request.

Known pre-provider configuration/credential/voice failures may retry the same
intent at most three times. A timeout, transport error, uncertain producer result,
invalid output or authority change after synthesis never automatically repeats
synthesis. A different operation ID cannot bypass the one-operation-per-slide
reservation. Replacing a deck is an explicit new revision; an old operation cannot
publish into that revision. This prevents duplicate charges and stale audio.

T-2h preparation remains the existing per-project calendar schedule and durable
registered Supervisor turn. The embedded JourFix skill first reads the bounded
KPI evidence, drafts slides, narrates each saved slide sequentially using current
revisions, and reads the committed meeting before reporting readiness. Owner start,
close and three-revision todo confirmation retain their existing native policy.
The configured gateway and voice must be installed and accepted before the real
Monday 12 October, 11:00 Europe/Berlin preparation run. A local Owner-uploaded WAV
is a separate provenance and cannot stand in for provider narration.

The producer seam is test-only. Regression fixtures prove custody, revisions,
policy/lease revocation during an unlocked synthesis, stable replay, bounded retry,
uncertain-result suppression and rollback. They do not prove installed voice
configuration, playable production speech, provider latency or the real calendar
run. Those checks run after merge on the installed stack through the existing
writer and Models owner.
