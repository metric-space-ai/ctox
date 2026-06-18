# Business OS Production Release Signoff

Schema: ctox.business_os.production_signoff.v1
Status: pending-signoff
Signoff owner: TBD
Signoff date: TBD
Evidence revision: f2cdd6a8d276d3d70f0930ed8d73c4f641ecc9c6

This file is the human-readable release checklist for Business OS roles,
permissions, dynamic apps and agent scopes. The machine-readable blocking
release artifact is `docs/business-os-security-privacy-signoff.json`.

The release workflow must fail while
`docs/business-os-security-privacy-signoff.json` is `pending-signoff`. Change
both files to signed-off only after every required checklist item below is
checked and the linked evidence matches the release commit.

## Required Checklist

- [ ] dynamic-app-runtime - same-origin runtime app boundary, forbidden
  network/import/storage/global/evaluator/worker bypasses and generated-app
  external-effect limits reviewed against current code and tests.
- [ ] app-source-visibility - source view, source snapshot and source save
  paths reviewed for Owner/Admin, App-Verantwortliche:r, exact grants and
  Teammitglied denial.
- [ ] data-review-locked-state - App Store publish review, evidence-only data
  review, explicit data grants, locked data areas and rollback state reviewed
  against native catalog projection.
- [ ] mcp-agent-scope - MCP app visibility, data access, submitted
  `client_context`, Business Chat, App Store context chat and Coding Agents
  visible-scope surfaces reviewed for spoofing and over-sharing.
- [ ] audit-support-export - Settings Activity, Why diagnostics and
  Support-Paket export reviewed for redaction of prompts, selected text,
  record bodies, message bodies, tokens and secrets.
- [ ] external-effects - MCP external-effect boundary and generated-app
  command-bus limits reviewed; no external-effect path is enabled without a
  separate approval model.
- [ ] artifact-integrity - production Browser/Rust smoke artifact, release
  workflow gate, dependency bootstrap, uploaded evidence and release artifact
  dependency chain reviewed.

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
