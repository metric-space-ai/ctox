# nachweise – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/` `store_ats_commands.rs`, `ats_gates.rs`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 3 von 3 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 0 PLATTFORM (nur dokumentiert). Tests: 6 grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ats.deployment.check` | FACHLOGIK | `store_ats_commands.rs:54-88` | `handlers/ats.deployment.check.js` | portiert |
| 2 | `ats.leistungsnachweis.signoff` | FACHLOGIK | `:709-870` | `handlers/ats.leistungsnachweis.signoff.js` | portiert |
| 3 | `ats.credential.verify` | FACHLOGIK (ats gemeinsam) | `:1004-1054` | `handlers/ats.credential.verify.js` | portiert (kein UI-Sender) |

Zuordnung `ats.credential.verify` zu nachweise, weil nachweise Eigentümer von `business_credentials` ist.

**Manifest-Wünsche:** `planning_time_records` (shiftflow) `['read','write']` (heute nur `read`; signoff schreibt
Abzeichnung/`billing_released` zurück), `signature_requests` (esign) `['read']`, `accounting_invoices` (invoices)
`['read','write']` (Rechnungsentwurf `inv_nachweis_<id>`). `payload.collection` darf jede ATS-Collection sein (Rust-
Allowlist); in v4 greift zusätzlich das Manifest. Wie Rust: kein Prüfspur-Ereignis beim signoff.

## Gemeinsame ATS-Grundlage

`handlers/_ats.js` ist in consent, esign, intake, interviews, nachweise, placements, submissions **byte-gleich** kopiert
(App-Artefakte importieren nicht über App-Grenzen, §16.15). Inhalt: 1:1-Port von `ats_gates.rs` (credential_status,
evaluate_deployment_readiness, consent_valid, evaluate_consent_gate, retention_due, find_double_submission,
signature_request_status, compute_nachweis_billing, evaluate_billing_gate) und die Helfer aus `store_ats_commands.rs:1-385`
(ATS_WRITABLE_COLLECTIONS, ats_actor_value, record_ats_governance_event, ats_invoice_line, ats_canonical_invoice,
upsert_business_record, first_string_field, checked_scaled). Gate-Tests: `consent/handlers/tests/ats_gates.test.mjs`.
Änderung an einer Kopie ⇒ alle sieben angleichen (Prüfung: `shasum modules/*/handlers/_ats.js`).

**Abweichungen vom Rust-Verhalten (bewusst, begründet):**
1. *Fehlercodes*: Rust lieferte für jeden Fehler einen generischen `anyhow`-Fehlschlag. v4: Pflichtfeld → `invalid`,
   unbekannter Datensatz → `not_found`, Fremd-Collection außerhalb der Allowlist bzw. anonymer Handelnder → `forbidden`.
   Meldungstexte sind wörtlich übernommen.
2. *Handelnder*: `ats_actor_value(session)` wird aus `command.actor {id, role}` gebildet (`display_name` nur, wenn der Kern
   es mitliefert). „Authentifiziert“ = `actor.id` nicht leer und ≠ `rxdb-command`. Die Rollenprüfung chef/admin
   (heute `is_ats_mutating_command` → `rxdb_command_session`) bleibt **Kern-Policy** und ist nicht Teil des Handlers.
3. *Atomarität*: Mehrere Schreibvorgänge eines Befehls laufen in **einer** `ctx.transaction` (Rust: einzelne Upserts
   ohne Transaktion). Das Prüfspur-Ereignis ist externe Wirkung und läuft **nach** der Transaktion.
4. *Löschen (DSGVO)*: Rust überschrieb den Payload vor `deleted=1` mit einem PII-freien Grabstein (`redacted`,
   `redacted_at_ms`, `erasure_basis`/`retention_days`). v4 nutzt `ctx.remove`. **Laufzeitanforderung an „ctox sync v4“:**
   der Lösch-Grabstein des Sync-Stores darf keinen Payload (PII) behalten; die Metadaten stehen im Prüfspur-Ereignis.
5. *IDs*: `appl_/plac_/subm_/sig_/transcript_{now}` wie Rust (Kollision bei gleicher Millisekunde unverändert).
6. *Konfiguration*: `env_or_config(root, KEY)` (SQLite-Laufzeitstore) → Capability-Wunsch `ctx.cap.config.get`.

**Capabilities (alle ATS-Apps):**

| Capability | Signatur (Vorschlag) | Beispielaufruf | Herkunft |
|---|---|---|---|
| `ctx.cap.audit.event` | `audit.event({collection, record_id, event_type, payload, observed_at_ms}) → void` | `await ctx.cap.audit.event({collection:'business_commands', record_id:'', event_type:'business_os.ats.candidate_presented', payload, observed_at_ms: now})` | Wunsch aus Strang H3 (outbound), ersetzt `insert_business_event` → `business_events` |
| `ctx.cap.config.get` | `config.get({key}) → {value: string|null}` | `await ctx.cap.config.get({key:'CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS'})` | **neu** (nicht in E §3); ersetzt `runtime_env::env_or_config`, Schlüssel je App im Manifest freizugeben |

## Rechte (Collection-Lesen, Manifest `collections.<name>.rights`)

Strang SHARED (R4, 15.10.2026): bisher in `shared/permissions.js` `ADMIN_ONLY_COLLECTIONS` (Spiegel von policy.rs
`ADMIN_ONLY_COLLECTIONS`); jetzt App-Daten im Manifest, übertragen über `DOC_COLLECTION_RIGHTS` in
`v4/tools/manifest/gen-manifest.mjs`.

| Collection | `rights` | Basis |
|---|---|---|
| `business_credentials` | `{read:{roles:['admin','chef']}}` | policy.rs `ADMIN_ONLY_COLLECTIONS` |
