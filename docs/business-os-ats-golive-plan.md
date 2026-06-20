# Business OS — ATS Go-Live Plan

Status: working go-live runbook · Scope: the steps to take the ATS that now lives
on CTOX Business OS (`docs/business-os-ats-plan.md` §1–§9) from "code-complete and
in-env verified" to "live for a Personalvermittler tenant".

This plan is split by **who can execute each step**, because much of go-live is
operations / configuration / coordination / legal — not code. Coding agents can
only execute the 🤖 items; the rest are documented with exact commands so an
operator can run them.

| Tag | Who | Meaning |
|-----|-----|---------|
| 🤖 **auto** | this session / a workflow | buildable + verifiable in-repo now |
| 🛠️ **operator** | a human on the live instance | a runtime/config/click-through step against the running daemon |
| 🤝 **coord** | with the parallel sync-layer owner | touches the guard-protected RxDB/WebRTC layer |
| ⚖️ **legal/org** | the business | permission, compliance, sign-off — outside code |

---

## G0. Vorbedingung — was bereits steht (verifiziert, auf `main`)

Kein offener ATS-Code. Durchgängiger Fachprozess nativ + alle 7 Modul-UIs:

- §5 komplett: parse → intake → consent/DSGVO → LLM-matching → submission
  (auditiert) → placement (+AÜG-Gate, auditiert) → billing (Honorar/Clawback/
  Leistungsnachweis + Rechnung/Storno/§17) → e-sign → retention → transcription.
- Sicherheits-Fundament: Capability-Token (nativ HMAC-signiert), Pfad
  issue → attach → verify ist live (`POST /api/business-os/auth/capability` +
  `command-bus.js getCapabilityToken()`).
- 7 Modul-UIs engine-grounded; adversarialer Review hat 2 echte Bugs gefixt.
- Grün: rxdb-Guards 37/0, ATS-Cores 74/0, ats_gates 8/8, invoices 18/18,
  capability 5/5, Schema-Parität ok.

**Acceptance:** nichts zu tun — Referenzzustand.

---

## G1. End-to-End-Beweis (deterministisch, isoliert) — 🤖

Ein wiederholbarer Smoke-Test, der einen **frischen, isolierten** Runtime-Store
seedet und die **gesamte** ATS-Befehlskette über `ctox business-os commands
dispatch` fährt, jedes Gate (positiv + negativ) prüft und PASS/FAIL meldet. Das
ist der bestmögliche „läuft wirklich"-Beweis ohne laufende Browser-Shell.

- [ ] `tests/business-os/ats_golive_smoke.sh` — baut das Binary aus dem
  Main-Checkout, legt einen Temp-Root an, seedet chef + Beispiel-Records, fährt:
  `ats.intake.capture` → `ats.consent.check` → `ats.submission.present`
  (positiv + doppelte-Vorstellung-Block) → `ats.placement.create` (AÜG-Gate
  block ohne Nachweis, dann ok mit Nachweis) → `ats.deployment.check` →
  `ats.leistungsnachweis.signoff` (block ohne charge_rate, ok mit) →
  `ats.signature.request`/`sign` → `ats.subject.export` (Art.15) →
  `ats.subject.erase` (Art.17) → `ats.retention.due`. Asserts je Schritt.
- **Acceptance:** `bash tests/business-os/ats_golive_smoke.sh` endet mit
  `ALL STEPS PASS` und nicht-leeren, semantisch korrekten Outcomes.

---

## G2. Tenant-Konfig-Bundle (Baukasten config layer) — 🤖 Scaffold / 🛠️ Werte

Die „ATS-Einrichtung" ist **Konfiguration eines generischen Baukastens**, kein
Code. Dieses Bundle liefert das Gerüst + dokumentierte Default-Werte; die
endgültigen geschäftlichen Werte setzt der Operator.

- [ ] `docs/ats-golive/tenant-config.md` — alle relevanten Runtime-Flags mit
  empfohlenem Wert, Wirkung und exaktem Setz-Befehl (siehe G3-Tabelle).
- [ ] `tests/business-os/ats_golive_seed.sql` — operator-editierbares
  Seed-Template (chef/admin-User, Beispiel-Stammdaten, AÜG-Pflichtnachweis-Typen).
- [ ] Konfig-Schicht-Hinweise: matching-Definition (`candidate_job.v1` als
  Default), Pipeline-Stage-Labels, Dokument-Templates (Angebot / AÜG-Vertrag /
  Leistungsnachweis), Locales `de.json` — alle als Config/Daten, nicht Code.
- **Acceptance:** ein Operator kann mit dem Bundle eine frische Instanz für den
  Personalvermittler-Tenant einrichten, ohne Modul-Code anzufassen.

---

## G3. Sicherheits-Enforcement scharf schalten — 🛠️ operator

Der Capability-Token-Pfad ist live (verify-if-present). Vor Multi-User-Produktion
muss `reject-if-absent` aktiviert werden — sonst ist Actor-Spoofing möglich.

**Vorbedingung:** bestätigt, dass jeder eingeloggte Client ein Token holt +
sendet (Browser ruft `POST /api/business-os/auth/capability` und hängt es als
`client_context.capability_token` an — bereits implementiert).

Flags (alle über den **Runtime-Store**, NICHT Prozess-Env — `env_or_config`
liest `runtime_env_kv` in `runtime/ctox-runtime.sqlite3`):

| Flag | Wert | Wirkung |
|------|------|---------|
| `CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN` | `1` | privilegierte Befehle ohne gültiges Token → abgelehnt |
| `CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS` | tenant-spezifisch | AÜG-Pflichtnachweise je Placement-Typ (fail-closed) |
| `CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE` | `1` (gehärtet) | Leistungsnachweis-Signoff braucht abgeschlossene Signatur |
| `CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE` | `1` (gehärtet) | Nicht-Consent-Rechtsgrundlagen brauchen `basis_evidence` |

- [ ] Setzen (Beispiel):
  `sqlite3 runtime/ctox-runtime.sqlite3 "INSERT OR REPLACE INTO runtime_env_kv(key,value) VALUES('CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN','1')"`
- [ ] Smoke gegen die laufende Instanz erneut fahren (privilegierter Befehl mit
  Token → ok; ohne Token → abgelehnt).
- **Acceptance:** ohne Token kein `manage-all`; mit gültigem chef-Token ok.

---

## G4. Live-Abnahme der 7 UIs — 🛠️ operator

Die Modul-UIs sind per `node --check` + Handler-Feld-Grounding + adversarialem
Review verifiziert, aber das **Mount-Verhalten** (RxDB-Handles, WebRTC-Dispatch-
Round-Trip) braucht die laufende Shell und ist hier nicht prüfbar.

- [ ] Jedes Modul (intake, submissions, interviews, esign, nachweise, consent,
  placements) gegen einen laufenden Peer durchklicken: Formular abschicken →
  korrekter `ats.*`-Befehl dispatcht → Ergebnis/Blocker rendert → Liste
  aktualisiert (Subscription).
- **Acceptance:** ein Recruiter führt jeden Schritt aus der UI aus, ohne
  Konsole/CLI.

---

## G5. Command-gated Audit für direkte RxDB-PII-Writes — 🤝 coord

Der Befehlspfad ist auditiert; **direkte** (nicht-Befehl) Replikations-Writes auf
PII-Collections sind die verbleibende DSGVO-Belegbarkeits-Lücke. Liegt im
guard-geschützten Sync-Layer (Domäne des parallelen Codex-Agenten) — **nicht**
unilateral ändern.

- [ ] Mit dem Sync-Layer-Owner abstimmen: jeder Write auf eine ATS-PII-Collection
  über die Replikations-Accept-Pfad muss ein Audit-Event erzeugen (analog
  `record_ats_governance_event`).
- **Acceptance:** kein PII-Write umgeht die Audit-Spur.

---

## G6. Infra & Recht — 🛠️ operator / ⚖️ legal

- [ ] 🛠️ **STT-Gewichte** (Q4-GGUF/Voxtral) installieren — *nur falls*
  Interview-Transkription live sein soll. Graph ist verdrahtet, nur Gewichte
  fehlen (`runtime stt-doctor` prüft).
- [ ] 🛠️ **Live-LLM-Matching-Turn** über das Modell-Gateway einmal verifizieren
  (Skill `business-os-requirement-matching` bindet; Gateway läuft).
- [ ] 🛠️ **WebRTC/Signaling**: `CTOX_BUSINESS_OS_SIGNALING_URLS`,
  `CTOX_BUSINESS_OS_ICE_SERVERS`, Login (`CTOX_BUSINESS_OS_REQUIRE_LOGIN`,
  `CTOX_BUSINESS_OS_LOGIN_URL`) für den Tenant konfigurieren.
- [ ] ⚖️ **AÜG-Erlaubnis** (Arbeitnehmerüberlassung) der Agentur vorhanden.
- [ ] ⚖️ **DSGVO**: Verarbeitungsverzeichnis, AVV, Lösch-/Aufbewahrungskonzept
  (Retention-Engine + Art.15/17 vorhanden — dokumentieren).
- [ ] ⚖️ **Security-/Last-Review** der Organisation.

---

## G7. Definition von „live-ready"

Live für Multi-User-Produktion = **G0** steht, **G1** grün, **G2** für den Tenant
gesetzt, **G3** scharf, **G4** abgenommen, **G6**-Recht erfüllt. **G5** ist ein
koordinierter Plattform-Schritt (DSGVO-Härtung), kein ATS-Blocker für einen
Einzel-/Pilot-Tenant. Bis **G3** scharf ist, ist die Instanz **nicht**
Multi-User-sicher.

---

## G8. Was dieser Workflow-Pass erledigt vs. delegiert

🤖 in diesem Pass (Workflow): **G1** (End-to-End-Smoke, gebaut + ausgeführt +
adversarial verifiziert) und **G2** (Konfig-Bundle-Scaffold + Seed-Template).

Dokumentiert, aber **nicht** auto-ausführbar (Grund je Zeile oben): G3
(Enforcement-Flip — Betriebsentscheidung gegen die laufende Instanz), G4
(Browser-Abnahme — braucht laufende Shell), G5 (Sync-Layer — Koordination), G6
(Infra/Recht). Die laufende Instanz gehört dem parallelen Agenten und wird nicht
angefasst.
