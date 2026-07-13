# Security Policy

## Supported versions

CTOX is pre-1.0 software. Security fixes land on `main` and ship with the next
tagged release; they are not backported to older tags.

| Version | Supported |
| --- | --- |
| Latest tagged release (`v0.3.x`) | Yes |
| Older tags | No — upgrade to the latest release |
| Untagged `main` builds | Best effort — pin a tagged release for anything beyond development |

## Reporting a vulnerability

Please report vulnerabilities privately. Do not open a public issue for a
security finding.

- Preferred: [GitHub private vulnerability reporting](https://github.com/metric-space-ai/ctox/security/advisories/new)
  ("Report a vulnerability" on the repository's Security tab).
- If you cannot use GitHub, contact the maintainers through the
  [project page](https://metric-space-ai.github.io/ctox/).

Include the affected component (daemon, Business OS, installer, sync engine,
harness), a reproduction or proof of concept, and the impact you see. Reports
against the latest tagged release or current `main` are most actionable.

### What to expect

- Acknowledgement within 3 business days.
- Initial assessment (accepted / needs info / declined) within 7 days.
- Confirmed issues get a fix or documented mitigation; critical issues are
  prioritized for the next release.
- Coordinated disclosure: we ask for up to 90 days before public disclosure,
  and we credit reporters in the release notes unless you prefer otherwise.

## Security model in brief

CTOX is a self-hosted daemon that executes agent work with durable state in
SQLite and serves Business OS apps to browsers over WebRTC sync. The relevant
boundaries:

- **Autonomy gates** — agent effects are governed by autonomy levels
  (`progressive`, `balanced`, `defensive`); external or irreversible actions
  route through approval gates ([src/core/autonomy.rs](src/core/autonomy.rs)).
- **Business OS policy** — roles, scopes, and permission decisions are
  server-authoritative in the native policy engine
  ([src/core/business_os/policy.rs](src/core/business_os/policy.rs)); browser
  helpers mirror UX state only.
- **Data boundary** — Business OS data syncs exclusively over the WebRTC/RxDB
  path; HTTP serves static assets and explicit control-plane endpoints, never
  business data ([docs/ctox-rxdb.md](docs/ctox-rxdb.md)).
- **Secrets** — credentials live in the CTOX secret store and are redacted on
  persist paths; they are not passed through process environment toggles.
- **Release integrity** — release artifacts ship with SHA-256 checksums from
  the release workflow.

The machine-readable security/privacy control inventory and its sign-off
status live in
[docs/business-os-security-privacy-signoff.json](docs/business-os-security-privacy-signoff.json).
Until that sign-off is complete, treat production deployments with sensitive
data as unsupported — the same boundary the project applies to itself.

## Scope notes

- The installer's optional GPU/CUDA setup and communication adapters are
  opt-in surface; findings there are in scope.
- The forked execution harness under `src/core/harness/` is maintained in this
  repository; report harness findings here, not upstream.
