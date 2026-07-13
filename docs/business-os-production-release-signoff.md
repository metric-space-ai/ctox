# Business OS Production Release Signoff

Schema: ctox.business_os.production_signoff.v1
Status: pending-signoff
Signoff owner: TBD
Signoff date: TBD
Evidence revision: cab3b57859058fb9721cdb4fb5385b6b7e4f0463

This file is the human-readable release checklist for Business OS roles,
permissions, dynamic apps and agent scopes. The machine-readable blocking
release artifact is `docs/business-os-security-privacy-signoff.json`.

The release workflow must fail while
`docs/business-os-security-privacy-signoff.json` is `pending-signoff`. Change
both files to signed-off only after every required checklist item below is
checked and the linked evidence matches the release commit.

## Required Checklist

- [ ] `dynamic_app_runtime_boundary` - same-origin runtime app boundary, forbidden
  network/import/storage/global/evaluator/worker bypasses and generated-app
  external-effect limits reviewed against current code and tests.
- [ ] `source_visibility` - source view, source snapshot and source save
  paths reviewed for Owner/Admin, App-Verantwortliche:r, exact grants and
  Teammitglied denial.
- [ ] `data_review_locked_state` - App Store publish review, evidence-only data
  review, explicit data grants, locked data areas and rollback state reviewed
  against native catalog projection.
- [ ] `mcp_agent_scope` - MCP app visibility, data access, submitted
  `client_context`, Business Chat, App Store context chat and Coding Agents
  visible-scope surfaces reviewed for spoofing and over-sharing.
- [ ] `audit_support_redaction` - Settings Activity, Why diagnostics and
  Support-Paket export reviewed for redaction of prompts, selected text,
  record bodies, message bodies, tokens and secrets.
- [ ] `external_effect_boundary` - MCP external-effect boundary and generated-app
  command-bus limits reviewed; no external-effect path is enabled without a
  separate approval model.
- [ ] `release_artifact_integrity` (`artifact-integrity`) - production Browser/Rust smoke artifact, release
  workflow gate, dependency bootstrap, uploaded evidence and release artifact
  dependency chain reviewed.
- [ ] `sync_recovery_crypto_boundary` - recovery journals, encrypted off-host
  backups, escrow separation, key revocation and restore evidence reviewed;
  raw recovery keys are never included in the signoff evidence.
- [ ] `webrtc_peer_identity_transport` - signaling/TURN credentials, peer
  identity, workspace isolation, impersonation/replay resistance and the
  no-HTTP business-data boundary reviewed.
- [ ] `saga_idempotency_compensation` - command admission, immutable action
  snapshots, crash replay, idempotency, compensation and durable manual
  intervention evidence reviewed.
- [ ] `production_evidence_runbook_integrity` - zero-retry production gates,
  evidence hashes, failure-injection runbooks and the prohibition on silently
  discarding journal, conflict or Saga state reviewed.

## Evidence To Review

- `runtime/build/business-os-smoke-matrix-summary.json` from the release gate.
- `runtime/build/business-os-production-smoke-key-escrow-zip64.json` from the
  current local production Browser/Rust matrix.
- `runtime/backup/business-os-drill-1781807557440-bec00472-78b8-4a62-ac02-737398289542/manifest.json`
  from the current local real-root restore drill.
- `ctox business-os backup key-escrow-status` output showing
  `ready_for_external_escrow_confirmation` without revealing the raw key.
- `docs/business-os-roles-permissions-plan.md` Phase 16 ledger.
- `docs/business-os-dynamic-apps-permissions-concept.md` current status.
- `docs/business-os-roles-permissions-operator-guide.md`.
- `docs/business-os-roles-permissions-rollout.md`.
- `.github/workflows/release.yml`.
- `src/core/rxdb/tools/business_os_production_smoke_registry.js`.
- `src/core/rxdb/tools/browser_rust_smoke_matrix.js`.
- `src/apps/business-os/scripts/assert-production-release-docs.mjs`.

## Signoff Notes

Record release-specific notes here before changing the status to
`signed-off`. Notes must name residual risks explicitly. Do not use this file
to waive failed CI, failed Browser/Rust smoke evidence, missing legacy
migration fixtures, missing customer/operator documentation or missing
security/privacy review.

- TBD
- 2026-06-18: Automated evidence hashes are populated in
  `docs/business-os-security-privacy-signoff.json`; checklist remains pending
  until explicit reviewer signoff.
- 2026-07-13: Automated source hashes were refreshed against the current
  release candidate after the Readiness/Soak workflow hardening. Human review
  and signoff remain pending.
