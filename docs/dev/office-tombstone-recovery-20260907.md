# Office deletion-projection regression — 2026-09-07

## Live evidence

Welsch Shell 0.1.46-beta.16 is active. The existing Office acceptance spreadsheet
opens with its stored cells, but a native-toolbar save of C1=23 failed at
2026-09-07T00:52:34.481Z. The browser emitted `push_confirmed` for
`cmd_office_55588961-59e9-4335-95d4-115bba2f96f3`; the native read-only command
inspector did not find that ID. The draft remains unsaved. This is an unresolved
command-intake finding, not proof that the deletion bug below causes the save failure.

The native journal independently reports repeated `document_blob_chunks`
projection failures with status 422: `field 'data' has wrong type (expected
Single("string"))`. The affected legacy row is already a tombstone. Its `data`
field contains an `_omitted` object left by older wire-budget clamping instead
of the required base64 string. Both previous and attempted next records are
deleted. An invalid required value survives the old tombstone helper because
that helper only fills missing/null fields.

## Repair scope

The deletion-projection helper now also replaces values whose JSON type does
not match the required field's declared schema type. It uses the existing
tombstone defaults, retains valid values, and keeps the row deleted. This does
not restore omitted file bytes, produce a blank live document, alter access
policy, or directly edit/delete production database rows. The ordinary live
document projection path is unchanged.

The helper now accepts `RxJsonSchema` directly, since it does not need the
database-facing `RxSchema` wrapper. Its two callers pass the same inner schema.
This allows the actual helper definitions to be compiled with the actual schema
types in a small isolated Rust unit test, without rebuilding the whole daemon.

## Validation and remaining gates

`rxdb_peer_projection_tombstone_tests.rs` is registered in the native test suite.
It covers the observed omission marker, retention of valid bytes/metadata,
required-type repair/idempotency, and schema-light deletion records.

The small runner `src/core/business_os/tools/test_projection_tombstones.mjs`
extracts the actual helper definitions and includes the actual schema types and
test file. It uses pinned serde dependencies, offline resolution, and disposable
output only under `/Volumes/tmp` via `TMPDIR`. It does **not** stand in for a
full CTOX/RxDB integration test or a deployed acceptance test.

Before the behavioral fix: **4 passed / 2 failed**, including the two existing
schema-type tests. Both new failures reproduce invalid required tombstone values.
After the fix: **6 passed / 0 failed / 0 ignored**. Actual extracted helper
SHA256: `c35927c42ba00a7adc60c6f2bd1c89736416a1663ac8d42ecfa6d54d0ff129e6`.
The focused rustfmt check also passes. The full native CI test is added as an
explicit gate. Full native checks, publication, deployment, and live
verification remain pending at this checkpoint. No production-ready claim.
