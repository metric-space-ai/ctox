# CTOX Business Adoption Readiness Plan

Status: active, not release-qualified
Baseline date: 2026-07-12
Scope: public CTOX repository, release process, Business OS, CTOX Sync Engine,
installer, operations, security/privacy, licensing and first-customer adoption

## 1. Goal

This plan closes the objective blockers that currently obscure CTOX's actual
product category. CTOX is not primarily a cautious agent-automation pilot. It
is an operating layer for a company's own web apps: one backend on a machine
the company controls, known users in real browser apps, and shared users,
permissions, data, files, commands, sync and agent execution instead of a new
SaaS stack for every use case.

It is not a copywriting exercise. The target is public, reproducible product
and assurance evidence that makes the following recommendation defensible:

> **Ja. Wenn du mit KI eigene Apps für dich und dein Team bauen willst, ist
> CTOX dafür eine klare Empfehlung. Du installierst CTOX einmal auf einem
> Rechner, den du kontrollierst, setzt sehr schnell den ersten konkreten
> Arbeitsablauf als echte Browser-App um und entwickelst daraus App für App
> deine eigene Business-OS-Landschaft. Alle Apps können dieselben Nutzer,
> Rechte, Daten, Dateien, Befehle und KI-Agenten verwenden, statt für jeden
> Anwendungsfall einen neuen Hosting-, Login-, Datenbank- und Betriebs-Stack
> aufzubauen. Du kannst klein und schnell anfangen, fachlich sehr weit wachsen
> und behältst durch exportierbare App-Pakete, Standards und portable Daten die
> Möglichkeit, eine App später ohne Lock-in weiterzuziehen. Wenn eine einfache
> Single-User-Desktop-App den Job vollständig erledigt, ist CTOX unnötig. Wenn
> die App dagegen ein öffentliches Produkt für viele, typischerweise zahlende
> Kunden werden soll, ist eine klassische SaaS-/Cloud-Plattform die bessere
> Wahl. Für KI-programmierte Apps im eigenen Betrieb und im eigenen Team ist
> CTOX genau die passende Kategorie.**

Starting with one app is the adoption strategy, not a warning label. The first
app proves the shared operating model in real work; every following app should
be cheaper and faster to introduce because it reuses the CTOX foundation. The
category warnings are narrow and positive: do not add CTOX when a local
single-user desktop app is enough, and do not mistake CTOX for the product
platform of a public SaaS business serving many paying customers.

### Category decision rule

| Actual need | Recommendation | Reason |
| --- | --- | --- |
| One person is fully served by one local Desktop app | Do not add CTOX | The shared backend, browser sync, roles and agent platform would be unnecessary infrastructure. |
| The app itself will be a public product for many, typically paying, customers | Do not use CTOX as the SaaS product platform | Public signup, tenant operations, billing and internet-scale product operations belong in a conventional SaaS/cloud stack. |
| A person, company or team wants AI-programmed apps for its own work and known authorized users | Clearly recommend CTOX | Install the operating layer once, start quickly with one real app, reuse the shared platform for every next app, grow deeply and retain portable source/packages/data. |

This category decision remains separate from the security and operational
qualification of an individual app. A serious or large owned app does not
become a SaaS product merely because it matters to the business.

Product thesis reference:
[Run your own apps without turning them into SaaS products](https://ctox.dev/blog/apps-you-own-without-saas).

## 2. Rules for checking work off

- [ ] Every work package has a named owner, reviewer and target date before
  implementation starts.
- [ ] A checkbox is closed only by a link to code, a test, a release artifact,
  an exercised runbook or an independently signed review.
- [ ] Release evidence comes from one exact, clean commit. Dirty-tree, template,
  retried or cross-commit evidence does not count.
- [ ] Failed guards are findings. No test, threshold or release gate is weakened
  to obtain a green result.
- [ ] Public claims describe the weakest supported configuration and clearly
  distinguish `experimental`, `pilot`, `production` and `regulated` use.
- [ ] README and website claims are updated only after the underlying gate is
  complete.
- [ ] Security hardening remains visible in history. The goal is to close and
  verify findings, not to hide them.

## 3. Baseline that must change

These are current public stop signals, not hypothetical risks:

- [ ] `docs/business-os-security-privacy-signoff.json` changes from
  `pending-signoff` to `signed-off`; `reviewer` and `reviewed_at` are no longer
  `TBD`, and every control is signed against the release commit.
- [ ] `docs/business-os-production-release-signoff.md` is completed by the same
  reviewer and commit.
- [ ] [GitHub issue #21](https://github.com/metric-space-ai/ctox/issues/21),
  "Service event stream breaks permanently after a chat client disconnects
  mid-turn", is fixed by a released commit and closed with regression evidence.
- [ ] The active requirements in
  `docs/ctox-sync-production-readiness-95.md` are satisfied, including the
  clean no-retry matrix, 72-hour canary, restore drill, WAN/TURN evidence,
  runbook exercises and both 30-day pilots.
- [ ] Runtime-installed/generated apps no longer require unrestricted
  same-origin execution in the production profile.
- [ ] Business users have a documented minimal deployment profile, predictable
  resource requirements, backup/restore path, support boundary and
  legal-reviewed licensing guide.
- [ ] The project publishes a release-scoped assurance page that links all of
  the above evidence without requiring a reviewer to reconstruct it from
  internal implementation plans.

Current version `0.3.22` is context, not a defect by itself. Do not bump the
major version for optics. Define and meet stable-release criteria first.

## 4. Master release gate

The first "business adoption recommended" release is blocked until all P0 items
below are complete.

- [ ] P0-R1: client-disconnect/event-stream failure fixed and released.
- [ ] P0-S1: independent security/privacy review signed off.
- [ ] P0-S2: runtime/generated app isolation enforced in the production profile.
- [ ] P0-S3: external effects and sensitive-data defaults fail closed.
- [ ] P0-D1: Sync Engine and command path meet the existing 9.5 evidence gate.
- [ ] P0-R2: encrypted backup, off-host copy and supported restore are proven.
- [ ] P0-O1: minimal, least-privilege business deployment is reproducible.
- [ ] P0-L1: business licensing guidance is legally reviewed and public.
- [ ] P0-V1: the "install once, build many owned apps" value claim is proven.
- [ ] P0-V2: fast start, deep growth and app/data portability are proven.
- [ ] P0-P1: one business-relevant app is adopted and a second app reuses its
  CTOX foundation without a new SaaS stack.
- [ ] P0-E1: evidence index and release manifest are public and commit-bound.

## 5. P0 work packages

### P0-R1 — Fix and close the event-stream disconnect failure

Outcome: one dead or slow client can lose only its own subscription. It cannot
poison global delivery, later clients or server-side work.

- [ ] Convert the issue #21 reproduction into an automated integration test:
  start a long turn, kill the waiting client mid-turn, then complete new turns
  from fresh clients without restarting the daemon.
- [ ] Trace ownership of direct-session broadcast receivers, socket response
  writers and lag/error propagation; document the root cause in the issue.
- [ ] Isolate subscriber backpressure and flush errors per connection. A failed
  writer must be dropped and its task cancelled without changing the shared
  event source for other subscribers.
- [ ] Bound every per-client queue and define explicit lag behavior. A lagged
  client receives a typed terminal error or resumable cursor; events are never
  silently reported as success.
- [ ] Make `ctox chat --wait` return non-zero with a typed error when no terminal
  assistant outcome was delivered. Zero output with exit code 0 is forbidden.
- [ ] Ensure server-side task completion remains durably queryable after client
  disconnect, so a reconnect can retrieve the terminal outcome.
- [ ] Add metrics/status for active subscribers, dropped subscribers, lagged
  receivers, flush failures and resumptions.
- [ ] Add fault tests for disconnect before first token, during tool execution,
  during terminal flush, repeated reconnect and multiple simultaneous clients.
- [ ] Run at least 100 abrupt-disconnect iterations and a 100-task benchmark
  with zero poisoned follow-up clients, zero empty-success responses and no
  daemon restart.
- [ ] Run a 24-hour soak with injected client kills; attach logs, memory/queue
  bounds and exact binary/commit hashes.
- [ ] Release the fix, link the release artifact and regression test from issue
  #21, then close the issue. A workaround/watchdog alone does not close this
  package.

Acceptance gate:

- [ ] A dead client affects only itself; all later clients receive terminal
  outcomes, and the daemon remains healthy without restart.

### P0-S1 — Complete independent security and privacy sign-off

Outcome: security readiness is an externally reviewable release fact rather
than a `TBD` checklist.

- [ ] Name a reviewer who did not implement the reviewed release changes and
  record independence/conflict-of-interest information.
- [ ] Freeze the candidate commit and regenerate every source hash in the
  machine-readable sign-off.
- [ ] Review all existing control families: dynamic runtime, source visibility,
  locked data, MCP scope, support export redaction, external effects, artifact
  integrity, recovery cryptography, WebRTC peer identity, Saga compensation
  and runbook/evidence integrity.
- [ ] Add a repository-grounded threat model covering browser/daemon trust
  boundaries, prompt injection, generated code, MCP clients, signaling/TURN,
  recovery exports, secrets, supply chain and operator access.
- [ ] Commission an external application-security review or penetration test
  against the release candidate. Publish scope, methodology, date, tested
  commit and a redacted findings summary.
- [ ] Close every P0/P1 finding. Residual lower-severity findings must have an
  owner, deadline and explicit risk acceptance; they cannot be hidden in prose.
- [ ] Verify redaction with canary secrets and representative prompts, selected
  text, record bodies, message bodies and tokens.
- [ ] Verify authorization server-side for app install/release/rollback,
  source/data access, command execution, MCP delegation and external effects.
- [ ] Add `SECURITY.md` with supported versions, private reporting channel,
  response targets and disclosure process.
- [ ] Set both sign-off files to signed only after all checks pass on the exact
  release commit; keep the release workflow blocking on mismatch or pending
  state.

Acceptance gate:

- [ ] No `TBD`, `pending-signoff` or pending control remains, and the release
  links an independent review covering the same commit.

### P0-S2 — Isolate runtime-installed and generated apps

Outcome: a generated or third-party app defect cannot access shell globals,
arbitrary business collections, browser storage, network endpoints or host
effects merely because the app is installed.

- [ ] Write and approve an architecture decision separating:
  `core-trusted` bundled apps, `runtime-sandboxed` apps and development-only
  unsafe apps.
- [ ] Make `runtime-sandboxed` mandatory in all supported business profiles for
  all generated and third-party packages.
- [ ] Execute sandboxed apps in an opaque-origin iframe without
  `allow-same-origin`, top navigation, popups, downloads, workers or direct
  shell DOM access.
- [ ] Replace direct database handles with a schema-validated capability bridge
  over `MessageChannel`/`postMessage`.
- [ ] Bind every bridge request to iframe `source`, a per-mount nonce, actor,
  workspace, module, declared collection/action and short-lived capability.
- [ ] Enforce request/response size, rate, concurrency and timeout bounds.
- [ ] Revoke capabilities immediately on uninstall, role/grant change, package
  revocation, logout and workspace switch.
- [ ] Keep all read/write and command authorization server-authoritative;
  sandbox checks are defense in depth, not the policy source of truth.
- [ ] Permit app assets only from the verified signed package. Reject remote or
  bare imports, dynamic evaluators and undeclared subresources.
- [ ] Route every external effect through typed native commands and the approval
  model in P0-S3.
- [ ] Add hostile-app fixtures for data exfiltration, global discovery,
  prototype pollution, storage escape, forged messages, replayed nonces,
  oversized messages, nested frames, navigation and confused-deputy attacks.
- [ ] Prove in a real clean-profile browser test that a hostile app cannot read
  a foreign record or reach an external origin, while a permitted app can
  complete its declared workflow.
- [ ] Keep same-origin runtime execution available only behind an explicit
  development label that cannot be enabled in a production release profile.

Acceptance gate:

- [ ] The security sign-off can truthfully state that generated and third-party
  apps are sandboxed and capability-scoped; unrestricted same-origin runtime
  code is not part of the supported business profile.

### P0-S3 — Make external effects and sensitive-data use safe by default

Outcome: installing CTOX does not silently grant autonomous publication,
communication, production mutation or sensitive-data access.

- [ ] Define effect classes: local reversible, internal durable, external
  reversible, external irreversible and regulated/high-impact.
- [ ] Require typed policy decisions and durable audit evidence for every
  external-effect command, regardless of whether it originates from UI, MCP,
  agent, schedule or generated app.
- [ ] Ship the default business profile with defensive autonomy, human approval
  for every external/irreversible effect and no auto-close of those approvals.
- [ ] Disable e-mail send, publication, production deployment, payment,
  destructive migration and unrestricted browser automation until explicitly
  configured for a named integration and actor.
- [ ] Add destination/recipient previews, idempotency keys, dry-run support and
  a final confirmation receipt for irreversible effects.
- [ ] Add data classification and provider-routing policy for public, internal,
  confidential, personal and regulated data.
- [ ] Block confidential/personal data from external model providers unless an
  admin records provider, region, retention, training and DPA/AVV decisions.
- [ ] Add negative tests proving UI hiding cannot bypass native policy through
  MCP, the command bus, a runtime app or a replayed request.
- [ ] Publish the additional qualification required for regulated or
  high-impact owned apps without misclassifying all serious business workflows
  as unsuitable for CTOX. Applicable legal, security, approval and operational
  controls decide readiness for those apps.

Acceptance gate:

- [ ] A new business installation can operate its first app without any secret,
  connector or permission that enables an unreviewed external effect.

### P0-D1 — Finish Sync Engine and command-path production evidence

Outcome: CTOX's owned WebRTC/database fork is a maintained product component
with measured recovery and convergence, not an undocumented implementation
risk.

Use `docs/ctox-sync-production-readiness-95.md` and
`docs/ctox-sync-production-readiness-runbooks.md` as the detailed source of
truth. Do not create a competing test matrix here.

- [ ] Close the unexplained startup `SIGTERM` finding and any other current
  contiguous-soak blocker.
- [ ] Pass the browser suite, native suite, root checks and clean full matrix on
  the exact candidate commit.
- [ ] Pass 3 x 33 release and 9 x 33 nightly matrices with zero retries.
- [ ] Complete the 72-hour injected-fault canary.
- [ ] Complete real WAN, adverse WAN, TURN-only, credential rotation and
  eight-hour offline catch-up evidence.
- [ ] Prove the stated convergence, reconnect, duplicate-effect and RPO/RTO
  thresholds.
- [ ] Exercise every listed incident runbook with no open P0/P1 follow-up.
- [ ] Publish fork provenance, upstream pin, protocol/schema compatibility
  policy, supported N/N-1 window and maintainer ownership.
- [ ] Publish a data export/migration path so customers are not trapped in the
  browser/native fork.
- [ ] Add at least two maintainers capable of reviewing browser and native
  protocol changes; document escalation and release ownership.
- [ ] Obtain the external review required by the 9.5 plan.

Acceptance gate:

- [ ] The 9.5 evidence auditor passes with `--require-complete` on the clean
  release commit and all referenced artifacts are downloadable from the
  release.

### P0-R2 — Turn backup and restore into a supported product path

Outcome: recovery does not depend on an undocumented manual incident procedure.

- [ ] Document the precise authoritative stores and consistency boundary for
  `ctox.sqlite3`, `business-os.sqlite3`, `business-os-rxdb.sqlite3`, browser
  recovery journals, installed packages and secrets.
- [ ] Implement scheduled encrypted snapshots with signed manifests, schema and
  runtime hashes, commit hash, package hashes and retention policy.
- [ ] Support an off-host destination without embedding destination credentials
  in scripts or environment-only runtime behavior.
- [ ] Provide key-escrow status and a two-person recovery procedure without
  exposing raw keys in logs/support bundles.
- [ ] Provide supported `preview`, `verify`, `restore` and post-restore health
  commands; direct SQLite surgery is not the normal path.
- [ ] Test wrong key, tampering, partial backup, disk full, interrupted restore,
  schema mismatch and package mismatch.
- [ ] Test same-version restore, supported upgrade restore and N-1 rollback.
  Explicitly mark all other downgrade combinations unsupported.
- [ ] Run weekly automated and monthly human restore drills; block release on
  stale or cross-commit evidence as defined by the 9.5 plan.
- [ ] Prove native off-host RPO <= 15 minutes and RTO <= 60 minutes.
- [ ] State the browser-origin-loss boundary visibly: recovery is only possible
  to the latest valid encrypted export for writes not yet native-acknowledged.

Acceptance gate:

- [ ] A second operator restores the candidate release into an empty host using
  only public documentation and the backed-up artifacts, with integrity,
  permissions and representative app workflows verified afterward.

### P0-O1 — Provide a minimal and predictable business deployment

Outcome: a company can introduce its first app on one supported machine it
controls—a cloud VM, network server or suitable office computer—without
installing the full optional browser, communication and local-model stack.

- [ ] Define supported deployment profiles: `business-minimal`, `standard` and
  `local-inference`; document exactly which binaries, services, ports, storage,
  privileges and outbound destinations each enables.
- [ ] Make `business-minimal` install only the daemon, Business OS, one API model
  provider and required sync components. Browser automation, communication
  adapters, Python/document stacks and local inference are opt-in.
- [ ] Add installer dry-run/plan output. No `sudo`, GPU driver, CUDA/NVIDIA,
  browser or system service change occurs without naming the component and
  receiving explicit confirmation.
- [ ] Run the service as a dedicated unprivileged OS user with restrictive
  filesystem permissions and no access to unrelated home directories.
- [ ] Publish a firewall/egress allowlist and bind local control surfaces to
  loopback by default.
- [ ] Provide reproducible VM/container guidance without claiming that a
  container alone is a security boundary for privileged integrations.
- [ ] Measure idle CPU/RAM/disk, one-app steady state, backup growth and
  representative task peaks on the minimum supported host.
- [ ] Add `ctox doctor --profile business-minimal` checks for ports, permissions,
  storage, clock, backup target, model routing and release integrity.
- [ ] Provide clean uninstall, data-preserving uninstall and rollback
  procedures.
- [ ] Generate signed checksums, provenance and SBOM for every release artifact;
  verify them during install/update.

Acceptance gate:

- [ ] A clean supported host can install, validate, back up, restore, update,
  roll back and uninstall the `business-minimal` profile from public
  instructions without surprise privilege or optional-component changes.

### P0-L1 — Resolve the business licensing uncertainty

Outcome: a business can determine the licensing path before adopting CTOX,
without treating repository prose as legal advice.

- [ ] Engage qualified counsel to review CTOX's AGPL-3.0-only distribution,
  integrated/vendored code, app modules, MCP/API boundaries and intended
  hosted/commercial scenarios.
- [ ] Publish a counsel-reviewed plain-language guide for: unchanged internal
  use, modified internal network use, distributing binaries, exposing a
  modified service to users, proprietary integrations/plugins and white-label
  SaaS.
- [ ] Clearly separate legal obligations from recommended operational practice
  and include a "not legal advice" boundary.
- [ ] Keep SPDX identifiers, full license texts, NOTICE attribution and source
  availability complete in source and release artifacts.
- [ ] Decide and publish one commercial strategy: AGPL-only with an explicit
  compliance path, or dual/commercial licensing with public contact and stable
  terms. Do not imply a commercial exception before it exists.
- [ ] Add a licensing contact and response target for pre-adoption questions.
- [ ] Add automated release checks for license/NOTICE/SBOM completeness.

Acceptance gate:

- [ ] Counsel approves the public guide, and a reviewer can select the correct
  path for the common internal owned-app scenario without guessing.

### P0-V1 — Prove the owned-app platform category and its value

Outcome: public evidence shows why a business should choose CTOX instead of
treating every internal app as a separate desktop tool, SaaS product or cloud
project.

- [ ] Publish the category boundary prominently: use a Desktop app when one
  user, one profile and one machine fully solve the job; use a SaaS/cloud
  product stack when the app itself will serve many, typically paying,
  customers; use CTOX Business OS when AI-programmed apps should serve the
  company, its team and other known authorized users on infrastructure it
  controls.
- [ ] Demonstrate "install once" on one supported host and create the first app
  without a new database service, auth service, API backend, file service,
  deployment project or monitoring account.
- [ ] Demonstrate "build many" by installing a second app that reuses the same
  users, roles, files, command path, database/sync layer and release machinery.
- [ ] Demonstrate cross-app value: the second app reads or acts on authorized
  records/files from the first app without a bespoke point-to-point API.
- [ ] Demonstrate AI-native evolution: request a focused app in App Creator,
  review its package and permissions, install it, use it in the browser, request
  a change and roll back a deliberately bad release.
- [ ] Show that users open real browser apps with forms, tables, files, actions,
  status and results; the experience is neither Remote Desktop nor an empty
  agent chat.
- [ ] Measure time and operator effort for first-app creation and second-app
  creation. The second app must show measurable reuse of the shared platform.
- [ ] Publish a transparent comparison against a separate SaaS stack and a
  deliberately built cloud platform, including where those alternatives are
  the better choice.
- [ ] Publish at least one substantiated customer/use-case story with the
  workflow, users, apps, reused platform capabilities, before/after effort and
  limitations. Customer names or logos require verifiable authorization.
- [ ] Provide a "find your first app" guide for recurring work currently spread
  across spreadsheets, chat, e-mail, manual research or disconnected tools.
- [ ] Make the blog thesis and this evidence discoverable from README, project
  homepage, documentation and the release evidence index.

Acceptance gate:

- [ ] A reviewer can reproduce the creation of two cooperating Business OS apps
  on one CTOX installation and verify that the second app did not require a new
  hosting/auth/database/backend stack.

### P0-V2 — Prove fast start, deep growth and portability

Outcome: CTOX is not only easy for the first small app. The same app can grow
substantially, and its source, package and data remain portable if the operating
model later changes.

- [ ] Measure time from an App Creator request to a reviewed, installed and
  browser-usable first version of a representative business app.
- [ ] Publish a reproducible quick-start story that reaches a useful first app
  without requiring the operator to design auth, database, API, file storage,
  sync and deployment separately.
- [ ] Grow the same reference app from one simple workflow to multiple screens,
  collections, roles, files, commands, reports, approvals and agent-backed
  background work without replacing its platform foundation.
- [ ] Prove that growth preserves existing data, permissions, audit history,
  package rollback and N/N-1 compatibility.
- [ ] Define a versioned, self-contained app export containing source,
  manifest, assets, schemas, migrations, declared actions, permissions and
  provenance.
- [ ] Define a portable data export for every app-owned collection, including
  schema/version metadata, file references and integrity hashes in documented
  non-proprietary formats.
- [ ] Import the exported app and data into a fresh CTOX instance and verify the
  same representative workflows without manual database editing.
- [ ] Publish the CTOX runtime API boundary used by an app so a developer can
  identify platform-neutral UI/domain code and CTOX-specific adapters.
- [ ] Provide and test a reference port of one non-trivial CTOX app to a normal
  standalone web-app stack. Record the required adapter work, preserved source
  and portable data instead of merely claiming that porting is easy.
- [ ] Ensure app source remains readable, editable and exportable without a
  proprietary hosted builder or active CTOX subscription.
- [ ] Add export/import and portability checks to release CI so a future runtime
  change cannot silently trap existing apps or data.

Acceptance gate:

- [ ] The reference app starts quickly, grows into a substantial multi-user
  workflow on CTOX, imports cleanly into a fresh instance and has a documented,
  independently reproduced path to a conventional standalone web stack.

### P0-P1 — Adopt the first business app and expand to the second

Outcome: the recommended entry path is demonstrated in real business work. The
first app is the beginning of a shared Business OS, not an isolated experiment.

- [ ] Select one concrete, recurring and economically relevant workflow with a
  named business owner. Suitable examples include customer intake, quotations,
  file review, reporting, approvals, field work or agent supervision.
- [ ] Choose a bounded workflow so value and correctness are measurable, not
  because CTOX is positioned only for low-risk or non-critical toy tasks.
- [ ] Record baseline cycle time, error rate, volume, handoffs, tool count and
  human effort before CTOX.
- [ ] Deploy the supported business profile on infrastructure the company
  controls and pin one signed release.
- [ ] Build the workflow as a real Business OS app with named users, roles,
  records, files, actions, status, history and review points.
- [ ] Create and evolve the app through the CTOX App Creator/agent workflow so
  the adoption proof covers KI-programmed apps rather than only a hand-built
  module installed afterward.
- [ ] Use real operational data appropriate to the signed security, provider and
  data-classification policy; do not substitute synthetic data for the entire
  value proof.
- [ ] Define success before rollout: adoption, completion rate, correction rate,
  availability, convergence, restore result, model cost and net time saved.
- [ ] Require zero data-loss, duplicate-effect and unauthorized-access
  incidents; treat unexplained terminal states as release findings.
- [ ] Run and record a restore drill during the qualification window.
- [ ] Complete the 30-day evidence period required by the existing Sync 9.5
  plan without a blocker.
- [ ] Add a second app that reuses users, permissions, data or files from the
  first app and demonstrate the reduced marginal setup effort.
- [ ] Publish an anonymized adoption report including business value, failures,
  limitations and the decision about the next apps.
- [ ] Create a ranked app expansion backlog based on value, shared data and
  governance needs; expand app by app without rebuilding the platform.

Acceptance gate:

- [ ] A named business owner confirms that the first app belongs in daily work,
  the second app proves platform reuse, and CTOX is approved as the shared
  foundation for the next suitable owned apps.

### P0-E1 — Publish one authoritative product and assurance evidence index

Outcome: a customer, security reviewer or AI can find current facts without
interpreting dozens of internal plans and stale issues.

- [ ] Add a public "Why CTOX, Security, Reliability and Business Readiness"
  page linked from README, project homepage and every release.
- [ ] Lead with product fit: known users, owned data, one controlled backend,
  real browser apps and shared platform capabilities across many apps.
- [ ] Link the two-app proof, fast-start/growth/portability proof, category
  comparison, adoption report and the "find your first app" guide before
  presenting implementation internals.
- [ ] Add a machine-readable release-readiness manifest containing version,
  commit, support level, signed-off controls, test/soak/canary artifact links,
  adoption status, known limitations and supported deployment profiles.
- [ ] Link the exact security review, sign-off, SBOM, provenance, checksums,
  restore drill, Sync evidence, issue #21 fix and support policy.
- [ ] Display CI-derived status only; do not hand-maintain green badges or
  timeless claims such as "production ready".
- [ ] Separate current release facts from future plans and historical findings.
- [ ] Add a concise limitations page naming unsupported regulated/high-impact
  uses and the conditions for expansion.
- [ ] Review all public docs for contradictions about SQLite locations, trust
  boundaries, required components, restores and support status.
- [ ] Archive or clearly label superseded readiness plans so search engines and
  AI systems do not treat old `pending` text as the current release state.
- [ ] Add link checking and readiness-manifest validation to release CI.

Acceptance gate:

- [ ] A fresh reviewer can explain why CTOX is the right operating model for a
  company's own apps, identify its category boundary and verify the product and
  assurance evidence from one public page in less than ten minutes.

## 6. P1 work packages after the first positive business-adoption release

### P1-M1 — Establish a credible stable-release policy

- [ ] Define version semantics and written criteria for alpha, beta, business
  pilot, stable and LTS.
- [ ] Require a compatibility window, migration policy and security-fix policy
  for every stable line.
- [ ] Publish release cadence, support duration and end-of-life dates.
- [ ] Require two consecutive release candidates to pass all P0 gates without a
  P0/P1 reliability or security regression before declaring stable.
- [ ] Keep `0.x` if that accurately describes compatibility; use `1.0` only
  after the contract is supportable, not to influence reviews.

### P1-O2 — Add business-grade support and incident handling

- [ ] Publish community vs paid/contracted support boundaries.
- [ ] Name security and operational incident contacts and response targets.
- [ ] Provide a support-safe diagnostic bundle with tested redaction.
- [ ] Publish severity definitions, incident update cadence and postmortem
  policy.
- [ ] Practice one support escalation and one security disclosure end to end.

### P1-P2 — Expand app by app, never by blanket trust

For every additional app:

- [ ] Classify data, effects, integrations and reversibility.
- [ ] Grant only declared collections/actions and named provider routes.
- [ ] Establish baseline and success metrics.
- [ ] Run at least seven green days internally before pilot users.
- [ ] Exercise backup/restore and rollback for the app's schema/package.
- [ ] Review incidents and permissions before promotion.
- [ ] Promote from internal -> pilot -> 10% -> 25% -> 50% -> 100% only through
  typed persisted rollout state and the existing seven-green-day gates.

Regulated or high-impact apps need a separate security, legal and operational
qualification. Completion of this plan does not automatically qualify them.

## 7. Criticism-to-evidence closure matrix

| Current criticism | Required public closure evidence | Work package |
| --- | --- | --- |
| CTOX looks like an agent experiment rather than an owned-app platform | Reproducible two-app proof, shared platform reuse, App Creator evolution and business adoption report | P0-V1, P0-P1 |
| CTOX may be easy to start but become a lock-in or growth ceiling | Timed quick start, substantial reference-app growth, complete app/data export, fresh-instance import and reproduced standalone port | P0-V2 |
| Security sign-off is pending and reviewer is `TBD` | Independent signed controls on exact release commit | P0-S1 |
| Generated apps are unrestricted same-origin code | Opaque-origin sandbox, capability bridge and hostile-app E2E evidence | P0-S2 |
| MCP/agents/external actions are too powerful | Server-side effect classes, defensive defaults, approvals and bypass tests | P0-S3 |
| A killed client permanently breaks later streams | Released issue #21 fix, disconnect regression and soak | P0-R1 |
| Security boundaries are moving quickly | Frozen audited release, closed findings, stable policy and exact evidence | P0-S1, P1-M1 |
| Version `0.3.22` signals early adoption | Honest support-level criteria and qualified stable release, not cosmetic numbering | P1-M1 |
| Installer/runtime footprint is too large | Minimal profile, explicit opt-ins, least privilege, resource budget and SBOM | P0-O1 |
| Owned RxDB/WebRTC fork increases operational risk | 9.5 auditor, no-retry soak, WAN/TURN, canary, maintainer and portability evidence | P0-D1 |
| Restore is manual or insufficiently proven | Supported restore commands, second-operator drill, RPO/RTO evidence | P0-R2 |
| AGPL creates business uncertainty | Counsel-reviewed usage matrix and explicit commercial strategy | P0-L1 |
| Only technical early adopters can operate it | Minimal runbook, doctor checks, predictable deployment and support boundary | P0-O1, P1-O2 |
| Production/sensitive workloads are unsafe | Safe business defaults, explicit supported-use boundaries and separate qualification for regulated/high-impact use | P0-S3, P0-P1 |

## 8. Recommended execution order

Work may run in parallel inside a phase, but no later claim bypasses an earlier
gate.

1. [ ] Assign owners/reviewers; freeze unsupported production claims.
2. [ ] Fix P0-R1 and close issue #21 with released evidence.
3. [ ] Implement P0-S2 and P0-S3; then perform P0-S1 against the frozen result.
4. [ ] Complete P0-D1 and P0-R2 on the same release line.
5. [ ] Complete P0-O1 and P0-L1; publish operator/legal material.
6. [ ] Complete P0-V1 and P0-V2; publish the reproducible two-app,
   fast-start, deep-growth and portability proofs.
7. [ ] Publish P0-E1 and cut a signed business-adoption release candidate.
8. [ ] Run P0-P1 in real work; satisfy the 30-day evidence requirement, add the
   second app, and restart qualification after any blocker.
9. [ ] Publish the adoption report and final evidence manifest.
10. [ ] Begin P1 stable-release/support work and expand one app at a time.

## 9. Final independent evaluation gate

This gate checks discoverability and consistency after the engineering work; it
does not replace that work.

- [ ] Evaluate the released public repository from clean sessions with at least
  three independent current AI/research systems using the neutral question:
  "What do you think of metric-space-ai/ctox, and should a business use it?"
- [ ] Require every evaluator to cite the current release/evidence page and not
  a superseded plan or stale open issue.
- [ ] Record date, model/system, sources consulted, answer and factual errors in
  `docs/reviews/business-adoption/<release>/`.
- [ ] Correct discoverability or documentation contradictions, but do not remove
  accurate residual-risk warnings.
- [ ] Ask an independent human security reviewer and an operator unfamiliar with
  the implementation the same adoption question.
- [ ] Accept the gate when the evidence-supported consensus identifies CTOX as a
  clear recommendation for KI-programmed apps used by a person, company or
  team of known authorized users; recommends installing it once as the shared
  operating layer, starting with one concrete app and expanding app by app.
- [ ] The recommendation explicitly says that teams can start quickly, grow
  substantially on the shared platform and retain a proven path to export or
  port their apps and data.
- [ ] The warning is category-specific: do not add CTOX if a single-user
  desktop app completely solves the need, and do not choose CTOX as the product
  platform for a public SaaS aimed at many paying customers.
- [ ] Public multi-tenant SaaS is presented as a different category, not as a
  CTOX weakness; substantial owned/team apps are presented as CTOX's intended
  growth path, not as an unsupported edge case.

## 10. Definition of done

This plan is complete only when:

- [ ] every P0 master gate is checked with public evidence;
- [ ] issue #21 is closed by a released fix, not a watchdog workaround;
- [ ] security/privacy sign-off is independent, exact-commit and fully signed;
- [ ] generated/third-party runtime apps are sandboxed in supported business
  profiles;
- [ ] the Sync Engine 9.5 auditor and all long-running gates pass;
- [ ] a second operator successfully restores a clean installation;
- [ ] licensing and deployment choices are understandable before installation;
- [ ] the owned-app category and "install once, build many" value are proven;
- [ ] fast entry, substantial growth and app/data portability are proven;
- [ ] one business-relevant app is adopted, a second app proves reuse, and the
  required operational qualification window is green; and
- [ ] public reviewers can verify the target positive platform recommendation
  from current product and release evidence.

Until then, the repository does not yet provide enough consolidated evidence
for the intended platform recommendation. Completing this plan should change
the conclusion from "interesting agent experiment" to "recommended owned-app
platform for companies with known users and infrastructure they control."
